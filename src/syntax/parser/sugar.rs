//! Surface forms defined by expansion.
//!
//! Each form has one function here that turns its parsed operands into core
//! arena statements. That function is the only definition of the form's
//! meaning: checking, lowering, and execution see the expansion and nothing
//! else, while formatting, lint, and the other source tools see the operands.
//!
//! An expansion keeps these rules. The `xsht` test
//! `every_form_keeps_the_expansion_rules` checks them for every `SugarForm`,
//! using the complete tree walk that the formatter's equivalence check owns:
//!
//! - It references each operand exactly once and in the order the operands
//!   are listed, which is the order they run: source order, except that a
//!   postfix condition precedes the statement it guards. Nothing the user
//!   wrote is evaluated twice or out of order. A value the expansion needs
//!   twice is bound once to a local whose name no identifier can spell.
//! - Its root statement carries the surface statement's span, so tracebacks,
//!   traces, and coverage report the statement the user wrote. Every other
//!   node it adds has a span of its own inside the surface statement, distinct
//!   from every other expression, statement, or block span, because checker
//!   facts are keyed by span and two nodes sharing one would overwrite each
//!   other.
//! - Its root is a core control or effect statement, never a declaration,
//!   import, export, or binding: those are collected by scans of statement
//!   lists that do not look inside a surface form.
//!
//! A form's meaning is its expansion and nothing more, with one exception an
//! expansion may ask for: that a block leave the enclosing continuation on
//! every path. The checker enforces that on the block, not on the form.

use super::{Keyword, Name, Parser, TokenTag};
use crate::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use crate::source::Span;
use crate::syntax::node::StreamStageKind;
use crate::syntax::arena::{
    ArenaCallArgInput, ArenaExprKind, ArenaExprOrRun, ArenaProgramBuilder, ArenaSugarOperand,
    BindingTargetId,
    BlockId, ExprId, StmtId, SugarForm,
};

/// The word that begins a `repeat` statement. It stays an ordinary identifier
/// everywhere else (`repeat` is also a stream stage).
const REPEAT_WORD: &str = "repeat";
/// The word that ends a `repeat` statement's count.
const TIMES_WORD: &str = "times";
/// The word that begins a `fail` statement. It stays an ordinary identifier
/// everywhere else (`test.fail`, a record field named `fail`).
const FAIL_WORD: &str = "fail";
/// The word between a `fail` statement's failure and its cause.
const BECAUSE_WORD: &str = "because";

impl Parser<'_> {
    /// Whether the statement at the cursor is `repeat COUNT times {`.
    ///
    /// `repeat` is not reserved, so the statement is recognized by its whole
    /// head, written on one line: the word `repeat`, a count, and then,
    /// outside every bracket and brace the count opens, the word `times`
    /// directly before `{`. A line break anywhere in the head leaves an
    /// ordinary statement that begins with a name spelled `repeat`.
    pub(super) fn lookahead_is_repeat(&self) -> bool {
        if !self.current_name().is_some_and(|name| name == REPEAT_WORD)
            || self.peek_start(1) == Some(self.current_end())
        {
            return false;
        }
        let mut depth = 0usize;
        let mut offset = 1;
        while let Some(tag) = self.peek_tag(offset) {
            match tag {
                TokenTag::Ident
                    if depth == 0
                        && offset > 1
                        && self.peek_tag(offset - 1) != Some(TokenTag::Dot)
                        && self.peek_tag(offset + 1) == Some(TokenTag::LBrace)
                        && self
                            .peek_name(offset)
                            .is_some_and(|name| name == TIMES_WORD) =>
                {
                    return true;
                }
                TokenTag::LParen
                | TokenTag::LBracket
                | TokenTag::LBrace
                | TokenTag::DollarLBrace => depth += 1,
                TokenTag::RParen | TokenTag::RBracket | TokenTag::RBrace => {
                    let Some(outer) = depth.checked_sub(1) else {
                        return false;
                    };
                    depth = outer;
                }
                TokenTag::Semicolon if depth == 0 => return false,
                TokenTag::Newline | TokenTag::Eof => return false,
                _ => {}
            }
            offset += 1;
        }
        false
    }

    pub(super) fn parse_repeat_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let keyword = self.bump();
        let count = self.parse_head_expr_arena_only(arena)?.id;
        if !self.current_name().is_some_and(|name| name == TIMES_WORD) {
            self.diagnostic_here(
                "expected `times` after the repeat count",
                DiagnosticCode::ParseExpectedKeyword,
            );
            return None;
        }
        self.bump();
        let head = self.span(start, self.previous_end());
        let body = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        let operands = RepeatOperands {
            keyword,
            head,
            count,
            body,
        };
        arena.push_sugar(
            SugarForm::Repeat,
            &[
                ArenaSugarOperand::Expr(count),
                ArenaSugarOperand::Block(body),
            ],
            span,
            |arena| expand_repeat(arena, operands, span),
        );
        Some(())
    }
}

struct RepeatOperands {
    /// The `repeat` word.
    keyword: Span,
    /// `repeat COUNT times`, everything before the body.
    head: Span,
    count: ExprId,
    body: BlockId,
}

/// `repeat COUNT times { BODY }` is `for _ in range(COUNT) { BODY }`.
///
/// The `range` callee sits on the `repeat` word and the call on the whole
/// head, so a diagnostic about the count lands on the count the user wrote
/// and one about the call lands on `repeat COUNT times`.
fn expand_repeat(
    arena: &mut ArenaProgramBuilder<'_>,
    operands: RepeatOperands,
    span: Span,
) -> StmtId {
    let callee = arena.push_ident_expr(Name::intern("range"), operands.keyword);
    arena.begin_call_args();
    arena.push_call_arg_input(ArenaCallArgInput::Positional(operands.count));
    let args = arena.finish_call_args();
    let iter = arena.push_call_expr(callee, args, operands.head);
    let target = arena.push_binding_target_name(Name::intern("_"));
    arena.push_for_id(target, iter, operands.body, span)
}

impl Parser<'_> {
    /// Parses the `when CONDITION` or `unless CONDITION` that follows `inner`,
    /// the statement the parser just registered.
    pub(super) fn parse_guarded_stmt_arena_only(
        &mut self,
        start: usize,
        inner: StmtId,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let keyword = self.current_span();
        let negate = self.consume_keyword(Keyword::Unless).is_some();
        if !negate {
            self.expect_keyword(Keyword::When, "expected `when` or `unless`");
        }
        let condition = self.parse_expr_id_arena_only(arena)?;
        let end = self.expect_terminator();
        let span = self.span(start, end);
        // The guarded statement belongs to the expansion's block, not to the
        // enclosing statement list.
        let registered = arena.pop_last_statement();
        assert_eq!(registered, inner, "the guarded statement is registered last");
        let operands = GuardedOperands {
            keyword,
            condition,
            stmt: inner,
        };
        // A postfix condition runs before the statement it guards, so the
        // operands are listed in that order.
        arena.push_sugar(
            if negate {
                SugarForm::Unless
            } else {
                SugarForm::When
            },
            &[
                ArenaSugarOperand::Expr(condition),
                ArenaSugarOperand::Stmt(inner),
            ],
            span,
            |arena| {
                if negate {
                    expand_unless(arena, operands, span)
                } else {
                    expand_when(arena, operands, span)
                }
            },
        );
        Some(())
    }

    /// Parses `CONDITION else { BLOCK }` after the `guard` word.
    pub(super) fn parse_boolean_guard_arena_only(
        &mut self,
        start: usize,
        keyword: Span,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let condition = self.parse_condition_arena_only(arena)?.id;
        self.expect_keyword(Keyword::Else, "expected `else` after guard condition");
        let else_block = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        arena.push_sugar(
            SugarForm::Guard,
            &[
                ArenaSugarOperand::Expr(condition),
                ArenaSugarOperand::Block(else_block),
            ],
            span,
            |arena| expand_guard(arena, keyword, condition, else_block, span),
        );
        Some(())
    }
}

#[derive(Clone, Copy)]
struct GuardedOperands {
    /// The `when` or `unless` word.
    keyword: Span,
    condition: ExprId,
    stmt: StmtId,
}

/// `STMT when CONDITION` is `if CONDITION { STMT }`.
///
/// The branch block sits on the guarded statement, which no written block
/// can span because it has no braces of its own.
fn expand_when(
    arena: &mut ArenaProgramBuilder<'_>,
    operands: GuardedOperands,
    span: Span,
) -> StmtId {
    let then_block = arena.push_block_of(&[operands.stmt], arena.stmt_span(operands.stmt));
    arena.push_if(&[(operands.condition, then_block)], None, span)
}

/// `STMT unless CONDITION` is `if CONDITION {} else { STMT }`.
///
/// The condition is not negated, so it keeps its own diagnostics and facts
/// and may be a `Status`. The empty branch sits on the `unless` word.
fn expand_unless(
    arena: &mut ArenaProgramBuilder<'_>,
    operands: GuardedOperands,
    span: Span,
) -> StmtId {
    let then_block = arena.push_block_of(&[], operands.keyword);
    let else_block = arena.push_block_of(&[operands.stmt], arena.stmt_span(operands.stmt));
    arena.push_if(
        &[(operands.condition, then_block)],
        Some(else_block),
        span,
    )
}

/// `guard CONDITION else { BLOCK }` is `if CONDITION {} else { BLOCK }`,
/// where the block must leave the enclosing continuation.
///
/// The empty branch sits on the `guard` word.
fn expand_guard(
    arena: &mut ArenaProgramBuilder<'_>,
    keyword: Span,
    condition: ExprId,
    else_block: BlockId,
    span: Span,
) -> StmtId {
    let then_block = arena.push_block_of(&[], keyword);
    arena.require_block_exit(else_block);
    arena.push_if(&[(condition, then_block)], Some(else_block), span)
}

impl Parser<'_> {
    /// Parses `, ITEM in SOURCE { BODY }` after `for INDEX`, with the cursor
    /// on the comma.
    pub(super) fn parse_for_index_arena_only(
        &mut self,
        start: usize,
        keyword: Span,
        index: BindingTargetId,
        index_span: Span,
        index_is_name: bool,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        if !index_is_name {
            self.diagnostics.push(
                Diagnostic::new(Severity::Error, "the index of a `for` loop is a name")
                    .with_code(DiagnosticCode::ParseForIndex)
                    .with_label(Label::primary(
                        index_span,
                        "write `for index, item in source`; only the item may be destructured",
                    )),
            );
        }
        self.bump();
        let item_start = self.current_start();
        let item = self.parse_binding_target_arena_only("expected loop binding name", arena)?;
        let item_span = self.span(item_start, self.previous_end());
        self.expect_keyword(Keyword::In, "expected `in` in for loop");
        let source = self.parse_head_expr_arena_only(arena)?.id;
        let source_span = arena.expr_span(source);
        let body = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        let operands = ForIndexOperands {
            keyword,
            index,
            index_span,
            item,
            item_span,
            source,
            source_span,
            body,
        };
        arena.push_sugar(
            SugarForm::ForIndex,
            &[
                ArenaSugarOperand::BindingTarget(index),
                ArenaSugarOperand::BindingTarget(item),
                ArenaSugarOperand::Expr(source),
                ArenaSugarOperand::Block(body),
            ],
            span,
            |arena| expand_for_index(arena, operands, span),
        );
        Some(())
    }
}

struct ForIndexOperands {
    /// The `for` word.
    keyword: Span,
    index: BindingTargetId,
    index_span: Span,
    item: BindingTargetId,
    item_span: Span,
    source: ExprId,
    source_span: Span,
    body: BlockId,
}

/// `for INDEX, ITEM in SOURCE { BODY }` is
/// `for {index: INDEX, value: ITEM} in SOURCE |> enumerate() { BODY }`.
///
/// The `enumerate` stage produces one `{index, value}` record per item, and
/// the record target binds its two fields to the names the user wrote, so no
/// hidden local is needed. The pipeline is the one expression the expansion
/// adds; it spans the head `for INDEX, ITEM in SOURCE`, which no written
/// expression can span. Its stage is the empty span where the source ends:
/// a stage follows its input, as the stages of a written pipeline do, and no
/// written stage is empty. A source that is not a list or a stream is
/// reported on the source itself, as the input of the pipeline.
fn expand_for_index(
    arena: &mut ArenaProgramBuilder<'_>,
    operands: ForIndexOperands,
    span: Span,
) -> StmtId {
    let within = |start: usize, end: usize| Span::new(span.source_id, start, end);
    arena.begin_destructure_fields();
    arena.push_destructure_field(Name::intern("index"), operands.index, operands.index_span);
    arena.push_destructure_field(Name::intern("value"), operands.item, operands.item_span);
    let fields = arena.finish_destructure_fields();
    let target = arena.push_binding_target_record(
        fields,
        false,
        within(operands.index_span.start(), operands.item_span.end()),
    );
    arena.begin_call_args();
    let args = arena.finish_call_args();
    let stage_span = within(operands.source_span.end(), operands.source_span.end());
    let stage = arena.build_stream_stage(StreamStageKind::Enumerate, None, args, stage_span);
    let iter = arena.build_structured_pipeline_expr(
        operands.source,
        vec![stage],
        within(operands.keyword.start(), operands.source_span.end()),
    );
    arena.push_for_id(target, iter, operands.body, span)
}

impl Parser<'_> {
    /// Whether the command statement at the cursor is a `fail` statement.
    /// `fail` is not reserved: it begins the statement only where a command
    /// named `fail` would be read, with its message or `.Variant(...)` after a
    /// space on the same line.
    pub(super) fn lookahead_is_fail(&self) -> bool {
        self.current_name().is_some_and(|name| name == FAIL_WORD)
            && self.peek_start(1).is_some_and(|next| next > self.current_end())
            && !matches!(
                self.peek_tag(1),
                None | Some(
                    TokenTag::Newline
                        | TokenTag::Semicolon
                        | TokenTag::RBrace
                        | TokenTag::Comment
                        | TokenTag::Eof
                )
            )
    }

    pub(super) fn parse_fail_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let keyword = self.bump();
        let failure = self.parse_expr_id_arena_only(arena)?;
        let failure_end = arena.expr_span(failure).end();
        let cause = if self.current_name().is_some_and(|name| name == BECAUSE_WORD) {
            let because = self.bump();
            Some((because, self.parse_expr_id_arena_only(arena)?))
        } else {
            None
        };
        let guarded = self.at_keyword(Keyword::When) || self.at_keyword(Keyword::Unless);
        let end = if guarded {
            self.previous_end()
        } else {
            self.expect_terminator()
        };
        let span = self.span(start, end);
        // A leading-dot name, alone or called, is an error of the family the
        // function declares. Anything else is a message.
        let variant = {
            let constructor = match arena.expr_kind(failure) {
                ArenaExprKind::Call { callee, .. } => callee,
                _ => failure,
            };
            matches!(
                arena.expr_kind(constructor),
                ArenaExprKind::Field { base, .. }
                    if matches!(arena.expr_kind(base), ArenaExprKind::Item)
            )
        };
        let operands = FailOperands {
            keyword,
            failure,
            failure_end,
            variant,
            cause: cause.map(|(because, cause)| FailCause {
                argument: self.span(because.start(), arena.expr_span(cause).end()),
                cause,
            }),
        };
        let written = match cause {
            Some((_, cause)) => vec![
                ArenaSugarOperand::Expr(failure),
                ArenaSugarOperand::Expr(cause),
            ],
            None => vec![ArenaSugarOperand::Expr(failure)],
        };
        let inner = arena.push_sugar(SugarForm::Fail, &written, span, |arena| {
            expand_fail(arena, operands, span)
        });
        if guarded {
            return self.parse_guarded_stmt_arena_only(start, inner, arena);
        }
        Some(())
    }
}

#[derive(Clone, Copy)]
struct FailOperands {
    /// The `fail` word.
    keyword: Span,
    failure: ExprId,
    failure_end: usize,
    /// The failure is written `.Variant` or `.Variant(...)`.
    variant: bool,
    cause: Option<FailCause>,
}

#[derive(Clone, Copy)]
struct FailCause {
    /// `because CAUSE`.
    argument: Span,
    cause: ExprId,
}

/// `fail MESSAGE` is `return Err(error.failure(MESSAGE))`, and
/// `fail .Variant(...)` is `return Err(.Variant(...))`. `because CAUSE` adds
/// `cause: CAUSE` to the `Err`.
///
/// A call spelled `error.failure(...)` names the module function even where
/// `error` is a local, so the expansion means the same inside an `Err(error)`
/// arm. The `Err` call sits on the `fail` word, so a function that cannot
/// return this error reports there; the `error.failure` call sits on the text
/// after the word, and a message of the wrong type reports on the message
/// itself. The three names that can carry no diagnostic of their own take
/// distinct prefixes of the word.
fn expand_fail(arena: &mut ArenaProgramBuilder<'_>, operands: FailOperands, span: Span) -> StmtId {
    let FailOperands {
        keyword,
        failure,
        failure_end,
        variant,
        cause,
    } = operands;
    let within = |start: usize, end: usize| Span::new(span.source_id, start, end);
    // `fail` has four bytes, so these three are distinct and none is the
    // whole word.
    let keyword_prefix = |len: usize| within(keyword.start(), keyword.start() + len);

    let error = if variant {
        failure
    } else {
        let module = arena.push_ident_expr(Name::intern("error"), keyword_prefix(1));
        let constructor =
            arena.push_field_expr(module, Name::intern("failure"), keyword_prefix(2));
        arena.begin_call_args();
        arena.push_call_arg_input(ArenaCallArgInput::Positional(failure));
        let args = arena.finish_call_args();
        arena.push_call_expr(constructor, args, within(keyword.end(), failure_end))
    };

    let err = arena.push_ident_expr(Name::intern("Err"), keyword_prefix(3));
    arena.begin_call_args();
    arena.push_call_arg_input(ArenaCallArgInput::Positional(error));
    if let Some(FailCause { argument, cause }) = cause {
        // The argument starts on `because`, before its value, as a written
        // `cause: value` starts on its name.
        arena.push_call_arg_input(ArenaCallArgInput::Named {
            name: Name::intern("cause"),
            value: cause,
            span: argument,
        });
    }
    let args = arena.finish_call_args();
    let value = arena.push_call_expr(err, args, keyword);
    arena.push_return(Some(ArenaExprOrRun::Expr(value)), span)
}

#[cfg(test)]
mod tests {
    use crate::source::SourceId;
    use crate::syntax::arena::{ArenaProgram, ArenaStmtKind, SugarForm};
    use crate::syntax::grammar::earley::{Recognizer, top_level_parts};
    use crate::syntax::grammar::generate::Generator;
    use crate::syntax::grammar::{grammar, lex_grammar_tokens};
    use crate::syntax::parser::Parser;

    /// `repeat` is recognized by lookahead, not by a reserved word, so a head
    /// the lookahead missed would still parse, as a command or an expression.
    /// Every sentence of the production must come out as the sugar statement.
    #[test]
    fn every_repeat_sentence_of_the_grammar_parses_as_a_repeat_statement() {
        let mut sentences = 0;
        for (depth, seed, source) in sentences_of("repeat_statement") {
            sentences += 1;
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(
                parsed.diagnostics.is_empty(),
                "depth {depth} seed {seed}: {}\n{source}",
                parsed.diagnostics[0].message
            );
            assert!(
                first_statement_is(&parsed.arena, SugarForm::Repeat),
                "depth {depth} seed {seed} is not a repeat statement:\n{source}"
            );
        }
        assert!(sentences > 300, "only {sentences} sentences");
    }

    /// A `tempdir` scope is recognized by lookahead, and `tempdir NAME at
    /// PATH {` has no word between its path and its body, so its head ends
    /// where the head of `for NAME in PATH {` ends. The grammar is looser than
    /// the parser about that end for both, so the two are held to each other:
    /// a sentence with a path parses exactly when the same text parses as its
    /// `for` twin, and every sentence that parses is a `tempdir` scope with
    /// the head that was written.
    #[test]
    fn every_tempdir_sentence_parses_as_a_scope_where_its_for_twin_parses() {
        use crate::syntax::arena::ArenaExprKind;
        let mut fresh = 0;
        let mut at_a_path = 0;
        for (depth, seed, source) in sentences_of("tempdir_scope") {
            let rest = source
                .strip_prefix("tempdir ")
                .unwrap_or_else(|| panic!("depth {depth} seed {seed} has no head:\n{source}"));
            let (name, rest) = rest
                .split_once(' ')
                .unwrap_or_else(|| panic!("depth {depth} seed {seed} has no name:\n{source}"));
            let path = rest.strip_prefix("at ");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            if let Some(path) = path {
                let twin = format!("for {name} in {path}");
                let parsed_twin = Parser::parse_source_arena_only(SourceId::new(0), &twin);
                assert_eq!(
                    parsed.diagnostics.is_empty(),
                    parsed_twin.diagnostics.is_empty(),
                    "depth {depth} seed {seed}:\n{source}"
                );
                if !parsed.diagnostics.is_empty() {
                    continue;
                }
            } else {
                assert!(
                    parsed.diagnostics.is_empty(),
                    "depth {depth} seed {seed}: {}\n{source}",
                    parsed.diagnostics[0].message
                );
            }
            let first = parsed.arena.statement_ids().next().expect("one statement");
            let ArenaStmtKind::Expr(scope) = parsed.arena.arena.stmt(first).kind else {
                panic!("depth {depth} seed {seed} is not an expression statement:\n{source}");
            };
            let ArenaExprKind::TempDirScope {
                path: scope_path, ..
            } = parsed.arena.arena.expr(scope).kind
            else {
                panic!("depth {depth} seed {seed} is not a tempdir scope:\n{source}");
            };
            assert_eq!(
                scope_path.is_some(),
                path.is_some(),
                "depth {depth} seed {seed}:\n{source}"
            );
            if path.is_some() {
                at_a_path += 1;
            } else {
                fresh += 1;
            }
        }
        assert!(fresh > 100, "only {fresh} fresh sentences");
        assert!(at_a_path > 100, "only {at_a_path} sentences with a path");
    }

    /// `atomically replace` is recognized by its first two words, so a head
    /// the parser read differently from the production would fail to parse.
    /// Every sentence of the production must come out as the sugar statement.
    #[test]
    fn every_atomically_sentence_of_the_grammar_parses_as_an_atomically_statement() {
        let mut sentences = 0;
        for (depth, seed, source) in sentences_of("atomically_statement") {
            sentences += 1;
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(
                parsed.diagnostics.is_empty(),
                "depth {depth} seed {seed}: {}\n{source}",
                parsed.diagnostics[0].message
            );
            assert!(
                first_statement_is(&parsed.arena, SugarForm::Atomically),
                "depth {depth} seed {seed} is not an atomically statement:\n{source}"
            );
        }
        assert!(sentences > 100, "only {sentences} sentences");
    }

    /// `within DURATION {` is recognized by its whole head, so every
    /// sentence of the production must parse, as the scope and nothing else.
    #[test]
    fn every_within_sentence_of_the_grammar_parses_as_a_within_scope() {
        use crate::syntax::arena::{ArenaExprKind, ContextScopeKind};
        let mut sentences = 0;
        for (depth, seed, source) in sentences_of("within_scope") {
            sentences += 1;
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(
                parsed.diagnostics.is_empty(),
                "depth {depth} seed {seed}: {}\n{source}",
                parsed.diagnostics[0].message
            );
            let first = parsed.arena.statement_ids().next().expect("one statement");
            let arena = &parsed.arena.arena;
            let is_scope = matches!(
                arena.stmt(first).kind,
                ArenaStmtKind::Expr(expr) if matches!(
                    arena.expr(expr).kind,
                    ArenaExprKind::ContextScope { kind: ContextScopeKind::Within, .. }
                )
            );
            assert!(is_scope, "depth {depth} seed {seed} is not a within scope:\n{source}");
        }
        assert!(sentences > 100, "only {sentences} sentences");
    }

    fn first_statement_is(program: &ArenaProgram, expected: SugarForm) -> bool {
        let first = program.statement_ids().next().expect("one statement");
        matches!(
            program.arena.stmt(first).kind,
            ArenaStmtKind::Sugar { form, .. } if form == expected
        )
    }

    /// Generated sentences of one production, with the depth and seed that
    /// made each.
    fn sentences_of(production: &str) -> Vec<(u32, u64, String)> {
        let grammar = grammar();
        let recognizer = Recognizer::new(grammar);
        let mut generator = Generator::new(grammar);
        let mut sentences = Vec::new();
        for depth in [3, 5, 8] {
            for seed in 0..300 {
                let Some(source) = generator.sentence(production, seed, depth) else {
                    continue;
                };
                // A candidate that misses a lookahead or lexes differently is
                // not a sentence of the grammar.
                let Some(tokens) = lex_grammar_tokens(&source) else {
                    continue;
                };
                if recognizer.recognize(&tokens).is_err() {
                    continue;
                }
                sentences.push((depth, seed, source));
            }
        }
        sentences
    }

    /// `exit` is recognized where a command named `exit` would be read, so a
    /// status that begins like an operator or an assignment leaves an
    /// ordinary statement. Either way a sentence of the production parses,
    /// bare or under a postfix guard.
    #[test]
    fn every_exit_sentence_of_the_grammar_parses() {
        let grammar = grammar();
        let recognizer = Recognizer::new(grammar);
        let mut generator = Generator::new(grammar);
        let mut sentences = 0;
        let mut exits = 0;
        for depth in [3, 5, 8] {
            for seed in 0..300 {
                let Some(source) = generator.sentence("exit_statement", seed, depth) else {
                    continue;
                };
                let Some(tokens) = lex_grammar_tokens(&source) else {
                    continue;
                };
                if recognizer.recognize(&tokens).is_err() {
                    continue;
                }
                sentences += 1;
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(
                    parsed.diagnostics.is_empty(),
                    "depth {depth} seed {seed}: {}\n{source}",
                    parsed.diagnostics[0].message
                );
                let first = parsed.arena.statement_ids().next().expect("one statement");
                let arena = &parsed.arena.arena;
                let guarded = match arena.stmt(first).kind {
                    ArenaStmtKind::Sugar {
                        form: SugarForm::When | SugarForm::Unless,
                        operands,
                        ..
                    } => arena.sugar_operands(operands).iter().find_map(|operand| {
                        match operand {
                            crate::syntax::arena::ArenaSugarOperand::Stmt(inner) => Some(*inner),
                            _ => None,
                        }
                    }),
                    _ => None,
                };
                exits += usize::from(matches!(
                    arena.stmt(guarded.unwrap_or(first)).kind,
                    ArenaStmtKind::Exit(_)
                ));
            }
        }
        assert!(sentences > 300, "only {sentences} sentences");
        assert!(exits * 2 > sentences, "only {exits} of {sentences} are exit statements");
    }

    /// `fail` is recognized where a command named `fail` would be read, so a
    /// message that begins like an operator leaves an ordinary statement.
    /// Either way a sentence of the production parses, bare or under a
    /// postfix guard.
    #[test]
    fn every_fail_sentence_of_the_grammar_parses() {
        let mut sentences = 0;
        let mut fails = 0;
        for (depth, seed, source) in sentences_of("fail_statement") {
            sentences += 1;
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(
                parsed.diagnostics.is_empty(),
                "depth {depth} seed {seed}: {}\n{source}",
                parsed.diagnostics[0].message
            );
            let first = parsed.arena.statement_ids().next().expect("one statement");
            let arena = &parsed.arena.arena;
            let guarded = match arena.stmt(first).kind {
                ArenaStmtKind::Sugar {
                    form: SugarForm::When | SugarForm::Unless,
                    operands,
                    ..
                } => arena
                    .sugar_operands(operands)
                    .iter()
                    .find_map(|operand| match operand {
                        crate::syntax::arena::ArenaSugarOperand::Stmt(inner) => Some(*inner),
                        _ => None,
                    }),
                _ => None,
            };
            fails += usize::from(matches!(
                arena.stmt(guarded.unwrap_or(first)).kind,
                ArenaStmtKind::Sugar {
                    form: SugarForm::Fail,
                    ..
                }
            ));
        }
        assert!(sentences > 300, "only {sentences} sentences");
        assert!(fails * 2 > sentences, "only {fails} of {sentences} are fail statements");
    }

    #[test]
    fn the_grammar_recognizes_written_sugar_statements() {
        assert_grammar_recognizes(&[
            include_str!("../../../tests/xsh/repeat.xsh"),
            include_str!("../../../docs/snippets/spec/45-repeat.xsh"),
            include_str!("../../../tests/xsh/tempdir.xsh"),
            include_str!("../../../docs/snippets/spec/60-tempdir.xsh"),
            include_str!("../../../tests/xsh/exit.xsh"),
            include_str!("../../../docs/snippets/spec/49-exit.xsh"),
            include_str!("../../../tests/xsh/fail.xsh"),
            include_str!("../../../docs/snippets/spec/61-fail.xsh"),
            include_str!("../../../docs/snippets/spec/61-fail-because.xsh"),
            include_str!("../../../tests/xsh/atomically.xsh"),
            include_str!("../../../docs/snippets/spec/61-atomically.xsh"),
            include_str!("../../../tests/xsh/within.xsh"),
            include_str!("../../../docs/snippets/spec/63-within.xsh"),
        ]);
    }

    /// `try run...` is a primary of its own, read by the parser before the
    /// `try` block.
    #[test]
    fn the_grammar_recognizes_written_captured_run_forms() {
        assert_grammar_recognizes(&[
            include_str!("../../../tests/xsh/try-run.xsh"),
            include_str!("../../../docs/snippets/spec/64-try-run.xsh"),
        ]);
    }

    fn assert_grammar_recognizes(sources: &[&str]) {
        let recognizer = Recognizer::new(grammar());
        for source in sources {
            let tokens = lex_grammar_tokens(source).expect("lexes");
            for part in top_level_parts(&tokens) {
                if let Err(rejection) = recognizer.recognize(part) {
                    let near: Vec<&str> = part
                        [rejection.token.saturating_sub(4)..(rejection.token + 3).min(part.len())]
                        .iter()
                        .map(|token| token.text)
                        .collect();
                    panic!(
                        "not a grammar sentence near {near:?}; expected {}",
                        rejection.expected.join(" ")
                    );
                }
            }
        }
    }
}
