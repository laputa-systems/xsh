use std::path::Path;
use std::process::ExitCode;

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let argv0 = std::env::args_os().next().unwrap_or_default();
    let name = Path::new(&argv0)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("xsh-multicall");
    let name = name.strip_prefix('-').unwrap_or(name);

    match name {
        "xsh" => xsh::app::main(),
        "xshi" => xshi::app::main(),
        "xsht" => xsht::app::main(),
        _ => {
            eprintln!("xsh-multicall: invoke as xsh, xshi, or xsht");
            ExitCode::from(2)
        }
    }
}
