use std::process::ExitCode;

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod xshi;

pub fn main() -> ExitCode {
    xshi::app::main()
}
