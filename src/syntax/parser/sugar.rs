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
//! - It references each operand exactly once and in source order, so nothing
//!   the user wrote is evaluated twice or out of order. A value the expansion
//!   needs twice is bound once to a local whose name no identifier can spell.
//! - Its root statement carries the surface statement's span, so tracebacks,
//!   traces, and coverage report the statement the user wrote. Every other
//!   node it adds has a span of its own inside the surface statement, distinct
//!   from every other expression, statement, or block span, because checker
//!   facts are keyed by span and two nodes sharing one would overwrite each
//!   other.
//! - Its root is a core control or effect statement, never a declaration,
//!   import, export, or binding: those are collected by scans of statement
//!   lists that do not look inside a surface form.

use super::{Name, Parser, TokenTag};
use crate::diagnostic::DiagnosticCode;
use crate::source::Span;
use crate::syntax::arena::{
    ArenaCallArgInput, ArenaProgramBuilder, ArenaSugarOperand, BlockId, ExprId, StmtId, SugarForm,
};

/// The word that begins a `repeat` statement. It stays an ordinary identifier
/// everywhere else (`repeat` is also a stream stage).
const REPEAT_WORD: &str = "repeat";
/// The word that ends a `repeat` statement's count.
const TIMES_WORD: &str = "times";

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

#[cfg(test)]
mod tests {
    use crate::source::SourceId;
    use crate::syntax::arena::{ArenaStmtKind, SugarForm};
    use crate::syntax::grammar::earley::{Recognizer, top_level_parts};
    use crate::syntax::grammar::generate::Generator;
    use crate::syntax::grammar::{grammar, lex_grammar_tokens};
    use crate::syntax::parser::Parser;

    /// `repeat` is recognized by lookahead, not by a reserved word, so a head
    /// the lookahead missed would still parse, as a command or an expression.
    /// Every sentence of the production must come out as the sugar statement.
    #[test]
    fn every_repeat_sentence_of_the_grammar_parses_as_a_repeat_statement() {
        let grammar = grammar();
        let recognizer = Recognizer::new(grammar);
        let mut generator = Generator::new(grammar);
        let mut sentences = 0;
        for depth in [3, 5, 8] {
            for seed in 0..300 {
                let Some(source) = generator.sentence("repeat_statement", seed, depth) else {
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
                sentences += 1;
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(
                    parsed.diagnostics.is_empty(),
                    "depth {depth} seed {seed}: {}\n{source}",
                    parsed.diagnostics[0].message
                );
                let first = parsed.arena.statement_ids().next().expect("one statement");
                assert!(
                    matches!(
                        parsed.arena.arena.stmt(first).kind,
                        ArenaStmtKind::Sugar {
                            form: SugarForm::Repeat,
                            ..
                        }
                    ),
                    "depth {depth} seed {seed} is not a repeat statement:\n{source}"
                );
            }
        }
        assert!(sentences > 300, "only {sentences} sentences");
    }

    #[test]
    fn the_grammar_recognizes_written_repeat_statements() {
        let recognizer = Recognizer::new(grammar());
        for source in [
            include_str!("../../../tests/xsh/repeat.xsh"),
            include_str!("../../../docs/snippets/spec/45-repeat.xsh"),
        ] {
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
