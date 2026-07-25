use crate::source::{SourceId, Span};
use crate::symbol::{Name, Symbol};
use crate::syntax::node::{
    AssignOp, BinaryOp, BlockParam, CoreCommand, DurationLiteral, Effect, EnvGetKind, FloatLiteral,
    FormatSpec, FormatSpecKind, IntLiteral, RedirectionKind, RunKind, SignalHookOptions,
    StreamStageKind, UnaryOp,
};
use std::mem::size_of;
use std::num::NonZeroU32;
use std::sync::Arc;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub struct $name(NonZeroU32);

        impl $name {
            fn new(index: usize) -> Self {
                let raw = u32::try_from(index + 1).expect("AST arena exceeded u32 node ids");
                Self(NonZeroU32::new(raw).expect("arena ids are one-based"))
            }

            pub fn from_index(index: usize) -> Self {
                Self::new(index)
            }

            pub fn index(self) -> usize {
                self.0.get() as usize - 1
            }
        }
    };
}

macro_rules! lower_table_range {
    ($lowerer:ident, $items:expr, $table:ident, $item:ident, $lowered:expr) => {{
        let items = $items;
        let start = match items {
            [] => $lowerer.arena.$table.len(),
            [$item] => {
                let lowered = $lowered;
                let start = $lowerer.arena.$table.len();
                $lowerer.arena.$table.push(lowered);
                start
            }
            [first, second] => {
                let first_lowered = {
                    let $item = first;
                    $lowered
                };
                let second_lowered = {
                    let $item = second;
                    $lowered
                };
                let start = $lowerer.arena.$table.len();
                $lowerer.arena.$table.push(first_lowered);
                $lowerer.arena.$table.push(second_lowered);
                start
            }
            [first, second, third] => {
                let first_lowered = {
                    let $item = first;
                    $lowered
                };
                let second_lowered = {
                    let $item = second;
                    $lowered
                };
                let third_lowered = {
                    let $item = third;
                    $lowered
                };
                let start = $lowerer.arena.$table.len();
                $lowerer.arena.$table.push(first_lowered);
                $lowerer.arena.$table.push(second_lowered);
                $lowerer.arena.$table.push(third_lowered);
                start
            }
            [first, second, third, fourth] => {
                let first_lowered = {
                    let $item = first;
                    $lowered
                };
                let second_lowered = {
                    let $item = second;
                    $lowered
                };
                let third_lowered = {
                    let $item = third;
                    $lowered
                };
                let fourth_lowered = {
                    let $item = fourth;
                    $lowered
                };
                let start = $lowerer.arena.$table.len();
                $lowerer.arena.$table.push(first_lowered);
                $lowerer.arena.$table.push(second_lowered);
                $lowerer.arena.$table.push(third_lowered);
                $lowerer.arena.$table.push(fourth_lowered);
                start
            }
            _ => {
                let mut lowered = Vec::with_capacity(items.len());
                for $item in items {
                    lowered.push($lowered);
                }
                let start = $lowerer.arena.$table.len();
                $lowerer.arena.$table.extend(lowered);
                start
            }
        };
        Self::pushed_range(&mut $lowerer.arena.$table, start)
    }};
}

id_type!(SpanId);
id_type!(StmtId);
id_type!(BlockId);
id_type!(ExprId);
id_type!(PatternId);
id_type!(BindingTargetId);
id_type!(AssignTargetId);
id_type!(TypeExprId);
id_type!(RunFormId);
id_type!(BuilderBlockId);
id_type!(UseStmtId);
id_type!(TypeDefId);
id_type!(ErrorDefId);
id_type!(FunctionDefId);
id_type!(SignalHookId);
id_type!(CommandStmtId);
id_type!(IntLiteralId);
id_type!(FloatLiteralId);
id_type!(DurationLiteralId);
id_type!(StringLiteralId);
id_type!(BytesLiteralId);
id_type!(TextLiteralId);

impl TypeExprId {
    pub fn from_optional_raw(raw: u32) -> Option<Self> {
        (raw != ARENA_ABSENT).then(|| Self::from_index(raw as usize))
    }
}

#[cfg(test)]
mod layout_tests {
    use super::{
        ArenaExprData, ArenaExprTag, ArenaStmtData, ArenaStmtTag, ArenaTypeExprData,
        ArenaTypeExprTag, ExprId, PatternId, StmtId, TypeExprId,
    };
    use std::mem::size_of;

    #[test]
    fn compact_arena_rows_keep_fixed_layouts() {
        assert_eq!(size_of::<StmtId>(), 4);
        assert_eq!(size_of::<ExprId>(), 4);
        assert_eq!(size_of::<PatternId>(), 4);
        assert_eq!(size_of::<TypeExprId>(), 4);
        assert_eq!(size_of::<ArenaStmtTag>(), 1);
        assert_eq!(size_of::<ArenaExprTag>(), 1);
        assert_eq!(size_of::<ArenaTypeExprTag>(), 1);
        assert_eq!(size_of::<ArenaStmtData>(), 8);
        assert_eq!(size_of::<ArenaExprData>(), 8);
        assert_eq!(size_of::<ArenaTypeExprData>(), 8);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArenaRange {
    pub start: u32,
    pub len: u32,
}

impl ArenaRange {
    fn new(start: usize, len: usize) -> Self {
        Self {
            start: u32::try_from(start).expect("AST arena exceeded u32 list offsets"),
            len: u32::try_from(len).expect("AST arena exceeded u32 list lengths"),
        }
    }

    pub fn len(self) -> usize {
        self.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaByteSpan {
    pub start: u32,
    pub len: u32,
}

impl ArenaByteSpan {
    fn from_span(span: Span) -> Self {
        Self {
            start: raw_index(span.start()),
            len: raw_index(span.end() - span.start()),
        }
    }

    fn to_span(self, source_id: SourceId) -> Span {
        let start = self.start as usize;
        Span::new(source_id, start, start + self.len as usize)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaByteSpan16 {
    start: u16,
    len: u16,
}

impl ArenaByteSpan16 {
    fn try_from_span(span: ArenaByteSpan) -> Option<Self> {
        Some(Self {
            start: u16::try_from(span.start).ok()?,
            len: u16::try_from(span.len).ok()?,
        })
    }

    fn to_span(self) -> ArenaByteSpan {
        ArenaByteSpan {
            start: self.start as u32,
            len: self.len as u32,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaByteSpans {
    U16(Vec<ArenaByteSpan16>),
    U16Checked(Vec<ArenaByteSpan16>),
    U32(Vec<ArenaByteSpan>),
}

impl Default for ArenaByteSpans {
    fn default() -> Self {
        Self::U16Checked(Vec::new())
    }
}

impl ArenaByteSpans {
    fn for_source_len(source_len: usize) -> Self {
        if u16::try_from(source_len).is_ok() {
            Self::U16(Vec::new())
        } else {
            Self::U32(Vec::new())
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::U16(spans) => spans.len(),
            Self::U16Checked(spans) => spans.len(),
            Self::U32(spans) => spans.len(),
        }
    }

    fn reserve(&mut self, additional: usize) {
        match self {
            Self::U16(spans) => spans.reserve(additional),
            Self::U16Checked(spans) => spans.reserve(additional),
            Self::U32(spans) => spans.reserve(additional),
        }
    }

    fn push(&mut self, span: ArenaByteSpan) {
        match self {
            Self::U16(spans) => {
                debug_assert!(ArenaByteSpan16::try_from_span(span).is_some());
                spans.push(ArenaByteSpan16 {
                    start: span.start as u16,
                    len: span.len as u16,
                });
            }
            Self::U16Checked(spans) => {
                if let Some(span) = ArenaByteSpan16::try_from_span(span) {
                    spans.push(span);
                    return;
                }
                let mut promoted = Vec::with_capacity(spans.capacity().max(spans.len() + 1));
                promoted.extend(spans.iter().map(|span| span.to_span()));
                promoted.push(span);
                *self = Self::U32(promoted);
            }
            Self::U32(spans) => spans.push(span),
        }
    }

    fn get(&self, index: usize) -> ArenaByteSpan {
        match self {
            Self::U16(spans) => spans[index].to_span(),
            Self::U16Checked(spans) => spans[index].to_span(),
            Self::U32(spans) => spans[index],
        }
    }

    fn shift_starts_since(&mut self, start: usize, offset: u32) {
        if offset == 0 || start >= self.len() {
            return;
        }
        match self {
            Self::U16(spans) | Self::U16Checked(spans) => {
                if spans[start..]
                    .iter()
                    .all(|span| u32::from(span.start).saturating_add(offset) <= u32::from(u16::MAX))
                {
                    for span in &mut spans[start..] {
                        span.start = (u32::from(span.start) + offset) as u16;
                    }
                    return;
                }
                let mut promoted = Vec::with_capacity(spans.capacity());
                for (index, span) in spans.iter().enumerate() {
                    let mut span = span.to_span();
                    if index >= start {
                        span.start += offset;
                    }
                    promoted.push(span);
                }
                *self = Self::U32(promoted);
            }
            Self::U32(spans) => {
                for span in &mut spans[start..] {
                    span.start += offset;
                }
            }
        }
    }

    fn retained_bytes(&self) -> usize {
        match self {
            Self::U16(spans) => spans.capacity() * size_of::<ArenaByteSpan16>(),
            Self::U16Checked(spans) => spans.capacity() * size_of::<ArenaByteSpan16>(),
            Self::U32(spans) => spans.capacity() * size_of::<ArenaByteSpan>(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaSpanSource {
    pub span: u32,
    pub source_id: SourceId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ArenaTypeExprTag {
    Named,
    Qualified,
    List,
    Map,
    Stream,
    Module,
    Result,
    Optional,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaTypeExprData {
    pub lhs: u32,
    pub rhs: u32,
}

impl ArenaTypeExprData {
    const fn new(lhs: u32, rhs: u32) -> Self {
        Self { lhs, rhs }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArenaProgram {
    pub arena: AstArena,
    pub statements: ArenaRange,
    pub modules: Vec<ArenaUserModule>,
}

impl ArenaProgram {
    pub fn stats(&self) -> ArenaStats {
        ArenaStats {
            modules: self.modules.len(),
            statements: self.arena.stmt_tags.len(),
            blocks: self.arena.blocks.len(),
            expressions: self.arena.expr_tags.len(),
            patterns: self.arena.patterns.len(),
            binding_targets: self.arena.binding_targets.len(),
            assign_targets: self.arena.assign_targets.len(),
            type_exprs: self.arena.type_expr_tags.len(),
            use_stmts: self.arena.use_stmts.len(),
            type_defs: self.arena.type_defs.len(),
            error_defs: self.arena.error_defs.len(),
            function_defs: self.arena.function_defs.len(),
            signal_hooks: self.arena.signal_hooks.len(),
            command_stmts: self.arena.command_stmts.len(),
            int_literals: self.arena.int_literals.len(),
            float_literals: self.arena.float_literals.len(),
            duration_literals: self.arena.duration_literals.len(),
            string_literals: self.arena.string_literals.len(),
            bytes_literals: self.arena.bytes_literals.len(),
            text_literals: self.arena.text_tags.len(),
            source_text_literals: self.arena.source_text_literals(),
            cooked_text_literals: self.arena.cooked_texts.len(),
            run_forms: self.arena.run_forms.len(),
            builder_blocks: self.arena.builder_blocks.len(),
            spans: self.arena.spans.len(),
            span_source_overrides: self.arena.span_source_overrides(),
            extra_items: self.arena.extra.len(),
            fmt_parts: self.arena.fmt_part_tags.len(),
            command_args: self.arena.command_args.len(),
            word_parts: self.arena.word_part_tags.len(),
            list_items: self.arena.list_items(),
            span_storage_bytes: self.arena.span_storage_bytes(),
            stmt_storage_bytes: self.arena.stmt_storage_bytes(),
            expr_storage_bytes: self.arena.expr_storage_bytes(),
            type_expr_storage_bytes: self.arena.type_expr_storage_bytes(),
            extra_storage_bytes: self.arena.extra_storage_bytes(),
            text_storage_bytes: self.arena.text_storage_bytes(),
            cooked_text_storage_bytes: self.arena.cooked_text_storage_bytes(),
            definition_storage_bytes: self.arena.definition_storage_bytes(),
            literal_storage_bytes: self.arena.literal_storage_bytes(),
            pattern_storage_bytes: self.arena.pattern_storage_bytes(),
            block_storage_bytes: self.arena.block_storage_bytes(),
            control_storage_bytes: self.arena.control_storage_bytes(),
            call_record_storage_bytes: self.arena.call_record_storage_bytes(),
            builder_storage_bytes: self.arena.builder_storage_bytes(),
            command_storage_bytes: self.arena.command_storage_bytes(),
            side_table_storage_bytes: self.arena.side_table_storage_bytes(),
            retained_bytes: self.retained_bytes(),
        }
    }

    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.modules.capacity() * size_of::<ArenaUserModule>()
            + self.arena.capacity_bytes()
    }

    pub fn statement_ids(&self) -> impl Iterator<Item = StmtId> + '_ {
        self.arena.stmt_ids(self.statements)
    }

    pub fn source_text_source_id(&self) -> Option<SourceId> {
        self.arena.span_source_id
    }

    pub fn module_statements(&self, module: &ArenaUserModule) -> impl Iterator<Item = StmtId> + '_ {
        self.arena.stmt_ids(module.statements)
    }
}

#[derive(Default)]
pub struct ArenaProgramBuilder<'a> {
    lowerer: ArenaLowerer<'a>,
    statements: Vec<StmtId>,
    block_statements: Vec<StmtId>,
    block_statement_starts: Vec<usize>,
    record_field_inputs: Vec<ArenaRecordFieldInput>,
    record_field_input_starts: Vec<usize>,
    call_arg_inputs: Vec<ArenaCallArgInput>,
    call_arg_input_starts: Vec<usize>,
    if_expr_branch_inputs: Vec<ArenaIfExprBranch>,
    if_expr_branch_input_starts: Vec<usize>,
    expr_id_inputs: Vec<ExprId>,
    expr_id_input_starts: Vec<usize>,
    match_arm_inputs: Vec<ArenaMatchArm>,
    match_arm_input_starts: Vec<usize>,
    match_expr_arm_inputs: Vec<ArenaMatchExprArm>,
    match_expr_arm_input_starts: Vec<usize>,
    run_segment_inputs: Vec<ArenaRunSegment>,
    run_segment_input_starts: Vec<usize>,
    destructure_field_inputs: Vec<ArenaDestructureField>,
    destructure_field_input_starts: Vec<usize>,
    command_arg_inputs: Vec<ArenaCommandArg>,
    command_arg_input_starts: Vec<usize>,
    env_assignment_inputs: Vec<ArenaEnvAssignment>,
    env_assignment_input_starts: Vec<usize>,
    redirection_inputs: Vec<ArenaRedirection>,
    redirection_input_starts: Vec<usize>,
    word_part_inputs: Vec<(ArenaWordPartTag, ArenaWordPartData)>,
    word_part_input_starts: Vec<usize>,
    fmt_part_inputs: Vec<(ArenaFmtPartTag, ArenaFmtPartData)>,
    fmt_part_input_starts: Vec<usize>,
    stream_stage_option_inputs: Vec<(Name, Option<ExprId>, Span)>,
    stream_stage_option_input_starts: Vec<usize>,
    builder_entry_inputs: Vec<ArenaBuilderEntry>,
    builder_entry_input_starts: Vec<usize>,
    modules: Vec<ArenaUserModule>,
}

impl<'a> ArenaProgramBuilder<'a> {
    pub fn with_token_capacity(tokens: usize) -> Self {
        let mut lowerer = ArenaLowerer::default();
        lowerer.reserve_frontend_capacity(tokens);
        Self {
            lowerer,
            statements: Vec::with_capacity(tokens / 16 + 1),
            block_statements: Vec::with_capacity(tokens / 12 + 1),
            block_statement_starts: Vec::new(),
            record_field_inputs: Vec::with_capacity(tokens / 32 + 1),
            record_field_input_starts: Vec::new(),
            call_arg_inputs: Vec::with_capacity(tokens / 24 + 1),
            call_arg_input_starts: Vec::new(),
            if_expr_branch_inputs: Vec::with_capacity(tokens / 64 + 1),
            if_expr_branch_input_starts: Vec::new(),
            expr_id_inputs: Vec::with_capacity(tokens / 32 + 1),
            expr_id_input_starts: Vec::new(),
            match_arm_inputs: Vec::with_capacity(tokens / 64 + 1),
            match_arm_input_starts: Vec::new(),
            match_expr_arm_inputs: Vec::with_capacity(tokens / 64 + 1),
            match_expr_arm_input_starts: Vec::new(),
            run_segment_inputs: Vec::with_capacity(tokens / 96 + 1),
            run_segment_input_starts: Vec::new(),
            destructure_field_inputs: Vec::new(),
            destructure_field_input_starts: Vec::new(),
            command_arg_inputs: Vec::new(),
            command_arg_input_starts: Vec::new(),
            env_assignment_inputs: Vec::new(),
            env_assignment_input_starts: Vec::new(),
            redirection_inputs: Vec::new(),
            redirection_input_starts: Vec::new(),
            word_part_inputs: Vec::new(),
            word_part_input_starts: Vec::new(),
            fmt_part_inputs: Vec::new(),
            fmt_part_input_starts: Vec::new(),
            stream_stage_option_inputs: Vec::new(),
            stream_stage_option_input_starts: Vec::new(),
            builder_entry_inputs: Vec::new(),
            builder_entry_input_starts: Vec::new(),
            modules: Vec::new(),
        }
    }

    pub fn with_source_and_token_capacity(source: &'a str, tokens: usize) -> Self {
        let mut lowerer = ArenaLowerer {
            arena: AstArena::with_source_len(source.len()),
            source: Some(source),
        };
        lowerer.reserve_frontend_capacity(tokens);
        Self {
            lowerer,
            statements: Vec::with_capacity(tokens / 16 + 1),
            block_statements: Vec::with_capacity(tokens / 12 + 1),
            block_statement_starts: Vec::new(),
            record_field_inputs: Vec::with_capacity(tokens / 32 + 1),
            record_field_input_starts: Vec::new(),
            call_arg_inputs: Vec::with_capacity(tokens / 24 + 1),
            call_arg_input_starts: Vec::new(),
            if_expr_branch_inputs: Vec::with_capacity(tokens / 64 + 1),
            if_expr_branch_input_starts: Vec::new(),
            expr_id_inputs: Vec::with_capacity(tokens / 32 + 1),
            expr_id_input_starts: Vec::new(),
            match_arm_inputs: Vec::with_capacity(tokens / 64 + 1),
            match_arm_input_starts: Vec::new(),
            match_expr_arm_inputs: Vec::with_capacity(tokens / 64 + 1),
            match_expr_arm_input_starts: Vec::new(),
            run_segment_inputs: Vec::with_capacity(tokens / 96 + 1),
            run_segment_input_starts: Vec::new(),
            destructure_field_inputs: Vec::new(),
            destructure_field_input_starts: Vec::new(),
            command_arg_inputs: Vec::new(),
            command_arg_input_starts: Vec::new(),
            env_assignment_inputs: Vec::new(),
            env_assignment_input_starts: Vec::new(),
            redirection_inputs: Vec::new(),
            redirection_input_starts: Vec::new(),
            word_part_inputs: Vec::new(),
            word_part_input_starts: Vec::new(),
            fmt_part_inputs: Vec::new(),
            fmt_part_input_starts: Vec::new(),
            stream_stage_option_inputs: Vec::new(),
            stream_stage_option_input_starts: Vec::new(),
            builder_entry_inputs: Vec::new(),
            builder_entry_input_starts: Vec::new(),
            modules: Vec::new(),
        }
    }

    fn push_current_statement(&mut self, id: StmtId) {
        if self.block_statement_starts.is_empty() {
            self.statements.push(id);
        } else {
            self.block_statements.push(id);
        }
    }

    /// Inverse of `push_current_statement` — for contexts like builder-block
    /// `Stmt` entries, which want the `StmtId` a `parse_statement_arena_only`
    /// call just produced without leaving it registered on the enclosing
    /// block's/root's statement list. Must be called immediately after the
    /// one `parse_statement_arena_only` call that produced it, with no other
    /// statement push in between.
    pub fn pop_last_statement(&mut self) -> StmtId {
        if self.block_statement_starts.is_empty() {
            self.statements
                .pop()
                .expect("pop_last_statement called without a preceding statement push")
        } else {
            self.block_statements
                .pop()
                .expect("pop_last_statement called without a preceding statement push")
        }
    }

    pub fn expr_kind(&self, id: ExprId) -> ArenaExprKind {
        self.lowerer.arena.expr(id).kind
    }

    pub fn root_statement_count(&self) -> usize {
        self.statements.len()
    }

    pub fn finish_root_statements_from(&mut self, start: usize) -> ArenaRange {
        let statements = self.lowerer.lower_stmt_id_range(&self.statements[start..]);
        self.statements.truncate(start);
        statements
    }

    pub fn statement_ids(&self, range: ArenaRange) -> Vec<StmtId> {
        self.lowerer.arena.stmt_ids(range).collect()
    }

    pub fn use_stmt_for_statement(&self, stmt: StmtId) -> Option<(UseStmtId, Vec<Name>, Span)> {
        let stmt = self.lowerer.arena.stmt(stmt);
        let ArenaStmtKind::Use(use_id) = stmt.kind else {
            return None;
        };
        let use_stmt = self.lowerer.arena.use_stmt(use_id);
        Some((
            use_id,
            self.lowerer.arena.names(use_stmt.path).collect(),
            stmt.span,
        ))
    }

    pub fn set_use_resolved(&mut self, use_id: UseStmtId, key: Arc<str>) {
        self.lowerer.arena.use_stmts[use_id.index()].resolved = Some(key);
    }

    pub fn push_arena_module(&mut self, key: String, name: Name, statements: ArenaRange) {
        self.modules.push(ArenaUserModule {
            key,
            name,
            statements,
        });
    }

    pub fn begin_block(&mut self) {
        self.block_statement_starts
            .push(self.block_statements.len());
    }

    pub fn finish_block(&mut self, params: &[BlockParam], span: Span) -> BlockId {
        let start = self
            .block_statement_starts
            .pop()
            .expect("finish_block called without begin_block");
        let id =
            self.lowerer
                .push_block_from_stmt_ids(params, &self.block_statements[start..], span);
        self.block_statements.truncate(start);
        id
    }

    pub fn discard_block(&mut self) {
        let start = self
            .block_statement_starts
            .pop()
            .expect("discard_block called without begin_block");
        self.block_statements.truncate(start);
    }

    /// If the last statement of the current (innermost) block is a bare,
    /// no-argument plain-identifier proc command, return its name so the caller
    /// can retag it as a tail-bare-ident. Arena-native equivalent of the old-AST
    /// `mark_tail_bare_ident` post-pass.
    pub fn current_block_tail_bare_ident_name(&self) -> Option<Name> {
        let start = *self.block_statement_starts.last()?;
        let id = self.block_statements.get(start..)?.last().copied()?;
        let ArenaStmtKind::Command(cmd_id) = self.lowerer.arena.stmt(id).kind else {
            return None;
        };
        let cmd = self.lowerer.arena.command_stmt(cmd_id);
        if cmd.propagate {
            return None;
        }
        let ArenaCommand::Proc { name, args } = &cmd.command else {
            return None;
        };
        let name = *name;
        if !self.lowerer.arena.command_args(*args).is_empty() {
            return None;
        }
        let text = name.as_str();
        let mut bytes = text.bytes();
        let first = bytes.next()?;
        if (first.is_ascii_alphabetic() || first == b'_')
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            Some(name)
        } else {
            None
        }
    }

    pub fn mark_current_tail_bare_ident(&mut self, name: Name) {
        let Some(id) = self
            .block_statement_starts
            .last()
            .and_then(|start| self.block_statements.get(*start..))
            .and_then(|statements| statements.last())
            .copied()
        else {
            return;
        };
        self.lowerer.retag_stmt_tail_bare_ident(id, name);
    }

    pub fn begin_record_fields(&mut self) {
        self.record_field_input_starts
            .push(self.record_field_inputs.len());
    }

    pub fn push_record_field_input(&mut self, field: ArenaRecordFieldInput) {
        self.record_field_inputs.push(field);
    }

    pub fn finish_record_fields(&mut self) -> ArenaRange {
        let start = self
            .record_field_input_starts
            .pop()
            .expect("finish_record_fields called without begin_record_fields");
        let range = self
            .lowerer
            .lower_record_field_input_range(&self.record_field_inputs[start..]);
        self.record_field_inputs.truncate(start);
        range
    }

    pub fn discard_record_fields(&mut self) {
        let start = self
            .record_field_input_starts
            .pop()
            .expect("discard_record_fields called without begin_record_fields");
        self.record_field_inputs.truncate(start);
    }

    pub fn begin_call_args(&mut self) {
        self.call_arg_input_starts.push(self.call_arg_inputs.len());
    }

    pub fn push_call_arg_input(&mut self, arg: ArenaCallArgInput) {
        self.call_arg_inputs.push(arg);
    }

    pub fn finish_call_args(&mut self) -> ArenaRange {
        let start = self
            .call_arg_input_starts
            .pop()
            .expect("finish_call_args called without begin_call_args");
        let range = self
            .lowerer
            .commit_call_arg_input_range(&self.call_arg_inputs[start..]);
        self.call_arg_inputs.truncate(start);
        range
    }

    pub fn begin_if_expr_branches(&mut self) {
        self.if_expr_branch_input_starts
            .push(self.if_expr_branch_inputs.len());
    }

    pub fn push_if_expr_branch_input(&mut self, condition: ExprId, value: ExprId) {
        self.if_expr_branch_inputs
            .push(ArenaIfExprBranch { condition, value });
    }

    pub fn finish_if_expr_branches(&mut self) -> ArenaRange {
        let start = self
            .if_expr_branch_input_starts
            .pop()
            .expect("finish_if_expr_branches called without begin_if_expr_branches");
        let range = self
            .lowerer
            .lower_if_expr_branch_id_range(&self.if_expr_branch_inputs[start..]);
        self.if_expr_branch_inputs.truncate(start);
        range
    }

    pub fn discard_if_expr_branches(&mut self) {
        let start = self
            .if_expr_branch_input_starts
            .pop()
            .expect("discard_if_expr_branches called without begin_if_expr_branches");
        self.if_expr_branch_inputs.truncate(start);
    }

    pub fn begin_expr_ids(&mut self) {
        self.expr_id_input_starts.push(self.expr_id_inputs.len());
    }

    pub fn push_expr_id_input(&mut self, id: ExprId) {
        self.expr_id_inputs.push(id);
    }

    pub fn finish_expr_ids(&mut self) -> ArenaRange {
        let start = self
            .expr_id_input_starts
            .pop()
            .expect("finish_expr_ids called without begin_expr_ids");
        let range = self
            .lowerer
            .lower_expr_id_range(&self.expr_id_inputs[start..]);
        self.expr_id_inputs.truncate(start);
        range
    }

    pub fn discard_expr_ids(&mut self) {
        let start = self
            .expr_id_input_starts
            .pop()
            .expect("discard_expr_ids called without begin_expr_ids");
        self.expr_id_inputs.truncate(start);
    }

    pub fn begin_match_arms(&mut self) {
        self.match_arm_input_starts
            .push(self.match_arm_inputs.len());
    }

    pub fn push_match_arm_input_id(
        &mut self,
        pattern: PatternId,
        guard: Option<ExprId>,
        block: BlockId,
        span: Span,
    ) {
        let span = self.lowerer.span(span);
        self.match_arm_inputs.push(ArenaMatchArm {
            pattern,
            guard,
            block,
            span,
        });
    }

    pub fn finish_match_arms(&mut self) -> ArenaRange {
        let start = self
            .match_arm_input_starts
            .pop()
            .expect("finish_match_arms called without begin_match_arms");
        let range = self
            .lowerer
            .lower_match_arm_id_range(&self.match_arm_inputs[start..]);
        self.match_arm_inputs.truncate(start);
        range
    }

    pub fn begin_match_expr_arms(&mut self) {
        self.match_expr_arm_input_starts
            .push(self.match_expr_arm_inputs.len());
    }

    pub fn push_match_expr_arm_input_id(
        &mut self,
        pattern: PatternId,
        guard: Option<ExprId>,
        value: ExprId,
        span: Span,
    ) {
        let span = self.lowerer.span(span);
        self.match_expr_arm_inputs.push(ArenaMatchExprArm {
            pattern,
            guard,
            value,
            span,
        });
    }

    /// Push an arena pattern node, returning its id. Children (sub-patterns,
    /// record fields) must already be built.
    fn push_pattern_kind(&mut self, kind: ArenaPatternKind, span: Span) -> PatternId {
        let id = PatternId::new(self.lowerer.arena.patterns.len());
        let span = self.lowerer.span(span);
        self.lowerer
            .arena
            .patterns
            .push(ArenaPattern { kind, span });
        id
    }

    pub fn push_pattern_wildcard(&mut self, span: Span) -> PatternId {
        self.push_pattern_kind(ArenaPatternKind::Wildcard, span)
    }

    pub fn push_pattern_binding(&mut self, name: Name, span: Span) -> PatternId {
        self.push_pattern_kind(ArenaPatternKind::Binding(name), span)
    }

    pub fn push_pattern_type(
        &mut self,
        binding: Option<Name>,
        ty: TypeExprId,
        span: Span,
    ) -> PatternId {
        self.push_pattern_kind(ArenaPatternKind::Type { binding, ty }, span)
    }

    pub fn push_pattern_literal(&mut self, expr: ExprId, span: Span) -> PatternId {
        self.push_pattern_kind(ArenaPatternKind::Literal(expr), span)
    }

    pub fn push_pattern_facet(&mut self, name: Name, span: Span) -> PatternId {
        self.push_pattern_kind(ArenaPatternKind::Facet(name), span)
    }

    pub fn push_pattern_constructor(
        &mut self,
        name: Name,
        arg: Option<PatternId>,
        span: Span,
    ) -> PatternId {
        self.push_pattern_kind(ArenaPatternKind::Constructor { name, arg }, span)
    }

    fn push_pattern_id_range(&mut self, patterns: &[PatternId]) -> ArenaRange {
        let start = self.lowerer.arena.extra.len();
        for pattern in patterns {
            self.lowerer.arena.extra.push(pattern.index() as u32);
        }
        ArenaRange::new(start, patterns.len())
    }

    pub fn push_pattern_alternation(&mut self, patterns: &[PatternId], span: Span) -> PatternId {
        let range = self.push_pattern_id_range(patterns);
        self.push_pattern_kind(ArenaPatternKind::Alternation(range), span)
    }

    pub fn push_pattern_tuple(&mut self, patterns: &[PatternId], span: Span) -> PatternId {
        let range = self.push_pattern_id_range(patterns);
        self.push_pattern_kind(ArenaPatternKind::Tuple(range), span)
    }

    fn push_record_pattern_field_range(
        &mut self,
        fields: &[(Name, PatternId, Span)],
    ) -> ArenaRange {
        let start = self.lowerer.arena.pattern_fields.len();
        for (name, pattern, span) in fields {
            let span = self.lowerer.span(*span);
            self.lowerer
                .arena
                .pattern_fields
                .push(ArenaRecordPatternField {
                    name: *name,
                    pattern: *pattern,
                    span,
                });
        }
        ArenaRange::new(start, fields.len())
    }

    pub fn push_pattern_record(
        &mut self,
        fields: &[(Name, PatternId, Span)],
        rest: bool,
        span: Span,
    ) -> PatternId {
        let range = self.push_record_pattern_field_range(fields);
        self.push_pattern_kind(
            ArenaPatternKind::Record {
                fields: range,
                rest,
            },
            span,
        )
    }

    pub fn push_pattern_error_variant(
        &mut self,
        family: Name,
        variant: Name,
        fields: &[(Name, PatternId, Span)],
        span: Span,
    ) -> PatternId {
        let range = self.push_record_pattern_field_range(fields);
        self.push_pattern_kind(
            ArenaPatternKind::ErrorVariant {
                family,
                variant,
                fields: range,
            },
            span,
        )
    }

    pub fn finish_match_expr_arms(&mut self) -> ArenaRange {
        let start = self
            .match_expr_arm_input_starts
            .pop()
            .expect("finish_match_expr_arms called without begin_match_expr_arms");
        let range = self
            .lowerer
            .lower_match_expr_arm_id_range(&self.match_expr_arm_inputs[start..]);
        self.match_expr_arm_inputs.truncate(start);
        range
    }

    pub fn begin_run_segments(&mut self) {
        self.run_segment_input_starts
            .push(self.run_segment_inputs.len());
    }

    pub fn push_run_segment_parts(
        &mut self,
        kind: RunKind,
        builtin: bool,
        timeout: Option<ExprId>,
        cpu_max: Option<ExprId>,
        env: ArenaRange,
        grouped: bool,
        target: ArenaCommandArg,
        args: ArenaRange,
        redirections: ArenaRange,
        span: Span,
    ) {
        let span = self.lowerer.span(span);
        self.run_segment_inputs.push(ArenaRunSegment {
            kind,
            builtin,
            timeout,
            cpu_max,
            env,
            grouped,
            target,
            args,
            redirections,
            span,
        });
    }

    pub fn finish_run_form(&mut self, propagate: bool, span: Span) -> RunFormId {
        let start = self
            .run_segment_input_starts
            .pop()
            .expect("finish_run_form called without begin_run_segments");
        let segments = self
            .lowerer
            .lower_run_segment_id_range(&self.run_segment_inputs[start..]);
        self.run_segment_inputs.truncate(start);
        self.lowerer.push_run_form_parts(segments, propagate, span)
    }

    pub fn set_run_form_propagate(&mut self, id: RunFormId, propagate: bool) {
        self.lowerer.arena.run_forms[id.index()].propagate = propagate;
    }

    pub fn discard_run_segments(&mut self) {
        let start = self
            .run_segment_input_starts
            .pop()
            .expect("discard_run_segments called without begin_run_segments");
        self.run_segment_inputs.truncate(start);
    }

    // Staged (not direct-append) for the same reason as fmt_part_inputs/
    // word_part_inputs: destructure targets/command args/env assignments/
    // redirections can all nest — an expression parsed while one of these
    // lists is "open" (e.g. a command arg's `${...}` interpolation, or an
    // env-assignment value, or a redirection target) can itself be, or
    // contain, another run-form/call with its own command args, env
    // assignments, redirections, or destructuring pattern. Direct-appending
    // to the permanent table and computing the finished range as
    // (mark, current_len) let an inner begin/finish pair's entries leak into
    // the still-open outer range — confirmed empirically for command_args
    // via a nested `${run echo "...${x}"}` command-word interpolation.
    pub fn begin_destructure_fields(&mut self) {
        self.destructure_field_input_starts
            .push(self.destructure_field_inputs.len());
    }

    pub fn push_destructure_field(&mut self, name: Name, span: Span) {
        let span = self.lowerer.span(span);
        self.destructure_field_inputs
            .push(ArenaDestructureField { name, span });
    }

    pub fn finish_destructure_fields(&mut self) -> ArenaRange {
        let start = self
            .destructure_field_input_starts
            .pop()
            .expect("finish_destructure_fields called without begin_destructure_fields");
        let range_start = self.lowerer.arena.destructure_fields.len();
        self.lowerer
            .arena
            .destructure_fields
            .extend(self.destructure_field_inputs.drain(start..));
        ArenaRange::new(
            range_start,
            self.lowerer.arena.destructure_fields.len() - range_start,
        )
    }

    pub fn discard_destructure_fields(&mut self) {
        let start = self
            .destructure_field_input_starts
            .pop()
            .expect("discard_destructure_fields called without begin_destructure_fields");
        self.destructure_field_inputs.truncate(start);
    }

    pub fn push_binding_target_name(&mut self, name: Name) -> BindingTargetId {
        self.push_binding_target_kind(ArenaBindingTargetKind::Name(name))
    }

    pub fn push_binding_target_record(
        &mut self,
        fields: ArenaRange,
        rest: bool,
    ) -> BindingTargetId {
        self.push_binding_target_kind(ArenaBindingTargetKind::Record { fields, rest })
    }

    fn push_binding_target_kind(&mut self, kind: ArenaBindingTargetKind) -> BindingTargetId {
        let id = BindingTargetId::new(self.lowerer.arena.binding_targets.len());
        self.lowerer
            .arena
            .binding_targets
            .push(ArenaBindingTarget { kind });
        id
    }

    pub fn begin_command_args(&mut self) {
        self.command_arg_input_starts
            .push(self.command_arg_inputs.len());
    }

    pub fn push_command_arg_input(&mut self, arg: ArenaCommandArg) {
        self.command_arg_inputs.push(arg);
    }

    pub fn finish_command_args(&mut self) -> ArenaRange {
        let start = self
            .command_arg_input_starts
            .pop()
            .expect("finish_command_args called without begin_command_args");
        let range_start = self.lowerer.arena.command_args.len();
        self.lowerer
            .arena
            .command_args
            .extend(self.command_arg_inputs.drain(start..));
        ArenaRange::new(
            range_start,
            self.lowerer.arena.command_args.len() - range_start,
        )
    }

    pub fn begin_env_assignments(&mut self) {
        self.env_assignment_input_starts
            .push(self.env_assignment_inputs.len());
    }

    pub fn push_env_assignment_input(
        &mut self,
        name: Name,
        value: ArenaEnvAssignmentValue,
        span: Span,
    ) {
        let span = self.lowerer.span(span);
        self.env_assignment_inputs
            .push(ArenaEnvAssignment { name, value, span });
    }

    pub fn finish_env_assignments(&mut self) -> ArenaRange {
        let start = self
            .env_assignment_input_starts
            .pop()
            .expect("finish_env_assignments called without begin_env_assignments");
        let range_start = self.lowerer.arena.env_assignments.len();
        self.lowerer
            .arena
            .env_assignments
            .extend(self.env_assignment_inputs.drain(start..));
        ArenaRange::new(
            range_start,
            self.lowerer.arena.env_assignments.len() - range_start,
        )
    }

    pub fn discard_env_assignments(&mut self) {
        let start = self
            .env_assignment_input_starts
            .pop()
            .expect("discard_env_assignments called without begin_env_assignments");
        self.env_assignment_inputs.truncate(start);
    }

    pub fn begin_redirections(&mut self) {
        self.redirection_input_starts
            .push(self.redirection_inputs.len());
    }

    pub fn push_redirection_input(
        &mut self,
        kind: RedirectionKind,
        target: ArenaRedirectionTarget,
        span: Span,
    ) {
        let span = self.lowerer.span(span);
        self.redirection_inputs
            .push(ArenaRedirection { kind, target, span });
    }

    pub fn finish_redirections(&mut self) -> ArenaRange {
        let start = self
            .redirection_input_starts
            .pop()
            .expect("finish_redirections called without begin_redirections");
        let range_start = self.lowerer.arena.redirections.len();
        self.lowerer
            .arena
            .redirections
            .extend(self.redirection_inputs.drain(start..));
        ArenaRange::new(
            range_start,
            self.lowerer.arena.redirections.len() - range_start,
        )
    }

    // Word parts can nest: a command-word `${...}` interpolation can contain
    // a run-form expression whose own command args are quoted strings with
    // their own `${...}` interpolation, recursing back into word-part
    // construction while the outer begin_word_parts() is still open. Staged
    // the same way as fmt_part_inputs, for the same reason.
    pub fn begin_word_parts(&mut self) {
        self.word_part_input_starts
            .push(self.word_part_inputs.len());
    }

    pub fn push_bare_word_part_source_span(&mut self, span: Span) {
        if self.extend_last_source_word_part(ArenaWordPartTag::Bare, span) {
            return;
        }
        let id = self.lowerer.push_source_text(span);
        self.word_part_inputs.push((
            ArenaWordPartTag::Bare,
            ArenaWordPartData::new(raw_text_literal_id(id), 0),
        ));
    }

    pub fn push_quoted_word_part_text(
        &mut self,
        value: &Arc<str>,
        container: Span,
        search_from: &mut usize,
        search_end: usize,
    ) {
        let id =
            self.lowerer
                .lower_text_literal(value, container.source_id, search_from, search_end);
        self.word_part_inputs.push((
            ArenaWordPartTag::Quoted,
            ArenaWordPartData::new(raw_text_literal_id(id), 0),
        ));
    }

    pub fn push_shorthand_word_part_expr(&mut self, expr: ExprId) {
        self.word_part_inputs.push((
            ArenaWordPartTag::Shorthand,
            ArenaWordPartData::new(raw_expr_id(expr), 0),
        ));
    }

    pub fn push_interpolation_word_part_expr(&mut self, expr: ExprId) {
        self.word_part_inputs.push((
            ArenaWordPartTag::Interpolation,
            ArenaWordPartData::new(raw_expr_id(expr), 0),
        ));
    }

    fn extend_last_source_word_part(&mut self, tag: ArenaWordPartTag, span: Span) -> bool {
        let Some(&range_start) = self.word_part_input_starts.last() else {
            return false;
        };
        let Some(last_index) = self.word_part_inputs.len().checked_sub(1) else {
            return false;
        };
        if last_index < range_start
            || self.word_part_inputs[last_index].0 != tag
            || self.lowerer.arena.span_source_id != Some(span.source_id)
        {
            return false;
        }
        let text_index = self.word_part_inputs[last_index].1.lhs as usize;
        if self.lowerer.arena.text_tags.get(text_index) != Some(&ArenaTextTag::Source) {
            return false;
        }
        let Some(data) = self.lowerer.arena.text_data.get_mut(text_index) else {
            return false;
        };
        let start = data.lhs as usize;
        let len = data.rhs as usize;
        if start + len != span.start() {
            return false;
        }
        data.rhs = raw_index(len + span.end() - span.start());
        true
    }

    pub fn finish_word_parts(&mut self) -> ArenaRange {
        let start = self
            .word_part_input_starts
            .pop()
            .expect("finish_word_parts called without begin_word_parts");
        let range_start = self.lowerer.arena.word_part_tags.len();
        for (tag, data) in self.word_part_inputs.drain(start..) {
            self.lowerer.arena.word_part_tags.push(tag);
            self.lowerer.arena.word_part_data.push(data);
        }
        ArenaRange::new(
            range_start,
            self.lowerer.arena.word_part_tags.len() - range_start,
        )
    }

    pub fn discard_word_parts(&mut self) {
        let start = self
            .word_part_input_starts
            .pop()
            .expect("discard_word_parts called without begin_word_parts");
        self.word_part_inputs.truncate(start);
    }

    // Fmt strings can nest (an interpolated `${...}` expression can itself be
    // another fmt string, parsed via a recursive sub-lexer). Stage parts in
    // `fmt_part_inputs` and only drain them into the permanent
    // `fmt_part_tags`/`fmt_part_data` columns at `finish_fmt_parts`, mirroring
    // `call_arg_inputs` — otherwise an inner begin/finish pair would claim
    // table slots that the still-open outer range later re-claims.
    pub fn begin_fmt_parts(&mut self) {
        self.fmt_part_input_starts.push(self.fmt_part_inputs.len());
    }

    pub fn push_fmt_text_part_source_span(&mut self, span: Span) {
        let id = self.lowerer.push_source_text(span);
        self.fmt_part_inputs.push((
            ArenaFmtPartTag::Text,
            ArenaFmtPartData::new(raw_text_literal_id(id), 0),
        ));
    }

    pub fn push_fmt_text_part_cooked(&mut self, value: &Arc<str>) {
        let id = self.lowerer.push_cooked_text(value);
        self.fmt_part_inputs.push((
            ArenaFmtPartTag::Text,
            ArenaFmtPartData::new(raw_text_literal_id(id), 0),
        ));
    }

    pub fn push_fmt_expr_part(&mut self, expr: ExprId, spec: Option<FormatSpec>) {
        self.fmt_part_inputs.push((
            ArenaFmtPartTag::Expr,
            ArenaFmtPartData::new(raw_expr_id(expr), raw_format_spec(spec.as_ref())),
        ));
    }

    pub fn finish_fmt_parts(&mut self) -> ArenaRange {
        let start = self
            .fmt_part_input_starts
            .pop()
            .expect("finish_fmt_parts called without begin_fmt_parts");
        let range_start = self.lowerer.arena.fmt_part_tags.len();
        for (tag, data) in self.fmt_part_inputs.drain(start..) {
            self.lowerer.arena.fmt_part_tags.push(tag);
            self.lowerer.arena.fmt_part_data.push(data);
        }
        ArenaRange::new(
            range_start,
            self.lowerer.arena.fmt_part_tags.len() - range_start,
        )
    }

    // Stream-stage options are staged too: an option's value can itself be an
    // arbitrary `${...}` expression containing another full pipeline with its
    // own options, so the same nesting hazard as fmt parts applies.
    pub fn begin_stream_stage_options(&mut self) {
        self.stream_stage_option_input_starts
            .push(self.stream_stage_option_inputs.len());
    }

    pub fn push_stream_stage_option_input(
        &mut self,
        name: Name,
        value: Option<ExprId>,
        span: Span,
    ) {
        self.stream_stage_option_inputs.push((name, value, span));
    }

    pub fn finish_stream_stage_options(&mut self) -> ArenaRange {
        let start = self
            .stream_stage_option_input_starts
            .pop()
            .expect("finish_stream_stage_options called without begin_stream_stage_options");
        let range_start = self.lowerer.arena.stream_options.len();
        for (name, value, span) in self.stream_stage_option_inputs.drain(start..) {
            let span = self.lowerer.span(span);
            self.lowerer
                .arena
                .stream_options
                .push(ArenaStreamStageOption { name, value, span });
        }
        ArenaRange::new(
            range_start,
            self.lowerer.arena.stream_options.len() - range_start,
        )
    }

    pub fn discard_stream_stage_options(&mut self) {
        let start = self
            .stream_stage_option_input_starts
            .pop()
            .expect("discard_stream_stage_options called without begin_stream_stage_options");
        self.stream_stage_option_inputs.truncate(start);
    }

    // Builder-block entries can nest (an `Entry` can itself carry a nested
    // builder block), so this needs the same stage-then-drain treatment as
    // fmt parts / stream-stage options.
    pub fn begin_builder_entries(&mut self) {
        self.builder_entry_input_starts
            .push(self.builder_entry_inputs.len());
    }

    pub fn push_builder_entry_input(&mut self, entry: ArenaBuilderEntry) {
        self.builder_entry_inputs.push(entry);
    }

    pub fn finish_builder_block(&mut self, span: Span) -> BuilderBlockId {
        let start = self
            .builder_entry_input_starts
            .pop()
            .expect("finish_builder_block called without begin_builder_entries");
        let range_start = self.lowerer.arena.builder_entries.len();
        for entry in self.builder_entry_inputs.drain(start..) {
            self.lowerer.arena.builder_entries.push(entry);
        }
        let entries = ArenaRange::new(
            range_start,
            self.lowerer.arena.builder_entries.len() - range_start,
        );
        let id = BuilderBlockId::new(self.lowerer.arena.builder_blocks.len());
        let span = self.lowerer.span(span);
        self.lowerer
            .arena
            .builder_blocks
            .push(ArenaBuilderBlock { entries, span });
        id
    }

    pub fn build_builder_entry(
        &mut self,
        kind: ArenaBuilderEntryKind,
        span: Span,
    ) -> ArenaBuilderEntry {
        ArenaBuilderEntry {
            kind,
            span: self.lowerer.span(span),
        }
    }

    pub fn push_builder_call_expr_id(
        &mut self,
        call: ExprId,
        block: BuilderBlockId,
        span: Span,
    ) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::BuilderCall { call, block }, span)
    }

    pub fn build_pipe_stage(&mut self, kind: ArenaPipeStageKind, span: Span) -> ArenaPipeStage {
        ArenaPipeStage {
            kind,
            span: self.lowerer.span(span),
        }
    }

    pub fn build_stream_stage(
        &mut self,
        kind: StreamStageKind,
        options: ArenaRange,
        block: Option<BlockId>,
        args: ArenaRange,
        span: Span,
    ) -> ArenaStreamStage {
        ArenaStreamStage {
            kind,
            options,
            block,
            args,
            span: self.lowerer.span(span),
        }
    }

    // Pipe stages accumulate in a plain `Vec` on the parser's call stack
    // across repeated `|>` occurrences (see `ArenaPendingPipeline` in
    // expr.rs) and are only committed here once, when the chain ends —
    // unlike the old with_arena bridge, which re-lowered the whole
    // accumulated tree from scratch on every additional stage.
    pub fn build_pipeline_expr(
        &mut self,
        input: ExprId,
        stages: Vec<ArenaPipeStage>,
        span: Span,
    ) -> ExprId {
        let start = self.lowerer.arena.pipe_stages.len();
        self.lowerer.arena.pipe_stages.extend(stages);
        let stages = ArenaRange::new(start, self.lowerer.arena.pipe_stages.len() - start);
        self.lowerer
            .push_expr_kind(ArenaExprKind::Pipeline { input, stages }, span)
    }

    pub fn build_structured_pipeline_expr(
        &mut self,
        input: ExprId,
        stages: Vec<ArenaStreamStage>,
        span: Span,
    ) -> ExprId {
        let start = self.lowerer.arena.stream_stages.len();
        self.lowerer.arena.stream_stages.extend(stages);
        let stages = ArenaRange::new(start, self.lowerer.arena.stream_stages.len() - start);
        self.lowerer
            .push_expr_kind(ArenaExprKind::StructuredPipeline { input, stages }, span)
    }

    /// Snapshot the lengths of the per-kind span columns that an arbitrary
    /// sub-expression parse can append to, so the appended range can later be
    /// shifted from chunk-relative to file-absolute byte offsets (used when
    /// parsing `${...}` interpolation expressions out of a fmt-string token
    /// via a throwaway sub-lexer/parser positioned at offset 0).
    pub fn span_marks(&self) -> ArenaSpanMarks {
        ArenaSpanMarks {
            spans: self.lowerer.arena.spans.len(),
            expr_spans: self.lowerer.arena.expr_spans.len(),
            stmt_spans: self.lowerer.arena.stmt_spans.len(),
            type_expr_spans: self.lowerer.arena.type_expr_spans.len(),
            text_data: self.lowerer.arena.text_data.len(),
        }
    }

    pub fn shift_spans_since(&mut self, marks: ArenaSpanMarks, offset: usize) {
        let offset = offset as u32;
        self.lowerer
            .arena
            .spans
            .shift_starts_since(marks.spans, offset);
        self.lowerer
            .arena
            .expr_spans
            .shift_starts_since(marks.expr_spans, offset);
        self.lowerer
            .arena
            .stmt_spans
            .shift_starts_since(marks.stmt_spans, offset);
        self.lowerer
            .arena
            .type_expr_spans
            .shift_starts_since(marks.type_expr_spans, offset);
        // Source-backed text literals (fmt-string and word-part text chunks)
        // store an absolute byte range directly in `text_data`, not via
        // SpanId — shift those too, but only `Source`-tagged entries; `Cooked`
        // entries' `lhs` is an index into `cooked_texts`, not a byte offset.
        for (tag, data) in self.lowerer.arena.text_tags[marks.text_data..]
            .iter()
            .zip(&mut self.lowerer.arena.text_data[marks.text_data..])
        {
            if *tag == ArenaTextTag::Source {
                data.lhs += offset;
            }
        }
    }

    pub fn word_command_arg(&mut self, parts: ArenaRange, span: Span) -> ArenaCommandArg {
        ArenaCommandArg {
            kind: ArenaCommandArgKind::Word(parts),
            span: self.lowerer.span(span),
        }
    }

    pub fn splice_name_command_arg(&mut self, name: Name, span: Span) -> ArenaCommandArg {
        ArenaCommandArg {
            kind: ArenaCommandArgKind::SpliceName(name),
            span: self.lowerer.span(span),
        }
    }

    pub fn splice_expr_command_arg(&mut self, expr: ExprId, span: Span) -> ArenaCommandArg {
        ArenaCommandArg {
            kind: ArenaCommandArgKind::SpliceExpr(expr),
            span: self.lowerer.span(span),
        }
    }

    pub fn typed_command_arg(&mut self, expr: ExprId, span: Span) -> ArenaCommandArg {
        ArenaCommandArg {
            kind: ArenaCommandArgKind::Typed(expr),
            span: self.lowerer.span(span),
        }
    }

    pub fn push_binding_parts(
        &mut self,
        immutable: bool,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        span: Span,
    ) -> StmtId {
        let kind = if immutable {
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            }
        } else {
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            }
        };
        let id = self.lowerer.push_stmt_kind(kind, span);
        self.push_current_statement(id);
        id
    }

    fn push_type_expr_row(
        &mut self,
        tag: ArenaTypeExprTag,
        data: ArenaTypeExprData,
        span: Span,
    ) -> TypeExprId {
        self.lowerer.push_type_expr_row(tag, data, span)
    }

    pub fn push_named_type_expr(&mut self, name: Name, span: Span) -> TypeExprId {
        self.push_type_expr_row(
            ArenaTypeExprTag::Named,
            ArenaTypeExprData::new(name.symbol().raw(), 0),
            span,
        )
    }

    pub fn push_qualified_type_expr(
        &mut self,
        namespace: Name,
        name: Name,
        span: Span,
    ) -> TypeExprId {
        self.push_type_expr_row(
            ArenaTypeExprTag::Qualified,
            ArenaTypeExprData::new(namespace.symbol().raw(), name.symbol().raw()),
            span,
        )
    }

    pub fn push_list_type_expr(&mut self, inner: TypeExprId, span: Span) -> TypeExprId {
        self.push_type_expr_row(
            ArenaTypeExprTag::List,
            ArenaTypeExprData::new(raw_type_expr_id(inner), 0),
            span,
        )
    }

    pub fn push_map_type_expr(&mut self, inner: TypeExprId, span: Span) -> TypeExprId {
        self.push_type_expr_row(
            ArenaTypeExprTag::Map,
            ArenaTypeExprData::new(raw_type_expr_id(inner), 0),
            span,
        )
    }

    pub fn push_stream_type_expr(&mut self, inner: TypeExprId, span: Span) -> TypeExprId {
        self.push_type_expr_row(
            ArenaTypeExprTag::Stream,
            ArenaTypeExprData::new(raw_type_expr_id(inner), 0),
            span,
        )
    }

    pub fn push_module_type_expr(&mut self, inner: TypeExprId, span: Span) -> TypeExprId {
        self.push_type_expr_row(
            ArenaTypeExprTag::Module,
            ArenaTypeExprData::new(raw_type_expr_id(inner), 0),
            span,
        )
    }

    pub fn push_result_type_expr(
        &mut self,
        ok: TypeExprId,
        err: Option<TypeExprId>,
        span: Span,
    ) -> TypeExprId {
        self.push_type_expr_row(
            ArenaTypeExprTag::Result,
            ArenaTypeExprData::new(raw_type_expr_id(ok), optional_raw_type_expr_id(err)),
            span,
        )
    }

    pub fn push_optional_type_expr(&mut self, inner: TypeExprId, span: Span) -> TypeExprId {
        self.push_type_expr_row(
            ArenaTypeExprTag::Optional,
            ArenaTypeExprData::new(raw_type_expr_id(inner), 0),
            span,
        )
    }

    pub fn run_expr_or_run(&mut self, run: RunFormId) -> ArenaExprOrRun {
        ArenaExprOrRun::Run(run)
    }

    pub fn push_return(&mut self, value: Option<ArenaExprOrRun>, span: Span) -> StmtId {
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Return(value), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_yield(&mut self, value: ArenaExprOrRun, span: Span) -> StmtId {
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Yield(value), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_defer(&mut self, value: ArenaExprOrRun, span: Span) -> StmtId {
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Defer(value), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_expr_statement(&mut self, expr: ExprId, span: Span) -> StmtId {
        let id = self.lowerer.push_stmt_kind(ArenaStmtKind::Expr(expr), span);
        self.push_current_statement(id);
        id
    }

    /// Write a parameter list directly into the arena (children already built).
    /// Each tuple is (name, type, ty_defaulted, default, rest, span).
    pub fn push_params(
        &mut self,
        params: &[(Name, TypeExprId, bool, Option<ExprId>, bool, Span)],
    ) -> ArenaRange {
        let start = self.lowerer.arena.params.len();
        for (name, ty, ty_defaulted, default, rest, span) in params {
            let span = self.lowerer.span(*span);
            self.lowerer.arena.params.push(ArenaParam {
                name: *name,
                ty: *ty,
                ty_defaulted: *ty_defaulted,
                default: *default,
                rest: *rest,
                span,
            });
        }
        ArenaRange::new(start, params.len())
    }

    pub fn push_effects(&mut self, effects: &[Effect]) -> ArenaRange {
        let start = self.lowerer.arena.extra.len();
        self.lowerer
            .arena
            .extra
            .extend(effects.iter().map(effect_code));
        ArenaRange::new(start, effects.len())
    }

    fn push_arena_function_def(
        &mut self,
        name: Name,
        params: ArenaRange,
        effects: Option<ArenaRange>,
        return_ty: TypeExprId,
        return_ty_defaulted: bool,
        body: BlockId,
    ) -> FunctionDefId {
        let id = FunctionDefId::new(self.lowerer.arena.function_defs.len());
        self.lowerer.arena.function_defs.push(ArenaFunctionDef {
            name,
            params,
            effects,
            return_ty,
            return_ty_defaulted,
            body,
        });
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_function_def_parts(
        &mut self,
        name: Name,
        params: ArenaRange,
        effects: Option<ArenaRange>,
        return_ty: TypeExprId,
        return_ty_defaulted: bool,
        body: BlockId,
        proc_def: bool,
        span: Span,
    ) -> StmtId {
        let def = self.push_arena_function_def(
            name,
            params,
            effects,
            return_ty,
            return_ty_defaulted,
            body,
        );
        let kind = if proc_def {
            ArenaStmtKind::ProcDef(def)
        } else {
            ArenaStmtKind::PureDef(def)
        };
        let id = self.lowerer.push_stmt_kind(kind, span);
        self.push_current_statement(id);
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_stream_function_def_parts(
        &mut self,
        name: Name,
        params: ArenaRange,
        effects: Option<ArenaRange>,
        return_ty: TypeExprId,
        return_ty_defaulted: bool,
        body: BlockId,
        span: Span,
    ) -> StmtId {
        let def = self.push_arena_function_def(
            name,
            params,
            effects,
            return_ty,
            return_ty_defaulted,
            body,
        );
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::StreamDef(def), span);
        self.push_current_statement(id);
        id
    }

    /// Infer a parameter's type name from its (already-built) default expression,
    /// mirroring `inferred_param_type`.
    pub fn infer_param_type_name(&self, default: ExprId) -> Option<Name> {
        let name = match self.lowerer.arena.expr(default).kind {
            ArenaExprKind::Bool(_) => "Bool",
            ArenaExprKind::Int(_) => "Int",
            ArenaExprKind::Float(_) => "Float",
            ArenaExprKind::Duration(_) => "Duration",
            ArenaExprKind::Str(_) | ArenaExprKind::FmtString(_) => "Str",
            ArenaExprKind::Bytes(_) => "Bytes",
            ArenaExprKind::PathStr(_) | ArenaExprKind::PathFmtString(_) => "Path",
            ArenaExprKind::Call { callee, args } => {
                let ArenaExprKind::Ident(callee_name) = self.lowerer.arena.expr(callee).kind else {
                    return None;
                };
                match callee_name.as_str() {
                    "Path" if args.len() == 1 => "Path",
                    "Error" => "Error",
                    "RunError" => "RunError",
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(Name::intern(name))
    }

    pub fn push_loop(&mut self, block: BlockId, span: Span) -> StmtId {
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Loop { block }, span);
        self.push_current_statement(id);
        id
    }

    pub fn push_while(&mut self, condition: ExprId, block: BlockId, span: Span) -> StmtId {
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::While { condition, block }, span);
        self.push_current_statement(id);
        id
    }

    pub fn push_for_id(
        &mut self,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
        span: Span,
    ) -> StmtId {
        let id = self.lowerer.push_stmt_kind(
            ArenaStmtKind::For {
                target,
                iter,
                block,
            },
            span,
        );
        self.push_current_statement(id);
        id
    }

    pub fn push_assign_target_name(&mut self, name: Name) -> AssignTargetId {
        self.lowerer
            .push_assign_target_kind(ArenaAssignTargetKind::Name(name))
    }

    pub fn push_assign_target_field(&mut self, base: AssignTargetId, name: Name) -> AssignTargetId {
        self.lowerer
            .push_assign_target_kind(ArenaAssignTargetKind::Field { base, name })
    }

    pub fn push_assign_target_index(
        &mut self,
        base: AssignTargetId,
        index: ExprId,
    ) -> AssignTargetId {
        self.lowerer
            .push_assign_target_kind(ArenaAssignTargetKind::Index { base, index })
    }

    pub fn push_assignment(
        &mut self,
        target: AssignTargetId,
        op: AssignOp,
        value: ArenaExprOrRun,
        span: Span,
    ) -> StmtId {
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Assign { target, op, value }, span);
        self.push_current_statement(id);
        id
    }

    pub fn push_if(
        &mut self,
        branches: &[(ExprId, BlockId)],
        else_block: Option<BlockId>,
        span: Span,
    ) -> StmtId {
        let branches = self.lowerer.lower_if_branch_id_range(branches);
        let id = self.lowerer.push_stmt_kind(
            ArenaStmtKind::If {
                branches,
                else_block,
            },
            span,
        );
        self.push_current_statement(id);
        id
    }

    /// Wrap an already-built inner statement in a guarded (`when`/`unless`) stmt.
    /// The inner statement was just registered by its own `push_*`; pop it so only
    /// the guarded wrapper appears in the current statement list (mirrors the old
    /// AST where the inner `Stmt` is a local that is never pushed on its own).
    pub fn push_guarded_stmt(
        &mut self,
        inner: StmtId,
        negate: bool,
        condition: ExprId,
        span: Span,
    ) -> StmtId {
        if self.block_statement_starts.is_empty() {
            self.statements.pop();
        } else {
            self.block_statements.pop();
        }
        let id = self.lowerer.push_stmt_kind(
            ArenaStmtKind::GuardedStmt {
                stmt: inner,
                negate,
                condition,
            },
            span,
        );
        self.push_current_statement(id);
        id
    }

    pub fn push_break(&mut self, value: Option<ExprId>, span: Span) -> StmtId {
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Break { value }, span);
        self.push_current_statement(id);
        id
    }

    pub fn push_continue(&mut self, span: Span) -> StmtId {
        let id = self.lowerer.push_stmt_kind(ArenaStmtKind::Continue, span);
        self.push_current_statement(id);
        id
    }

    pub fn push_with_bindings(&mut self, bindings: &[(Name, ExprId, Span)]) -> ArenaRange {
        let start = self.lowerer.arena.with_bindings.len();
        for (name, initializer, span) in bindings {
            let span = self.lowerer.span(*span);
            self.lowerer.arena.with_bindings.push(ArenaWithBinding {
                name: *name,
                initializer: *initializer,
                span,
            });
        }
        ArenaRange::new(start, bindings.len())
    }

    pub fn push_with(
        &mut self,
        bindings: ArenaRange,
        body: BlockId,
        else_param: Option<Name>,
        else_block: BlockId,
        span: Span,
    ) -> StmtId {
        let id = self.lowerer.push_stmt_kind(
            ArenaStmtKind::With {
                bindings,
                body,
                else_param,
                else_block,
            },
            span,
        );
        self.push_current_statement(id);
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_guard(
        &mut self,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        else_param: Option<Name>,
        else_block: BlockId,
        span: Span,
    ) -> StmtId {
        let id = self.lowerer.push_stmt_kind(
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_param,
                else_block,
            },
            span,
        );
        self.push_current_statement(id);
        id
    }

    pub fn push_signal_hook(
        &mut self,
        signal: Name,
        options: SignalHookOptions,
        effects: ArenaRange,
        body: BlockId,
        span: Span,
    ) -> StmtId {
        let signal_hook_id = SignalHookId::new(self.lowerer.arena.signal_hooks.len());
        self.lowerer.arena.signal_hooks.push(ArenaSignalHook {
            signal,
            options,
            effects,
            body,
        });
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::SignalHook(signal_hook_id), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_name_range(&mut self, names: &[Name]) -> ArenaRange {
        let start = self.lowerer.arena.extra.len();
        self.lowerer
            .arena
            .extra
            .extend(names.iter().map(|name| name.symbol().raw()));
        ArenaRange::new(start, names.len())
    }

    pub fn build_error_field(&mut self, name: Name, ty: TypeExprId, span: Span) -> ArenaErrorField {
        ArenaErrorField {
            name,
            ty,
            span: self.lowerer.span(span),
        }
    }

    pub fn build_error_variant(
        &mut self,
        name: Name,
        fields: Vec<ArenaErrorField>,
        facets: &[Name],
        span: Span,
    ) -> ArenaErrorVariant {
        let field_start = self.lowerer.arena.error_fields.len();
        self.lowerer.arena.error_fields.extend(fields);
        let fields = ArenaRange::new(
            field_start,
            self.lowerer.arena.error_fields.len() - field_start,
        );
        let facets = self.push_name_range(facets);
        ArenaErrorVariant {
            name,
            fields,
            facets,
            span: self.lowerer.span(span),
        }
    }

    pub fn push_type_expr_id_range(&mut self, ids: &[TypeExprId]) -> ArenaRange {
        let start = self.lowerer.arena.extra.len();
        self.lowerer
            .arena
            .extra
            .extend(ids.iter().map(|id| raw_type_expr_id(*id)));
        ArenaRange::new(start, ids.len())
    }

    pub fn build_schema_field(
        &mut self,
        name: Name,
        ty: TypeExprId,
        span: Span,
    ) -> ArenaSchemaField {
        ArenaSchemaField {
            name,
            ty,
            span: self.lowerer.span(span),
        }
    }

    pub fn push_schema_field_range(&mut self, fields: Vec<ArenaSchemaField>) -> ArenaRange {
        let start = self.lowerer.arena.schema_fields.len();
        self.lowerer.arena.schema_fields.extend(fields);
        ArenaRange::new(start, self.lowerer.arena.schema_fields.len() - start)
    }

    pub fn build_tag_variant(
        &mut self,
        name: Name,
        fields: &[TypeExprId],
        span: Span,
    ) -> ArenaTagVariant {
        let fields = self.push_type_expr_id_range(fields);
        ArenaTagVariant {
            name,
            fields,
            span: self.lowerer.span(span),
        }
    }

    pub fn push_tag_variant_range(&mut self, variants: Vec<ArenaTagVariant>) -> ArenaRange {
        let start = self.lowerer.arena.tag_variants.len();
        self.lowerer.arena.tag_variants.extend(variants);
        ArenaRange::new(start, self.lowerer.arena.tag_variants.len() - start)
    }

    pub fn build_module_contract_entry(
        &mut self,
        name: Name,
        optional: bool,
        kind: ArenaModuleContractEntryKind,
        span: Span,
    ) -> ArenaModuleContractEntry {
        ArenaModuleContractEntry {
            name,
            optional,
            kind,
            span: self.lowerer.span(span),
        }
    }

    pub fn push_module_contract_entry_range(
        &mut self,
        entries: Vec<ArenaModuleContractEntry>,
    ) -> ArenaRange {
        let start = self.lowerer.arena.module_contract_entries.len();
        self.lowerer.arena.module_contract_entries.extend(entries);
        ArenaRange::new(
            start,
            self.lowerer.arena.module_contract_entries.len() - start,
        )
    }

    pub fn push_type_def(&mut self, name: Name, body: ArenaTypeDefBody, span: Span) -> StmtId {
        let type_def_id = TypeDefId::new(self.lowerer.arena.type_defs.len());
        self.lowerer
            .arena
            .type_defs
            .push(ArenaTypeDef { name, body });
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::TypeDef(type_def_id), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_error_def(
        &mut self,
        name: Name,
        variants: Vec<ArenaErrorVariant>,
        span: Span,
    ) -> StmtId {
        let variant_start = self.lowerer.arena.error_variants.len();
        self.lowerer.arena.error_variants.extend(variants);
        let variants = ArenaRange::new(
            variant_start,
            self.lowerer.arena.error_variants.len() - variant_start,
        );
        let error_def_id = ErrorDefId::new(self.lowerer.arena.error_defs.len());
        self.lowerer
            .arena
            .error_defs
            .push(ArenaErrorDef { name, variants });
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::ErrorDef(error_def_id), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_use(&mut self, path: &[Name], alias: Option<Name>, span: Span) -> StmtId {
        let use_id = self.lowerer.lower_use_stmt(path, alias, None);
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Use(use_id), span);
        self.push_current_statement(id);
        id
    }

    /// The id of the statement most recently registered in the current scope
    /// (used to wrap an inner declaration in `export`).
    pub fn last_current_statement_id(&self) -> Option<StmtId> {
        if self.block_statement_starts.is_empty() {
            self.statements.last().copied()
        } else {
            self.block_statements.last().copied()
        }
    }

    /// Wrap an already-registered inner statement in `export`. Pops the inner so
    /// only the export wrapper stays in the current scope (like `push_guarded_stmt`).
    pub fn push_export(&mut self, inner: StmtId, span: Span) -> StmtId {
        if self.block_statement_starts.is_empty() {
            self.statements.pop();
        } else {
            self.block_statements.pop();
        }
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Export(inner), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_match(&mut self, value: ExprId, arms: ArenaRange, span: Span) -> StmtId {
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Match { value, arms }, span);
        self.push_current_statement(id);
        id
    }

    pub fn push_run_command_statement(
        &mut self,
        run: RunFormId,
        propagate: bool,
        span: Span,
    ) -> StmtId {
        let id = self
            .lowerer
            .push_command_stmt_kind(ArenaCommand::Run(run), propagate, span);
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Command(id), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_command_statement(
        &mut self,
        command: ArenaCommand,
        propagate: bool,
        span: Span,
    ) -> StmtId {
        let id = self
            .lowerer
            .push_command_stmt_kind(command, propagate, span);
        let id = self
            .lowerer
            .push_stmt_kind(ArenaStmtKind::Command(id), span);
        self.push_current_statement(id);
        id
    }

    pub fn push_null_expr(&mut self, span: Span) -> ExprId {
        self.lowerer.push_expr_kind(ArenaExprKind::Null, span)
    }

    pub fn push_bool_expr(&mut self, value: bool, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Bool(value), span)
    }

    pub fn push_ident_expr(&mut self, name: Name, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Ident(name), span)
    }

    pub fn push_item_expr(&mut self, span: Span) -> ExprId {
        self.lowerer.push_expr_kind(ArenaExprKind::Item, span)
    }

    pub fn push_last_status_expr(&mut self, span: Span) -> ExprId {
        self.lowerer.push_expr_kind(ArenaExprKind::LastStatus, span)
    }

    pub fn push_int_expr(&mut self, value: &IntLiteral, span: Span) -> ExprId {
        let value = self.lowerer.lower_int_literal(value);
        self.lowerer.push_expr_kind(ArenaExprKind::Int(value), span)
    }

    pub fn push_float_expr(&mut self, value: &FloatLiteral, span: Span) -> ExprId {
        let value = self.lowerer.lower_float_literal(value);
        self.lowerer
            .push_expr_kind(ArenaExprKind::Float(value), span)
    }

    pub fn push_duration_expr(&mut self, value: &DurationLiteral, span: Span) -> ExprId {
        let value = self.lowerer.lower_duration_literal(value);
        self.lowerer
            .push_expr_kind(ArenaExprKind::Duration(value), span)
    }

    pub fn push_str_expr(&mut self, value: &Arc<str>, span: Span) -> ExprId {
        let value = self.lowerer.lower_string_literal(value);
        self.lowerer.push_expr_kind(ArenaExprKind::Str(value), span)
    }

    pub fn push_path_str_expr(&mut self, value: &Arc<str>, span: Span) -> ExprId {
        let value = self.lowerer.lower_string_literal(value);
        self.lowerer
            .push_expr_kind(ArenaExprKind::PathStr(value), span)
    }

    pub fn push_glob_str_expr(&mut self, value: &Arc<str>, span: Span) -> ExprId {
        let value = self.lowerer.lower_string_literal(value);
        self.lowerer
            .push_expr_kind(ArenaExprKind::GlobStr(value), span)
    }

    pub fn push_fmt_string_expr(&mut self, parts: ArenaRange, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::FmtString(parts), span)
    }

    pub fn push_path_fmt_string_expr(&mut self, parts: ArenaRange, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::PathFmtString(parts), span)
    }

    pub fn push_bytes_expr(&mut self, value: &Arc<[u8]>, span: Span) -> ExprId {
        let value = self.lowerer.lower_bytes_literal(value);
        self.lowerer
            .push_expr_kind(ArenaExprKind::Bytes(value), span)
    }

    pub fn push_list_expr_range(&mut self, items: ArenaRange, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::List(items), span)
    }

    pub fn push_list_comp_expr(
        &mut self,
        expr: ExprId,
        target: BindingTargetId,
        iter: ExprId,
        condition: Option<ExprId>,
        span: Span,
    ) -> ExprId {
        self.lowerer.push_expr_kind(
            ArenaExprKind::ListComp {
                expr,
                target,
                iter,
                condition,
            },
            span,
        )
    }

    pub fn push_map_comp_expr(
        &mut self,
        key: ExprId,
        value: ExprId,
        target: BindingTargetId,
        iter: ExprId,
        condition: Option<ExprId>,
        span: Span,
    ) -> ExprId {
        self.lowerer.push_expr_kind(
            ArenaExprKind::MapComp {
                key,
                value,
                target,
                iter,
                condition,
            },
            span,
        )
    }

    pub fn push_unary_expr(&mut self, op: UnaryOp, expr: ExprId, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Unary { op, expr }, span)
    }

    pub fn push_binary_expr(
        &mut self,
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
        span: Span,
    ) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Binary { op, left, right }, span)
    }

    pub fn push_field_expr(&mut self, base: ExprId, name: Name, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Field { base, name }, span)
    }

    pub fn push_null_safe_field_expr(&mut self, base: ExprId, name: Name, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::NullSafeField { base, name }, span)
    }

    pub fn push_index_expr(&mut self, base: ExprId, index: ExprId, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Index { base, index }, span)
    }

    pub fn push_slice_expr(
        &mut self,
        base: ExprId,
        start: Option<ExprId>,
        end: Option<ExprId>,
        span: Span,
    ) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Slice { base, start, end }, span)
    }

    pub fn push_call_expr(&mut self, callee: ExprId, args: ArenaRange, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Call { callee, args }, span)
    }

    pub fn push_record_expr(&mut self, fields: ArenaRange, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Record(fields), span)
    }

    pub fn push_if_expr(&mut self, branches: ArenaRange, else_value: ExprId, span: Span) -> ExprId {
        self.lowerer.push_expr_kind(
            ArenaExprKind::If {
                branches,
                else_value,
            },
            span,
        )
    }

    pub fn push_match_expr(&mut self, value: ExprId, arms: ArenaRange, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Match { value, arms }, span)
    }

    pub fn push_loop_expr(&mut self, block: BlockId, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Loop { block }, span)
    }

    pub fn push_run_expr_id(&mut self, run: RunFormId, span: Span) -> ExprId {
        self.lowerer.push_expr_kind(ArenaExprKind::Run(run), span)
    }

    pub fn push_spawn_run_expr_id(
        &mut self,
        run: RunFormId,
        form_span: Span,
        span: Span,
    ) -> ExprId {
        let form_span = self.lowerer.span(form_span);
        self.lowerer.push_expr_kind(
            ArenaExprKind::Spawn(ArenaSpawnForm {
                target: ArenaSpawnTarget::Run(run),
                span: form_span,
            }),
            span,
        )
    }

    pub fn push_spawn_command_expr(
        &mut self,
        command: ExprId,
        form_span: Span,
        span: Span,
    ) -> ExprId {
        let form_span = self.lowerer.span(form_span);
        self.lowerer.push_expr_kind(
            ArenaExprKind::Spawn(ArenaSpawnForm {
                target: ArenaSpawnTarget::Command(command),
                span: form_span,
            }),
            span,
        )
    }

    pub fn push_wait_expr(&mut self, target: ExprId, form_span: Span, span: Span) -> ExprId {
        let form_span = self.lowerer.span(form_span);
        self.lowerer.push_expr_kind(
            ArenaExprKind::Wait(ArenaWaitForm {
                target,
                span: form_span,
            }),
            span,
        )
    }

    pub fn push_retry_expr(&mut self, delays: ArenaRange, block: BlockId, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Retry { delays, block }, span)
    }

    pub fn push_try_expr(&mut self, value: ExprId, span: Span) -> ExprId {
        self.lowerer.push_expr_kind(ArenaExprKind::Try(value), span)
    }

    pub fn push_require_expr(&mut self, value: ExprId, schema: TypeExprId, span: Span) -> ExprId {
        self.lowerer
            .push_expr_kind(ArenaExprKind::Require { value, schema }, span)
    }

    pub fn finish(mut self) -> ArenaProgram {
        let statements = self.lowerer.lower_stmt_id_range(&self.statements);
        ArenaProgram {
            arena: self.lowerer.arena,
            statements,
            modules: self.modules,
        }
    }

    pub fn finish_with_statements(self, statements: ArenaRange) -> ArenaProgram {
        ArenaProgram {
            arena: self.lowerer.arena,
            statements,
            modules: self.modules,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArenaStats {
    pub modules: usize,
    pub statements: usize,
    pub blocks: usize,
    pub expressions: usize,
    pub patterns: usize,
    pub binding_targets: usize,
    pub assign_targets: usize,
    pub type_exprs: usize,
    pub use_stmts: usize,
    pub type_defs: usize,
    pub error_defs: usize,
    pub function_defs: usize,
    pub signal_hooks: usize,
    pub command_stmts: usize,
    pub int_literals: usize,
    pub float_literals: usize,
    pub duration_literals: usize,
    pub string_literals: usize,
    pub bytes_literals: usize,
    pub text_literals: usize,
    pub source_text_literals: usize,
    pub cooked_text_literals: usize,
    pub run_forms: usize,
    pub builder_blocks: usize,
    pub spans: usize,
    pub span_source_overrides: usize,
    pub extra_items: usize,
    pub fmt_parts: usize,
    pub command_args: usize,
    pub word_parts: usize,
    pub list_items: usize,
    pub span_storage_bytes: usize,
    pub stmt_storage_bytes: usize,
    pub expr_storage_bytes: usize,
    pub type_expr_storage_bytes: usize,
    pub extra_storage_bytes: usize,
    pub text_storage_bytes: usize,
    pub cooked_text_storage_bytes: usize,
    pub definition_storage_bytes: usize,
    pub literal_storage_bytes: usize,
    pub pattern_storage_bytes: usize,
    pub block_storage_bytes: usize,
    pub control_storage_bytes: usize,
    pub call_record_storage_bytes: usize,
    pub builder_storage_bytes: usize,
    pub command_storage_bytes: usize,
    pub side_table_storage_bytes: usize,
    pub retained_bytes: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AstArena {
    pub span_source_id: Option<SourceId>,
    pub spans: ArenaByteSpans,
    pub span_source_overrides: Vec<ArenaSpanSource>,
    pub stmt_tags: Vec<ArenaStmtTag>,
    pub stmt_data: Vec<ArenaStmtData>,
    pub stmt_spans: ArenaByteSpans,
    pub stmt_span_source_overrides: Vec<ArenaSpanSource>,
    pub blocks: Vec<ArenaBlock>,
    pub expr_tags: Vec<ArenaExprTag>,
    pub expr_data: Vec<ArenaExprData>,
    pub expr_spans: ArenaByteSpans,
    pub expr_span_source_overrides: Vec<ArenaSpanSource>,
    pub patterns: Vec<ArenaPattern>,
    pub binding_targets: Vec<ArenaBindingTarget>,
    pub assign_targets: Vec<ArenaAssignTarget>,
    pub type_expr_tags: Vec<ArenaTypeExprTag>,
    pub type_expr_data: Vec<ArenaTypeExprData>,
    pub type_expr_spans: ArenaByteSpans,
    pub type_expr_span_source_overrides: Vec<ArenaSpanSource>,
    pub use_stmts: Vec<ArenaUseStmt>,
    pub type_defs: Vec<ArenaTypeDef>,
    pub error_defs: Vec<ArenaErrorDef>,
    pub function_defs: Vec<ArenaFunctionDef>,
    pub signal_hooks: Vec<ArenaSignalHook>,
    pub command_stmts: Vec<ArenaCommandStmt>,
    pub int_literals: Vec<IntLiteral>,
    pub float_literals: Vec<FloatLiteral>,
    pub duration_literals: Vec<DurationLiteral>,
    pub string_literals: Vec<Arc<str>>,
    pub bytes_literals: Vec<Arc<[u8]>>,
    pub text_tags: Vec<ArenaTextTag>,
    pub text_data: Vec<ArenaTextData>,
    pub cooked_texts: Vec<Arc<str>>,
    pub run_forms: Vec<ArenaRunForm>,
    pub builder_blocks: Vec<ArenaBuilderBlock>,
    pub extra: Vec<u32>,
    pub block_params: Vec<ArenaBlockParam>,
    pub params: Vec<ArenaParam>,
    pub schema_fields: Vec<ArenaSchemaField>,
    pub module_contract_entries: Vec<ArenaModuleContractEntry>,
    pub tag_variants: Vec<ArenaTagVariant>,
    pub error_variants: Vec<ArenaErrorVariant>,
    pub error_fields: Vec<ArenaErrorField>,
    pub if_branches: Vec<ArenaIfBranch>,
    pub with_bindings: Vec<ArenaWithBinding>,
    pub match_arms: Vec<ArenaMatchArm>,
    pub destructure_fields: Vec<ArenaDestructureField>,
    pub pattern_fields: Vec<ArenaRecordPatternField>,
    pub fmt_part_tags: Vec<ArenaFmtPartTag>,
    pub fmt_part_data: Vec<ArenaFmtPartData>,
    pub if_expr_branches: Vec<ArenaIfExprBranch>,
    pub match_expr_arms: Vec<ArenaMatchExprArm>,
    pub record_fields: Vec<ArenaRecordField>,
    pub call_args: Vec<ArenaCallArg>,
    pub pipe_stages: Vec<ArenaPipeStage>,
    pub stream_stages: Vec<ArenaStreamStage>,
    pub stream_options: Vec<ArenaStreamStageOption>,
    pub builder_entries: Vec<ArenaBuilderEntry>,
    pub command_args: Vec<ArenaCommandArg>,
    pub env_assignments: Vec<ArenaEnvAssignment>,
    pub run_segments: Vec<ArenaRunSegment>,
    pub redirections: Vec<ArenaRedirection>,
    pub word_part_tags: Vec<ArenaWordPartTag>,
    pub word_part_data: Vec<ArenaWordPartData>,
}

impl AstArena {
    fn with_source_len(source_len: usize) -> Self {
        Self {
            spans: ArenaByteSpans::for_source_len(source_len),
            stmt_spans: ArenaByteSpans::for_source_len(source_len),
            expr_spans: ArenaByteSpans::for_source_len(source_len),
            type_expr_spans: ArenaByteSpans::for_source_len(source_len),
            ..Self::default()
        }
    }

    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>() + self.capacity_bytes()
    }

    pub fn span_storage_bytes(&self) -> usize {
        self.spans.retained_bytes()
            + vec_capacity_bytes(&self.span_source_overrides)
            + vec_capacity_bytes(&self.stmt_span_source_overrides)
            + vec_capacity_bytes(&self.expr_span_source_overrides)
            + vec_capacity_bytes(&self.type_expr_span_source_overrides)
    }

    pub fn stmt_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.stmt_tags)
            + vec_capacity_bytes(&self.stmt_data)
            + self.stmt_spans.retained_bytes()
    }

    pub fn expr_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.expr_tags)
            + vec_capacity_bytes(&self.expr_data)
            + self.expr_spans.retained_bytes()
    }

    pub fn type_expr_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.type_expr_tags)
            + vec_capacity_bytes(&self.type_expr_data)
            + self.type_expr_spans.retained_bytes()
    }

    pub fn extra_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.extra)
    }

    pub fn span_source_overrides(&self) -> usize {
        self.span_source_overrides.len()
            + self.stmt_span_source_overrides.len()
            + self.expr_span_source_overrides.len()
            + self.type_expr_span_source_overrides.len()
    }

    pub fn text_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.text_tags) + vec_capacity_bytes(&self.text_data)
    }

    pub fn cooked_text_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.cooked_texts)
    }

    pub fn source_text_literals(&self) -> usize {
        self.text_tags
            .iter()
            .filter(|tag| matches!(tag, ArenaTextTag::Source))
            .count()
    }

    pub fn fmt_part_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.fmt_part_tags) + vec_capacity_bytes(&self.fmt_part_data)
    }

    pub fn word_part_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.word_part_tags) + vec_capacity_bytes(&self.word_part_data)
    }

    pub fn definition_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.use_stmts)
            + vec_capacity_bytes(&self.type_defs)
            + vec_capacity_bytes(&self.error_defs)
            + vec_capacity_bytes(&self.function_defs)
            + vec_capacity_bytes(&self.signal_hooks)
            + vec_capacity_bytes(&self.schema_fields)
            + vec_capacity_bytes(&self.module_contract_entries)
            + vec_capacity_bytes(&self.tag_variants)
            + vec_capacity_bytes(&self.error_variants)
            + vec_capacity_bytes(&self.error_fields)
    }

    pub fn literal_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.int_literals)
            + vec_capacity_bytes(&self.float_literals)
            + vec_capacity_bytes(&self.duration_literals)
            + vec_capacity_bytes(&self.string_literals)
            + vec_capacity_bytes(&self.bytes_literals)
    }

    pub fn pattern_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.patterns)
            + vec_capacity_bytes(&self.binding_targets)
            + vec_capacity_bytes(&self.assign_targets)
            + vec_capacity_bytes(&self.destructure_fields)
            + vec_capacity_bytes(&self.pattern_fields)
    }

    pub fn block_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.blocks)
            + vec_capacity_bytes(&self.block_params)
            + vec_capacity_bytes(&self.params)
    }

    pub fn control_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.if_branches)
            + vec_capacity_bytes(&self.with_bindings)
            + vec_capacity_bytes(&self.match_arms)
            + vec_capacity_bytes(&self.if_expr_branches)
            + vec_capacity_bytes(&self.match_expr_arms)
    }

    pub fn call_record_storage_bytes(&self) -> usize {
        self.fmt_part_storage_bytes()
            + vec_capacity_bytes(&self.record_fields)
            + vec_capacity_bytes(&self.call_args)
            + vec_capacity_bytes(&self.pipe_stages)
            + vec_capacity_bytes(&self.stream_stages)
            + vec_capacity_bytes(&self.stream_options)
    }

    pub fn builder_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.builder_blocks) + vec_capacity_bytes(&self.builder_entries)
    }

    pub fn command_storage_bytes(&self) -> usize {
        vec_capacity_bytes(&self.command_stmts)
            + vec_capacity_bytes(&self.run_forms)
            + vec_capacity_bytes(&self.command_args)
            + vec_capacity_bytes(&self.env_assignments)
            + vec_capacity_bytes(&self.run_segments)
            + vec_capacity_bytes(&self.redirections)
            + self.word_part_storage_bytes()
    }

    pub fn side_table_storage_bytes(&self) -> usize {
        self.text_storage_bytes()
            + self.cooked_text_storage_bytes()
            + self.definition_storage_bytes()
            + self.literal_storage_bytes()
            + self.pattern_storage_bytes()
            + self.block_storage_bytes()
            + self.control_storage_bytes()
            + self.call_record_storage_bytes()
            + self.builder_storage_bytes()
            + self.command_storage_bytes()
    }

    pub fn capacity_bytes(&self) -> usize {
        self.span_storage_bytes()
            + self.stmt_storage_bytes()
            + vec_capacity_bytes(&self.blocks)
            + self.expr_storage_bytes()
            + vec_capacity_bytes(&self.patterns)
            + vec_capacity_bytes(&self.binding_targets)
            + vec_capacity_bytes(&self.assign_targets)
            + self.type_expr_storage_bytes()
            + vec_capacity_bytes(&self.use_stmts)
            + vec_capacity_bytes(&self.type_defs)
            + vec_capacity_bytes(&self.error_defs)
            + vec_capacity_bytes(&self.function_defs)
            + vec_capacity_bytes(&self.signal_hooks)
            + vec_capacity_bytes(&self.schema_fields)
            + vec_capacity_bytes(&self.module_contract_entries)
            + vec_capacity_bytes(&self.tag_variants)
            + vec_capacity_bytes(&self.error_variants)
            + vec_capacity_bytes(&self.error_fields)
            + vec_capacity_bytes(&self.command_stmts)
            + vec_capacity_bytes(&self.int_literals)
            + vec_capacity_bytes(&self.float_literals)
            + vec_capacity_bytes(&self.duration_literals)
            + vec_capacity_bytes(&self.string_literals)
            + vec_capacity_bytes(&self.bytes_literals)
            + self.text_storage_bytes()
            + self.cooked_text_storage_bytes()
            + vec_capacity_bytes(&self.run_forms)
            + vec_capacity_bytes(&self.builder_blocks)
            + self.extra_storage_bytes()
            + vec_capacity_bytes(&self.block_params)
            + vec_capacity_bytes(&self.params)
            + vec_capacity_bytes(&self.if_branches)
            + vec_capacity_bytes(&self.with_bindings)
            + vec_capacity_bytes(&self.match_arms)
            + vec_capacity_bytes(&self.destructure_fields)
            + vec_capacity_bytes(&self.pattern_fields)
            + self.fmt_part_storage_bytes()
            + vec_capacity_bytes(&self.if_expr_branches)
            + vec_capacity_bytes(&self.match_expr_arms)
            + vec_capacity_bytes(&self.record_fields)
            + vec_capacity_bytes(&self.call_args)
            + vec_capacity_bytes(&self.pipe_stages)
            + vec_capacity_bytes(&self.stream_stages)
            + vec_capacity_bytes(&self.stream_options)
            + vec_capacity_bytes(&self.builder_entries)
            + vec_capacity_bytes(&self.command_args)
            + vec_capacity_bytes(&self.env_assignments)
            + vec_capacity_bytes(&self.run_segments)
            + vec_capacity_bytes(&self.redirections)
            + self.word_part_storage_bytes()
    }

    pub fn list_items(&self) -> usize {
        self.extra.len()
            + self.block_params.len()
            + self.params.len()
            + self.schema_fields.len()
            + self.module_contract_entries.len()
            + self.tag_variants.len()
            + self.error_variants.len()
            + self.error_fields.len()
            + self.if_branches.len()
            + self.with_bindings.len()
            + self.match_arms.len()
            + self.destructure_fields.len()
            + self.pattern_fields.len()
            + self.fmt_part_tags.len()
            + self.if_expr_branches.len()
            + self.match_expr_arms.len()
            + self.record_fields.len()
            + self.call_args.len()
            + self.pipe_stages.len()
            + self.stream_stages.len()
            + self.stream_options.len()
            + self.builder_entries.len()
            + self.command_args.len()
            + self.env_assignments.len()
            + self.run_segments.len()
            + self.redirections.len()
            + self.word_part_tags.len()
    }

    pub fn span(&self, id: SpanId) -> Span {
        let index = id.index();
        let source_id = self.span_source_id_for(index, &self.span_source_overrides);
        self.spans.get(index).to_span(source_id)
    }

    fn span_source_id_for(&self, index: usize, overrides: &[ArenaSpanSource]) -> SourceId {
        overrides
            .binary_search_by_key(&raw_index(index), |source| source.span)
            .ok()
            .map(|override_index| overrides[override_index].source_id)
            .or(self.span_source_id)
            .expect("arena span without source id")
    }

    fn inline_span(
        &self,
        span: ArenaByteSpan,
        index: usize,
        overrides: &[ArenaSpanSource],
    ) -> Span {
        span.to_span(self.span_source_id_for(index, overrides))
    }

    pub fn stmt(&self, id: StmtId) -> ArenaStmt {
        let index = id.index();
        ArenaStmt {
            kind: self.stmt_kind(id),
            span: self.inline_span(
                self.stmt_spans.get(index),
                index,
                &self.stmt_span_source_overrides,
            ),
        }
    }

    fn stmt_kind(&self, id: StmtId) -> ArenaStmtKind {
        let tag = self.stmt_tags[id.index()];
        let data = self.stmt_data[id.index()];
        match tag {
            ArenaStmtTag::Use => ArenaStmtKind::Use(UseStmtId::new(data.lhs as usize)),
            ArenaStmtTag::Export => ArenaStmtKind::Export(StmtId::new(data.lhs as usize)),
            ArenaStmtTag::TypeDef => ArenaStmtKind::TypeDef(TypeDefId::new(data.lhs as usize)),
            ArenaStmtTag::ErrorDef => ArenaStmtKind::ErrorDef(ErrorDefId::new(data.lhs as usize)),
            ArenaStmtTag::LetExprNoTy => ArenaStmtKind::Let {
                target: BindingTargetId::new(data.lhs as usize),
                ty: None,
                initializer: ArenaExprOrRun::Expr(ExprId::new(data.rhs as usize)),
            },
            ArenaStmtTag::Let => {
                let raw = range_slice(&self.extra, range_from_stmt_data(data));
                ArenaStmtKind::Let {
                    target: BindingTargetId::new(raw[0] as usize),
                    ty: optional_type_expr_id(raw[1]),
                    initializer: expr_or_run_from_raw(raw[2]),
                }
            }
            ArenaStmtTag::VarExprNoTy => ArenaStmtKind::Var {
                target: BindingTargetId::new(data.lhs as usize),
                ty: None,
                initializer: ArenaExprOrRun::Expr(ExprId::new(data.rhs as usize)),
            },
            ArenaStmtTag::Var => {
                let raw = range_slice(&self.extra, range_from_stmt_data(data));
                ArenaStmtKind::Var {
                    target: BindingTargetId::new(raw[0] as usize),
                    ty: optional_type_expr_id(raw[1]),
                    initializer: expr_or_run_from_raw(raw[2]),
                }
            }
            ArenaStmtTag::AssignSet
            | ArenaStmtTag::AssignAdd
            | ArenaStmtTag::AssignSub
            | ArenaStmtTag::AssignMul
            | ArenaStmtTag::AssignDiv
            | ArenaStmtTag::AssignRem => ArenaStmtKind::Assign {
                target: AssignTargetId::new(data.lhs as usize),
                op: assign_op_from_stmt_tag(tag),
                value: expr_or_run_from_raw(data.rhs),
            },
            ArenaStmtTag::ProcDef => ArenaStmtKind::ProcDef(FunctionDefId::new(data.lhs as usize)),
            ArenaStmtTag::PureDef => ArenaStmtKind::PureDef(FunctionDefId::new(data.lhs as usize)),
            ArenaStmtTag::StreamDef => {
                ArenaStmtKind::StreamDef(FunctionDefId::new(data.lhs as usize))
            }
            ArenaStmtTag::SignalHook => {
                ArenaStmtKind::SignalHook(SignalHookId::new(data.lhs as usize))
            }
            ArenaStmtTag::ReturnNone => ArenaStmtKind::Return(None),
            ArenaStmtTag::ReturnExpr => {
                ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(ExprId::new(data.lhs as usize))))
            }
            ArenaStmtTag::ReturnRun => {
                ArenaStmtKind::Return(Some(ArenaExprOrRun::Run(RunFormId::new(data.lhs as usize))))
            }
            ArenaStmtTag::YieldExpr => {
                ArenaStmtKind::Yield(ArenaExprOrRun::Expr(ExprId::new(data.lhs as usize)))
            }
            ArenaStmtTag::YieldRun => {
                ArenaStmtKind::Yield(ArenaExprOrRun::Run(RunFormId::new(data.lhs as usize)))
            }
            ArenaStmtTag::DeferExpr => {
                ArenaStmtKind::Defer(ArenaExprOrRun::Expr(ExprId::new(data.lhs as usize)))
            }
            ArenaStmtTag::DeferRun => {
                ArenaStmtKind::Defer(ArenaExprOrRun::Run(RunFormId::new(data.lhs as usize)))
            }
            ArenaStmtTag::IfNoElse => ArenaStmtKind::If {
                branches: range_from_stmt_data(data),
                else_block: None,
            },
            ArenaStmtTag::IfElse => {
                let raw = range_slice(&self.extra, range_from_stmt_data(data));
                ArenaStmtKind::If {
                    branches: ArenaRange::new(raw[0] as usize, raw[1] as usize),
                    else_block: Some(BlockId::new(raw[2] as usize)),
                }
            }
            ArenaStmtTag::While => ArenaStmtKind::While {
                condition: ExprId::new(data.lhs as usize),
                block: BlockId::new(data.rhs as usize),
            },
            ArenaStmtTag::For => {
                let raw = range_slice(&self.extra, range_from_stmt_data(data));
                ArenaStmtKind::For {
                    target: BindingTargetId::new(raw[0] as usize),
                    iter: ExprId::new(raw[1] as usize),
                    block: BlockId::new(raw[2] as usize),
                }
            }
            ArenaStmtTag::With => {
                let raw = range_slice(&self.extra, range_from_stmt_data(data));
                ArenaStmtKind::With {
                    bindings: ArenaRange::new(raw[0] as usize, raw[1] as usize),
                    body: BlockId::new(raw[2] as usize),
                    else_param: optional_name(raw[3]),
                    else_block: BlockId::new(raw[4] as usize),
                }
            }
            ArenaStmtTag::Loop => ArenaStmtKind::Loop {
                block: BlockId::new(data.lhs as usize),
            },
            ArenaStmtTag::Guard => {
                let raw = range_slice(&self.extra, range_from_stmt_data(data));
                ArenaStmtKind::Guard {
                    target: BindingTargetId::new(raw[0] as usize),
                    ty: optional_type_expr_id(raw[1]),
                    initializer: expr_or_run_from_raw(raw[2]),
                    else_param: optional_name(raw[3]),
                    else_block: BlockId::new(raw[4] as usize),
                }
            }
            ArenaStmtTag::GuardedStmt | ArenaStmtTag::GuardedStmtNegated => {
                ArenaStmtKind::GuardedStmt {
                    stmt: StmtId::new(data.lhs as usize),
                    negate: tag == ArenaStmtTag::GuardedStmtNegated,
                    condition: ExprId::new(data.rhs as usize),
                }
            }
            ArenaStmtTag::BreakNone => ArenaStmtKind::Break { value: None },
            ArenaStmtTag::BreakValue => ArenaStmtKind::Break {
                value: Some(ExprId::new(data.lhs as usize)),
            },
            ArenaStmtTag::Continue => ArenaStmtKind::Continue,
            ArenaStmtTag::Match => {
                let raw = range_slice(&self.extra, range_from_stmt_data(data));
                ArenaStmtKind::Match {
                    value: ExprId::new(raw[0] as usize),
                    arms: ArenaRange::new(raw[1] as usize, raw[2] as usize),
                }
            }
            ArenaStmtTag::Command => ArenaStmtKind::Command(CommandStmtId::new(data.lhs as usize)),
            ArenaStmtTag::TailBareIdent => {
                ArenaStmtKind::TailBareIdent(Name::from_symbol(Symbol::from_raw(data.lhs)))
            }
            ArenaStmtTag::Expr => ArenaStmtKind::Expr(ExprId::new(data.lhs as usize)),
        }
    }

    pub fn block(&self, id: BlockId) -> &ArenaBlock {
        &self.blocks[id.index()]
    }

    pub fn expr(&self, id: ExprId) -> ArenaExpr {
        let index = id.index();
        ArenaExpr {
            kind: self.expr_kind(id),
            span: self.inline_span(
                self.expr_spans.get(index),
                index,
                &self.expr_span_source_overrides,
            ),
        }
    }

    fn expr_kind(&self, id: ExprId) -> ArenaExprKind {
        let tag = self.expr_tags[id.index()];
        let data = self.expr_data[id.index()];
        match tag {
            ArenaExprTag::Null => ArenaExprKind::Null,
            ArenaExprTag::BoolFalse => ArenaExprKind::Bool(false),
            ArenaExprTag::BoolTrue => ArenaExprKind::Bool(true),
            ArenaExprTag::Int => ArenaExprKind::Int(IntLiteralId::new(data.lhs as usize)),
            ArenaExprTag::Float => ArenaExprKind::Float(FloatLiteralId::new(data.lhs as usize)),
            ArenaExprTag::Duration => {
                ArenaExprKind::Duration(DurationLiteralId::new(data.lhs as usize))
            }
            ArenaExprTag::Str => ArenaExprKind::Str(StringLiteralId::new(data.lhs as usize)),
            ArenaExprTag::PathStr => {
                ArenaExprKind::PathStr(StringLiteralId::new(data.lhs as usize))
            }
            ArenaExprTag::GlobStr => {
                ArenaExprKind::GlobStr(StringLiteralId::new(data.lhs as usize))
            }
            ArenaExprTag::FmtString => ArenaExprKind::FmtString(range_from_data(data)),
            ArenaExprTag::PathFmtString => ArenaExprKind::PathFmtString(range_from_data(data)),
            ArenaExprTag::Bytes => ArenaExprKind::Bytes(BytesLiteralId::new(data.lhs as usize)),
            ArenaExprTag::Ident => {
                ArenaExprKind::Ident(Name::from_symbol(Symbol::from_raw(data.lhs)))
            }
            ArenaExprTag::Item => ArenaExprKind::Item,
            ArenaExprTag::LastStatus => ArenaExprKind::LastStatus,
            ArenaExprTag::List => ArenaExprKind::List(range_from_data(data)),
            ArenaExprTag::ListComp => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::ListComp {
                    expr: ExprId::new(raw[0] as usize),
                    target: BindingTargetId::new(raw[1] as usize),
                    iter: ExprId::new(raw[2] as usize),
                    condition: optional_expr_id(raw[3]),
                }
            }
            ArenaExprTag::MapComp => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::MapComp {
                    key: ExprId::new(raw[0] as usize),
                    value: ExprId::new(raw[1] as usize),
                    target: BindingTargetId::new(raw[2] as usize),
                    iter: ExprId::new(raw[3] as usize),
                    condition: optional_expr_id(raw[4]),
                }
            }
            ArenaExprTag::Record => ArenaExprKind::Record(range_from_data(data)),
            ArenaExprTag::If => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::If {
                    branches: ArenaRange::new(raw[0] as usize, raw[1] as usize),
                    else_value: ExprId::new(raw[2] as usize),
                }
            }
            ArenaExprTag::Match => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::Match {
                    value: ExprId::new(raw[0] as usize),
                    arms: ArenaRange::new(raw[1] as usize, raw[2] as usize),
                }
            }
            ArenaExprTag::UnaryNot => ArenaExprKind::Unary {
                op: UnaryOp::Not,
                expr: ExprId::new(data.lhs as usize),
            },
            ArenaExprTag::UnaryNeg => ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr: ExprId::new(data.lhs as usize),
            },
            ArenaExprTag::BinaryResultFallback
            | ArenaExprTag::BinaryOr
            | ArenaExprTag::BinaryAnd
            | ArenaExprTag::BinaryEq
            | ArenaExprTag::BinaryNe
            | ArenaExprTag::BinaryLt
            | ArenaExprTag::BinaryLe
            | ArenaExprTag::BinaryGt
            | ArenaExprTag::BinaryGe
            | ArenaExprTag::BinaryIn
            | ArenaExprTag::BinaryNotIn
            | ArenaExprTag::BinaryAdd
            | ArenaExprTag::BinarySub
            | ArenaExprTag::BinaryMul
            | ArenaExprTag::BinaryDiv
            | ArenaExprTag::BinaryRem => ArenaExprKind::Binary {
                op: binary_op_from_expr_tag(tag),
                left: ExprId::new(data.lhs as usize),
                right: ExprId::new(data.rhs as usize),
            },
            ArenaExprTag::Call => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::Call {
                    callee: ExprId::new(raw[0] as usize),
                    args: ArenaRange::new(raw[1] as usize, raw[2] as usize),
                }
            }
            ArenaExprTag::Field => ArenaExprKind::Field {
                base: ExprId::new(data.lhs as usize),
                name: Name::from_symbol(Symbol::from_raw(data.rhs)),
            },
            ArenaExprTag::NullSafeField => ArenaExprKind::NullSafeField {
                base: ExprId::new(data.lhs as usize),
                name: Name::from_symbol(Symbol::from_raw(data.rhs)),
            },
            ArenaExprTag::Index => ArenaExprKind::Index {
                base: ExprId::new(data.lhs as usize),
                index: ExprId::new(data.rhs as usize),
            },
            ArenaExprTag::Slice => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::Slice {
                    base: ExprId::new(raw[0] as usize),
                    start: optional_expr_id(raw[1]),
                    end: optional_expr_id(raw[2]),
                }
            }
            ArenaExprTag::EnvGetStr => ArenaExprKind::EnvGet {
                kind: EnvGetKind::Str,
                name: Name::from_symbol(Symbol::from_raw(data.lhs)),
            },
            ArenaExprTag::EnvGetPath => ArenaExprKind::EnvGet {
                kind: EnvGetKind::Path,
                name: Name::from_symbol(Symbol::from_raw(data.lhs)),
            },
            ArenaExprTag::EnvGetPathList => ArenaExprKind::EnvGet {
                kind: EnvGetKind::PathList,
                name: Name::from_symbol(Symbol::from_raw(data.lhs)),
            },
            ArenaExprTag::EnvPathList => ArenaExprKind::EnvPathList,
            ArenaExprTag::Pipeline => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::Pipeline {
                    input: ExprId::new(raw[0] as usize),
                    stages: ArenaRange::new(raw[1] as usize, raw[2] as usize),
                }
            }
            ArenaExprTag::StructuredPipeline => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::StructuredPipeline {
                    input: ExprId::new(raw[0] as usize),
                    stages: ArenaRange::new(raw[1] as usize, raw[2] as usize),
                }
            }
            ArenaExprTag::Run => ArenaExprKind::Run(RunFormId::new(data.lhs as usize)),
            ArenaExprTag::SpawnRun => ArenaExprKind::Spawn(ArenaSpawnForm {
                target: ArenaSpawnTarget::Run(RunFormId::new(data.lhs as usize)),
                span: SpanId::new(data.rhs as usize),
            }),
            ArenaExprTag::SpawnCommand => ArenaExprKind::Spawn(ArenaSpawnForm {
                target: ArenaSpawnTarget::Command(ExprId::new(data.lhs as usize)),
                span: SpanId::new(data.rhs as usize),
            }),
            ArenaExprTag::Wait => ArenaExprKind::Wait(ArenaWaitForm {
                target: ExprId::new(data.lhs as usize),
                span: SpanId::new(data.rhs as usize),
            }),
            ArenaExprTag::BuilderCall => ArenaExprKind::BuilderCall {
                call: ExprId::new(data.lhs as usize),
                block: BuilderBlockId::new(data.rhs as usize),
            },
            ArenaExprTag::Try => ArenaExprKind::Try(ExprId::new(data.lhs as usize)),
            ArenaExprTag::Require => ArenaExprKind::Require {
                value: ExprId::new(data.lhs as usize),
                schema: TypeExprId::new(data.rhs as usize),
            },
            ArenaExprTag::Loop => ArenaExprKind::Loop {
                block: BlockId::new(data.lhs as usize),
            },
            ArenaExprTag::Retry => {
                let raw = range_slice(&self.extra, range_from_data(data));
                ArenaExprKind::Retry {
                    delays: ArenaRange::new(raw[0] as usize, raw[1] as usize),
                    block: BlockId::new(raw[2] as usize),
                }
            }
        }
    }

    pub fn pattern(&self, id: PatternId) -> &ArenaPattern {
        &self.patterns[id.index()]
    }

    pub fn binding_target(&self, id: BindingTargetId) -> &ArenaBindingTarget {
        &self.binding_targets[id.index()]
    }

    pub fn assign_target(&self, id: AssignTargetId) -> &ArenaAssignTarget {
        &self.assign_targets[id.index()]
    }

    pub fn type_expr_span(&self, id: TypeExprId) -> Span {
        let index = id.index();
        self.inline_span(
            self.type_expr_spans.get(index),
            index,
            &self.type_expr_span_source_overrides,
        )
    }

    /// True when the type expression is a bare `Named(name)` matching `name`.
    pub fn type_expr_named(&self, id: TypeExprId, name: &str) -> bool {
        let data = self.type_expr_data[id.index()];
        matches!(self.type_expr_tags[id.index()], ArenaTypeExprTag::Named)
            && Name::from_symbol(Symbol::from_raw(data.lhs)).as_str() == name
    }

    pub fn use_stmt(&self, id: UseStmtId) -> &ArenaUseStmt {
        &self.use_stmts[id.index()]
    }

    pub fn type_def(&self, id: TypeDefId) -> &ArenaTypeDef {
        &self.type_defs[id.index()]
    }

    pub fn error_def(&self, id: ErrorDefId) -> &ArenaErrorDef {
        &self.error_defs[id.index()]
    }

    pub fn schema_fields(&self, range: ArenaRange) -> &[ArenaSchemaField] {
        range_slice(&self.schema_fields, range)
    }

    pub fn module_contract_entries(&self, range: ArenaRange) -> &[ArenaModuleContractEntry] {
        range_slice(&self.module_contract_entries, range)
    }

    pub fn tag_variants(&self, range: ArenaRange) -> &[ArenaTagVariant] {
        range_slice(&self.tag_variants, range)
    }

    pub fn error_variants(&self, range: ArenaRange) -> &[ArenaErrorVariant] {
        range_slice(&self.error_variants, range)
    }

    pub fn error_fields(&self, range: ArenaRange) -> &[ArenaErrorField] {
        range_slice(&self.error_fields, range)
    }

    pub fn function_def(&self, id: FunctionDefId) -> &ArenaFunctionDef {
        &self.function_defs[id.index()]
    }

    pub fn signal_hook(&self, id: SignalHookId) -> &ArenaSignalHook {
        &self.signal_hooks[id.index()]
    }

    pub fn command_stmt(&self, id: CommandStmtId) -> &ArenaCommandStmt {
        &self.command_stmts[id.index()]
    }

    pub fn int_literal(&self, id: IntLiteralId) -> &IntLiteral {
        &self.int_literals[id.index()]
    }

    pub fn float_literal(&self, id: FloatLiteralId) -> &FloatLiteral {
        &self.float_literals[id.index()]
    }

    pub fn duration_literal(&self, id: DurationLiteralId) -> &DurationLiteral {
        &self.duration_literals[id.index()]
    }

    pub fn string_literal(&self, id: StringLiteralId) -> &Arc<str> {
        &self.string_literals[id.index()]
    }

    pub fn bytes_literal(&self, id: BytesLiteralId) -> &Arc<[u8]> {
        &self.bytes_literals[id.index()]
    }

    pub fn run_form(&self, id: RunFormId) -> &ArenaRunForm {
        &self.run_forms[id.index()]
    }

    pub fn builder_block(&self, id: BuilderBlockId) -> &ArenaBuilderBlock {
        &self.builder_blocks[id.index()]
    }

    pub fn extra_range(&self, range: ArenaRange) -> &[u32] {
        range_slice(&self.extra, range)
    }

    pub fn stmt_ids(&self, range: ArenaRange) -> impl Iterator<Item = StmtId> + '_ {
        self.extra_range(range)
            .iter()
            .copied()
            .map(|index| StmtId::new(index as usize))
    }

    pub fn expr_ids(&self, range: ArenaRange) -> impl Iterator<Item = ExprId> + '_ {
        self.extra_range(range)
            .iter()
            .copied()
            .map(|index| ExprId::new(index as usize))
    }

    pub fn pattern_ids(&self, range: ArenaRange) -> impl Iterator<Item = PatternId> + '_ {
        self.extra_range(range)
            .iter()
            .copied()
            .map(|index| PatternId::new(index as usize))
    }

    pub fn names(&self, range: ArenaRange) -> impl Iterator<Item = Name> + '_ {
        self.extra_range(range)
            .iter()
            .copied()
            .map(|raw| Name::from_symbol(Symbol::from_raw(raw)))
    }

    pub fn effects(&self, range: ArenaRange) -> impl Iterator<Item = Effect> + '_ {
        self.extra_range(range)
            .iter()
            .copied()
            .map(effect_from_code)
    }

    pub fn block_params(&self, range: ArenaRange) -> &[ArenaBlockParam] {
        range_slice(&self.block_params, range)
    }

    pub fn params(&self, range: ArenaRange) -> &[ArenaParam] {
        range_slice(&self.params, range)
    }

    pub fn if_branches(&self, range: ArenaRange) -> &[ArenaIfBranch] {
        range_slice(&self.if_branches, range)
    }

    pub fn with_bindings(&self, range: ArenaRange) -> &[ArenaWithBinding] {
        range_slice(&self.with_bindings, range)
    }

    pub fn match_arms(&self, range: ArenaRange) -> &[ArenaMatchArm] {
        range_slice(&self.match_arms, range)
    }

    pub fn destructure_fields(&self, range: ArenaRange) -> &[ArenaDestructureField] {
        range_slice(&self.destructure_fields, range)
    }

    pub fn pattern_fields(&self, range: ArenaRange) -> &[ArenaRecordPatternField] {
        range_slice(&self.pattern_fields, range)
    }

    pub fn fmt_parts(&self, range: ArenaRange) -> impl ExactSizeIterator<Item = ArenaFmtPart> + '_ {
        (range.start..range.start + range.len).map(|raw| self.fmt_part_at(raw as usize))
    }

    pub fn if_expr_branches(&self, range: ArenaRange) -> &[ArenaIfExprBranch] {
        range_slice(&self.if_expr_branches, range)
    }

    pub fn match_expr_arms(&self, range: ArenaRange) -> &[ArenaMatchExprArm] {
        range_slice(&self.match_expr_arms, range)
    }

    pub fn record_fields(&self, range: ArenaRange) -> &[ArenaRecordField] {
        range_slice(&self.record_fields, range)
    }

    pub fn call_args(&self, range: ArenaRange) -> &[ArenaCallArg] {
        range_slice(&self.call_args, range)
    }

    pub fn pipe_stages(&self, range: ArenaRange) -> &[ArenaPipeStage] {
        range_slice(&self.pipe_stages, range)
    }

    pub fn stream_stages(&self, range: ArenaRange) -> &[ArenaStreamStage] {
        range_slice(&self.stream_stages, range)
    }

    pub fn stream_options(&self, range: ArenaRange) -> &[ArenaStreamStageOption] {
        range_slice(&self.stream_options, range)
    }

    pub fn builder_entries(&self, range: ArenaRange) -> &[ArenaBuilderEntry] {
        range_slice(&self.builder_entries, range)
    }

    pub fn command_args(&self, range: ArenaRange) -> &[ArenaCommandArg] {
        range_slice(&self.command_args, range)
    }

    pub fn env_assignments(&self, range: ArenaRange) -> &[ArenaEnvAssignment] {
        range_slice(&self.env_assignments, range)
    }

    pub fn run_segments(&self, range: ArenaRange) -> &[ArenaRunSegment] {
        range_slice(&self.run_segments, range)
    }

    pub fn redirections(&self, range: ArenaRange) -> &[ArenaRedirection] {
        range_slice(&self.redirections, range)
    }

    pub fn word_parts(
        &self,
        range: ArenaRange,
    ) -> impl ExactSizeIterator<Item = ArenaWordPart> + '_ {
        (range.start..range.start + range.len).map(|raw| self.word_part_at(raw as usize))
    }

    fn fmt_part_at(&self, index: usize) -> ArenaFmtPart {
        let data = self.fmt_part_data[index];
        match self.fmt_part_tags[index] {
            ArenaFmtPartTag::Text => {
                ArenaFmtPart::Text(self.text_at(TextLiteralId::new(data.lhs as usize)))
            }
            ArenaFmtPartTag::Expr => ArenaFmtPart::Expr(
                ExprId::new(data.lhs as usize),
                format_spec_from_raw(data.rhs),
            ),
        }
    }

    fn word_part_at(&self, index: usize) -> ArenaWordPart {
        let data = self.word_part_data[index];
        match self.word_part_tags[index] {
            ArenaWordPartTag::Bare => {
                ArenaWordPart::Bare(self.text_at(TextLiteralId::new(data.lhs as usize)))
            }
            ArenaWordPartTag::Quoted => {
                ArenaWordPart::Quoted(self.text_at(TextLiteralId::new(data.lhs as usize)))
            }
            ArenaWordPartTag::Shorthand => ArenaWordPart::Shorthand(ExprId::new(data.lhs as usize)),
            ArenaWordPartTag::Interpolation => {
                ArenaWordPart::Interpolation(ExprId::new(data.lhs as usize))
            }
        }
    }

    fn text_at(&self, id: TextLiteralId) -> ArenaText {
        let index = id.index();
        let data = self.text_data[index];
        match self.text_tags[index] {
            ArenaTextTag::Source => ArenaText::Source(ArenaByteSpan {
                start: data.lhs,
                len: data.rhs,
            }),
            ArenaTextTag::Cooked => ArenaText::Cooked(self.cooked_texts[data.lhs as usize].clone()),
        }
    }

    pub fn text_value<'a>(&'a self, text: &'a ArenaText, source: &'a str) -> Option<&'a str> {
        match text {
            ArenaText::Source(bytes) => {
                let span = bytes.to_span(self.span_source_id?);
                source.get(span.start()..span.end())
            }
            ArenaText::Cooked(value) => Some(value.as_ref()),
        }
    }
}

fn vec_capacity_bytes<T>(items: &Vec<T>) -> usize {
    items.capacity() * size_of::<T>()
}

fn range_slice<T>(items: &[T], range: ArenaRange) -> &[T] {
    let start = range.start as usize;
    let end = start + range.len();
    &items[start..end]
}

const ARENA_ABSENT: u32 = u32::MAX;
const ARENA_RUN_FORM_FLAG: u32 = 1 << 31;

fn raw_index(index: usize) -> u32 {
    u32::try_from(index).expect("AST arena exceeded u32 indexes")
}

fn raw_index_for_tagged_id(index: usize) -> u32 {
    let raw = raw_index(index);
    assert!(
        raw < ARENA_RUN_FORM_FLAG,
        "AST arena exceeded compact tagged-id range"
    );
    raw
}

fn raw_stmt_id(id: StmtId) -> u32 {
    raw_index(id.index())
}

fn raw_expr_id(id: ExprId) -> u32 {
    raw_index(id.index())
}

fn raw_tagged_expr_id(id: ExprId) -> u32 {
    raw_index_for_tagged_id(id.index())
}

fn raw_assign_target_id(id: AssignTargetId) -> u32 {
    raw_index(id.index())
}

fn raw_binding_target_id(id: BindingTargetId) -> u32 {
    raw_index(id.index())
}

fn raw_type_expr_id(id: TypeExprId) -> u32 {
    raw_index(id.index())
}

fn raw_block_id(id: BlockId) -> u32 {
    raw_index(id.index())
}

fn raw_run_form_id(id: RunFormId) -> u32 {
    raw_index(id.index())
}

fn raw_tagged_run_form_id(id: RunFormId) -> u32 {
    raw_index_for_tagged_id(id.index()) | ARENA_RUN_FORM_FLAG
}

fn raw_builder_block_id(id: BuilderBlockId) -> u32 {
    raw_index(id.index())
}

fn raw_span_id(id: SpanId) -> u32 {
    raw_index(id.index())
}

fn raw_use_stmt_id(id: UseStmtId) -> u32 {
    raw_index(id.index())
}

fn raw_type_def_id(id: TypeDefId) -> u32 {
    raw_index(id.index())
}

fn raw_error_def_id(id: ErrorDefId) -> u32 {
    raw_index(id.index())
}

fn raw_function_def_id(id: FunctionDefId) -> u32 {
    raw_index(id.index())
}

fn raw_signal_hook_id(id: SignalHookId) -> u32 {
    raw_index(id.index())
}

fn raw_command_stmt_id(id: CommandStmtId) -> u32 {
    raw_index(id.index())
}

fn raw_text_literal_id(id: TextLiteralId) -> u32 {
    raw_index(id.index())
}

fn optional_raw_expr_id(id: Option<ExprId>) -> u32 {
    id.map(raw_expr_id).unwrap_or(ARENA_ABSENT)
}

fn optional_expr_id(raw: u32) -> Option<ExprId> {
    (raw != ARENA_ABSENT).then(|| ExprId::new(raw as usize))
}

fn optional_raw_type_expr_id(id: Option<TypeExprId>) -> u32 {
    id.map(raw_type_expr_id).unwrap_or(ARENA_ABSENT)
}

fn optional_type_expr_id(raw: u32) -> Option<TypeExprId> {
    (raw != ARENA_ABSENT).then(|| TypeExprId::new(raw as usize))
}

fn optional_raw_name(name: Option<Name>) -> u32 {
    name.map(|name| name.symbol().raw()).unwrap_or(ARENA_ABSENT)
}

fn optional_name(raw: u32) -> Option<Name> {
    (raw != ARENA_ABSENT).then(|| Name::from_symbol(Symbol::from_raw(raw)))
}

fn raw_format_spec(spec: Option<&FormatSpec>) -> u32 {
    let Some(spec) = spec else {
        return ARENA_ABSENT;
    };
    let width = raw_index(spec.width);
    assert!(
        width <= ARENA_ABSENT >> 2,
        "format width exceeded compact arena encoding"
    );
    let kind = match spec.kind {
        FormatSpecKind::RightAlign => 0,
        FormatSpecKind::LeftAlign => 1,
        FormatSpecKind::ZeroPad => 2,
    };
    (width << 2) | kind
}

fn format_spec_from_raw(raw: u32) -> Option<FormatSpec> {
    if raw == ARENA_ABSENT {
        return None;
    }
    let kind = match raw & 0b11 {
        0 => FormatSpecKind::RightAlign,
        1 => FormatSpecKind::LeftAlign,
        2 => FormatSpecKind::ZeroPad,
        _ => unreachable!("invalid compact format spec kind"),
    };
    Some(FormatSpec {
        kind,
        width: (raw >> 2) as usize,
    })
}

fn raw_expr_or_run(value: ArenaExprOrRun) -> u32 {
    match value {
        ArenaExprOrRun::Expr(id) => raw_tagged_expr_id(id),
        ArenaExprOrRun::Run(id) => raw_tagged_run_form_id(id),
    }
}

fn expr_or_run_from_raw(raw: u32) -> ArenaExprOrRun {
    if raw & ARENA_RUN_FORM_FLAG == 0 {
        ArenaExprOrRun::Expr(ExprId::new(raw as usize))
    } else {
        ArenaExprOrRun::Run(RunFormId::new((raw & !ARENA_RUN_FORM_FLAG) as usize))
    }
}

fn range_stmt_data(range: ArenaRange) -> ArenaStmtData {
    ArenaStmtData::new(range.start, range.len)
}

fn range_from_stmt_data(data: ArenaStmtData) -> ArenaRange {
    ArenaRange {
        start: data.lhs,
        len: data.rhs,
    }
}

fn range_data(range: ArenaRange) -> ArenaExprData {
    ArenaExprData::new(range.start, range.len)
}

fn range_from_data(data: ArenaExprData) -> ArenaRange {
    ArenaRange {
        start: data.lhs,
        len: data.rhs,
    }
}

fn binary_expr_tag(op: BinaryOp) -> ArenaExprTag {
    match op {
        BinaryOp::ResultFallback => ArenaExprTag::BinaryResultFallback,
        BinaryOp::Or => ArenaExprTag::BinaryOr,
        BinaryOp::And => ArenaExprTag::BinaryAnd,
        BinaryOp::Eq => ArenaExprTag::BinaryEq,
        BinaryOp::Ne => ArenaExprTag::BinaryNe,
        BinaryOp::Lt => ArenaExprTag::BinaryLt,
        BinaryOp::Le => ArenaExprTag::BinaryLe,
        BinaryOp::Gt => ArenaExprTag::BinaryGt,
        BinaryOp::Ge => ArenaExprTag::BinaryGe,
        BinaryOp::In => ArenaExprTag::BinaryIn,
        BinaryOp::NotIn => ArenaExprTag::BinaryNotIn,
        BinaryOp::Add => ArenaExprTag::BinaryAdd,
        BinaryOp::Sub => ArenaExprTag::BinarySub,
        BinaryOp::Mul => ArenaExprTag::BinaryMul,
        BinaryOp::Div => ArenaExprTag::BinaryDiv,
        BinaryOp::Rem => ArenaExprTag::BinaryRem,
    }
}

fn binary_op_from_expr_tag(tag: ArenaExprTag) -> BinaryOp {
    match tag {
        ArenaExprTag::BinaryResultFallback => BinaryOp::ResultFallback,
        ArenaExprTag::BinaryOr => BinaryOp::Or,
        ArenaExprTag::BinaryAnd => BinaryOp::And,
        ArenaExprTag::BinaryEq => BinaryOp::Eq,
        ArenaExprTag::BinaryNe => BinaryOp::Ne,
        ArenaExprTag::BinaryLt => BinaryOp::Lt,
        ArenaExprTag::BinaryLe => BinaryOp::Le,
        ArenaExprTag::BinaryGt => BinaryOp::Gt,
        ArenaExprTag::BinaryGe => BinaryOp::Ge,
        ArenaExprTag::BinaryIn => BinaryOp::In,
        ArenaExprTag::BinaryNotIn => BinaryOp::NotIn,
        ArenaExprTag::BinaryAdd => BinaryOp::Add,
        ArenaExprTag::BinarySub => BinaryOp::Sub,
        ArenaExprTag::BinaryMul => BinaryOp::Mul,
        ArenaExprTag::BinaryDiv => BinaryOp::Div,
        ArenaExprTag::BinaryRem => BinaryOp::Rem,
        _ => panic!("arena expression tag is not binary"),
    }
}

fn assign_stmt_tag(op: AssignOp) -> ArenaStmtTag {
    match op {
        AssignOp::Set => ArenaStmtTag::AssignSet,
        AssignOp::Add => ArenaStmtTag::AssignAdd,
        AssignOp::Sub => ArenaStmtTag::AssignSub,
        AssignOp::Mul => ArenaStmtTag::AssignMul,
        AssignOp::Div => ArenaStmtTag::AssignDiv,
        AssignOp::Rem => ArenaStmtTag::AssignRem,
    }
}

fn assign_op_from_stmt_tag(tag: ArenaStmtTag) -> AssignOp {
    match tag {
        ArenaStmtTag::AssignSet => AssignOp::Set,
        ArenaStmtTag::AssignAdd => AssignOp::Add,
        ArenaStmtTag::AssignSub => AssignOp::Sub,
        ArenaStmtTag::AssignMul => AssignOp::Mul,
        ArenaStmtTag::AssignDiv => AssignOp::Div,
        ArenaStmtTag::AssignRem => AssignOp::Rem,
        _ => panic!("arena statement tag is not assignment"),
    }
}

fn effect_code(effect: &Effect) -> u32 {
    match effect {
        Effect::Fs => 0,
        Effect::Net => 1,
        Effect::Process => 2,
        Effect::Env => 3,
        Effect::Time => 4,
        Effect::Error => 5,
        Effect::Io => 6,
    }
}

fn effect_from_code(code: u32) -> Effect {
    match code {
        0 => Effect::Fs,
        1 => Effect::Net,
        2 => Effect::Process,
        3 => Effect::Env,
        4 => Effect::Time,
        5 => Effect::Error,
        6 => Effect::Io,
        _ => panic!("invalid arena effect code {code}"),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaUserModule {
    pub key: String,
    pub name: Name,
    pub statements: ArenaRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ArenaStmtTag {
    Use,
    Export,
    TypeDef,
    ErrorDef,
    LetExprNoTy,
    Let,
    VarExprNoTy,
    Var,
    AssignSet,
    AssignAdd,
    AssignSub,
    AssignMul,
    AssignDiv,
    AssignRem,
    ProcDef,
    PureDef,
    StreamDef,
    SignalHook,
    ReturnNone,
    ReturnExpr,
    ReturnRun,
    YieldExpr,
    YieldRun,
    DeferExpr,
    DeferRun,
    IfNoElse,
    IfElse,
    While,
    For,
    With,
    Loop,
    Guard,
    GuardedStmt,
    GuardedStmtNegated,
    BreakNone,
    BreakValue,
    Continue,
    Match,
    Command,
    TailBareIdent,
    Expr,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaStmtData {
    pub lhs: u32,
    pub rhs: u32,
}

impl ArenaStmtData {
    const ZERO: Self = Self { lhs: 0, rhs: 0 };

    const fn new(lhs: u32, rhs: u32) -> Self {
        Self { lhs, rhs }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaStmt {
    pub kind: ArenaStmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaStmtKind {
    Use(UseStmtId),
    Export(StmtId),
    TypeDef(TypeDefId),
    ErrorDef(ErrorDefId),
    Let {
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
    },
    Var {
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
    },
    Assign {
        target: AssignTargetId,
        op: AssignOp,
        value: ArenaExprOrRun,
    },
    ProcDef(FunctionDefId),
    PureDef(FunctionDefId),
    StreamDef(FunctionDefId),
    SignalHook(SignalHookId),
    Return(Option<ArenaExprOrRun>),
    Yield(ArenaExprOrRun),
    Defer(ArenaExprOrRun),
    If {
        branches: ArenaRange,
        else_block: Option<BlockId>,
    },
    While {
        condition: ExprId,
        block: BlockId,
    },
    For {
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
    },
    With {
        bindings: ArenaRange,
        body: BlockId,
        else_param: Option<Name>,
        else_block: BlockId,
    },
    Loop {
        block: BlockId,
    },
    Guard {
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        else_param: Option<Name>,
        else_block: BlockId,
    },
    GuardedStmt {
        stmt: StmtId,
        negate: bool,
        condition: ExprId,
    },
    Break {
        value: Option<ExprId>,
    },
    Continue,
    Match {
        value: ExprId,
        arms: ArenaRange,
    },
    Command(CommandStmtId),
    TailBareIdent(Name),
    Expr(ExprId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaUseStmt {
    pub path: ArenaRange,
    pub alias: Option<Name>,
    pub resolved: Option<Arc<str>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaTypeDef {
    pub name: Name,
    pub body: ArenaTypeDefBody,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaTypeDefBody {
    Alias(TypeExprId),
    RecordSchema(ArenaRange),
    ModuleContract(ArenaRange),
    TagUnion(ArenaRange),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaSchemaField {
    pub name: Name,
    pub ty: TypeExprId,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaModuleContractEntry {
    pub name: Name,
    pub optional: bool,
    pub kind: ArenaModuleContractEntryKind,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaModuleContractEntryKind {
    Value(TypeExprId),
    Proc {
        params: ArenaRange,
        effects: Option<ArenaRange>,
        return_ty: TypeExprId,
    },
    Pure {
        params: ArenaRange,
        return_ty: TypeExprId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaTagVariant {
    pub name: Name,
    pub fields: ArenaRange,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaErrorDef {
    pub name: Name,
    pub variants: ArenaRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaErrorVariant {
    pub name: Name,
    pub fields: ArenaRange,
    pub facets: ArenaRange,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaErrorField {
    pub name: Name,
    pub ty: TypeExprId,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaBlock {
    pub params: ArenaRange,
    pub statements: ArenaRange,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaBlockParam {
    pub name: Name,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaFunctionDef {
    pub name: Name,
    pub params: ArenaRange,
    pub effects: Option<ArenaRange>,
    pub return_ty: TypeExprId,
    pub return_ty_defaulted: bool,
    pub body: BlockId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaSignalHook {
    pub signal: Name,
    pub options: SignalHookOptions,
    pub effects: ArenaRange,
    pub body: BlockId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaParam {
    pub name: Name,
    pub ty: TypeExprId,
    pub ty_defaulted: bool,
    pub default: Option<ExprId>,
    pub rest: bool,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaIfBranch {
    pub condition: ExprId,
    pub block: BlockId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaWithBinding {
    pub name: Name,
    pub initializer: ExprId,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaBindingTarget {
    pub kind: ArenaBindingTargetKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaBindingTargetKind {
    Name(Name),
    Record { fields: ArenaRange, rest: bool },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaDestructureField {
    pub name: Name,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaMatchArm {
    pub pattern: PatternId,
    pub guard: Option<ExprId>,
    pub block: BlockId,
    pub span: SpanId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArenaExprOrRun {
    Expr(ExprId),
    Run(RunFormId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaAssignTarget {
    pub kind: ArenaAssignTargetKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaAssignTargetKind {
    Name(Name),
    Field { base: AssignTargetId, name: Name },
    Index { base: AssignTargetId, index: ExprId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaPattern {
    pub kind: ArenaPatternKind,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaPatternKind {
    Wildcard,
    Binding(Name),
    Type {
        binding: Option<Name>,
        ty: TypeExprId,
    },
    Literal(ExprId),
    Record {
        fields: ArenaRange,
        rest: bool,
    },
    Alternation(ArenaRange),
    Constructor {
        name: Name,
        arg: Option<PatternId>,
    },
    ErrorVariant {
        family: Name,
        variant: Name,
        fields: ArenaRange,
    },
    Facet(Name),
    Tuple(ArenaRange),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaRecordPatternField {
    pub name: Name,
    pub pattern: PatternId,
    pub span: SpanId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ArenaExprTag {
    Null,
    BoolFalse,
    BoolTrue,
    Int,
    Float,
    Duration,
    Str,
    PathStr,
    GlobStr,
    FmtString,
    PathFmtString,
    Bytes,
    Ident,
    Item,
    LastStatus,
    List,
    ListComp,
    MapComp,
    Record,
    If,
    Match,
    UnaryNot,
    UnaryNeg,
    BinaryResultFallback,
    BinaryOr,
    BinaryAnd,
    BinaryEq,
    BinaryNe,
    BinaryLt,
    BinaryLe,
    BinaryGt,
    BinaryGe,
    BinaryIn,
    BinaryNotIn,
    BinaryAdd,
    BinarySub,
    BinaryMul,
    BinaryDiv,
    BinaryRem,
    Call,
    Field,
    NullSafeField,
    Index,
    Slice,
    EnvGetStr,
    EnvGetPath,
    EnvGetPathList,
    EnvPathList,
    Pipeline,
    StructuredPipeline,
    Run,
    SpawnRun,
    SpawnCommand,
    Wait,
    BuilderCall,
    Try,
    Require,
    Loop,
    Retry,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaExprData {
    pub lhs: u32,
    pub rhs: u32,
}

impl ArenaExprData {
    const ZERO: Self = Self { lhs: 0, rhs: 0 };

    const fn new(lhs: u32, rhs: u32) -> Self {
        Self { lhs, rhs }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaExpr {
    pub kind: ArenaExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaExprKind {
    Null,
    Bool(bool),
    Int(IntLiteralId),
    Float(FloatLiteralId),
    Duration(DurationLiteralId),
    Str(StringLiteralId),
    PathStr(StringLiteralId),
    GlobStr(StringLiteralId),
    FmtString(ArenaRange),
    PathFmtString(ArenaRange),
    Bytes(BytesLiteralId),
    Ident(Name),
    Item,
    LastStatus,
    List(ArenaRange),
    ListComp {
        expr: ExprId,
        target: BindingTargetId,
        iter: ExprId,
        condition: Option<ExprId>,
    },
    MapComp {
        key: ExprId,
        value: ExprId,
        target: BindingTargetId,
        iter: ExprId,
        condition: Option<ExprId>,
    },
    Record(ArenaRange),
    If {
        branches: ArenaRange,
        else_value: ExprId,
    },
    Match {
        value: ExprId,
        arms: ArenaRange,
    },
    Unary {
        op: UnaryOp,
        expr: ExprId,
    },
    Binary {
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
    },
    Call {
        callee: ExprId,
        args: ArenaRange,
    },
    Field {
        base: ExprId,
        name: Name,
    },
    NullSafeField {
        base: ExprId,
        name: Name,
    },
    Index {
        base: ExprId,
        index: ExprId,
    },
    Slice {
        base: ExprId,
        start: Option<ExprId>,
        end: Option<ExprId>,
    },
    EnvGet {
        kind: EnvGetKind,
        name: Name,
    },
    EnvPathList,
    Pipeline {
        input: ExprId,
        stages: ArenaRange,
    },
    StructuredPipeline {
        input: ExprId,
        stages: ArenaRange,
    },
    Run(RunFormId),
    Spawn(ArenaSpawnForm),
    Wait(ArenaWaitForm),
    BuilderCall {
        call: ExprId,
        block: BuilderBlockId,
    },
    Try(ExprId),
    Require {
        value: ExprId,
        schema: TypeExprId,
    },
    Loop {
        block: BlockId,
    },
    Retry {
        delays: ArenaRange,
        block: BlockId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaFmtPart {
    Text(ArenaText),
    Expr(ExprId, Option<FormatSpec>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaText {
    Source(ArenaByteSpan),
    Cooked(Arc<str>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ArenaTextTag {
    Source,
    Cooked,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaTextData {
    pub lhs: u32,
    pub rhs: u32,
}

impl ArenaTextData {
    const fn new(lhs: u32, rhs: u32) -> Self {
        Self { lhs, rhs }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ArenaFmtPartTag {
    Text,
    Expr,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaFmtPartData {
    pub lhs: u32,
    pub rhs: u32,
}

impl ArenaFmtPartData {
    const fn new(lhs: u32, rhs: u32) -> Self {
        Self { lhs, rhs }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArenaSpanMarks {
    spans: usize,
    expr_spans: usize,
    stmt_spans: usize,
    type_expr_spans: usize,
    text_data: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaIfExprBranch {
    pub condition: ExprId,
    pub value: ExprId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaMatchExprArm {
    pub pattern: PatternId,
    pub guard: Option<ExprId>,
    pub value: ExprId,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaSpawnForm {
    pub target: ArenaSpawnTarget,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaSpawnTarget {
    Run(RunFormId),
    Command(ExprId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaWaitForm {
    pub target: ExprId,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaRecordField {
    pub kind: ArenaRecordFieldKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaRecordFieldInput {
    Named {
        name: Name,
        value: ExprId,
        span: Span,
    },
    Shorthand {
        name: Name,
        span: Span,
    },
    Spread {
        expr: ExprId,
        span: Span,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaRecordFieldKind {
    Named {
        name: Name,
        value: ExprId,
        span: SpanId,
    },
    Shorthand {
        name: Name,
        span: SpanId,
    },
    Spread {
        expr: ExprId,
        span: SpanId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaCallArg {
    pub kind: ArenaCallArgKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaCallArgInput {
    Positional(ExprId),
    Splice {
        value: ExprId,
        span: Span,
    },
    Named {
        name: Name,
        value: ExprId,
        span: Span,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaCallArgKind {
    Positional(ExprId),
    Splice {
        value: ExprId,
        span: SpanId,
    },
    Named {
        name: Name,
        value: ExprId,
        span: SpanId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaPipeStage {
    pub kind: ArenaPipeStageKind,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaPipeStageKind {
    Expr(ExprId),
    Stream(ArenaStreamStage),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaStreamStage {
    pub kind: StreamStageKind,
    pub options: ArenaRange,
    pub block: Option<BlockId>,
    pub args: ArenaRange,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaStreamStageOption {
    pub name: Name,
    pub value: Option<ExprId>,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaBuilderBlock {
    pub entries: ArenaRange,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaBuilderEntry {
    pub kind: ArenaBuilderEntryKind,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaBuilderEntryKind {
    Field {
        name: Name,
        value: ExprId,
    },
    Entry {
        name: Name,
        args: ArenaRange,
        block: Option<BuilderBlockId>,
    },
    Task {
        name: Name,
        block: BlockId,
    },
    Stmt(StmtId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaCommandStmt {
    pub command: ArenaCommand,
    pub propagate: bool,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaCommand {
    Proc {
        name: Name,
        args: ArenaRange,
    },
    Core {
        name: CoreCommand,
        args: ArenaRange,
        env: ArenaRange,
        block: Option<BlockId>,
    },
    Run(RunFormId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaRunForm {
    pub segments: ArenaRange,
    pub propagate: bool,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaRunSegment {
    pub kind: RunKind,
    pub builtin: bool,
    pub timeout: Option<ExprId>,
    pub cpu_max: Option<ExprId>,
    pub env: ArenaRange,
    pub grouped: bool,
    pub target: ArenaCommandArg,
    pub args: ArenaRange,
    pub redirections: ArenaRange,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaRedirection {
    pub kind: RedirectionKind,
    pub target: ArenaRedirectionTarget,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaRedirectionTarget {
    Path(ArenaCommandArg),
    Fd(ArenaCommandArg),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaEnvAssignment {
    pub name: Name,
    pub value: ArenaEnvAssignmentValue,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaEnvAssignmentValue {
    CommandArg(ArenaCommandArg),
    Expr(ExprId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArenaCommandArg {
    pub kind: ArenaCommandArgKind,
    pub span: SpanId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaCommandArgKind {
    Word(ArenaRange),
    SpliceName(Name),
    SpliceExpr(ExprId),
    Typed(ExprId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaWordPart {
    Bare(ArenaText),
    Quoted(ArenaText),
    Shorthand(ExprId),
    Interpolation(ExprId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ArenaWordPartTag {
    Bare,
    Quoted,
    Shorthand,
    Interpolation,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ArenaWordPartData {
    pub lhs: u32,
    pub rhs: u32,
}

impl ArenaWordPartData {
    const fn new(lhs: u32, rhs: u32) -> Self {
        Self { lhs, rhs }
    }
}

#[derive(Default)]
struct ArenaLowerer<'a> {
    arena: AstArena,
    source: Option<&'a str>,
}

impl ArenaLowerer<'_> {
    fn reserve_frontend_capacity(&mut self, tokens: usize) {
        self.arena.spans.reserve(tokens / 10 + 1);
        self.arena.stmt_tags.reserve(tokens / 10 + 1);
        self.arena.stmt_data.reserve(tokens / 10 + 1);
        self.arena.stmt_spans.reserve(tokens / 10 + 1);
        self.arena.blocks.reserve(tokens / 24 + 1);
        let expr_capacity = tokens.saturating_mul(5) / 12 + 1;
        self.arena.expr_tags.reserve(expr_capacity);
        self.arena.expr_data.reserve(expr_capacity);
        self.arena.expr_spans.reserve(expr_capacity);
        self.arena.extra.reserve(tokens / 3 + 1);
    }

    fn pushed_range<T>(table: &mut [T], start: usize) -> ArenaRange {
        ArenaRange::new(start, table.len() - start)
    }

    fn pushed_extra_range(&self, start: usize) -> ArenaRange {
        ArenaRange::new(start, self.arena.extra.len() - start)
    }

    fn span(&mut self, span: Span) -> SpanId {
        let id = SpanId::new(self.arena.spans.len());
        match self.arena.span_source_id {
            Some(source_id) if source_id != span.source_id => {
                self.arena.span_source_overrides.push(ArenaSpanSource {
                    span: raw_index(id.index()),
                    source_id: span.source_id,
                });
            }
            Some(_) => {}
            None => {
                self.arena.span_source_id = Some(span.source_id);
            }
        }
        self.arena.spans.push(ArenaByteSpan::from_span(span));
        id
    }

    fn inline_span_for(
        primary_source: &mut Option<SourceId>,
        index: usize,
        overrides: &mut Vec<ArenaSpanSource>,
        span: Span,
    ) -> ArenaByteSpan {
        match *primary_source {
            Some(source_id) if source_id != span.source_id => {
                overrides.push(ArenaSpanSource {
                    span: raw_index(index),
                    source_id: span.source_id,
                });
            }
            Some(_) => {}
            None => {
                *primary_source = Some(span.source_id);
            }
        }
        ArenaByteSpan::from_span(span)
    }

    fn stmt_inline_span(&mut self, index: usize, span: Span) -> ArenaByteSpan {
        Self::inline_span_for(
            &mut self.arena.span_source_id,
            index,
            &mut self.arena.stmt_span_source_overrides,
            span,
        )
    }

    fn expr_inline_span(&mut self, index: usize, span: Span) -> ArenaByteSpan {
        Self::inline_span_for(
            &mut self.arena.span_source_id,
            index,
            &mut self.arena.expr_span_source_overrides,
            span,
        )
    }

    fn type_expr_inline_span(&mut self, index: usize, span: Span) -> ArenaByteSpan {
        Self::inline_span_for(
            &mut self.arena.span_source_id,
            index,
            &mut self.arena.type_expr_span_source_overrides,
            span,
        )
    }

    fn lower_stmt_id_range(&mut self, stmts: &[StmtId]) -> ArenaRange {
        let start = self.arena.extra.len();
        self.arena
            .extra
            .extend(stmts.iter().map(|id| id.index() as u32));
        self.pushed_extra_range(start)
    }

    fn lower_block_param_range(&mut self, params: &[BlockParam]) -> ArenaRange {
        let start = self.arena.block_params.len();
        for param in params {
            let span = self.span(param.span);
            self.arena.block_params.push(ArenaBlockParam {
                name: param.name,
                span,
            });
        }
        Self::pushed_range(&mut self.arena.block_params, start)
    }

    fn lower_if_branch_id_range(&mut self, branches: &[(ExprId, BlockId)]) -> ArenaRange {
        let start = self.arena.if_branches.len();
        self.arena
            .if_branches
            .extend(branches.iter().map(|(condition, block)| ArenaIfBranch {
                condition: *condition,
                block: *block,
            }));
        Self::pushed_range(&mut self.arena.if_branches, start)
    }

    fn lower_name_range(&mut self, names: &[Name]) -> ArenaRange {
        let start = self.arena.extra.len();
        self.arena
            .extra
            .extend(names.iter().map(|name| name.symbol().raw()));
        self.pushed_extra_range(start)
    }

    fn push_type_expr_row(
        &mut self,
        tag: ArenaTypeExprTag,
        data: ArenaTypeExprData,
        span: Span,
    ) -> TypeExprId {
        let id = TypeExprId::new(self.arena.type_expr_tags.len());
        let span = self.type_expr_inline_span(id.index(), span);
        self.arena.type_expr_tags.push(tag);
        self.arena.type_expr_data.push(data);
        self.arena.type_expr_spans.push(span);
        id
    }

    fn lower_use_stmt(
        &mut self,
        path: &[Name],
        alias: Option<Name>,
        resolved: Option<&String>,
    ) -> UseStmtId {
        let path = self.lower_name_range(path);
        let resolved = resolved.map(|value| Arc::from(value.as_str()));
        let id = UseStmtId::new(self.arena.use_stmts.len());
        self.arena.use_stmts.push(ArenaUseStmt {
            path,
            alias,
            resolved,
        });
        id
    }

    fn push_command_stmt_kind(
        &mut self,
        command: ArenaCommand,
        propagate: bool,
        span: Span,
    ) -> CommandStmtId {
        let id = CommandStmtId::new(self.arena.command_stmts.len());
        let span = self.span(span);
        self.arena.command_stmts.push(ArenaCommandStmt {
            command,
            propagate,
            span,
        });
        id
    }

    fn lower_int_literal(&mut self, value: &IntLiteral) -> IntLiteralId {
        let id = IntLiteralId::new(self.arena.int_literals.len());
        self.arena.int_literals.push(value.clone());
        id
    }

    fn lower_float_literal(&mut self, value: &FloatLiteral) -> FloatLiteralId {
        let id = FloatLiteralId::new(self.arena.float_literals.len());
        self.arena.float_literals.push(value.clone());
        id
    }

    fn lower_duration_literal(&mut self, value: &DurationLiteral) -> DurationLiteralId {
        let id = DurationLiteralId::new(self.arena.duration_literals.len());
        self.arena.duration_literals.push(value.clone());
        id
    }

    fn lower_string_literal(&mut self, value: &Arc<str>) -> StringLiteralId {
        let id = StringLiteralId::new(self.arena.string_literals.len());
        self.arena.string_literals.push(value.clone());
        id
    }

    fn lower_bytes_literal(&mut self, value: &Arc<[u8]>) -> BytesLiteralId {
        let id = BytesLiteralId::new(self.arena.bytes_literals.len());
        self.arena.bytes_literals.push(value.clone());
        id
    }

    fn lower_text_literal(
        &mut self,
        value: &Arc<str>,
        source_id: SourceId,
        search_from: &mut usize,
        search_end: usize,
    ) -> TextLiteralId {
        if let Some(span) = self.source_text_span(value, source_id, search_from, search_end) {
            return self.push_source_text(span);
        }
        self.push_cooked_text(value)
    }

    fn source_text_span(
        &self,
        value: &str,
        source_id: SourceId,
        search_from: &mut usize,
        search_end: usize,
    ) -> Option<Span> {
        let source = self.source?;
        if value.is_empty()
            || *search_from > search_end
            || search_end > source.len()
            || !source.is_char_boundary(*search_from)
            || !source.is_char_boundary(search_end)
        {
            return None;
        }
        let relative = source[*search_from..search_end].find(value)?;
        let start = *search_from + relative;
        let end = start + value.len();
        if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            return None;
        }
        *search_from = end;
        Some(Span::new(source_id, start, end))
    }

    fn push_source_text(&mut self, span: Span) -> TextLiteralId {
        let id = TextLiteralId::new(self.arena.text_tags.len());
        if self.arena.span_source_id.is_none() {
            self.arena.span_source_id = Some(span.source_id);
        }
        if self.arena.span_source_id != Some(span.source_id) {
            let cooked: Arc<str> = Arc::from(
                self.source
                    .and_then(|source| source.get(span.start()..span.end()))
                    .unwrap_or(""),
            );
            return self.push_cooked_text(&cooked);
        }
        self.arena.text_tags.push(ArenaTextTag::Source);
        self.arena.text_data.push(ArenaTextData::new(
            raw_index(span.start()),
            raw_index(span.end() - span.start()),
        ));
        id
    }

    fn push_cooked_text(&mut self, value: &Arc<str>) -> TextLiteralId {
        let id = TextLiteralId::new(self.arena.text_tags.len());
        let cooked = raw_index(self.arena.cooked_texts.len());
        self.arena.cooked_texts.push(value.clone());
        self.arena.text_tags.push(ArenaTextTag::Cooked);
        self.arena.text_data.push(ArenaTextData::new(cooked, 0));
        id
    }

    fn lower_match_arm_id_range(&mut self, arms: &[ArenaMatchArm]) -> ArenaRange {
        let start = self.arena.match_arms.len();
        self.arena.match_arms.extend_from_slice(arms);
        Self::pushed_range(&mut self.arena.match_arms, start)
    }

    fn lower_expr_id_range(&mut self, exprs: &[ExprId]) -> ArenaRange {
        let start = self.arena.extra.len();
        self.arena
            .extra
            .extend(exprs.iter().map(|id| id.index() as u32));
        self.pushed_extra_range(start)
    }

    fn push_expr_extra(&mut self, values: &[u32]) -> ArenaExprData {
        let start = self.arena.extra.len();
        self.arena.extra.extend_from_slice(values);
        range_data(ArenaRange::new(start, values.len()))
    }

    fn push_stmt_extra(&mut self, values: &[u32]) -> ArenaStmtData {
        let start = self.arena.extra.len();
        self.arena.extra.extend_from_slice(values);
        range_stmt_data(ArenaRange::new(start, values.len()))
    }

    fn lower_if_expr_branch_id_range(&mut self, branches: &[ArenaIfExprBranch]) -> ArenaRange {
        let start = self.arena.if_expr_branches.len();
        self.arena.if_expr_branches.extend_from_slice(branches);
        Self::pushed_range(&mut self.arena.if_expr_branches, start)
    }

    fn lower_match_expr_arm_id_range(&mut self, arms: &[ArenaMatchExprArm]) -> ArenaRange {
        let start = self.arena.match_expr_arms.len();
        self.arena.match_expr_arms.extend_from_slice(arms);
        Self::pushed_range(&mut self.arena.match_expr_arms, start)
    }

    fn lower_record_field_input_range(&mut self, fields: &[ArenaRecordFieldInput]) -> ArenaRange {
        lower_table_range!(
            self,
            fields,
            record_fields,
            field,
            self.lower_record_field_input(field)
        )
    }

    fn commit_call_arg_input_range(&mut self, args: &[ArenaCallArgInput]) -> ArenaRange {
        lower_table_range!(self, args, call_args, arg, self.commit_call_arg_input(arg))
    }

    fn lower_run_segment_id_range(&mut self, segments: &[ArenaRunSegment]) -> ArenaRange {
        let start = self.arena.run_segments.len();
        self.arena.run_segments.extend_from_slice(segments);
        Self::pushed_range(&mut self.arena.run_segments, start)
    }

    fn encode_stmt_kind(&mut self, kind: ArenaStmtKind) -> (ArenaStmtTag, ArenaStmtData) {
        match kind {
            ArenaStmtKind::Use(id) => (
                ArenaStmtTag::Use,
                ArenaStmtData::new(raw_use_stmt_id(id), 0),
            ),
            ArenaStmtKind::Export(id) => {
                (ArenaStmtTag::Export, ArenaStmtData::new(raw_stmt_id(id), 0))
            }
            ArenaStmtKind::TypeDef(id) => (
                ArenaStmtTag::TypeDef,
                ArenaStmtData::new(raw_type_def_id(id), 0),
            ),
            ArenaStmtKind::ErrorDef(id) => (
                ArenaStmtTag::ErrorDef,
                ArenaStmtData::new(raw_error_def_id(id), 0),
            ),
            ArenaStmtKind::Let {
                target,
                ty: None,
                initializer: ArenaExprOrRun::Expr(initializer),
            } => (
                ArenaStmtTag::LetExprNoTy,
                ArenaStmtData::new(raw_binding_target_id(target), raw_expr_id(initializer)),
            ),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            } => {
                let data = self.push_stmt_extra(&[
                    raw_binding_target_id(target),
                    optional_raw_type_expr_id(ty),
                    raw_expr_or_run(initializer),
                ]);
                (ArenaStmtTag::Let, data)
            }
            ArenaStmtKind::Var {
                target,
                ty: None,
                initializer: ArenaExprOrRun::Expr(initializer),
            } => (
                ArenaStmtTag::VarExprNoTy,
                ArenaStmtData::new(raw_binding_target_id(target), raw_expr_id(initializer)),
            ),
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                let data = self.push_stmt_extra(&[
                    raw_binding_target_id(target),
                    optional_raw_type_expr_id(ty),
                    raw_expr_or_run(initializer),
                ]);
                (ArenaStmtTag::Var, data)
            }
            ArenaStmtKind::Assign { target, op, value } => (
                assign_stmt_tag(op),
                ArenaStmtData::new(raw_assign_target_id(target), raw_expr_or_run(value)),
            ),
            ArenaStmtKind::ProcDef(id) => (
                ArenaStmtTag::ProcDef,
                ArenaStmtData::new(raw_function_def_id(id), 0),
            ),
            ArenaStmtKind::PureDef(id) => (
                ArenaStmtTag::PureDef,
                ArenaStmtData::new(raw_function_def_id(id), 0),
            ),
            ArenaStmtKind::StreamDef(id) => (
                ArenaStmtTag::StreamDef,
                ArenaStmtData::new(raw_function_def_id(id), 0),
            ),
            ArenaStmtKind::SignalHook(id) => (
                ArenaStmtTag::SignalHook,
                ArenaStmtData::new(raw_signal_hook_id(id), 0),
            ),
            ArenaStmtKind::Return(None) => (ArenaStmtTag::ReturnNone, ArenaStmtData::ZERO),
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(id))) => (
                ArenaStmtTag::ReturnExpr,
                ArenaStmtData::new(raw_expr_id(id), 0),
            ),
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Run(id))) => (
                ArenaStmtTag::ReturnRun,
                ArenaStmtData::new(raw_run_form_id(id), 0),
            ),
            ArenaStmtKind::Yield(ArenaExprOrRun::Expr(id)) => (
                ArenaStmtTag::YieldExpr,
                ArenaStmtData::new(raw_expr_id(id), 0),
            ),
            ArenaStmtKind::Yield(ArenaExprOrRun::Run(id)) => (
                ArenaStmtTag::YieldRun,
                ArenaStmtData::new(raw_run_form_id(id), 0),
            ),
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(id)) => (
                ArenaStmtTag::DeferExpr,
                ArenaStmtData::new(raw_expr_id(id), 0),
            ),
            ArenaStmtKind::Defer(ArenaExprOrRun::Run(id)) => (
                ArenaStmtTag::DeferRun,
                ArenaStmtData::new(raw_run_form_id(id), 0),
            ),
            ArenaStmtKind::If {
                branches,
                else_block: None,
            } => (ArenaStmtTag::IfNoElse, range_stmt_data(branches)),
            ArenaStmtKind::If {
                branches,
                else_block: Some(else_block),
            } => {
                let data =
                    self.push_stmt_extra(&[branches.start, branches.len, raw_block_id(else_block)]);
                (ArenaStmtTag::IfElse, data)
            }
            ArenaStmtKind::While { condition, block } => (
                ArenaStmtTag::While,
                ArenaStmtData::new(raw_expr_id(condition), raw_block_id(block)),
            ),
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                let data = self.push_stmt_extra(&[
                    raw_binding_target_id(target),
                    raw_expr_id(iter),
                    raw_block_id(block),
                ]);
                (ArenaStmtTag::For, data)
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_param,
                else_block,
            } => {
                let data = self.push_stmt_extra(&[
                    bindings.start,
                    bindings.len,
                    raw_block_id(body),
                    optional_raw_name(else_param),
                    raw_block_id(else_block),
                ]);
                (ArenaStmtTag::With, data)
            }
            ArenaStmtKind::Loop { block } => (
                ArenaStmtTag::Loop,
                ArenaStmtData::new(raw_block_id(block), 0),
            ),
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_param,
                else_block,
            } => {
                let data = self.push_stmt_extra(&[
                    raw_binding_target_id(target),
                    optional_raw_type_expr_id(ty),
                    raw_expr_or_run(initializer),
                    optional_raw_name(else_param),
                    raw_block_id(else_block),
                ]);
                (ArenaStmtTag::Guard, data)
            }
            ArenaStmtKind::GuardedStmt {
                stmt,
                negate,
                condition,
            } => {
                let tag = if negate {
                    ArenaStmtTag::GuardedStmtNegated
                } else {
                    ArenaStmtTag::GuardedStmt
                };
                (
                    tag,
                    ArenaStmtData::new(raw_stmt_id(stmt), raw_expr_id(condition)),
                )
            }
            ArenaStmtKind::Break { value: None } => (ArenaStmtTag::BreakNone, ArenaStmtData::ZERO),
            ArenaStmtKind::Break { value: Some(value) } => (
                ArenaStmtTag::BreakValue,
                ArenaStmtData::new(raw_expr_id(value), 0),
            ),
            ArenaStmtKind::Continue => (ArenaStmtTag::Continue, ArenaStmtData::ZERO),
            ArenaStmtKind::Match { value, arms } => {
                let data = self.push_stmt_extra(&[raw_expr_id(value), arms.start, arms.len]);
                (ArenaStmtTag::Match, data)
            }
            ArenaStmtKind::Command(id) => (
                ArenaStmtTag::Command,
                ArenaStmtData::new(raw_command_stmt_id(id), 0),
            ),
            ArenaStmtKind::TailBareIdent(name) => (
                ArenaStmtTag::TailBareIdent,
                ArenaStmtData::new(name.symbol().raw(), 0),
            ),
            ArenaStmtKind::Expr(id) => (ArenaStmtTag::Expr, ArenaStmtData::new(raw_expr_id(id), 0)),
        }
    }

    fn push_stmt_kind(&mut self, kind: ArenaStmtKind, span: Span) -> StmtId {
        let id = StmtId::new(self.arena.stmt_tags.len());
        let span = self.stmt_inline_span(id.index(), span);
        let (tag, data) = self.encode_stmt_kind(kind);
        self.arena.stmt_tags.push(tag);
        self.arena.stmt_data.push(data);
        self.arena.stmt_spans.push(span);
        id
    }

    fn push_block_from_stmt_ids(
        &mut self,
        params: &[BlockParam],
        statements: &[StmtId],
        span: Span,
    ) -> BlockId {
        let params = self.lower_block_param_range(params);
        let statements = self.lower_stmt_id_range(statements);
        self.push_block_parts(params, statements, span)
    }

    fn push_block_parts(
        &mut self,
        params: ArenaRange,
        statements: ArenaRange,
        span: Span,
    ) -> BlockId {
        let id = BlockId::new(self.arena.blocks.len());
        let span = self.span(span);
        self.arena.blocks.push(ArenaBlock {
            params,
            statements,
            span,
        });
        id
    }

    fn retag_stmt_tail_bare_ident(&mut self, id: StmtId, name: Name) {
        let index = id.index();
        self.arena.stmt_tags[index] = ArenaStmtTag::TailBareIdent;
        self.arena.stmt_data[index] = ArenaStmtData::new(name.symbol().raw(), 0);
    }

    fn push_assign_target_kind(&mut self, kind: ArenaAssignTargetKind) -> AssignTargetId {
        let id = AssignTargetId::new(self.arena.assign_targets.len());
        self.arena.assign_targets.push(ArenaAssignTarget { kind });
        id
    }

    fn encode_expr_kind(&mut self, kind: ArenaExprKind) -> (ArenaExprTag, ArenaExprData) {
        match kind {
            ArenaExprKind::Null => (ArenaExprTag::Null, ArenaExprData::ZERO),
            ArenaExprKind::Bool(false) => (ArenaExprTag::BoolFalse, ArenaExprData::ZERO),
            ArenaExprKind::Bool(true) => (ArenaExprTag::BoolTrue, ArenaExprData::ZERO),
            ArenaExprKind::Int(id) => (
                ArenaExprTag::Int,
                ArenaExprData::new(raw_index(id.index()), 0),
            ),
            ArenaExprKind::Float(id) => (
                ArenaExprTag::Float,
                ArenaExprData::new(raw_index(id.index()), 0),
            ),
            ArenaExprKind::Duration(id) => (
                ArenaExprTag::Duration,
                ArenaExprData::new(raw_index(id.index()), 0),
            ),
            ArenaExprKind::Str(id) => (
                ArenaExprTag::Str,
                ArenaExprData::new(raw_index(id.index()), 0),
            ),
            ArenaExprKind::PathStr(id) => (
                ArenaExprTag::PathStr,
                ArenaExprData::new(raw_index(id.index()), 0),
            ),
            ArenaExprKind::GlobStr(id) => (
                ArenaExprTag::GlobStr,
                ArenaExprData::new(raw_index(id.index()), 0),
            ),
            ArenaExprKind::FmtString(range) => (ArenaExprTag::FmtString, range_data(range)),
            ArenaExprKind::PathFmtString(range) => (ArenaExprTag::PathFmtString, range_data(range)),
            ArenaExprKind::Bytes(id) => (
                ArenaExprTag::Bytes,
                ArenaExprData::new(raw_index(id.index()), 0),
            ),
            ArenaExprKind::Ident(name) => (
                ArenaExprTag::Ident,
                ArenaExprData::new(name.symbol().raw(), 0),
            ),
            ArenaExprKind::Item => (ArenaExprTag::Item, ArenaExprData::ZERO),
            ArenaExprKind::LastStatus => (ArenaExprTag::LastStatus, ArenaExprData::ZERO),
            ArenaExprKind::List(range) => (ArenaExprTag::List, range_data(range)),
            ArenaExprKind::ListComp {
                expr,
                target,
                iter,
                condition,
            } => {
                let data = self.push_expr_extra(&[
                    raw_expr_id(expr),
                    raw_binding_target_id(target),
                    raw_expr_id(iter),
                    optional_raw_expr_id(condition),
                ]);
                (ArenaExprTag::ListComp, data)
            }
            ArenaExprKind::MapComp {
                key,
                value,
                target,
                iter,
                condition,
            } => {
                let data = self.push_expr_extra(&[
                    raw_expr_id(key),
                    raw_expr_id(value),
                    raw_binding_target_id(target),
                    raw_expr_id(iter),
                    optional_raw_expr_id(condition),
                ]);
                (ArenaExprTag::MapComp, data)
            }
            ArenaExprKind::Record(range) => (ArenaExprTag::Record, range_data(range)),
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let data =
                    self.push_expr_extra(&[branches.start, branches.len, raw_expr_id(else_value)]);
                (ArenaExprTag::If, data)
            }
            ArenaExprKind::Match { value, arms } => {
                let data = self.push_expr_extra(&[raw_expr_id(value), arms.start, arms.len]);
                (ArenaExprTag::Match, data)
            }
            ArenaExprKind::Unary { op, expr } => {
                let tag = match op {
                    UnaryOp::Not => ArenaExprTag::UnaryNot,
                    UnaryOp::Neg => ArenaExprTag::UnaryNeg,
                };
                (tag, ArenaExprData::new(raw_expr_id(expr), 0))
            }
            ArenaExprKind::Binary { op, left, right } => (
                binary_expr_tag(op),
                ArenaExprData::new(raw_expr_id(left), raw_expr_id(right)),
            ),
            ArenaExprKind::Call { callee, args } => {
                let data = self.push_expr_extra(&[raw_expr_id(callee), args.start, args.len]);
                (ArenaExprTag::Call, data)
            }
            ArenaExprKind::Field { base, name } => (
                ArenaExprTag::Field,
                ArenaExprData::new(raw_expr_id(base), name.symbol().raw()),
            ),
            ArenaExprKind::NullSafeField { base, name } => (
                ArenaExprTag::NullSafeField,
                ArenaExprData::new(raw_expr_id(base), name.symbol().raw()),
            ),
            ArenaExprKind::Index { base, index } => (
                ArenaExprTag::Index,
                ArenaExprData::new(raw_expr_id(base), raw_expr_id(index)),
            ),
            ArenaExprKind::Slice { base, start, end } => {
                let data = self.push_expr_extra(&[
                    raw_expr_id(base),
                    optional_raw_expr_id(start),
                    optional_raw_expr_id(end),
                ]);
                (ArenaExprTag::Slice, data)
            }
            ArenaExprKind::EnvGet { kind, name } => {
                let tag = match kind {
                    EnvGetKind::Str => ArenaExprTag::EnvGetStr,
                    EnvGetKind::Path => ArenaExprTag::EnvGetPath,
                    EnvGetKind::PathList => ArenaExprTag::EnvGetPathList,
                };
                (tag, ArenaExprData::new(name.symbol().raw(), 0))
            }
            ArenaExprKind::EnvPathList => (ArenaExprTag::EnvPathList, ArenaExprData::ZERO),
            ArenaExprKind::Pipeline { input, stages } => {
                let data = self.push_expr_extra(&[raw_expr_id(input), stages.start, stages.len]);
                (ArenaExprTag::Pipeline, data)
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                let data = self.push_expr_extra(&[raw_expr_id(input), stages.start, stages.len]);
                (ArenaExprTag::StructuredPipeline, data)
            }
            ArenaExprKind::Run(id) => (
                ArenaExprTag::Run,
                ArenaExprData::new(raw_run_form_id(id), 0),
            ),
            ArenaExprKind::Spawn(form) => match form.target {
                ArenaSpawnTarget::Run(id) => (
                    ArenaExprTag::SpawnRun,
                    ArenaExprData::new(raw_run_form_id(id), raw_span_id(form.span)),
                ),
                ArenaSpawnTarget::Command(id) => (
                    ArenaExprTag::SpawnCommand,
                    ArenaExprData::new(raw_expr_id(id), raw_span_id(form.span)),
                ),
            },
            ArenaExprKind::Wait(form) => (
                ArenaExprTag::Wait,
                ArenaExprData::new(raw_expr_id(form.target), raw_span_id(form.span)),
            ),
            ArenaExprKind::BuilderCall { call, block } => (
                ArenaExprTag::BuilderCall,
                ArenaExprData::new(raw_expr_id(call), raw_builder_block_id(block)),
            ),
            ArenaExprKind::Try(id) => (ArenaExprTag::Try, ArenaExprData::new(raw_expr_id(id), 0)),
            ArenaExprKind::Require { value, schema } => (
                ArenaExprTag::Require,
                ArenaExprData::new(raw_expr_id(value), raw_type_expr_id(schema)),
            ),
            ArenaExprKind::Loop { block } => (
                ArenaExprTag::Loop,
                ArenaExprData::new(raw_block_id(block), 0),
            ),
            ArenaExprKind::Retry { delays, block } => {
                let data = self.push_expr_extra(&[delays.start, delays.len, raw_block_id(block)]);
                (ArenaExprTag::Retry, data)
            }
        }
    }

    fn push_expr_kind(&mut self, kind: ArenaExprKind, span: Span) -> ExprId {
        let id = ExprId::new(self.arena.expr_tags.len());
        let span = self.expr_inline_span(id.index(), span);
        let (tag, data) = self.encode_expr_kind(kind);
        self.arena.expr_tags.push(tag);
        self.arena.expr_data.push(data);
        self.arena.expr_spans.push(span);
        id
    }

    fn lower_record_field_input(&mut self, field: &ArenaRecordFieldInput) -> ArenaRecordField {
        ArenaRecordField {
            kind: match field {
                ArenaRecordFieldInput::Named { name, value, span } => ArenaRecordFieldKind::Named {
                    name: *name,
                    value: *value,
                    span: self.span(*span),
                },
                ArenaRecordFieldInput::Shorthand { name, span } => {
                    ArenaRecordFieldKind::Shorthand {
                        name: *name,
                        span: self.span(*span),
                    }
                }
                ArenaRecordFieldInput::Spread { expr, span } => ArenaRecordFieldKind::Spread {
                    expr: *expr,
                    span: self.span(*span),
                },
            },
        }
    }

    fn commit_call_arg_input(&mut self, arg: &ArenaCallArgInput) -> ArenaCallArg {
        ArenaCallArg {
            kind: match arg {
                ArenaCallArgInput::Positional(expr) => ArenaCallArgKind::Positional(*expr),
                ArenaCallArgInput::Splice { value, span } => ArenaCallArgKind::Splice {
                    value: *value,
                    span: self.span(*span),
                },
                ArenaCallArgInput::Named { name, value, span } => ArenaCallArgKind::Named {
                    name: *name,
                    value: *value,
                    span: self.span(*span),
                },
            },
        }
    }

    fn push_run_form_parts(
        &mut self,
        segments: ArenaRange,
        propagate: bool,
        span: Span,
    ) -> RunFormId {
        let id = RunFormId::new(self.arena.run_forms.len());
        let span = self.span(span);
        self.arena.run_forms.push(ArenaRunForm {
            segments,
            propagate,
            span,
        });
        id
    }
}
