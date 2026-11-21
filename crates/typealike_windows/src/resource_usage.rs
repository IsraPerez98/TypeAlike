//! The `resource usage` record of the suggestion log: the memory and the CPU time of the process,
//! and the counts of the items that core keeps. Only a build with the `suggestion-log` feature has
//! it.

use std::mem;
use std::time::{Duration, Instant};

use typealike_core::SUGGESTION_LOG_TARGET;
use typealike_core::diagnostics::{KeptItems, cpu_percent, megabytes};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// The time between two `resource usage` records.
pub(crate) const RECORD_INTERVAL: Duration = Duration::from_mins(1);
/// The length of one tick of a `FILETIME`.
const FILETIME_TICK_NANOS: u64 = 100;

/// The memory and the CPU time of the process at one time.
#[derive(Debug, Clone, Copy)]
struct ProcessUsage {
    /// The memory that only this process uses, which Task Manager shows as its memory.
    private: u64,
    working_set: u64,
    peak_working_set: u64,
    user_time: Duration,
    kernel_time: Duration,
}

impl ProcessUsage {
    /// Reads the usage of this process, or `None` when Windows does not give it.
    fn read() -> Option<Self> {
        let size = u32::try_from(mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>()).ok()?;
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: size,
            ..PROCESS_MEMORY_COUNTERS_EX::default()
        };
        // SAFETY: the pseudo handle of the own process needs no close, and `counters` is a
        // `PROCESS_MEMORY_COUNTERS_EX` of `size` bytes, which the call accepts in place of the
        // smaller struct.
        unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
                size,
            )
        }
        .ok()?;
        let [mut creation, mut exit, mut kernel, mut user] = [FILETIME::default(); 4];
        // SAFETY: the pseudo handle of the own process needs no close, and the call fills the four
        // local times.
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &raw mut creation,
                &raw mut exit,
                &raw mut kernel,
                &raw mut user,
            )
        }
        .ok()?;
        Some(Self {
            private: u64::try_from(counters.PrivateUsage).ok()?,
            working_set: u64::try_from(counters.WorkingSetSize).ok()?,
            peak_working_set: u64::try_from(counters.PeakWorkingSetSize).ok()?,
            user_time: duration_of(user),
            kernel_time: duration_of(kernel),
        })
    }
}

/// The time of `time`, a count of ticks of 100 ns.
fn duration_of(time: FILETIME) -> Duration {
    let ticks = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Duration::from_nanos(ticks.saturating_mul(FILETIME_TICK_NANOS))
}

/// Writes the `resource usage` records, each with the CPU percent since the record before.
#[derive(Debug)]
pub(crate) struct ResourceLog {
    started: Instant,
    last_record: Instant,
    last_cpu_time: Duration,
}

impl ResourceLog {
    /// Starts the log now. The first record counts the CPU time from the start of the process.
    pub(crate) fn start() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            last_record: now,
            last_cpu_time: Duration::ZERO,
        }
    }

    /// Writes the usage of the process now, with the `kept` items of core, when the suggestion log
    /// is on.
    pub(crate) fn write(&mut self, kept: KeptItems) {
        if !tracing::enabled!(target: SUGGESTION_LOG_TARGET, tracing::Level::INFO) {
            return;
        }
        let Some(usage) = ProcessUsage::read() else {
            tracing::debug!("failed to read the resource usage");
            return;
        };
        let now = Instant::now();
        let cpu_time = usage.user_time + usage.kernel_time;
        let cpu_percent = cpu_percent(
            cpu_time.saturating_sub(self.last_cpu_time),
            now - self.last_record,
        );
        self.last_record = now;
        self.last_cpu_time = cpu_time;
        tracing::info!(
            target: SUGGESTION_LOG_TARGET,
            uptime_s = (now - self.started).as_secs(),
            private_mb = megabytes(usage.private),
            working_set_mb = megabytes(usage.working_set),
            peak_working_set_mb = megabytes(usage.peak_working_set),
            user_cpu_s = usage.user_time.as_secs_f64(),
            system_cpu_s = usage.kernel_time.as_secs_f64(),
            cpu_percent,
            history_entries = kept.history_entries,
            history_words = kept.history_words,
            dictionary_answers = kept.dictionary_answers,
            shown_suggestions = kept.shown_suggestions,
            conversation_lines = kept.conversation_lines,
            "resource usage"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_counts_ticks_of_100_ns_in_both_halves() {
        let times = [
            FILETIME {
                dwLowDateTime: 10_000_000,
                dwHighDateTime: 0,
            },
            FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 1,
            },
        ];

        let durations = times.map(duration_of);

        assert_eq!(
            durations,
            [Duration::from_secs(1), Duration::from_nanos(429_496_729_600)]
        );
    }
}
