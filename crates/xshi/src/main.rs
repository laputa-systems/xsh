use std::process::ExitCode;

#[cfg(feature = "laputa-asterinas")]
#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "laputa-asterinas")]
mod xshi;

#[cfg(feature = "laputa-asterinas")]
pub fn main() -> ExitCode {
    xshi::app::main()
}

#[cfg(not(feature = "laputa-asterinas"))]
pub fn main() -> ExitCode {
    xshi::stub_main()
}
