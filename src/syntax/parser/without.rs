//! `without EFFECT, ... { BODY }`: a lexical block under a local negative
//! effect bound.
//!
//! The statement is an ordinary lexical block statement whose block carries
//! the bound (`AstArena::block_effect_bound`). Nothing about it exists at run
//! time; the checker subtracts the listed effects from what the region may do.

use super::{Parser, TokenTag};
use crate::syntax::arena::ArenaProgramBuilder;
use crate::syntax::node::Effect;
use std::str::FromStr;

/// The word that begins the statement. It stays an ordinary identifier
/// everywhere else.
const WITHOUT_WORD: &str = "without";

impl Parser<'_> {
    /// Whether the statement at the cursor is `without EFFECT, ... {`.
    ///
    /// `without` is not reserved, so the statement is recognized by its whole
    /// head, written on one line: the word, one or more effect names separated
    /// by commas, and `{`. Anything else leaves an ordinary statement that
    /// begins with a name spelled `without`.
    pub(super) fn lookahead_is_without(&self) -> bool {
        if !self.current_name().is_some_and(|name| name == WITHOUT_WORD)
            || self.peek_start(1) == Some(self.current_end())
        {
            return false;
        }
        let mut offset = 1;
        loop {
            let is_effect = self.peek_tag(offset) == Some(TokenTag::Ident)
                && self
                    .peek_name(offset)
                    .is_some_and(|name| Effect::from_str(&name.as_str()).is_ok());
            if !is_effect {
                return false;
            }
            match self.peek_tag(offset + 1) {
                Some(TokenTag::Comma) => offset += 2,
                Some(TokenTag::LBrace) => return true,
                _ => return false,
            }
        }
    }

    pub(super) fn parse_without_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        self.bump();
        let mut effects = Vec::new();
        loop {
            let name = self.expect_ident("expected effect name after `without`")?;
            // The lookahead admitted only effect names.
            effects.push(Effect::from_str(&name.as_str()).ok()?);
            if self.at(super::TokenKindMatch::LBrace) {
                break;
            }
            self.expect(
                super::TokenKindMatch::Comma,
                "expected `,` or `{` after an effect name",
            )?;
        }
        let head = self.span(start, self.previous_end());
        let block_start = self.current_start();
        let block = self.parse_block_arena_only(arena)?;
        let end = self.previous_end();
        let effects = arena.push_effects(&effects);
        arena.set_block_effect_bound(block, effects, head);
        let value = arena.push_value_block_expr(block, self.span(block_start, end));
        arena.push_expr_statement(value, self.span(start, end));
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use crate::source::SourceId;
    use crate::syntax::arena::{ArenaExprKind, ArenaStmtKind};
    use crate::syntax::grammar::earley::Recognizer;
    use crate::syntax::grammar::generate::Generator;
    use crate::syntax::grammar::{grammar, lex_grammar_tokens};
    use crate::syntax::node::Effect;
    use crate::syntax::parser::Parser;

    fn bound_effects(source: &str) -> Option<Vec<Effect>> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{}",
            parsed.diagnostics[0].message
        );
        let first = parsed.arena.statement_ids().next().expect("one statement");
        let ArenaStmtKind::Expr(value) = parsed.arena.arena.stmt(first).kind else {
            return None;
        };
        let ArenaExprKind::ValueBlock(block) = parsed.arena.arena.expr(value).kind else {
            return None;
        };
        let bound = parsed.arena.arena.block_effect_bound(block)?;
        Some(parsed.arena.arena.effects(bound.effects).collect())
    }

    #[test]
    fn a_without_statement_is_a_lexical_block_carrying_its_bound() {
        assert_eq!(
            bound_effects("without net, process {\n  print ok\n}\n"),
            Some(vec![Effect::Net, Effect::Process])
        );
        // A plain lexical block carries none.
        assert_eq!(bound_effects("{\n  print ok\n}\n"), None);
    }

    #[test]
    fn without_stays_a_name_outside_the_statement_head() {
        for source in [
            "let without = [1]\nprint ${without.len()}\n",
            "let options = {without: [1], net: 2}\nprint ${options.without.len()}\n",
            "var without = 1\nwithout = 2\nprint $without\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(
                parsed.diagnostics.is_empty(),
                "{source}: {}",
                parsed.diagnostics[0].message
            );
            assert!(
                parsed.arena.arena.block_effect_bounds.is_empty(),
                "{source}"
            );
        }
    }

    /// `without` is recognized by lookahead, not by a reserved word, so a head
    /// the lookahead missed would still parse as something else. Every
    /// sentence of the production must come out as a bounded block.
    #[test]
    fn every_without_sentence_of_the_grammar_parses_as_a_bounded_block() {
        let grammar = grammar();
        let recognizer = Recognizer::new(grammar);
        let mut generator = Generator::new(grammar);
        let mut sentences = 0;
        for depth in [3, 5, 8] {
            for seed in 0..200 {
                let Some(source) = generator.sentence("without_statement", seed, depth) else {
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
                assert_eq!(
                    parsed.arena.arena.block_effect_bounds.is_empty(),
                    false,
                    "depth {depth} seed {seed} is not a without statement:\n{source}"
                );
            }
        }
        assert!(sentences > 100, "only {sentences} sentences");
    }
}
