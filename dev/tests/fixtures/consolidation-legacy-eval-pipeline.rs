#[derive(Clone, Debug)]
enum LoweredPipelineStage {
    TextLines,
    JsonLines,
    Where {
        slot: usize,
        predicate: BuildExprId,
    },
    WhereBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    Map {
        slot: usize,
        value: BuildExprId,
    },
    MapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    FlatMap {
        slot: usize,
        value: BuildExprId,
    },
    FlatMapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    BytesChunks {
        size: BuildExprId,
    },
    BatchCount {
        count: BuildExprId,
    },
    BatchMaxArgv {
        max_argv: Option<BuildExprId>,
    },
    BatchMaxBytes {
        max_bytes: BuildExprId,
    },
    BatchLimits {
        configuration: BuildExprId,
    },
    Shuffle {
        seed: Option<BuildExprId>,
    },
    Fold {
        acc_slot: usize,
        item_slot: usize,
        initial: BuildExprId,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    ReduceBy {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        op: ReduceByOp,
        jobs: Option<BuildExprId>,
    },
    ReduceByConfigured {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        configuration: BuildExprId,
    },
    ParMap {
        slot: usize,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    },
    ParMapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    },
    ParMapFlatMapReduceBy {
        slot: usize,
        body: Option<Vec<BuildStmtId>>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
        flatten: bool,
        reduce_item_slot: usize,
        reduce_body: Vec<BuildStmtId>,
        reduce_value: BuildExprId,
        op: ReduceByOp,
    },
    Tee {
        slot: usize,
        body: Vec<BuildStmtId>,
    },
    Each {
        slot: usize,
        body: Vec<BuildStmtId>,
    },
    TablePrint {
        columns: Option<Vec<String>>,
    },
    TablePrintConfigured {
        columns: BuildExprId,
    },
    Enumerate,
    Zip {
        other: BuildExprId,
    },
    Sort {
        descending: Option<BuildExprId>,
    },
    SortBy {
        slot: usize,
        key: BuildExprId,
        descending: Option<BuildExprId>,
    },
    GroupBy {
        slot: usize,
        key: BuildExprId,
    },
    CountBy {
        slot: usize,
        key: BuildExprId,
    },
    Any {
        slot: usize,
        predicate: BuildExprId,
    },
    AnyBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    All {
        slot: usize,
        predicate: BuildExprId,
    },
    AllBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    UniqueBy {
        slot: usize,
        key: BuildExprId,
    },
    Count,
    Sum,
    Collect,
    First,
    Last,
    Min,
    Max,
    Take(BuildExprId),
    Drop(BuildExprId),
    Repeat {
        count: BuildExprId,
    },
    Range {
        start: BuildExprId,
        end: BuildExprId,
    },
}
