//! Emits non-standard descriptors inherited across `exec`.
//!
//! The integration test uses this after XSH has initialized its network
//! runtime. It intentionally scans with `F_GETFD` instead of opening
//! `/proc/self/fd` or `/dev/fd`, which would create an observation descriptor
//! of its own.

fn main() {
    for fd in 3..1024 {
        let result = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if result != -1 {
            println!("{fd}");
        }
    }
}
