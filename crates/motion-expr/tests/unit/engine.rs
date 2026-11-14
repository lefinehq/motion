//! Unit tests for `src/engine.rs`: the corpus's idioms over a small host.

use std::collections::HashMap;
use std::rc::Rc;

use pretty_assertions::assert_eq;

use super::*;

/// A node of the test host.
#[derive(Default)]
struct TestNode {
    kind: Option<NodeKind>,
    name: String,
    up: Option<NodeId>,
    children: Vec<NodeId>,
    attributes: HashMap<&'static str, Val>,
    keys: Vec<(f64, Val)>,
    value: Option<Val>,
    /// Holds an expression not baked yet.
    pending: bool,
}

/// A project: comp "Main" (30 fps), layers "CONTROLS" and "Title".
struct TestHost {
    nodes: Vec<TestNode>,
}

impl TestHost {
    fn add(&mut self, up: Option<NodeId>, kind: NodeKind, name: &str) -> NodeId {
        let id = self.nodes.len() as NodeId;
        self.nodes.push(TestNode {
            kind: Some(kind),
            name: name.into(),
            up,
            ..TestNode::default()
        });
        if let Some(up) = up {
            self.nodes[up as usize].children.push(id);
        }
        id
    }

    fn property(&mut self, up: NodeId, name: &str, value: Val) -> NodeId {
        let id = self.add(Some(up), NodeKind::Property, name);
        self.nodes[id as usize].value = Some(value);
        id
    }

    fn keyed(&mut self, up: NodeId, name: &str, keys: Vec<(f64, Val)>) -> NodeId {
        let id = self.add(Some(up), NodeKind::Property, name);
        self.nodes[id as usize].keys = keys;
        id
    }
}

/// Node ids of the test project.
struct Ids {
    position: NodeId,
    opacity: NodeId,
    rotation: NodeId,
}

fn v2(x: f64, y: f64) -> Val {
    Val::Vector(vec![x, y])
}

fn project() -> (Rc<TestHost>, Ids) {
    let mut h = TestHost { nodes: Vec::new() };
    let project = h.add(None, NodeKind::Project, "project");
    let main = h.add(Some(project), NodeKind::Comp, "Main");
    for (k, v) in [
        ("width", 1920.0),
        ("height", 1080.0),
        ("frameDuration", 1.0 / 30.0),
        ("duration", 10.0),
        ("numLayers", 2.0),
    ] {
        h.nodes[main as usize].attributes.insert(k, Val::Number(v));
    }
    let controls = h.add(Some(main), NodeKind::Layer, "CONTROLS");
    h.nodes[controls as usize]
        .attributes
        .insert("index", Val::Number(1.0));
    let effects = h.add(Some(controls), NodeKind::Group, "ADBE Effect Parade");
    let colour = h.add(Some(effects), NodeKind::Group, "Color_01");
    h.property(
        colour,
        "ADBE Color Control-0001",
        Val::Vector(vec![0.0, 0.0, 0.0, 1.0]),
    );
    let check = h.add(Some(effects), NodeKind::Group, "Shadows ON/OFF");
    h.property(check, "ADBE Checkbox Control-0001", Val::Number(1.0));
    let title = h.add(Some(main), NodeKind::Layer, "Title");
    h.nodes[title as usize]
        .attributes
        .insert("index", Val::Number(2.0));
    h.nodes[title as usize]
        .attributes
        .insert("inPoint", Val::Number(0.0));
    let transform = h.add(Some(title), NodeKind::Group, "ADBE Transform Group");
    let position = h.keyed(
        transform,
        "ADBE Position",
        vec![
            (0.0, v2(0.0, 0.0)),
            (1.0, v2(500.0, 300.0)),
            (2.0, v2(800.0, 300.0)),
        ],
    );
    let opacity = h.keyed(
        transform,
        "ADBE Opacity",
        vec![(0.0, Val::Number(0.0)), (1.0, Val::Number(100.0))],
    );
    let rotation = h.property(transform, "ADBE Rotate Z", Val::Number(0.0));
    h.nodes[rotation as usize].pending = true;
    h.keyed(
        transform,
        "ADBE Scale",
        vec![(0.0, v2(0.0, 0.0)), (1.0, v2(100.0, 100.0))],
    );
    (
        Rc::new(h),
        Ids {
            position,
            opacity,
            rotation,
        },
    )
}

/// The language's attribute names, as the test host knows them.
fn attribute(name: &str) -> Option<&'static str> {
    Some(match name {
        "transform" => "ADBE Transform Group",
        "effect" => "ADBE Effect Parade",
        "position" => "ADBE Position",
        "opacity" => "ADBE Opacity",
        "rotation" => "ADBE Rotate Z",
        "scale" => "ADBE Scale",
        _ => return None,
    })
}

impl Host for TestHost {
    fn project(&self) -> NodeId {
        0
    }
    fn kind(&self, node: NodeId) -> NodeKind {
        self.nodes[node as usize].kind.unwrap()
    }
    fn up(&self, node: NodeId) -> Option<NodeId> {
        self.nodes[node as usize].up
    }
    fn child(&self, node: NodeId, select: Select<'_>) -> Option<NodeId> {
        let children = &self.nodes[node as usize].children;
        let named = |name: &str| {
            children
                .iter()
                .copied()
                .find(|&c| self.nodes[c as usize].name == name)
        };
        match select {
            Select::Index(i) => children.get(usize::try_from(i - 1).ok()?).copied(),
            Select::Name(name) => named(name),
            Select::Attribute(name) => match (self.kind(node), attribute(name)) {
                (_, Some(match_name)) if named(match_name).is_some() => named(match_name),
                (NodeKind::Layer, Some(match_name)) => {
                    let transform = named("ADBE Transform Group")?;
                    let props = &self.nodes[transform as usize].children;
                    props
                        .iter()
                        .copied()
                        .find(|&c| self.nodes[c as usize].name == match_name)
                }
                _ => None,
            },
        }
    }
    fn link(&self, _: NodeId, _: &str) -> Option<NodeId> {
        None
    }
    fn attribute(&self, node: NodeId, name: &str) -> Option<Val> {
        let n = &self.nodes[node as usize];
        match name {
            "name" if n.kind != Some(NodeKind::Project) => Some(Val::Text(n.name.clone())),
            _ => n.attributes.get(name).cloned(),
        }
    }
    fn value_at(&self, node: NodeId, time: f64, own: bool) -> std::result::Result<Val, Missing> {
        let n = &self.nodes[node as usize];
        if n.pending && !own {
            return Err(Missing::Pending(node));
        }
        if let Some(v) = &n.value {
            return Ok(v.clone());
        }
        let keys = &n.keys;
        let lerp = |a: &Val, b: &Val, u: f64| match (a, b) {
            (Val::Number(a), Val::Number(b)) => Val::Number(a + (b - a) * u),
            (Val::Vector(a), Val::Vector(b)) => {
                Val::Vector(a.iter().zip(b).map(|(a, b)| a + (b - a) * u).collect())
            }
            _ => a.clone(),
        };
        if time <= keys[0].0 {
            return Ok(keys[0].1.clone());
        }
        for w in keys.windows(2) {
            if time <= w[1].0 {
                return Ok(lerp(&w[0].1, &w[1].1, (time - w[0].0) / (w[1].0 - w[0].0)));
            }
        }
        Ok(keys.last().unwrap().1.clone())
    }
    fn key_count(&self, node: NodeId) -> usize {
        self.nodes[node as usize].keys.len()
    }
    fn key(&self, node: NodeId, n: usize) -> Option<(f64, Val)> {
        self.nodes[node as usize]
            .keys
            .get(n.checked_sub(1)?)
            .cloned()
    }
}

fn engine(dialect: Dialect) -> (Engine, Ids) {
    let (host, ids) = project();
    (Engine::new(host, dialect, Limits::default()).unwrap(), ids)
}

fn eval(source: &str, this: NodeId, time: f64) -> std::result::Result<Run, Failure> {
    eval_in(Dialect::Modern, source, this, time)
}

fn eval_in(
    dialect: Dialect,
    source: &str,
    this: NodeId,
    time: f64,
) -> std::result::Result<Run, Failure> {
    let (mut engine, _) = engine(dialect);
    let compiled = engine.compile(source, dialect).unwrap();
    engine.run(compiled.program, this, time)
}

fn ids() -> Ids {
    project().1
}

fn value(run: std::result::Result<Run, Failure>) -> Val {
    run.unwrap().value
}

#[test]
fn a_control_link_reads_the_control() {
    let source = r#"thisComp.layer("CONTROLS").effect("Color_01")("ADBE Color Control-0001")"#;
    assert_eq!(
        value(eval(source, ids().opacity, 0.5)),
        Val::Vector(vec![0.0, 0.0, 0.0, 1.0])
    );
}

#[test]
fn arrays_add_as_the_language_says() {
    let p = ids().position;
    assert_eq!(value(eval("value + [10, 1]", p, 1.0)), v2(510.0, 301.0));
    assert_eq!(value(eval("value + 10", p, 1.0)), v2(510.0, 300.0));
    assert_eq!(value(eval("value * 2", p, 1.0)), v2(1000.0, 600.0));
    assert_eq!(value(eval("[1, 2] - [1]", p, 1.0)), v2(0.0, 2.0));
    assert_eq!(value(eval("'a' + 1", p, 1.0)), Val::Text("a1".into()));
    assert_eq!(
        value(eval(
            "thisComp.layer(\"Title\").transform.scale + [1, 1]",
            p,
            0.5
        )),
        v2(51.0, 51.0)
    );
}

#[test]
fn the_corpus_easing_script_eases_out() {
    let source = include_str!("easing-script.js");
    let p = ids().position;
    // Between the first two keys, out-expo from 0 to 500 (and 0 to 300) over 1 s.
    let expected =
        |t: f64, from: f64, by: f64| by * 1.000_976 * (1.0 - 2f64.powf(-10.0 * t)) + from;
    match value(eval(source, p, 0.25)) {
        Val::Vector(v) => {
            assert!((v[0] - expected(0.25, 0.0, 500.0)).abs() < 1e-9, "{v:?}");
            assert!((v[1] - expected(0.25, 0.0, 300.0)).abs() < 1e-9, "{v:?}");
        }
        other => panic!("{other:?}"),
    }
    // On a key, the key's value.
    assert_eq!(value(eval(source, p, 2.0)), v2(800.0, 300.0));
}

#[test]
fn a_bounce_after_the_last_key_settles() {
    let source =
        "n = 0; if (numKeys > 0) { n = nearestKey(time).index; if (key(n).time > time) n--; }
        t = n == 0 ? 0 : time - key(n).time;
        if (n > 0 && t < 1) { v = velocityAtTime(key(n).time - thisComp.frameDuration / 10);
          value + v * 0.05 * Math.sin(3 * t * 2 * Math.PI) / Math.exp(4 * t); } else value";
    let o = ids().opacity;
    let after = |t: f64| match value(eval(source, o, t)) {
        Val::Number(n) => n,
        other => panic!("{other:?}"),
    };
    assert!(after(1.05) > 100.0, "it overshoots");
    assert!((after(1.99) - 100.0).abs() < 0.5, "and settles");
    assert_eq!(after(0.5), 50.0);
}

#[test]
fn loops_repeat_the_keys() {
    let o = ids().opacity;
    let at = |s: &str, t: f64| match value(eval(s, o, t)) {
        Val::Number(n) => n,
        other => panic!("{other:?}"),
    };
    assert!((at("loopOut()", 1.25) - 25.0).abs() < 1e-9);
    assert!((at("loopOut('pingpong')", 1.25) - 75.0).abs() < 1e-9);
    assert!((at("loopOut('offset')", 1.25) - 125.0).abs() < 1e-9);
    assert!((at("loopOut('continue')", 1.5) - 150.0).abs() < 1e-3);
    assert!((at("loopIn()", -0.25) - 75.0).abs() < 1e-9);
    assert_eq!(at("loopOut()", 0.5), 50.0);
}

#[test]
fn a_legacy_else_on_one_line_runs() {
    let source = "x = thisComp.layer(\"CONTROLS\").effect(\"Shadows ON/OFF\")(\"ADBE Checkbox Control-0001\");\nif (x == 1) 100 else 0";
    assert_eq!(
        value(eval_in(Dialect::Legacy, source, ids().opacity, 0.0)),
        Val::Number(100.0)
    );
}

#[test]
fn legacy_aliases_exist_only_for_legacy_projects() {
    let o = ids().opacity;
    assert_eq!(
        value(eval_in(Dialect::Legacy, "this_comp.width", o, 0.0)),
        Val::Number(1920.0)
    );
    assert!(matches!(
        eval("this_comp.width", o, 0.0),
        Err(Failure::Error(_))
    ));
}

#[test]
fn nothing_a_run_sets_survives_it() {
    let (mut engine, ids) = engine(Dialect::Modern);
    let program = engine
        .compile("if (time > 1) { z = 5; position = 3; } typeof z == 'undefined' ? position[0] : z + position", Dialect::Modern)
        .unwrap()
        .program;
    let at = |e: &mut Engine, t: f64| e.run(program, ids.opacity, t).unwrap().value;
    assert_eq!(at(&mut engine, 2.0), Val::Number(8.0));
    assert_eq!(at(&mut engine, 0.5), Val::Number(250.0));
}

#[test]
fn reading_an_unbaked_expression_waits_for_it_even_in_a_try() {
    let r = ids().rotation;
    let o = ids().opacity;
    assert_eq!(
        eval("thisComp.layer('Title').transform.rotation + 1", o, 0.0),
        Err(Failure::Pending(r))
    );
    assert_eq!(
        eval(
            "try { thisComp.layer('Title').transform.rotation } catch (e) { 7 }",
            o,
            0.0
        ),
        Err(Failure::Pending(r))
    );
    // Its own value is the one under its expression.
    assert_eq!(value(eval("value + 1", r, 0.0)), Val::Number(1.0));
}

#[test]
fn runs_are_deterministic_and_sandboxed() {
    let o = ids().opacity;
    assert_eq!(
        value(eval("typeof Date", o, 0.0)),
        Val::Text("undefined".into())
    );
    assert_eq!(
        value(eval("Math.sin(1)", o, 0.0)),
        Val::Number(libm::sin(1.0))
    );
    assert!(matches!(
        eval("while (true) {}", o, 0.0),
        Err(Failure::Error(_))
    ));
    let wiggled = eval("wiggle(2, 30)", ids().position, 0.7).unwrap();
    assert!(wiggled.approximate);
    assert_eq!(eval("wiggle(2, 30)", ids().position, 0.7).unwrap(), wiggled);
    assert!(!eval("value", o, 0.7).unwrap().approximate);
    assert_eq!(eval("random()", o, 0.3), eval("Math.random()", o, 0.3));
}

#[test]
fn posterize_time_holds_values_between_steps() {
    let o = ids().opacity;
    assert_eq!(
        value(eval("posterizeTime(4); value", o, 0.6)),
        Val::Number(50.0)
    );
}

#[test]
fn a_layer_has_its_name_and_index() {
    let o = ids().opacity;
    assert_eq!(
        value(eval("thisLayer.name", o, 0.0)),
        Val::Text("Title".into())
    );
    assert_eq!(
        value(eval("thisComp.layer(1).name + index", o, 0.0)),
        Val::Text("CONTROLS2".into())
    );
    assert_eq!(value(eval("position[1]", o, 1.0)), Val::Number(300.0));
    assert_eq!(
        value(eval("thisProperty.propertyGroup(1).name", o, 0.0)),
        Val::Text("ADBE Transform Group".into())
    );
}

#[test]
fn a_value_that_is_not_one_is_an_error() {
    assert!(matches!(
        eval("undefined", ids().opacity, 0.0),
        Err(Failure::Error(_))
    ));
    assert!(matches!(
        eval("throw new Error('x')", ids().opacity, 0.0),
        Err(Failure::Error(_))
    ));
}

#[test]
fn an_encoded_script_is_reported() {
    let (mut engine, _) = engine(Dialect::Legacy);
    let error = engine
        .compile("eval(\"@JSXBIN@ES@2.1@MyBn0\")", Dialect::Legacy)
        .unwrap_err();
    assert!(error.message.contains("encoded"), "{}", error.message);
}
