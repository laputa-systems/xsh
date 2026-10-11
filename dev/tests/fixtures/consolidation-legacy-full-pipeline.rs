pub(in crate::runtime::eval) enum FullStageTag {
    TextLines,
    JsonLines,
    Where,
    WhereBlock,
    Map,
    MapBlock,
    FlatMap,
    FlatMapBlock,
    BytesChunks,
    BatchCount,
    BatchMaxArgv,
    BatchMaxBytes,
    BatchLimits,
    Shuffle,
    Fold,
    ReduceBy,
    ReduceByConfigured,
    ParMap,
    ParMapBlock,
    ParMapFlatMapReduceBy,
    Tee,
    Each,
    TablePrint,
    TablePrintConfigured,
    Enumerate,
    Zip,
    Sort,
    SortBy,
    GroupBy,
    CountBy,
    Any,
    AnyBlock,
    All,
    AllBlock,
    UniqueBy,
    Count,
    Sum,
    Collect,
    First,
    Last,
    Min,
    Max,
    Take,
    Drop,
    Repeat,
    Range,
}
macro_rules! impl_stage_codec {
    (
        $(
            $pattern:pat => $tag:ident {
                $($field:ident : $field_ty:ty),* $(,)?
            } => $construct:expr
        ),* $(,)?
    ) => {
        impl FullCodec for LoweredPipelineStage {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let (tag, payload) = match self {
                    $(
                        $pattern => {
                            #[allow(unused_mut)]
                            let mut payload = builder.take_payload();
                            $(
                                $field.encode(builder, &mut payload)?;
                            )*
                            (FullStageTag::$tag, payload)
                        }
                    ),*
                };
                let stage = builder.push_stage(tag, &payload);
                builder.recycle_payload(payload);
                output.push(stage?);
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = decoder
                    .store
                    .stages
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
                let data = decoder.store.stage_data[index];
                let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
                let stage = match tag {
                    $(
                        FullStageTag::$tag => {
                            $(
                                let $field = <$field_ty>::decode(decoder, &mut payload)?;
                            )*
                            $construct
                        }
                    ),*
                };
                payload.finish()?;
                Ok(stage)
            }

            fn verify(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<(), IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = decoder
                    .store
                    .stages
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
                let data = decoder.store.stage_data[index];
                let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
                match tag {
                    $(
                        FullStageTag::$tag => {
                            $(
                                <$field_ty>::verify(decoder, &mut payload)?;
                            )*
                        }
                    ),*
                }
                payload.finish()
            }
        }
    };
}

impl_stage_codec! {
    LoweredPipelineStage::TextLines => TextLines {} => LoweredPipelineStage::TextLines,
    LoweredPipelineStage::JsonLines => JsonLines {} => LoweredPipelineStage::JsonLines,
    LoweredPipelineStage::Where { slot, predicate } => Where {
        slot: usize,
        predicate: BuildExprId,
    } => LoweredPipelineStage::Where { slot, predicate },
    LoweredPipelineStage::WhereBlock { slot, body, value } => WhereBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::WhereBlock { slot, body, value },
    LoweredPipelineStage::Map { slot, value } => Map {
        slot: usize,
        value: BuildExprId,
    } => LoweredPipelineStage::Map { slot, value },
    LoweredPipelineStage::MapBlock { slot, body, value } => MapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::MapBlock { slot, body, value },
    LoweredPipelineStage::FlatMap { slot, value } => FlatMap {
        slot: usize,
        value: BuildExprId,
    } => LoweredPipelineStage::FlatMap { slot, value },
    LoweredPipelineStage::FlatMapBlock { slot, body, value } => FlatMapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::FlatMapBlock { slot, body, value },
    LoweredPipelineStage::BytesChunks { size } => BytesChunks {
        size: BuildExprId,
    } => LoweredPipelineStage::BytesChunks { size },
    LoweredPipelineStage::BatchCount { count } => BatchCount {
        count: BuildExprId,
    } => LoweredPipelineStage::BatchCount { count },
    LoweredPipelineStage::BatchMaxArgv { max_argv } => BatchMaxArgv {
        max_argv: Option<BuildExprId>,
    } => LoweredPipelineStage::BatchMaxArgv { max_argv },
    LoweredPipelineStage::BatchMaxBytes { max_bytes } => BatchMaxBytes {
        max_bytes: BuildExprId,
    } => LoweredPipelineStage::BatchMaxBytes { max_bytes },
    LoweredPipelineStage::BatchLimits { configuration } => BatchLimits {
        configuration: BuildExprId,
    } => LoweredPipelineStage::BatchLimits { configuration },
    LoweredPipelineStage::Shuffle { seed } => Shuffle {
        seed: Option<BuildExprId>,
    } => LoweredPipelineStage::Shuffle { seed },
    LoweredPipelineStage::Fold {
        acc_slot,
        item_slot,
        initial,
        body,
        value,
    } => Fold {
        acc_slot: usize,
        item_slot: usize,
        initial: BuildExprId,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::Fold {
        acc_slot,
        item_slot,
        initial,
        body,
        value,
    },
    LoweredPipelineStage::ReduceBy {
        item_slot,
        body,
        value,
        op,
        jobs,
    } => ReduceBy {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        op: ReduceByOp,
        jobs: Option<BuildExprId>,
    } => LoweredPipelineStage::ReduceBy {
        item_slot,
        body,
        value,
        op,
        jobs,
    },
    LoweredPipelineStage::ReduceByConfigured { item_slot, body, value, configuration } => ReduceByConfigured {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        configuration: BuildExprId,
    } => LoweredPipelineStage::ReduceByConfigured { item_slot, body, value, configuration },
    LoweredPipelineStage::ParMap { slot, jobs, value } => ParMap {
        slot: usize,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    } => LoweredPipelineStage::ParMap { slot, jobs, value },
    LoweredPipelineStage::ParMapBlock {
        slot,
        body,
        jobs,
        value,
    } => ParMapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    } => LoweredPipelineStage::ParMapBlock {
        slot,
        body,
        jobs,
        value,
    },
    LoweredPipelineStage::ParMapFlatMapReduceBy {
        slot,
        body,
        jobs,
        value,
        flatten,
        reduce_item_slot,
        reduce_body,
        reduce_value,
        op,
    } => ParMapFlatMapReduceBy {
        slot: usize,
        body: Option<Vec<BuildStmtId>>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
        flatten: bool,
        reduce_item_slot: usize,
        reduce_body: Vec<BuildStmtId>,
        reduce_value: BuildExprId,
        op: ReduceByOp,
    } => LoweredPipelineStage::ParMapFlatMapReduceBy {
        slot,
        body,
        jobs,
        value,
        flatten,
        reduce_item_slot,
        reduce_body,
        reduce_value,
        op,
    },
    LoweredPipelineStage::Tee { slot, body } => Tee {
        slot: usize,
        body: Vec<BuildStmtId>,
    } => LoweredPipelineStage::Tee { slot, body },
    LoweredPipelineStage::Each {
        slot,
        body,
    } => Each {
        slot: usize,
        body: Vec<BuildStmtId>,
    } => LoweredPipelineStage::Each {
        slot,
        body,
    },
    LoweredPipelineStage::TablePrint { columns } => TablePrint {
        columns: Option<Vec<String>>,
    } => LoweredPipelineStage::TablePrint { columns },
    LoweredPipelineStage::TablePrintConfigured { columns } => TablePrintConfigured {
        columns: BuildExprId,
    } => LoweredPipelineStage::TablePrintConfigured { columns },
    LoweredPipelineStage::Enumerate => Enumerate {} => LoweredPipelineStage::Enumerate,
    LoweredPipelineStage::Zip { other } => Zip {
        other: BuildExprId,
    } => LoweredPipelineStage::Zip { other },
    LoweredPipelineStage::Sort { descending } => Sort {
        descending: Option<BuildExprId>,
    } => LoweredPipelineStage::Sort { descending },
    LoweredPipelineStage::SortBy {
        slot,
        key,
        descending,
    } => SortBy {
        slot: usize,
        key: BuildExprId,
        descending: Option<BuildExprId>,
    } => LoweredPipelineStage::SortBy {
        slot,
        key,
        descending,
    },
    LoweredPipelineStage::GroupBy { slot, key } => GroupBy {
        slot: usize,
        key: BuildExprId,
    } => LoweredPipelineStage::GroupBy { slot, key },
    LoweredPipelineStage::CountBy { slot, key } => CountBy {
        slot: usize,
        key: BuildExprId,
    } => LoweredPipelineStage::CountBy { slot, key },
    LoweredPipelineStage::Any { slot, predicate } => Any {
        slot: usize,
        predicate: BuildExprId,
    } => LoweredPipelineStage::Any { slot, predicate },
    LoweredPipelineStage::AnyBlock { slot, body, value } => AnyBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::AnyBlock { slot, body, value },
    LoweredPipelineStage::All { slot, predicate } => All {
        slot: usize,
        predicate: BuildExprId,
    } => LoweredPipelineStage::All { slot, predicate },
    LoweredPipelineStage::AllBlock { slot, body, value } => AllBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::AllBlock { slot, body, value },
    LoweredPipelineStage::UniqueBy { slot, key } => UniqueBy {
        slot: usize,
        key: BuildExprId,
    } => LoweredPipelineStage::UniqueBy { slot, key },
    LoweredPipelineStage::Count => Count {} => LoweredPipelineStage::Count,
    LoweredPipelineStage::Sum => Sum {} => LoweredPipelineStage::Sum,
    LoweredPipelineStage::Collect => Collect {} => LoweredPipelineStage::Collect,
    LoweredPipelineStage::First => First {} => LoweredPipelineStage::First,
    LoweredPipelineStage::Last => Last {} => LoweredPipelineStage::Last,
    LoweredPipelineStage::Min => Min {} => LoweredPipelineStage::Min,
    LoweredPipelineStage::Max => Max {} => LoweredPipelineStage::Max,
    LoweredPipelineStage::Take(value) => Take {
        value: BuildExprId,
    } => LoweredPipelineStage::Take(value),
    LoweredPipelineStage::Drop(value) => Drop {
        value: BuildExprId,
    } => LoweredPipelineStage::Drop(value),
    LoweredPipelineStage::Repeat { count } => Repeat {
        count: BuildExprId,
    } => LoweredPipelineStage::Repeat { count },
    LoweredPipelineStage::Range { start, end } => Range {
        start: BuildExprId,
        end: BuildExprId,
    } => LoweredPipelineStage::Range { start, end },
}
