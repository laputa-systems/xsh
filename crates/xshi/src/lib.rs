//! xshi is disabled.
//!
//! The interactive shell implementation has been removed and no longer
//! compiles. This stub keeps the `xshi` crate — and therefore the `xshi`
//! binary target — buildable so workspace commands that still reference the
//! `xshi` package keep working. The original implementation remains in git
//! history.

use std::process::ExitCode;

/// Stub entry point kept so the `xshi` binary still compiles.
pub fn stub_main() -> ExitCode {
    eprintln!("xshi: disabled — the interactive shell is no longer available");
    ExitCode::FAILURE
}
