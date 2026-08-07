#![allow(clippy::single_call_fn, dead_code)]

use std::fmt::Write as _;
use std::sync::Arc;
use xsh::diagnostic::Diagnostic;
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::symbols::{Name, Symbol};
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCommand, ArenaCommandArg,
    ArenaCommandArgKind, ArenaEnvAssignment, ArenaEnvAssignmentValue, ArenaExprKind,
    ArenaExprOrRun, ArenaFmtPart, ArenaModuleContractEntryKind, ArenaPatternKind,
    ArenaPipeStageKind, ArenaProgram, ArenaRecordField, ArenaRecordFieldKind,
    ArenaRedirectionTarget, ArenaSpawnForm, ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage,
    ArenaText, ArenaTypeExprTag, ArenaWordPart, AstArena, BindingTargetId, BlockId, ExprId,
    FunctionDefId, PatternId, StmtId, TypeExprId,
};
use xsh::frontend::syntax::cst::SyntaxTree;
use xsh::frontend::syntax::literal;
use xsh::frontend::syntax::node::{
    AssignOp, BinaryOp, CoreCommand, Effect, EnvGetKind, FormatSpecKind, RedirectionKind, RunKind,
    StreamStageKind, UnaryOp,
};
use xsh::frontend::syntax::parser::{ArenaParseOutput, Parser};

pub const DEFAULT_LINE_WIDTH: usize = 120;
const MULTILINE_LIST_ITEM_THRESHOLD: usize = 8;
const MULTILINE_RECORD_FIELD_THRESHOLD: usize = 6;
const MULTILINE_SCHEMA_FIELD_THRESHOLD: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Formatter {
    line_width: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FormatOutput {
    pub formatted: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingComment {
    span: Span,
    text: String,
}

#[derive(Clone, Debug)]
enum Doc {
    Text(String),
    Line,
    SoftLine,
    Indent(usize, Box<Doc>),
    Dedent(usize, Box<Doc>),
    Group {
        flat: Box<Doc>,
        broken: Box<Doc>,
        prefer_broken: bool,
    },
    Concat(Vec<Doc>),
}

impl Doc {
    fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    fn group(flat: Self, broken: Self, prefer_broken: bool) -> Self {
        Self::Group {
            flat: Box::new(flat),
            broken: Box::new(broken),
            prefer_broken,
        }
    }

    fn render(&self, line_width: usize, column: usize) -> String {
        self.render_with_indent(line_width, column, 0)
    }

    fn render_with_indent(&self, line_width: usize, column: usize, indent: usize) -> String {
        let mut renderer = DocRenderer {
            line_width,
            output: String::new(),
            column,
            indent,
        };
        renderer.render(self, RenderMode::Broken);
        renderer.output
    }

    fn flat_width(&self) -> Option<usize> {
        match self {
            Self::Text(text) => (!text.contains('\n')).then(|| text.chars().count()),
            Self::Line => Some(1),
            Self::SoftLine => Some(1),
            Self::Indent(_, doc) => doc.flat_width(),
            Self::Dedent(_, doc) => doc.flat_width(),
            Self::Concat(parts) => parts.iter().try_fold(0usize, |width, part| {
                part.flat_width().map(|value| width + value)
            }),
            Self::Group { flat, .. } => flat.flat_width(),
        }
    }
}

#[derive(Clone, Copy)]
enum RenderMode {
    Flat,
    Broken,
}

struct DocRenderer {
    line_width: usize,
    output: String,
    column: usize,
    indent: usize,
}

impl DocRenderer {
    fn render(&mut self, doc: &Doc, mode: RenderMode) {
        match doc {
            Doc::Text(text) => {
                self.output.push_str(text);
                if let Some((_, suffix)) = text.rsplit_once('\n') {
                    self.column = suffix.chars().count();
                } else {
                    self.column += text.chars().count();
                }
            }
            Doc::Line => match mode {
                RenderMode::Flat => self.output.push(' '),
                RenderMode::Broken => {
                    self.output.push('\n');
                    self.output.push_str(&"  ".repeat(self.indent));
                    self.column = self.indent * 2;
                }
            },
            Doc::SoftLine => match mode {
                RenderMode::Flat => self.output.push(' '),
                RenderMode::Broken => {
                    self.output.push('\n');
                    self.output.push_str(&"  ".repeat(self.indent));
                    self.column = self.indent * 2;
                }
            },
            Doc::Indent(amount, doc) => {
                self.indent += amount;
                self.render(doc, mode);
                self.indent -= amount;
            }
            Doc::Dedent(amount, doc) => {
                self.indent -= amount;
                self.render(doc, mode);
                self.indent += amount;
            }
            Doc::Concat(parts) => {
                for part in parts {
                    self.render(part, mode);
                }
            }
            Doc::Group {
                flat,
                broken,
                prefer_broken,
            } => {
                let fits = flat
                    .flat_width()
                    .is_some_and(|width| self.column + width <= self.line_width);
                if matches!(mode, RenderMode::Flat) || (!prefer_broken && fits) {
                    self.render(flat, RenderMode::Flat);
                } else {
                    self.render(broken, RenderMode::Broken);
                }
            }
        }
    }
}

struct Writer<'a> {
    arena: &'a AstArena,
    source: Arc<str>,
    comments: Vec<PendingComment>,
    next_comment: usize,
    line_width: usize,
    force_collection_expanded: bool,
    inline_only: bool,
}

#[derive(Clone, Copy, Debug)]
struct CallChainSegment {
    name: Name,
    args: xsh::frontend::syntax::arena::ArenaRange,
}

/// A decoded type expression node, mirroring the arena's compact type-expr
/// encoding without referencing the old recursive AST.
enum ArenaTypeExprKind {
    Named(Name),
    Qualified {
        namespace: Name,
        name: Name,
    },
    List(TypeExprId),
    Map(TypeExprId),
    Stream(TypeExprId),
    Module(TypeExprId),
    Result {
        ok: TypeExprId,
        err: Option<TypeExprId>,
    },
    Optional(TypeExprId),
}

fn type_expr_kind(arena: &AstArena, id: TypeExprId) -> ArenaTypeExprKind {
    let index = id.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Named => {
            ArenaTypeExprKind::Named(Name::from_symbol(Symbol::from_raw(data.lhs)))
        }
        ArenaTypeExprTag::Qualified => ArenaTypeExprKind::Qualified {
            namespace: Name::from_symbol(Symbol::from_raw(data.lhs)),
            name: Name::from_symbol(Symbol::from_raw(data.rhs)),
        },
        ArenaTypeExprTag::List => {
            ArenaTypeExprKind::List(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Map => ArenaTypeExprKind::Map(TypeExprId::from_index(data.lhs as usize)),
        ArenaTypeExprTag::Stream => {
            ArenaTypeExprKind::Stream(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Module => {
            ArenaTypeExprKind::Module(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Result => ArenaTypeExprKind::Result {
            ok: TypeExprId::from_index(data.lhs as usize),
            err: TypeExprId::from_optional_raw(data.rhs),
        },
        ArenaTypeExprTag::Optional => {
            ArenaTypeExprKind::Optional(TypeExprId::from_index(data.lhs as usize))
        }
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self {
            line_width: DEFAULT_LINE_WIDTH,
        }
    }
}

impl Formatter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_line_width(mut self, line_width: usize) -> Self {
        self.line_width = line_width.max(1);
        self
    }

    pub fn format_source(&self, source_id: SourceId, source: &str) -> FormatOutput {
        let parsed = Parser::parse_source_arena_only(source_id, source);
        self.format_parsed_source(source, &parsed)
    }

    pub fn format_parsed_source(&self, source: &str, parsed: &ArenaParseOutput) -> FormatOutput {
        if !parsed.diagnostics.is_empty() {
            return FormatOutput {
                formatted: String::new(),
                diagnostics: parsed.diagnostics.clone(),
            };
        }

        self.format_program_with_cst(source, &parsed.arena, parsed.cst.get())
    }

    pub fn format_program_with_source(
        &self,
        source_id: SourceId,
        source: &str,
        program: &ArenaProgram,
    ) -> FormatOutput {
        let (cst, diagnostics) = SyntaxTree::parse(source_id, source);
        if !diagnostics.is_empty() {
            return FormatOutput {
                formatted: String::new(),
                diagnostics,
            };
        }
        self.format_program_with_cst(source, program, &cst)
    }

    fn format_program_with_cst(
        &self,
        source: &str,
        program: &ArenaProgram,
        cst: &SyntaxTree,
    ) -> FormatOutput {
        let comments = cst
            .comment_trivia()
            .map(|(id, comment)| PendingComment {
                span: comment.span,
                text: cst
                    .trivia_text(id)
                    .strip_prefix('#')
                    .unwrap_or(cst.trivia_text(id))
                    .to_string(),
            })
            .collect();

        FormatOutput {
            formatted: Writer {
                arena: &program.arena,
                source: Arc::from(source),
                comments,
                next_comment: 0,
                line_width: self.line_width,
                force_collection_expanded: false,
                inline_only: false,
            }
            .format_program(program),
            diagnostics: Vec::new(),
        }
    }
}

impl<'a> Writer<'a> {
    fn format_program(&mut self, program: &ArenaProgram) -> String {
        let mut output = String::new();
        let mut previous: Option<ArenaStmtKind> = None;
        let mut previous_end: Option<usize> = None;

        for stmt_id in program.statement_ids() {
            let stmt = self.arena.stmt(stmt_id);
            let pending_comment = self.has_comment_before(stmt.span.start());
            if !output.is_empty() {
                output.push('\n');
                let original_blank =
                    previous_end.is_some_and(|end| self.gap_has_blank_line(end, stmt.span.start()));
                if previous
                    .as_ref()
                    .is_some_and(|prev| needs_top_level_blank(prev, &stmt.kind))
                    || pending_comment
                    || original_blank
                {
                    output.push('\n');
                }
            }
            self.write_stmt(stmt_id, 0, &mut output);
            previous = Some(stmt.kind);
            previous_end = Some(stmt.span.end());
        }

        if self.next_comment < self.comments.len() {
            if !output.is_empty() {
                output.push('\n');
            }
            self.write_remaining_comments(0, &mut output);
        }

        if !output.is_empty() {
            output.push('\n');
        }
        output
    }

    fn gap_has_blank_line(&self, start: usize, end: usize) -> bool {
        self.source
            .get(start..end)
            .is_some_and(|gap| !gap.contains('#') && gap.contains('\n'))
    }

    fn gap_has_blank_line_in_block(&self, previous: Span, current_start: usize) -> bool {
        let Some(gap) = self.source.get(previous.end()..current_start) else {
            return false;
        };
        let previous_is_multiline = self
            .source
            .get(previous.range())
            .is_some_and(|source| source.contains('\n'));
        !gap.contains('#')
            && (gap.matches('\n').count() >= 2 || (previous_is_multiline && gap.contains('\n')))
    }

    fn write_stmt(&mut self, stmt_id: StmtId, indent: usize, output: &mut String) {
        let stmt = self.arena.stmt(stmt_id);
        let skip_formatting = self.write_comments_before(stmt.span.start(), indent, output);
        if skip_formatting {
            self.write_indent(indent, output);
            self.write_raw_stmt(stmt.span, output);
            return;
        }
        self.write_indent(indent, output);
        match &stmt.kind {
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = self.arena.use_stmt(*use_id);
                output.push_str("use ");
                output.push_str(&self.join_name_range(use_stmt.path, "."));
                if let Some(alias) = &use_stmt.alias {
                    output.push_str(" as ");
                    output.push_str(alias.as_str().as_str());
                }
            }
            ArenaStmtKind::Export(inner) => {
                output.push_str("export ");
                self.write_stmt_inline(*inner, indent, output);
            }
            ArenaStmtKind::TypeDef(def) => self.write_type_def(*def, indent, output),
            ArenaStmtKind::ErrorDef(def) => self.write_error_def(*def, output),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            } => {
                output.push_str("let ");
                self.write_binding_target(*target, output);
                self.write_optional_type(*ty, output);
                output.push_str(" = ");
                self.write_expr_or_run_safe(initializer, output);
            }
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                output.push_str("var ");
                self.write_binding_target(*target, output);
                self.write_optional_type(*ty, output);
                output.push_str(" = ");
                self.write_expr_or_run_safe(initializer, output);
            }
            ArenaStmtKind::Assign { target, op, value } => {
                self.write_assign_target(*target, output);
                output.push(' ');
                output.push_str(assign_op_text(*op));
                output.push(' ');
                self.write_expr_or_run(value, output);
            }
            ArenaStmtKind::ProcDef(def) => self.write_function("proc", *def, indent, output),
            ArenaStmtKind::PureDef(def) => self.write_function("pure", *def, indent, output),
            ArenaStmtKind::StreamDef(def) => self.write_function("stream", *def, indent, output),
            ArenaStmtKind::SignalHook(hook_id) => self.write_signal_hook(*hook_id, indent, output),
            ArenaStmtKind::Return(value) => {
                output.push_str("return");
                if let Some(value) = value {
                    output.push(' ');
                    self.write_expr_or_run_safe(value, output);
                }
            }
            ArenaStmtKind::Yield(value) => {
                output.push_str("yield ");
                self.write_expr_or_run_safe(value, output);
            }
            ArenaStmtKind::Defer(value) => {
                output.push_str("defer ");
                self.write_expr_or_run(value, output);
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => self.write_if(*branches, *else_block, indent, output),
            ArenaStmtKind::While { condition, block } => {
                output.push_str("while ");
                self.write_expr(*condition, 0, output);
                output.push(' ');
                self.write_block(*block, indent, output);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                output.push_str("for ");
                self.write_binding_target(*target, output);
                output.push_str(" in ");
                self.write_expr(*iter, 0, output);
                output.push(' ');
                self.write_block(*block, indent, output);
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_param,
                else_block,
            } => {
                output.push_str("with\n");
                let bindings = self.arena.with_bindings(*bindings).to_vec();
                let len = bindings.len();
                for (index, binding) in bindings.iter().enumerate() {
                    self.write_indent(indent + 1, output);
                    output.push_str(binding.name.as_str().as_str());
                    output.push_str(" = ");
                    self.write_expr(binding.initializer, 0, output);
                    if index + 1 < len {
                        output.push(',');
                    }
                    output.push('\n');
                }
                self.write_indent(indent, output);
                self.write_block(*body, indent, output);
                output.push_str(" else ");
                if let Some(param) = else_param {
                    output.push('|');
                    output.push_str(param.as_str().as_str());
                    output.push_str("| ");
                }
                self.write_block(*else_block, indent, output);
            }
            ArenaStmtKind::Loop { block } => {
                output.push_str("loop ");
                self.write_block(*block, indent, output);
            }
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_param,
                else_block,
            } => {
                output.push_str("guard let ");
                self.write_binding_target(*target, output);
                self.write_optional_type(*ty, output);
                output.push_str(" = ");
                self.write_expr_or_run_safe(initializer, output);
                output.push_str(" else ");
                if let Some(param) = else_param {
                    output.push('|');
                    output.push_str(param.as_str().as_str());
                    output.push_str("| ");
                }
                self.write_block(*else_block, indent, output);
            }
            ArenaStmtKind::GuardedStmt {
                stmt: inner,
                negate,
                condition,
            } => {
                self.write_stmt_inline(*inner, indent, output);
                if *negate {
                    output.push_str(" unless ");
                } else {
                    output.push_str(" when ");
                }
                self.write_expr(*condition, 0, output);
            }
            ArenaStmtKind::Break { value } => {
                output.push_str("break");
                if let Some(expr) = value {
                    output.push(' ');
                    self.write_expr(*expr, 0, output);
                }
            }
            ArenaStmtKind::Continue => output.push_str("continue"),
            ArenaStmtKind::Match { value, arms } => self.write_match(*value, *arms, indent, output),
            ArenaStmtKind::Command(command) => self.write_command_stmt(*command, indent, output),
            ArenaStmtKind::TailBareIdent(name) => output.push_str(name.as_str().as_str()),
            ArenaStmtKind::Expr(expr) => self.write_expr(*expr, 0, output),
        }
        self.write_trailing_comment(stmt.span.end(), output);
    }

    fn write_signal_hook(
        &mut self,
        hook_id: xsh::frontend::syntax::arena::SignalHookId,
        indent: usize,
        output: &mut String,
    ) {
        let hook = self.arena.signal_hook(hook_id);
        let signal = hook.signal;
        let pre_cancel = hook.options.pre_cancel.clone();
        let effects: Vec<Effect> = self.arena.effects(hook.effects).collect();
        let body = hook.body;
        output.push_str("on ");
        output.push_str(signal.as_str().as_str());
        if let Some(pre_cancel) = &pre_cancel {
            output.push_str(" --pre-cancel=");
            output.push_str(pre_cancel);
        }
        output.push_str(" [");
        for (i, eff) in canonical_effects(&effects).iter().enumerate() {
            if i > 0 {
                output.push_str(", ");
            }
            output.push_str(eff.as_str());
        }
        output.push_str("] ");
        self.write_block(body, indent, output);
    }

    fn write_type_def(
        &mut self,
        def_id: xsh::frontend::syntax::arena::TypeDefId,
        indent: usize,
        output: &mut String,
    ) {
        use xsh::frontend::syntax::arena::ArenaTypeDefBody;
        let def = self.arena.type_def(def_id).clone();
        output.push_str("type ");
        output.push_str(def.name.as_str().as_str());
        match &def.body {
            ArenaTypeDefBody::Alias(ty) => {
                output.push_str(" = ");
                self.write_type(*ty, output);
            }
            ArenaTypeDefBody::RecordSchema(fields) => {
                output.push_str(" = ");
                self.write_record_schema(*fields, output);
            }
            ArenaTypeDefBody::ModuleContract(entries) => {
                output.push_str(" = ");
                self.write_module_contract(*entries, output);
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                let variants = self.arena.tag_variants(*variants).to_vec();
                let mut parts = Vec::new();
                for v in &variants {
                    let mut part = v.name.as_str().to_string();
                    if !v.fields.is_empty() {
                        part.push('(');
                        let mut field_strs = Vec::new();
                        let field_ids: Vec<TypeExprId> = self
                            .arena
                            .extra_range(v.fields)
                            .iter()
                            .map(|raw| TypeExprId::from_index(*raw as usize))
                            .collect();
                        for f in field_ids {
                            let mut s = String::new();
                            self.write_type(f, &mut s);
                            field_strs.push(s);
                        }
                        part.push_str(&field_strs.join(", "));
                        part.push(')');
                    }
                    parts.push(part);
                }
                let use_multiline = variants.len() >= 5
                    || (variants.len() >= 3
                        && parts.iter().map(|p| p.len() + 3).sum::<usize>() > 60);
                if use_multiline {
                    output.push_str(" =\n");
                    let variant_indent = " ".repeat(indent + 4);
                    output.push_str(&format!("{variant_indent}{}", parts[0]));
                    let cont_indent = " ".repeat(indent);
                    for part in &parts[1..] {
                        output.push('\n');
                        output.push_str(&format!("{cont_indent}  | {part}"));
                    }
                } else {
                    output.push_str(" = ");
                    output.push_str(&parts.join(" | "));
                }
            }
        }
    }

    fn write_error_def(
        &mut self,
        def_id: xsh::frontend::syntax::arena::ErrorDefId,
        output: &mut String,
    ) {
        let def = self.arena.error_def(def_id).clone();
        output.push_str("error ");
        output.push_str(def.name.as_str().as_str());
        output.push_str(" = ");
        let variants = self.arena.error_variants(def.variants).to_vec();
        for (index, variant) in variants.iter().enumerate() {
            if index > 0 {
                output.push_str(" | ");
            }
            output.push_str(variant.name.as_str().as_str());
            if !variant.fields.is_empty() {
                output.push('(');
                let fields = self.arena.error_fields(variant.fields).to_vec();
                for (field_index, field) in fields.iter().enumerate() {
                    if field_index > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(field.name.as_str().as_str());
                    output.push_str(": ");
                    self.write_type(field.ty, output);
                }
                output.push(')');
            }
            if !variant.facets.is_empty() {
                output.push_str(" : ");
                output.push_str(&self.join_name_range(variant.facets, ", "));
            }
        }
    }

    fn write_function(
        &mut self,
        keyword: &str,
        def_id: FunctionDefId,
        indent: usize,
        output: &mut String,
    ) {
        let body = self.arena.function_def(def_id).body;
        let params_empty = self.arena.function_def(def_id).params.is_empty();
        let inline = self.render_inline(|writer, inline| {
            writer.write_function_signature(keyword, def_id, inline);
        });
        if params_empty || self.fits_inline_with_extra(output, &inline, 2) {
            output.push_str(&inline);
        } else {
            self.write_multiline_function_signature(keyword, def_id, indent, output);
        }
        output.push(' ');
        self.write_block(body, indent, output);
    }

    fn write_function_signature(
        &mut self,
        keyword: &str,
        def_id: FunctionDefId,
        output: &mut String,
    ) {
        let def = self.arena.function_def(def_id).clone();
        output.push_str(keyword);
        output.push(' ');
        output.push_str(def.name.as_str().as_str());
        output.push('(');
        let params = self.arena.params(def.params).to_vec();
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            self.write_param(param, output);
        }
        output.push(')');
        if let Some(effects) = def.effects {
            let effects: Vec<Effect> = self.arena.effects(effects).collect();
            output.push_str(" [");
            for (i, eff) in canonical_effects(&effects).iter().enumerate() {
                if i > 0 {
                    output.push_str(", ");
                }
                output.push_str(eff.as_str());
            }
            output.push(']');
        }
        if !def.return_ty_defaulted {
            output.push_str(" -> ");
            self.write_type(def.return_ty, output);
        }
    }

    fn write_multiline_function_signature(
        &mut self,
        keyword: &str,
        def_id: FunctionDefId,
        indent: usize,
        output: &mut String,
    ) {
        let def = self.arena.function_def(def_id).clone();
        output.push_str(keyword);
        output.push(' ');
        output.push_str(def.name.as_str().as_str());
        output.push_str("(\n");
        let params = self.arena.params(def.params).to_vec();
        for param in &params {
            self.write_indent(indent + 1, output);
            self.write_param(param, output);
            output.push_str(",\n");
        }
        self.write_indent(indent, output);
        output.push(')');
        if let Some(effects) = def.effects {
            let effects: Vec<Effect> = self.arena.effects(effects).collect();
            output.push_str(" [");
            for (i, eff) in canonical_effects(&effects).iter().enumerate() {
                if i > 0 {
                    output.push_str(", ");
                }
                output.push_str(eff.as_str());
            }
            output.push(']');
        }
        if !def.return_ty_defaulted {
            output.push_str(" -> ");
            self.write_type(def.return_ty, output);
        }
    }

    fn write_param(
        &mut self,
        param: &xsh::frontend::syntax::arena::ArenaParam,
        output: &mut String,
    ) {
        if param.rest {
            output.push_str("...");
        }
        output.push_str(param.name.as_str().as_str());
        if !param.ty_defaulted {
            output.push_str(": ");
            self.write_type(param.ty, output);
        }
        if let Some(default) = param.default {
            output.push_str(" = ");
            self.write_expr(default, 0, output);
        }
    }

    fn write_params(
        &mut self,
        params: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let params = self.arena.params(params).to_vec();
        output.push('(');
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            self.write_param(param, output);
        }
        output.push(')');
    }

    fn write_effect_list(&mut self, effects: &[Effect], output: &mut String) {
        output.push('[');
        for (index, effect) in canonical_effects(effects).iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push_str(effect.as_str());
        }
        output.push(']');
    }

    fn write_if(
        &mut self,
        branches: xsh::frontend::syntax::arena::ArenaRange,
        else_block: Option<BlockId>,
        indent: usize,
        output: &mut String,
    ) {
        let branches = self.arena.if_branches(branches).to_vec();
        if let Some(first) = branches.first() {
            output.push_str("if ");
            self.write_expr(first.condition, 0, output);
            output.push(' ');
            self.write_block(first.block, indent, output);
            for branch in &branches[1..] {
                output.push_str(" else if ");
                self.write_expr(branch.condition, 0, output);
                output.push(' ');
                self.write_block(branch.block, indent, output);
            }
        }
        if let Some(block) = else_block {
            output.push_str(" else ");
            self.write_block(block, indent, output);
        }
    }

    fn write_match(
        &mut self,
        value: ExprId,
        arms: xsh::frontend::syntax::arena::ArenaRange,
        indent: usize,
        output: &mut String,
    ) {
        output.push_str("match ");
        self.write_expr(value, 0, output);
        output.push_str(" {");
        let arms = self.arena.match_arms(arms).to_vec();
        if arms.is_empty() {
            output.push('}');
            return;
        }
        output.push('\n');
        for (index, arm) in arms.iter().enumerate() {
            if index > 0 {
                output.push('\n');
            }
            self.write_indent(indent + 1, output);
            self.write_pattern(arm.pattern, output);
            if let Some(guard) = arm.guard {
                output.push_str(" if ");
                self.write_expr(guard, 0, output);
            }
            output.push_str(" => ");
            let block = self.arena.block(arm.block);
            let stmts: Vec<StmtId> = self.arena.stmt_ids(block.statements).collect();
            if stmts.len() == 1 && block.params.is_empty() {
                let stmt_id = stmts[0];
                let stmt_kind = self.arena.stmt(stmt_id).kind;
                if !matches!(
                    stmt_kind,
                    ArenaStmtKind::If { .. }
                        | ArenaStmtKind::While { .. }
                        | ArenaStmtKind::For { .. }
                        | ArenaStmtKind::Match { .. }
                        | ArenaStmtKind::With { .. }
                        | ArenaStmtKind::ProcDef(_)
                        | ArenaStmtKind::PureDef(_)
                        | ArenaStmtKind::StreamDef(_)
                ) {
                    self.write_stmt_inline(stmt_id, indent + 1, output);
                    continue;
                }
            }
            self.write_block(arm.block, indent + 1, output);
        }
        output.push('\n');
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_stmt_inline(&mut self, stmt_id: StmtId, indent: usize, output: &mut String) {
        let kind = self.arena.stmt(stmt_id).kind;
        match &kind {
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = self.arena.use_stmt(*use_id);
                output.push_str("use ");
                output.push_str(&self.join_name_range(use_stmt.path, "."));
                if let Some(alias) = &use_stmt.alias {
                    output.push_str(" as ");
                    output.push_str(alias.as_str().as_str());
                }
            }
            ArenaStmtKind::Export(inner) => {
                output.push_str("export ");
                self.write_stmt_inline(*inner, indent, output);
            }
            ArenaStmtKind::TypeDef(def) => self.write_type_def(*def, indent, output),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            } => {
                output.push_str("let ");
                self.write_binding_target(*target, output);
                self.write_optional_type(*ty, output);
                output.push_str(" = ");
                self.write_expr_or_run_safe(initializer, output);
            }
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                output.push_str("var ");
                self.write_binding_target(*target, output);
                self.write_optional_type(*ty, output);
                output.push_str(" = ");
                self.write_expr_or_run_safe(initializer, output);
            }
            ArenaStmtKind::Assign { target, op, value } => {
                self.write_assign_target(*target, output);
                output.push(' ');
                output.push_str(assign_op_text(*op));
                output.push(' ');
                self.write_expr_or_run(value, output);
            }
            ArenaStmtKind::Return(value) => {
                output.push_str("return");
                if let Some(value) = value {
                    output.push(' ');
                    self.write_expr_or_run_safe(value, output);
                }
            }
            ArenaStmtKind::Yield(value) => {
                output.push_str("yield ");
                self.write_expr_or_run_safe(value, output);
            }
            ArenaStmtKind::Defer(value) => {
                output.push_str("defer ");
                self.write_expr_or_run(value, output);
            }
            ArenaStmtKind::Break { value } => {
                output.push_str("break");
                if let Some(expr) = value {
                    output.push(' ');
                    self.write_expr(*expr, 0, output);
                }
            }
            ArenaStmtKind::Continue => output.push_str("continue"),
            ArenaStmtKind::Command(command) => self.write_command_stmt(*command, indent, output),
            ArenaStmtKind::TailBareIdent(name) => output.push_str(name.as_str().as_str()),
            ArenaStmtKind::Expr(expr) => self.write_expr(*expr, 0, output),
            _ => self.write_stmt(stmt_id, indent, output),
        }
    }

    fn write_pattern(&mut self, pattern_id: PatternId, output: &mut String) {
        let kind = self.arena.pattern(pattern_id).kind.clone();
        match &kind {
            ArenaPatternKind::Wildcard => output.push('_'),
            ArenaPatternKind::Binding(name) => output.push_str(name.as_str().as_str()),
            ArenaPatternKind::Type { binding, ty } => {
                if let Some(binding) = binding {
                    output.push_str(binding.as_str().as_str());
                } else {
                    output.push('_');
                }
                output.push_str(" is ");
                self.write_type(*ty, output);
            }
            ArenaPatternKind::Literal(expr) => self.write_expr(*expr, 0, output),
            ArenaPatternKind::Record { fields, rest } => {
                output.push('{');
                let fields = self.arena.pattern_fields(*fields).to_vec();
                let len = fields.len();
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(field.name.as_str().as_str());
                    output.push_str(": ");
                    self.write_pattern(field.pattern, output);
                }
                if *rest {
                    if len != 0 {
                        output.push_str(", ");
                    }
                    output.push_str("..");
                }
                output.push('}');
            }
            ArenaPatternKind::Alternation(patterns) => {
                let patterns: Vec<PatternId> = self.arena.pattern_ids(*patterns).collect();
                for (index, pattern) in patterns.iter().enumerate() {
                    if index > 0 {
                        output.push_str(" | ");
                    }
                    self.write_pattern(*pattern, output);
                }
            }
            ArenaPatternKind::Constructor { name, arg } => {
                output.push_str(name.as_str().as_str());
                output.push('(');
                if let Some(arg) = arg {
                    self.write_pattern(*arg, output);
                }
                output.push(')');
            }
            ArenaPatternKind::ErrorVariant {
                family,
                variant,
                fields,
            } => {
                output.push_str(family.as_str().as_str());
                output.push('.');
                output.push_str(variant.as_str().as_str());
                output.push_str(" {");
                let fields = self.arena.pattern_fields(*fields).to_vec();
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(field.name.as_str().as_str());
                    output.push_str(": ");
                    self.write_pattern(field.pattern, output);
                }
                output.push('}');
            }
            ArenaPatternKind::Facet(name) => {
                output.push_str("is ");
                output.push_str(name.as_str().as_str());
            }
            ArenaPatternKind::Tuple(patterns) => {
                let patterns: Vec<PatternId> = self.arena.pattern_ids(*patterns).collect();
                for (index, pattern) in patterns.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    self.write_pattern(*pattern, output);
                }
            }
        }
    }

    fn write_binding_target(&mut self, target_id: BindingTargetId, output: &mut String) {
        let kind = self.arena.binding_target(target_id).kind.clone();
        match &kind {
            ArenaBindingTargetKind::Name(name) => output.push_str(name.as_str().as_str()),
            ArenaBindingTargetKind::Record { fields, rest } => {
                output.push('{');
                let fields = self.arena.destructure_fields(*fields).to_vec();
                let len = fields.len();
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(field.name.as_str().as_str());
                }
                if *rest {
                    if len != 0 {
                        output.push_str(", ");
                    }
                    output.push_str("..");
                }
                output.push('}');
            }
        }
    }

    fn write_assign_target(
        &mut self,
        target_id: xsh::frontend::syntax::arena::AssignTargetId,
        output: &mut String,
    ) {
        use xsh::frontend::syntax::arena::ArenaAssignTargetKind;
        let kind = self.arena.assign_target(target_id).kind.clone();
        match &kind {
            ArenaAssignTargetKind::Name(name) => output.push_str(name.as_str().as_str()),
            ArenaAssignTargetKind::Field { base, name } => {
                self.write_assign_target(*base, output);
                output.push('.');
                output.push_str(name.as_str().as_str());
            }
            ArenaAssignTargetKind::Index { base, index } => {
                self.write_assign_target(*base, output);
                output.push('[');
                self.write_expr(*index, 0, output);
                output.push(']');
            }
        }
    }

    fn write_block(&mut self, block_id: BlockId, indent: usize, output: &mut String) {
        let block = self.arena.block(block_id);
        let params = self.arena.block_params(block.params).to_vec();
        let stmts: Vec<StmtId> = self.arena.stmt_ids(block.statements).collect();
        output.push('{');
        if !params.is_empty() {
            output.push(' ');
            output.push('|');
            for (index, param) in params.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str(param.name.as_str().as_str());
            }
            output.push('|');
        }
        if stmts.is_empty() {
            if !params.is_empty() {
                output.push(' ');
            }
            output.push('}');
            return;
        }
        output.push('\n');
        let mut previous_span: Option<Span> = None;
        for (index, stmt_id) in stmts.iter().enumerate() {
            let stmt = self.arena.stmt(*stmt_id);
            let stmt_span = stmt.span;
            let pending_comment = self.has_comment_before(stmt_span.start());
            if index > 0 {
                output.push('\n');
                let original_blank = previous_span.is_some_and(|previous| {
                    self.gap_has_blank_line_in_block(previous, stmt_span.start())
                });
                if pending_comment || original_blank {
                    output.push('\n');
                }
            }
            self.write_stmt(*stmt_id, indent + 1, output);
            previous_span = Some(stmt_span);
        }
        output.push('\n');
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_command_stmt(
        &mut self,
        stmt_id: xsh::frontend::syntax::arena::CommandStmtId,
        indent: usize,
        output: &mut String,
    ) {
        let stmt = self.arena.command_stmt(stmt_id).clone();
        match &stmt.command {
            ArenaCommand::Proc { name, args } => {
                output.push_str(name.as_str().as_str());
                self.write_command_args(*args, output);
            }
            ArenaCommand::Core {
                name,
                args,
                env,
                block,
            } => {
                output.push_str(name.as_str());
                let env_assignments = self.arena.env_assignments(*env).to_vec();
                if *name == CoreCommand::Env && env_assignments_are_exprs(&env_assignments) {
                    output.push(' ');
                    self.write_env_expr_assignments(&env_assignments, indent, output);
                    if let Some(block) = block {
                        output.push(' ');
                        self.write_block(*block, indent, output);
                    }
                } else {
                    self.write_command_args(*args, output);
                    for assignment in &env_assignments {
                        output.push(' ');
                        self.write_env_assignment(assignment, output);
                    }
                    if let Some(block) = block {
                        output.push(' ');
                        self.write_block(*block, indent, output);
                    }
                }
            }
            ArenaCommand::Run(run) => self.write_run(*run, output),
        }
        if stmt.propagate {
            output.push_str(" ?");
        }
    }

    fn write_expr_or_run(&mut self, value: &ArenaExprOrRun, output: &mut String) {
        match value {
            ArenaExprOrRun::Expr(expr) => self.write_expr(*expr, 0, output),
            ArenaExprOrRun::Run(run) => self.write_run(*run, output),
        }
    }

    fn write_expr_or_run_safe(&mut self, value: &ArenaExprOrRun, output: &mut String) {
        match value {
            ArenaExprOrRun::Expr(expr) => self.write_expr_safe(*expr, output),
            ArenaExprOrRun::Run(run) => self.write_run(*run, output),
        }
    }

    fn write_field_base_expr(&mut self, expr: ExprId, parent_precedence: u8, output: &mut String) {
        if let ArenaExprKind::PathStr(value) = self.arena.expr(expr).kind {
            output.push('p');
            write_quoted(self.arena.string_literal(value), output);
        } else {
            self.write_expr(expr, parent_precedence, output);
        }
    }

    fn write_run(&mut self, run_id: xsh::frontend::syntax::arena::RunFormId, output: &mut String) {
        let run = self.arena.run_form(run_id).clone();
        let indent = indent_for_expr(output);
        let segments: Vec<xsh::frontend::syntax::arena::ArenaRunSegment> =
            self.arena.run_segments(run.segments).to_vec();
        for (index, segment) in segments.iter().enumerate() {
            if index > 0 {
                output.push_str(" | ");
            }
            self.write_run_segment(segment, indent, output);
        }
        if run.propagate {
            output.push_str(" ?");
        }
    }

    fn write_run_segment(
        &mut self,
        segment: &xsh::frontend::syntax::arena::ArenaRunSegment,
        indent: usize,
        output: &mut String,
    ) {
        output.push_str(run_head_text(segment.kind, segment.builtin));
        output.push(' ');
        if let Some(timeout) = segment.timeout {
            output.push_str("--timeout=");
            self.write_expr(timeout, 0, output);
            output.push(' ');
        }
        if let Some(cpu_max) = segment.cpu_max {
            output.push_str("--cpumax=");
            self.write_expr(cpu_max, 0, output);
            output.push(' ');
        }
        let env = self.arena.env_assignments(segment.env).to_vec();
        for assignment in &env {
            self.write_env_assignment(assignment, output);
            output.push(' ');
        }
        let args: Vec<ArenaCommandArg> = self.arena.command_args(segment.args).to_vec();
        let redirections = self.arena.redirections(segment.redirections).to_vec();
        if segment.grouped {
            output.push_str("(\n");
            self.write_indent(indent + 1, output);
            self.write_command_arg(&segment.target, output);
            for arg in &args {
                output.push('\n');
                self.write_indent(indent + 1, output);
                self.write_command_arg(arg, output);
            }
            for redirection in &redirections {
                output.push('\n');
                self.write_indent(indent + 1, output);
                self.write_redirection(redirection, output);
            }
            output.push('\n');
            self.write_indent(indent, output);
            output.push(')');
            return;
        }
        self.write_command_arg(&segment.target, output);
        for arg in &args {
            output.push(' ');
            self.write_command_arg(arg, output);
        }
        for redirection in &redirections {
            output.push(' ');
            self.write_redirection(redirection, output);
        }
    }

    fn write_env_assignment(&mut self, assignment: &ArenaEnvAssignment, output: &mut String) {
        output.push_str(assignment.name.as_str().as_str());
        output.push('=');
        match &assignment.value {
            ArenaEnvAssignmentValue::CommandArg(arg) => self.write_command_arg(arg, output),
            ArenaEnvAssignmentValue::Expr(expr) => self.write_expr(*expr, 0, output),
        }
    }

    fn write_env_expr_assignments(
        &mut self,
        assignments: &[ArenaEnvAssignment],
        indent: usize,
        output: &mut String,
    ) {
        if assignments.is_empty() {
            output.push_str("{}");
            return;
        }
        output.push_str("{\n");
        for assignment in assignments {
            self.write_indent(indent + 1, output);
            output.push_str(assignment.name.as_str().as_str());
            output.push_str(" = ");
            match &assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => self.write_command_arg(arg, output),
                ArenaEnvAssignmentValue::Expr(expr) => self.write_expr(*expr, 0, output),
            }
            output.push('\n');
        }
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_redirection(
        &mut self,
        redirection: &xsh::frontend::syntax::arena::ArenaRedirection,
        output: &mut String,
    ) {
        output.push_str(match redirection.kind {
            RedirectionKind::StdoutWrite => ">",
            RedirectionKind::StdoutAppend => ">>",
            RedirectionKind::StdinRead => "<",
            RedirectionKind::StderrWrite => "2>",
            RedirectionKind::StderrAppend => "2>>",
            RedirectionKind::StdoutDup => ">&",
            RedirectionKind::StdinDup => "<&",
        });
        output.push(' ');
        match &redirection.target {
            ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => {
                self.write_command_arg(arg, output);
            }
        }
    }

    fn write_command_args(
        &mut self,
        args: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let args: Vec<ArenaCommandArg> = self.arena.command_args(args).to_vec();
        for arg in &args {
            output.push(' ');
            self.write_command_arg(arg, output);
        }
    }

    fn write_command_arg(&mut self, arg: &ArenaCommandArg, output: &mut String) {
        match &arg.kind {
            ArenaCommandArgKind::Word(parts) => {
                let parts: Vec<ArenaWordPart> = self.arena.word_parts(*parts).collect();
                for part in &parts {
                    match part {
                        ArenaWordPart::Bare(text) => {
                            output.push_str(self.text_value(text));
                        }
                        ArenaWordPart::Quoted(text) => {
                            let value = self.text_value(text).to_string();
                            write_command_quoted(&value, output);
                        }
                        ArenaWordPart::Shorthand(expr) => {
                            output.push('$');
                            self.write_expr(*expr, 0, output);
                        }
                        ArenaWordPart::Interpolation(expr) => {
                            output.push_str("${");
                            self.write_expr(*expr, 0, output);
                            output.push('}');
                        }
                    }
                }
            }
            ArenaCommandArgKind::SpliceName(name) => {
                output.push('@');
                output.push_str(name.as_str().as_str());
            }
            ArenaCommandArgKind::SpliceExpr(expr)
                if matches!(self.arena.expr(*expr).kind, ArenaExprKind::GlobStr(_)) =>
            {
                output.push('@');
                self.write_expr(*expr, 0, output);
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                output.push_str("@(");
                self.write_expr(*expr, 0, output);
                output.push(')');
            }
            ArenaCommandArgKind::Typed(expr) => {
                let kind = self.arena.expr(*expr).kind;
                if matches!(kind, ArenaExprKind::PathStr(_) | ArenaExprKind::GlobStr(_))
                    || self.command_typed_arg_can_be_bare(*expr)
                {
                    self.write_expr(*expr, 0, output);
                } else {
                    output.push('(');
                    self.write_expr(*expr, 0, output);
                    output.push(')');
                }
            }
        }
    }

    fn write_expr(&mut self, expr_id: ExprId, parent_precedence: u8, output: &mut String) {
        let kind = self.arena.expr(expr_id).kind;
        let precedence = expr_precedence(&kind);
        let parens = precedence < parent_precedence;
        if parens {
            output.push('(');
        }
        if self.write_expr_with_internal_comment(expr_id, output) {
            if parens {
                output.push(')');
            }
            return;
        }
        if !parens && self.try_write_stable_call_chain(expr_id, output) {
            return;
        }
        match &kind {
            ArenaExprKind::Null => output.push_str("null"),
            ArenaExprKind::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            ArenaExprKind::Int(value) => self.arena.int_literal(*value).write(output),
            ArenaExprKind::Float(value) => self.arena.float_literal(*value).write(output),
            ArenaExprKind::Duration(value) => self.arena.duration_literal(*value).write(output),
            ArenaExprKind::Str(value) => {
                let span = self.arena.expr(expr_id).span;
                if let Some(original) = original_preserved_string_literal(&self.source, span) {
                    output.push_str(original);
                } else {
                    write_str_literal(self.arena.string_literal(*value), output);
                }
            }
            ArenaExprKind::PathStr(value) => {
                let value = self.arena.string_literal(*value);
                if let Some(path) = bare_path_literal_text(value) {
                    output.push_str(path);
                } else {
                    output.push('p');
                    write_quoted(value, output);
                }
            }
            ArenaExprKind::GlobStr(value) => {
                output.push('g');
                write_quoted(self.arena.string_literal(*value), output);
            }
            ArenaExprKind::FmtString(parts) => {
                let span = self.arena.expr(expr_id).span;
                if let Some(original) = original_multiline_string_literal(&self.source, span) {
                    output.push_str(original);
                } else {
                    self.write_fmt_string(*parts, output);
                }
            }
            ArenaExprKind::PathFmtString(parts) => {
                let span = self.arena.expr(expr_id).span;
                if let Some(original) = original_multiline_string_literal(&self.source, span) {
                    output.push_str(original);
                } else {
                    self.write_path_fmt_string(*parts, output);
                }
            }
            ArenaExprKind::Bytes(value) => write_bytes(self.arena.bytes_literal(*value), output),
            ArenaExprKind::Ident(name) => output.push_str(name.as_str().as_str()),
            ArenaExprKind::Item => output.push('.'),
            ArenaExprKind::LastStatus => output.push_str("$?"),
            ArenaExprKind::List(items) => self.write_list(expr_id, *items, output),
            ArenaExprKind::ListComp {
                expr,
                target,
                iter,
                condition,
            } => {
                output.push('[');
                self.write_expr(*expr, 0, output);
                output.push_str(" for ");
                self.write_binding_target(*target, output);
                output.push_str(" in ");
                self.write_expr(*iter, 0, output);
                if let Some(cond) = condition {
                    output.push_str(" if ");
                    self.write_expr(*cond, 0, output);
                }
                output.push(']');
            }
            ArenaExprKind::MapComp {
                key,
                value,
                target,
                iter,
                condition,
            } => {
                output.push('{');
                self.write_expr(*key, 0, output);
                output.push_str(": ");
                self.write_expr(*value, 0, output);
                output.push_str(" for ");
                self.write_binding_target(*target, output);
                output.push_str(" in ");
                self.write_expr(*iter, 0, output);
                if let Some(cond) = condition {
                    output.push_str(" if ");
                    self.write_expr(*cond, 0, output);
                }
                output.push('}');
            }
            ArenaExprKind::Record(fields) => self.write_record(*fields, output),
            ArenaExprKind::If {
                branches,
                else_value,
            } => self.write_if_expr(*branches, *else_value, output),
            ArenaExprKind::Match { value, arms } => self.write_match_expr(*value, *arms, output),
            ArenaExprKind::Unary { op, expr } => {
                output.push_str(match op {
                    UnaryOp::Not => "! ",
                    UnaryOp::Neg => "-",
                });
                self.write_expr(*expr, precedence, output);
            }
            ArenaExprKind::Binary { op, left, right } => {
                self.write_expr(*left, precedence, output);
                output.push(' ');
                output.push_str(binary_op_text(*op));
                output.push(' ');
                let right_precedence = if *op == BinaryOp::ResultFallback {
                    precedence
                } else {
                    precedence + 1
                };
                self.write_expr(*right, right_precedence, output);
            }
            ArenaExprKind::Call { callee, args } => {
                self.write_expr(*callee, precedence, output);
                let callee_end = self.arena.expr(*callee).span.end();
                let call_end = self.arena.expr(expr_id).span.end();
                let original_multiline =
                    self.call_args_original_multiline(*args, callee_end, call_end);
                self.write_call_args(*args, original_multiline, output);
            }
            ArenaExprKind::Field { base, name } => {
                if matches!(self.arena.expr(*base).kind, ArenaExprKind::Item) {
                    output.push('.');
                } else {
                    self.write_field_base_expr(*base, precedence, output);
                    output.push('.');
                }
                output.push_str(name.as_str().as_str());
            }
            ArenaExprKind::NullSafeField { base, name } => {
                self.write_field_base_expr(*base, precedence, output);
                output.push_str("?.");
                output.push_str(name.as_str().as_str());
            }
            ArenaExprKind::Index { base, index } => {
                self.write_expr(*base, precedence, output);
                output.push('[');
                self.write_expr(*index, 0, output);
                output.push(']');
            }
            ArenaExprKind::Slice { base, start, end } => {
                self.write_expr(*base, precedence, output);
                output.push('[');
                if let Some(start) = start {
                    self.write_expr(*start, 0, output);
                }
                output.push_str("..");
                if let Some(end) = end {
                    self.write_expr(*end, 0, output);
                }
                output.push(']');
            }
            ArenaExprKind::EnvGet { kind, name } => {
                output.push_str("env.");
                output.push_str(match kind {
                    EnvGetKind::Str => "Str",
                    EnvGetKind::Path => "Path",
                    EnvGetKind::PathList => "PathList",
                });
                output.push('.');
                output.push_str(name.as_str().as_str());
            }
            ArenaExprKind::EnvPathList => output.push_str("env.PATH"),
            ArenaExprKind::Pipeline { input, stages } => {
                self.write_expr(*input, precedence, output);
                let indent = continuation_indent_for_expr(output);
                self.write_pipe_stages(*stages, indent, output);
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.write_expr(*input, precedence, output);
                let indent = continuation_indent_for_expr(output);
                self.write_stream_stages(*stages, indent, output);
            }
            ArenaExprKind::Run(run) => self.write_run(*run, output),
            ArenaExprKind::Spawn(form) => {
                output.push_str("spawn ");
                match &form.target {
                    ArenaSpawnTarget::Run(run) => self.write_run(*run, output),
                    ArenaSpawnTarget::Command(expr) => self.write_expr(*expr, 8, output),
                }
            }
            ArenaExprKind::Wait(form) => {
                output.push_str("wait ");
                self.write_expr(form.target, 8, output);
            }
            ArenaExprKind::BuilderCall { call, block } => {
                self.write_expr(*call, precedence, output);
                output.push(' ');
                let indent = indent_for_expr(output);
                self.write_builder_block(*block, indent, output);
            }
            ArenaExprKind::Try(inner) => {
                self.write_expr(*inner, precedence, output);
                let inner_kind = self.arena.expr(*inner).kind;
                if matches!(
                    inner_kind,
                    ArenaExprKind::Spawn(ArenaSpawnForm {
                        target: ArenaSpawnTarget::Run(_),
                        ..
                    })
                ) {
                    output.push_str(" ?");
                } else {
                    output.push('?');
                }
            }
            ArenaExprKind::Require { value, schema } => {
                self.write_expr(*value, precedence, output);
                output.push_str(".require(");
                self.write_type(*schema, output);
                output.push(')');
            }
            ArenaExprKind::Loop { block } => {
                output.push_str("loop ");
                self.write_block(*block, 0, output);
            }
            ArenaExprKind::Retry { delays, block } => {
                output.push_str("retry ");
                self.write_list_inline(*delays, output);
                output.push(' ');
                let indent = indent_for_expr(output);
                self.write_block(*block, indent, output);
            }
        }
        if parens {
            output.push(')');
        }
    }

    fn write_expr_with_internal_comment(&mut self, expr_id: ExprId, output: &mut String) -> bool {
        let span = self.arena.expr(expr_id).span;
        let has_comment = self.comments[self.next_comment..].iter().any(|comment| {
            comment.span.start() >= span.start() && comment.span.start() < span.end()
        });
        if !has_comment {
            return false;
        }
        if let Some(raw) = self.source.get(span.range()) {
            output.push_str(raw);
        }
        while self
            .comments
            .get(self.next_comment)
            .is_some_and(|comment| comment.span.start() < span.end())
        {
            self.next_comment += 1;
        }
        true
    }

    fn write_pipe_stage(
        &mut self,
        stage: &xsh::frontend::syntax::arena::ArenaPipeStage,
        indent: usize,
        output: &mut String,
    ) {
        match &stage.kind {
            ArenaPipeStageKind::Expr(expr) => self.write_expr(*expr, 0, output),
            ArenaPipeStageKind::Stream(stage) => self.write_stream_stage(stage, indent, output),
        }
    }

    fn write_pipe_stages(
        &mut self,
        stages: xsh::frontend::syntax::arena::ArenaRange,
        indent: usize,
        output: &mut String,
    ) {
        let stages = self.arena.pipe_stages(stages).to_vec();
        if let [stage] = stages.as_slice() {
            let inline = self.render_inline(|writer, inline| {
                inline.push_str(" |> ");
                writer.write_pipe_stage(stage, indent, inline);
            });
            if self.fits_inline(output, &inline) {
                output.push_str(&inline);
                return;
            }
        }
        for stage in &stages {
            output.push('\n');
            self.write_indent(indent, output);
            output.push_str("|> ");
            self.write_pipe_stage(stage, indent, output);
        }
    }

    fn write_list(
        &mut self,
        expr_id: ExprId,
        items: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let item_count = items.len();
        let inline = self.render_inline(|writer, inline| writer.write_list_inline(items, inline));
        let original_multiline = self.expr_source_is_multiline(expr_id);
        if self.inline_only
            || item_count == 0
            || (item_count < MULTILINE_LIST_ITEM_THRESHOLD && self.fits_inline(output, &inline))
                && !original_multiline
                && !self.force_collection_expanded
        {
            output.push_str(&inline);
            return;
        }

        let indent = indent_for_expr(output);
        output.push_str("[\n");
        for index in 0..item_count {
            let item = ExprId::from_index(self.arena.extra_range(items)[index] as usize);
            self.write_indent(indent + 1, output);
            let previous_force = self.force_collection_expanded;
            self.force_collection_expanded = true;
            self.write_expr_safe(item, output);
            self.force_collection_expanded = previous_force;
            output.push_str(",\n");
        }
        self.write_indent(indent, output);
        output.push(']');
    }

    fn write_list_inline(
        &mut self,
        items: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        output.push('[');
        for index in 0..items.len() {
            if index > 0 {
                output.push_str(", ");
            }
            let item = ExprId::from_index(self.arena.extra_range(items)[index] as usize);
            self.write_expr_safe(item, output);
        }
        output.push(']');
    }

    fn write_record(
        &mut self,
        fields: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let field_count = fields.len();
        let original_multiline = record_fields_original_multiline(
            self.arena,
            &self.source,
            self.arena.record_fields(fields),
        );
        let inline =
            self.render_inline(|writer, inline| writer.write_record_inline(fields, inline));
        if field_count == 0
            || (!original_multiline
                && field_count < MULTILINE_RECORD_FIELD_THRESHOLD
                && self.fits_inline(output, &inline)
                && !self.force_collection_expanded)
        {
            output.push_str(&inline);
            return;
        }

        let indent = indent_for_expr(output);
        output.push_str("{\n");
        for index in 0..field_count {
            let field = self.arena.record_fields(fields)[index].kind.clone();
            self.write_indent(indent + 1, output);
            let previous_force = self.force_collection_expanded;
            self.force_collection_expanded = true;
            self.write_record_field(&field, output);
            self.force_collection_expanded = previous_force;
            output.push_str(",\n");
        }
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_record_inline(
        &mut self,
        fields: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        output.push('{');
        for index in 0..fields.len() {
            if index > 0 {
                output.push_str(", ");
            }
            let field = self.arena.record_fields(fields)[index].kind.clone();
            self.write_record_field(&field, output);
        }
        output.push('}');
    }

    fn write_record_field(&mut self, field: &ArenaRecordFieldKind, output: &mut String) {
        match field {
            ArenaRecordFieldKind::Named { name, value, .. } => {
                output.push_str(name.as_str().as_str());
                output.push_str(": ");
                self.write_expr_safe(*value, output);
            }
            ArenaRecordFieldKind::Shorthand { name, .. } => output.push_str(name.as_str().as_str()),
            ArenaRecordFieldKind::Spread { expr, .. } => {
                output.push_str("...");
                self.write_expr_safe(*expr, output);
            }
        }
    }

    fn write_if_expr(
        &mut self,
        branches: xsh::frontend::syntax::arena::ArenaRange,
        else_value: ExprId,
        output: &mut String,
    ) {
        if let Some(first) = self.arena.if_expr_branches(branches).first().cloned() {
            output.push_str("if ");
            self.write_expr(first.condition, 0, output);
            output.push_str(" { ");
            self.write_expr(first.value, 0, output);
            output.push_str(" }");
            for index in 1..branches.len() {
                let branch = self.arena.if_expr_branches(branches)[index].clone();
                output.push_str(" else if ");
                self.write_expr(branch.condition, 0, output);
                output.push_str(" { ");
                self.write_expr(branch.value, 0, output);
                output.push_str(" }");
            }
        }
        output.push_str(" else { ");
        self.write_expr(else_value, 0, output);
        output.push_str(" }");
    }

    fn write_if_expr_multiline(
        &mut self,
        branches: xsh::frontend::syntax::arena::ArenaRange,
        else_value: ExprId,
        output: &mut String,
    ) {
        let indent = indent_for_expr(output);
        if let Some(first) = self.arena.if_expr_branches(branches).first().cloned() {
            output.push_str("if ");
            self.write_expr(first.condition, 0, output);
            output.push_str(" {\n");
            self.write_indent(indent + 1, output);
            self.write_expr_safe(first.value, output);
            output.push('\n');
            self.write_indent(indent, output);
            output.push('}');
            for index in 1..branches.len() {
                let branch = self.arena.if_expr_branches(branches)[index].clone();
                output.push_str(" else if ");
                self.write_expr(branch.condition, 0, output);
                output.push_str(" {\n");
                self.write_indent(indent + 1, output);
                self.write_expr_safe(branch.value, output);
                output.push('\n');
                self.write_indent(indent, output);
                output.push('}');
            }
        }
        output.push_str(" else {\n");
        self.write_indent(indent + 1, output);
        self.write_expr_safe(else_value, output);
        output.push('\n');
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_match_expr(
        &mut self,
        value: ExprId,
        arms: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        output.push_str("match ");
        self.write_expr(value, 0, output);
        output.push_str(" {");
        for index in 0..arms.len() {
            let arm = self.arena.match_expr_arms(arms)[index].clone();
            if index > 0 {
                output.push_str(", ");
            } else {
                output.push(' ');
            }
            self.write_pattern(arm.pattern, output);
            if let Some(guard) = arm.guard {
                output.push_str(" if ");
                self.write_expr(guard, 0, output);
            }
            output.push_str(" => ");
            self.write_expr(arm.value, 0, output);
        }
        if !arms.is_empty() {
            output.push(' ');
        }
        output.push('}');
    }

    fn write_match_expr_multiline(
        &mut self,
        value: ExprId,
        arms: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let indent = indent_for_expr(output);
        output.push_str("match ");
        self.write_expr(value, 0, output);
        output.push_str(" {");
        if arms.is_empty() {
            output.push('}');
            return;
        }
        output.push('\n');
        for index in 0..arms.len() {
            let arm = self.arena.match_expr_arms(arms)[index].clone();
            self.write_indent(indent + 1, output);
            self.write_pattern(arm.pattern, output);
            if let Some(guard) = arm.guard {
                output.push_str(" if ");
                self.write_expr(guard, 0, output);
            }
            output.push_str(" => ");
            self.write_expr_safe(arm.value, output);
            output.push_str(",\n");
        }
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_stream_stage(&mut self, stage: &ArenaStreamStage, indent: usize, output: &mut String) {
        output.push_str(stage.kind.as_str());
        let options = self.arena.stream_options(stage.options).to_vec();
        for option in &options {
            output.push_str(" --");
            output.push_str(option.name.as_str().as_str());
            if let Some(value) = option.value {
                output.push('=');
                self.write_expr(value, 0, output);
            }
        }
        if !stage.args.is_empty()
            || (stage.block.is_none() && stage.kind.canonical_parens_when_empty())
        {
            self.write_call_args(stage.args, false, output);
        }
        if let Some(expr) = self.inline_stream_block_expr(stage) {
            output.push(' ');
            self.write_expr(expr, 0, output);
        } else if let Some(block) = stage.block {
            output.push(' ');
            self.write_block(block, indent, output);
        }
    }

    fn write_stream_stages(
        &mut self,
        stages: xsh::frontend::syntax::arena::ArenaRange,
        indent: usize,
        output: &mut String,
    ) {
        let stages = self.arena.stream_stages(stages).to_vec();
        if let [stage] = stages.as_slice() {
            let inline = self.render_inline(|writer, inline| {
                inline.push_str(" |> ");
                writer.write_stream_stage(stage, indent, inline);
            });
            if self.fits_inline(output, &inline) {
                output.push_str(&inline);
                return;
            }
        }
        for stage in &stages {
            output.push('\n');
            self.write_indent(indent, output);
            output.push_str("|> ");
            self.write_stream_stage(stage, indent, output);
        }
    }

    fn write_fmt_string(
        &mut self,
        parts: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        self.write_fmt_string_with_prefix("f", parts, output);
    }

    fn write_path_fmt_string(
        &mut self,
        parts: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        self.write_fmt_string_with_prefix("fp", parts, output);
    }

    fn write_fmt_string_with_prefix(
        &mut self,
        prefix: &str,
        parts: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let parts: Vec<ArenaFmtPart> = self.arena.fmt_parts(parts).collect();
        let multiline = parts.iter().any(|part| match part {
            ArenaFmtPart::Text(text) => self.text_value(text).contains('\n'),
            ArenaFmtPart::Expr(..) => false,
        });
        output.push_str(prefix);
        if multiline {
            output.push_str("\"\"\"");
        } else {
            output.push('"');
        }
        let len = parts.len();
        for (index, part) in parts.iter().enumerate() {
            match part {
                ArenaFmtPart::Text(text) if multiline => {
                    let text = self.text_value(text).to_string();
                    write_triple_text(&text, index + 1 == len, output);
                }
                ArenaFmtPart::Text(text) => {
                    let text = self.text_value(text).to_string();
                    write_fmt_text(&text, output);
                }
                ArenaFmtPart::Expr(expr, spec) => {
                    output.push_str("${");
                    self.write_expr(*expr, 0, output);
                    if let Some(spec) = spec {
                        output.push(':');
                        match spec.kind {
                            FormatSpecKind::RightAlign => output.push('>'),
                            FormatSpecKind::LeftAlign => output.push('<'),
                            FormatSpecKind::ZeroPad => output.push('0'),
                        }
                        write!(output, "{}", spec.width).unwrap();
                    }
                    output.push('}');
                }
            }
        }
        if multiline {
            output.push_str("\"\"\"");
        } else {
            output.push('"');
        }
    }

    fn write_builder_block(
        &mut self,
        block_id: xsh::frontend::syntax::arena::BuilderBlockId,
        indent: usize,
        output: &mut String,
    ) {
        let block = self.arena.builder_block(block_id).clone();
        let entries: Vec<xsh::frontend::syntax::arena::ArenaBuilderEntry> =
            self.arena.builder_entries(block.entries).to_vec();
        output.push('{');
        if entries.is_empty() {
            output.push('}');
            return;
        }
        output.push('\n');
        for (index, entry) in entries.iter().enumerate() {
            if index > 0 {
                output.push('\n');
            }
            self.write_builder_entry(entry, indent + 1, output);
        }
        output.push('\n');
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_builder_entry(
        &mut self,
        entry: &xsh::frontend::syntax::arena::ArenaBuilderEntry,
        indent: usize,
        output: &mut String,
    ) {
        self.write_indent(indent, output);
        match &entry.kind {
            ArenaBuilderEntryKind::Field { name, value } => {
                output.push_str(name.as_str().as_str());
                output.push_str(" = ");
                self.write_expr(*value, 0, output);
            }
            ArenaBuilderEntryKind::Entry { name, args, block } => {
                output.push_str(name.as_str().as_str());
                self.write_command_args(*args, output);
                if let Some(block) = block {
                    output.push(' ');
                    self.write_builder_block(*block, indent, output);
                }
            }
            ArenaBuilderEntryKind::Task { name, block } => {
                output.push_str("task ");
                output.push_str(name.as_str().as_str());
                output.push_str("() ");
                self.write_block(*block, indent, output);
            }
            ArenaBuilderEntryKind::Stmt(stmt) => self.write_stmt_inline(*stmt, indent, output),
        }
    }

    fn write_optional_type(&mut self, ty: Option<TypeExprId>, output: &mut String) {
        if let Some(ty) = ty {
            output.push_str(": ");
            self.write_type(ty, output);
        }
    }

    fn write_type(&mut self, ty: TypeExprId, output: &mut String) {
        match type_expr_kind(self.arena, ty) {
            ArenaTypeExprKind::Named(name) => output.push_str(name.as_str().as_str()),
            ArenaTypeExprKind::Qualified { namespace, name } => {
                output.push_str(namespace.as_str().as_str());
                output.push('.');
                output.push_str(name.as_str().as_str());
            }
            ArenaTypeExprKind::List(inner) => {
                output.push_str("List[");
                self.write_type(inner, output);
                output.push(']');
            }
            ArenaTypeExprKind::Map(inner) => {
                output.push_str("Map[");
                self.write_type(inner, output);
                output.push(']');
            }
            ArenaTypeExprKind::Stream(inner) => {
                output.push_str("Stream[");
                self.write_type(inner, output);
                output.push(']');
            }
            ArenaTypeExprKind::Module(inner) => {
                output.push_str("Module[");
                self.write_type(inner, output);
                output.push(']');
            }
            ArenaTypeExprKind::Result { ok, err } => {
                output.push_str("Result[");
                self.write_type(ok, output);
                if let Some(err) = err {
                    output.push_str(", ");
                    self.write_type(err, output);
                }
                output.push(']');
            }
            ArenaTypeExprKind::Optional(inner) => {
                self.write_type(inner, output);
                output.push('?');
            }
        }
    }

    fn write_record_schema(
        &mut self,
        fields: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let schema_fields = self.arena.schema_fields(fields).to_vec();
        let field_ids: Vec<(Name, TypeExprId)> =
            schema_fields.iter().map(|f| (f.name, f.ty)).collect();
        let original_multiline = schema_fields
            .first()
            .zip(schema_fields.last())
            .and_then(|(first, last)| {
                let start = self.arena.span(first.span).start();
                let end = self.arena.span(last.span).end();
                self.source.get(start..end)
            })
            .is_some_and(|source| source.contains('\n'));
        let inline =
            self.render_inline(|writer, inline| writer.write_record_schema_inline(fields, inline));
        if field_ids.is_empty()
            || (!original_multiline
                && field_ids.len() < MULTILINE_SCHEMA_FIELD_THRESHOLD
                && self.fits_inline(output, &inline))
        {
            output.push_str(&inline);
            return;
        }

        let indent = indent_for_expr(output);
        output.push_str("{\n");
        for field in &field_ids {
            self.write_indent(indent + 1, output);
            self.write_schema_field(field, output);
            output.push_str(",\n");
        }
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_record_schema_inline(
        &mut self,
        fields: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let field_ids: Vec<(Name, TypeExprId)> = self
            .arena
            .schema_fields(fields)
            .iter()
            .map(|f| (f.name, f.ty))
            .collect();
        output.push('{');
        for (index, field) in field_ids.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            self.write_schema_field(field, output);
        }
        output.push('}');
    }

    fn write_schema_field(&mut self, field: &(Name, TypeExprId), output: &mut String) {
        output.push_str(field.0.as_str().as_str());
        output.push_str(": ");
        self.write_type(field.1, output);
    }

    fn write_module_contract(
        &mut self,
        entries: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let indent = indent_for_expr(output);
        let entries = self.arena.module_contract_entries(entries).to_vec();
        output.push_str("module {\n");
        for entry in &entries {
            self.write_indent(indent + 1, output);
            self.write_module_contract_entry(entry, output);
            output.push('\n');
        }
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_module_contract_entry(
        &mut self,
        entry: &xsh::frontend::syntax::arena::ArenaModuleContractEntry,
        output: &mut String,
    ) {
        output.push_str("export ");
        if entry.optional {
            output.push_str("optional ");
        }
        match &entry.kind {
            ArenaModuleContractEntryKind::Value(ty) => {
                output.push_str("let ");
                output.push_str(entry.name.as_str().as_str());
                output.push_str(": ");
                self.write_type(*ty, output);
            }
            ArenaModuleContractEntryKind::Proc {
                params,
                effects,
                return_ty,
            } => {
                output.push_str("proc ");
                output.push_str(entry.name.as_str().as_str());
                self.write_params(*params, output);
                if let Some(effects) = effects {
                    let effects: Vec<Effect> = self.arena.effects(*effects).collect();
                    output.push(' ');
                    self.write_effect_list(&effects, output);
                }
                output.push_str(" -> ");
                self.write_type(*return_ty, output);
            }
            ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                output.push_str("pure ");
                output.push_str(entry.name.as_str().as_str());
                self.write_params(*params, output);
                output.push_str(" -> ");
                self.write_type(*return_ty, output);
            }
        }
    }

    fn write_call_args(
        &mut self,
        args: xsh::frontend::syntax::arena::ArenaRange,
        original_multiline: bool,
        output: &mut String,
    ) {
        let inline =
            self.render_inline(|writer, inline| writer.write_call_args_inline(args, inline));
        if args.is_empty() || (!original_multiline && self.fits_inline(output, &inline)) {
            output.push_str(&inline);
            return;
        }
        if args.len() == 1 {
            let arg_kind = self.arena.call_args(args)[0].kind.clone();
            if self.call_arg_is_multiline_literal(&arg_kind)
                && self.fits_multiline_inline(output, &inline)
            {
                if self.call_arg_is_multiline_record(&arg_kind) {
                    self.write_multiline_inline(&inline, output);
                } else {
                    output.push_str(&inline);
                }
                return;
            }
        }

        let arg_kinds: Vec<xsh::frontend::syntax::arena::ArenaCallArgKind> = self
            .arena
            .call_args(args)
            .iter()
            .map(|a| a.kind.clone())
            .collect();
        let indent = indent_for_expr(output);
        let broken = self.render_broken_call_args_doc(&arg_kinds, indent);
        let doc = Doc::group(Doc::text(inline), broken, original_multiline);
        output.push_str(&doc.render_with_indent(
            self.line_width,
            current_line_width(output),
            indent,
        ));
    }

    fn render_broken_call_args_doc(
        &mut self,
        args: &[xsh::frontend::syntax::arena::ArenaCallArgKind],
        indent: usize,
    ) -> Doc {
        let mut body = vec![Doc::Line];
        let continuation = "  ".repeat(indent + 1);
        for (index, arg) in args.iter().enumerate() {
            let mut output = String::new();
            self.write_call_arg_multiline(arg, &mut output);
            let output = if output.trim_start().contains("\"\"\"") {
                output
            } else {
                output.replace('\n', &format!("\n{continuation}"))
            };
            body.push(Doc::text(output));
            body.push(Doc::text(","));
            if index + 1 < args.len() {
                body.push(Doc::Line);
            }
        }
        Doc::Concat(vec![
            Doc::text("("),
            Doc::Indent(
                1,
                Box::new(Doc::Concat(vec![
                    Doc::Concat(body),
                    Doc::Dedent(1, Box::new(Doc::Concat(vec![Doc::Line, Doc::text(")")]))),
                ])),
            ),
        ])
    }

    fn call_args_original_multiline(
        &self,
        args: xsh::frontend::syntax::arena::ArenaRange,
        callee_end: usize,
        call_end: usize,
    ) -> bool {
        let mut previous_end = callee_end;
        for arg in self.arena.call_args(args) {
            let span = match &arg.kind {
                xsh::frontend::syntax::arena::ArenaCallArgKind::Positional(expr) => {
                    self.arena.expr(*expr).span
                }
                xsh::frontend::syntax::arena::ArenaCallArgKind::Splice { span, .. }
                | xsh::frontend::syntax::arena::ArenaCallArgKind::Named { span, .. } => {
                    self.arena.span(*span)
                }
            };
            if self
                .source
                .get(previous_end..span.start())
                .is_some_and(|gap| gap.contains('\n'))
            {
                return true;
            }
            previous_end = span.end();
        }
        self.source
            .get(previous_end..call_end)
            .is_some_and(|gap| gap.contains('\n'))
    }

    fn try_write_stable_call_chain(&mut self, expr_id: ExprId, output: &mut String) -> bool {
        let Some((base, segments)) = call_chain_segments(self.arena, expr_id) else {
            return false;
        };
        if segments.len() < 2 {
            return false;
        }
        let expr_span = self.arena.expr(expr_id).span;
        if self
            .source
            .get(expr_span.range())
            .is_some_and(|source| source.contains('#'))
        {
            return false;
        }
        let inline = self.render_inline(|writer, inline| {
            writer.write_expr(base, 0, inline);
            for segment in &segments {
                inline.push('.');
                inline.push_str(segment.name.as_str().as_str());
                writer.write_call_args_inline(segment.args, inline);
            }
        });
        if self.fits_inline(output, &inline) && !self.expr_source_is_multiline(expr_id) {
            return false;
        }

        self.write_expr(base, 0, output);
        let indent = continuation_indent_for_expr(output);
        for (index, segment) in segments.iter().enumerate() {
            if index > 0 {
                output.push('\n');
                self.write_indent(indent, output);
            }
            output.push('.');
            output.push_str(segment.name.as_str().as_str());
            self.write_call_args(segment.args, false, output);
        }
        true
    }

    fn write_call_args_inline(
        &mut self,
        args: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        output.push('(');
        for index in 0..args.len() {
            if index > 0 {
                output.push_str(", ");
            }
            let arg = self.arena.call_args(args)[index].kind.clone();
            self.write_call_arg(&arg, output);
        }
        output.push(')');
    }

    fn write_multiline_inline(&self, inline: &str, output: &mut String) {
        let prefix = current_line_indent(output).to_string();
        for (index, line) in inline.split('\n').enumerate() {
            if index > 0 {
                output.push('\n');
                output.push_str(&prefix);
            }
            output.push_str(line);
        }
    }

    fn write_call_arg(
        &mut self,
        arg: &xsh::frontend::syntax::arena::ArenaCallArgKind,
        output: &mut String,
    ) {
        use xsh::frontend::syntax::arena::ArenaCallArgKind;
        match arg {
            ArenaCallArgKind::Positional(expr) => self.write_expr_safe(*expr, output),
            ArenaCallArgKind::Splice { value, .. } => {
                output.push('@');
                if matches!(self.arena.expr(*value).kind, ArenaExprKind::Ident(_)) {
                    self.write_expr_safe(*value, output);
                } else {
                    output.push('(');
                    self.write_expr_safe(*value, output);
                    output.push(')');
                }
            }
            ArenaCallArgKind::Named { name, value, .. } => {
                output.push_str(name.as_str().as_str());
                output.push_str(": ");
                self.write_expr_safe(*value, output);
            }
        }
    }

    fn write_call_arg_multiline(
        &mut self,
        arg: &xsh::frontend::syntax::arena::ArenaCallArgKind,
        output: &mut String,
    ) {
        use xsh::frontend::syntax::arena::ArenaCallArgKind;
        match arg {
            ArenaCallArgKind::Positional(expr) => {
                self.write_expr_safe_multiline_preferred(*expr, output)
            }
            ArenaCallArgKind::Splice { value, .. } => {
                output.push('@');
                if matches!(self.arena.expr(*value).kind, ArenaExprKind::Ident(_)) {
                    self.write_expr_safe(*value, output);
                } else {
                    output.push('(');
                    self.write_expr_safe_multiline_preferred(*value, output);
                    output.push(')');
                }
            }
            ArenaCallArgKind::Named { name, value, .. } => {
                output.push_str(name.as_str().as_str());
                output.push_str(": ");
                self.write_expr_safe_multiline_preferred(*value, output);
            }
        }
    }

    fn write_expr_safe(&mut self, expr_id: ExprId, output: &mut String) {
        let kind = self.arena.expr(expr_id).kind;
        match &kind {
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let branches = *branches;
                let else_value = *else_value;
                let inline = self.render_inline(|writer, inline| {
                    writer.write_if_expr(branches, else_value, inline);
                });
                if self.inline_only
                    || (self.fits_inline(output, &inline)
                        && !self.expr_source_is_multiline(expr_id))
                {
                    output.push_str(&inline);
                } else {
                    self.write_if_expr_multiline(branches, else_value, output);
                }
            }
            ArenaExprKind::Match { value, arms } => {
                let value = *value;
                let arms = *arms;
                let inline = self.render_inline(|writer, inline| {
                    writer.write_match_expr(value, arms, inline);
                });
                if self.inline_only
                    || (self.fits_inline(output, &inline)
                        && !self.expr_source_is_multiline(expr_id))
                {
                    output.push_str(&inline);
                } else {
                    self.write_match_expr_multiline(value, arms, output);
                }
            }
            ArenaExprKind::ListComp { .. } => {
                let inline = self.render_inline(|writer, inline| {
                    writer.write_expr(expr_id, 0, inline);
                });
                if self.inline_only
                    || (self.fits_inline(output, &inline)
                        && !self.expr_source_is_multiline(expr_id))
                {
                    output.push_str(&inline);
                } else {
                    self.write_list_comp_multiline(expr_id, output);
                }
            }
            ArenaExprKind::MapComp { .. } => {
                let inline = self.render_inline(|writer, inline| {
                    writer.write_expr(expr_id, 0, inline);
                });
                if self.inline_only
                    || (self.fits_inline(output, &inline)
                        && !self.expr_source_is_multiline(expr_id))
                {
                    output.push_str(&inline);
                } else {
                    self.write_map_comp_multiline(expr_id, output);
                }
            }
            _ => self.write_expr(expr_id, 0, output),
        }
    }

    fn write_expr_safe_multiline_preferred(&mut self, expr_id: ExprId, output: &mut String) {
        let kind = self.arena.expr(expr_id).kind;
        match &kind {
            ArenaExprKind::If {
                branches,
                else_value,
            } => self.write_if_expr_multiline(*branches, *else_value, output),
            ArenaExprKind::Match { value, arms } => {
                self.write_match_expr_multiline(*value, *arms, output)
            }
            ArenaExprKind::ListComp { .. } => self.write_list_comp_multiline(expr_id, output),
            ArenaExprKind::MapComp { .. } => self.write_map_comp_multiline(expr_id, output),
            _ => self.write_expr_safe(expr_id, output),
        }
    }

    fn write_list_comp_multiline(&mut self, expr_id: ExprId, output: &mut String) {
        let ArenaExprKind::ListComp {
            expr,
            target,
            iter,
            condition,
        } = self.arena.expr(expr_id).kind
        else {
            return self.write_expr(expr_id, 0, output);
        };
        let indent = indent_for_expr(output);
        output.push_str("[\n");
        self.write_indent(indent + 1, output);
        self.write_expr_safe(expr, output);
        output.push('\n');
        self.write_indent(indent + 1, output);
        output.push_str("for ");
        self.write_binding_target(target, output);
        output.push_str(" in ");
        self.write_expr(iter, 0, output);
        if let Some(condition) = condition {
            output.push('\n');
            self.write_indent(indent + 1, output);
            output.push_str("if ");
            self.write_expr(condition, 0, output);
        }
        output.push('\n');
        self.write_indent(indent, output);
        output.push(']');
    }

    fn write_map_comp_multiline(&mut self, expr_id: ExprId, output: &mut String) {
        let ArenaExprKind::MapComp {
            key,
            value,
            target,
            iter,
            condition,
        } = self.arena.expr(expr_id).kind
        else {
            return self.write_expr(expr_id, 0, output);
        };
        let indent = indent_for_expr(output);
        output.push_str("{\n");
        self.write_indent(indent + 1, output);
        self.write_expr_safe(key, output);
        output.push_str(": ");
        self.write_expr_safe(value, output);
        output.push('\n');
        self.write_indent(indent + 1, output);
        output.push_str("for ");
        self.write_binding_target(target, output);
        output.push_str(" in ");
        self.write_expr(iter, 0, output);
        if let Some(condition) = condition {
            output.push('\n');
            self.write_indent(indent + 1, output);
            output.push_str("if ");
            self.write_expr(condition, 0, output);
        }
        output.push('\n');
        self.write_indent(indent, output);
        output.push('}');
    }

    fn expr_source_is_multiline(&self, expr_id: ExprId) -> bool {
        let span = self.arena.expr(expr_id).span;
        self.source
            .get(span.range())
            .is_some_and(|source| source.contains('\n'))
    }

    fn write_comments_before(&mut self, offset: usize, indent: usize, output: &mut String) -> bool {
        let mut skip_formatting = false;
        while self.has_comment_before(offset) {
            self.write_indent(indent, output);
            output.push('#');
            let text = self.comments[self.next_comment].text.trim_end();
            if text.trim() == "fmt: skip" {
                skip_formatting = true;
            }
            output.push_str(text);
            output.push('\n');
            self.next_comment += 1;
        }
        skip_formatting
    }

    fn write_remaining_comments(&mut self, indent: usize, output: &mut String) {
        while self.next_comment < self.comments.len() {
            self.write_indent(indent, output);
            output.push('#');
            output.push_str(self.comments[self.next_comment].text.trim_end());
            output.push('\n');
            self.next_comment += 1;
        }
    }

    fn has_comment_before(&self, offset: usize) -> bool {
        self.comments
            .get(self.next_comment)
            .is_some_and(|comment| comment.span.start() < offset)
    }

    fn write_trailing_comment(&mut self, offset: usize, output: &mut String) {
        let Some(comment) = self.comments.get(self.next_comment) else {
            return;
        };
        if matches!(
            self.source.as_bytes().get(offset.saturating_sub(1)),
            Some(b'\n' | b'\r')
        ) {
            return;
        }
        if comment.span.start() < offset {
            return;
        }
        let Some(gap) = self.source.get(offset..comment.span.start()) else {
            return;
        };
        if gap.contains('\n') || gap.contains('\r') {
            return;
        }
        output.push_str(" #");
        output.push_str(comment.text.trim_end());
        self.next_comment += 1;
    }

    fn write_raw_stmt(&self, span: Span, output: &mut String) {
        let raw = self
            .source
            .get(span.start()..span.end())
            .unwrap_or("")
            .trim_matches(|ch| ch == '\n' || ch == '\r');
        output.push_str(raw);
    }

    fn write_indent(&self, indent: usize, output: &mut String) {
        for _ in 0..indent {
            output.push_str("  ");
        }
    }

    fn fits_inline(&self, output: &str, inline: &str) -> bool {
        self.fits_inline_with_extra(output, inline, 0)
    }

    fn fits_inline_with_extra(&self, output: &str, inline: &str, extra: usize) -> bool {
        !inline.contains('\n')
            && current_line_width(output) + inline.chars().count() + extra <= self.line_width
    }

    fn fits_multiline_inline(&self, output: &str, inline: &str) -> bool {
        let mut lines = inline.split('\n');
        let Some(first) = lines.next() else {
            return true;
        };
        current_line_width(output) + first.chars().count() <= self.line_width
            && lines.all(|line| line.chars().count() <= self.line_width)
    }

    fn render_inline(&self, f: impl FnOnce(&mut Writer, &mut String)) -> String {
        let mut writer = Writer {
            arena: self.arena,
            source: Arc::clone(&self.source),
            comments: Vec::new(),
            next_comment: 0,
            line_width: self.line_width,
            force_collection_expanded: false,
            inline_only: true,
        };
        let mut output = String::new();
        f(&mut writer, &mut output);
        output
    }

    fn stmt_preview(&self, stmt_id: StmtId, indent: usize) -> String {
        self.render_inline(|writer, output| writer.write_stmt(stmt_id, indent, output))
    }

    fn text_value<'b>(&'b self, text: &'b ArenaText) -> &'b str {
        self.arena.text_value(text, &self.source).unwrap_or("")
    }

    fn join_name_range(
        &self,
        range: xsh::frontend::syntax::arena::ArenaRange,
        separator: &str,
    ) -> String {
        self.arena
            .names(range)
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(separator)
    }

    fn inline_stream_block_expr(&self, stage: &ArenaStreamStage) -> Option<ExprId> {
        if !matches!(
            stage.kind,
            StreamStageKind::Where
                | StreamStageKind::Map
                | StreamStageKind::ParMap
                | StreamStageKind::Each
                | StreamStageKind::SortBy
                | StreamStageKind::UniqueBy
                | StreamStageKind::Tee
                | StreamStageKind::GroupBy
                | StreamStageKind::FlatMap
                | StreamStageKind::Any
                | StreamStageKind::All
        ) {
            return None;
        }
        let block_id = stage.block?;
        let block = self.arena.block(block_id);
        let stmts: Vec<StmtId> = self.arena.stmt_ids(block.statements).collect();
        if !block.params.is_empty() || stmts.len() != 1 {
            return None;
        }
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(stmts[0]).kind else {
            return None;
        };
        Some(expr)
    }

    fn call_arg_is_multiline_literal(
        &self,
        arg: &xsh::frontend::syntax::arena::ArenaCallArgKind,
    ) -> bool {
        use xsh::frontend::syntax::arena::ArenaCallArgKind;
        match arg {
            ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. } => {
                self.expr_is_multiline_literal(*expr)
            }
            ArenaCallArgKind::Splice { value, .. } => self.expr_is_multiline_literal(*value),
        }
    }

    fn call_arg_is_multiline_record(
        &self,
        arg: &xsh::frontend::syntax::arena::ArenaCallArgKind,
    ) -> bool {
        use xsh::frontend::syntax::arena::ArenaCallArgKind;
        match arg {
            ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. } => {
                matches!(self.arena.expr(*expr).kind, ArenaExprKind::Record(_))
            }
            ArenaCallArgKind::Splice { value, .. } => {
                matches!(self.arena.expr(*value).kind, ArenaExprKind::Record(_))
            }
        }
    }

    fn expr_is_multiline_literal(&self, expr_id: ExprId) -> bool {
        match self.arena.expr(expr_id).kind {
            ArenaExprKind::Str(value) => self.arena.string_literal(value).contains('\n'),
            ArenaExprKind::Record(fields) => {
                record_fields_original_multiline(
                    self.arena,
                    &self.source,
                    self.arena.record_fields(fields),
                ) || fields.len() >= MULTILINE_RECORD_FIELD_THRESHOLD
            }
            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
                self.arena.fmt_parts(parts).any(|part| match part {
                    ArenaFmtPart::Text(text) => self.text_value(&text).contains('\n'),
                    ArenaFmtPart::Expr(..) => false,
                })
            }
            _ => false,
        }
    }

    fn command_typed_arg_can_be_bare(&self, expr_id: ExprId) -> bool {
        match self.arena.expr(expr_id).kind {
            ArenaExprKind::Ident(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::FmtString(_)
            | ArenaExprKind::PathFmtString(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::GlobStr(_) => true,
            ArenaExprKind::Call { callee, .. } => self.command_chain_base_can_be_bare(callee),
            ArenaExprKind::Index { base, index } => {
                self.command_chain_base_can_be_bare(base)
                    && matches!(self.arena.expr(index).kind, ArenaExprKind::Int(_))
            }
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.command_chain_has_call_or_index(expr_id)
                    && self.command_chain_base_can_be_bare(base)
            }
            _ => false,
        }
    }

    fn command_chain_base_can_be_bare(&self, expr_id: ExprId) -> bool {
        match self.arena.expr(expr_id).kind {
            ArenaExprKind::Ident(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::FmtString(_)
            | ArenaExprKind::PathFmtString(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::GlobStr(_) => true,
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.command_chain_base_can_be_bare(base)
            }
            ArenaExprKind::Call { callee, .. } => self.command_chain_base_can_be_bare(callee),
            ArenaExprKind::Index { base, index } => {
                self.command_chain_base_can_be_bare(base)
                    && matches!(self.arena.expr(index).kind, ArenaExprKind::Int(_))
            }
            _ => false,
        }
    }

    fn command_chain_has_call_or_index(&self, expr_id: ExprId) -> bool {
        match self.arena.expr(expr_id).kind {
            ArenaExprKind::Call { .. } | ArenaExprKind::Index { .. } => true,
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.command_chain_has_call_or_index(base)
            }
            _ => false,
        }
    }
}

fn needs_top_level_blank(previous: &ArenaStmtKind, current: &ArenaStmtKind) -> bool {
    matches!(
        (previous, current),
        (
            ArenaStmtKind::ProcDef(_) | ArenaStmtKind::PureDef(_) | ArenaStmtKind::StreamDef(_),
            _
        ) | (
            _,
            ArenaStmtKind::ProcDef(_) | ArenaStmtKind::PureDef(_) | ArenaStmtKind::StreamDef(_)
        ) | (ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_), _)
            | (_, ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_))
            | (ArenaStmtKind::Export(_), _)
            | (_, ArenaStmtKind::Export(_))
    ) || is_top_level_section(previous)
        || is_top_level_section(current)
}

fn is_top_level_section(kind: &ArenaStmtKind) -> bool {
    matches!(
        kind,
        ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::Match { .. }
            | ArenaStmtKind::With { .. }
    )
}

fn expr_precedence(kind: &ArenaExprKind) -> u8 {
    match kind {
        ArenaExprKind::Binary { op, .. } => match op {
            BinaryOp::ResultFallback => 1,
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Eq | BinaryOp::Ne => 3,
            BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::In
            | BinaryOp::NotIn => 4,
            BinaryOp::Add | BinaryOp::Sub => 5,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 6,
        },
        ArenaExprKind::If { .. } | ArenaExprKind::Match { .. } => 0,
        ArenaExprKind::Unary { .. } => 7,
        ArenaExprKind::Call { .. }
        | ArenaExprKind::Field { .. }
        | ArenaExprKind::NullSafeField { .. }
        | ArenaExprKind::EnvGet { .. }
        | ArenaExprKind::EnvPathList
        | ArenaExprKind::Index { .. }
        | ArenaExprKind::Slice { .. }
        | ArenaExprKind::BuilderCall { .. }
        | ArenaExprKind::Require { .. }
        | ArenaExprKind::Try(_) => 8,
        ArenaExprKind::Pipeline { .. } | ArenaExprKind::StructuredPipeline { .. } => 0,
        ArenaExprKind::Run(_) | ArenaExprKind::Spawn(_) | ArenaExprKind::Wait(_) => 9,
        _ => 9,
    }
}

fn original_preserved_string_literal(source: &str, span: Span) -> Option<&str> {
    let original = source.get(span.range())?;
    let trimmed = original.trim_start();
    (trimmed.starts_with('r') || trimmed.contains("\"\"\"")).then_some(original)
}

fn original_multiline_string_literal(source: &str, span: Span) -> Option<&str> {
    let original = source.get(span.range())?;
    original.contains("\"\"\"").then_some(original)
}

fn record_fields_original_multiline(
    arena: &AstArena,
    source: &str,
    fields: &[ArenaRecordField],
) -> bool {
    let Some(first) = fields
        .first()
        .and_then(|field| record_field_span(arena, &field.kind))
    else {
        return false;
    };
    let Some(last) = fields
        .last()
        .and_then(|field| record_field_span(arena, &field.kind))
    else {
        return false;
    };
    source
        .get(first.start()..last.end())
        .is_some_and(|source| source.contains('\n'))
}

fn record_field_span(arena: &AstArena, field: &ArenaRecordFieldKind) -> Option<Span> {
    match field {
        ArenaRecordFieldKind::Named { span, .. }
        | ArenaRecordFieldKind::Shorthand { span, .. }
        | ArenaRecordFieldKind::Spread { span, .. } => Some(arena.span(*span)),
    }
}

fn call_chain_segments(
    arena: &AstArena,
    expr_id: ExprId,
) -> Option<(ExprId, Vec<CallChainSegment>)> {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr_id).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    let (root, mut segments) =
        call_chain_segments(arena, base).unwrap_or_else(|| (base, Vec::new()));
    segments.push(CallChainSegment { name, args });
    Some((root, segments))
}

fn indent_for_expr(output: &str) -> usize {
    output.rsplit('\n').next().map_or(0, |line| {
        line.chars().take_while(|ch| *ch == ' ').count() / 2
    })
}

fn continuation_indent_for_expr(output: &str) -> usize {
    indent_for_expr(output) + 1
}

fn current_line_width(output: &str) -> usize {
    output
        .rsplit('\n')
        .next()
        .map_or(0, |line| line.chars().count())
}

fn current_line_indent(output: &str) -> &str {
    output.rsplit('\n').next().map_or("", |line| {
        &line[..line.len() - line.trim_start_matches(' ').len()]
    })
}

fn binary_op_text(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::ResultFallback => "??",
        BinaryOp::Or => "or",
        BinaryOp::And => "and",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::In => "in",
        BinaryOp::NotIn => "not in",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
    }
}

fn assign_op_text(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Set => "=",
        AssignOp::Add => "+=",
        AssignOp::Sub => "-=",
        AssignOp::Mul => "*=",
        AssignOp::Div => "/=",
        AssignOp::Rem => "%=",
    }
}

fn run_head_text(kind: RunKind, builtin: bool) -> &'static str {
    match (builtin, kind) {
        (false, RunKind::Plain) => "run",
        (false, RunKind::Status) => "run.status",
        (false, RunKind::CaptureText) => "run.text",
        (false, RunKind::CaptureBytes) => "run.bytes",
        (false, RunKind::CaptureTextRecord) => "run.capture --text",
        (false, RunKind::CaptureBytesRecord) => "run.capture --bytes",
        (false, RunKind::StreamText) => "run.stream --text",
        (false, RunKind::StreamBytes) => "run.stream --bytes",
        (true, RunKind::Plain) => "run.builtin",
        (true, RunKind::Status) => "run.builtin.status",
        (true, RunKind::CaptureText) => "run.builtin.text",
        (true, RunKind::CaptureBytes) => "run.builtin.bytes",
        (true, RunKind::CaptureTextRecord) => "run.builtin.capture --text",
        (true, RunKind::CaptureBytesRecord) => "run.builtin.capture --bytes",
        (true, RunKind::StreamText) => "run.builtin.stream --text",
        (true, RunKind::StreamBytes) => "run.builtin.stream --bytes",
    }
}

fn canonical_effects(effects: &[Effect]) -> Vec<&Effect> {
    [
        Effect::Fs,
        Effect::Net,
        Effect::Process,
        Effect::Env,
        Effect::Time,
        Effect::Error,
        Effect::Io,
    ]
    .iter()
    .filter_map(|canonical| effects.iter().find(|effect| *effect == canonical))
    .collect()
}

fn env_assignments_are_exprs(assignments: &[ArenaEnvAssignment]) -> bool {
    assignments
        .iter()
        .any(|assignment| matches!(assignment.value, ArenaEnvAssignmentValue::Expr(_)))
}

fn write_str_literal(value: &str, output: &mut String) {
    if value == "\n" {
        write_quoted(value, output);
    } else if value.contains('\n') {
        output.push_str("\"\"\"");
        write_triple_text(value, true, output);
        output.push_str("\"\"\"");
    } else {
        write_quoted(value, output);
    }
}

fn write_quoted(value: &str, output: &mut String) {
    write_quoted_with_dollar(value, false, output);
}

fn write_command_quoted(value: &str, output: &mut String) {
    write_quoted_with_dollar(value, true, output);
}

fn write_quoted_with_dollar(value: &str, command_shorthand: bool, output: &mut String) {
    output.push('"');
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '$' if should_escape_dollar(chars.peek().copied(), command_shorthand) => {
                output.push_str("\\$");
            }
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\0' => output.push_str("\\0"),
            ch if ch.is_ascii_graphic() || ch == ' ' => output.push(ch),
            ch => {
                let _ = write!(output, "\\u{{{:x}}}", ch as u32);
            }
        }
    }
    output.push('"');
}

fn write_triple_text(value: &str, trailing: bool, output: &mut String) {
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => {
                let mut run = 1usize;
                while chars.peek() == Some(&'"') {
                    chars.next();
                    run += 1;
                }
                let unescaped = if trailing && chars.peek().is_none() {
                    0
                } else {
                    run.min(2)
                };
                for _ in 0..(run - unescaped) {
                    output.push_str("\\\"");
                }
                for _ in 0..unescaped {
                    output.push('"');
                }
            }
            '\r' => output.push_str("\\r"),
            '\t' => output.push('\t'),
            '\0' => output.push_str("\\0"),
            '$' if should_escape_dollar(chars.peek().copied(), false) => output.push_str("\\$"),
            ch => output.push(ch),
        }
    }
}

fn write_fmt_text(value: &str, output: &mut String) {
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\0' => output.push_str("\\0"),
            '$' if should_escape_dollar(chars.peek().copied(), false) => output.push_str("\\$"),
            ch => output.push(ch),
        }
    }
}

fn should_escape_dollar(next: Option<char>, command_shorthand: bool) -> bool {
    next == Some('{') || (command_shorthand && next.is_some_and(is_identifier_start))
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn bare_path_literal_text(value: &str) -> Option<&str> {
    if literal::can_be_bare_path_literal(value) {
        Some(value)
    } else {
        None
    }
}

fn write_bytes(value: &[u8], output: &mut String) {
    output.push_str("b\"");
    for byte in value {
        match *byte {
            b'\\' => output.push_str("\\\\"),
            b'"' => output.push_str("\\\""),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\t' => output.push_str("\\t"),
            0 => output.push_str("\\0"),
            byte if byte.is_ascii_graphic() || byte == b' ' => output.push(byte as char),
            byte => {
                let _ = write!(output, "\\x{byte:02x}");
            }
        }
    }
    output.push('"');
}
