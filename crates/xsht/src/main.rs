use std::process::ExitCode;
use xsh::mem_track::CountingAllocator;

// The statistics commands report allocation traffic, which only the global
// allocator can observe, so the counter wraps the allocator every command
// uses. It stays off until one of those commands turns it on.
#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: CountingAllocator<mimalloc::MiMalloc> = CountingAllocator::new(mimalloc::MiMalloc);

#[cfg(not(target_os = "linux"))]
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new(std::alloc::System);

mod stats;
mod xsht;

fn main() -> ExitCode {
    xsh::execution::script::on_preparation_stack(|| match stats::run() {
        Some(status) => status,
        None => xsht::app::main(),
    })
}
