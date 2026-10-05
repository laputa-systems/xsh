#![allow(clippy::single_call_fn)]

use super::{ArenaProgramBuilder, Name, Parser, Span, TokenKindMatch, TypeExprId};
use crate::syntax::arena::ArenaCallableTypeExpr;
use crate::syntax::token::Keyword;

impl<'a> Parser<'a> {
    pub(super) fn parse_type_expr(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<TypeExprId> {
        self.nested(arena, Self::parse_type_expr_form)
    }

    fn parse_type_expr_form(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<TypeExprId> {
        let start = self.current_start();
        if self.at_keyword(Keyword::Proc) || self.at_keyword(Keyword::Pure) {
            return self.parse_callable_type_expr(arena);
        }
        let name = self.expect_ident("expected type name")?;
        let mut ty = match name.as_str().as_str() {
            "List" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `List`");
                let inner = self.parse_type_expr(arena)?;
                self.expect(TokenKindMatch::RBracket, "expected `]` after list type");
                let end = self.previous_end();
                arena.push_list_type_expr(inner, self.span(start, end))
            }
            "Map" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `Map`");
                let first = self.parse_type_expr(arena)?;
                let (key, inner) = if self.consume(TokenKindMatch::Comma).is_some() {
                    (Some(first), self.parse_type_expr(arena)?)
                } else {
                    (None, first)
                };
                self.expect(TokenKindMatch::RBracket, "expected `]` after map type");
                let end = self.previous_end();
                arena.push_typed_map_type_expr(key, inner, self.span(start, end))
            }
            "Stream" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `Stream`");
                let inner = self.parse_type_expr(arena)?;
                self.expect(TokenKindMatch::RBracket, "expected `]` after stream type");
                let end = self.previous_end();
                arena.push_stream_type_expr(inner, self.span(start, end))
            }
            "Module" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `Module`");
                let inner = self.parse_type_expr(arena)?;
                self.expect(TokenKindMatch::RBracket, "expected `]` after module type");
                let end = self.previous_end();
                arena.push_module_type_expr(inner, self.span(start, end))
            }
            "Result" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `Result`");
                let ok = self.parse_type_expr(arena)?;
                let err = if self.consume(TokenKindMatch::Comma).is_some() {
                    Some(self.parse_type_expr(arena)?)
                } else {
                    None
                };
                self.expect(TokenKindMatch::RBracket, "expected `]` after result type");
                let end = self.previous_end();
                arena.push_result_type_expr(ok, err, self.span(start, end))
            }
            "Union" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `Union`");
                let mut members = vec![self.parse_type_expr(arena)?];
                while self.consume(TokenKindMatch::Comma).is_some() {
                    members.push(self.parse_type_expr(arena)?);
                }
                self.expect(TokenKindMatch::RBracket, "expected `]` after union members");
                let end = self.previous_end();
                arena.push_union_type_expr(&members, self.span(start, end))
            }
            "Set" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `Set`");
                let inner = self.parse_type_expr(arena)?;
                self.expect(
                    TokenKindMatch::RBracket,
                    "expected `]` after the element type of `Set`",
                );
                let end = self.previous_end();
                arena.push_set_type_expr(inner, self.span(start, end))
            }
            "NonEmpty" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `NonEmpty`");
                let inner = self.parse_type_expr(arena)?;
                self.expect(
                    TokenKindMatch::RBracket,
                    "expected `]` after the element type of `NonEmpty`",
                );
                let end = self.previous_end();
                arena.push_non_empty_type_expr(inner, self.span(start, end))
            }
            _ => {
                if self.consume(TokenKindMatch::Dot).is_some() {
                    let ty_name = self.expect_ident("expected type name after `.`")?;
                    let end = self.previous_end();
                    arena.push_qualified_type_expr(name, ty_name, self.span(start, end))
                } else {
                    let end = self.previous_end();
                    arena.push_named_type_expr(name, self.span(start, end))
                }
            }
        };
        if self.consume(TokenKindMatch::LBracket).is_some() {
            let mut arguments = Vec::new();
            if !self.at(TokenKindMatch::RBracket) {
                arguments.push(self.parse_type_expr(arena)?);
                while self.consume(TokenKindMatch::Comma).is_some() {
                    arguments.push(self.parse_type_expr(arena)?);
                }
            }
            self.expect(
                TokenKindMatch::RBracket,
                "expected `]` after type arguments",
            );
            ty =
                arena.push_applied_type_expr(ty, &arguments, self.span(start, self.previous_end()));
        }
        if self.consume(TokenKindMatch::Question).is_some() {
            let end = self.previous_end();
            ty = arena.push_optional_type_expr(ty, self.span(start, end));
        }
        Some(ty)
    }

    /// `proc(PARAMS) [EFFECTS] -> T` or `pure(PARAMS) -> T`. The return type
    /// is always written, so a trailing `?` belongs to it; an optional
    /// callable is spelled through an alias.
    fn parse_callable_type_expr(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<TypeExprId> {
        let start = self.current_start();
        let pure = self.at_keyword(Keyword::Pure);
        self.bump();
        self.expect(
            TokenKindMatch::LParen,
            "expected `(` after `proc` or `pure` in a callable type",
        );
        let params = self.parse_params_arena_only(arena);
        self.expect(TokenKindMatch::RParen, "expected `)` after parameters");
        let params = arena.push_params(&params);
        let effects = if pure {
            None
        } else {
            self.parse_effect_list()
                .map(|effects| arena.push_effects(&effects))
        };
        self.expect(
            TokenKindMatch::Arrow,
            "expected `->` and a return type in a callable type",
        );
        let return_ty = self.parse_type_expr(arena)?;
        let end = self.previous_end();
        Some(arena.push_callable_type_expr(
            ArenaCallableTypeExpr {
                pure,
                params,
                effects,
                return_ty,
            },
            self.span(start, end),
        ))
    }
}

pub(in crate::syntax::parser) fn result_unit_type_expr(
    arena: &mut ArenaProgramBuilder<'_>,
    span: Span,
) -> TypeExprId {
    let ok = arena.push_named_type_expr(Name::UNIT, span);
    arena.push_result_type_expr(ok, None, span)
}

pub(in crate::syntax::parser) fn unknown_type_expr(
    arena: &mut ArenaProgramBuilder<'_>,
    span: Span,
) -> TypeExprId {
    arena.push_named_type_expr(Name::UNKNOWN, span)
}
