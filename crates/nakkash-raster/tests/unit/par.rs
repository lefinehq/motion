//! Unit tests for `src/par.rs`.

use rstest::rstest;

use super::*;

/// Runs `work` on a pool of `threads` threads.
fn on<R: Send>(threads: usize, work: impl FnOnce() -> R + Send) -> R {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("a test pool")
        .install(work)
}

/// A grid of distinct values, `width` by `height`.
fn grid(width: usize, height: usize) -> Vec<f32> {
    (0..width * height).map(|i| (i as f32).sin()).collect()
}

/// A running sum down a line: work whose result depends on the order of
/// the line's items, as a blur pass does.
fn running(line: &mut [f32], room: &mut Vec<f32>) {
    room.clear();
    room.extend_from_slice(line);
    let mut sum = 0.0;
    for (out, v) in line.iter_mut().zip(room.iter()) {
        sum += v * 0.5;
        *out = sum;
    }
}

#[rstest]
#[case(1, 1)]
#[case(7, 3)]
#[case(300, 200)]
#[case(1000, 1)]
#[case(1, 1000)]
fn rows_work_each_row_as_a_serial_loop_does(#[case] width: usize, #[case] height: usize) {
    let mut serial = grid(width, height);
    let mut room = Vec::new();
    for row in serial.chunks_exact_mut(width) {
        running(row, &mut room);
    }
    for threads in [1, 4] {
        let mut data = grid(width, height);
        on(threads, || {
            rows(&mut data, width, Vec::new, |_, row, room| {
                running(row, room)
            })
        });
        assert_eq!(data, serial, "{threads} threads");
    }
}

#[rstest]
#[case(1, 1)]
#[case(17, 5)]
#[case(300, 200)]
#[case(1000, 1)]
#[case(1, 1000)]
fn columns_work_each_column_as_a_serial_loop_does(#[case] width: usize, #[case] height: usize) {
    let mut serial = grid(width, height);
    let mut room = Vec::new();
    let mut column = vec![0.0; height];
    for x in 0..width {
        for (y, v) in column.iter_mut().enumerate() {
            *v = serial[y * width + x];
        }
        running(&mut column, &mut room);
        for (y, v) in column.iter().enumerate() {
            serial[y * width + x] = *v;
        }
    }
    for threads in [1, 4] {
        let mut data = grid(width, height);
        on(threads, || {
            columns(&mut data, width, height, Vec::new, running)
        });
        assert_eq!(data, serial, "{threads} threads");
    }
}

#[test]
fn each_and_map_give_every_item_its_own_index() {
    let len = 3 * TASK + 5;
    for threads in [1, 4] {
        let mut data = vec![0usize; len];
        on(threads, || each(&mut data, |i, v| *v = i * 2));
        assert!(data.iter().enumerate().all(|(i, v)| *v == i * 2));
        let other: Vec<usize> = (0..len).collect();
        on(threads, || {
            each_with(&mut data, &other, |i, v, o| *v += i + o)
        });
        assert!(data.iter().enumerate().all(|(i, v)| *v == i * 4));
        let mapped = on(threads, || map(&data, |i, v| v + i));
        assert!(mapped.iter().enumerate().all(|(i, v)| *v == i * 5));
    }
}

#[test]
fn bands_hand_out_whole_runs_of_rows_from_their_first() {
    let (width, height, band) = (5, 23, 4);
    let mut data = vec![0usize; width * height];
    on(4, || {
        bands(&mut data, width, band, |y0, rows| {
            assert_eq!(y0 % band, 0);
            for (k, v) in rows.iter_mut().enumerate() {
                *v = y0 * width + k;
            }
        });
    });
    assert!(data.iter().enumerate().all(|(i, v)| *v == i));
}

#[test]
fn nothing_is_worked_on_an_empty_grid() {
    let mut data: Vec<f32> = Vec::new();
    rows(&mut data, 0, || (), |_, _, ()| unreachable!("no rows"));
    columns(&mut data, 0, 0, || (), |_, ()| unreachable!("no columns"));
    bands(&mut data, 0, 4, |_, _| unreachable!("no bands"));
}
