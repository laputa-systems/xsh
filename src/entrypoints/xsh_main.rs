use std::process::ExitCode;

#[path = "xsh.rs"]
mod app;

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(target_os = "linux")]
unsafe extern "C" {
    safe fn xsh_set_allocator_purge_default();
}

fn main() -> ExitCode {
    #[cfg(target_os = "linux")]
    xsh_set_allocator_purge_default();
    app::main()
}
