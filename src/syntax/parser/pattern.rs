use super::{Keyword, Parser, TokenKindMatch, TokenTag};
use crate::diagnostic::DiagnosticCode;

impl<'a> Parser<'a> {
    pub(super) fn parse_pattern_test_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        let (pattern, span) = self.parse_pattern_test_rhs_arena_only(arena)?;
        if matches!(
            arena.ast_arena().pattern(pattern).kind,
            crate::syntax::arena::ArenaPatternKind::Alternation(_)
        ) {
            self.diagnostics.push(
                crate::diagnostic::Diagnostic::error("group alternatives in a pattern test")
                    .with_code(DiagnosticCode::ParsePatternTestAlternation)
                    .with_label(crate::diagnostic::Label::primary(span, "write `(P | Q)`")),
            );
        }
        Some((self.normalize_pattern_test_names(arena, pattern), span))
    }

    pub(super) fn parse_pattern_test_rhs_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        let mut offset = 1;
        while self.matches_terms_at(offset, crate::syntax::grammar::PATTERN_MEMBER, self.current_end()) {
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
                name.push_str(
                    &self
                        .expect_ident("expected pattern name after `.`")?
                        .as_str(),
                );
            }
            let span = self.span(start, self.previous_end());
            return Some((
                arena.push_pattern_test_name(crate::symbol::Name::intern(name), span),
                span,
            ));
        }
        if self.at_inferred_variant_pattern()
            && self.condition_expr
            && self.peek_tag(2) == Some(TokenTag::LBrace)
            && !self.pattern_test_brace_is_payload(2)
        {
            // A target-typed predicate can end immediately before a control
            // body, like a qualified one.
            let start = self.current_start();
            self.bump();
            let variant = self.expect_ident("expected variant name after `.`")?;
            let span = self.span(start, self.previous_end());
            return Some((
                arena.push_pattern_error_variant(
                    crate::symbol::Name::intern(""),
                    variant,
                    &[],
                    span,
                ),
                span,
            ));
        }
        if self.current_name().is_some()
            && matches!(
                self.peek_tag(offset),
                Some(TokenTag::LBracket | TokenTag::Question)
            )
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
            // `{}` is the payload when a body block or the next `with`
            // binding follows it.
            Some(TokenTag::RBrace) => matches!(
                self.peek_tag(field + 1),
                Some(TokenTag::LBrace | TokenTag::Comma)
            ),
            _ => false,
        }
    }

    /// Parses a match arm head through its `=>`: `PATTERN [if GUARD] =>` or
    /// the catch-all `else =>`. `else_arm` is the `else` keyword of an
    /// earlier arm of the same match, and this records the one it parses.
    ///
    /// An `else` arm becomes a wildcard pattern on the keyword, so everything
    /// after the parser treats it as the ordinary catch-all. Nothing can
    /// follow it and it takes no guard, both rejected here: a guarded
    /// catch-all is not a catch-all, and `_ if COND =>` already says it.
    pub(super) fn parse_match_arm_head_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
        else_arm: &mut Option<crate::source::Span>,
    ) -> Option<(
        crate::syntax::arena::PatternId,
        Option<crate::syntax::arena::ExprId>,
        crate::syntax::arena::ArenaArmSpelling,
    )> {
        use crate::syntax::arena::ArenaArmSpelling;
        if else_arm.is_some() {
            self.diagnostic_here(
                "`else` must be the last match arm; this arm is unreachable",
                DiagnosticCode::ParseMatchElseArm,
            );
        }
        let (pattern, mut spelling) = if let Some(span) = self.consume_keyword(Keyword::Else) {
            *else_arm = Some(span);
            (arena.push_pattern_wildcard(span), ArenaArmSpelling::Else)
        } else {
            (
                self.parse_match_arm_pattern_arena_only(arena)?.0,
                ArenaArmSpelling::Pattern,
            )
        };
        let guard = if self.at_keyword(Keyword::If) {
            if spelling == ArenaArmSpelling::Else {
                self.diagnostic_here(
                    "an `else` match arm takes no guard; write `_ if COND =>` for a guarded arm",
                    DiagnosticCode::ParseMatchElseArm,
                );
                // Keep the guard so the rest of the arm still parses, as the
                // guarded wildcard arm the message names.
                spelling = ArenaArmSpelling::Pattern;
            }
            self.bump();
            Some(self.parse_expr_id_arena_only(arena)?)
        } else {
            None
        };
        self.expect(TokenKindMatch::FatArrow, "expected `=>` in match arm");
        Some((pattern, guard, spelling))
    }

    /// `.Name` at the current token: a variant selected by the type of the
    /// matched value. `..` is a rest marker, never a variant.
    pub(super) fn at_inferred_variant_pattern(&self) -> bool {
        self.current_tag() == TokenTag::Dot && self.peek_tag(1) == Some(TokenTag::Ident)
    }

    /// The pattern of a match arm head. A head cannot begin with `.Name`:
    /// a line that begins with `.name` continues the expression on the line
    /// before it, so only the first arm could be written that way.
    pub(super) fn parse_match_arm_pattern_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        if self.at_inferred_variant_pattern() {
            let span = self.span(self.current_start(), self.peek_end(1)?);
            self.diagnostics.push(
                crate::diagnostic::Diagnostic::error(
                    "a match arm head must qualify its variant",
                )
                .with_code(DiagnosticCode::ParseInferredVariantArm)
                .with_label(crate::diagnostic::Label::primary(
                    span,
                    "spell the variant in full, as `Variant`, `module.Variant`, or `Family.Variant`",
                ))
                .with_note(
                    "a line that begins with `.name` continues the line before it, so an arm cannot start with a target-typed variant; inside a pattern, as in `Err(.Name)`, it can",
                ),
            );
        }
        self.parse_pattern_arena_only(arena)
    }

    pub(super) fn parse_pattern_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        self.nested(arena, Self::parse_pattern_alternatives_arena_only)
    }

    fn parse_pattern_alternatives_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        let (first, first_span) = self.parse_alias_pattern_arena_only(arena)?;
        if self.consume(TokenKindMatch::Pipe).is_none() {
            return Some((first, first_span));
        }
        let start = first_span.start();
        let (second, second_span) = self.parse_alias_pattern_arena_only(arena)?;
        let mut patterns = vec![first, second];
        let mut end = second_span.end();
        while self.consume(TokenKindMatch::Pipe).is_some() {
            let (pattern, pattern_span) = self.parse_alias_pattern_arena_only(arena)?;
            patterns.push(pattern);
            end = pattern_span.end();
        }
        let span = self.span(start, end);
        Some((arena.push_pattern_alternation(&patterns, span), span))
    }

    fn parse_alias_pattern_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<(crate::syntax::arena::PatternId, crate::source::Span)> {
        let (mut pattern, mut span) = self.parse_type_pattern_arena_only(arena)?;
        while self.at_ident("as") && !self.at_head_as() {
            self.bump();
            let name_span = self.current_span();
            let name = self.expect_ident("expected a name after pattern alias `as`")?;
            if name == "_" {
                self.diagnostics.push(
                    crate::diagnostic::Diagnostic::error(
                        "pattern alias requires a non-discard name",
                    )
                    .with_code(DiagnosticCode::ParsePatternAliasName)
                    .with_label(crate::diagnostic::Label::primary(
                        name_span,
                        "choose a binding name",
                    )),
                );
                return None;
            }
            span = self.span(span.start(), self.previous_end());
            pattern = arena.push_pattern_alias(pattern, name, name_span, span);
        }
        Some((pattern, span))
    }

    pub(super) fn normalize_pattern_test_names(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
        pattern: crate::syntax::arena::PatternId,
    ) -> crate::syntax::arena::PatternId {
        use crate::syntax::arena::ArenaPatternKind;
        let node = arena.ast_arena().pattern(pattern).clone();
        let span = arena.ast_arena().span(node.span);
        match node.kind {
            ArenaPatternKind::Binding(name) => arena.push_pattern_test_name(name, span),
            // A target-typed `.Name` has no spelled type to test by name.
            ArenaPatternKind::ErrorVariant {
                family,
                variant,
                fields,
            } if fields.len == 0 && !family.as_str().is_empty() => arena.push_pattern_test_name(
                crate::symbol::Name::intern(format!("{family}.{variant}")),
                span,
            ),
            ArenaPatternKind::Group(child) => {
                let child = self.normalize_pattern_test_names(arena, child);
                arena.push_pattern_group(child, span)
            }
            ArenaPatternKind::Alternation(children) => {
                let children: Vec<_> = arena.ast_arena().pattern_ids(children).collect();
                let children: Vec<_> = children
                    .into_iter()
                    .map(|child| self.normalize_pattern_test_names(arena, child))
                    .collect();
                arena.push_pattern_alternation(&children, span)
            }
            _ => pattern,
        }
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

    /// An f-string in pattern position: literal text and holes. A hole is a
    /// name or `_`, then optionally `:` and a spec; what a spec means is the
    /// checker's to say. Text between two holes is one literal part, and an
    /// empty literal is not stored, so two holes written back to back are
    /// adjacent parts.
    fn parse_text_pattern_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
        span: crate::source::Span,
        raw_literal: bool,
    ) -> Option<crate::syntax::arena::PatternId> {
        use super::InterpolationChunk;
        use crate::diagnostic::{Diagnostic, Label};
        use crate::source::Span;
        let source_id = self.source_id;
        let (chunks, mut diagnostics) = self.quoted_text_chunks(span, true);
        let mut parts = Vec::new();
        // The literal text since the last hole, with the span it started at.
        let mut text: Option<(String, usize, usize)> = None;
        for chunk in chunks {
            match chunk {
                InterpolationChunk::Text { source, offset } => {
                    let decoded = if raw_literal {
                        source.to_owned()
                    } else {
                        let (decoded, decode_diagnostics) =
                            super::literals::decode_interpolation_text_for(
                                source_id, source, span, offset,
                            );
                        diagnostics.extend(decode_diagnostics);
                        decoded
                    };
                    let pending = text.get_or_insert_with(|| (String::new(), offset, offset));
                    pending.0.push_str(&decoded);
                    pending.2 = offset + source.len();
                }
                InterpolationChunk::Expr { source, offset } => {
                    if let Some((literal, start, end)) = text.take()
                        && !literal.is_empty()
                    {
                        let literal_span = Span::new(source_id, start, end);
                        let expr =
                            arena.push_str_expr(&std::sync::Arc::from(literal), literal_span);
                        parts.push(arena.push_pattern_literal(expr, literal_span));
                    }
                    let hole_span = Span::new(source_id, offset, offset + source.len());
                    let (name, spec) = match source.split_once(':') {
                        Some((name, spec)) => (name.trim(), Some(spec)),
                        None => (source.trim(), None),
                    };
                    let is_name = name
                        .chars()
                        .next()
                        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
                        && name
                            .chars()
                            .all(|part| part == '_' || part.is_ascii_alphanumeric())
                        && crate::syntax::token::Keyword::from_ident(name).is_none();
                    if !is_name || spec.is_some_and(|spec| spec.trim().is_empty()) {
                        diagnostics.push(
                            Diagnostic::error("a text pattern hole is a name, optionally with a spec")
                                .with_code(DiagnosticCode::ParseTextPatternHole)
                                .with_label(Label::primary(
                                    hole_span,
                                    "write `{name}`, `{_}`, or `{name:SPEC}`",
                                ))
                                .with_note(
                                    "a hole binds the text it matches; it does not evaluate an expression",
                                ),
                        );
                        continue;
                    }
                    let binding = (name != "_").then(|| crate::symbol::Name::intern(name));
                    let spec = spec.map(|spec| crate::symbol::Name::intern(spec.trim()));
                    parts.push(arena.push_pattern_text_hole(binding, spec, hole_span));
                }
            }
        }
        if let Some((literal, start, end)) = text
            && !literal.is_empty()
        {
            let literal_span = Span::new(source_id, start, end);
            let expr = arena.push_str_expr(&std::sync::Arc::from(literal), literal_span);
            parts.push(arena.push_pattern_literal(expr, literal_span));
        }
        let failed = !diagnostics.is_empty();
        self.diagnostics.extend(diagnostics);
        if failed {
            return None;
        }
        Some(arena.push_pattern_text(&parts, span))
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
            TokenTag::LParen => {
                self.bump();
                while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                    self.bump();
                }
                let (pattern, _) = self.parse_pattern_arena_only(arena)?;
                while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                    self.bump();
                }
                let end = self
                    .expect(TokenKindMatch::RParen, "expected `)` after grouped pattern")?
                    .end();
                let span = self.span(span.start(), end);
                Some((arena.push_pattern_group(pattern, span), span, None))
            }
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
                if self.at(TokenKindMatch::Dot)
                    && !self.matches_terms(crate::syntax::grammar::RANGE_MARKER, self.current_start())
                {
                    self.bump();
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
                    let (family, variant) = if self.at(TokenKindMatch::Dot)
                        && !self.matches_terms(crate::syntax::grammar::RANGE_MARKER, self.current_start())
                    {
                        self.bump();
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
                    let arg = self.parse_constructor_pattern_args_arena_only(arena)?;
                    let span = self.span(span.start(), self.previous_end());
                    return Some((arena.push_pattern_constructor(name, arg, span), span, None));
                }
                Some((
                    arena.push_pattern_binding(name, span),
                    span,
                    Some(Some(name)),
                ))
            }
            TokenTag::Dot if self.at_inferred_variant_pattern() => {
                self.bump();
                let variant = self.expect_ident("expected variant name after `.`")?;
                if self.consume(TokenKindMatch::LParen).is_some() {
                    let arg = self.parse_constructor_pattern_args_arena_only(arena)?;
                    let span = self.span(span.start(), self.previous_end());
                    return Some((
                        arena.push_pattern_constructor(
                            crate::symbol::Name::intern(format!(".{variant}")),
                            arg,
                            span,
                        ),
                        span,
                        None,
                    ));
                }
                let fields = if self.at(TokenKindMatch::LBrace) {
                    self.parse_record_pattern_fields_arena_only(arena)?.0
                } else {
                    Vec::new()
                };
                let span = self.span(span.start(), self.previous_end());
                Some((
                    arena.push_pattern_error_variant(
                        crate::symbol::Name::intern(""),
                        variant,
                        &fields,
                        span,
                    ),
                    span,
                    None,
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
            // A negative number is a literal pattern: `-1 =>`.
            TokenTag::Minus
                if matches!(self.peek_tag(1), Some(TokenTag::Int | TokenTag::Float))
                    && self.peek_start(1) == Some(self.current_end()) =>
            {
                self.bump();
                let magnitude = self.parse_primary_arena_only(arena)?;
                let span = self.span(span.start(), magnitude.span.end());
                let expr =
                    arena.push_unary_expr(crate::syntax::node::UnaryOp::Neg, magnitude.id, span);
                Some((arena.push_pattern_literal(expr, span), span, None))
            }
            TokenTag::FmtString => {
                let raw_literal = self
                    .token_table
                    .string_flags_at(self.index)
                    .expect("formatted string token has flags payload")
                    .raw_literal;
                self.bump();
                let pattern = self.parse_text_pattern_arena_only(arena, span, raw_literal)?;
                Some((pattern, span, None))
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
                while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                    self.bump();
                }
                let mut elements = Vec::new();
                let mut rest = None;
                while !self.at(TokenKindMatch::RBracket) && !self.at(TokenKindMatch::Eof) {
                    if rest.is_some() {
                        self.diagnostic_here(
                            "list rest must occur once, at the end",
                            DiagnosticCode::ParseListPatternRest,
                        );
                        return None;
                    }
                    if self.matches_terms(crate::syntax::grammar::RANGE_MARKER, self.current_start()) {
                        let rest_start = self.current_start();
                        self.bump();
                        self.bump();
                        let name = if matches!(self.current_tag(), TokenTag::Ident | TokenTag::ProcIdent) {
                            Some(self.expect_ident("expected rest binding")?)
                        } else {
                            None
                        };
                        let rest_span = self.span(rest_start, self.previous_end());
                        rest = Some(match name {
                            Some(name) if name != "_" => {
                                arena.push_pattern_binding(name, rest_span)
                            }
                            _ => arena.push_pattern_wildcard(rest_span),
                        });
                    } else {
                        elements.push(self.parse_pattern_arena_only(arena)?.0);
                    }
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                        self.bump();
                    }
                    if self.consume(TokenKindMatch::Comma).is_none() {
                        break;
                    }
                    while matches!(self.current_tag(), TokenTag::Newline | TokenTag::Comment) {
                        self.bump();
                    }
                }
                let end = self
                    .expect(TokenKindMatch::RBracket, "expected `]` after list pattern")?
                    .end();
                let span = self.span(start, end);
                Some((arena.push_pattern_list(&elements, rest, span), span, None))
            }
            TokenTag::LBrace => {
                let (fields, rest, span) = self.parse_record_pattern_fields_arena_only(arena)?;
                Some((arena.push_pattern_record(&fields, rest, span), span, None))
            }
            _ => {
                self.diagnostic_here("expected pattern", DiagnosticCode::ParseExpectedPattern);
                None
            }
        }
    }

    /// The arguments of a constructor pattern after its `(`, through the
    /// `)`: none, one pattern, or several as a tuple.
    fn parse_constructor_pattern_args_arena_only(
        &mut self,
        arena: &mut crate::syntax::arena::ArenaProgramBuilder<'_>,
    ) -> Option<Option<crate::syntax::arena::PatternId>> {
        let arg = if self.at(TokenKindMatch::RParen) {
            None
        } else {
            let (first, first_span) = self.parse_pattern_arena_only(arena)?;
            if self.consume(TokenKindMatch::Comma).is_some() {
                let mut tuple = vec![first];
                while !self.at(TokenKindMatch::RParen) && !self.at(TokenKindMatch::Eof) {
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
        Some(arg)
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
                    if !self.require_label_binding_name(label_tag, label_span) {
                        return None;
                    }
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
