use std::process::ExitCode;

#[path = "xsh.rs"]
mod app;

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    app::main()
}
