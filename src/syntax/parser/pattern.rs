use super::{Keyword, Parser, TokenKindMatch, TokenTag};

impl<'a> Parser<'a> {
    pub(super) fn parse_pattern_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        let (first, first_span) = self.parse_type_pattern_arena_only(arena)?;
        if self.consume(TokenKindMatch::Pipe).is_none() {
            return Some((first, first_span));
        }
        let start = first_span.start();
        let (second, second_span) = self.parse_type_pattern_arena_only(arena)?;
        let mut patterns = vec![first, second];
        let mut end = second_span.end();
        while self.consume(TokenKindMatch::Pipe).is_some() {
            let (pattern, pattern_span) = self.parse_type_pattern_arena_only(arena)?;
            patterns.push(pattern);
            end = pattern_span.end();
        }
        let span = self.span(start, end);
        Some((arena.push_pattern_alternation(&patterns, span), span))
    }

    fn parse_type_pattern_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        let (first, first_span, binding) = self.parse_pattern_primary_arena_only(arena)?;
        let Some(binding) = binding else {
            return Some((first, first_span));
        };
        let Some(name) = self.current_name() else {
            return Some((first, first_span));
        };
        if name != "is" {
            return Some((first, first_span));
        }
        self.bump();
        let ty_id = self.parse_type_expr(arena)?;
        let span = self.span(first_span.start(), self.previous_end());
        Some((arena.push_pattern_type(binding, ty_id, span), span))
    }

    fn parse_pattern_primary_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(
        crate::syntax::arena::PatternId,
        crate::source::Span,
        Option<Option<crate::symbol::Name>>,
    )> {
        let span = self.current_span();
        match self.current_tag() {
            TokenTag::Ident | TokenTag::ProcIdent => {
                let name = self
                    .current_name()
                    .expect("identifier token has name payload");
                self.bump();
                if name == "_" {
                    return Some((arena.push_pattern_wildcard(span), span, Some(None)));
                }
                if name == "is" {
                    let facet = self.expect_ident("expected error facet after `is`")?;
                    let span = self.span(span.start(), self.previous_end());
                    return Some((arena.push_pattern_facet(facet, span), span, None));
                }
                if self.consume(TokenKindMatch::Dot).is_some() {
                    let variant = self.expect_ident("expected error variant after `.`")?;
                    let fields = if self.at(TokenKindMatch::LBrace) {
                        self.parse_record_pattern_fields_arena_only(arena)?.0
                    } else {
                        Vec::new()
                    };
                    let span = self.span(span.start(), self.previous_end());
                    return Some((
                        arena.push_pattern_error_variant(name, variant, &fields, span),
                        span,
                        None,
                    ));
                }
                if self.consume(TokenKindMatch::LParen).is_some() {
                    let arg = if self.at(TokenKindMatch::RParen) {
                        None
                    } else {
                        let (first, first_span) = self.parse_pattern_arena_only(arena)?;
                        if self.consume(TokenKindMatch::Comma).is_some() {
                            let mut tuple = vec![first];
                            while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof)
                            {
                                tuple.push(self.parse_pattern_arena_only(arena)?.0);
                                if self.consume(TokenKindMatch::Comma).is_none() {
                                    break;
                                }
                            }
                            let tuple_span = self.span(first_span.start(), self.previous_end());
                            Some(arena.push_pattern_tuple(&tuple, tuple_span))
                        } else {
                            Some(first)
                        }
                    };
                    self.expect(
                        TokenKindMatch::RParen,
                        "expected `)` after constructor pattern",
                    );
                    let span = self.span(span.start(), self.previous_end());
                    return Some((arena.push_pattern_constructor(name, arg, span), span, None));
                }
                Some((
                    arena.push_pattern_binding(name, span),
                    span,
                    Some(Some(name)),
                ))
            }
            TokenTag::Keyword
                if matches!(
                    self.current_keyword(),
                    Some(Keyword::Null | Keyword::True | Keyword::False)
                ) =>
            {
                let expr = self.parse_primary_arena_only(arena)?;
                Some((
                    arena.push_pattern_literal(expr.id, expr.span),
                    expr.span,
                    None,
                ))
            }
            TokenTag::Int
            | TokenTag::Float
            | TokenTag::Duration
            | TokenTag::String
            | TokenTag::Bytes => {
                let expr = self.parse_primary_arena_only(arena)?;
                Some((
                    arena.push_pattern_literal(expr.id, expr.span),
                    expr.span,
                    None,
                ))
            }
            TokenTag::LBrace => {
                let (fields, rest, span) = self.parse_record_pattern_fields_arena_only(arena)?;
                Some((arena.push_pattern_record(&fields, rest, span), span, None))
            }
            _ => {
                self.diagnostic_here("expected pattern", "parse.expected-pattern");
                None
            }
        }
    }

    fn parse_record_pattern_fields_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(
        Vec<(
            crate::symbol::Name,
            crate::syntax::arena::PatternId,
            crate::source::Span,
        )>,
        bool,
        crate::source::Span,
    )> {
        let start = self.current_start();
        self.bump();
        self.skip_newlines();
        let mut fields = Vec::new();
        let mut rest = false;
        while !self.at(TokenKindMatch::RBrace) && !self.at(TokenKindMatch::Eof) {
            if self.at(TokenKindMatch::Dot) && self.peek_tag(1) == Some(TokenTag::Dot) {
                self.bump();
                self.bump();
                rest = true;
            } else {
                let field_start = self.current_start();
                let name = self.expect_ident("expected record pattern field")?;
                let pattern = if self.consume(TokenKindMatch::Colon).is_some() {
                    self.parse_pattern_arena_only(arena)?.0
                } else {
                    arena.push_pattern_binding(name, self.span(field_start, self.previous_end()))
                };
                fields.push((name, pattern, self.span(field_start, self.previous_end())));
            }
            self.skip_newlines();
            if self.consume(TokenKindMatch::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        let end = self
            .expect(TokenKindMatch::RBrace, "expected `}` after record pattern")
            .map(|span| span.end())
            .unwrap_or_else(|| self.previous_end());
        Some((fields, rest, self.span(start, end)))
    }
}
