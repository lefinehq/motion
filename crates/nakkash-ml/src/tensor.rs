//! [`Tensor`]: the values graphs take and give.

use nakkash_format::{Diagnostic, Kind, Result};

/// A dense tensor: its shape and its values, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Tensor {
    /// The size of each dimension.
    pub shape: Vec<usize>,
    /// The values, as many as the shape's product.
    pub data: Data,
}

/// The values of a [`Tensor`], by element type.
#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    /// 32-bit floats.
    F32(Vec<f32>),
    /// 64-bit integers.
    I64(Vec<i64>),
    /// 32-bit integers.
    I32(Vec<i32>),
}

impl Tensor {
    /// A float tensor of `shape`.
    ///
    /// # Panics
    /// When `values` does not fill `shape`.
    pub fn f32(shape: impl Into<Vec<usize>>, values: Vec<f32>) -> Self {
        Self::new(shape.into(), Data::F32(values))
    }

    /// A 64-bit integer tensor of `shape`.
    ///
    /// # Panics
    /// When `values` does not fill `shape`.
    pub fn i64(shape: impl Into<Vec<usize>>, values: Vec<i64>) -> Self {
        Self::new(shape.into(), Data::I64(values))
    }

    /// A 32-bit integer tensor of `shape`.
    ///
    /// # Panics
    /// When `values` does not fill `shape`.
    pub fn i32(shape: impl Into<Vec<usize>>, values: Vec<i32>) -> Self {
        Self::new(shape.into(), Data::I32(values))
    }

    fn new(shape: Vec<usize>, data: Data) -> Self {
        let len = match &data {
            Data::F32(v) => v.len(),
            Data::I64(v) => v.len(),
            Data::I32(v) => v.len(),
        };
        assert_eq!(
            shape.iter().product::<usize>(),
            len,
            "{len} values for shape {shape:?}"
        );
        Self { shape, data }
    }

    /// Its float values.
    ///
    /// # Errors
    /// A [`Kind::WrongType`] diagnostic when it holds integers.
    pub fn into_f32(self) -> Result<Vec<f32>> {
        match self.data {
            Data::F32(values) => Ok(values),
            _ => Err(Diagnostic::new(
                Kind::WrongType,
                "a graph gave integers where floats were expected",
            )),
        }
    }
}

/// What one run of a graph gave: its outputs by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outputs(pub Vec<(String, Tensor)>);

impl Outputs {
    /// Takes the output called `name`.
    ///
    /// # Errors
    /// A [`Kind::Missing`] diagnostic when the graph gave no such output.
    pub fn take(&mut self, name: &str) -> Result<Tensor> {
        let at = self.0.iter().position(|(n, _)| n == name).ok_or_else(|| {
            Diagnostic::new(Kind::Missing, format!("the graph gave no output `{name}`"))
        })?;
        Ok(self.0.swap_remove(at).1)
    }

    /// Takes the float values of the output called `name`.
    ///
    /// # Errors
    /// As [`Outputs::take`] and [`Tensor::into_f32`].
    pub fn floats(&mut self, name: &str) -> Result<Vec<f32>> {
        self.take(name)?.into_f32()
    }
}

#[cfg(test)]
#[path = "../tests/unit/tensor.rs"]
mod tests;
