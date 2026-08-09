#[path = "api.rs"]
pub mod api;
#[path = "app.rs"]
pub mod app;
#[path = "cli/mod.rs"]
pub mod cli;
#[path = "help.rs"]
pub(crate) mod help;
#[path = "config.rs"]
pub(crate) mod config;
#[path = "edit.rs"]
pub(crate) mod edit;
#[cfg(feature = "native-tests")]
pub mod examples;
#[path = "format.rs"]
pub mod format;
#[path = "grep.rs"]
pub mod grep;
#[path = "lint.rs"]
pub mod lint;
#[path = "table.rs"]
pub(crate) mod table;
#[cfg(feature = "native-tests")]
#[path = "xsht/test.rs"]
pub(crate) mod test;
#[path = "trace.rs"]
pub mod trace;
#[cfg(not(feature = "native-tests"))]
pub(crate) mod test {
    use crate::xsht::cli::CliOutput;

    #[allow(dead_code)]
    #[derive(Clone, Debug)]
    pub(crate) struct TestOptions {
        pub(crate) filter: Option<String>,
        pub(crate) native: bool,
        pub(crate) examples: bool,
        pub(crate) list: bool,
        pub(crate) exact: bool,
        pub(crate) nocapture: bool,
        pub(crate) fail_fast: bool,
        pub(crate) keep_temp: bool,
        pub(crate) jobs: Option<usize>,
        pub(crate) coverage: bool,
        pub(crate) api: bool,
        pub(crate) coverage_json_out: Option<String>,
    }

    pub(crate) fn test_scripts(_: TestOptions) -> CliOutput {
        CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: b"xsht test requires a build with the native-tests feature\n".to_vec(),
            trace_text: String::new(),
            syscall_summary: None,
        }
    }
}
