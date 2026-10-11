use super::{
    Arc, AssignOp, BinaryOp, BorrowCodec, BuildBoolId, BuildExprId, BuildIntId, BuildPatternId,
    BuildStmtId, DecodeContext, DurationValue, EFFECT_CANCELLATION, EFFECT_CWD, EFFECT_DEFER,
    EFFECT_DYNAMIC_CALL, EFFECT_ENV, EFFECT_HOST, EFFECT_PROCESS, EFFECT_PROPAGATE, EFFECT_TRACE,
    FloatValue, FullBuilder, FullCodec, FullCursor, FullDecoder, FullExecution, FullPayload,
    FunctionName, FxHashMap, IrBuildError, IrVerifyError, LoweredAssignPath, LoweredCallArg,
    LoweredCompQualifiers, LoweredCompTarget, LoweredErrorExpr, LoweredFmtPart,
    LoweredFunctionKey, LoweredPipelineStage, LoweredProcessCommandArgv,
    LoweredProcessCommandBuilderEntry, LoweredRecordEntry, LoweredRecordUpdates,
    LoweredRunCapture, LoweredRunEnv, LoweredRunPipelineSegment, LoweredSpawnRun,
    LoweredStrPredicate, LoweredType, LoweredTypeCheck, LoweredValue, Name, NameText, PathValue,
    PreparedConstantValue, QualifiedName, RegexValue, RuntimeOp, ScanBytes, ScanCheck, Span,
    Type, verify_node_metadata,
};

macro_rules! ignore_field {
    ($field:ident) => { _ };
}

macro_rules! instruction_schema {
    (
        $(
            $row:ident => $view:ident [$kind:ident] {
                $(
                    $(#[$attr:meta])*
                    $variant:ident
                    $(($($tuple_field:ident: $tuple_ty:ty),* $(,)?))?
                    $({$($(#[$field_attr:meta])* $field:ident: $field_ty:ty),* $(,)?})?
                    => $tag:ident = $ordinal:literal [$effects:expr];
                )*
            }
        )*
    ) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub(in crate::runtime::eval) enum FullTag {
            $($($tag = $ordinal,)*)*
        }

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(in crate::runtime::eval) enum InstructionKind {
            $($kind,)*
        }

        impl FullTag {
            pub(in crate::runtime::eval) fn field_cursor<'a>(
                self, context: DecodeContext<'a>, mut input: FullCursor<'a>, name: &str,
            ) -> Result<FullCursor<'a>, IrVerifyError> {
                match self {
                    $($(
                        Self::$tag => {
                            $($(
                                if name == stringify!($tuple_field) { return Ok(input); }
                                <$tuple_ty>::borrow(context, &mut input)?;
                            )*)?
                            $($(
                                if name == stringify!($field) { return Ok(input); }
                                <$field_ty>::borrow(context, &mut input)?;
                            )*)?
                        }
                    )*)*
                }
                Err(IrVerifyError::new("instruction field does not exist"))
            }

            pub(in crate::runtime::eval) fn kind(self) -> InstructionKind {
                match self {
                    $($(Self::$tag => InstructionKind::$kind,)*)*
                }
            }

            pub(in crate::runtime::eval) fn effects(self) -> u32 {
                match self {
                    $($(Self::$tag => $effects,)*)*
                }
            }

            pub(in crate::runtime::eval) fn name(self) -> &'static str {
                match self {
                    $($(Self::$tag => stringify!($tag),)*)*
                }
            }
        }

        pub(in crate::runtime::eval) const INSTRUCTION_TAGS: &[FullTag] = &[
            $($(FullTag::$tag,)*)*
        ];

        $(
            #[derive(Clone, Debug)]
            pub(in crate::runtime::eval) enum $row {
                $(
                    $(#[$attr])*
                    $variant
                    $(($($tuple_ty),*))?
                    $({$($(#[$field_attr])* $field: $field_ty),*})?,
                )*
            }

            pub(in crate::runtime::eval) enum $view<'a> {
                $(
                    $variant
                    $(($(<$tuple_ty as BorrowCodec>::View<'a>),*))?
                    $({$($field: <$field_ty as BorrowCodec>::View<'a>),*})?,
                )*
            }

            impl<'a> $view<'a> {
                pub(in crate::runtime::eval) fn decode(
                    execution: &FullExecution<'a>,
                    tag: FullTag,
                    mut payload: FullPayload<'a>,
                ) -> Result<Self, IrVerifyError> {
                    let context = DecodeContext::new(execution);
                    let value = match tag {
                        $(
                            FullTag::$tag => {
                                $($(
                                    let $tuple_field = <$tuple_ty>::borrow(context, &mut payload.cursor)?;
                                )*)?
                                $($(
                                    let $field = <$field_ty>::borrow(context, &mut payload.cursor)?;
                                )*)?
                                Self::$variant
                                    $(($($tuple_field),*))?
                                    $({$($field),*})?
                            }
                        )*
                        _ => return Err(IrVerifyError::new("full IR instruction tag has the wrong category")),
                    };
                    payload.finish()?;
                    Ok(value)
                }
            }

            impl FullCodec for $row {
                fn encode(
                    &self,
                    builder: &mut FullBuilder,
                    output: &mut Vec<u32>,
                ) -> Result<(), IrBuildError> {
                    // A row encodes the rows it refers to, so this function is on
                    // the stack once for every level of nesting in a lowered body.
                    // Each variant encodes its fields in a function of its own:
                    // an unoptimized build gives every temporary of every arm a
                    // separate stack slot, and with the arms inline one level of
                    // nesting cost tens of kilobytes of stack.
                    match self {
                        $(
                            Self::$variant $(($(ignore_field!($tuple_field)),*))? $({$($field: _),*})? => {
                                fn encode_variant(
                                    row: &$row,
                                    builder: &mut FullBuilder,
                                    output: &mut Vec<u32>,
                                ) -> Result<(), IrBuildError> {
                                    let $row::$variant $(($($tuple_field),*))? $({$($field),*})? = row else {
                                        unreachable!("the caller matched this variant")
                                    };
                                    #[allow(unused_mut)]
                                    let mut payload = builder.take_payload();
                                    $($($tuple_field.encode(builder, &mut payload)?;)*)?
                                    $($($field.encode(builder, &mut payload)?;)*)?
                                    let instruction = builder.push_instruction(FullTag::$tag, &payload);
                                    builder.recycle_payload(payload);
                                    output.push(instruction?);
                                    Ok(())
                                }
                                encode_variant(self, builder, output)
                            }
                        )*
                    }
                }

                fn decode(
                    _decoder: &FullDecoder<'_>,
                    _input: &mut FullCursor<'_>,
                ) -> Result<Self, IrVerifyError> {
                    Err(IrVerifyError::new("indexed construction rows cannot be decoded from executable IR"))
                }

                fn verify(
                    decoder: &FullDecoder<'_>,
                    input: &mut FullCursor<'_>,
                ) -> Result<(), IrVerifyError> {
                    let (instruction, tag, mut payload) = decoder.instruction(input)?;
                    verify_node_metadata(decoder, tag, payload)?;
                    // Verification follows the rows an instruction refers to, so
                    // this function is on the stack once for every level of
                    // nesting in a lowered body. Each tag verifies its fields in
                    // a function of its own, which keeps the temporaries of every
                    // other tag out of this frame in an unoptimized build.
                    match tag {
                        $(
                            FullTag::$tag => {
                                fn verify_variant(
                                    _decoder: &FullDecoder<'_>,
                                    _payload: &mut FullCursor<'_>,
                                ) -> Result<(), IrVerifyError> {
                                    $($(<$tuple_ty>::verify(_decoder, _payload)?;)*)?
                                    $($(<$field_ty>::verify(_decoder, _payload)?;)*)?
                                    Ok(())
                                }
                                verify_variant(decoder, &mut payload)
                            }
                        )*
                        _ => Err(IrVerifyError::new("full IR instruction tag has the wrong category")),
                    }?;
                    payload.finish()?;
                    if tag == FullTag::ExprRetry {
                        let payload = decoder.cursor(decoder.store.payload(decoder.store.data[instruction].range())?);
                        decoder.verify_retry_shape(payload)?;
                    }
                    if tag == FullTag::StmtGuard {
                        let payload = decoder.cursor(decoder.store.payload(decoder.store.data[instruction].range())?);
                        decoder.verify_guard_shape(payload)?;
                    }
                    decoder.finish_instruction(instruction);
                    Ok(())
                }
            }
        )*
    };
}

instruction_schema! {
    BuildIntRow => IntView [Int] {
        Int(value: i64) => IntInt = 0 [0];
        Slot(slot: usize) => IntSlot = 1 [0];
        Binary {
            op: BinaryOp,
            left: BuildIntId,
            right: BuildIntId,
        } => IntBinary = 2 [0];
        StrByteLenSlot {
            slot: usize,
            span: Span,
        } => IntStrByteLenSlot = 3 [0];
        StrCountLinesSlot {
            slot: usize,
            span: Span,
        } => IntStrCountLinesSlot = 4 [0];
        StrByteAtSlot {
            slot: usize,
            index: BuildIntId,
            default: Option<BuildIntId>,
            span: Span,
        } => IntStrByteAtSlot = 5 [0];
    }
    BuildBoolRow => BoolView [Bool] {
        Bool(value: bool) => BoolBool = 6 [0];
        Slot(slot: usize) => BoolSlot = 7 [0];
        Not(value: BuildBoolId) => BoolNot = 8 [0];
        And(left: BuildBoolId, right: BuildBoolId) => BoolAnd = 9 [0];
        Or(left: BuildBoolId, right: BuildBoolId) => BoolOr = 10 [0];
        IntCompare {
            op: BinaryOp,
            left: BuildIntId,
            right: BuildIntId,
        } => BoolIntCompare = 11 [0];
        StrPredicateSlot {
            slot: usize,
            predicate: LoweredStrPredicate,
            needle: Arc<[u8]>,
            span: Span,
        } => BoolStrPredicateSlot = 12 [0];
        ContainsSlot {
            slot: usize,
            needle: LoweredValue,
            span: Span,
        } => BoolContainsSlot = 13 [0];
        StrContainsSlot {
            slot: usize,
            needle: Arc<str>,
            span: Span,
        } => BoolStrContainsSlot = 14 [0];
        TrimEmptySlot {
            slot: usize,
            span: Span,
        } => BoolTrimEmptySlot = 15 [0];
        TrimStrPredicateSlot {
            slot: usize,
            predicate: LoweredStrPredicate,
            needle: Arc<[u8]>,
            span: Span,
        } => BoolTrimStrPredicateSlot = 16 [0];
        LiteralCompareSlot {
            op: BinaryOp,
            slot: usize,
            value: LoweredValue,
        } => BoolLiteralCompareSlot = 17 [0];
    }
    BuildExprRow => ExprView [Expr] {
        Null => ExprNull = 18 [0];
        Unit => ExprUnit = 19 [0];
        Int(value: i64) => ExprInt = 20 [0];
        Float(value: FloatValue) => ExprFloat = 21 [0];
        Duration(value: DurationValue) => ExprDuration = 22 [0];
        Bool(value: bool) => ExprBool = 23 [0];
        Str(value: Arc<str>) => ExprStr = 24 [0];
        Bytes(value: Arc<[u8]>) => ExprBytes = 25 [0];
        PreparedRegex(value: RegexValue) => ExprPreparedRegex = 26 [0];
        PreparedConstant(value: PreparedConstantValue) => ExprPreparedConstant = 27 [0];
        Path(value: PathValue) => ExprPath = 28 [0];
        FunctionRef {
            function: FunctionName,
            pure: bool,
        } => ExprFunctionRef = 29 [0];
        PathFrom {
            value: BuildExprId,
            span: Span,
        } => ExprPathFrom = 30 [0];
        Param(slot: usize) => ExprParam = 31 [0];
        ComparisonChain {
            pairs: Vec<BuildExprId>,
            assertion: bool,
        } => ExprComparisonChain = 32 [0];
        Binary {
            op: BinaryOp,
            left: BuildExprId,
            right: BuildExprId,
            span: Span,
        } => ExprBinary = 33 [0];
        IfExpr {
            branches: Vec<(BuildExprId, BuildExprId)>,
            else_value: BuildExprId,
            span: Span,
        } => ExprIf = 34 [0];
        PatternIf {
            branches: Vec<(BuildExprId, BuildExprId, Vec<usize>)>,
            else_value: BuildExprId,
            span: Span,
        } => ExprPatternIf = 35 [0];
        MatchExpr {
            value: BuildExprId,
            arms: Vec<(BuildPatternId, Option<BuildExprId>, BuildExprId)>,
            span: Span,
        } => ExprMatch = 36 [0];
        StrMatchExpr {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, BuildExprId>,
            fallback: Option<BuildExprId>,
            span: Span,
        } => ExprStrMatch = 37 [0];
        TagMatchExpr {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, BuildExprId>,
            fallback: Option<BuildExprId>,
            span: Span,
        } => ExprTagMatch = 38 [0];
        ResultFallback {
            left: BuildExprId,
            right: BuildExprId,
        } => ExprResultFallback = 39 [0];
        FmtString(parts: Vec<LoweredFmtPart>) => ExprFmtString = 40 [0];
        PathFmtString {
            parts: Vec<LoweredFmtPart>,
            span: Span,
        } => ExprPathFmtString = 41 [0];
        // A `g"..."` glob literal. Expanded against the current cwd at eval time into
        // a `List[Path]` (mirrors the recursive evaluator's `eval_glob`).
        Glob {
            pattern: Arc<str>,
            span: Span,
        } => ExprGlob = 42 [0];
        // The `$?` last-process-status expression. Reads the evaluator's tracked
        // `last_status` at eval time, erroring if no process has run yet.
        LastStatus {
            span: Span,
        } => ExprLastStatus = 43 [0];
        Record(entries: Vec<LoweredRecordEntry>) => ExprRecord = 44 [0];
        // A missing key identifies a Map spread; keyed entries evaluate key before value.
        MapLiteral(entries: Vec<(Option<BuildExprId>, BuildExprId, Span)>) => ExprMapLiteral = 45 [0];
        RecordUpdate {
            base: BuildExprId,
            updates: LoweredRecordUpdates,
            span: Span,
        } => ExprRecordUpdate = 46 [0];
        List(values: Vec<BuildExprId>) => ExprList = 47 [0];
        // Each element records whether it splices, its value, and its source span.
        ListBuild(values: Vec<(bool, BuildExprId, Span)>) => ExprListBuild = 48 [0];
        // The `map.empty()` builtin constructor (empty list literals already lower via `List`).
        EmptyMap => ExprEmptyMap = 49 [0];
        // The `bytes.concat(<List[Bytes]>)` builtin constructor.
        BytesConcat {
            arg: BuildExprId,
            span: Span,
        } => ExprBytesConcat = 50 [0];
        Range {
            start: BuildExprId,
            end: BuildExprId,
            span: Span,
        } => ExprRange = 51 [0];
        Tag {
            type_name: Name,
            name: Arc<str>,
            fields: Vec<BuildExprId>,
            wire: Option<Arc<crate::sema::wire_enums::WireEnumMapping>>,
        } => ExprTag = 52 [0];
        ListComp {
            value: BuildExprId,
            qualifiers: LoweredCompQualifiers,
            span: Span,
        } => ExprListComp = 53 [0];
        MapComp {
            key: BuildExprId,
            value: BuildExprId,
            qualifiers: LoweredCompQualifiers,
            span: Span,
        } => ExprMapComp = 54 [0];
        ListPipeline {
            input: BuildExprId,
            stages: Vec<LoweredPipelineStage>,
            span: Span,
        } => ExprPipeline = 55 [0];
        Field {
            base: BuildExprId,
            // Scratch rows preserve generated static text without allocation while
            // retaining an owned spelling for dynamic names.
            name: NameText,
            span: Span,
        } => ExprField = 56 [0];
        Index {
            base: BuildExprId,
            index: BuildExprId,
            span: Span,
        } => ExprIndex = 57 [0];
        /// `base[-distance]` over a list, where the checker saw the negative
        /// literal.
        IndexFromEnd {
            base: BuildExprId,
            distance: crate::runtime::eval::EndDistance,
            span: Span,
        } => ExprIndexFromEnd = 58 [0];
        Slice {
            base: BuildExprId,
            start: Option<BuildExprId>,
            end: Option<BuildExprId>,
            span: Span,
        } => ExprSlice = 59 [0];
        Method {
            receiver: BuildExprId,
            name: NameText,
            args: Vec<BuildExprId>,
            span: Span,
        } => ExprMethod = 60 [0];
        StrByteLen {
            receiver: BuildExprId,
            span: Span,
        } => ExprStrByteLen = 61 [0];
        StrByteAt {
            receiver: BuildExprId,
            index: BuildExprId,
            span: Span,
        } => ExprStrByteAt = 62 [0];
        StrPredicate {
            receiver: BuildExprId,
            predicate: LoweredStrPredicate,
            needle: BuildExprId,
            span: Span,
        } => ExprStrPredicate = 63 [0];
        RegexCompile {
            pattern: BuildExprId,
            span: Span,
        } => ExprRegexCompile = 64 [0];
        Require {
            value: BuildExprId,
            check: LoweredTypeCheck,
            span: Span,
        } => ExprRequire = 65 [0];
        // Preserve a checked domain while retaining ordinary runtime failure semantics.
        CheckedValue {
            value: BuildExprId,
            check: LoweredTypeCheck,
            span: Span,
        } => ExprCheckedValue = 66 [0];
        // Boxed because captured and spawned runs are cold, unusually wide
        // payloads that would otherwise determine the size of every expression.
        RunCapture(value: Box<LoweredRunCapture>) => ExprRunCapture = 67 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE];
        RunPipeline {
            segments: Vec<LoweredRunPipelineSegment>,
            propagate: bool,
            span: Span,
        } => ExprRunPipeline = 68 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE];
        SpawnRun(value: Box<LoweredSpawnRun>) => ExprSpawnRun = 69 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE];
        SpawnCommand {
            command: BuildExprId,
            span: Span,
        } => ExprSpawnCommand = 70 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE];
        Wait {
            target: BuildExprId,
            span: Span,
        } => ExprWait = 71 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE];
        Capture {
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprCapture = 72 [0];
        ValueBlock {
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprValueBlock = 73 [0];
        ErrorContext {
            message: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprErrorContext = 74 [0];
        ContextScope {
            kind: crate::syntax::arena::ContextScopeKind,
            input: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprContextScope = 75 [EFFECT_CWD | EFFECT_ENV | EFFECT_HOST | EFFECT_TRACE];
        Loop {
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprLoop = 76 [EFFECT_CANCELLATION];
        Retry {
            delays: Vec<BuildExprId>,
            pattern: Option<BuildPatternId>,
            body: Vec<BuildStmtId>,
            span: Span,
            schedule: crate::syntax::arena::RetrySchedule,
        } => ExprRetry = 77 [0];
        FsFiles {
            root: BuildExprId,
            gitignore: Option<BuildExprId>,
            stat: Option<BuildExprId>,
            hidden: Option<BuildExprId>,
            exts: Option<BuildExprId>,
            result_wrapped: bool,
            span: Span,
        } => ExprFsFiles = 78 [EFFECT_PROPAGATE | EFFECT_TRACE];
        FsWalk {
            root: BuildExprId,
            gitignore: Option<BuildExprId>,
            stat: Option<BuildExprId>,
            hidden: Option<BuildExprId>,
            exts: Option<BuildExprId>,
            result_wrapped: bool,
            span: Span,
        } => ExprFsWalk = 79 [EFFECT_PROPAGATE | EFFECT_TRACE];
        FsList {
            op: RuntimeOp,
            path: BuildExprId,
            stat: Option<BuildExprId>,
            ordered: Option<BuildExprId>,
            span: Span,
        } => ExprFsList = 80 [EFFECT_PROPAGATE | EFFECT_TRACE];
        FsTempDir {
            span: Span,
        } => ExprFsTempDir = 81 [EFFECT_PROPAGATE | EFFECT_TRACE];
        FsMkdir {
            path: BuildExprId,
            parents: Option<BuildExprId>,
            span: Span,
        } => ExprFsMkdir = 82 [EFFECT_PROPAGATE | EFFECT_TRACE];
        FsRemove {
            path: BuildExprId,
            missing_ok: Option<BuildExprId>,
            span: Span,
        } => ExprFsRemove = 83 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathReadText {
            path: BuildExprId,
            span: Span,
        } => ExprPathReadText = 84 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathReadBytes {
            path: BuildExprId,
            span: Span,
        } => ExprPathReadBytes = 85 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathExists {
            path: BuildExprId,
            span: Span,
        } => ExprPathExists = 86 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathExecutable {
            path: BuildExprId,
            span: Span,
        } => ExprPathExecutable = 87 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathDu {
            path: BuildExprId,
            span: Span,
        } => ExprPathDu = 88 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathMetadata {
            path: BuildExprId,
            span: Span,
        } => ExprPathMetadata = 89 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathReadlink {
            path: BuildExprId,
            span: Span,
        } => ExprPathReadlink = 90 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathResolve {
            path: BuildExprId,
            span: Span,
        } => ExprPathResolve = 91 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathWrite {
            path: BuildExprId,
            data: BuildExprId,
            atomic: bool,
            span: Span,
        } => ExprPathWrite = 92 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathMkdir {
            path: BuildExprId,
            parents: Option<BuildExprId>,
            span: Span,
        } => ExprPathMkdir = 93 [EFFECT_PROPAGATE | EFFECT_TRACE];
        PathRemove {
            path: BuildExprId,
            missing_ok: Option<BuildExprId>,
            span: Span,
        } => ExprPathRemove = 94 [EFFECT_PROPAGATE | EFFECT_TRACE];
        JsonEncode {
            value: BuildExprId,
            span: Span,
        } => ExprJsonEncode = 95 [0];
        ArchiveTarCreate {
            path: BuildExprId,
            root: BuildExprId,
            entries: BuildExprId,
            compression: Option<BuildExprId>,
            overwrite: Option<BuildExprId>,
            span: Span,
        } => ExprArchiveTarCreate = 96 [EFFECT_PROPAGATE | EFFECT_TRACE];
        ArchiveTarList {
            path: BuildExprId,
            span: Span,
        } => ExprArchiveTarList = 97 [EFFECT_PROPAGATE | EFFECT_TRACE];
        ArchiveTarExtract {
            path: BuildExprId,
            dest: BuildExprId,
            span: Span,
        } => ExprArchiveTarExtract = 98 [EFFECT_PROPAGATE | EFFECT_TRACE];
        ModuleCall {
            op: RuntimeOp,
            call_plan: Option<Arc<crate::runtime::eval::ModuleCallPlan>>,
            args: Vec<Option<BuildExprId>>,
            span: Span,
        } => ExprModuleCall = 99 [EFFECT_HOST | EFFECT_TRACE];
        // Boxed because this cold command-construction payload otherwise makes
        // every `BuildExprId` 16 bytes larger, including expression-heavy scripts.
        ProcessCommandArgv(value: Box<LoweredProcessCommandArgv>) => ExprProcessCommandArgv = 100 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE];
        ProcessCommandBuilder {
            entries: Vec<LoweredProcessCommandBuilderEntry>,
            span: Span,
        } => ExprProcessCommandBuilder = 101 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE];
        Abort {
            status: BuildExprId,
            span: Span,
        } => ExprAbort = 102 [EFFECT_PROPAGATE | EFFECT_TRACE];
        Fail {
            message: BuildExprId,
            span: Span,
        } => ExprFail = 103 [EFFECT_PROPAGATE | EFFECT_TRACE];
        Ok(value: BuildExprId) => ExprOk = 104 [0];
        Err {
            value: BuildExprId,
            cause: Option<BuildExprId>,
        } => ExprErr = 105 [0];
        // Boxed: `LoweredErrorExpr::Structured` inlines two `String`s plus two
        // `Vec`s (~96 bytes) that would otherwise size every `BuildExprId` variant
        // for the sake of the comparatively rare structured-error-literal case.
        Error(value: Box<LoweredErrorExpr>) => ExprError = 106 [0];
        Try {
            value: BuildExprId,
            span: Span,
        } => ExprTry = 107 [EFFECT_PROPAGATE | EFFECT_TRACE];
        Call {
            function: LoweredFunctionKey,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprCall = 108 [EFFECT_TRACE];
        DirectPureCall {
            function: LoweredFunctionKey,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprDirectPureCall = 109 [EFFECT_TRACE];
        /// A call to an implementation a loading program published.
        ///
        /// Used only by a dynamically loaded module that links its standard-library
        /// calls to the implementations its loader already prepared, so the callee
        /// has no local function identity to encode.
        ExternalCall {
            function: QualifiedName,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprExternalCall = 110 [EFFECT_TRACE];
        DynamicCall {
            callee: BuildExprId,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprDynamicCall = 111 [EFFECT_DYNAMIC_CALL | EFFECT_TRACE];
        SelfCall {
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprSelfCall = 112 [EFFECT_TRACE];
        /// A call through a value of a callable type. `signature` is the checked
        /// `Type::Callable` the arguments were bound against, and `pure` its
        /// kind; `args` supplies each of its parameters once, in order.
        TypedCall {
            callee: BuildExprId,
            pure: bool,
            signature: Type,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprTypedCall = 113 [EFFECT_DYNAMIC_CALL | EFFECT_TRACE];
        /// What `Proc.call` returns for the call it wraps: the `Result` the proc
        /// returned, or `Ok` of any other value. A dynamic handle does not say
        /// what its proc returns, so the checker types the call `Result[Any]`
        /// and this makes every proc honor that.
        ProcCallResult(value: BuildExprId) => ExprProcCallResult = 114 [0];
    }
    BuildStmtRow => StmtView [Stmt] {
        Let {
            slot: usize,
            value: BuildExprId,
        } => StmtLet = 115 [0];
        /// `guard let slot = value else { |failure| ... }`: evaluate
        /// `value` (a `Result`); on `Ok`, bind its inner value to `slot` and
        /// continue; on `Err`, bind the error to `else_param_slot` (if present) and
        /// run `else_body`, which must diverge.
        ///
        /// With `optional`, `value` is an optional instead: `null` runs
        /// `else_body` and any other value is bound to `target` as it is. A null
        /// value carries no error, so an optional guard has no `else_param_slot`.
        Guard {
            target: LoweredCompTarget,
            value: BuildExprId,
            else_param_slot: Option<usize>,
            else_body: Vec<BuildStmtId>,
            span: Span,
            optional: bool,
        } => StmtGuard = 116 [EFFECT_PROPAGATE | EFFECT_TRACE];
        With {
            bindings: Vec<(usize, BuildExprId)>,
            body: Vec<BuildStmtId>,
            else_param_slot: Option<usize>,
            else_body: Vec<BuildStmtId>,
            captures: Vec<usize>,
            span: Span,
        } => StmtWith = 117 [EFFECT_PROPAGATE | EFFECT_TRACE];
        LetInt {
            slot: usize,
            value: BuildIntId,
        } => StmtLetInt = 118 [0];
        LetBool {
            slot: usize,
            value: BuildBoolId,
        } => StmtLetBool = 119 [0];
        Assign {
            slot: usize,
            op: AssignOp,
            value: BuildExprId,
            // Validate the final replacement before offering storage to consuming operations.
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => StmtAssign = 120 [0];
        AssignField {
            slot: usize,
            field: Arc<str>,
            op: AssignOp,
            value: BuildExprId,
            span: Span,
        } => StmtAssignField = 121 [0];
        AssignFieldInt {
            slot: usize,
            field: Arc<str>,
            op: AssignOp,
            value: BuildIntId,
            span: Span,
        } => StmtAssignFieldInt = 122 [0];
        AssignPath {
            slot: usize,
            path: LoweredAssignPath,
            op: AssignOp,
            value: BuildExprId,
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => StmtAssignPath = 123 [0];
        AssignInt {
            slot: usize,
            op: AssignOp,
            value: BuildIntId,
            span: Span,
        } => StmtAssignInt = 124 [0];
        AssignBool {
            slot: usize,
            value: BuildBoolId,
        } => StmtAssignBool = 125 [0];
        Value {
            value: BuildExprId,
        } => StmtValue = 126 [0];
        Assert {
            value: BuildExprId,
            message: Option<BuildExprId>,
            span: Span,
        } => StmtAssert = 127 [0];
        Expr {
            value: BuildExprId,
            span: Span,
        } => StmtExpr = 128 [0];
        If {
            branches: Vec<(BuildExprId, Vec<BuildStmtId>)>,
            else_body: Option<Vec<BuildStmtId>>,
        } => StmtIf = 129 [0];
        IfBool {
            branches: Vec<(BuildBoolId, Vec<BuildStmtId>)>,
            else_body: Option<Vec<BuildStmtId>>,
        } => StmtIfBool = 130 [0];
        While {
            condition: BuildExprId,
            body: Vec<BuildStmtId>,
        } => StmtWhile = 131 [EFFECT_CANCELLATION];
        WhileBool {
            condition: BuildBoolId,
            body: Vec<BuildStmtId>,
        } => StmtWhileBool = 132 [EFFECT_CANCELLATION];
        PatternIf {
            branches: Vec<(BuildExprId, Vec<BuildStmtId>, Vec<usize>)>,
            else_body: Option<Vec<BuildStmtId>>,
            span: Span,
        } => StmtPatternIf = 133 [EFFECT_CANCELLATION];
        PatternWhile {
            condition: BuildExprId,
            body: Vec<BuildStmtId>,
            captures: Vec<usize>,
            span: Span,
        } => StmtPatternWhile = 134 [EFFECT_CANCELLATION];
        Match {
            value: BuildExprId,
            arms: Vec<(BuildPatternId, Option<BuildExprId>, Vec<BuildStmtId>)>,
            span: Span,
        } => StmtMatch = 135 [0];
        StrMatch {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, Vec<BuildStmtId>>,
            fallback: Option<Vec<BuildStmtId>>,
            span: Span,
        } => StmtStrMatch = 136 [0];
        TagMatch {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, Vec<BuildStmtId>>,
            fallback: Option<Vec<BuildStmtId>>,
            span: Span,
        } => StmtTagMatch = 137 [0];
        For {
            slot: usize,
            iter: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => StmtFor = 138 [EFFECT_CANCELLATION];
        // Select a recursive record target into independent local values.
        LetRecord {
            source: BuildExprId,
            target: LoweredCompTarget,
            span: Span,
        } => StmtLetRecord = 139 [0];
        // Select the recursive record target anew for each iteration.
        ForRecord {
            target: LoweredCompTarget,
            iter: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => StmtForRecord = 140 [EFFECT_CANCELLATION];
        ForStrLines {
            slot: usize,
            text: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => StmtForStrLines = 141 [EFFECT_CANCELLATION];
        ScanLines {
            text_slot: usize,
            line_slot: usize,
            checks: Vec<ScanCheck>,
            span: Span,
        } => StmtScanLines = 142 [EFFECT_CANCELLATION];
        ScanBytes {
            config: ScanBytes,
        } => StmtScanBytes = 143 [EFFECT_CANCELLATION];
        Print {
            args: Vec<BuildExprId>,
            stderr: bool,
            flush: bool,
            propagate_result: bool,
            span: Span,
        } => StmtPrint = 144 [EFFECT_HOST | EFFECT_TRACE];
        Cd {
            target: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => StmtCd = 145 [EFFECT_CWD | EFFECT_HOST | EFFECT_TRACE];
        Env {
            env: Vec<LoweredRunEnv>,
            body: Vec<BuildStmtId>,
        } => StmtEnv = 146 [EFFECT_ENV | EFFECT_HOST | EFFECT_TRACE];
        Proc {
            // The registry identity is stable; resolve it once while lowering
            // instead of retaining two names and looking it up on every execution.
            op: RuntimeOp,
            args: Vec<BuildExprId>,
            propagate_result: bool,
            span: Span,
        } => StmtProc = 147 [EFFECT_HOST | EFFECT_TRACE];
        Run {
            value: BuildExprId,
            propagate_result: bool,
        } => StmtRun = 148 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE];
        Loop {
            body: Vec<BuildStmtId>,
        } => StmtLoop = 149 [EFFECT_CANCELLATION];
        Return {
            value: BuildExprId,
        } => StmtReturn = 150 [0];
        Yield {
            value: BuildExprId,
        } => StmtYield = 151 [0];
        YieldDelegate {
            value: BuildExprId,
            span: Span,
        } => StmtYieldDelegate = 152 [0];
        Break => StmtBreak = 153 [0];
        BreakValue {
            value: BuildExprId,
        } => StmtBreakValue = 154 [0];
        Continue => StmtContinue = 155 [0];
        Defer {
            value: BuildExprId,
            /// Whether this is an `errdefer`: the action runs only when its scope
            /// leaves with an error.
            on_error: bool,
        } => StmtDefer = 156 [EFFECT_DEFER | EFFECT_PROPAGATE | EFFECT_TRACE];
        DefaultParameter {
            slot: usize,
            value: BuildExprId,
            kind: LoweredType,
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => StmtDefaultParameter = 157 [0];
    }
}
