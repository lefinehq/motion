//! Name tables: the one way a name is looked up among named rows, so a lookup
//! and the names an unknown-name error lists always read the same rows.

/// A row of a name table.
pub(crate) trait Named {
    /// The name the row is looked up by.
    fn name(&self) -> &str;
}

impl<T> Named for (&str, T) {
    fn name(&self) -> &str {
        self.0
    }
}

impl<T> Named for (String, T) {
    fn name(&self) -> &str {
        &self.0
    }
}

/// The first of `rows` called `name`.
pub(crate) fn lookup<'a, R: Named + 'a>(
    rows: impl IntoIterator<Item = &'a R>,
    name: &str,
) -> Option<&'a R> {
    rows.into_iter().find(|row| row.name() == name)
}

/// The position of the first of `rows` called `name`.
pub(crate) fn position<'a, R: Named + 'a>(
    rows: impl IntoIterator<Item = &'a R>,
    name: &str,
) -> Option<usize> {
    rows.into_iter().position(|row| row.name() == name)
}

/// The names of `rows`, in table order.
pub(crate) fn names<'a, R: Named + 'a>(
    rows: impl IntoIterator<Item = &'a R>,
) -> impl Iterator<Item = &'a str> {
    rows.into_iter().map(Named::name)
}

#[cfg(test)]
#[path = "../tests/unit/table.rs"]
mod tests;
