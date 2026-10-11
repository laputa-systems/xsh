use super::{
    Arc, AssignOp, BinaryOp, BorrowCodec, BuildBoolId, BuildExprId, BuildIntId, BuildPatternId,
    BuildPatternIdSlots, BuildPatternRow, BuildStmtId, DecodeContext, DurationValue,
    EFFECT_CANCELLATION, EFFECT_CWD, EFFECT_DEFER,
    EFFECT_DYNAMIC_CALL, EFFECT_ENV, EFFECT_HOST, EFFECT_PROCESS, EFFECT_PROPAGATE, EFFECT_TRACE,
    FloatValue, FullBuilder, FullCodec, FullCursor, FullDecoder, FullExecution, FullPayload,
    FunctionName, FxHashMap, IrBuildError, IrVerifyError, LoweredAssignPath, LoweredCallArg,
    LoweredCompQualifiers, LoweredCompTarget, LoweredErrorExpr, LoweredErrorPatternFields,
    LoweredFmtPart,
    LoweredFunctionKey, LoweredPipelineStage, LoweredProcessCommandArgv,
    LoweredProcessCommandBuilderEntry, LoweredRecordEntry, LoweredRecordUpdates,
    LoweredRunCapture, LoweredRunEnv, LoweredRunPipelineSegment, LoweredSpawnRun,
    LoweredStrPredicate, LoweredType, LoweredTypeCheck, LoweredValue, Name, NameText, PathValue,
    PreparedConstantValue, QualifiedName, RegexValue, RuntimeOp, ScanBytes, ScanCheck, Span,
    Type, text_hole_kind, verify_node_metadata,
};

macro_rules! ignore_field {
    ($field:ident) => { _ };
}

macro_rules! pattern_encode_payload {
    (Normal, $builder:ident, $payload:ident) => {};
    (Normal, $builder:ident, $payload:ident, { $($field:ident),* $(,)? }) => {
        $($field.encode($builder, &mut $payload)?;)*
    };
    (Normal, $builder:ident, $payload:ident, ($field:ident)) => {
        $field.encode($builder, &mut $payload)?;
    };
    (Text, $builder:ident, $payload:ident, { $holes:ident, $kinds:ident, $segments:ident $(,)? }) => {
            if $kinds.len() != $holes.len() || $segments.len() != $holes.len() + 1 {
                return Err(IrBuildError::format(
                    "full_ir_text_pattern",
                    None,
                    0,
                    $builder.store.tags.len(),
                ));
            }
            $holes.encode($builder, &mut $payload)?;
            for kind in $kinds {
                $payload.push(*kind as u32);
            }
            for segment in $segments {
                segment.encode($builder, &mut $payload)?;
            }
    };
}

macro_rules! pattern_decode_row {
    ($variant:ident, Normal, $decoder:ident, $payload:ident) => {
        Self::$variant
    };
    ($variant:ident, Normal, $decoder:ident, $payload:ident, { $($field:ident: $field_ty:ty $(=> $verify_ty:ty)?),* $(,)? }) => {
        Self::$variant {
            $($field: <$field_ty>::decode($decoder, &mut $payload)?,)*
        }
    };
    ($variant:ident, Normal, $decoder:ident, $payload:ident, ($field:ident: $field_ty:ty)) => {
        Self::$variant(
            <$field_ty>::decode($decoder, &mut $payload)?,
        )
    };
    ($variant:ident, Text, $decoder:ident, $payload:ident, { $($fields:tt)* }) => {
        {
            let holes = Vec::<BuildPatternId>::decode($decoder, &mut $payload)?;
            let mut kinds = Vec::with_capacity(holes.len());
            for _ in 0..holes.len() {
                kinds.push(text_hole_kind($payload.raw()?)?);
            }
            let mut segments = Vec::with_capacity(holes.len() + 1);
            for _ in 0..=holes.len() {
                segments.push(Arc::<str>::decode($decoder, &mut $payload)?);
            }
            Self::$variant { holes, kinds, segments }
        }
    };
}

macro_rules! pattern_verify_field {
    ($decoder:ident, $payload:ident, $field_ty:ty) => {
        <$field_ty>::verify($decoder, &mut $payload)?;
    };
    ($decoder:ident, $payload:ident, $field_ty:ty => $verify_ty:ty) => {
        <$verify_ty>::verify($decoder, &mut $payload)?;
    };
}

macro_rules! pattern_verify_fields {
    ($decoder:ident, $payload:ident, { $($field:ident: $field_ty:ty $(=> $verify_ty:ty)?),* $(,)? }) => {
        $(pattern_verify_field!($decoder, $payload, $field_ty $(=> $verify_ty)?);)*
    };
}

macro_rules! pattern_verify_constraint {
    (None, $decoder:ident, $index:ident, $payload:ident) => {};
    (CaptureSlots, $decoder:ident, $index:ident, $payload:ident) => {
        $decoder.pattern_capture_slots($index)?;
    };
    (ListRest, $decoder:ident, $index:ident, $payload:ident) => {
        let mut rest_payload = $payload;
        Vec::<BuildPatternId>::verify($decoder, &mut rest_payload)?;
        if bool::decode($decoder, &mut rest_payload)? {
            let rest = rest_payload.raw()? as usize;
            if !matches!(
                $decoder.store.patterns.get(rest),
                Some(FullPatternTag::Wildcard | FullPatternTag::Bind)
            ) {
                return Err(IrVerifyError::new("list rest must be a wildcard or binding"));
            }
        }
    };
    (Alternation, $decoder:ident, $index:ident, $payload:ident) => {
        let mut alternatives = $payload;
        let mut children = $decoder.pattern_list_cursor(&mut alternatives)?;
        let count = children.raw()? as usize;
        if count < 2 {
            return Err(IrVerifyError::new(
                "alternative pattern requires at least two choices",
            ));
        }
        let mut common = None;
        for _ in 0..count {
            let child = children.raw()? as usize;
            let captures = $decoder.pattern_capture_slots(child)?;
            if common.as_ref().is_some_and(|common| common != &captures) {
                return Err(IrVerifyError::new(
                    "pattern alternatives must publish identical capture slots",
                ));
            }
            common = Some(captures);
        }
        children.finish()?;
    };
}

macro_rules! pattern_verify_row {
    ($variant:ident, Normal, $decoder:ident, $payload:ident, $index:ident, $semantic_payload:ident, $constraint:ident) => {
        pattern_verify_constraint!($constraint, $decoder, $index, $semantic_payload);
    };
    ($variant:ident, Normal, $decoder:ident, $payload:ident, $index:ident, $semantic_payload:ident, $constraint:ident, { $($field:ident: $field_ty:ty $(=> $verify_ty:ty)?),* $(,)? }) => {
        pattern_verify_fields!($decoder, $payload, { $($field: $field_ty $(=> $verify_ty)?),* });
        pattern_verify_constraint!($constraint, $decoder, $index, $semantic_payload);
    };
    ($variant:ident, Normal, $decoder:ident, $payload:ident, $index:ident, $semantic_payload:ident, $constraint:ident, ($field:ident: $field_ty:ty)) => {
        pattern_verify_field!($decoder, $payload, $field_ty);
        pattern_verify_constraint!($constraint, $decoder, $index, $semantic_payload);
    };
    ($variant:ident, Text, $decoder:ident, $payload:ident, $index:ident, $semantic_payload:ident, $constraint:ident, { holes: $holes_ty:ty, kinds: $kinds_ty:ty, segments: $segments_ty:ty $(,)? }) => {
        let mut children_payload = $semantic_payload;
        Vec::<BuildPatternId>::verify($decoder, &mut $payload)?;
        let mut children = $decoder.pattern_list_cursor(&mut children_payload)?;
        let count = children.raw()? as usize;
        for _ in 0..count {
            let hole = children.raw()? as usize;
            if !matches!(
                $decoder.store.patterns.get(hole),
                Some(FullPatternTag::Wildcard | FullPatternTag::Bind)
            ) {
                return Err(IrVerifyError::new(
                    "text pattern hole must be a wildcard or binding",
                ));
            }
        }
        children.finish()?;
        for _ in 0..count {
            text_hole_kind($payload.raw()?)?;
        }
        for segment_index in 0..=count {
            let segment = $decoder.store.string($payload.raw()?)?;
            if segment.is_empty() && segment_index != 0 && segment_index != count {
                return Err(IrVerifyError::new(
                    "text pattern holes must be separated by text",
                ));
            }
        }
    };
}

macro_rules! pattern_schema {
    (
        $enum_keyword:ident $tag_enum:ident {
            $(
                $variant:ident = $ordinal:literal
                $({ $($field:ident: $field_ty:ty $(=> $verify_ty:ty)?),* $(,)? })?
                $(($tuple_field:ident: $tuple_ty:ty))?
                [$mode:ident, $constraint:ident]
            ),* $(,)?
        }
    ) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub(in crate::runtime::eval) enum $tag_enum {
            $($variant = $ordinal,)*
        }

        pub(in crate::runtime::eval) const PATTERN_TAGS: &[FullPatternTag] = &[
            $(FullPatternTag::$variant,)*
        ];

        impl FullCodec for BuildPatternRow {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let row = self;
                let mut payload = builder.take_payload();
                let tag = match row {
                    $(Self::$variant
                        $( { $($field),* })?
                        $(($tuple_field))?
                        => {
                            pattern_encode_payload!(
                                $mode, builder, payload
                                $(, { $($field),* })?
                                $(, ($tuple_field))?
                            );
                            FullPatternTag::$variant
                        },)*
                };
                let pattern = builder.push_pattern(tag, &payload);
                builder.recycle_payload(payload);
                output.push(pattern?);
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = decoder
                    .store
                    .patterns
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
                let data = decoder.store.pattern_data[index];
                let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
                let pattern = match tag {
                    $(FullPatternTag::$variant => pattern_decode_row!(
                        $variant, $mode, decoder, payload
                        $(, { $($field: $field_ty $(=> $verify_ty)?),* })?
                        $(, ($tuple_field: $tuple_ty))?
                    ),)*
                };
                payload.finish()?;
                Ok(pattern)
            }

            fn verify(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<(), IrVerifyError> {
                let index = input.raw()? as usize;
                let parent = decoder.pattern_ceiling.get();
                if index >= parent {
                    return Err(IrVerifyError::new("nested pattern must precede its parent"));
                }
                decoder.pattern_ceiling.set(index);
                let tag = decoder
                    .store
                    .patterns
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
                let data = decoder.store.pattern_data[index];
                let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
                match tag {
                    $(FullPatternTag::$variant => {
                        let _semantic_payload = payload;
                        pattern_verify_row!(
                            $variant, $mode, decoder, payload, index, _semantic_payload,
                            $constraint
                            $(, { $($field: $field_ty $(=> $verify_ty)?),* })?
                            $(, ($tuple_field: $tuple_ty))?
                        );
                    },)*
                }
                payload.finish()?;
                decoder.pattern_ceiling.set(parent);
                Ok(())
            }
        }
    };
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
                    => $tag:ident = $ordinal:literal [$effects:expr; $class:ident];
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

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(in crate::runtime::eval) enum InPlaceClass {
            Operands,
            Scheduled,
        }

        impl FullTag {
            pub(in crate::runtime::eval) fn in_place_class(self) -> InPlaceClass {
                match self {
                    $($(Self::$tag => InPlaceClass::$class,)*)*
                }
            }

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
        Int(value: i64) => IntInt = 0 [0; Operands];
        Slot(slot: usize) => IntSlot = 1 [0; Operands];
        Binary {
            op: BinaryOp,
            left: BuildIntId,
            right: BuildIntId,
        } => IntBinary = 2 [0; Operands];
        StrByteLenSlot {
            slot: usize,
            span: Span,
        } => IntStrByteLenSlot = 3 [0; Operands];
        StrCountLinesSlot {
            slot: usize,
            span: Span,
        } => IntStrCountLinesSlot = 4 [0; Operands];
        StrByteAtSlot {
            slot: usize,
            index: BuildIntId,
            default: Option<BuildIntId>,
            span: Span,
        } => IntStrByteAtSlot = 5 [0; Operands];
    }
    BuildBoolRow => BoolView [Bool] {
        Bool(value: bool) => BoolBool = 6 [0; Operands];
        Slot(slot: usize) => BoolSlot = 7 [0; Operands];
        Not(value: BuildBoolId) => BoolNot = 8 [0; Operands];
        And(left: BuildBoolId, right: BuildBoolId) => BoolAnd = 9 [0; Operands];
        Or(left: BuildBoolId, right: BuildBoolId) => BoolOr = 10 [0; Operands];
        IntCompare {
            op: BinaryOp,
            left: BuildIntId,
            right: BuildIntId,
        } => BoolIntCompare = 11 [0; Operands];
        StrPredicateSlot {
            slot: usize,
            predicate: LoweredStrPredicate,
            needle: Arc<[u8]>,
            span: Span,
        } => BoolStrPredicateSlot = 12 [0; Operands];
        ContainsSlot {
            slot: usize,
            needle: LoweredValue,
            span: Span,
        } => BoolContainsSlot = 13 [0; Operands];
        StrContainsSlot {
            slot: usize,
            needle: Arc<str>,
            span: Span,
        } => BoolStrContainsSlot = 14 [0; Operands];
        TrimEmptySlot {
            slot: usize,
            span: Span,
        } => BoolTrimEmptySlot = 15 [0; Operands];
        TrimStrPredicateSlot {
            slot: usize,
            predicate: LoweredStrPredicate,
            needle: Arc<[u8]>,
            span: Span,
        } => BoolTrimStrPredicateSlot = 16 [0; Operands];
        LiteralCompareSlot {
            op: BinaryOp,
            slot: usize,
            value: LoweredValue,
        } => BoolLiteralCompareSlot = 17 [0; Operands];
    }
    BuildExprRow => ExprView [Expr] {
        Null => ExprNull = 18 [0; Operands];
        Unit => ExprUnit = 19 [0; Operands];
        Int(value: i64) => ExprInt = 20 [0; Operands];
        Float(value: FloatValue) => ExprFloat = 21 [0; Operands];
        Duration(value: DurationValue) => ExprDuration = 22 [0; Operands];
        Bool(value: bool) => ExprBool = 23 [0; Operands];
        Str(value: Arc<str>) => ExprStr = 24 [0; Operands];
        Bytes(value: Arc<[u8]>) => ExprBytes = 25 [0; Operands];
        PreparedRegex(value: RegexValue) => ExprPreparedRegex = 26 [0; Operands];
        PreparedConstant(value: PreparedConstantValue) => ExprPreparedConstant = 27 [0; Operands];
        Path(value: PathValue) => ExprPath = 28 [0; Operands];
        FunctionRef {
            function: FunctionName,
            pure: bool,
        } => ExprFunctionRef = 29 [0; Operands];
        PathFrom {
            value: BuildExprId,
            span: Span,
        } => ExprPathFrom = 30 [0; Operands];
        Param(slot: usize) => ExprParam = 31 [0; Operands];
        ComparisonChain {
            pairs: Vec<BuildExprId>,
            assertion: bool,
        } => ExprComparisonChain = 32 [0; Operands];
        Binary {
            op: BinaryOp,
            left: BuildExprId,
            right: BuildExprId,
            span: Span,
        } => ExprBinary = 33 [0; Operands];
        IfExpr {
            branches: Vec<(BuildExprId, BuildExprId)>,
            else_value: BuildExprId,
            span: Span,
        } => ExprIf = 34 [0; Operands];
        PatternIf {
            branches: Vec<(BuildExprId, BuildExprId, Vec<usize>)>,
            else_value: BuildExprId,
            span: Span,
        } => ExprPatternIf = 35 [0; Operands];
        MatchExpr {
            value: BuildExprId,
            arms: Vec<(BuildPatternId, Option<BuildExprId>, BuildExprId)>,
            span: Span,
        } => ExprMatch = 36 [0; Operands];
        StrMatchExpr {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, BuildExprId>,
            fallback: Option<BuildExprId>,
            span: Span,
        } => ExprStrMatch = 37 [0; Operands];
        TagMatchExpr {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, BuildExprId>,
            fallback: Option<BuildExprId>,
            span: Span,
        } => ExprTagMatch = 38 [0; Operands];
        ResultFallback {
            left: BuildExprId,
            right: BuildExprId,
        } => ExprResultFallback = 39 [0; Operands];
        FmtString(parts: Vec<LoweredFmtPart>) => ExprFmtString = 40 [0; Operands];
        PathFmtString {
            parts: Vec<LoweredFmtPart>,
            span: Span,
        } => ExprPathFmtString = 41 [0; Operands];
        // A `g"..."` glob literal. Expanded against the current cwd at eval time into
        // a `List[Path]` (mirrors the recursive evaluator's `eval_glob`).
        Glob {
            pattern: Arc<str>,
            span: Span,
        } => ExprGlob = 42 [0; Operands];
        // The `$?` last-process-status expression. Reads the evaluator's tracked
        // `last_status` at eval time, erroring if no process has run yet.
        LastStatus {
            span: Span,
        } => ExprLastStatus = 43 [0; Operands];
        Record(entries: Vec<LoweredRecordEntry>) => ExprRecord = 44 [0; Operands];
        // A missing key identifies a Map spread; keyed entries evaluate key before value.
        MapLiteral(entries: Vec<(Option<BuildExprId>, BuildExprId, Span)>) => ExprMapLiteral = 45 [0; Operands];
        RecordUpdate {
            base: BuildExprId,
            updates: LoweredRecordUpdates,
            span: Span,
        } => ExprRecordUpdate = 46 [0; Operands];
        List(values: Vec<BuildExprId>) => ExprList = 47 [0; Operands];
        // Each element records whether it splices, its value, and its source span.
        ListBuild(values: Vec<(bool, BuildExprId, Span)>) => ExprListBuild = 48 [0; Operands];
        // The `map.empty()` builtin constructor (empty list literals already lower via `List`).
        EmptyMap => ExprEmptyMap = 49 [0; Operands];
        // The `bytes.concat(<List[Bytes]>)` builtin constructor.
        BytesConcat {
            arg: BuildExprId,
            span: Span,
        } => ExprBytesConcat = 50 [0; Operands];
        Range {
            start: BuildExprId,
            end: BuildExprId,
            span: Span,
        } => ExprRange = 51 [0; Operands];
        Tag {
            type_name: Name,
            name: Arc<str>,
            fields: Vec<BuildExprId>,
            wire: Option<Arc<crate::sema::wire_enums::WireEnumMapping>>,
        } => ExprTag = 52 [0; Operands];
        ListComp {
            value: BuildExprId,
            qualifiers: LoweredCompQualifiers,
            span: Span,
        } => ExprListComp = 53 [0; Scheduled];
        MapComp {
            key: BuildExprId,
            value: BuildExprId,
            qualifiers: LoweredCompQualifiers,
            span: Span,
        } => ExprMapComp = 54 [0; Scheduled];
        ListPipeline {
            input: BuildExprId,
            stages: Vec<LoweredPipelineStage>,
            span: Span,
        } => ExprPipeline = 55 [0; Scheduled];
        Field {
            base: BuildExprId,
            // Scratch rows preserve generated static text without allocation while
            // retaining an owned spelling for dynamic names.
            name: NameText,
            span: Span,
        } => ExprField = 56 [0; Operands];
        Index {
            base: BuildExprId,
            index: BuildExprId,
            span: Span,
        } => ExprIndex = 57 [0; Operands];
        /// `base[-distance]` over a list, where the checker saw the negative
        /// literal.
        IndexFromEnd {
            base: BuildExprId,
            distance: crate::runtime::eval::EndDistance,
            span: Span,
        } => ExprIndexFromEnd = 58 [0; Operands];
        Slice {
            base: BuildExprId,
            start: Option<BuildExprId>,
            end: Option<BuildExprId>,
            span: Span,
        } => ExprSlice = 59 [0; Operands];
        Method {
            receiver: BuildExprId,
            name: NameText,
            args: Vec<BuildExprId>,
            span: Span,
        } => ExprMethod = 60 [0; Scheduled];
        StrByteLen {
            receiver: BuildExprId,
            span: Span,
        } => ExprStrByteLen = 61 [0; Operands];
        StrByteAt {
            receiver: BuildExprId,
            index: BuildExprId,
            span: Span,
        } => ExprStrByteAt = 62 [0; Operands];
        StrPredicate {
            receiver: BuildExprId,
            predicate: LoweredStrPredicate,
            needle: BuildExprId,
            span: Span,
        } => ExprStrPredicate = 63 [0; Operands];
        RegexCompile {
            pattern: BuildExprId,
            span: Span,
        } => ExprRegexCompile = 64 [0; Operands];
        Require {
            value: BuildExprId,
            check: LoweredTypeCheck,
            span: Span,
        } => ExprRequire = 65 [0; Operands];
        // Preserve a checked domain while retaining ordinary runtime failure semantics.
        CheckedValue {
            value: BuildExprId,
            check: LoweredTypeCheck,
            span: Span,
        } => ExprCheckedValue = 66 [0; Operands];
        // Boxed because captured and spawned runs are cold, unusually wide
        // payloads that would otherwise determine the size of every expression.
        RunCapture(value: Box<LoweredRunCapture>) => ExprRunCapture = 67 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        RunPipeline {
            segments: Vec<LoweredRunPipelineSegment>,
            propagate: bool,
            span: Span,
        } => ExprRunPipeline = 68 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        SpawnRun(value: Box<LoweredSpawnRun>) => ExprSpawnRun = 69 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        SpawnCommand {
            command: BuildExprId,
            span: Span,
        } => ExprSpawnCommand = 70 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Wait {
            target: BuildExprId,
            span: Span,
        } => ExprWait = 71 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Capture {
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprCapture = 72 [0; Scheduled];
        ValueBlock {
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprValueBlock = 73 [0; Scheduled];
        ErrorContext {
            message: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprErrorContext = 74 [0; Scheduled];
        ContextScope {
            kind: crate::syntax::arena::ContextScopeKind,
            input: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprContextScope = 75 [EFFECT_CWD | EFFECT_ENV | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Loop {
            body: Vec<BuildStmtId>,
            span: Span,
        } => ExprLoop = 76 [EFFECT_CANCELLATION; Scheduled];
        Retry {
            delays: Vec<BuildExprId>,
            pattern: Option<BuildPatternId>,
            body: Vec<BuildStmtId>,
            span: Span,
            schedule: crate::syntax::arena::RetrySchedule,
        } => ExprRetry = 77 [0; Scheduled];
        FsFiles {
            root: BuildExprId,
            gitignore: Option<BuildExprId>,
            stat: Option<BuildExprId>,
            hidden: Option<BuildExprId>,
            exts: Option<BuildExprId>,
            result_wrapped: bool,
            span: Span,
        } => ExprFsFiles = 78 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        FsWalk {
            root: BuildExprId,
            gitignore: Option<BuildExprId>,
            stat: Option<BuildExprId>,
            hidden: Option<BuildExprId>,
            exts: Option<BuildExprId>,
            result_wrapped: bool,
            span: Span,
        } => ExprFsWalk = 79 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        FsList {
            op: RuntimeOp,
            path: BuildExprId,
            stat: Option<BuildExprId>,
            ordered: Option<BuildExprId>,
            span: Span,
        } => ExprFsList = 80 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        FsTempDir {
            span: Span,
        } => ExprFsTempDir = 81 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        FsMkdir {
            path: BuildExprId,
            parents: Option<BuildExprId>,
            span: Span,
        } => ExprFsMkdir = 82 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        FsRemove {
            path: BuildExprId,
            missing_ok: Option<BuildExprId>,
            span: Span,
        } => ExprFsRemove = 83 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathReadText {
            path: BuildExprId,
            span: Span,
        } => ExprPathReadText = 84 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathReadBytes {
            path: BuildExprId,
            span: Span,
        } => ExprPathReadBytes = 85 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathExists {
            path: BuildExprId,
            span: Span,
        } => ExprPathExists = 86 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathExecutable {
            path: BuildExprId,
            span: Span,
        } => ExprPathExecutable = 87 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathDu {
            path: BuildExprId,
            span: Span,
        } => ExprPathDu = 88 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathMetadata {
            path: BuildExprId,
            span: Span,
        } => ExprPathMetadata = 89 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathReadlink {
            path: BuildExprId,
            span: Span,
        } => ExprPathReadlink = 90 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathResolve {
            path: BuildExprId,
            span: Span,
        } => ExprPathResolve = 91 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathWrite {
            path: BuildExprId,
            data: BuildExprId,
            atomic: bool,
            span: Span,
        } => ExprPathWrite = 92 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathMkdir {
            path: BuildExprId,
            parents: Option<BuildExprId>,
            span: Span,
        } => ExprPathMkdir = 93 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        PathRemove {
            path: BuildExprId,
            missing_ok: Option<BuildExprId>,
            span: Span,
        } => ExprPathRemove = 94 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        JsonEncode {
            value: BuildExprId,
            span: Span,
        } => ExprJsonEncode = 95 [0; Operands];
        ArchiveTarCreate {
            path: BuildExprId,
            root: BuildExprId,
            entries: BuildExprId,
            compression: Option<BuildExprId>,
            overwrite: Option<BuildExprId>,
            span: Span,
        } => ExprArchiveTarCreate = 96 [EFFECT_PROPAGATE | EFFECT_TRACE; Scheduled];
        ArchiveTarList {
            path: BuildExprId,
            span: Span,
        } => ExprArchiveTarList = 97 [EFFECT_PROPAGATE | EFFECT_TRACE; Scheduled];
        ArchiveTarExtract {
            path: BuildExprId,
            dest: BuildExprId,
            span: Span,
        } => ExprArchiveTarExtract = 98 [EFFECT_PROPAGATE | EFFECT_TRACE; Scheduled];
        ModuleCall {
            op: RuntimeOp,
            call_plan: Option<Arc<crate::runtime::eval::ModuleCallPlan>>,
            args: Vec<Option<BuildExprId>>,
            span: Span,
        } => ExprModuleCall = 99 [EFFECT_HOST | EFFECT_TRACE; Scheduled];
        // Boxed because this cold command-construction payload otherwise makes
        // every `BuildExprId` 16 bytes larger, including expression-heavy scripts.
        ProcessCommandArgv(value: Box<LoweredProcessCommandArgv>) => ExprProcessCommandArgv = 100 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        ProcessCommandBuilder {
            entries: Vec<LoweredProcessCommandBuilderEntry>,
            span: Span,
        } => ExprProcessCommandBuilder = 101 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Abort {
            status: BuildExprId,
            span: Span,
        } => ExprAbort = 102 [EFFECT_PROPAGATE | EFFECT_TRACE; Scheduled];
        Fail {
            message: BuildExprId,
            span: Span,
        } => ExprFail = 103 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        Ok(value: BuildExprId) => ExprOk = 104 [0; Operands];
        Err {
            value: BuildExprId,
            cause: Option<BuildExprId>,
        } => ExprErr = 105 [0; Operands];
        // Boxed: `LoweredErrorExpr::Structured` inlines two `String`s plus two
        // `Vec`s (~96 bytes) that would otherwise size every `BuildExprId` variant
        // for the sake of the comparatively rare structured-error-literal case.
        Error(value: Box<LoweredErrorExpr>) => ExprError = 106 [0; Operands];
        Try {
            value: BuildExprId,
            span: Span,
        } => ExprTry = 107 [EFFECT_PROPAGATE | EFFECT_TRACE; Operands];
        Call {
            function: LoweredFunctionKey,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprCall = 108 [EFFECT_TRACE; Scheduled];
        DirectPureCall {
            function: LoweredFunctionKey,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprDirectPureCall = 109 [EFFECT_TRACE; Scheduled];
        /// A call to an implementation a loading program published.
        ///
        /// Used only by a dynamically loaded module that links its standard-library
        /// calls to the implementations its loader already prepared, so the callee
        /// has no local function identity to encode.
        ExternalCall {
            function: QualifiedName,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprExternalCall = 110 [EFFECT_TRACE; Scheduled];
        DynamicCall {
            callee: BuildExprId,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprDynamicCall = 111 [EFFECT_DYNAMIC_CALL | EFFECT_TRACE; Scheduled];
        SelfCall {
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprSelfCall = 112 [EFFECT_TRACE; Scheduled];
        /// A call through a value of a callable type. `signature` is the checked
        /// `Type::Callable` the arguments were bound against, and `pure` its
        /// kind; `args` supplies each of its parameters once, in order.
        TypedCall {
            callee: BuildExprId,
            pure: bool,
            signature: Type,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => ExprTypedCall = 113 [EFFECT_DYNAMIC_CALL | EFFECT_TRACE; Scheduled];
        /// What `Proc.call` returns for the call it wraps: the `Result` the proc
        /// returned, or `Ok` of any other value. A dynamic handle does not say
        /// what its proc returns, so the checker types the call `Result[Any]`
        /// and this makes every proc honor that.
        ProcCallResult(value: BuildExprId) => ExprProcCallResult = 114 [0; Scheduled];
    }
    BuildStmtRow => StmtView [Stmt] {
        Let {
            slot: usize,
            value: BuildExprId,
        } => StmtLet = 115 [0; Scheduled];
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
        } => StmtGuard = 116 [EFFECT_PROPAGATE | EFFECT_TRACE; Scheduled];
        With {
            bindings: Vec<(usize, BuildExprId)>,
            body: Vec<BuildStmtId>,
            else_param_slot: Option<usize>,
            else_body: Vec<BuildStmtId>,
            captures: Vec<usize>,
            span: Span,
        } => StmtWith = 117 [EFFECT_PROPAGATE | EFFECT_TRACE; Scheduled];
        LetInt {
            slot: usize,
            value: BuildIntId,
        } => StmtLetInt = 118 [0; Scheduled];
        LetBool {
            slot: usize,
            value: BuildBoolId,
        } => StmtLetBool = 119 [0; Scheduled];
        Assign {
            slot: usize,
            op: AssignOp,
            value: BuildExprId,
            // Validate the final replacement before offering storage to consuming operations.
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => StmtAssign = 120 [0; Scheduled];
        AssignField {
            slot: usize,
            field: Arc<str>,
            op: AssignOp,
            value: BuildExprId,
            span: Span,
        } => StmtAssignField = 121 [0; Scheduled];
        AssignFieldInt {
            slot: usize,
            field: Arc<str>,
            op: AssignOp,
            value: BuildIntId,
            span: Span,
        } => StmtAssignFieldInt = 122 [0; Scheduled];
        AssignPath {
            slot: usize,
            path: LoweredAssignPath,
            op: AssignOp,
            value: BuildExprId,
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => StmtAssignPath = 123 [0; Scheduled];
        AssignInt {
            slot: usize,
            op: AssignOp,
            value: BuildIntId,
            span: Span,
        } => StmtAssignInt = 124 [0; Scheduled];
        AssignBool {
            slot: usize,
            value: BuildBoolId,
        } => StmtAssignBool = 125 [0; Scheduled];
        Value {
            value: BuildExprId,
        } => StmtValue = 126 [0; Scheduled];
        Assert {
            value: BuildExprId,
            message: Option<BuildExprId>,
            span: Span,
        } => StmtAssert = 127 [0; Scheduled];
        Expr {
            value: BuildExprId,
            span: Span,
        } => StmtExpr = 128 [0; Scheduled];
        If {
            branches: Vec<(BuildExprId, Vec<BuildStmtId>)>,
            else_body: Option<Vec<BuildStmtId>>,
        } => StmtIf = 129 [0; Scheduled];
        IfBool {
            branches: Vec<(BuildBoolId, Vec<BuildStmtId>)>,
            else_body: Option<Vec<BuildStmtId>>,
        } => StmtIfBool = 130 [0; Scheduled];
        While {
            condition: BuildExprId,
            body: Vec<BuildStmtId>,
        } => StmtWhile = 131 [EFFECT_CANCELLATION; Scheduled];
        WhileBool {
            condition: BuildBoolId,
            body: Vec<BuildStmtId>,
        } => StmtWhileBool = 132 [EFFECT_CANCELLATION; Scheduled];
        PatternIf {
            branches: Vec<(BuildExprId, Vec<BuildStmtId>, Vec<usize>)>,
            else_body: Option<Vec<BuildStmtId>>,
            span: Span,
        } => StmtPatternIf = 133 [EFFECT_CANCELLATION; Scheduled];
        PatternWhile {
            condition: BuildExprId,
            body: Vec<BuildStmtId>,
            captures: Vec<usize>,
            span: Span,
        } => StmtPatternWhile = 134 [EFFECT_CANCELLATION; Scheduled];
        Match {
            value: BuildExprId,
            arms: Vec<(BuildPatternId, Option<BuildExprId>, Vec<BuildStmtId>)>,
            span: Span,
        } => StmtMatch = 135 [0; Scheduled];
        StrMatch {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, Vec<BuildStmtId>>,
            fallback: Option<Vec<BuildStmtId>>,
            span: Span,
        } => StmtStrMatch = 136 [0; Scheduled];
        TagMatch {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, Vec<BuildStmtId>>,
            fallback: Option<Vec<BuildStmtId>>,
            span: Span,
        } => StmtTagMatch = 137 [0; Scheduled];
        For {
            slot: usize,
            iter: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => StmtFor = 138 [EFFECT_CANCELLATION; Scheduled];
        // Select a recursive record target into independent local values.
        LetRecord {
            source: BuildExprId,
            target: LoweredCompTarget,
            span: Span,
        } => StmtLetRecord = 139 [0; Scheduled];
        // Select the recursive record target anew for each iteration.
        ForRecord {
            target: LoweredCompTarget,
            iter: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => StmtForRecord = 140 [EFFECT_CANCELLATION; Scheduled];
        ForStrLines {
            slot: usize,
            text: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => StmtForStrLines = 141 [EFFECT_CANCELLATION; Scheduled];
        ScanLines {
            text_slot: usize,
            line_slot: usize,
            checks: Vec<ScanCheck>,
            span: Span,
        } => StmtScanLines = 142 [EFFECT_CANCELLATION; Scheduled];
        ScanBytes {
            config: ScanBytes,
        } => StmtScanBytes = 143 [EFFECT_CANCELLATION; Scheduled];
        Print {
            args: Vec<BuildExprId>,
            stderr: bool,
            flush: bool,
            propagate_result: bool,
            span: Span,
        } => StmtPrint = 144 [EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Cd {
            target: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => StmtCd = 145 [EFFECT_CWD | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Env {
            env: Vec<LoweredRunEnv>,
            body: Vec<BuildStmtId>,
        } => StmtEnv = 146 [EFFECT_ENV | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Proc {
            // The registry identity is stable; resolve it once while lowering
            // instead of retaining two names and looking it up on every execution.
            op: RuntimeOp,
            args: Vec<BuildExprId>,
            propagate_result: bool,
            span: Span,
        } => StmtProc = 147 [EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Run {
            value: BuildExprId,
            propagate_result: bool,
        } => StmtRun = 148 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_HOST | EFFECT_TRACE; Scheduled];
        Loop {
            body: Vec<BuildStmtId>,
        } => StmtLoop = 149 [EFFECT_CANCELLATION; Scheduled];
        Return {
            value: BuildExprId,
        } => StmtReturn = 150 [0; Scheduled];
        Yield {
            value: BuildExprId,
        } => StmtYield = 151 [0; Scheduled];
        YieldDelegate {
            value: BuildExprId,
            span: Span,
        } => StmtYieldDelegate = 152 [0; Scheduled];
        Break => StmtBreak = 153 [0; Scheduled];
        BreakValue {
            value: BuildExprId,
        } => StmtBreakValue = 154 [0; Scheduled];
        Continue => StmtContinue = 155 [0; Scheduled];
        Defer {
            value: BuildExprId,
            /// Whether this is an `errdefer`: the action runs only when its scope
            /// leaves with an error.
            on_error: bool,
        } => StmtDefer = 156 [EFFECT_DEFER | EFFECT_PROPAGATE | EFFECT_TRACE; Scheduled];
        DefaultParameter {
            slot: usize,
            value: BuildExprId,
            kind: LoweredType,
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => StmtDefaultParameter = 157 [0; Scheduled];
    }
}

pattern_schema! {
    enum FullPatternTag {
        Alias = 0 {
            pattern: BuildPatternId,
            slot: usize,
        } [Normal, CaptureSlots],
        Alternation = 1 {
            patterns: Vec<BuildPatternId>,
        } [Normal, Alternation],
        List = 2 {
            elements: Vec<BuildPatternId>,
            rest: Option<BuildPatternId>,
        } [Normal, ListRest],
        TagType = 3 {
            type_name: Name,
            variants: Vec<Name>,
        } [Normal, None],
        RecordTest = 4 {
            fields: Vec<(Name, BuildPatternId)> => Box<Vec<(Name, BuildPatternId)>>,
        } [Normal, None],
        ResultTest = 5 {
            ok: bool,
            inner: BuildPatternId,
        } [Normal, None],
        TagTest = 6 {
            type_name: Name,
            name: Name,
            fields: Vec<BuildPatternId>,
        } [Normal, None],
        ErrorTest = 7 {
            family: Name,
            variant: Name,
            fields: Vec<(Name, BuildPatternId)> => Box<Vec<(Name, BuildPatternId)>>,
        } [Normal, None],
        Wildcard = 8 [Normal, None],
        Bind = 9 {
            slot: usize,
        } [Normal, None],
        Type = 10 {
            ty: Type,
            slot: Option<usize>,
        } [Normal, None],
        Literal = 11 (value: LoweredValue) [Normal, None],
        ResultOk = 12 {
            slot: Option<usize>,
            unit_only: bool,
        } [Normal, None],
        ResultErr = 13 {
            slot: Option<usize>,
            unit_only: bool,
        } [Normal, None],
        ErrorVariant = 14 {
            family: Name,
            variant: Name,
            fields: Box<LoweredErrorPatternFields>,
            result_wrapped: bool,
        } [Normal, None],
        Facet = 15 {
            facet: Name,
            result_wrapped: bool,
        } [Normal, None],
        Tag = 16 {
            type_name: Name,
            name: Name,
            slots: BuildPatternIdSlots,
        } [Normal, None],
        Text = 17 {
            holes: Vec<BuildPatternId>,
            kinds: Vec<crate::sema::check::TextHoleKind>,
            segments: Vec<Arc<str>>,
        } [Text, None],
    }
}

#[cfg(test)]
mod tests {
    use super::{FullPatternTag, INSTRUCTION_TAGS, InPlaceClass, PATTERN_TAGS};

    #[test]
    fn instruction_classes_include_only_operations_that_never_run_script() {
        let operands = r#"
            IntInt
            IntSlot
            IntBinary
            IntStrByteLenSlot
            IntStrCountLinesSlot
            IntStrByteAtSlot
            BoolBool
            BoolSlot
            BoolNot
            BoolAnd
            BoolOr
            BoolIntCompare
            BoolStrPredicateSlot
            BoolContainsSlot
            BoolStrContainsSlot
            BoolTrimEmptySlot
            BoolTrimStrPredicateSlot
            BoolLiteralCompareSlot
            ExprNull
            ExprUnit
            ExprInt
            ExprFloat
            ExprDuration
            ExprBool
            ExprStr
            ExprBytes
            ExprPreparedRegex
            ExprPreparedConstant
            ExprPath
            ExprFunctionRef
            ExprPathFrom
            ExprParam
            ExprComparisonChain
            ExprBinary
            ExprIf
            ExprPatternIf
            ExprMatch
            ExprStrMatch
            ExprTagMatch
            ExprResultFallback
            ExprFmtString
            ExprPathFmtString
            ExprGlob
            ExprLastStatus
            ExprRecord
            ExprMapLiteral
            ExprRecordUpdate
            ExprList
            ExprListBuild
            ExprEmptyMap
            ExprBytesConcat
            ExprRange
            ExprTag
            ExprField
            ExprIndex
            ExprIndexFromEnd
            ExprSlice
            ExprStrByteLen
            ExprStrByteAt
            ExprStrPredicate
            ExprRegexCompile
            ExprRequire
            ExprCheckedValue
            ExprFsFiles
            ExprFsWalk
            ExprFsList
            ExprFsTempDir
            ExprFsMkdir
            ExprFsRemove
            ExprPathReadText
            ExprPathReadBytes
            ExprPathExists
            ExprPathExecutable
            ExprPathDu
            ExprPathMetadata
            ExprPathReadlink
            ExprPathResolve
            ExprPathWrite
            ExprPathMkdir
            ExprPathRemove
            ExprJsonEncode
            ExprFail
            ExprOk
            ExprErr
            ExprError
            ExprTry
        "#;
        let names: Vec<_> = operands.split_whitespace().collect();
        assert_eq!(names.len(), 86);
        for tag in INSTRUCTION_TAGS {
            let expected = if names.contains(&tag.name()) { InPlaceClass::Operands } else { InPlaceClass::Scheduled };
            assert_eq!(tag.in_place_class(), expected, "{}", tag.name());
        }
    }

    #[test]
    fn pattern_schema_preserves_all_tag_ordinals() {
        assert_eq!(PATTERN_TAGS.len(), 18);
        for (ordinal, tag) in PATTERN_TAGS.iter().enumerate() {
            assert_eq!(*tag as usize, ordinal);
        }
        assert_eq!(FullPatternTag::Text as u8, 17);
    }
}
