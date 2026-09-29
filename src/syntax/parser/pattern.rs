use super::{Keyword, Parser, TokenKindMatch, TokenTag};

impl<'a> Parser<'a> {
    pub(super) fn parse_pattern_test_rhs_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        let mut offset = 1;
        while self.peek_tag(offset) == Some(TokenTag::Dot) {
            offset += 2;
        }
        if self.condition_expr
            && offset > 1
            && self.peek_tag(offset) == Some(TokenTag::LBrace)
            && !self.pattern_test_brace_is_payload(offset)
        {
            // A qualified predicate can end immediately before a control body.
            // Explicit payload fields retain their colon spelling here.
            let start = self.current_start();
            let mut name = self.current_name()?.to_string();
            self.bump();
            while self.consume(TokenKindMatch::Dot).is_some() {
                name.push('.');
                name.push_str(&self.expect_ident("expected pattern name after `.`")?.as_str());
            }
            let span = self.span(start, self.previous_end());
            return Some((
                arena.push_pattern_test_name(crate::symbol::Name::intern(name), span),
                span,
            ));
        }
        if self.current_name().is_some()
            && matches!(self.peek_tag(offset), Some(TokenTag::LBracket | TokenTag::Question))
        {
            let start = self.current_start();
            let ty = self.parse_type_expr(arena)?;
            let span = self.span(start, self.previous_end());
            Some((arena.push_pattern_type(None, ty, span), span))
        } else {
            self.parse_pattern_arena_only(arena)
        }
    }

    fn pattern_test_brace_is_payload(&self, brace: usize) -> bool {
        let mut field = brace + 1;
        while self.peek_tag(field) == Some(TokenTag::Newline) {
            field += 1;
        }
        match self.peek_tag(field) {
            Some(TokenTag::Ident | TokenTag::ProcIdent) => {
                self.peek_tag(field + 1) == Some(TokenTag::Colon)
            }
            Some(TokenTag::Dot) => self.peek_tag(field + 1) == Some(TokenTag::Dot),
            Some(TokenTag::RBrace) => matches!(
                self.peek_tag(field + 1),
                Some(TokenTag::LBrace | TokenTag::RParen | TokenTag::RBracket | TokenTag::Comma)
            ),
            _ => false,
        }
    }

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
                    let facet = if self.consume(TokenKindMatch::Dot).is_some() {
                        let member = self.expect_ident("expected error facet after `.`")?;
                        crate::symbol::Name::intern(format!("{facet}.{member}"))
                    } else {
                        facet
                    };
                    let span = self.span(span.start(), self.previous_end());
                    return Some((arena.push_pattern_facet(facet, span), span, None));
                }
                if self.consume(TokenKindMatch::Dot).is_some() {
                    let family_or_variant =
                        self.expect_ident("expected error variant after `.`")?;
                    if self.consume(TokenKindMatch::LParen).is_some() {
                        let arg = if self.at(TokenKindMatch::RParen) {
                            None
                        } else {
                            Some(self.parse_pattern_arena_only(arena)?.0)
                        };
                        self.expect(
                            TokenKindMatch::RParen,
                            "expected `)` after constructor pattern",
                        );
                        let span = self.span(span.start(), self.previous_end());
                        return Some((
                            arena.push_pattern_constructor(
                                crate::symbol::Name::intern(format!("{name}.{family_or_variant}")),
                                arg,
                                span,
                            ),
                            span,
                            None,
                        ));
                    }
                    let (family, variant) = if self.consume(TokenKindMatch::Dot).is_some() {
                        let variant = self.expect_ident("expected error variant after `.`")?;
                        (
                            crate::symbol::Name::intern(format!("{name}.{family_or_variant}")),
                            variant,
                        )
                    } else {
                        (name, family_or_variant)
                    };
                    let fields = if self.at(TokenKindMatch::LBrace) {
                        self.parse_record_pattern_fields_arena_only(arena)?.0
                    } else {
                        Vec::new()
                    };
                    let span = self.span(span.start(), self.previous_end());
                    return Some((
                        arena.push_pattern_error_variant(family, variant, &fields, span),
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
            TokenTag::LBracket => {
                let start = self.current_start();
                self.bump();
                while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) { self.bump(); }
                let mut elements = Vec::new();
                let mut rest = None;
                while !self.at(TokenKindMatch::RBracket) && !self.at(TokenKindMatch::Eof) {
                    if rest.is_some() {
                        self.diagnostic_here("list rest must occur once, at the end", "parse.list-pattern-rest");
                        return None;
                    }
                    if self.at(TokenKindMatch::Dot) && self.peek_tag(1) == Some(TokenTag::Dot) {
                        let rest_start = self.current_start();
                        self.bump();
                        self.bump();
                        let name = if matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
                            Some(self.expect_ident("expected rest binding")?)
                        } else { None };
                        let rest_span = self.span(rest_start, self.previous_end());
                        rest = Some(match name {
                            Some(name) if name != "_" => arena.push_pattern_binding(name, rest_span),
                            _ => arena.push_pattern_wildcard(rest_span),
                        });
                    } else {
                        elements.push(self.parse_pattern_arena_only(arena)?.0);
                    }
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) { self.bump(); }
                    if self.consume(TokenKindMatch::Comma).is_none() { break; }
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) { self.bump(); }
                }
                let end = self.expect(TokenKindMatch::RBracket, "expected `]` after list pattern")?.end();
                let span = self.span(start, end);
                Some((arena.push_pattern_list(&elements, rest, span), span, None))
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
                let label_tag = self.current_tag();
                let label_span = self.current_span();
                let name = self.expect_label_name("expected record pattern field")?;
                let pattern = if self.consume(TokenKindMatch::Colon).is_some() {
                    self.parse_pattern_arena_only(arena)?.0
                } else {
                    if !self.require_label_binding_name(label_tag, label_span) { return None; }
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
