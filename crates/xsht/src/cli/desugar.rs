use crate::xsht::cli::{CliOutput, XshConfig, text_bytes};
use crate::xsht::config::config_for_file;
use crate::xsht::format::Formatter;
use xsh::diagnostic::DiagnosticRenderer;
use xsh::frontend::source::SourceMap;

/// Prints one script with every sugar statement replaced by its expansion
/// into core forms.
///
/// The file is parsed but not checked, so the output checks exactly when the
/// input does. Nothing is written to the file.
pub fn desugar_script(script: &str) -> CliOutput {
    let failure = |status: u8, stderr: String| CliOutput {
        status,
        stdout: Vec::new(),
        stderr: text_bytes(stderr),
        trace_text: String::new(),
        syscall_summary: None,
    };
    let config = match config_for_file(script) {
        Ok(config) => config,
        Err(message) => return failure(2, format!("xsht: {message}\n")),
    };
    let source = match std::fs::read_to_string(script) {
        Ok(source) => source,
        Err(err) => return failure(2, format!("xsht: failed to read '{script}': {err}\n")),
    };
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(script, source.as_str());
    let output = Formatter::new()
        .with_line_width(config.line_width())
        .desugar_source(source_id, &source);
    if !output.diagnostics.is_empty() {
        return failure(
            1,
            DiagnosticRenderer::new().render(&output.diagnostics, &sources),
        );
    }
    CliOutput {
        status: 0,
        stdout: text_bytes(output.formatted),
        stderr: Vec::new(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}
