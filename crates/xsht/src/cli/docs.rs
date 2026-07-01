use crate::xsht::cli::{CliOutput, text_bytes};
use crate::xsht::docs;

#[cfg(feature = "docs-html")]
pub fn docs_command(command: &str) -> CliOutput {
    let result = match command {
        "build" => docs::build("."),
        "check" => docs::check("."),
        _ => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht docs: unknown command '{command}'\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };

    match result {
        Ok(_) => CliOutput::success(),
        Err(message) => CliOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: text_bytes(format!("{message}\n")),
            trace_text: String::new(),
            syscall_summary: None,
        },
    }
}

#[cfg(not(feature = "docs-html"))]
pub fn docs_command(_command: &str) -> CliOutput {
    CliOutput {
        status: 2,
        stdout: Vec::new(),
        stderr: text_bytes(
            "xsht docs requires the `docs-html` feature; rebuild with `--features docs-html`\n",
        ),
        trace_text: String::new(),
        syscall_summary: None,
    }
}
