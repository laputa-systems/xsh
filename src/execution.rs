//! The `libxsh` execution contract.
//!
//! `script` is the high-level execution API. `evaluator`, `value`, and
//! `process`-adjacent types are currently first-party APIs used by `xshi` and
//! `xsht`; they remain separate so session and runtime representation work is
//! not mistaken for the ordinary script-running contract.

pub mod evaluator {
    pub use crate::runtime::eval::{
        CompactLowerBodyProbeOutput, CompactLowerConstructProbeOutput,
        CompactRuntimeDeclProbeOutput, EvalFlow, EvalOutput, Evaluator, FrontendLoweredStats,
        InteractiveCommandContext, InteractiveCommandDispatcher, LoweredFunctionBlocker,
        LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit, Propagation, apply_question,
        probe_compact_lower_constructed_bodies, probe_compact_runtime_declarations,
    };

    #[cfg(feature = "native-tests")]
    pub use crate::runtime::eval::{
        NativeTestHost, NativeTestRunKind, NativeTestRunRequest, PreparedTestProgram,
        TestEvalOutput,
    };
}

pub mod script {
    use std::path::PathBuf;

    /// Environment variable used to collect nested `xsh` coverage traces.
    ///
    /// The trace file format is owned by `xsht`; this name is the small
    /// execution-side hook needed to ask the script runner to emit one.
    pub const XSH_COVERAGE_TRACE_DIR: &str = "XSH_COVERAGE_TRACE_DIR";

    /// Inputs for one ordinary, non-interactive script run.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct RunOptions {
        pub script: String,
        pub args: Vec<String>,
        pub coverage_trace_dir: Option<PathBuf>,
    }

    /// Captured result of an ordinary script run.
    ///
    /// `status` is the process-style result converted to the CLI's byte-sized
    /// exit representation; script failures remain represented in `stderr`
    /// rather than being collapsed into a Rust error.
    #[derive(Clone, Debug)]
    pub struct ScriptOutput {
        pub status: u8,
        pub stdout: Vec<u8>,
        pub stderr: Vec<u8>,
    }

    pub use crate::runner::{run_script, run_startup, script_command_name};

    #[cfg(feature = "native-tests")]
    pub use crate::runner::{PreparedBenchmarkScript, prepare_benchmark_script};
}

pub mod value {
    pub use crate::runtime::value::{
        AbortSignal, CommandPlan, CommandRedirection, CommandRedirectionMode,
        CommandRedirectionStream, DigestValue, DurationValue, ErrorContext, FloatValue,
        FsEntryKind, FsEntryValue, FunctionName, PathValue, ProcessHandleValue, RecordIter,
        RecordKeys, RecordMap, RecordShape, RecordShapeData, RecordValues, RegexValue, ResultValue,
        RunError, RuntimeError, RuntimeShapeStats, SparseRecordMap, StreamItem, StreamValue, Value,
        error_constructor, run_error_constructor, run_error_from_status, runtime_shape_stats,
        structured_error_constructor,
    };
}
