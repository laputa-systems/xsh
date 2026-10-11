use std::io;
use std::sync::{LazyLock, atomic::{AtomicU8, Ordering}};
use std::os::unix::process::CommandExt;

unsafe extern "C" {
    fn xsh_absent_standard_descriptors() -> libc::c_uint;
}

// Keep Rust's reservations open so source loading and unrelated file opens
// cannot accidentally acquire stdin, stdout, or stderr descriptor numbers.
static ABSENT: LazyLock<AtomicU8> = LazyLock::new(|| {
    AtomicU8::new(unsafe { xsh_absent_standard_descriptors() } as u8)
});

pub(crate) fn check(fd: i32) -> io::Result<()> {
    if (0..3).contains(&fd) && ABSENT.load(Ordering::Acquire) & (1 << fd) != 0 {
        Err(io::Error::from_raw_os_error(libc::EBADF))
    } else {
        Ok(())
    }
}

pub(crate) fn replaced(fd: i32) {
    if (0..3).contains(&fd) {
        ABSENT.fetch_and(!(1 << fd), Ordering::AcqRel);
    }
}

// Run before ordered child redirections: inherited reservations must be
// absent, while a later explicit redirection may reopen that standard stream.
pub(crate) fn inherit(command: &mut std::process::Command, inherited: u8) {
    let absent = ABSENT.load(Ordering::Acquire) & inherited;
    if absent == 0 { return; }
    // SAFETY: close is async-signal-safe and only addresses inherited standard
    // descriptors; no parent-owned descriptor is changed.
    unsafe {
        command.pre_exec(move || {
            for fd in 0..3 {
                if absent & (1 << fd) != 0 { libc::close(fd); }
            }
            Ok(())
        });
    }
}
