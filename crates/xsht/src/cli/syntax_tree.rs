use crate::xsht::cli::{CliOutput, load_config, parse_script_with_module_roots, text_bytes};
use std::path::PathBuf;
use xsh::diagnostic::DiagnosticRenderer;

pub fn ast_script(script: &str) -> CliOutput {
    let config = match load_config() {
        Ok(config) => config,
        Err(message) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht: {message}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };
    let module_roots: Vec<PathBuf> = config.module_path.iter().map(PathBuf::from).collect();
    let (sources, parsed) = match parse_script_with_module_roots(script, &module_roots) {
        Ok(parsed) => parsed,
        Err(err) => {
            return CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: text_bytes(format!("xsht: failed to read '{script}': {err}\n")),
                trace_text: String::new(),
                syscall_summary: None,
            };
        }
    };

    if !parsed.diagnostics.is_empty() {
        return CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: text_bytes(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources)),
            trace_text: String::new(),
            syscall_summary: None,
        };
    }

    CliOutput {
        status: 0,
        stdout: text_bytes(format!("{:#?}\n", parsed.arena)),
        stderr: Vec::new(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}
