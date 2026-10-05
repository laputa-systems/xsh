#![allow(clippy::single_call_fn, dead_code)]

use std::fmt::Write as _;
use std::sync::Arc;
use xsh::diagnostic::{Diagnostic, DiagnosticCode};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::symbols::{Name, Symbol};
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCommand, ArenaCommandArg,
    ArenaCommandArgKind, ArenaCompQualifier, ArenaEnvAssignment, ArenaEnvAssignmentValue,
    ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaModuleContractEntryKind, ArenaPatternKind,
    ArenaPipeStageKind, ArenaProgram, ArenaRange, ArenaRecordFieldKind, ArenaRedirectionTarget,
    ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaSugar, ArenaText, SugarForm, ArenaTypeExprTag, ArenaWordPart,
    AstArena, BindingTargetId, BlockId, ExprId, FunctionDefId, PatternId, StmtId, TypeExprId,
};
use xsh::frontend::syntax::cst::SyntaxTree;
use xsh::frontend::syntax::grammar;
use xsh::frontend::syntax::grouping::{self, Context, Follow, FollowToken};
use xsh::frontend::syntax::lexer::Lexer;
use xsh::frontend::syntax::lexer::{join_tokens, lex_spellings, tokens_stay_separate};
use xsh::frontend::syntax::literal;
use xsh::frontend::syntax::node::{
    AssignOp, BinaryOp, CoreCommand, Effect, FormatSpecKind, RedirectionKind, RunKind,
    UnaryOp,
};
use xsh::frontend::syntax::parser::{ArenaParseOutput, Parser};
use xsh::frontend::syntax::token::TokenTag;

#[path = "format_equivalence.rs"]
mod format_equivalence;
pub(crate) use format_equivalence::canonical_subtree;
#[cfg(test)]
#[path = "format_proofs.rs"]
mod format_proofs;
#[cfg(test)]
#[path = "desugar_tests.rs"]
mod desugar_tests;

pub const DEFAULT_LINE_WIDTH: usize = 120;
/// Inside delimiters, before `,`, `)`, `]`, `:`, or `=>`.
const CLOSE: Context = Context::open(Follow::CLOSE);
/// At the end of a statement, line, or interpolation.
const END: Context = Context::open(Follow::END);
/// Before the `{` of a block.
const BRACE: Context = Context::open(Follow::BRACE);
/// Before a keyword.
const WORD: Context = Context::open(Follow::WORD);
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
    /// Set before each block statement: the previous statement ends in an
    /// expression that a line starting with `.name` would continue.
    after_expression: bool,
    /// Set while writing the unbraced expression statement of a `match` arm.
    arm_statement: bool,
    /// Set by a desugared print: a sugar statement is written as its
    /// expansion instead of as the user spelled it.
    expand_sugar: bool,
    /// For a desugared print, the source range of every sugar statement in
    /// source order, outermost first. Text that holds one is never copied
    /// from the source, because the copy would keep the sugar.
    sugar_ranges: Arc<Vec<std::ops::Range<usize>>>,
    /// For a desugared print, the legal spelling of each local an expansion
    /// bound under a name no identifier can spell.
    hidden_names: Arc<Vec<(Name, String)>>,
}

#[derive(Clone, Copy, Debug)]
struct CallChainSegment {
    name: Name,
    args: xsh::frontend::syntax::arena::ArenaRange,
}

/// A decoded type expression node, mirroring the arena's compact type-expr
/// encoding without referencing the old recursive AST.
enum ArenaTypeExprKind {
    Applied {
        base: TypeExprId,
        arguments: Vec<TypeExprId>,
    },
    Named(Name),
    Qualified {
        namespace: Name,
        name: Name,
    },
    List(TypeExprId),
    Map(Option<TypeExprId>, TypeExprId),
    Stream(TypeExprId),
    Module(TypeExprId),
    Result {
        ok: TypeExprId,
        err: Option<TypeExprId>,
    },
    Optional(TypeExprId),
    Union(Vec<TypeExprId>),
}

fn type_expr_kind(arena: &AstArena, id: TypeExprId) -> ArenaTypeExprKind {
    let index = id.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Applied => ArenaTypeExprKind::Applied {
            base: TypeExprId::from_index(data.lhs as usize),
            arguments: arena.applied_type_arguments(id).collect(),
        },
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
        ArenaTypeExprTag::Map => ArenaTypeExprKind::Map(
            TypeExprId::from_optional_raw(data.rhs),
            TypeExprId::from_index(data.lhs as usize),
        ),
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
        ArenaTypeExprTag::Union => {
            ArenaTypeExprKind::Union(arena.union_type_members(id).collect())
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
        let output = self.format_parsed_source_unverified(source, &parsed);
        verify_formatted_output(source_id, source, &parsed, output)
    }

    /// Formats `parsed`, whose root statements must come from `source`.
    ///
    /// The parse may also hold loaded modules; only its root statements are
    /// formatted and compared.
    pub fn format_parsed_source(&self, source: &str, parsed: &ArenaParseOutput) -> FormatOutput {
        let output = self.format_parsed_source_unverified(source, parsed);
        let source_id = parsed
            .arena
            .source_text_source_id()
            .unwrap_or(SourceId::new(0));
        verify_formatted_output(source_id, source, parsed, output)
    }

    fn format_parsed_source_unverified(
        &self,
        source: &str,
        parsed: &ArenaParseOutput,
    ) -> FormatOutput {
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
        let output = self.format_program_with_cst(source, program, &cst);
        if output.formatted == source {
            return output;
        }
        let original = Parser::parse_source_arena_only(source_id, source);
        verify_formatted_output(source_id, source, &original, output)
    }

    /// Prints `source` with every sugar statement replaced by its expansion
    /// into core forms, laid out as `format_source` would lay out that
    /// program.
    ///
    /// A comment on a sugar statement stays on its expansion. A local the
    /// expansion bound under a name no identifier can spell is printed under
    /// a fresh name that the source spells nowhere. The result is refused,
    /// with a diagnostic, unless it parses to exactly the tree the checker
    /// and the runtime read for `source`.
    pub fn desugar_source(&self, source_id: SourceId, source: &str) -> FormatOutput {
        let parsed = Parser::parse_source_arena_only(source_id, source);
        if !parsed.diagnostics.is_empty() {
            return FormatOutput {
                formatted: String::new(),
                diagnostics: parsed.diagnostics,
            };
        }
        let mut writer = self.writer(source, &parsed.arena, parsed.cst.get());
        writer.expand_sugar = true;
        writer.sugar_ranges = Arc::new(sugar_ranges(&parsed.arena, source_id));
        writer.hidden_names = Arc::new(hidden_names(&parsed.arena.arena, source));
        let renamed = !writer.hidden_names.is_empty();
        let expanded = writer.format_program(&parsed.arena);
        let refuse = |message: String| FormatOutput {
            formatted: String::new(),
            diagnostics: vec![
                Diagnostic::error(format!("cannot print the expansion of this file: {message}"))
                    .with_code(DiagnosticCode::FormatEquivalence)
                    .with_span(Span::new(source_id, 0, 0))
                    .with_note("this is an `xsht desugar` bug"),
            ],
        };
        let reparsed = Parser::parse_source_arena_only(source_id, &expanded);
        if let Some(error) = reparsed.diagnostics.first() {
            return refuse(format!("the expansion does not parse: {}", error.message));
        }
        if !reparsed.arena.arena.sugar_expansions.is_empty() {
            return refuse("a sugar statement was left as written".to_string());
        }
        // A renamed local spells differently on purpose; every other program
        // must come back node for node.
        if !renamed
            && format_equivalence::canonical(&reparsed.arena, &expanded).text
                != format_equivalence::canonical_expanded(&parsed.arena, source).text
        {
            return refuse("the printed program is not the expansion".to_string());
        }
        self.format_source(source_id, &expanded)
    }

    fn format_program_with_cst(
        &self,
        source: &str,
        program: &ArenaProgram,
        cst: &SyntaxTree,
    ) -> FormatOutput {
        FormatOutput {
            formatted: self.writer(source, program, cst).format_program(program),
            diagnostics: Vec::new(),
        }
    }

    fn writer<'a>(&self, source: &str, program: &'a ArenaProgram, cst: &SyntaxTree) -> Writer<'a> {
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

        Writer {
            arena: &program.arena,
            source: Arc::from(source),
            comments,
            next_comment: 0,
            line_width: self.line_width,
            force_collection_expanded: false,
            inline_only: false,
            after_expression: false,
            arm_statement: false,
            expand_sugar: false,
            sugar_ranges: Arc::default(),
            hidden_names: Arc::default(),
        }
    }
}

/// The source range of every sugar statement of the root source, in source
/// order with an enclosing statement before the ones inside it.
fn sugar_ranges(program: &ArenaProgram, source_id: SourceId) -> Vec<std::ops::Range<usize>> {
    let arena = &program.arena;
    let mut ranges = (0..arena.stmt_tags.len())
        .map(|index| arena.stmt(StmtId::from_index(index)))
        .filter(|stmt| {
            matches!(stmt.kind, ArenaStmtKind::Sugar { .. }) && stmt.span.source_id == source_id
        })
        .map(|stmt| stmt.span.range())
        .collect::<Vec<_>>();
    ranges.sort_by_key(|range| (range.start, std::cmp::Reverse(range.end)));
    ranges
}

/// Whether `text` is one identifier token, so a name spelled that way can be
/// written in source.
fn is_spellable_name(text: &str) -> bool {
    matches!(
        lex_spellings(text).as_slice(),
        [(TokenTag::Ident | TokenTag::ProcIdent, spelled)] if *spelled == text
    )
}

/// A legal name for the hidden local `hidden` that `taken` does not hold,
/// recorded in `taken`. It keeps the identifier characters of the hidden
/// name so a reader can tell which form bound it.
fn fresh_name(hidden: &str, taken: &mut std::collections::BTreeSet<String>) -> String {
    let stem = hidden
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    let stem = stem.trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == '_');
    let stem = if stem.is_empty() { "local" } else { stem };
    // The numeric suffix keeps the name from being a keyword.
    (1..)
        .map(|n| format!("{stem}_{n}"))
        .find(|candidate| taken.insert(candidate.clone()))
        .expect("an unused suffix exists")
}

/// The printed spelling of every local of the program whose name no
/// identifier can spell. Each fresh name differs from every identifier-shaped
/// word of `source`, so it cannot collide with a name in any scope.
fn hidden_names(arena: &AstArena, source: &str) -> Vec<(Name, String)> {
    let mut hidden: Vec<Name> = Vec::new();
    let mut note = |name: Name| {
        if !hidden.contains(&name) && !is_spellable_name(name.as_str().as_str()) {
            hidden.push(name);
        }
    };
    for target in &arena.binding_targets {
        if let ArenaBindingTargetKind::Name(name) = target.kind {
            note(name);
        }
    }
    for target in &arena.assign_targets {
        if let xsh::frontend::syntax::arena::ArenaAssignTargetKind::Name(name) = target.kind {
            note(name);
        }
    }
    for index in 0..arena.expr_tags.len() {
        if let ArenaExprKind::Ident(name) = arena.expr(ExprId::from_index(index)).kind {
            note(name);
        }
    }
    if hidden.is_empty() {
        return Vec::new();
    }
    let mut taken = source
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    hidden
        .into_iter()
        .map(|name| {
            let fresh = fresh_name(name.as_str().as_str(), &mut taken);
            (name, fresh)
        })
        .collect()
}

impl<'a> Writer<'a> {
    fn format_program(&mut self, program: &ArenaProgram) -> String {
        let mut output = String::new();
        let mut previous: Option<ArenaStmtKind> = None;
        let mut previous_span: Option<Span> = None;

        for stmt_id in program.statement_ids() {
            let stmt = self.arena.stmt(self.layout_stmt(stmt_id));
            if let Some(previous_span) = previous_span {
                output.push('\n');
                let forced = previous
                    .as_ref()
                    .is_some_and(|prev| needs_top_level_blank(prev, &stmt.kind));
                if forced || self.authored_blank_before(previous_span, stmt.span.start()) {
                    output.push('\n');
                }
            }
            self.after_expression = previous
                .as_ref()
                .is_some_and(grouping::statement_may_continue);
            self.write_stmt(stmt_id, 0, &mut output);
            previous = Some(stmt.kind);
            previous_span = Some(stmt.span);
        }

        if !output.is_empty() {
            output.push('\n');
        }
        if self.next_comment < self.comments.len() {
            if let Some(previous) = previous_span
                && self.authored_blank_before(previous, self.source.len())
            {
                output.push('\n');
            }
            self.write_closing_comments(self.source.len(), 0, &mut output);
        }
        output
    }

    /// Where the source text of a span ends, before trailing whitespace.
    /// Control-flow statement spans end at their closing brace while other
    /// statement spans, and expressions that end a line, own the newline.
    fn text_end(&self, span: Span) -> usize {
        self.source
            .get(span.range())
            .map_or(span.end(), |text| span.start() + text.trim_end().len())
    }

    fn blank_line_between(&self, start: usize, end: usize) -> bool {
        self.source
            .get(start..end)
            .is_some_and(|gap| gap.matches('\n').count() >= 2)
    }

    /// Whether the author left a blank line between the previous statement
    /// and the next one, or the comments leading it.
    fn authored_blank_before(&self, previous: Span, next_start: usize) -> bool {
        let next = self
            .comments
            .get(self.next_comment)
            .filter(|comment| comment.span.start() < next_start)
            .map_or(next_start, |comment| comment.span.start());
        self.blank_line_between(self.text_end(previous), next)
    }

    /// The statement whose kind decides layout: the statement itself, or in a
    /// desugared print the expansion that is written in its place. The two
    /// share a span.
    fn layout_stmt(&self, id: StmtId) -> StmtId {
        if self.expand_sugar {
            self.arena.core_stmt_id(id)
        } else {
            id
        }
    }

    /// Whether a desugared print must write this text itself because a
    /// sugar statement lies inside it.
    fn holds_sugar(&self, span: Span) -> bool {
        let first = self
            .sugar_ranges
            .partition_point(|range| range.start < span.start());
        self.sugar_ranges
            .get(first)
            .is_some_and(|range| range.end <= span.end())
    }

    /// The spelling a name is printed under.
    fn name_text(&self, name: Name) -> String {
        self.hidden_names
            .iter()
            .find(|(hidden, _)| *hidden == name)
            .map_or_else(|| name.as_str().as_str().to_string(), |(_, fresh)| fresh.clone())
    }

    fn write_stmt(&mut self, stmt_id: StmtId, indent: usize, output: &mut String) {
        let stmt = self.arena.stmt(stmt_id);
        let skip_formatting = self.write_comments_before(stmt.span.start(), indent, output)
            && !self.holds_sugar(stmt.span);
        if skip_formatting {
            self.write_indent(indent, output);
            self.write_raw_stmt(stmt.span, output);
            self.write_raw_trailing_comment(stmt.span.end(), output);
            return;
        }
        self.write_indent(indent, output);
        self.write_stmt_body(stmt_id, indent, output);
        self.write_trailing_comment(stmt.span.end(), output);
    }

    /// Writes a statement without its leading comments, indentation, or
    /// trailing comment.
    fn write_stmt_body(&mut self, stmt_id: StmtId, indent: usize, output: &mut String) {
        let stmt = self.arena.stmt(stmt_id);
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
                self.write_stmt_body(*inner, indent, output);
            }
            ArenaStmtKind::TypeDef(def) => self.write_type_def(*def, stmt.span, indent, output),
            ArenaStmtKind::ErrorDef(def) => self.write_error_def(*def, stmt.span, indent, output),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            } => {
                output.push_str(if matches!(stmt.kind, ArenaStmtKind::Const { .. }) {
                    "const "
                } else {
                    "let "
                });
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
                // An assigned value breaks like a `let` initializer. Written
                // flat, an over-long conditional or comprehension broke inside
                // a one-line block instead, and the next pass read that block
                // as author-broken and laid it out differently.
                self.write_expr_or_run_safe(value, output);
            }
            ArenaStmtKind::ProcDef(def) => self.write_function("proc", *def, indent, output),
            ArenaStmtKind::CliMain(def) => self.write_function("cli", *def, indent, output),
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
            ArenaStmtKind::YieldDelegate(value) => {
                output.push_str("yield @");
                self.write_expr(*value, END, output);
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
                self.write_expr(*condition, BRACE, output);
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
                self.write_expr(*iter, BRACE, output);
                output.push(' ');
                self.write_block(*block, indent, output);
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
            } => {
                let bindings = self.arena.with_bindings(*bindings).to_vec();
                let len = bindings.len();
                let body_start = self.arena.span(self.arena.block(*body).span).start();
                // Bindings the author wrote on the `with` line stay there
                // when they fit.
                let one_line = self
                    .source
                    .get(stmt.span.start()..body_start)
                    .is_some_and(|header| !header.contains('\n'));
                let flat = one_line
                    && !self.has_comment_in(stmt.span.start(), body_start)
                    && self.try_write_flat(output, |writer, line| {
                        line.push_str("with ");
                        for (index, binding) in bindings.iter().enumerate() {
                            if index > 0 {
                                line.push_str(", ");
                            }
                            line.push_str(binding.name.as_str().as_str());
                            line.push_str(" = ");
                            writer.write_expr(
                                binding.initializer,
                                if index + 1 < len { CLOSE } else { BRACE },
                                line,
                            );
                        }
                        line.push_str(" {");
                    });
                if flat {
                    output.pop();
                } else {
                    output.push_str("with\n");
                    for (index, binding) in bindings.iter().enumerate() {
                        self.write_indent(indent + 1, output);
                        output.push_str(binding.name.as_str().as_str());
                        output.push_str(" = ");
                        self.write_expr(
                            binding.initializer,
                            if index + 1 < len { CLOSE } else { END },
                            output,
                        );
                        if index + 1 < len {
                            output.push(',');
                        }
                        output.push('\n');
                    }
                    self.write_indent(indent, output);
                }
                self.write_block(*body, indent, output);
                output.push_str(" else ");
                self.write_block(*else_block, indent, output);
            }
            ArenaStmtKind::Loop { block } => {
                output.push_str("loop ");
                self.write_block(*block, indent, output);
            }
            ArenaStmtKind::Sugar { expansion, .. } if self.expand_sugar => {
                self.write_stmt_body(*expansion, indent, output);
            }
            ArenaStmtKind::Sugar { form, operands, .. } => {
                match self.arena.sugar(*form, *operands) {
                    ArenaSugar::Repeat { count, body } => {
                        output.push_str("repeat ");
                        self.write_expr(count, WORD, output);
                        output.push_str(" times ");
                        self.write_block(body, indent, output);
                    }
                    ArenaSugar::Guarded {
                        stmt: inner,
                        negate,
                        condition,
                    } => {
                        self.write_guarded_action(inner, indent, output);
                        if negate {
                            output.push_str(" unless ");
                        } else {
                            output.push_str(" when ");
                        }
                        self.write_expr(condition, END, output);
                    }
                    ArenaSugar::Guard {
                        condition,
                        else_block,
                    } => {
                        output.push_str("guard ");
                        self.write_expr(condition, WORD, output);
                        output.push_str(" else ");
                        self.write_block(else_block, indent, output);
                    }
                }
            }
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_block,
            } => {
                output.push_str("guard let ");
                self.write_binding_target(*target, output);
                self.write_optional_type(*ty, output);
                output.push_str(" = ");
                self.write_expr_or_run_safe(initializer, output);
                output.push_str(" else ");
                self.write_block(*else_block, indent, output);
            }
            ArenaStmtKind::Assert { condition, message } => {
                self.write_assert(*condition, *message, output)
            }
            ArenaStmtKind::Break { value } => {
                output.push_str("break");
                if let Some(expr) = value {
                    output.push(' ');
                    self.write_expr(*expr, END, output);
                }
            }
            ArenaStmtKind::Continue => output.push_str("continue"),
            ArenaStmtKind::Match { value, arms } => {
                self.write_match(*value, *arms, stmt.span, indent, output)
            }
            ArenaStmtKind::Command(command) => self.write_command_stmt(*command, indent, output),
            ArenaStmtKind::TailBareIdent(name) => output.push_str(name.as_str().as_str()),
            ArenaStmtKind::Expr(expr) => self.write_statement_expr(*expr, output),
        }
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
        span: Span,
        indent: usize,
        output: &mut String,
    ) {
        use xsh::frontend::syntax::arena::ArenaTypeDefBody;
        let def = self.arena.type_def(def_id).clone();
        if matches!(def.body, ArenaTypeDefBody::TagUnion(_))
            && self.comments[self.next_comment..]
                .iter()
                .any(|comment| span.range().contains(&comment.span.start()))
        {
            // Keep variant and payload comments at their authored positions.
            let raw = self
                .source
                .get(span.range())
                .unwrap_or("")
                .trim_end_matches(['\n', '\r']);
            output.push_str(
                raw.strip_prefix("export")
                    .map(str::trim_start)
                    .unwrap_or(raw),
            );
            while self
                .comments
                .get(self.next_comment)
                .is_some_and(|comment| comment.span.start() < span.end())
            {
                self.next_comment += 1;
            }
            return;
        }
        output.push_str(if matches!(def.body, ArenaTypeDefBody::TagUnion(_)) {
            "enum "
        } else {
            "type "
        });
        output.push_str(def.name.as_str().as_str());
        if !def.type_parameters.is_empty() {
            output.push('[');
            for (index, parameter) in self.arena.names(def.type_parameters).enumerate() {
                if index != 0 {
                    output.push_str(", ");
                }
                output.push_str(parameter.as_str().as_str());
            }
            output.push(']');
        }
        match &def.body {
            ArenaTypeDefBody::Alias(ty) => {
                output.push_str(" = ");
                self.write_type(*ty, output);
            }
            ArenaTypeDefBody::RecordSchema(fields) => {
                output.push_str(" = ");
                self.write_record_schema(*fields, output);
            }
            ArenaTypeDefBody::ModuleContract { entries, exact } => {
                output.push_str(if *exact { " = exact " } else { " = " });
                self.write_module_contract(*entries, output);
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                let variant_range = *variants;
                let variants = self.arena.tag_variants(variant_range).to_vec();
                if variants.iter().any(|variant| variant.wire_value.is_some()) {
                    output.push_str(": Str");
                }
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
                    if let Some(value) = v.wire_value {
                        part.push_str(" = ");
                        self.write_expr_safe(value, &mut part);
                    }
                    parts.push(part);
                }
                let use_multiline = variants.len() >= 5
                    || (variants.len() >= 3
                        && parts.iter().map(|p| p.len() + 3).sum::<usize>() > 60)
                    // A union the author already wrote across lines stays that
                    // way for three or more variants.
                    || (variants.len() >= 3
                        && tag_variants_original_multiline(self.arena, &self.source, variant_range));
                if use_multiline {
                    output.push_str(" {\n");
                    let variant_indent = " ".repeat(indent + 4);
                    for part in &parts {
                        output.push_str(&format!("{variant_indent}{part},\n"));
                    }
                    output.push_str(&" ".repeat(indent));
                    output.push('}');
                } else {
                    output.push_str(" { ");
                    output.push_str(&parts.join(", "));
                    output.push_str(" }");
                }
            }
        }
    }

    /// An error family in its `=` form while that fits the line, and in the
    /// brace form (one variant per line) once it does not or when the author
    /// wrote braces.
    fn write_error_def(
        &mut self,
        def_id: xsh::frontend::syntax::arena::ErrorDefId,
        span: Span,
        indent: usize,
        output: &mut String,
    ) {
        let def = self.arena.error_def(def_id).clone();
        let variants = self.arena.error_variants(def.variants).to_vec();
        let raw = self
            .source
            .get(span.range())
            .unwrap_or("")
            .trim_end_matches(['\n', '\r']);
        let authored_braces = raw
            .find(['=', '{'])
            .is_some_and(|at| raw[at..].starts_with('{'));
        let variants_end = variants
            .last()
            .map_or(span.start(), |variant| self.arena.span(variant.span).end());
        // Comments between variants, and in braces up to the closing `}`,
        // have no place in the rebuilt declaration.
        let body_end = if authored_braces {
            error_block_close(&self.source, variants_end, span.end())
        } else {
            variants_end
        };
        if self.comments[self.next_comment..].iter().any(|comment| {
            comment.span.start() >= span.start() && comment.span.start() < body_end
        }) {
            // Keep variant comments at their authored positions.
            output.push_str(
                raw.strip_prefix("export")
                    .map(str::trim_start)
                    .unwrap_or(raw),
            );
            while self
                .comments
                .get(self.next_comment)
                .is_some_and(|comment| comment.span.start() < span.end())
            {
                self.next_comment += 1;
            }
            return;
        }
        let mut parts = Vec::with_capacity(variants.len());
        for variant in &variants {
            let mut part = variant.name.as_str().to_string();
            if !variant.fields.is_empty() {
                part.push('(');
                let fields = self.arena.error_fields(variant.fields).to_vec();
                for (field_index, field) in fields.iter().enumerate() {
                    if field_index > 0 {
                        part.push_str(", ");
                    }
                    part.push_str(field.name.as_str().as_str());
                    part.push_str(": ");
                    self.write_type(field.ty, &mut part);
                }
                part.push(')');
            }
            if !variant.facets.is_empty() {
                part.push_str(" : ");
                part.push_str(&self.join_name_range(variant.facets, ", "));
            }
            parts.push(part);
        }
        output.push_str("error ");
        output.push_str(def.name.as_str().as_str());
        let inline = format!(" = {}", parts.join(" | "));
        if !authored_braces && self.fits_inline(output, &inline) {
            output.push_str(&inline);
            return;
        }
        output.push_str(" {\n");
        for part in &parts {
            self.write_indent(indent, output);
            output.push_str("    ");
            output.push_str(part);
            output.push('\n');
        }
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_function(
        &mut self,
        keyword: &str,
        def_id: FunctionDefId,
        indent: usize,
        output: &mut String,
    ) {
        let def = self.arena.function_def(def_id).clone();
        let body = def.body;
        if def.test_declaration {
            output.push_str("test ");
            output.push_str(def.name.as_str().as_str());
            if let Some(effects) = def.effects {
                output.push_str(" [");
                for (index, effect) in
                    canonical_effects(&self.arena.effects(effects).collect::<Vec<_>>())
                        .iter()
                        .enumerate()
                {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(effect.as_str());
                }
                output.push(']');
            }
            output.push(' ');
            self.write_block(body, indent, output);
            return;
        }
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
        if keyword != "yield @" {
            output.push(' ');
        }
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
        if keyword != "yield @" {
            output.push(' ');
        }
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
            self.write_expr(default, CLOSE, output);
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
        let blocks: Vec<BlockId> = branches
            .iter()
            .map(|branch| branch.block)
            .chain(else_block)
            .collect();
        let flat: Option<Vec<StmtId>> = blocks
            .iter()
            .map(|block| self.flat_block_statement(*block, false))
            .collect();
        let start = branches
            .first()
            .map_or(0, |branch| self.arena.expr(branch.condition).span.start());
        let end = blocks.last().map_or(0, |block| {
            self.arena.span(self.arena.block(*block).span).end()
        });
        // Every branch stays on one line, or every branch breaks.
        if let Some(flat) = flat
            && !self.has_comment_in(start, end)
            && self.try_write_flat(output, |writer, line| {
                writer.write_if_branches(&branches, else_block, line, |writer, index, line| {
                    writer.write_flat_block(blocks[index], flat[index], line)
                })
            })
        {
            return;
        }
        self.write_if_branches(&branches, else_block, output, |writer, index, output| {
            writer.write_broken_block(blocks[index], indent, output, false)
        });
    }

    fn write_if_branches(
        &mut self,
        branches: &[xsh::frontend::syntax::arena::ArenaIfBranch],
        else_block: Option<BlockId>,
        output: &mut String,
        mut write_block: impl FnMut(&mut Self, usize, &mut String),
    ) {
        for (index, branch) in branches.iter().enumerate() {
            output.push_str(if index == 0 { "if " } else { " else if " });
            self.write_expr(branch.condition, BRACE, output);
            output.push(' ');
            write_block(self, index, output);
        }
        if else_block.is_some() {
            output.push_str(" else ");
            write_block(self, branches.len(), output);
        }
    }

    fn write_match(
        &mut self,
        value: ExprId,
        arms: xsh::frontend::syntax::arena::ArenaRange,
        span: Span,
        indent: usize,
        output: &mut String,
    ) {
        output.push_str("match ");
        self.write_expr(value, BRACE, output);
        output.push_str(" {");
        let arms = self.arena.match_arms(arms).to_vec();
        // Comments before the closing brace stay inside the match.
        let close = self.text_end(span).saturating_sub(1);
        if arms.is_empty() {
            output.push('}');
            return;
        }
        output.push('\n');
        for (index, arm) in arms.iter().enumerate() {
            if index > 0 {
                output.push('\n');
            }
            let pattern_start = self
                .arena
                .span(self.arena.pattern(arm.pattern).span)
                .start();
            self.write_comments_before(pattern_start, indent + 1, output);
            self.write_indent(indent + 1, output);
            self.write_arm_pattern(arm.pattern, arm.spelling, output);
            if let Some(guard) = arm.guard {
                output.push_str(" if ");
                self.write_expr(guard, CLOSE, output);
            }
            output.push_str(" => ");
            let block = self.arena.block(arm.block);
            let stmts: Vec<StmtId> = self.arena.stmt_ids(block.statements).collect();
            if stmts.len() == 1 && block.params.is_empty() {
                let stmt_id = stmts[0];
                let stmt = self.arena.stmt(self.layout_stmt(stmt_id));
                let definition = matches!(
                    stmt.kind,
                    ArenaStmtKind::ProcDef(_)
                        | ArenaStmtKind::PureDef(_)
                        | ArenaStmtKind::StreamDef(_)
                );
                let control_flow = matches!(
                    stmt.kind,
                    ArenaStmtKind::If { .. }
                        | ArenaStmtKind::While { .. }
                        | ArenaStmtKind::For { .. }
                        | ArenaStmtKind::Match { .. }
                        | ArenaStmtKind::With { .. }
                ) || matches!(stmt.kind, ArenaStmtKind::Sugar { form, .. } if form.is_compound());
                let write_arm = |writer: &mut Self, line: &mut String| {
                    // An initializer's nested statements have ordinary block
                    // syntax; only the arm's own expression needs arm grouping.
                    writer.arm_statement = matches!(stmt.kind, ArenaStmtKind::Expr(_));
                    writer.write_stmt_body(stmt_id, indent + 1, line);
                    writer.arm_statement = false;
                };
                // A control-flow arm statement goes unbraced only when it
                // stays on the arm's line.
                let written = if control_flow {
                    !self.has_comment_in(stmt.span.start(), self.text_end(stmt.span))
                        && self.try_write_flat(output, write_arm)
                } else if !definition {
                    write_arm(self, output);
                    true
                } else {
                    false
                };
                if written {
                    self.write_trailing_comment(stmt.span.end(), output);
                    continue;
                }
            }
            self.write_block(arm.block, indent + 1, output);
        }
        output.push('\n');
        self.write_closing_comments(close, indent + 1, output);
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_assert(&mut self, condition: ExprId, message: Option<ExprId>, output: &mut String) {
        output.push_str("assert ");
        self.write_expr(
            condition,
            if message.is_some() { CLOSE } else { END },
            output,
        );
        if let Some(message) = message {
            output.push_str(", ");
            self.write_expr(message, END, output);
        }
    }

    /// A match arm's pattern as its author spelled it: the catch-all stays
    /// `else` or `_`, whichever was written.
    fn write_arm_pattern(
        &mut self,
        pattern_id: PatternId,
        spelling: xsh::frontend::syntax::arena::ArenaArmSpelling,
        output: &mut String,
    ) {
        match spelling {
            xsh::frontend::syntax::arena::ArenaArmSpelling::Else => output.push_str("else"),
            xsh::frontend::syntax::arena::ArenaArmSpelling::Pattern => {
                self.write_pattern(pattern_id, output)
            }
        }
    }

    fn write_pattern(&mut self, pattern_id: PatternId, output: &mut String) {
        let span = self.arena.span(self.arena.pattern(pattern_id).span);
        if self.comments[self.next_comment..]
            .iter()
            .any(|comment| span.range().contains(&comment.span.start()))
        {
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
            return;
        }
        let kind = self.arena.pattern(pattern_id).kind.clone();
        match &kind {
            ArenaPatternKind::Group(child) => {
                output.push('(');
                self.write_pattern(*child, output);
                output.push(')');
            }
            ArenaPatternKind::Alias { pattern, name, .. } => {
                let grouped = matches!(
                    self.arena.pattern(*pattern).kind,
                    ArenaPatternKind::Alternation(_)
                );
                if grouped {
                    output.push('(');
                }
                self.write_pattern(*pattern, output);
                if grouped {
                    output.push(')');
                }
                output.push_str(" as ");
                output.push_str(&name.as_str());
            }
            ArenaPatternKind::Wildcard => output.push('_'),
            ArenaPatternKind::Binding(name) | ArenaPatternKind::TestName { name, .. } => {
                output.push_str(name.as_str().as_str())
            }
            ArenaPatternKind::Type { binding, ty } => {
                if let Some(binding) = binding {
                    output.push_str(binding.as_str().as_str());
                } else {
                    output.push('_');
                }
                output.push_str(" is ");
                self.write_type(*ty, output);
            }
            ArenaPatternKind::Literal(expr) => self.write_expr(*expr, CLOSE, output),
            ArenaPatternKind::List { elements, rest } => {
                output.push('[');
                let elements: Vec<_> = self.arena.pattern_ids(*elements).collect();
                for (index, element) in elements.iter().enumerate() {
                    if index != 0 {
                        output.push_str(", ");
                    }
                    self.write_pattern(*element, output);
                }
                if let Some(rest) = rest {
                    if !elements.is_empty() {
                        output.push_str(", ");
                    }
                    output.push_str("..");
                    if !matches!(self.arena.pattern(*rest).kind, ArenaPatternKind::Wildcard) {
                        self.write_pattern(*rest, output);
                    }
                }
                output.push(']');
            }
            ArenaPatternKind::Record { fields, rest } => {
                output.push('{');
                let fields = self.arena.pattern_fields(*fields).to_vec();
                let len = fields.len();
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    self.write_pattern_field(field, output);
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
                let fields = self.arena.pattern_fields(*fields).to_vec();
                // `Family.Variant` and `Family.Variant {}` are one pattern;
                // keep the author's spelling instead of adding braces.
                if fields.is_empty()
                    && !self
                        .source
                        .get(self.arena.span(self.arena.pattern(pattern_id).span).range())
                        .unwrap_or("")
                        .trim_end()
                        .ends_with('}')
                {
                    return;
                }
                output.push_str(" {");
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    self.write_pattern_field(field, output);
                }
                // Error variant patterns always ignore unnamed fields, so the
                // tree omits `..`; keep the author's spelling of it.
                let source = self
                    .source
                    .get(self.arena.span(self.arena.pattern(pattern_id).span).range())
                    .unwrap_or("");
                if source
                    .trim_end()
                    .strip_suffix('}')
                    .is_some_and(|fields| fields.trim_end().ends_with(".."))
                {
                    output.push_str(if fields.is_empty() { ".." } else { ", .." });
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

    /// Writes a record or error-variant pattern field. A `{name}` shorthand
    /// stays shorthand and `{name: name}` stays spelled out.
    fn write_pattern_field(
        &mut self,
        field: &xsh::frontend::syntax::arena::ArenaRecordPatternField,
        output: &mut String,
    ) {
        output.push_str(field.name.as_str().as_str());
        let shorthand = matches!(self.arena.pattern(field.pattern).kind, ArenaPatternKind::Binding(name) if name == field.name)
            && self
                .source
                .get(self.arena.span(field.span).range())
                .is_some_and(|text| !text.contains(':'));
        if !shorthand {
            output.push_str(": ");
            self.write_pattern(field.pattern, output);
        }
    }

    fn write_binding_target(&mut self, target_id: BindingTargetId, output: &mut String) {
        if let Some(span) = self.arena.binding_target(target_id).span {
            let span = self.arena.span(span);
            if self.comments[self.next_comment..]
                .iter()
                .any(|comment| span.range().contains(&comment.span.start()))
            {
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
                return;
            }
        }
        let kind = self.arena.binding_target(target_id).kind.clone();
        match &kind {
            ArenaBindingTargetKind::Name(name) => output.push_str(&self.name_text(*name)),
            ArenaBindingTargetKind::Record { fields, rest } => {
                output.push('{');
                let fields = self.arena.destructure_fields(*fields).to_vec();
                let len = fields.len();
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(field.name.as_str().as_str());
                    if !matches!(self.arena.binding_target(field.target).kind, ArenaBindingTargetKind::Name(name) if name == field.name)
                    {
                        output.push_str(": ");
                        self.write_binding_target(field.target, output);
                    }
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
            ArenaAssignTargetKind::Name(name) => output.push_str(&self.name_text(*name)),
            ArenaAssignTargetKind::Env(name) => write_env_string(*name, output),
            ArenaAssignTargetKind::Field { base, name } => {
                self.write_assign_target(*base, output);
                output.push('.');
                output.push_str(name.as_str().as_str());
            }
            ArenaAssignTargetKind::Index { base, index } => {
                self.write_assign_target(*base, output);
                output.push('[');
                self.write_expr(*index, CLOSE, output);
                output.push(']');
            }
        }
    }

    fn write_block(&mut self, block_id: BlockId, indent: usize, output: &mut String) {
        self.write_block_contents(block_id, indent, output, false);
    }

    fn write_block_contents(
        &mut self,
        block_id: BlockId,
        indent: usize,
        output: &mut String,
        preserve_value_shape: bool,
    ) {
        if let Some(stmt) = self.flat_block_statement(block_id, preserve_value_shape)
            && self.try_write_flat(output, |writer, line| {
                writer.write_flat_block(block_id, stmt, line)
            })
        {
            return;
        }
        self.write_broken_block(block_id, indent, output, preserve_value_shape);
    }

    /// The single statement of a block the author wrote on one line with no
    /// comment inside, which may stay on one line: `{ return 1 }`,
    /// `{ |x| x + 1 }`.
    fn flat_block_statement(
        &self,
        block_id: BlockId,
        preserve_value_shape: bool,
    ) -> Option<StmtId> {
        let block = self.arena.block(block_id);
        let span = self.arena.span(block.span);
        let [stmt] = self.arena.stmt_ids(block.statements).collect::<Vec<_>>()[..] else {
            return None;
        };
        // A value block's lone bare name prints as `(name)`, which only the
        // broken shape keeps apart from a record shorthand.
        let grouped_name = preserve_value_shape
            && block.params.is_empty()
            && matches!(self.arena.stmt(stmt).kind, ArenaStmtKind::TailBareIdent(_));
        let one_line = self
            .source
            .get(span.range())
            .is_some_and(|text| !text.contains('\n'));
        (one_line && !grouped_name && !self.has_comment_in(span.start(), span.end()))
            .then_some(stmt)
    }

    fn has_comment_in(&self, start: usize, end: usize) -> bool {
        self.comments[self.next_comment..]
            .iter()
            .any(|comment| (start..end).contains(&comment.span.start()))
    }

    fn write_block_params(&self, block_id: BlockId, output: &mut String) {
        let params = self
            .arena
            .block_params(self.arena.block(block_id).params)
            .to_vec();
        if params.is_empty() {
            return;
        }
        output.push_str(" |");
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push_str(param.name.as_str().as_str());
        }
        output.push('|');
    }

    fn write_flat_block(&mut self, block_id: BlockId, stmt: StmtId, output: &mut String) {
        output.push('{');
        self.write_block_params(block_id, output);
        output.push(' ');
        self.after_expression = false;
        self.write_stmt(stmt, 0, output);
        output.push_str(" }");
    }

    /// Writes with `write` onto a copy of the current line and keeps the
    /// result only when it stays on that line within the width. The region
    /// written must hold no comments; they are hidden while writing so a
    /// statement's trailing comment cannot be pulled inside the line.
    fn try_write_flat(
        &mut self,
        output: &mut String,
        write: impl FnOnce(&mut Self, &mut String),
    ) -> bool {
        self.try_write_on_line(output, false, write)
    }

    /// Like `try_write_flat`, but with `allow_break` the result may continue
    /// onto further lines as long as its first line fits.
    fn try_write_on_line(
        &mut self,
        output: &mut String,
        allow_break: bool,
        write: impl FnOnce(&mut Self, &mut String),
    ) -> bool {
        let line_start = output.rfind('\n').map_or(0, |index| index + 1);
        let mut line = output[line_start..].to_string();
        let prefix = line.len();
        let next_comment = self.next_comment;
        self.next_comment = self.comments.len();
        write(self, &mut line);
        self.next_comment = next_comment;
        let first_line = line.split('\n').next().unwrap_or_default();
        if (!allow_break && line.contains('\n')) || first_line.chars().count() > self.line_width {
            return false;
        }
        output.push_str(&line[prefix..]);
        true
    }

    fn write_broken_block(
        &mut self,
        block_id: BlockId,
        indent: usize,
        output: &mut String,
        preserve_value_shape: bool,
    ) {
        let block = self.arena.block(block_id);
        let params = self.arena.block_params(block.params).to_vec();
        let stmts: Vec<StmtId> = self.arena.stmt_ids(block.statements).collect();
        // Comments before the closing brace stay inside the block.
        let close = self.arena.span(block.span).end().saturating_sub(1);
        output.push('{');
        self.write_block_params(block_id, output);
        if stmts.is_empty() && !self.has_comment_before(close) {
            if !params.is_empty() {
                output.push(' ');
            }
            output.push('}');
            return;
        }
        output.push('\n');
        let mut previous_span: Option<Span> = None;
        let mut previous_multiline_control_flow = false;
        for (index, stmt_id) in stmts.iter().enumerate() {
            let stmt = self.arena.stmt(self.layout_stmt(*stmt_id));
            let stmt_span = stmt.span;
            if let Some(previous) = previous_span {
                output.push('\n');
                if previous_multiline_control_flow
                    || self.authored_blank_before(previous, stmt_span.start())
                {
                    output.push('\n');
                }
            }
            let stmt_output_start = output.len();
            let grouped = preserve_value_shape && index == 0 && params.is_empty();
            self.after_expression = index > 0
                && grouping::statement_may_continue(
                    &self.arena.stmt(self.layout_stmt(stmts[index - 1])).kind,
                );
            match stmt.kind {
                ArenaStmtKind::TailBareIdent(name) if grouped => {
                    self.write_comments_before(stmt_span.start(), indent + 1, output);
                    self.write_indent(indent + 1, output);
                    output.push('(');
                    output.push_str(name.as_str().as_str());
                    output.push(')');
                    self.write_raw_trailing_comment(stmt_span.end(), output);
                }
                _ => self.write_stmt(*stmt_id, indent + 1, output),
            }
            previous_span = Some(stmt_span);
            previous_multiline_control_flow = (matches!(
                stmt.kind,
                ArenaStmtKind::If { .. }
                    | ArenaStmtKind::While { .. }
                    | ArenaStmtKind::For { .. }
                    | ArenaStmtKind::With { .. }
                    | ArenaStmtKind::Loop { .. }
                    | ArenaStmtKind::Match { .. }
            ) || matches!(
                stmt.kind,
                // A `guard` reads as a precondition of what follows it.
                ArenaStmtKind::Sugar { form, .. } if form.is_compound() && form != SugarForm::Guard
            )) && output[stmt_output_start..].contains('\n');
        }
        if self.has_comment_before(close) {
            if let Some(previous) = previous_span {
                output.push('\n');
                if self.authored_blank_before(previous, close) {
                    output.push('\n');
                }
            }
            self.write_closing_comments(close, indent + 1, output);
        } else {
            output.push('\n');
        }
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
        // The `?` or `{` that follows the last part on its line.
        let tail = 2 * usize::from(stmt.propagate);
        let continuation = indent_for_expr(output) + 1;
        match &stmt.command {
            ArenaCommand::Proc { name, args } => {
                output.push_str(name.as_str().as_str());
                self.write_command_args(*args, continuation, tail, output);
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
                    let tail = if block.is_some() { 2 } else { tail };
                    // A continuation line under a command with a block is
                    // indented twice, so it does not read as the block's
                    // first statement.
                    let continuation = continuation + usize::from(block.is_some());
                    let mut parts: Vec<CommandPart> = self
                        .arena
                        .command_args(*args)
                        .iter()
                        .cloned()
                        .map(CommandPart::Arg)
                        .collect();
                    parts.extend(env_assignments.iter().cloned().map(CommandPart::Env));
                    self.write_command_parts(&parts, true, continuation, tail, output);
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
            ArenaExprOrRun::Expr(expr) => {
                self.write_expr(*expr, Context::initializer(Follow::END), output)
            }
            ArenaExprOrRun::Run(run) => self.write_run(*run, output),
        }
    }

    fn write_guarded_action(&mut self, stmt: StmtId, indent: usize, output: &mut String) {
        let (keyword, value) = match self.arena.stmt(stmt).kind {
            ArenaStmtKind::Return(Some(value)) => ("return", value),
            ArenaStmtKind::YieldDelegate(value) => ("yield @", ArenaExprOrRun::Expr(value)),
            ArenaStmtKind::Yield(value) => ("yield", value),
            ArenaStmtKind::Break { value: Some(value) } => ("break", ArenaExprOrRun::Expr(value)),
            _ => {
                self.write_stmt_body(stmt, indent, output);
                return;
            }
        };
        output.push_str(keyword);
        if keyword != "yield @" {
            output.push(' ');
        }
        match value {
            // Group the run form so command argv cannot consume the postfix guard.
            ArenaExprOrRun::Run(run) => {
                output.push('(');
                self.write_run(run, output);
                output.push(')');
            }
            ArenaExprOrRun::Expr(expr) => {
                self.write_expr_safe_in(expr, Context::initializer(Follow::WORD), output)
            }
        }
    }

    fn write_expr_or_run_safe(&mut self, value: &ArenaExprOrRun, output: &mut String) {
        match value {
            ArenaExprOrRun::Expr(expr) => {
                self.write_expr_safe_in(*expr, Context::initializer(Follow::END), output)
            }
            ArenaExprOrRun::Run(run) => self.write_run(*run, output),
        }
    }

    /// Writes an expression statement, or a single-expression block or arm
    /// body, where the parser dispatches on the leading tokens.
    fn write_statement_expr(&mut self, expr: ExprId, output: &mut String) {
        let after_expression = std::mem::take(&mut self.after_expression);
        let context = if std::mem::take(&mut self.arm_statement) {
            Context::arm_statement()
        } else {
            Context::statement(Follow::END, after_expression)
        };
        self.write_expr(expr, context, output);
    }

    fn write_run(&mut self, run_id: xsh::frontend::syntax::arena::RunFormId, output: &mut String) {
        let run = self.arena.run_form(run_id).clone();
        let indent = indent_for_expr(output);
        let segments: Vec<xsh::frontend::syntax::arena::ArenaRunSegment> =
            self.arena.run_segments(run.segments).to_vec();
        for (index, segment) in segments.iter().enumerate() {
            let tail = if index + 1 < segments.len() || run.propagate {
                2
            } else {
                0
            };
            self.write_run_segment(segment, index > 0, indent, tail, output);
        }
        if run.propagate {
            output.push_str(" ?");
        }
    }

    /// Writes one segment of a run form. `piped` segments follow a `|`, which
    /// begins a continuation line when the segment does. `tail` is the width
    /// that follows the segment's last part on its line: ` |` or ` ?`.
    fn write_run_segment(
        &mut self,
        segment: &xsh::frontend::syntax::arena::ArenaRunSegment,
        piped: bool,
        indent: usize,
        tail: usize,
        output: &mut String,
    ) {
        let head = run_head_text(segment.kind);
        let args: Vec<ArenaCommandArg> = self.arena.command_args(segment.args).to_vec();
        let redirections = self.arena.redirections(segment.redirections).to_vec();
        if segment.grouped {
            if piped {
                output.push_str(" | ");
            }
            output.push_str(head);
            output.push(' ');
            for (name, value) in [
                ("timeout", segment.timeout),
                ("cpumax", segment.cpu_max),
                ("accept", segment.accept),
            ] {
                if let Some(value) = value {
                    self.write_command_part_text(&CommandPart::Option { name, value }, output);
                    output.push(' ');
                }
            }
            for assignment in self.arena.env_assignments(segment.env).to_vec() {
                self.write_env_assignment(&assignment, output);
                output.push(' ');
            }
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
        let mut parts = Vec::new();
        for (name, value) in [
            ("timeout", segment.timeout),
            ("cpumax", segment.cpu_max),
            ("accept", segment.accept),
        ] {
            if let Some(value) = value {
                parts.push(CommandPart::Option { name, value });
            }
        }
        parts.extend(
            self.arena
                .env_assignments(segment.env)
                .iter()
                .cloned()
                .map(CommandPart::Env),
        );
        parts.push(CommandPart::Arg(segment.target.clone()));
        parts.extend(args.into_iter().map(CommandPart::Arg));
        parts.extend(redirections.into_iter().map(CommandPart::Redirection));
        // The run form's name and its first part are one unit: a line break
        // never separates them.
        let first = parts.remove(0);
        let first_tail = if parts.is_empty() { tail } else { 2 };
        if piped {
            let begins_line = self.piped_segment_begins_line(segment);
            self.write_continued_command_part(begins_line, indent + 1, first_tail, output, |writer, output| {
                output.push_str("| ");
                output.push_str(head);
                output.push(' ');
                writer.write_command_part_text(&first, output);
            });
        } else {
            output.push_str(head);
            output.push(' ');
            self.write_command_part_text(&first, output);
        }
        self.write_command_parts(&parts, false, indent + 1, tail, output);
    }

    /// Writes the parts of a command after its first word. A part goes on
    /// the current line after a space, or begins a continuation line after
    /// ` \`, indented one level under the command, when the author began a
    /// line with it or when it does not fit. The first part of a command
    /// stays on the command's line, so `glue_first` is set unless the caller
    /// has already written it. `tail` is the width that follows the last
    /// part on its line.
    fn write_command_parts(
        &mut self,
        parts: &[CommandPart],
        glue_first: bool,
        indent: usize,
        tail: usize,
        output: &mut String,
    ) {
        for (index, part) in parts.iter().enumerate() {
            if index == 0 && glue_first {
                output.push(' ');
                self.write_command_part_text(part, output);
                continue;
            }
            // A part that is not the last needs room for the ` \` that may
            // follow it.
            let tail = if index + 1 == parts.len() { tail } else { 2 };
            let begins_line = self.command_part_begins_line(part);
            self.write_continued_command_part(begins_line, indent, tail, output, |writer, output| {
                writer.write_command_part_text(part, output)
            });
        }
    }

    fn write_continued_command_part(
        &mut self,
        begins_line: bool,
        indent: usize,
        tail: usize,
        output: &mut String,
        write: impl Fn(&mut Self, &mut String),
    ) {
        let start = output.len();
        let next_comment = self.next_comment;
        if !begins_line {
            output.push(' ');
            write(self, output);
            // Width never breaks an inline-only rendering, and a part that
            // already begins its line cannot move further.
            let first_line = output[start..].split('\n').next().unwrap_or_default();
            let part_width = first_line.chars().count() - 1;
            let width = current_line_width(&output[..start]) + 1 + part_width;
            // A part too wide for a line of its own overflows wherever it
            // is written, so it stays where the author put it.
            if self.inline_only
                || width + tail <= self.line_width
                || indent * 2 + part_width + tail > self.line_width
            {
                return;
            }
            output.truncate(start);
            self.next_comment = next_comment;
        }
        output.push_str(" \\\n");
        self.write_indent(indent, output);
        write(self, output);
    }

    fn write_command_part_text(&mut self, part: &CommandPart, output: &mut String) {
        match part {
            CommandPart::Option { name, value } => {
                output.push_str("--");
                output.push_str(name);
                output.push('=');
                self.write_expr(*value, CLOSE, output);
            }
            CommandPart::Env(assignment) => self.write_env_assignment(assignment, output),
            CommandPart::Arg(arg) => self.write_command_arg(arg, output),
            CommandPart::Redirection(redirection) => self.write_redirection(redirection, output),
        }
    }

    /// Whether the author began a line with `part`, after a line that ends
    /// with a `\` continuation.
    fn command_part_begins_line(&self, part: &CommandPart) -> bool {
        let (start, lead) = match part {
            CommandPart::Option { value, .. } => (self.arena.expr(*value).span.start(), "="),
            CommandPart::Env(assignment) => (self.arena.span(assignment.span).start(), ""),
            CommandPart::Arg(arg) => (self.arena.span(arg.span).start(), ""),
            CommandPart::Redirection(redirection) => {
                (self.arena.span(redirection.span).start(), "")
            }
        };
        let Some(before) = self.source.get(..start) else {
            return false;
        };
        // An option's span starts at its value, after `--name=`.
        let before = if lead.is_empty() {
            before
        } else {
            let Some(before) = before.strip_suffix(lead) else {
                return false;
            };
            let name = before.trim_end_matches(|ch: char| ch.is_ascii_alphanumeric());
            let Some(before) = name.strip_suffix("--") else {
                return false;
            };
            before
        };
        follows_line_continuation(before)
    }

    /// Whether the author began a line with the `|` before `segment`.
    fn piped_segment_begins_line(
        &self,
        segment: &xsh::frontend::syntax::arena::ArenaRunSegment,
    ) -> bool {
        self.source
            .get(..self.arena.span(segment.span).start())
            .and_then(|before| before.trim_end_matches([' ', '\t']).strip_suffix('|'))
            .is_some_and(follows_line_continuation)
    }

    fn write_env_assignment(&mut self, assignment: &ArenaEnvAssignment, output: &mut String) {
        output.push_str(assignment.name.as_str().as_str());
        output.push('=');
        match &assignment.value {
            ArenaEnvAssignmentValue::CommandArg(arg) => self.write_command_arg(arg, output),
            ArenaEnvAssignmentValue::Expr(expr) => self.write_expr(*expr, CLOSE, output),
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
                ArenaEnvAssignmentValue::Expr(expr) => self.write_expr(*expr, CLOSE, output),
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
            ArenaRedirectionTarget::Path(arg) if matches!(arg.kind, ArenaCommandArgKind::Typed(expr) if matches!(self.arena.expr(expr).kind, ArenaExprKind::Bytes(_))) => {
                if let ArenaCommandArgKind::Typed(expr) = arg.kind {
                    self.write_expr(expr, CLOSE, output);
                }
            }
            ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => {
                self.write_command_arg(arg, output);
            }
        }
    }

    fn write_command_args(
        &mut self,
        args: xsh::frontend::syntax::arena::ArenaRange,
        indent: usize,
        tail: usize,
        output: &mut String,
    ) {
        let parts: Vec<CommandPart> = self
            .arena
            .command_args(args)
            .iter()
            .cloned()
            .map(CommandPart::Arg)
            .collect();
        self.write_command_parts(&parts, true, indent, tail, output);
    }

    fn write_command_arg(&mut self, arg: &ArenaCommandArg, output: &mut String) {
        match &arg.kind {
            ArenaCommandArgKind::Word(parts) => {
                let parts: Vec<ArenaWordPart> = self.arena.word_parts(*parts).collect();
                let source = self
                    .source
                    .get(self.arena.span(arg.span).range())
                    .unwrap_or_default();
                // A word the author wrote as one quoted string with
                // interpolations, `"-j${jobs}"`, stays one quoted string.
                if parts.len() > 1
                    && !parts
                        .iter()
                        .any(|part| matches!(part, ArenaWordPart::Bare(_)))
                    && is_one_quoted_string(source)
                {
                    output.push('"');
                    for part in &parts {
                        match part {
                            ArenaWordPart::Quoted(text) => {
                                let mut quoted = String::new();
                                write_command_quoted(self.text_value(text), &mut quoted);
                                output.push_str(&quoted[1..quoted.len() - 1]);
                            }
                            ArenaWordPart::Shorthand(expr) => {
                                escape_trailing_dollar(output);
                                output.push('$');
                                self.write_expr(*expr, CLOSE, output);
                            }
                            ArenaWordPart::Interpolation(expr) => {
                                escape_trailing_dollar(output);
                                output.push_str("${");
                                self.write_expr(*expr, END, output);
                                output.push('}');
                            }
                            ArenaWordPart::Bare(_) => unreachable!("checked above"),
                        }
                    }
                    output.push('"');
                    return;
                }
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
                            self.write_expr(*expr, CLOSE, output);
                        }
                        ArenaWordPart::Interpolation(expr) => {
                            output.push_str("${");
                            self.write_expr(*expr, END, output);
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
                self.write_expr(*expr, CLOSE, output);
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                output.push_str("@(");
                self.write_expr(*expr, CLOSE, output);
                output.push(')');
            }
            ArenaCommandArgKind::Typed(expr) => {
                let kind = self.arena.expr(*expr).kind;
                if matches!(kind, ArenaExprKind::PathStr(_) | ArenaExprKind::GlobStr(_))
                    || self.command_typed_arg_can_be_bare(*expr)
                {
                    self.write_expr(*expr, CLOSE, output);
                } else {
                    output.push('(');
                    self.write_expr(*expr, CLOSE, output);
                    output.push(')');
                }
            }
        }
    }

    fn write_expr(&mut self, expr_id: ExprId, context: Context, output: &mut String) {
        let parens = grouping::needs_parens(self.arena, &self.source, expr_id, context);
        let context = if parens { context.group() } else { context };
        if parens {
            output.push('(');
        }
        self.write_expr_contents(expr_id, context, output);
        if parens {
            output.push(')');
        }
    }

    fn write_expr_contents(&mut self, expr_id: ExprId, context: Context, output: &mut String) {
        if self.write_expr_with_internal_comment(expr_id, output)
            || self.try_write_stable_call_chain(expr_id, context, output)
        {
            return;
        }
        let kind = self.arena.expr(expr_id).kind;
        let arena = self.arena;
        let child = |child: ExprId| grouping::child_context(arena, expr_id, context, child);
        match &kind {
            ArenaExprKind::Null => output.push_str("null"),
            ArenaExprKind::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            ArenaExprKind::Int(value) => self.arena.int_literal(*value).write(output),
            ArenaExprKind::Float(value) => self.arena.float_literal(*value).write(output),
            ArenaExprKind::Duration(value) => self.arena.duration_literal(*value).write(output),
            ArenaExprKind::Str(value) => {
                let span = self.arena.expr(expr_id).span;
                // Block and raw strings keep their source; a `"..."` string
                // stays one, with its line breaks escaped.
                if let Some(original) = original_preserved_string_literal(&self.source, span) {
                    output.push_str(original);
                } else {
                    write_quoted(self.arena.string_literal(*value), output);
                }
            }
            ArenaExprKind::PathStr(value) => {
                let value = self.arena.string_literal(*value);
                // A bare path takes in an adjacent suffix.
                if let Some(path) = bare_path_literal_text(value).filter(|_| {
                    !context.follow.adjacent
                        && !matches!(context.slot, grouping::Slot::CommandTarget { .. })
                }) {
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
            ArenaExprKind::Regex(value) => {
                let literal = self.arena.regex_literal(*value);
                output.push_str(&literal.source_text);
            }
            ArenaExprKind::Bytes(value) => write_bytes(self.arena.bytes_literal(*value), output),
            ArenaExprKind::Ident(name) => output.push_str(&self.name_text(*name)),
            ArenaExprKind::Item => output.push('.'),
            ArenaExprKind::LastStatus => output.push_str("$?"),
            ArenaExprKind::List(items) => self.write_list(expr_id, *items, output),
            ArenaExprKind::ListComp { expr, qualifiers } => {
                output.push('[');
                self.write_expr(*expr, child(*expr), output);
                self.write_comp_qualifiers(*qualifiers, None, output);
                output.push(']');
            }
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => {
                output.push('{');
                self.write_map_comp_key(*key, output);
                output.push_str(": ");
                self.write_expr(*value, child(*value), output);
                self.write_comp_qualifiers(*qualifiers, None, output);
                output.push('}');
            }
            ArenaExprKind::Record(fields) => self.write_record(expr_id, *fields, output),
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                // A branch written `{ value }` on one line must not break
                // inside: the next pass would read that block as broken by
                // the author and lay it out differently. Every branch breaks
                // instead, the same layout that pass then keeps.
                let start = output.len();
                let next_comment = self.next_comment;
                let after_expression = self.after_expression;
                let arm_statement = self.arm_statement;
                if self.write_if_expr(*branches, *else_value, output) && !self.inline_only {
                    output.truncate(start);
                    self.next_comment = next_comment;
                    self.after_expression = after_expression;
                    self.arm_statement = arm_statement;
                    self.write_if_expr_multiline(*branches, *else_value, output);
                }
            }
            ArenaExprKind::Match { value, arms } => self.write_match_expr(*value, *arms, output),
            ArenaExprKind::PatternCondition { value, arms } => {
                output.push_str("let ");
                self.write_pattern(self.arena.match_expr_arms(*arms)[0].pattern, output);
                output.push_str(" = ");
                self.write_expr(*value, child(*value), output);
            }
            ArenaExprKind::PatternTest { value, arms } => {
                self.write_expr(*value, child(*value), output);
                output.push_str(" is ");
                let pattern = self.arena.match_expr_arms(*arms)[0].pattern;
                if let ArenaPatternKind::Type { binding: None, ty } =
                    self.arena.pattern(pattern).kind
                    && !matches!(
                        self.arena.type_expr_tags[ty.index()],
                        ArenaTypeExprTag::Named | ArenaTypeExprTag::Qualified
                    )
                {
                    self.write_type(ty, output);
                } else {
                    self.write_pattern(pattern, output);
                }
            }
            ArenaExprKind::Unary { op, expr } => {
                let operator = match op {
                    UnaryOp::Not => "! ",
                    UnaryOp::Neg => "-",
                };
                output.push_str(operator);
                let start = output.len();
                self.write_expr(*expr, child(*expr), output);
                if !tokens_stay_separate(
                    operator,
                    lex_spellings(&output[start..])
                        .first()
                        .map_or("", |(_, text)| *text),
                ) {
                    output.insert(start, ' ');
                }
            }
            ArenaExprKind::ComparisonChain(pairs) => {
                for (index, pair) in self.arena.expr_ids(*pairs).enumerate() {
                    let ArenaExprKind::Binary { op, left, right } = self.arena.expr(pair).kind
                    else {
                        unreachable!()
                    };
                    let pair_context = child(pair);
                    if index == 0 {
                        self.write_expr(
                            left,
                            grouping::child_context(arena, pair, pair_context, left),
                            output,
                        );
                    }
                    output.push(' ');
                    output.push_str(binary_op_text(op));
                    output.push(' ');
                    self.write_expr(
                        right,
                        grouping::child_context(arena, pair, pair_context, right),
                        output,
                    );
                }
            }
            ArenaExprKind::Binary { op, left, right } => {
                let continuation = continuation_indent_for_expr(output);
                self.write_expr(*left, child(*left), output);
                // A statement whose first line is a bare name would read as a
                // command, so its first operator stays on that line.
                let command_like = matches!(
                    context.lead,
                    Some(grouping::Lead::Statement { .. } | grouping::Lead::ArmStatement)
                ) && self.is_name_path(*left);
                if !command_like && self.authored_operator_break(*op, *left, *right) {
                    output.push('\n');
                    self.write_indent(continuation, output);
                } else {
                    output.push(' ');
                }
                output.push_str(binary_op_text(*op));
                output.push(' ');
                self.write_expr(*right, child(*right), output);
            }
            ArenaExprKind::Call { callee, args } => {
                self.write_expr(*callee, child(*callee), output);
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
                    let start = output.len();
                    self.write_expr(*base, child(*base), output);
                    push_joined(output, start, ".");
                }
                output.push_str(name.as_str().as_str());
            }
            ArenaExprKind::NullSafeField { base, name } => {
                let start = output.len();
                self.write_expr(*base, child(*base), output);
                push_joined(output, start, "?.");
                output.push_str(name.as_str().as_str());
            }
            ArenaExprKind::Index {
                base,
                index,
                guarded,
            } => {
                let start = output.len();
                self.write_expr(*base, child(*base), output);
                push_joined(output, start, if *guarded { "?[" } else { "[" });
                self.write_expr(*index, child(*index), output);
                output.push(']');
            }
            ArenaExprKind::Slice {
                base,
                start,
                end,
                guarded,
            } => {
                let base_start = output.len();
                self.write_expr(*base, child(*base), output);
                push_joined(output, base_start, if *guarded { "?[" } else { "[" });
                let start_at = output.len();
                if let Some(start) = start {
                    self.write_expr(*start, child(*start), output);
                }
                push_joined(output, start_at, "..");
                if let Some(end) = end {
                    self.write_expr(*end, child(*end), output);
                }
                output.push(']');
            }
            ArenaExprKind::EnvString(name) => write_env_string(*name, output),
            ArenaExprKind::EnvPathList => output.push_str("env.PATH"),
            ArenaExprKind::Pipeline { input, stages } => {
                self.write_expr(*input, child(*input), output);
                let indent = continuation_indent_for_expr(output);
                self.write_pipe_stages(self.arena.expr(*input).span.end(), *stages, indent, output);
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.write_expr(*input, child(*input), output);
                let indent = continuation_indent_for_expr(output);
                self.write_stream_stages(
                    self.arena.expr(*input).span.end(),
                    *stages,
                    indent,
                    output,
                );
            }
            ArenaExprKind::Run(run) => self.write_run(*run, output),
            ArenaExprKind::Spawn(form) => {
                output.push_str("spawn ");
                match &form.target {
                    ArenaSpawnTarget::Run(run) => self.write_run(*run, output),
                    ArenaSpawnTarget::Command(target) => {
                        self.write_expr(*target, child(*target), output)
                    }
                }
            }
            ArenaExprKind::Wait(form) => {
                output.push_str("wait ");
                self.write_expr(form.target, child(form.target), output);
            }
            ArenaExprKind::BuilderCall { call, block } => {
                self.write_expr(*call, child(*call), output);
                output.push(' ');
                let indent = indent_for_expr(output);
                self.write_builder_block(*block, indent, output);
            }
            ArenaExprKind::Try(inner) => {
                let start = output.len();
                self.write_expr(*inner, child(*inner), output);
                // A statement-final `?` after `spawn run` keeps its customary space.
                let spaced = context.follow.token == grouping::FollowToken::End
                    && matches!(self.arena.expr(*inner).kind, ArenaExprKind::Spawn(form) if matches!(form.target, ArenaSpawnTarget::Run(_)));
                if spaced {
                    output.push_str(" ?")
                } else {
                    push_joined(output, start, "?")
                }
            }
            ArenaExprKind::Require { value, schema } => {
                let start = output.len();
                self.write_expr(*value, child(*value), output);
                push_joined(output, start, ".require(");
                if let Some(schema) = schema {
                    self.write_type(*schema, output);
                }
                output.push(')');
            }
            ArenaExprKind::ContextScope {
                kind, input, block, ..
            } => {
                output.push_str(match kind {
                    xsh::frontend::syntax::arena::ContextScopeKind::Cwd => "cd (",
                    xsh::frontend::syntax::arena::ContextScopeKind::Env => "env (",
                });
                self.write_expr(*input, child(*input), output);
                output.push_str(") ");
                self.write_block(*block, indent_for_expr(output), output);
            }
            ArenaExprKind::ErrorContext { message, block } => {
                output.push_str("ctx ");
                self.write_expr(*message, child(*message), output);
                output.push(' ');
                self.write_block(*block, indent_for_expr(output), output);
            }
            ArenaExprKind::Capture(block) => {
                output.push_str("try ");
                self.write_block(*block, indent_for_expr(output), output);
            }
            ArenaExprKind::ValueBlock(block) => {
                // The indentation is that of the line the head starts.
                let indent = indent_for_expr(output);
                if let Some(bound) = self.arena.block_effect_bound(*block) {
                    let effects = self
                        .arena
                        .effects(bound.effects)
                        .map(|effect| effect.as_str())
                        .collect::<Vec<_>>();
                    output.push_str("without ");
                    output.push_str(&effects.join(", "));
                    output.push(' ');
                }
                self.write_block_contents(*block, indent, output, true)
            }
            ArenaExprKind::ValuePipelineCall { input, call, .. } => {
                self.write_expr(*input, child(*input), output);
                output.push_str(" |> ");
                self.write_expr(*call, child(*call), output);
            }
            ArenaExprKind::Loop { block } => {
                output.push_str("loop ");
                self.write_block(*block, 0, output);
            }
            ArenaExprKind::Retry {
                delays,
                pattern,
                block,
            } => {
                output.push_str("retry ");
                self.write_list_inline(*delays, output);
                if let Some(pattern) = pattern {
                    output.push_str(" on ");
                    if matches!(
                        self.arena.pattern(*pattern).kind,
                        ArenaPatternKind::Group(_)
                    ) {
                        self.write_pattern(*pattern, output);
                    } else {
                        output.push('(');
                        self.write_pattern(*pattern, output);
                        output.push(')');
                    }
                }
                output.push(' ');
                let indent = indent_for_expr(output);
                self.write_block(*block, indent, output);
            }
        }
    }

    fn is_name_path(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Ident(_) => true,
            ArenaExprKind::Field { base, .. } => self.is_name_path(base),
            _ => false,
        }
    }

    /// Whether the author broke the line at this operator, which then leads
    /// the continuation line: `ready = enabled` / `  and target.exists()`.
    /// Only operators that may begin a continuation line qualify.
    fn authored_operator_break(&self, op: BinaryOp, left: ExprId, right: ExprId) -> bool {
        // An operand that ends a line can own its newline, so measure from
        // the end of its text.
        !self.inline_only
            && grammar::BINARY_OPERATORS
                .iter()
                .any(|operator| operator.op == op && operator.continues_line)
            && self
                .source
                .get(self.text_end(self.arena.expr(left).span)..self.arena.expr(right).span.start())
                .is_some_and(|gap| gap.contains('\n'))
    }

    fn write_expr_with_internal_comment(&mut self, expr_id: ExprId, output: &mut String) -> bool {
        let span = self.arena.expr(expr_id).span;
        let has_comment = self.comments[self.next_comment..].iter().any(|comment| {
            comment.span.start() >= span.start() && comment.span.start() < span.end()
        });
        if !has_comment || self.holds_sugar(span) {
            return false;
        }
        if let Some(raw) = self.source.get(span.range()) {
            if raw.contains('(') {
                // The raw text keeps its comments but not its redundant parentheses.
                let (text, ranges) =
                    grouping::remove_redundant_parens(&self.source, &[span.range()]);
                output.push_str(&text[ranges[0].clone()]);
            } else {
                output.push_str(raw);
            }
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
            ArenaPipeStageKind::Expr(expr) => self.write_expr(*expr, CLOSE, output),
            ArenaPipeStageKind::Stream(stage) => self.write_stream_stage(stage, indent, output),
        }
    }

    fn write_pipe_stages(
        &mut self,
        input_end: usize,
        stages: xsh::frontend::syntax::arena::ArenaRange,
        indent: usize,
        output: &mut String,
    ) {
        let stages = self.arena.pipe_stages(stages).to_vec();
        let spans: Vec<Span> = stages
            .iter()
            .map(|stage| self.arena.span(stage.span))
            .collect();
        if self.try_write_stages_on_line(
            input_end,
            &spans,
            output,
            |writer, index, stage_indent, line| {
                writer.write_pipe_stage(&stages[index], stage_indent, line)
            },
        ) {
            return;
        }
        for stage in &stages {
            output.push('\n');
            self.write_indent(indent, output);
            output.push_str("|> ");
            self.write_pipe_stage(stage, indent, output);
        }
    }

    /// Writes pipeline stages after the input on the input's line: a single
    /// stage that fits, or stages the author wrote on one line. Those may
    /// also end in a block that breaks, as in `rows |> fold(0) { |acc, row|`,
    /// when the first line fits.
    fn try_write_stages_on_line(
        &mut self,
        input_end: usize,
        stages: &[Span],
        output: &mut String,
        write: impl Fn(&mut Self, usize, usize, &mut String),
    ) -> bool {
        let Some(last) = stages.last() else {
            return true;
        };
        let authored_flat = !self.collection_source_is_multiline(
            Span::new(last.source_id, input_end, last.end()),
            stages,
        );
        if stages.len() == 1 || authored_flat {
            let inline = self.render_inline(|writer, inline| {
                for index in 0..stages.len() {
                    inline.push_str(" |> ");
                    write(writer, index, 0, inline);
                }
            });
            if self.fits_inline(output, &inline) {
                output.push_str(&inline);
                return true;
            }
        }
        let line_indent = indent_for_expr(output);
        authored_flat
            && !self.has_comment_in(input_end, last.end())
            && self.try_write_on_line(output, true, |writer, line| {
                for index in 0..stages.len() {
                    line.push_str(" |> ");
                    write(writer, index, line_indent, line);
                }
            })
    }

    fn write_list(
        &mut self,
        expr_id: ExprId,
        items: xsh::frontend::syntax::arena::ArenaListElementRange,
        output: &mut String,
    ) {
        let elements: Vec<_> = self.arena.list_elements(items).collect();
        let inline =
            self.render_inline(|writer, inline| writer.write_list_literal_inline(items, inline));
        let item_spans: Vec<Span> = elements
            .iter()
            .map(|item| {
                let value = self.arena.expr(item.value).span;
                item.splice_span.map_or(value, |splice| {
                    Span::new(
                        value.source_id,
                        self.arena.span(splice).start(),
                        value.end(),
                    )
                })
            })
            .collect();
        let original_multiline =
            self.collection_source_is_multiline(self.arena.expr(expr_id).span, &item_spans);
        if self.inline_only
            || elements.is_empty()
            || (self.fits_inline(output, &inline)
                && !original_multiline
                && !self.force_collection_expanded)
        {
            output.push_str(&inline);
            return;
        }

        let indent = indent_for_expr(output);
        let expand = self.force_collection_expanded
            || self.sibling_overflows(indent + 1, elements.len(), |writer, index, line| {
                if elements[index].splice_span.is_some() {
                    line.push('@');
                }
                writer.write_expr_safe(elements[index].value, line);
            });
        output.push_str("[\n");
        for item in elements {
            self.write_indent(indent + 1, output);
            let previous_force = self.force_collection_expanded;
            self.force_collection_expanded = expand;
            if item.splice_span.is_some() {
                output.push('@');
            }
            self.write_expr_safe(item.value, output);
            self.force_collection_expanded = previous_force;
            output.push_str(",\n");
        }
        self.write_indent(indent, output);
        output.push(']');
    }

    /// Whether the author broke a collection between its elements or inside
    /// its delimiters. A line break inside one element is that element's own
    /// layout and does not count.
    fn collection_source_is_multiline(&self, span: Span, elements: &[Span]) -> bool {
        let mut previous = span.start();
        for element in elements {
            if self
                .source
                .get(previous..element.start())
                .is_some_and(|gap| gap.contains('\n'))
            {
                return true;
            }
            previous = element.end();
        }
        self.source
            .get(previous..span.end())
            .is_some_and(|gap| gap.contains('\n'))
    }

    /// Whether an element of a broken collection at `indent` cannot fit flat
    /// on its own line. Similar siblings share a shape, so then every element
    /// expands, and so do the collections nested in them; otherwise each
    /// element keeps the layout the author wrote.
    fn sibling_overflows(
        &self,
        indent: usize,
        count: usize,
        write: impl Fn(&mut Self, usize, &mut String),
    ) -> bool {
        (0..count).any(|index| {
            let flat = self.render_inline(|writer, line| write(writer, index, line));
            flat.contains('\n') || indent * 2 + flat.chars().count() >= self.line_width
        })
    }

    fn write_list_literal_inline(
        &mut self,
        items: xsh::frontend::syntax::arena::ArenaListElementRange,
        output: &mut String,
    ) {
        output.push('[');
        for (index, item) in self
            .arena
            .list_elements(items)
            .collect::<Vec<_>>()
            .into_iter()
            .enumerate()
        {
            if index > 0 {
                output.push_str(", ");
            }
            if item.splice_span.is_some() {
                output.push('@');
            }
            self.write_expr_safe(item.value, output);
        }
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
        expr_id: ExprId,
        fields: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let field_count = fields.len();
        let original_multiline = self.record_source_is_multiline(expr_id);
        let inline =
            self.render_inline(|writer, inline| writer.write_record_inline(fields, inline));
        if field_count == 0
            || (!original_multiline
                && self.fits_inline(output, &inline)
                && !self.force_collection_expanded)
        {
            output.push_str(&inline);
            return;
        }

        let indent = indent_for_expr(output);
        let expand = self.force_collection_expanded
            || self.sibling_overflows(indent + 1, field_count, |writer, index, line| {
                let field = writer.arena.record_fields(fields)[index].kind.clone();
                writer.write_record_field(&field, Follow::CLOSE, line);
            });
        output.push_str("{\n");
        for index in 0..field_count {
            let field = self.arena.record_fields(fields)[index].kind.clone();
            self.write_indent(indent + 1, output);
            let previous_force = self.force_collection_expanded;
            self.force_collection_expanded = expand;
            self.write_record_field(&field, Follow::CLOSE, output);
            self.force_collection_expanded = previous_force;
            output.push_str(",\n");
        }
        self.write_indent(indent, output);
        output.push('}');
    }

    fn record_source_is_multiline(&self, expr_id: ExprId) -> bool {
        let ArenaExprKind::Record(fields) = self.arena.expr(expr_id).kind else {
            return false;
        };
        let spans: Vec<Span> = self
            .arena
            .record_fields(fields)
            .iter()
            .filter_map(|field| record_field_span(self.arena, &field.kind))
            .collect();
        self.collection_source_is_multiline(self.arena.expr(expr_id).span, &spans)
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
            self.write_record_field(
                &field,
                if index + 1 == fields.len() {
                    Follow::END
                } else {
                    Follow::CLOSE
                },
                output,
            );
        }
        output.push('}');
    }

    /// Writes a record field followed by `follow`: `}` closes a command
    /// value, `,` does not.
    fn write_record_field(
        &mut self,
        field: &ArenaRecordFieldKind,
        follow: Follow,
        output: &mut String,
    ) {
        let value_context = Context::open(follow);
        match field {
            ArenaRecordFieldKind::Computed { key, value, .. } => {
                output.push('[');
                self.write_expr_safe(*key, output);
                output.push_str("]: ");
                self.write_expr_safe_in(*value, value_context, output);
            }
            ArenaRecordFieldKind::Path { path, value, span } => {
                let names = self.arena.names(*path).collect::<Vec<_>>();
                for (index, name) in names.iter().enumerate() {
                    if index == 0 {
                        self.write_record_key(name, *span, output);
                    } else {
                        output.push('.');
                        output.push_str(name.as_str().as_str());
                    }
                }
                output.push_str(": ");
                self.write_expr_safe_in(*value, value_context, output);
            }
            ArenaRecordFieldKind::Named { name, value, span } => {
                self.write_record_key(name, *span, output);
                output.push_str(": ");
                self.write_expr_safe_in(*value, value_context, output);
            }
            ArenaRecordFieldKind::Shorthand { name, .. } => output.push_str(name.as_str().as_str()),
            ArenaRecordFieldKind::Spread { expr, .. } => {
                output.push_str("...");
                self.write_expr_safe_in(*expr, value_context, output);
            }
        }
    }

    /// Write a record key in a spelling that parses.
    ///
    /// The arena stores an identifier key and a string-literal key as the same
    /// `Name`, so the field's own source text decides which spelling to print.
    /// A string key keeps its quotes: printed bare, a key that is not an
    /// identifier produces source that does not parse at all, which is what
    /// this printer used to do.
    fn write_record_key(
        &mut self,
        name: &Name,
        span: xsh::frontend::syntax::arena::SpanId,
        output: &mut String,
    ) {
        let name = name.as_str();
        let text = name.as_str();
        let quoted = self
            .source
            .get(self.arena.span(span).range())
            .is_some_and(|field| {
                let tokens = Lexer::new(self.arena.span(span).source_id, field).lex_compact();
                tokens.token_table.tag_at(0) == Some(TokenTag::String)
            });
        if quoted {
            write_quoted(text, output);
        } else {
            output.push_str(text);
        }
    }

    fn write_if_expr(
        &mut self,
        branches: xsh::frontend::syntax::arena::ArenaRange,
        else_value: ExprId,
        output: &mut String,
    ) -> bool {
        let indent = indent_for_expr(output);
        let mut flat_branch_broke = false;
        for (index, branch) in self
            .arena
            .if_expr_branches(branches)
            .to_vec()
            .iter()
            .enumerate()
        {
            output.push_str(if index == 0 { "if " } else { " else if " });
            self.write_expr(branch.condition, BRACE, output);
            output.push(' ');
            flat_branch_broke |= self.write_value_branch(branch.value, indent, output);
        }
        output.push_str(" else ");
        flat_branch_broke |= self.write_value_branch(else_value, indent, output);
        flat_branch_broke
    }

    /// Writes one branch of a conditional expression and reports whether a
    /// branch kept on one line as `{ value }` had to break inside the value.
    fn write_value_branch(&mut self, value: ExprId, indent: usize, output: &mut String) -> bool {
        let start = output.len();
        if let ArenaExprKind::ValueBlock(block) = self.arena.expr(value).kind {
            let statements = self
                .arena
                .stmt_ids(self.arena.block(block).statements)
                .collect::<Vec<_>>();
            let span = self.arena.span(self.arena.block(block).span);
            let simple = statements.len() == 1
                && !self
                    .source
                    .get(span.range())
                    .is_some_and(|text| text.contains('#'));
            if simple && (self.inline_only || !self.expr_source_is_multiline(value)) {
                match self.arena.stmt(statements[0]).kind {
                    ArenaStmtKind::Expr(expr) => {
                        output.push_str("{ ");
                        self.write_statement_expr(expr, output);
                        output.push_str(" }");
                        return output[start..].contains('\n');
                    }
                    ArenaStmtKind::TailBareIdent(name) => {
                        output.push_str("{ ");
                        output.push_str(name.as_str().as_str());
                        output.push_str(" }");
                    }
                    _ => self.write_block(block, indent, output),
                }
            } else {
                self.write_block(block, indent, output);
            }
            false
        } else {
            output.push_str("{ ");
            self.write_statement_expr(value, output);
            output.push_str(" }");
            output[start..].contains('\n')
        }
    }

    fn write_if_expr_multiline(
        &mut self,
        branches: xsh::frontend::syntax::arena::ArenaRange,
        else_value: ExprId,
        output: &mut String,
    ) {
        let indent = indent_for_expr(output);
        for (index, branch) in self
            .arena
            .if_expr_branches(branches)
            .to_vec()
            .iter()
            .enumerate()
        {
            output.push_str(if index == 0 { "if " } else { " else if " });
            self.write_expr(branch.condition, BRACE, output);
            output.push(' ');
            if let ArenaExprKind::ValueBlock(block) = self.arena.expr(branch.value).kind {
                self.write_broken_block(block, indent, output, false);
            } else {
                output.push_str("{\n");
                self.write_indent(indent + 1, output);
                self.write_expr_safe_in(
                    branch.value,
                    Context::statement(Follow::END, false),
                    output,
                );
                output.push('\n');
                self.write_indent(indent, output);
                output.push('}');
            }
        }
        output.push_str(" else ");
        if let ArenaExprKind::ValueBlock(block) = self.arena.expr(else_value).kind {
            self.write_broken_block(block, indent, output, false);
        } else {
            output.push_str("{\n");
            self.write_indent(indent + 1, output);
            self.write_expr_safe_in(else_value, Context::statement(Follow::END, false), output);
            output.push('\n');
            self.write_indent(indent, output);
            output.push('}');
        }
    }

    fn write_match_expr(
        &mut self,
        value: ExprId,
        arms: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        output.push_str("match ");
        self.write_expr(value, BRACE, output);
        output.push_str(" {");
        for index in 0..arms.len() {
            let arm = self.arena.match_expr_arms(arms)[index].clone();
            if index > 0 {
                output.push_str(", ");
            } else {
                output.push(' ');
            }
            self.write_arm_pattern(arm.pattern, arm.spelling, output);
            if let Some(guard) = arm.guard {
                output.push_str(" if ");
                self.write_expr(guard, CLOSE, output);
            }
            output.push_str(" => ");
            let follow = if index + 1 == arms.len() {
                Follow::END
            } else {
                Follow::CLOSE
            };
            self.write_expr(arm.value, Context::arm_body(follow), output);
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
        self.write_expr(value, BRACE, output);
        output.push_str(" {");
        if arms.is_empty() {
            output.push('}');
            return;
        }
        output.push('\n');
        for index in 0..arms.len() {
            let arm = self.arena.match_expr_arms(arms)[index].clone();
            self.write_indent(indent + 1, output);
            self.write_arm_pattern(arm.pattern, arm.spelling, output);
            if let Some(guard) = arm.guard {
                output.push_str(" if ");
                self.write_expr(guard, CLOSE, output);
            }
            output.push_str(" => ");
            self.write_expr_safe_in(arm.value, Context::arm_body(Follow::CLOSE), output);
            output.push_str(",\n");
        }
        self.write_indent(indent, output);
        output.push('}');
    }

    fn write_stream_stage(&mut self, stage: &ArenaStreamStage, indent: usize, output: &mut String) {
        output.push_str(stage.kind.as_str());
        if !stage.args.is_empty()
            || (stage.block.is_none() && stage.kind.canonical_parens_when_empty())
        {
            self.write_call_args(stage.args, false, output);
        }
        if let Some(expr) = self.inline_stream_block_expr(stage) {
            output.push(' ');
            let predicate =
                self.render_inline(|writer, inline| writer.write_expr(expr, END, inline));
            // A leading parenthesis immediately after the stage name belongs
            // to its argument list, so preserve the callback's delimiters.
            if stage.args.is_empty() && predicate.starts_with('(') {
                self.write_block(stage.block.unwrap(), indent, output);
            } else {
                output.push_str(&predicate);
            }
        } else if let Some(block) = stage.block {
            output.push(' ');
            self.write_block(block, indent, output);
        }
    }

    fn write_stream_stages(
        &mut self,
        input_end: usize,
        stages: xsh::frontend::syntax::arena::ArenaRange,
        indent: usize,
        output: &mut String,
    ) {
        let stages = self.arena.stream_stages(stages).to_vec();
        let spans: Vec<Span> = stages
            .iter()
            .map(|stage| self.arena.span(stage.span))
            .collect();
        if self.try_write_stages_on_line(
            input_end,
            &spans,
            output,
            |writer, index, stage_indent, line| {
                writer.write_stream_stage(&stages[index], stage_indent, line)
            },
        ) {
            return;
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

    /// Writes a one-line f-string; block f-strings keep their source text,
    /// so line breaks in the value are escaped here.
    fn write_fmt_string_with_prefix(
        &mut self,
        prefix: &str,
        parts: xsh::frontend::syntax::arena::ArenaRange,
        output: &mut String,
    ) {
        let parts: Vec<ArenaFmtPart> = self.arena.fmt_parts(parts).collect();
        output.push_str(prefix);
        output.push('"');
        for part in &parts {
            match part {
                ArenaFmtPart::Text(text) => {
                    let text = self.text_value(text).to_string();
                    write_fmt_text(&text, output);
                }
                ArenaFmtPart::Expr(expr, spec) => {
                    escape_trailing_dollar(output);
                    output.push('{');
                    let start = output.len();
                    // A one-line f-string cannot hold a line break inside
                    // `{...}`: write the expression flat, or keep its source.
                    let line_width = self.line_width;
                    self.line_width = usize::MAX / 4;
                    // A format spec's adjacent colon must stay outside a
                    // bare path or command expression.
                    let context = if spec.is_some() {
                        Context::open(Follow::adjacent(FollowToken::Colon))
                    } else {
                        END
                    };
                    self.write_expr(*expr, context, output);
                    self.line_width = line_width;
                    if output[start..].contains(['\n', '\r']) {
                        output.truncate(start);
                        let span = self.arena.expr(*expr).span;
                        output.push_str(&self.source[span.range()]);
                    }
                    // `{{` is always a brace escape, so an expression that
                    // begins with `{` is set off by spaces: `{ {a: 1}.a }`.
                    if output[start..].starts_with('{') {
                        output.insert(start, ' ');
                        output.push(' ');
                    }
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
        output.push('"');
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
        if let ArenaBuilderEntryKind::Stmt(stmt) = entry.kind {
            self.write_stmt(stmt, indent, output);
            return;
        }
        self.write_indent(indent, output);
        match &entry.kind {
            ArenaBuilderEntryKind::Field { name, value } => {
                output.push_str(name.as_str().as_str());
                output.push_str(" = ");
                self.write_expr(*value, END, output);
            }
            ArenaBuilderEntryKind::Entry { name, args, block } => {
                output.push_str(name.as_str().as_str());
                let tail = if block.is_some() { 2 } else { 0 };
                let continuation = indent + 1 + usize::from(block.is_some());
                self.write_command_args(*args, continuation, tail, output);
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
            ArenaBuilderEntryKind::Stmt(_) => unreachable!("written above"),
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
            ArenaTypeExprKind::Applied { base, arguments } => {
                self.write_type(base, output);
                output.push('[');
                for (index, argument) in arguments.iter().enumerate() {
                    if index != 0 {
                        output.push_str(", ");
                    }
                    self.write_type(*argument, output);
                }
                output.push(']');
            }
            ArenaTypeExprKind::Union(members) => {
                output.push_str("Union[");
                for (index, member) in members.iter().enumerate() {
                    if index != 0 {
                        output.push_str(", ");
                    }
                    self.write_type(*member, output);
                }
                output.push(']');
            }
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
            ArenaTypeExprKind::Map(key, inner) => {
                output.push_str("Map[");
                if let Some(key) = key {
                    self.write_type(key, output);
                    output.push_str(", ");
                }
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
        let field_ids: Vec<(Name, TypeExprId, Option<ExprId>)> = schema_fields
            .iter()
            .map(|f| (f.name, f.ty, f.default))
            .collect();
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
        let field_ids: Vec<(Name, TypeExprId, Option<ExprId>)> = self
            .arena
            .schema_fields(fields)
            .iter()
            .map(|f| (f.name, f.ty, f.default))
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

    fn write_schema_field(
        &mut self,
        field: &(Name, TypeExprId, Option<ExprId>),
        output: &mut String,
    ) {
        output.push_str(field.0.as_str().as_str());
        output.push_str(": ");
        self.write_type(field.1, output);
        if let Some(default) = field.2 {
            output.push_str(" = ");
            self.write_expr_safe(default, output);
        }
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
        // A lone multi-line argument hugs the parentheses unless the author
        // put it on its own line.
        if args.len() == 1 && !original_multiline {
            let arg_kind = self.arena.call_args(args)[0].kind.clone();
            if (self.call_arg_is_multiline_literal(&arg_kind)
                || (self.call_arg_is_multiline_record(&arg_kind) && inline.contains('\n')))
                && self.fits_multiline_inline(output, &inline)
            {
                if self.call_arg_is_multiline_record(&arg_kind) {
                    self.write_multiline_inline(&inline, output);
                } else {
                    output.push_str(&inline);
                }
                return;
            }
            // A lone record that cannot stay flat hugs the parentheses too,
            // rendered expanded. Breaking the argument list instead would
            // hand the next formatting an input whose record source is
            // multiline, which hugs then: the two layouts would alternate.
            if self.call_arg_is_multiline_record(&arg_kind)
                && !inline.contains('\n')
                && !self.fits_inline(output, &inline)
            {
                let mut hugged = String::from("(");
                self.write_call_arg_multiline(&arg_kind, &mut hugged);
                hugged.push(')');
                if self.fits_multiline_inline(output, &hugged) {
                    self.write_multiline_inline(&hugged, output);
                    return;
                }
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
                xsh::frontend::syntax::arena::ArenaCallArgKind::NamedSpread { span, .. }
                | xsh::frontend::syntax::arena::ArenaCallArgKind::Splice { span, .. }
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

    fn try_write_stable_call_chain(
        &mut self,
        expr_id: ExprId,
        context: Context,
        output: &mut String,
    ) -> bool {
        let Some((base, segments)) = call_chain_segments(self.arena, expr_id) else {
            return false;
        };
        let mut base_context = context;
        let mut node = expr_id;
        while node != base {
            let next = match self.arena.expr(node).kind {
                ArenaExprKind::Call { callee, .. } => callee,
                ArenaExprKind::Field { base, .. } => base,
                _ => break,
            };
            base_context = grouping::child_context(self.arena, node, base_context, next);
            node = next;
        }
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
            writer.write_expr(base, base_context, inline);
            for segment in &segments {
                inline.push('.');
                inline.push_str(segment.name.as_str().as_str());
                writer.write_call_args_inline(segment.args, inline);
            }
        });
        if self.fits_inline(output, &inline) && !self.expr_source_is_multiline(expr_id) {
            return false;
        }

        self.write_expr(base, base_context, output);
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
        let tokens = Lexer::new(SourceId::new(0), inline).lex_compact();
        let literal_spans: Vec<_> = (0..tokens.token_table.len())
            .filter(|&index| {
                matches!(
                    tokens.token_table.tag_at(index),
                    Some(
                        TokenTag::String
                            | TokenTag::PathString
                            | TokenTag::GlobString
                            | TokenTag::FmtString
                            | TokenTag::PathFmtString
                            | TokenTag::Bytes
                    )
                )
            })
            .filter_map(|index| tokens.token_table.span_at(index, SourceId::new(0), inline))
            .collect();
        let mut offset = 0;
        for (index, line) in inline.split('\n').enumerate() {
            if index > 0 {
                output.push('\n');
                // Continuation indentation inside a literal changes its value,
                // including the whitespace preceding its closing delimiter.
                if !literal_spans
                    .iter()
                    .any(|span| span.start() < offset && offset < span.end())
                {
                    output.push_str(&prefix);
                }
            }
            output.push_str(line);
            offset += line.len() + 1;
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
            ArenaCallArgKind::NamedSpread { value, .. } => {
                output.push_str("...");
                self.write_expr_safe(*value, output);
            }
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
            ArenaCallArgKind::Named { name, value, span } => {
                output.push_str(name.as_str().as_str());
                output.push(':');
                if self.arena.expr(*value).span.start() != self.arena.span(*span).start() {
                    output.push(' ');
                    self.write_expr_safe(*value, output);
                }
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
            ArenaCallArgKind::NamedSpread { value, .. } => {
                output.push_str("...");
                self.write_expr_safe_multiline_preferred(*value, output);
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
            ArenaCallArgKind::Named { name, value, span } => {
                output.push_str(name.as_str().as_str());
                output.push(':');
                if self.arena.expr(*value).span.start() != self.arena.span(*span).start() {
                    output.push(' ');
                    self.write_expr_safe_multiline_preferred(*value, output);
                }
            }
        }
    }

    fn write_expr_safe(&mut self, expr_id: ExprId, output: &mut String) {
        self.write_expr_safe_in(expr_id, CLOSE, output);
    }

    /// Writes `expr_id`, breaking a conditional, match, or comprehension
    /// across lines when it does not fit.
    fn write_expr_safe_in(&mut self, expr_id: ExprId, context: Context, output: &mut String) {
        let kind = self.arena.expr(expr_id).kind;
        if !matches!(
            kind,
            ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::ListComp { .. }
                | ArenaExprKind::MapComp { .. }
        ) {
            return self.write_expr(expr_id, context, output);
        }
        let parens = grouping::needs_parens(self.arena, &self.source, expr_id, context);
        let inner = if parens { context.group() } else { context };
        if parens {
            output.push('(');
        }
        let inline =
            self.render_inline(|writer, inline| writer.write_expr_contents(expr_id, inner, inline));
        // An inline-only rendering stays flat unless a part of it cannot,
        // such as a multi-statement branch; that part is written broken here
        // so its indentation follows `output`.
        let fits = if self.inline_only {
            !inline.contains('\n')
        } else {
            self.fits_inline(output, &inline) && !self.expr_source_is_multiline(expr_id)
        };
        match kind {
            ArenaExprKind::If {
                branches,
                else_value,
            } if !fits => self.write_if_expr_multiline(branches, else_value, output),
            ArenaExprKind::Match { value, arms } if !fits => {
                self.write_match_expr_multiline(value, arms, output)
            }
            ArenaExprKind::ListComp { qualifiers, .. }
            | ArenaExprKind::MapComp { qualifiers, .. }
                if !fits
                    || self
                        .arena
                        .comp_qualifiers(qualifiers)
                        .iter()
                        .filter(|q| matches!(q, ArenaCompQualifier::For { .. }))
                        .count()
                        != 1
                    || self.arena.comp_qualifiers(qualifiers).len() > 2 =>
            {
                if matches!(kind, ArenaExprKind::ListComp { .. }) {
                    self.write_list_comp_multiline(expr_id, output);
                } else {
                    self.write_map_comp_multiline(expr_id, output);
                }
            }
            _ => output.push_str(&inline),
        }
        if parens {
            output.push(')');
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

    fn write_comp_qualifiers(
        &mut self,
        range: ArenaRange,
        indent: Option<usize>,
        output: &mut String,
    ) {
        for qualifier in self.arena.comp_qualifiers(range).to_vec() {
            if let Some(indent) = indent {
                output.push('\n');
                self.write_comments_before(qualifier.span().start(), indent, output);
                self.write_indent(indent, output);
            } else {
                output.push(' ');
            }
            match qualifier {
                ArenaCompQualifier::For { target, iter, .. } => {
                    output.push_str("for ");
                    self.write_binding_target(target, output);
                    output.push_str(" in ");
                    self.write_expr(iter, CLOSE, output);
                }
                ArenaCompQualifier::If { condition, .. } => {
                    output.push_str("if ");
                    self.write_expr(condition, CLOSE, output);
                }
            }
        }
    }

    fn write_list_comp_multiline(&mut self, expr_id: ExprId, output: &mut String) {
        let ArenaExprKind::ListComp { expr, qualifiers } = self.arena.expr(expr_id).kind else {
            return self.write_expr(expr_id, CLOSE, output);
        };
        let indent = indent_for_expr(output);
        output.push_str("[\n");
        self.write_comments_before(self.arena.expr(expr).span.start(), indent + 1, output);
        self.write_indent(indent + 1, output);
        self.write_expr_safe_in(expr, END, output);
        self.write_comp_qualifiers(qualifiers, Some(indent + 1), output);
        output.push('\n');
        self.write_comments_before(self.arena.expr(expr_id).span.end(), indent + 1, output);
        self.write_indent(indent, output);
        output.push(']');
    }

    fn write_map_comp_key(&mut self, key: ExprId, output: &mut String) {
        fn label_path(arena: &AstArena, id: ExprId) -> bool {
            match arena.expr(id).kind {
                ArenaExprKind::Ident(_) => true,
                ArenaExprKind::Field { base, .. } => label_path(arena, base),
                _ => false,
            }
        }
        let computed = !label_path(self.arena, key);
        if computed {
            output.push('[');
        }
        self.write_expr_safe(key, output);
        if computed {
            output.push(']');
        }
    }

    fn write_map_comp_multiline(&mut self, expr_id: ExprId, output: &mut String) {
        let ArenaExprKind::MapComp {
            key,
            value,
            qualifiers,
        } = self.arena.expr(expr_id).kind
        else {
            return self.write_expr(expr_id, CLOSE, output);
        };
        let indent = indent_for_expr(output);
        output.push_str("{\n");
        self.write_comments_before(self.arena.expr(key).span.start(), indent + 1, output);
        self.write_indent(indent + 1, output);
        self.write_map_comp_key(key, output);
        output.push_str(": ");
        self.write_expr_safe_in(value, END, output);
        self.write_comp_qualifiers(qualifiers, Some(indent + 1), output);
        output.push('\n');
        self.write_comments_before(self.arena.expr(expr_id).span.end(), indent + 1, output);
        self.write_indent(indent, output);
        output.push('}');
    }

    fn expr_source_is_multiline(&self, expr_id: ExprId) -> bool {
        let span = self.arena.expr(expr_id).span;
        self.source
            .get(span.range())
            .is_some_and(|source| source.contains('\n'))
    }

    /// Writes the comments that start before `offset`, one per line, and
    /// reports whether one of them is `# fmt: skip`. A blank line the author
    /// left between two of them, or between the last one and the code at
    /// `offset`, stays as one blank line.
    fn write_comments_before(&mut self, offset: usize, indent: usize, output: &mut String) -> bool {
        let mut skip_formatting = false;
        while self.has_comment_before(offset) {
            self.write_indent(indent, output);
            output.push('#');
            let comment = &self.comments[self.next_comment];
            let text = comment.text.trim_end();
            if text.trim() == "fmt: skip" {
                skip_formatting = true;
            }
            output.push_str(text);
            output.push('\n');
            let end = comment.span.end();
            self.next_comment += 1;
            let next = self
                .comments
                .get(self.next_comment)
                .filter(|next| next.span.start() < offset)
                .map_or(offset, |next| next.span.start());
            if self.blank_line_between(end, next) {
                output.push('\n');
            }
        }
        skip_formatting
    }

    /// Writes the comments before a closing delimiter at `close`, ending on a
    /// fresh line with no blank line before the delimiter.
    fn write_closing_comments(&mut self, close: usize, indent: usize, output: &mut String) {
        self.write_comments_before(close, indent, output);
        if output.ends_with("\n\n") {
            output.pop();
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

    fn write_raw_trailing_comment(&mut self, offset: usize, output: &mut String) {
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
        let Some(raw) = self.source.get(offset..comment.span.end()) else {
            return;
        };
        if raw.contains('\n') || raw.contains('\r') {
            return;
        }
        output.push_str(raw);
        self.next_comment += 1;
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

    fn render_inline(&self, f: impl FnOnce(&mut Self, &mut String)) -> String {
        let mut writer = Writer {
            arena: self.arena,
            source: Arc::clone(&self.source),
            comments: Vec::new(),
            next_comment: 0,
            line_width: self.line_width,
            force_collection_expanded: false,
            inline_only: true,
            after_expression: false,
            arm_statement: false,
            expand_sugar: self.expand_sugar,
            sugar_ranges: Arc::clone(&self.sugar_ranges),
            hidden_names: Arc::clone(&self.hidden_names),
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
        grouping::inline_stage_expr(self.arena, stage)
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
            ArenaCallArgKind::Splice { value, .. }
            | ArenaCallArgKind::NamedSpread { value, .. } => self.expr_is_multiline_literal(*value),
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
            ArenaCallArgKind::Splice { value, .. }
            | ArenaCallArgKind::NamedSpread { value, .. } => {
                matches!(self.arena.expr(*value).kind, ArenaExprKind::Record(_))
            }
        }
    }

    fn expr_is_multiline_literal(&self, expr_id: ExprId) -> bool {
        match self.arena.expr(expr_id).kind {
            ArenaExprKind::Str(_)
            | ArenaExprKind::Regex(_)
            | ArenaExprKind::FmtString(_)
            | ArenaExprKind::PathFmtString(_) => self.expr_source_is_multiline(expr_id),
            ArenaExprKind::Record(_) => self.record_source_is_multiline(expr_id),
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
            | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::EnvString(_) => true,
            ArenaExprKind::Call { callee, .. } => self.command_chain_base_can_be_bare(callee),
            ArenaExprKind::Index { base, index, .. } => {
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
            | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::EnvString(_) => true,
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.command_chain_base_can_be_bare(base)
            }
            ArenaExprKind::Call { callee, .. } => self.command_chain_base_can_be_bare(callee),
            ArenaExprKind::Index { base, index, .. } => {
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

/// Refuses formatter output that does not reparse to the input's syntax tree.
///
/// The formatter regenerates source from the AST, so a missing grouping or a
/// token merge silently changes meaning; the reparse catches that class
/// before any caller writes the file.
fn verify_formatted_output(
    source_id: SourceId,
    source: &str,
    original: &ArenaParseOutput,
    output: FormatOutput,
) -> FormatOutput {
    if !output.diagnostics.is_empty()
        || !original.diagnostics.is_empty()
        || output.formatted == source
    {
        return output;
    }
    let reparsed = Parser::parse_source_arena_only(source_id, &output.formatted);
    let difference = if let Some(error) = reparsed.diagnostics.first() {
        let location = error
            .span
            .or_else(|| error.labels.first().map(|label| label.span))
            .map(|span| line_column(&output.formatted, span.start()))
            .map_or(String::new(), |(line, column)| {
                format!(" at output {line}:{column}")
            });
        Some((
            0,
            format!(
                "formatted output does not parse{location}: {}",
                error.message
            ),
        ))
    } else {
        let before = format_equivalence::canonical(&original.arena, source);
        let after = format_equivalence::canonical(&reparsed.arena, &output.formatted);
        let index = before
            .text
            .bytes()
            .zip(after.text.bytes())
            .position(|(left, right)| left != right)
            .or((before.text.len() != after.text.len())
                .then(|| before.text.len().min(after.text.len())));
        index.map(|index| {
            let (line, column) = line_column(
                &output.formatted,
                after.source_offset_at(index).unwrap_or(0),
            );
            (
                before.source_offset_at(index).unwrap_or(0),
                format!(
                    "formatted output parses to a different syntax tree at output {line}:{column}"
                ),
            )
        })
    };
    match difference {
        None => output,
        Some((offset, message)) => FormatOutput {
            formatted: String::new(),
            diagnostics: vec![
                Diagnostic::error(format!("formatter refused to rewrite this file: {message}"))
                    .with_code(DiagnosticCode::FormatEquivalence)
                    .with_span(Span::new(source_id, offset, offset))
                    .with_note("this is a formatter bug; the file was left unchanged"),
            ],
        },
    }
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.len() - before.rfind('\n').map_or(0, |index| index + 1) + 1;
    (line, column)
}

/// Appends `token` directly after the expression written from `start`,
/// separated by a space where the lexer would merge them (`lexer::join_tokens`).
fn push_joined(output: &mut String, start: usize, token: &str) {
    let joined = join_tokens(&output[start..], token);
    output.truncate(start);
    output.push_str(&joined);
}

fn needs_top_level_blank(previous: &ArenaStmtKind, current: &ArenaStmtKind) -> bool {
    matches!(
        (previous, current),
        (
            ArenaStmtKind::ProcDef(_)
                | ArenaStmtKind::CliMain(_)
                | ArenaStmtKind::PureDef(_)
                | ArenaStmtKind::StreamDef(_),
            _
        ) | (
            _,
            ArenaStmtKind::ProcDef(_)
                | ArenaStmtKind::CliMain(_)
                | ArenaStmtKind::PureDef(_)
                | ArenaStmtKind::StreamDef(_)
        ) | (
            ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_) | ArenaStmtKind::SignalHook(_),
            _
        ) | (
            _,
            ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_) | ArenaStmtKind::SignalHook(_)
        ) | (ArenaStmtKind::Export(_), _)
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
    ) || matches!(kind, ArenaStmtKind::Sugar { form, .. } if form.is_compound())
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

/// Whether the source wrote this union's variants across more than one line.
fn tag_variants_original_multiline(
    arena: &AstArena,
    source: &str,
    variants: xsh::frontend::syntax::arena::ArenaRange,
) -> bool {
    let spans: Vec<Span> = arena
        .tag_variants(variants)
        .iter()
        .map(|variant| arena.span(variant.span))
        .collect();
    let (Some(first), Some(last)) = (spans.first(), spans.last()) else {
        return false;
    };
    source
        .get(first.start()..last.end())
        .is_some_and(|source| source.contains('\n'))
}

/// The offset of the `}` that closes a braced error family whose last variant
/// ends at `from`: only blank space and comments can come before it.
fn error_block_close(source: &str, from: usize, limit: usize) -> usize {
    let mut in_comment = false;
    for (offset, character) in source.get(from..limit).unwrap_or("").char_indices() {
        match character {
            '\n' => in_comment = false,
            '#' => in_comment = true,
            '}' if !in_comment => return from + offset,
            _ => {}
        }
    }
    limit
}

fn record_field_span(arena: &AstArena, field: &ArenaRecordFieldKind) -> Option<Span> {
    match field {
        ArenaRecordFieldKind::Computed { span, .. }
        | ArenaRecordFieldKind::Named { span, .. }
        | ArenaRecordFieldKind::Shorthand { span, .. }
        | ArenaRecordFieldKind::Spread { span, .. }
        | ArenaRecordFieldKind::Path { span, .. } => Some(arena.span(*span)),
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

/// One part of a command after its first word: the unit a `\` line
/// continuation separates.
#[derive(Clone)]
enum CommandPart {
    /// A run option, `--name=value`.
    Option {
        name: &'static str,
        value: ExprId,
    },
    Env(ArenaEnvAssignment),
    Arg(ArenaCommandArg),
    Redirection(xsh::frontend::syntax::arena::ArenaRedirection),
}

/// Whether the source `before` a command part ends with a `\` line
/// continuation and the indentation of the line it continues onto.
fn follows_line_continuation(before: &str) -> bool {
    let before = before.trim_end_matches([' ', '\t']);
    before
        .strip_suffix('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .is_some_and(|line| line.ends_with('\\'))
}

fn run_head_text(kind: RunKind) -> &'static str {
    match kind {
        RunKind::Plain => "run",
        RunKind::Status => "run.status",
        RunKind::CaptureText => "run.text",
        RunKind::CaptureBytes => "run.bytes",
        RunKind::CaptureTextRecord => "run.capture --text",
        RunKind::CaptureBytesRecord => "run.capture --bytes",
        RunKind::StreamText => "run.stream --text",
        RunKind::StreamBytes => "run.stream --bytes",
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

/// `e"NAME"`; the parser admits only identifier names, so nothing needs
/// escaping.
fn write_env_string(name: xsh::frontend::symbols::Name, output: &mut String) {
    output.push_str("e\"");
    output.push_str(name.as_str().as_str());
    output.push('"');
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
            ch if ch == ' '
                || !(ch.is_control() || ch.is_whitespace() || is_invisible_format_char(ch)) =>
            {
                output.push(ch)
            }
            ch => {
                let _ = write!(output, "\\u{{{:x}}}", ch as u32);
            }
        }
    }
    output.push('"');
}

/// Zero-width and bidirectional formatting characters stay escaped so a
/// formatted string literal never hides or reorders its visible text.
fn is_invisible_format_char(ch: char) -> bool {
    matches!(ch, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
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
            '{' => output.push_str("{{"),
            '}' => output.push_str("}}"),
            '$' if chars.peek().copied().is_some_and(is_identifier_start) => output.push_str("\\$"),
            ch => output.push(ch),
        }
    }
}

/// Whether `text` is exactly one `"..."` string: its only unescaped quotes
/// are the first and last characters.
fn is_one_quoted_string(text: &str) -> bool {
    let Some(inner) = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return false;
    };
    let mut escaped = false;
    for ch in inner.chars() {
        match ch {
            '\\' => escaped = !escaped,
            '"' if !escaped => return false,
            _ => escaped = false,
        }
    }
    !escaped
}

/// Text that ends in a literal `$` right before an interpolation must keep
/// it escaped: `\${i}` prints `$` then `i`, while `${i}` is an error.
fn escape_trailing_dollar(output: &mut String) {
    let Some(before) = output.strip_suffix('$') else {
        return;
    };
    let backslashes = before.chars().rev().take_while(|ch| *ch == '\\').count();
    if backslashes % 2 == 0 {
        output.insert(output.len() - 1, '\\');
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

#[cfg(test)]
mod tests {
    use super::{
        FormatOutput, Formatter, Parser, SourceId, format_equivalence, verify_formatted_output,
    };
    use xsh::diagnostic::DiagnosticCode;

    /// Formats `source`, checks the exact text, and checks that the text
    /// reparses to the same position-free tree and is a formatting fixpoint.
    fn assert_round_trip(source: &str, expected: &str) {
        let formatted = Formatter::new().format_source(SourceId::new(0), source);
        assert!(
            formatted.diagnostics.is_empty(),
            "{:?}",
            formatted.diagnostics
        );
        assert_eq!(formatted.formatted, expected);
        let before = Parser::parse_source_arena_only(SourceId::new(0), source);
        let after = Parser::parse_source_arena_only(SourceId::new(0), expected);
        assert!(after.diagnostics.is_empty(), "{:?}", after.diagnostics);
        assert_eq!(
            format_equivalence::canonical(&before.arena, source).text,
            format_equivalence::canonical(&after.arena, expected).text,
        );
        assert_eq!(
            Formatter::new()
                .format_source(SourceId::new(0), expected)
                .formatted,
            expected
        );
    }

    /// Groupings an AST-regenerating printer once dropped or invented; the
    /// general case is `format_proofs::needs_parens_is_exact_for_every_slot_and_form`.
    #[test]
    fn reported_grouping_bugs_print_only_required_parentheses() {
        for (source, expected) in [
            (
                "let a = (nested?)?\nlet b = (nested?)?.trim()\nlet c = (items?)[0]\nlet d = (items?)?[0]\n",
                None,
            ),
            (
                "let c = xs[(start.require(Int)?)..(end.require(Int)?)]\n",
                Some("let c = xs[(start.require(Int)?)..end.require(Int)?]\n"),
            ),
            (
                "assert (run.bytes cat < b\"\" ?) == b\"\"\n",
                Some("assert (run.bytes cat < b\"\")? == b\"\"\n"),
            ),
            (
                "assert (wait child?).exited_with(1)\nlet n = (marker.read_text()?).parse_int()?\n",
                None,
            ),
            (
                "proc sign(x: Bool) -> Int {\n  return 1 when x\n  (-1)\n}\n",
                Some("proc sign(x: Bool) -> Int {\n  return 1 when x\n  -1\n}\n"),
            ),
            (
                "let picked = match name { \"a\" => ({...value, a: 1}), _ => ({a: 2}) }\n",
                Some("let picked = match name { \"a\" => {...value, a: 1}, _ => {a: 2} }\n"),
            ),
            ("let x = (run foo)?\nlet y = {\n  (x)\n}\n", None),
            (
                "match r {\n  Err(X.Timeout {..}) => {a: 1}\n  _ => ({b})\n}\n",
                None,
            ),
            (
                "match r {\n  _ => ({a: 1})\n}\n",
                Some("match r {\n  _ => {a: 1}\n}\n"),
            ),
        ] {
            assert_round_trip(source, expected.unwrap_or(source));
        }
    }

    #[test]
    fn equivalence_ignores_lowered_value_pipeline_sugar() {
        assert_round_trip("let t = \" x \" |> trim()\n", "let t = \" x \".trim()\n");
    }

    #[test]
    fn repeat_prints_its_operands_and_keeps_needed_grouping() {
        for (source, expected) in [
            (
                "repeat   n+1   times{\nprint \"tick\"\n}\n",
                "repeat n + 1 times {\n  print \"tick\"\n}\n",
            ),
            (
                "repeat (n) times { # each\n  total += 1 # one\n}\n",
                "repeat n times {\n  # each\n  total += 1 # one\n}\n",
            ),
            (
                "repeat times times { repeat (a |> len()) times { total += 1 } }\n",
                "repeat times times { repeat a.len() times { total += 1 } }\n",
            ),
            (
                "match n {\n  0 => repeat 2 times { total += 1 }\n  _ => {}\n}\n",
                "match n {\n  0 => repeat 2 times { total += 1 }\n  _ => {}\n}\n",
            ),
        ] {
            assert_round_trip(source, expected);
        }
    }

    #[test]
    fn safety_net_refuses_output_with_a_different_tree() {
        let source = "let x = a - (b - c)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let regrouped = FormatOutput {
            formatted: "let x = a - b - c\n".to_string(),
            diagnostics: Vec::new(),
        };
        let refused = verify_formatted_output(SourceId::new(0), source, &parsed, regrouped);
        assert!(refused.formatted.is_empty());
        assert_eq!(refused.diagnostics.len(), 1);
        assert_eq!(
            refused.diagnostics[0].code,
            Some(DiagnosticCode::FormatEquivalence)
        );
        assert!(
            refused.diagnostics[0].message.contains("output 1:9"),
            "{}",
            refused.diagnostics[0].message
        );
        assert_eq!(
            refused.diagnostics[0].span.map(|span| span.start()),
            Some(8)
        );

        let unparsable = FormatOutput {
            formatted: "let x = (a - b\n".to_string(),
            diagnostics: Vec::new(),
        };
        let refused = verify_formatted_output(SourceId::new(0), source, &parsed, unparsable);
        assert!(refused.formatted.is_empty());
        assert!(
            refused.diagnostics[0].message.contains("does not parse"),
            "{}",
            refused.diagnostics[0].message
        );
    }

    #[test]
    fn assert_statements_format_in_every_statement_position() {
        let source = "proc check(n: Int) {\n  match n {\n    1 => assert n == 1\n    _ => { assert n > 1, \"large\" }\n  }\n  {\n    assert n > 0\n  }\n  if n > 0 {\n    assert 0 < n < 10, \"bounded\"\n  }\n}\n";
        let expected = "proc check(n: Int) {\n  match n {\n    1 => assert n == 1\n    _ => assert n > 1, \"large\"\n  }\n\n  {\n    assert n > 0\n  }\n  if n > 0 {\n    assert 0 < n < 10, \"bounded\"\n  }\n}\n";
        let formatted = Formatter::new().format_source(SourceId::new(0), source);
        assert!(
            formatted.diagnostics.is_empty(),
            "{:?}",
            formatted.diagnostics
        );
        assert_eq!(formatted.formatted, expected);
        assert_eq!(
            Formatter::new()
                .format_source(SourceId::new(0), expected)
                .formatted,
            expected
        );
    }

    #[test]
    fn control_flow_expanded_to_multiple_lines_is_followed_by_one_blank_line() {
        let source = "proc runner() -> Result[Path] {\n  let configured = \"\"\n  if configured != \"\" {\n  return fp\"{configured}\" }\n  process.which(\"xsh\")?\n}\n";
        let expected = "proc runner() -> Result[Path] {\n  let configured = \"\"\n  if configured != \"\" {\n    return fp\"{configured}\"\n  }\n\n  process.which(\"xsh\")?\n}\n";
        let formatted = Formatter::new().format_source(SourceId::new(0), source);
        assert!(
            formatted.diagnostics.is_empty(),
            "{:?}",
            formatted.diagnostics
        );
        assert_eq!(formatted.formatted, expected);
        assert_eq!(
            Formatter::new()
                .format_source(SourceId::new(0), expected)
                .formatted,
            expected
        );
    }

    #[test]
    fn string_literals_keep_printable_unicode_and_escape_invisible_characters() {
        let source = "let word = \"caf\\u{e9} \u{1f600}\"\nlet hidden = \"a\\u{200b}b\\u{202e}c\"\nprint ${word} ${hidden}\n";
        let expected = "let word = \"caf\u{e9} \u{1f600}\"\nlet hidden = \"a\\u{200b}b\\u{202e}c\"\nprint ${word} ${hidden}\n";
        let formatted = Formatter::new().format_source(SourceId::new(0), source);
        assert!(
            formatted.diagnostics.is_empty(),
            "{:?}",
            formatted.diagnostics
        );
        assert_eq!(formatted.formatted, expected);
    }
}
