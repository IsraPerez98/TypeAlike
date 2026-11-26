//! The `resource usage` record of the suggestion log: the memory, the CPU time and the threads of
//! the process, the memory of the Metal device, and the counts of the items that core keeps. Only a
//! build with the `suggestion-log` feature has it.

use std::mem::{self, MaybeUninit};
use std::time::{Duration, Instant};

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use typealike_core::diagnostics::{CpuWindow, KeptItems, megabytes};
use typealike_core::{SUGGESTION_LOG_TARGET, SuggestionLog};

/// The time between two `resource usage` records.
pub(crate) const RECORD_INTERVAL: Duration = Duration::from_mins(1);

#[link(name = "Metal", kind = "framework")]
unsafe extern "C" {
    fn MTLCreateSystemDefaultDevice() -> *mut AnyObject;
}

/// The memory, the CPU time and the threads of the process at one time.
#[derive(Debug, Clone, Copy)]
struct ProcessUsage {
    /// The memory that Activity Monitor shows for the process (`phys_footprint`).
    footprint: u64,
    /// The highest footprint since the process started.
    peak_footprint: u64,
    resident: u64,
    user_time: Duration,
    system_time: Duration,
    threads: u32,
}

impl ProcessUsage {
    /// Reads the usage of this process, or `None` when macOS does not give it.
    fn read() -> Option<Self> {
        let memory = read_memory()?;
        let (user_time, system_time) = read_cpu_times()?;
        Some(Self {
            footprint: memory.ri_phys_footprint,
            peak_footprint: memory.ri_lifetime_max_phys_footprint,
            resident: memory.ri_resident_size,
            user_time,
            system_time,
            threads: read_thread_count()?,
        })
    }
}

fn read_memory() -> Option<libc::rusage_info_v4> {
    let mut info = MaybeUninit::<libc::rusage_info_v4>::zeroed();
    // SAFETY: the buffer is a `rusage_info_v4`, the struct that the flavor `RUSAGE_INFO_V4` fills.
    let status = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V4,
            info.as_mut_ptr().cast::<libc::rusage_info_t>(),
        )
    };
    // SAFETY: the call filled the buffer when it returned 0, and a zeroed struct is valid anyway.
    (status == 0).then(|| unsafe { info.assume_init() })
}

/// The user and the system CPU time of all the threads of the process since it started.
fn read_cpu_times() -> Option<(Duration, Duration)> {
    let mut usage = MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: the buffer is a `rusage`, which `getrusage` fills for `RUSAGE_SELF`.
    let status = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if status != 0 {
        return None;
    }
    // SAFETY: the call filled the buffer.
    let usage = unsafe { usage.assume_init() };
    Some((duration_of(usage.ru_utime)?, duration_of(usage.ru_stime)?))
}

fn duration_of(time: libc::timeval) -> Option<Duration> {
    let micros = u32::try_from(time.tv_usec).ok()?;
    Some(Duration::new(u64::try_from(time.tv_sec).ok()?, micros * 1000))
}

fn read_thread_count() -> Option<u32> {
    let mut info = MaybeUninit::<libc::proc_taskinfo>::zeroed();
    let size = i32::try_from(mem::size_of::<libc::proc_taskinfo>()).ok()?;
    // SAFETY: the buffer is a `proc_taskinfo` of `size` bytes, the struct of `PROC_PIDTASKINFO`.
    let written = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    // SAFETY: the call filled the whole buffer.
    u32::try_from(unsafe { info.assume_init() }.pti_threadnum).ok()
}

/// The bytes of all the buffers of the default Metal device, which llama.cpp also uses, or `None`
/// on a Mac without Metal.
fn metal_allocated_bytes() -> Option<u64> {
    // SAFETY: the function returns a retained device, or null on a Mac without Metal.
    let device = unsafe { Retained::from_raw(MTLCreateSystemDefaultDevice()) }?;
    // SAFETY: each `MTLDevice` has the property `currentAllocatedSize`, an `NSUInteger`.
    let bytes: usize = unsafe { msg_send![&*device, currentAllocatedSize] };
    u64::try_from(bytes).ok()
}

/// Writes the `resource usage` records, each but the first with the CPU percent since the record
/// before.
#[derive(Debug)]
pub(crate) struct ResourceLog {
    started: Instant,
    cpu: CpuWindow,
}

impl ResourceLog {
    /// Starts the log now.
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            cpu: CpuWindow::default(),
        }
    }

    /// Writes the usage of the process now, with the `kept` items of core, when the suggestion log
    /// is on.
    pub(crate) fn write(&mut self, kept: KeptItems) {
        if !SuggestionLog::of_run().is_on() {
            return;
        }
        let Some(usage) = ProcessUsage::read() else {
            tracing::debug!("failed to read the resource usage");
            return;
        };
        let uptime = self.started.elapsed();
        let cpu_percent = self.cpu.percent_at(uptime, usage.user_time + usage.system_time);
        tracing::info!(
            target: SUGGESTION_LOG_TARGET,
            uptime_s = uptime.as_secs(),
            footprint_mb = megabytes(usage.footprint),
            peak_footprint_mb = megabytes(usage.peak_footprint),
            resident_mb = megabytes(usage.resident),
            metal_mb = metal_allocated_bytes().map(megabytes),
            user_cpu_s = usage.user_time.as_secs_f64(),
            system_cpu_s = usage.system_time.as_secs_f64(),
            cpu_percent,
            threads = usage.threads,
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
    use std::hint::black_box;

    use super::*;

    const TOUCHED_BYTES: usize = 64 * 1024 * 1024;

    #[test]
    fn footprint_grows_by_the_memory_that_the_process_writes() {
        let before = ProcessUsage::read().expect("macOS gives the usage of the own process");

        let buffer = black_box(vec![1_u8; TOUCHED_BYTES]);
        let after = ProcessUsage::read().expect("macOS gives the usage of the own process");
        drop(buffer);

        let growth = after.footprint.saturating_sub(before.footprint);
        assert!(
            growth >= u64::try_from(TOUCHED_BYTES / 2).expect("fits"),
            "{growth}"
        );
    }

    #[test]
    fn usage_of_a_running_process_has_threads_and_a_peak_at_or_above_its_footprint() {
        let usage = ProcessUsage::read().expect("macOS gives the usage of the own process");

        assert!(
            usage.threads >= 1 && usage.peak_footprint >= usage.footprint && usage.resident > 0,
            "{usage:?}"
        );
    }
}
