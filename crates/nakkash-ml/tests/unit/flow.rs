use super::*;

fn field(w: usize, h: usize, dx: f32, dy: f32) -> Field {
    Field {
        dx: Plane::filled(w, h, dx),
        dy: Plane::filled(w, h, dy),
    }
}

#[test]
fn warping_reads_where_the_motion_points() {
    let ramp = Plane {
        width: 4,
        height: 1,
        data: vec![0.0, 1.0, 2.0, 3.0],
    };
    let moved = warp(&ramp, &field(4, 1, 1.0, 0.0));
    assert_eq!(moved.data[..2], [1.0, 2.0]);
    assert!(moved.data[3] > 2.9, "held at the edge");
}

#[test]
fn flows_agree_when_they_undo_each_other() {
    let back = field(8, 8, -2.0, 0.0);
    assert!(consistent(&back, &field(8, 8, 2.0, 0.0)).iter().all(|&v| v));
    assert!(
        consistent(&back, &field(8, 8, -2.0, 0.0))
            .iter()
            .all(|&v| !v)
    );
}

#[test]
fn blending_pulls_small_differences_and_keeps_large_ones() {
    let still = field(2, 1, 0.0, 0.0);
    let previous = Plane {
        width: 2,
        height: 1,
        data: vec![0.5, 0.0],
    };
    let matte = Plane {
        width: 2,
        height: 1,
        data: vec![0.52, 1.0],
    };
    let out = blend(&matte, &previous, &still, &[true, true], 0.6, 0.08);
    assert!(
        (out.data[0] - 0.52).abs() > 0.005 && out.data[0] > 0.5,
        "pulled: {}",
        out.data[0]
    );
    assert!(out.data[1] > 0.99, "a real change stays: {}", out.data[1]);
    let kept = blend(&matte, &previous, &still, &[false, false], 0.6, 0.08);
    assert_eq!(kept, matte);
}

#[test]
fn brings_the_flow_of_another_size_back_to_its_frames() {
    use crate::Outputs;
    use crate::segment::tests::Fake;

    let (fw, fh) = SIZE;
    let graph = Fake {
        inputs: &["image1", "image2"],
        f: Box::new(move |inputs: Vec<(String, Tensor)>| {
            // Both frames come in at the graph's size, channels first,
            // values 0 to 255.
            assert!(inputs.iter().all(|(_, t)| t.shape == [1, 3, fh, fw]));
            let crate::Data::F32(v) = &inputs[0].1.data else {
                panic!()
            };
            assert_eq!(v[0], 200.0);
            // Ten pixels right and four down, everywhere.
            let mut flow = vec![10.0; fw * fh];
            flow.extend(vec![4.0; fw * fh]);
            Outputs(vec![("flow".into(), Tensor::f32([1, 2, fh, fw], flow))])
        }),
    };
    let mut model = Flow::from_graph(Box::new(graph));
    assert!(model.run_record().is_none());
    let frame = Rgb8 {
        width: 960,
        height: 540,
        data: vec![200; 960 * 540 * 3],
    };
    let field = model.between(&frame, &frame).unwrap();
    assert_eq!((field.dx.width, field.dx.height), (960, 540));
    // Half the size: half the motion.
    assert!((field.dx.data[1000] - 5.0).abs() < 1e-4 && (field.dy.data[1000] - 2.0).abs() < 1e-4);
}
