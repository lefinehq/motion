use super::*;

#[test]
fn takes_outputs_by_name() {
    let mut outputs = Outputs(vec![
        ("a".into(), Tensor::f32([2], vec![1.0, 2.0])),
        ("b".into(), Tensor::i64([1], vec![7])),
    ]);
    assert_eq!(outputs.floats("a").unwrap(), [1.0, 2.0]);
    assert_eq!(
        outputs.take("a").unwrap_err().kind,
        Kind::Missing,
        "taken once"
    );
    assert_eq!(outputs.floats("b").unwrap_err().kind, Kind::WrongType);
}

#[test]
#[should_panic(expected = "values for shape")]
fn refuses_values_that_do_not_fill_the_shape() {
    let _ = Tensor::i32([2, 2], vec![1, 2, 3]);
}
