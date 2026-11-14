//! The source rewrite before an expression runs: arithmetic becomes calls
//! to the library's array-aware helpers, and legacy-engine sources get the
//! forms that engine accepted.

use std::ops::ControlFlow;

use boa_ast::expression::operator::Assign;
use boa_ast::expression::operator::assign::{AssignOp, AssignTarget};
use boa_ast::expression::operator::binary::{ArithmeticOp, BinaryOp};
use boa_ast::expression::operator::unary::UnaryOp;
use boa_ast::expression::{Call, Expression, Identifier};
use boa_ast::scope::Scope;
use boa_ast::visitor::{VisitWith, VisitorMut};
use boa_ast::{Position, Span};
use boa_interner::{Interner, ToIndentedString};
use boa_parser::{Parser, Source};
use motion_format::{Diagnostic, Kind, Result};

/// The engine a project evaluates its expressions with, which decides the
/// forms a source may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// The modern engine: ECMAScript 2018.
    Modern,
    /// The legacy engine: ECMAScript 3, with `if … else` on one line, the
    /// snake_case aliases and `this(…)` for `thisLayer(…)`.
    Legacy,
}

/// A source ready to compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewritten {
    /// The source with every arithmetic operator a helper call.
    pub source: String,
    /// The legacy forms that were repaired, as messages.
    pub repairs: Vec<String>,
}

/// The most legacy forms repaired in one source.
const MAX_REPAIRS: usize = 64;

/// Rewrites `source` for `dialect`: `a + b` becomes `$add(a, b)`, and `-`,
/// `*`, `/`, `%`, unary minus and compound assignments likewise, so arrays
/// combine as the language says. A legacy source may also have `else` right
/// after a statement, which gets the semicolon it lacks, and `this(…)`,
/// which becomes `thisLayer(…)`.
///
/// # Errors
/// A syntax diagnostic when the source does not parse, after any repairs.
pub fn rewrite(source: &str, dialect: Dialect) -> Result<Rewritten> {
    let mut text = source.to_owned();
    let mut repairs = Vec::new();
    loop {
        let mut interner = Interner::default();
        let parsed = Parser::new(Source::from_bytes(&text))
            .parse_script(&Scope::new_global(), &mut interner);
        match parsed {
            Ok(mut script) => {
                let mut visitor = Rewrite {
                    interner: &mut interner,
                    dialect,
                };
                let _ = script.visit_with_mut(&mut visitor);
                return Ok(Rewritten {
                    source: script.to_indented_string(&interner, 0),
                    repairs,
                });
            }
            Err(error) => {
                let message = error.to_string();
                let repaired = (dialect == Dialect::Legacy && repairs.len() < MAX_REPAIRS)
                    .then(|| repair_else(&text, &message))
                    .flatten();
                match repaired {
                    Some((fixed, line)) => {
                        repairs.push(format!("a semicolon before `else` on line {line}"));
                        text = fixed;
                    }
                    None => {
                        return Err(Diagnostic::new(
                            Kind::Syntax,
                            format!("the expression does not parse: {message}"),
                        ));
                    }
                }
            }
        }
    }
}

/// `text` with a semicolon before the `else` the parser stopped at, when
/// that is what it reports, and the line.
fn repair_else(text: &str, message: &str) -> Option<(String, usize)> {
    if !message.contains("got 'else'") {
        return None;
    }
    let (line, column) = position(message)?;
    let start: usize = text
        .split_inclusive('\n')
        .take(line - 1)
        .map(str::len)
        .sum();
    let line_text = text[start..].split('\n').next()?;
    // Columns count characters from 1.
    let offset = line_text.char_indices().nth(column - 1).map(|(i, _)| i)?;
    let at = start + offset;
    text[at..].starts_with("else").then(|| {
        let mut fixed = text.to_owned();
        fixed.insert(at, ';');
        (fixed, line)
    })
}

/// The `line N, col M` of a parser message.
fn position(message: &str) -> Option<(usize, usize)> {
    let rest = &message[message.rfind("line ")? + 5..];
    let (line, rest) = rest.split_once(", col ")?;
    let column: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some((line.trim().parse().ok()?, column.parse().ok()?))
}

/// The syntax tree visitor that makes the rewrite.
struct Rewrite<'i> {
    interner: &'i mut Interner,
    dialect: Dialect,
}

impl Rewrite<'_> {
    fn call(&mut self, name: &str, args: Vec<Expression>) -> Expression {
        let span = span();
        let callee: Expression = Identifier::new(self.interner.get_or_intern(name), span).into();
        Call::new(callee, args.into(), span).into()
    }
}

/// Where rewritten nodes claim to be: they have no place in the source.
fn span() -> Span {
    let start = Position::new(1, 1);
    Span::new(start, start)
}

/// The helper for an arithmetic operator; none for `**`, which takes numbers only.
fn helper(op: ArithmeticOp) -> Option<&'static str> {
    match op {
        ArithmeticOp::Add => Some("$add"),
        ArithmeticOp::Sub => Some("$sub"),
        ArithmeticOp::Mul => Some("$mul"),
        ArithmeticOp::Div => Some("$div"),
        ArithmeticOp::Mod => Some("$mod"),
        ArithmeticOp::Exp => None,
    }
}

/// The helper for a compound assignment such as `+=`.
fn assign_helper(op: AssignOp) -> Option<&'static str> {
    match op {
        AssignOp::Add => Some("$add"),
        AssignOp::Sub => Some("$sub"),
        AssignOp::Mul => Some("$mul"),
        AssignOp::Div => Some("$div"),
        AssignOp::Mod => Some("$mod"),
        _ => None,
    }
}

impl<'ast> VisitorMut<'ast> for Rewrite<'_> {
    type BreakTy = ();

    fn visit_expression_mut(&mut self, node: &'ast mut Expression) -> ControlFlow<()> {
        node.visit_with_mut(self)?;
        let placeholder =
            || Expression::Identifier(Identifier::new(boa_interner::Sym::EMPTY_STRING, span()));
        let replaced = match node {
            Expression::Binary(binary) => match binary.op() {
                BinaryOp::Arithmetic(op) => helper(op).map(|name| {
                    let lhs = std::mem::replace(binary.lhs_mut(), placeholder());
                    let rhs = std::mem::replace(binary.rhs_mut(), placeholder());
                    self.call(name, vec![lhs, rhs])
                }),
                _ => None,
            },
            Expression::Unary(unary) if unary.op() == UnaryOp::Minus => {
                let target = std::mem::replace(unary.target_mut(), placeholder());
                Some(self.call("$neg", vec![target]))
            }
            Expression::Assign(assign) => {
                let target: Option<Expression> = match assign.lhs() {
                    AssignTarget::Identifier(identifier) => Some((*identifier).into()),
                    AssignTarget::Access(access) => Some(access.clone().into()),
                    AssignTarget::Pattern(_) => None,
                };
                match (assign_helper(assign.op()), target) {
                    (Some(name), Some(target)) => {
                        let rhs = assign.rhs().clone();
                        let value = self.call(name, vec![target, rhs]);
                        Some(Assign::new(AssignOp::Assign, assign.lhs().clone(), value).into())
                    }
                    _ => None,
                }
            }
            Expression::Call(call)
                if self.dialect == Dialect::Legacy
                    && matches!(call.function(), Expression::This(_)) =>
            {
                let args = call.args().to_vec();
                Some(self.call("thisLayer", args))
            }
            _ => None,
        };
        if let Some(replaced) = replaced {
            *node = replaced;
        }
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/rewrite.rs"]
mod tests;
