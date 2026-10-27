//! The physical cores of the CPU and their kinds.

use std::mem;
use std::num::NonZeroUsize;

use typealike_core::inference::CpuCores;
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, PROCESSOR_RELATIONSHIP, RelationProcessorCore,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};

const SIZE_OFFSET: usize = mem::offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Size);
const EFFICIENCY_CLASS_OFFSET: usize = mem::offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Anonymous)
    + mem::offset_of!(PROCESSOR_RELATIONSHIP, EfficiencyClass);

/// Returns the physical cores of the CPU of this computer, or `None` when Windows does not tell.
#[must_use]
pub fn cpu_cores() -> Option<CpuCores> {
    let records = core_records()
        .inspect_err(|error| tracing::warn!(%error, "failed to read the cores of the CPU"))
        .ok()?;
    cores_with(&efficiency_classes_in(&records))
}

/// Returns the records that Windows writes for the physical cores, one record for each core.
fn core_records() -> windows::core::Result<Vec<u8>> {
    let mut length = 0;
    // SAFETY: with no buffer, the call only writes the length that the records need.
    let measured = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &raw mut length) };
    if length == 0 {
        return measured.map(|()| Vec::new());
    }
    let byte_count = usize::try_from(length).expect("a u32 fits in usize");
    let mut buffer = vec![0_u64; byte_count.div_ceil(mem::size_of::<u64>())];
    // SAFETY: the buffer holds at least `length` bytes, aligned for the records.
    unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(buffer.as_mut_ptr().cast()),
            &raw mut length,
        )
    }?;
    let written = usize::try_from(length).expect("a u32 fits in usize");
    Ok(buffer
        .iter()
        .flat_map(|word| word.to_ne_bytes())
        .take(written)
        .collect())
}

/// Returns the efficiency class of each core in `records`, the records that Windows writes for the
/// physical cores. It ignores a record that the end of `records` cuts.
fn efficiency_classes_in(mut records: &[u8]) -> Vec<u8> {
    let mut classes = Vec::new();
    while let Some(size) = first_record_size(records) {
        let (record, rest) = records.split_at(size);
        classes.push(record[EFFICIENCY_CLASS_OFFSET]);
        records = rest;
    }
    classes
}

/// Returns the size of the first record in `records`, or `None` when `records` has no whole record.
fn first_record_size(records: &[u8]) -> Option<usize> {
    let size_bytes = records.get(SIZE_OFFSET..SIZE_OFFSET + mem::size_of::<u32>())?;
    let size = u32::from_ne_bytes(size_bytes.try_into().ok()?);
    let size = usize::try_from(size).ok()?;
    (size > EFFICIENCY_CLASS_OFFSET && size <= records.len()).then_some(size)
}

/// Returns the cores with `efficiency_classes`, one class for each core. The cores of the highest
/// class are the performance cores. Returns `None` with no cores.
fn cores_with(efficiency_classes: &[u8]) -> Option<CpuCores> {
    let highest = efficiency_classes.iter().max()?;
    let efficiency = efficiency_classes.iter().filter(|&class| class < highest).count();
    Some(CpuCores {
        performance: NonZeroUsize::new(efficiency_classes.len() - efficiency)?,
        efficiency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORE_RECORD_BYTES: usize = mem::offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Anonymous)
        + mem::size_of::<PROCESSOR_RELATIONSHIP>();

    fn core_record(efficiency_class: u8) -> Vec<u8> {
        let mut record = vec![0; CORE_RECORD_BYTES];
        let size = u32::try_from(CORE_RECORD_BYTES).expect("a record size fits in u32");
        record[..4].copy_from_slice(&RelationProcessorCore.0.to_ne_bytes());
        record[SIZE_OFFSET..SIZE_OFFSET + 4].copy_from_slice(&size.to_ne_bytes());
        record[EFFICIENCY_CLASS_OFFSET] = efficiency_class;
        record
    }

    fn cores(performance: usize, efficiency: usize) -> CpuCores {
        CpuCores {
            performance: NonZeroUsize::new(performance).expect("a test CPU has a performance core"),
            efficiency,
        }
    }

    #[test]
    fn each_core_record_gives_its_efficiency_class() {
        let records = [core_record(1), core_record(0), core_record(1)].concat();

        assert_eq!(efficiency_classes_in(&records), vec![1, 0, 1]);
    }

    #[test]
    fn record_that_the_end_cuts_is_ignored() {
        let records = [core_record(1), core_record(0)].concat();

        assert_eq!(efficiency_classes_in(&records[..records.len() - 1]), vec![1]);
    }

    #[test]
    fn cores_of_the_highest_class_are_the_performance_cores() {
        let classes: [&[u8]; 4] = [&[0, 0, 0, 0], &[1, 1, 0, 0, 0], &[0, 1, 2, 2], &[]];

        let found = classes.map(cores_with);

        assert_eq!(
            found,
            [Some(cores(4, 0)), Some(cores(2, 3)), Some(cores(2, 2)), None]
        );
    }
}
