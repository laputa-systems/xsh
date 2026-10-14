//! The `libxsh` execution contract.
//!
//! `script` is the high-level execution API. `evaluator`, `value`, and
//! `process`-adjacent types are currently first-party APIs used by `xshi` and
//! `xsht`; they remain separate so session and runtime representation work is
//! not mistaken for the ordinary script-running contract.

pub mod evaluator {
    /// Reserved environment namespace for the private native-test route-witness side channel.
    #[cfg(feature = "native-tests")]
    #[doc(hidden)]
    pub const NATIVE_TEST_ROUTE_WITNESS_ENV_PREFIX: &str =
        crate::runtime::eval::route_witness::ENV_PREFIX;
    /// Reserved environment namespace for xsht's private per-invocation opt-in.
    #[cfg(feature = "native-tests")]
    #[doc(hidden)]
    pub const NATIVE_TEST_ROUTE_WITNESS_HARNESS_ENV_PREFIX: &str =
        crate::runtime::eval::route_witness::HARNESS_ENV_PREFIX;
    /// Hidden evaluator-overlay key carrying the exact outer harness test ID.
    #[cfg(feature = "native-tests")]
    #[doc(hidden)]
    pub const NATIVE_TEST_ROUTE_WITNESS_TEST_ID_OVERLAY: &[u8] =
        crate::runtime::eval::route_witness::INVOCATION_TEST_ID_OVERLAY;
    /// Hidden evaluator-overlay key carrying the current invocation output directory.
    #[cfg(feature = "native-tests")]
    #[doc(hidden)]
    pub const NATIVE_TEST_ROUTE_WITNESS_OUTPUT_DIR_OVERLAY: &[u8] =
        crate::runtime::eval::route_witness::INVOCATION_OUTPUT_DIR_OVERLAY;

    pub use crate::runtime::eval::{
        CompactLowerBodyProbeOutput, CompactLowerConstructProbeOutput,
        CompactRuntimeDeclProbeOutput, EvalFlow, EvalOutput, Evaluator, FrontendLoweredStats,
        InteractiveCommandContext, InteractiveCommandDispatcher, LoweredFunctionBlocker,
        LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit, Propagation, apply_question,
        probe_compact_lower_constructed_bodies, probe_compact_runtime_declarations,
    };

    #[cfg(feature = "native-tests")]
    pub use crate::runtime::eval::{
        LinuxFake, NativeTestHost, NativeTestRunKind, NativeTestRunRequest, PreparedTestProgram,
        TestCancellation, TestEvalOutput, UnixFake, NATIVE_TEST_FORCE_FRAMES_ENV,
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
        /// Native words, decoded only when the checked entry requires text.
        pub args: Vec<std::ffi::OsString>,
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

    pub use crate::runner::{
        on_preparation_stack, run_script, run_script_with_shared_stdio, run_startup,
        script_command_name,
    };

    #[cfg(feature = "native-tests")]
    pub use crate::runner::{
        PreparedBenchmarkScript, prepare_benchmark_script, run_script_with_test_fakes,
    };
}

pub mod value {
    pub use crate::runtime::value::{
        AbortSignal, CommandPlan, CommandRedirection, CommandRedirectionMode,
        CommandRedirectionStream, DigestValue, DurationValue, ErrorContext, FloatValue,
        FsEntryKind, FsEntryValue, FunctionName, MapKey, MapKeyRef, PathValue, ProcessHandleValue,
        RecordIter, RecordKeys, RecordMap, RecordShape, RecordShapeData, RecordValues, RegexValue,
        ResultValue, RunError, RuntimeError, RuntimeShapeStats, SparseRecordMap, StreamItem,
        StreamValue, Value, error_constructor, run_error_constructor, run_error_from_status,
        runtime_shape_stats, structured_error_constructor,
    };
}
