//! Process memory as the OS reports it: what Activity Monitor (macOS) or
//! Task Manager (Windows) shows. This is the number the 6 GB budget applies to.

/// Process memory figures in bytes. `None` where the OS can't tell us.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessMemory {
    /// macOS: physical footprint (includes GPU memory on Apple Silicon).
    /// Windows: working set. Linux: resident set size.
    pub current: Option<u64>,
    /// Peak of the same measure, where available.
    pub peak: Option<u64>,
}

pub fn process_memory() -> ProcessMemory {
    imp::process_memory()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::ProcessMemory;

    // From <mach/task_info.h>: the TASK_VM_INFO layout up to `phys_footprint`
    // (revision 1). The kernel fills as many revisions as `count` allows.
    #[repr(C)]
    #[derive(Default)]
    struct TaskVmInfo {
        virtual_size: u64,
        region_count: i32,
        page_size: i32,
        resident_size: u64,
        resident_size_peak: u64,
        device: u64,
        device_peak: u64,
        internal: u64,
        internal_peak: u64,
        external: u64,
        external_peak: u64,
        reusable: u64,
        reusable_peak: u64,
        purgeable_volatile_pmap: u64,
        purgeable_volatile_resident: u64,
        purgeable_volatile_virtual: u64,
        compressed: u64,
        compressed_peak: u64,
        compressed_lifetime: u64,
        phys_footprint: u64,
    }

    const TASK_VM_INFO: u32 = 22;

    unsafe extern "C" {
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: u32, info: *mut i32, count: *mut u32) -> i32;
    }

    pub fn process_memory() -> ProcessMemory {
        let mut info = TaskVmInfo::default();
        let mut count = (size_of::<TaskVmInfo>() / size_of::<u32>()) as u32;
        let kr = unsafe {
            task_info(
                mach_task_self_,
                TASK_VM_INFO,
                (&raw mut info).cast(),
                &mut count,
            )
        };
        if kr != 0 {
            return ProcessMemory::default();
        }
        ProcessMemory {
            current: Some(info.phys_footprint),
            peak: Some(info.resident_size_peak),
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::ProcessMemory;

    // PROCESS_MEMORY_COUNTERS from <psapi.h>.
    #[repr(C)]
    #[derive(Default)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    pub fn process_memory() -> ProcessMemory {
        let mut pmc = ProcessMemoryCounters {
            cb: size_of::<ProcessMemoryCounters>() as u32,
            ..Default::default()
        };
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) };
        if ok == 0 {
            return ProcessMemory::default();
        }
        ProcessMemory {
            current: Some(pmc.working_set_size as u64),
            peak: Some(pmc.peak_working_set_size as u64),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::ProcessMemory;

    fn status_kb(status: &str, key: &str) -> Option<u64> {
        let line = status.lines().find(|l| l.starts_with(key))?;
        line[key.len()..]
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse::<u64>()
            .ok()
            .map(|kb| kb * 1024)
    }

    pub fn process_memory() -> ProcessMemory {
        let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
            return ProcessMemory::default();
        };
        ProcessMemory {
            current: status_kb(&status, "VmRSS:"),
            peak: status_kb(&status, "VmHWM:"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_something_plausible() {
        let m = super::process_memory();
        let current = m
            .current
            .expect("process memory should be available on CI platforms");
        assert!(current > 100 * 1024, "{current}");
        assert!(current < 64 * 1024 * 1024 * 1024, "{current}");
    }
}
