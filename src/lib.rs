extern crate self as xsh;

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[path = "entrypoints/xsh.rs"]
pub mod app;
pub mod diagnostic;
pub mod frontend_stats;
#[path = "runtime/eval/modules/host.rs"]
pub mod host;
pub mod loader;
pub mod mem_track;
pub mod modules;
pub mod runner;
pub mod runtime;
pub use loader::parse_script_with_module_roots;
pub mod sema;
pub mod source;
pub mod symbol;
pub mod syntax;
pub(crate) mod terminal;
pub mod trace;
