use std::sync::{Arc, Mutex};

use super::*;
use crate::Outputs;

fn others(frames: &[usize]) -> BTreeMap<usize, ()> {
    frames.iter().map(|&f| (f, ())).collect()
}

#[test]
fn a_frame_reads_the_prompt_then_six_before_then_pointers() {
    // One prompt at 0, tracking forward at frame 9 of 48, frames 1 to 8 held.
    let held = others(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let s = select(9, 48, 1, vec![0], &held);
    // The prompt at temporal index 6, then frames 3 to 8 (t_pos 1 to 6) at
    // indices 5 down to 0, oldest first.
    assert_eq!(
        s.spatial,
        [(0, 6), (3, 5), (4, 4), (5, 3), (6, 2), (7, 1), (8, 0)]
    );
    // The prompt's pointer at its distance over 15, then 8, 7, … 1.
    let expected: Vec<(usize, f32)> = std::iter::once((0, 9.0 / 15.0))
        .chain((1..=8).map(|d| (9 - d, d as f32 / 15.0)))
        .collect();
    assert_eq!(s.pointers, expected);
}

#[test]
fn early_frames_read_what_exists() {
    let s = select(1, 48, 1, vec![0], &others(&[]));
    assert_eq!(s.spatial, [(0, 6)]);
    assert_eq!(s.pointers, [(0, 1.0 / 15.0)]);
    // A clip shorter than sixteen frames scales offsets by its own length.
    let s = select(2, 4, 1, vec![0], &others(&[1]));
    assert_eq!(s.pointers, [(0, 2.0 / 3.0), (1, 1.0 / 3.0)]);
}

#[test]
fn backward_reads_the_frames_after_and_only_prompts_passed() {
    // Prompts at 10 and 30; going backward at frame 8, the forward pass
    // left 11 to 14 and the backward pass 9.
    let held = others(&[9, 11, 12, 13, 14]);
    let s = select(8, 48, -1, vec![10, 30], &held);
    // Every prompt is a spatial memory; then 8 + (7 - t_pos): 14, 13, 12,
    // 11, (10 is a prompt), 9.
    assert_eq!(
        s.spatial,
        [(10, 6), (30, 6), (14, 5), (13, 4), (12, 3), (11, 2), (9, 0)]
    );
    // Going backward from 8, both prompts lie behind it, so both point.
    assert_eq!(s.pointers[..2], [(10, 2.0 / 15.0), (30, 22.0 / 15.0)]);
    assert_eq!(
        s.pointers[2..],
        [
            (9, 1.0 / 15.0),
            (11, 3.0 / 15.0),
            (12, 4.0 / 15.0),
            (13, 5.0 / 15.0),
            (14, 6.0 / 15.0)
        ]
    );
    // Forward at 20 the prompt at 30 is a memory but not yet a pointer.
    let s = select(20, 48, 1, vec![10, 30], &others(&[19]));
    assert_eq!(s.pointers[0], (10, 10.0 / 15.0));
    assert!(s.pointers.iter().all(|p| p.0 != 30));
}

#[test]
fn memories_round_to_bfloat16_as_stored() {
    let mut v = [1.0, 1.0 + 1.0 / 256.0, 1.0 + 3.0 / 256.0, -0.1];
    bfloat16(&mut v);
    // 1 + 2^-8 is a tie between 1 and 1 + 2^-7: to even, 1.
    assert_eq!(v[..3], [1.0, 1.0, 1.0 + 4.0 / 256.0]);
    assert_eq!(v[3].to_bits() & 0xffff, 0);
}

/// What the fake graphs saw.
#[derive(Default)]
struct Seen {
    /// Per memory attention run: the frame of each spatial memory.
    attended: Vec<Vec<f32>>,
    /// Per decode: the multimask decoder or not, and the point count.
    decoded: Vec<(bool, usize)>,
}

/// A graph computing `f` from its inputs.
struct Fake {
    inputs: &'static [&'static str],
    f: Box<dyn FnMut(Vec<(String, Tensor)>) -> Outputs + Send>,
}

impl Graph for Fake {
    fn inputs(&self) -> Vec<String> {
        self.inputs.iter().map(|s| s.to_string()).collect()
    }
    fn run(&mut self, inputs: Vec<(String, Tensor)>) -> Result<Outputs> {
        Ok((self.f)(inputs))
    }
}

fn get(inputs: &[(String, Tensor)], name: &str) -> Tensor {
    inputs.iter().find(|(n, _)| n == name).unwrap().1.clone()
}

fn first(t: &Tensor) -> f32 {
    match &t.data {
        crate::Data::F32(v) => v[0],
        _ => panic!("floats"),
    }
}

/// Five fake graphs in which a frame's identity, its pixel value, flows
/// through features, memories and pointers, so the test can see what each
/// frame read.
fn fakes(seen: Arc<Mutex<Seen>>) -> [Box<dyn Graph>; 5] {
    let filled =
        |shape: &[usize], v: f32| Tensor::f32(shape.to_vec(), vec![v; shape.iter().product()]);
    let encoder = Fake {
        inputs: &["image"],
        f: Box::new(move |i| {
            let v = first(&get(&i, "image"));
            Outputs(vec![
                ("feat_s0".into(), filled(&[1, 32, 256, 256], 0.0)),
                ("feat_s1".into(), filled(&[1, 64, 128, 128], 0.0)),
                ("vision_feat".into(), filled(&[1, 256, 64, 64], v)),
                ("vision_pos".into(), filled(&[1, 256, 64, 64], 0.0)),
            ])
        }),
    };
    let decoder = |multi: bool, seen: Arc<Mutex<Seen>>| Fake {
        inputs: &[
            "pix_feat",
            "feat_s0",
            "feat_s1",
            "point_coords",
            "point_labels",
        ],
        f: Box::new(move |i| {
            let k = get(&i, "point_labels").shape[1];
            seen.lock().unwrap().decoded.push((multi, k));
            let v = first(&get(&i, "pix_feat"));
            Outputs(vec![
                ("low_res_mask".into(), filled(&[1, 1, 256, 256], v)),
                ("high_res_mask".into(), filled(&[1, 1, 1024, 1024], v)),
                ("iou".into(), filled(&[1, 1], 1.0)),
                ("obj_ptr".into(), filled(&[1, 256], v)),
                ("obj_score".into(), filled(&[1, 1], 1.0)),
            ])
        }),
    };
    let memory_encoder = Fake {
        // `vision_feat` only: an export may drop inputs it does not use.
        inputs: &["vision_feat", "high_res_mask", "obj_score"],
        f: Box::new(move |i| {
            let v = first(&get(&i, "vision_feat"));
            Outputs(vec![
                ("maskmem_feat".into(), filled(&[1, 64, 64, 64], v)),
                ("maskmem_pos".into(), filled(&[1, 64, 64, 64], 0.0)),
            ])
        }),
    };
    let attend_seen = seen.clone();
    let attention = Fake {
        inputs: &[
            "curr",
            "curr_pos",
            "mem_feat",
            "mem_tpos_idx",
            "maskmem_pos",
            "obj_ptrs",
            "obj_ptr_tpos",
        ],
        f: Box::new(move |i| {
            let mem = get(&i, "mem_feat");
            let per = 64 * 64 * 64;
            let crate::Data::F32(values) = &mem.data else {
                panic!()
            };
            attend_seen
                .lock()
                .unwrap()
                .attended
                .push(values.chunks(per).map(|c| c[0]).collect());
            Outputs(vec![("pix_feat".into(), get(&i, "curr"))])
        }),
    };
    [
        Box::new(encoder),
        Box::new(decoder(true, seen.clone())),
        Box::new(decoder(false, seen)),
        Box::new(memory_encoder),
        Box::new(attention),
    ]
}

/// Frames filled with grey levels 10, 20, 30, …, so each is known by its
/// normalised red value.
struct Greys(usize);

impl Frames for Greys {
    fn len(&self) -> usize {
        self.0
    }
    fn size(&self) -> (usize, usize) {
        (8, 6)
    }
    fn frame(&mut self, index: usize) -> Result<Rgb8> {
        Ok(Rgb8 {
            width: 8,
            height: 6,
            data: vec![(10 * (index + 1)) as u8; 8 * 6 * 3],
        })
    }
}

fn id(frame: usize) -> f32 {
    // The red channel's normalisation, as the encoder does it; bfloat16, as
    // memories are stored.
    let mut v = [((10 * (frame + 1)) as f32 / 255.0 - MEAN[0]) / DEVIATION[0]];
    bfloat16(&mut v);
    v[0]
}

#[test]
fn propagates_both_ways_from_the_first_prompt_reading_the_memories_selected() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let mut segmenter = Segmenter::from_graphs(fakes(seen.clone()), vec![0.0; 256]);
    let prompts = [
        Prompt {
            frame: 2,
            area: Some([1.0, 1.0, 4.0, 4.0]),
            include: vec![],
            exclude: vec![],
        },
        Prompt {
            frame: 5,
            area: None,
            include: vec![[3.0, 3.0]],
            exclude: vec![],
        },
    ];
    let mut order = Vec::new();
    let masks = segmenter
        .segment(&mut Greys(7), &prompts, |f| order.push(f))
        .unwrap();
    assert_eq!(masks.len(), 7);
    assert!(
        masks
            .iter()
            .all(|m| m.width == 256 && m.data.len() == 256 * 256)
    );
    // Prompts first, then forward from the first, then backward.
    assert_eq!(order, [2, 5, 3, 4, 6, 1, 0]);
    let seen = seen.lock().unwrap();
    // A box is two points and decodes one mask; a click decodes three; the
    // rest decode three from no point.
    assert_eq!(seen.decoded[..2], [(false, 2), (true, 1)]);
    assert!(seen.decoded[2..].iter().all(|d| *d == (true, 1)));
    // Frame 3 reads both prompts; frame 6 reads both, then 3 and 4 (5 is a
    // prompt); frame 1 going back reads both prompts, then 4 and 3 after it.
    assert_eq!(seen.attended[0], [id(2), id(5)]);
    assert_eq!(seen.attended[2], [id(2), id(5), id(3), id(4)]);
    assert_eq!(seen.attended[3], [id(2), id(5), id(6), id(4), id(3)]);
}

#[test]
fn refuses_prompts_that_point_at_nothing() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let mut segmenter = Segmenter::from_graphs(fakes(seen), vec![0.0; 256]);
    let empty = Prompt {
        frame: 0,
        area: None,
        include: vec![],
        exclude: vec![[1.0, 1.0]],
    };
    let past = Prompt {
        frame: 9,
        area: Some([0.0, 0.0, 1.0, 1.0]),
        include: vec![],
        exclude: vec![],
    };
    let twice = Prompt {
        frame: 0,
        area: Some([0.0, 0.0, 1.0, 1.0]),
        include: vec![],
        exclude: vec![],
    };
    for prompts in [vec![], vec![empty], vec![past], vec![twice.clone(), twice]] {
        let error = segmenter
            .segment(&mut Greys(3), &prompts, |_| {})
            .unwrap_err();
        assert_eq!(error.kind, Kind::Invalid);
    }
}
