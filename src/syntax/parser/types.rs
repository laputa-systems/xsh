#![allow(clippy::single_call_fn)]

use super::{ArenaProgramBuilder, Name, Parser, Span, TokenKindMatch, TypeExprId};

impl<'a> Parser<'a> {
    pub(super) fn parse_type_expr(
        &mut self,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<TypeExprId> {
        let start = self.current_start();
        let name = self.expect_ident("expected type name")?;
        let mut ty = match name.as_str() {
            "List" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `List`");
                let inner = self.parse_type_expr(arena)?;
                self.expect(TokenKindMatch::RBracket, "expected `]` after list type");
                let end = self.previous_end();
                arena.push_list_type_expr(inner, self.span(start, end))
            }
            "Map" => {
                self.expect(TokenKindMatch::LBracket, "expected `[` after `Map`");
                let inner = self.parse_type_expr(arena)?;
                self.expect(TokenKindMatch::RBracket, "expected `]` after map type");
                let end = self.previous_end();
                arena.push_map_type_expr(inner, self.span(start, end))
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
        if self.consume(TokenKindMatch::Question).is_some() {
            let end = self.previous_end();
            ty = arena.push_optional_type_expr(ty, self.span(start, end));
        }
        Some(ty)
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
