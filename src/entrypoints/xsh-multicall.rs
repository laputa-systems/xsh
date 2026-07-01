#![allow(clippy::single_call_fn)]

use std::path::Path;
use std::process::ExitCode;

#[path = "xsh.rs"]
mod xsh_entry;
#[path = "../../crates/xshi/src/xshi.rs"]
mod xshi;
#[path = "../../crates/xsht/src/xsht.rs"]
mod xsht;

fn main() -> ExitCode {
    let argv0 = std::env::args_os().next().unwrap_or_default();
    let name = Path::new(&argv0)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("xsh-multicall");

    let name = name.strip_prefix("-").unwrap_or(name);

    match name {
        "xsh" => xsh_entry::main(),
        "xshi" => xshi::app::main(),
        "xsht" => xsht::app::main(),
        _ => {
            eprintln!("xsh-multicall: invoke as xsh, xshi, or xsht");
            ExitCode::from(2)
        }
    }
}
