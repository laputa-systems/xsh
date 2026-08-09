pub mod xsht;

#[cfg(feature = "native-tests")]
#[path = "xsht/examples.rs"]
pub mod examples;

pub use xsht::cli::{
    AnnotationPolicy, AnnotationSelection, CliOutput, CoverageCollector, TraceFormat, TraceOptions,
    api_command, ast_script, check_paths_with_options, check_script, check_script_with_options,
    collect_configured_xsh_files, collect_xsh_files, format_files, grep_scripts, lint_files,
    load_config, refactor_scripts, trace_script,
};
pub use xsht::{api, app, cli, format, grep, lint, trace};
