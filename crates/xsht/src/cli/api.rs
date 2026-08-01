use crate::xsht::api::{self, ApiOptions};
use crate::xsht::cli::{CliOutput, text_bytes};

pub fn api_command(options: &ApiOptions) -> CliOutput {
    match api::query(options) {
        Ok(output) => CliOutput {
            status: output.status,
            stdout: text_bytes(output.stdout),
            stderr: Vec::new(),
            trace_text: String::new(),
            syscall_summary: None,
        },
        Err(error) => CliOutput {
            status: error.status,
            stdout: Vec::new(),
            stderr: text_bytes(format!("xsht api: {}\n", error.message)),
            trace_text: String::new(),
            syscall_summary: None,
        },
    }
}
