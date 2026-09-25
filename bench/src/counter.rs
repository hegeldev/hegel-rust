use std::mem;

const PERF_TYPE_HARDWARE: u32 = 0;
const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
const PERF_FLAG_FD_CLOEXEC: libc::c_ulong = 8;
const PERF_EVENT_IOC_ENABLE: libc::c_ulong = 0x2400;
const PERF_EVENT_IOC_DISABLE: libc::c_ulong = 0x2401;
const PERF_EVENT_IOC_RESET: libc::c_ulong = 0x2403;

const FLAG_DISABLED: u64 = 1 << 0;
const FLAG_INHERIT: u64 = 1 << 1;
const FLAG_EXCLUDE_KERNEL: u64 = 1 << 5;
const FLAG_EXCLUDE_HV: u64 = 1 << 6;

#[repr(C)]
#[derive(Default)]
struct PerfEventAttr {
    type_: u32,
    size: u32,
    config: u64,
    sample_period: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup_events: u32,
    bp_type: u32,
    config1: u64,
    config2: u64,
    branch_sample_type: u64,
    sample_regs_user: u64,
    sample_stack_user: u32,
    clockid: i32,
    sample_regs_intr: u64,
    aux_watermark: u32,
    sample_max_stack: u16,
    reserved_2: u16,
    aux_sample_size: u32,
    reserved_3: u32,
    sig_data: u64,
    config3: u64,
}

pub struct Instructions {
    fd: libc::c_int,
}

impl Instructions {
    pub fn new() -> Self {
        let attr = PerfEventAttr {
            type_: PERF_TYPE_HARDWARE,
            size: mem::size_of::<PerfEventAttr>() as u32,
            config: PERF_COUNT_HW_INSTRUCTIONS,
            flags: FLAG_DISABLED | FLAG_INHERIT | FLAG_EXCLUDE_KERNEL | FLAG_EXCLUDE_HV,
            ..Default::default()
        };
        let fd = unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                &attr as *const PerfEventAttr,
                0 as libc::pid_t,
                -1 as libc::c_int,
                -1 as libc::c_int,
                PERF_FLAG_FD_CLOEXEC,
            )
        } as libc::c_int;
        if fd < 0 {
            panic!(
                "perf_event_open(PERF_COUNT_HW_INSTRUCTIONS) failed: {} (this VM must expose hardware counters; check /proc/sys/kernel/perf_event_paranoid)",
                std::io::Error::last_os_error()
            );
        }
        Self { fd }
    }

    pub fn start(&mut self) {
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_RESET, 0);
            libc::ioctl(self.fd, PERF_EVENT_IOC_ENABLE, 0);
        }
    }

    pub fn stop(&mut self) -> u64 {
        let mut count: u64 = 0;
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_DISABLE, 0);
            let n = libc::read(self.fd, &mut count as *mut u64 as *mut libc::c_void, 8);
            assert_eq!(n, 8, "short read from perf counter");
        }
        count
    }
}

impl Drop for Instructions {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}
