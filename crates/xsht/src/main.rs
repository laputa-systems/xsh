use std::process::ExitCode;

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod xsht;

fn main() -> ExitCode {
    xsh::execution::script::on_preparation_stack(xsht::app::main)
}
