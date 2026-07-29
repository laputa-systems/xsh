use std::path::PathBuf;
use std::process::ExitCode;
use xsh::frontend_stats::{DEFAULT_ROOTS, measure_roots};
use xsh::mem_track::CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator::new();

const HELP: &str = "\
xsh-frontend-stats

Usage:
  xsh-frontend-stats [--json|--text] [ROOT ...]

With no roots, measures the frontend fixture corpus.
";

fn main() -> ExitCode {
    CountingAllocator::install_marker();
    let mut json = false;
    let mut roots = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "--text" => json = false,
            "--help" | "-h" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            _ if arg.starts_with('-') => {
                eprintln!("xsh-frontend-stats: unknown option `{arg}`");
                return ExitCode::from(2);
            }
            _ => roots.push(PathBuf::from(arg)),
        }
    }
    if roots.is_empty() {
        roots.extend(DEFAULT_ROOTS.iter().map(PathBuf::from));
    }

    match measure_roots(&roots) {
        Ok(stats) => {
            if json {
                print!("{}", stats.to_json());
            } else {
                print!("{}", stats.to_text());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xsh-frontend-stats: {error}");
            ExitCode::from(1)
        }
    }
}
