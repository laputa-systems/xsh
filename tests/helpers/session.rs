#![allow(clippy::single_call_fn)]

fn main() {
    let pid = unsafe { libc::getpid() };
    let sid = unsafe { libc::getsid(0) };
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, format!("{pid} {sid}\n")).expect("write session marker");
    } else {
        println!("{pid} {sid}");
    }
}
