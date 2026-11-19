use super::*;

#[test]
fn eigenvectors_of_a_symmetric_matrix() {
    // Eigenvalues 1, 2, 4 of a rotated diagonal matrix.
    let a = [2.5, -1.5, 0.0, -1.5, 2.5, 0.0, 0.0, 0.0, 1.0];
    let (values, vectors) = symmetric_eigen(&a, 3);
    for (got, want) in values.iter().zip([1.0, 1.0, 4.0]) {
        assert!((got - want).abs() < 1e-12, "{values:?}");
    }
    // Each column v satisfies A v = λ v.
    for i in 0..3 {
        for r in 0..3 {
            let av: f64 = (0..3).map(|k| a[r * 3 + k] * vectors[k * 3 + i]).sum();
            assert!((av - values[i] * vectors[r * 3 + i]).abs() < 1e-12);
        }
    }
}

#[test]
fn inverts_and_maps() {
    let h = [[2.0, 0.0, 1.0], [0.0, 3.0, -2.0], [0.0, 0.001, 1.0]];
    let inv = inverse(&h).unwrap();
    let back = mul(&h, &inv);
    for (i, row) in back.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            assert!((v - IDENTITY[i][j]).abs() < 1e-12);
        }
    }
    let p = apply(&h, [10.0, 20.0]).unwrap();
    let q = apply(&inv, p).unwrap();
    assert!((q[0] - 10.0).abs() < 1e-9 && (q[1] - 20.0).abs() < 1e-9);
    assert!(inverse(&[[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [0.0, 0.0, 1.0]]).is_none());
    assert!(
        apply(
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            [0.0, 5.0]
        )
        .is_none()
    );
}
