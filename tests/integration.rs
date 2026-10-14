#[macro_use]
#[path = "test_binary.rs"]
mod test_binary;
#[path = "cli.rs"]
mod cli;
#[path = "core.rs"]
mod core;
#[path = "diagnostics.rs"]
mod diagnostics;
#[path = "grammar.rs"]
mod grammar;
#[path = "libc_hygiene.rs"]
mod libc_hygiene;
#[path = "libxsh_api.rs"]
mod libxsh_api;
#[path = "runtime.rs"]
mod runtime;
#[path = "sema.rs"]
mod sema;
#[path = "sema_bundle_view.rs"]
mod sema_bundle_view;
#[path = "stdlib_port.rs"]
mod stdlib_port;
