//! `--stats`: what a command cost the machine ([D90]), printed at its end.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::time::Instant;

use nakkash_machine::{MIB, Swap};

/// What was true when the command started.
pub struct Start {
    at: Instant,
    swap: Option<Swap>,
}

impl Start {
    /// Now.
    pub fn now() -> Start {
        Start {
            at: Instant::now(),
            swap: nakkash_machine::swap(),
        }
    }

    /// The summary, one line per kind of resource, each starting `stats:`.
    pub fn report(&self) -> Vec<String> {
        let usage = nakkash_machine::usage();
        let memory = nakkash_engine::memory_stats();
        let mib = |bytes: u64| bytes as f64 / MIB as f64;
        let over = memory.peak_working.saturating_sub(memory.budget);
        let swap = match (self.swap, nakkash_machine::swap()) {
            (Some(before), Some(after)) => after.since(before),
            _ => "unknown".to_owned(),
        };
        let written = match usage.written {
            Some(bytes) => format!("{bytes} B"),
            None => "unknown".to_owned(),
        };
        vec![
            format!(
                "stats: time: wall {:.2} s, user {:.2} s, system {:.2} s",
                self.at.elapsed().as_secs_f64(),
                usage.user.as_secs_f64(),
                usage.system.as_secs_f64()
            ),
            format!(
                "stats: memory: peak resident {:.1} MiB; budget {:.0} MiB, held at most {:.1} MiB, working grids at most {:.1} MiB ({:.1} MiB over the budget)",
                mib(usage.peak_resident),
                mib(memory.budget),
                mib(memory.peak),
                mib(memory.peak_working),
                mib(over),
            ),
            format!(
                "stats: budget: reached {} times; drawings let go {}; grids reused {} of {}; pooled grids let go {}; frames held back {}; memory pressure {} times",
                memory.hits,
                memory.evictions,
                memory.reused,
                memory.made,
                memory.trimmed,
                memory.held_back,
                memory.pressed,
            ),
            format!(
                "stats: plan: drawings reused {}; frames copied {}",
                nakkash_engine::drawings_reused(),
                nakkash_engine::frames_copied(),
            ),
            format!("stats: swap: {} major faults; {swap}", usage.major_faults),
            format!("stats: storage: written {written}"),
        ]
    }
}
