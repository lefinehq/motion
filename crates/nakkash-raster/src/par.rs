//! Work inside one frame, split over runs of rows on the thread pool the
//! caller draws on ([D75]).
//!
//! Every helper here hands each task its own items to write and lets it
//! read anything shared; no task sums, counts or keeps anything across
//! another's items. Each item is worked out by the same code, in the same
//! order of operations, whichever thread runs it and however the work was
//! split, so a frame drawn on one thread and on many has the same bits
//! ([D12]).
//!
//! The helpers run on the pool of the thread that calls them: the
//! engine's, which draws a render's frames and holds as many threads as
//! the render may use, or the process's default pool for a caller outside
//! one. A frame drawn on a thread of a busy pool works its own rows when no
//! other thread is free to take them.
//!
//! Making an effect parallel is changing its loop over pixels into one of
//! these calls; the body stays as it was, and must only write the item it
//! is given.
//!
//! [D12]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d12-what-repeatable-means
//! [D75]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d75-a-frame-is-drawn-on-every-thread-of-its-render

use rayon::prelude::*;

/// About how many items one task works: enough that splitting costs
/// little beside the work, few enough that a frame splits into hundreds
/// of tasks for the threads to share.
pub(crate) const TASK: usize = 1 << 14;

/// How many whole rows of `width` items a task works.
fn rows_per_task(width: usize) -> usize {
    (TASK / width.max(1)).max(1)
}

/// Runs `work` on every item of `data` with its index.
pub(crate) fn each<T: Send>(data: &mut [T], work: impl Fn(usize, &mut T) + Sync) {
    data.par_chunks_mut(TASK)
        .enumerate()
        .for_each(|(task, items)| {
            let first = task * TASK;
            for (i, item) in items.iter_mut().enumerate() {
                work(first + i, item);
            }
        });
}

/// Runs `work` on every item of `data` with its index and the item of
/// `other` at the same index; `other` is at least as long.
pub(crate) fn each_with<T: Send, U: Sync>(
    data: &mut [T],
    other: &[U],
    work: impl Fn(usize, &mut T, &U) + Sync,
) {
    debug_assert!(other.len() >= data.len(), "every item has its pair");
    data.par_chunks_mut(TASK)
        .enumerate()
        .for_each(|(task, items)| {
            let first = task * TASK;
            for (i, (item, with)) in items.iter_mut().zip(&other[first..]).enumerate() {
                work(first + i, item, with);
            }
        });
}

/// A copy of `data`, made a run of items at a time on every thread.
pub(crate) fn copied<T: Copy + Default + Send + Sync>(data: &[T]) -> Vec<T> {
    let mut copy = vec![T::default(); data.len()];
    copy.par_chunks_mut(TASK)
        .zip(data.par_chunks(TASK))
        .for_each(|(to, from)| to.copy_from_slice(from));
    copy
}

/// What `make` gives for each item of `data`, with its index, in order.
pub(crate) fn map<T: Sync, U: Copy + Default + Send>(
    data: &[T],
    make: impl Fn(usize, &T) -> U + Sync,
) -> Vec<U> {
    let mut made = vec![U::default(); data.len()];
    each_with(&mut made, data, |i, out, item| *out = make(i, item));
    made
}

/// Runs `work` on every row of `data`, `width` items long, with the row's
/// index and scratch room `scratch` made once per task.
pub(crate) fn rows<T: Send, S>(
    data: &mut [T],
    width: usize,
    scratch: impl Fn() -> S + Sync,
    work: impl Fn(usize, &mut [T], &mut S) + Sync,
) {
    if width == 0 {
        return;
    }
    data.par_chunks_mut(width)
        .with_min_len(rows_per_task(width))
        .enumerate()
        .for_each_init(&scratch, |room, (y, row)| work(y, row, room));
}

/// Runs `work` on every band of `band` rows of `data`, `width` items
/// long, with the index of the band's first row: for work that keeps rows
/// together, such as tiles.
pub(crate) fn bands<T: Send>(
    data: &mut [T],
    width: usize,
    band: usize,
    work: impl Fn(usize, &mut [T]) + Sync,
) {
    if width == 0 {
        return;
    }
    let band = band.max(1);
    data.par_chunks_mut(width * band)
        .enumerate()
        .for_each(|(i, rows)| work(i * band, rows));
}

/// How many columns one task of [`columns`] reads at once: a run of
/// pixels long enough to fill cache lines.
const COLUMNS: usize = 16;

/// Runs `work` on every column of `data`, `width` items wide and `height`
/// tall, as one run of items from the top, with scratch room `scratch`
/// made once per task; the column then replaces the one in `data`. Each
/// task copies out a band of columns and works them one by one; the bands
/// are copied back row by row after.
pub(crate) fn columns<T: Copy + Default + Send + Sync, S>(
    data: &mut [T],
    width: usize,
    height: usize,
    scratch: impl Fn() -> S + Sync,
    work: impl Fn(&mut [T], &mut S) + Sync,
) {
    if width == 0 || height == 0 {
        return;
    }
    debug_assert_eq!(data.len(), width * height, "a grid of width by height");
    // The bands one after another, each its rows of [`COLUMNS`] items: a
    // band is copied out and back a row at a time, and its columns are
    // read and written within it while it sits in cache.
    let source = &*data;
    let mut bands = vec![T::default(); width * height];
    bands
        .par_chunks_mut(COLUMNS * height)
        .enumerate()
        .for_each_init(
            || (scratch(), Vec::with_capacity(height)),
            |(room, column), (band, out)| {
                let x0 = band * COLUMNS;
                let wide = out.len() / height;
                for (y, row) in out.chunks_exact_mut(wide).enumerate() {
                    row.copy_from_slice(&source[y * width + x0..y * width + x0 + wide]);
                }
                for k in 0..wide {
                    column.clear();
                    column.extend(out.iter().skip(k).step_by(wide));
                    work(column, room);
                    for (item, v) in out.iter_mut().skip(k).step_by(wide).zip(column.iter()) {
                        *item = *v;
                    }
                }
            },
        );
    let bands = &bands;
    data.par_chunks_mut(width)
        .with_min_len(rows_per_task(width))
        .enumerate()
        .for_each(|(y, row)| {
            for x0 in (0..width).step_by(COLUMNS) {
                let wide = COLUMNS.min(width - x0);
                let band = &bands[x0 * height..];
                row[x0..x0 + wide].copy_from_slice(&band[y * wide..(y + 1) * wide]);
            }
        });
}

#[cfg(test)]
#[path = "../tests/unit/par.rs"]
mod tests;
