//! Fixed configuration parameters accepted by structured stream stages.

/// Available CPU count is bounded by this default worker limit.
pub const DEFAULT_PAR_MAP_WORKERS: usize = 6;

/// These unary bodies can also be supplied as a statically resolved named call.
/// The callable is a per-item descriptor, separate from fixed configuration.
pub fn stage_accepts_callable(stage: &str) -> bool {
    matches!(stage, "map" | "where" | "flat-map" | "each" | "tee" | "sort-by"
        | "group-by" | "unique-by" | "any" | "all")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageParameterType {
    Int,
    Bool,
    Value,
    /// List or Stream; the item type participates in the stage's result type.
    Sequence,
    /// List of column names, each a Str.
    Columns,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageParameterDefault {
    Required,
    Absent,
    False,
    WorkerLimit,
    Serial,
    Random,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageParameterValidation {
    None,
    Positive,
    Nonnegative,
    /// Exactly one of the reduction mode parameters must be true.
    ReductionMode,
    /// A true value enables the platform argv budget; some batch limit is required.
    BatchLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageParameter {
    pub name: &'static str,
    pub ty: StageParameterType,
    pub default: StageParameterDefault,
    pub validation: StageParameterValidation,
    pub positional: bool,
}

const fn parameter(name: &'static str, ty: StageParameterType, default: StageParameterDefault, validation: StageParameterValidation, positional: bool) -> StageParameter {
    StageParameter { name, ty, default, validation, positional }
}

/// Configuration is evaluated once when execution reaches the stage. Projection
/// expressions and parameterized bodies retain their per-item evaluation role.
pub fn stage_parameters(stage: &str) -> &'static [StageParameter] {
    use StageParameterDefault::{Absent, False, Random, Required, Serial, WorkerLimit};
    use StageParameterType::{Bool, Columns, Int, Sequence, Value};
    use StageParameterValidation::{BatchLimit, None, Nonnegative, Positive, ReductionMode};
    match stage {
        "par-map" => const { &[parameter("jobs", Int, WorkerLimit, Positive, false)] },
        "sort" | "sort-by" => const { &[parameter("desc", Bool, False, None, false)] },
        "batch" => const { &[
            parameter("count", Int, Absent, Positive, false),
            parameter("max_bytes", Int, Absent, Positive, false),
            parameter("max_argv", Bool, False, BatchLimit, false),
        ] },
        "reduce-by" => const { &[
            parameter("sum", Bool, False, ReductionMode, false),
            parameter("min", Bool, False, ReductionMode, false),
            parameter("max", Bool, False, ReductionMode, false),
            parameter("jobs", Int, Serial, Positive, false),
        ] },
        "take" | "drop" | "repeat" => const { &[parameter("count", Int, Required, Nonnegative, true)] },
        "range" => const { &[
            parameter("start", Int, Required, None, true),
            parameter("end", Int, Required, None, true),
        ] },
        "bytes.chunks" => const { &[parameter("size", Int, Required, Positive, true)] },
        "zip" => const { &[parameter("other", Sequence, Required, None, true)] },
        "fold" | "reduce" => const { &[parameter("init", Value, Required, None, true)] },
        "shuffle" => const { &[parameter("seed", Int, Random, None, true)] },
        "table.print" => const { &[parameter("columns", Columns, Absent, None, true)] },
        _ => &[],
    }
}
