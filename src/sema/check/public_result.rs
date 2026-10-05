//! Exported declarations and module contracts spell the error type of every
//! `Result` they write. `Result[T]` means `Result[T, Error]`; at an API
//! boundary the broad error is part of the contract and has to be visible.

use super::Checker;
use crate::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use crate::source::Span;
use crate::syntax::arena::{
    ArenaProgram, ArenaStmtKind, ArenaTypeDefBody, ArenaTypeExprTag, StmtId, TypeExprId,
};

impl Checker {
    /// Reports each `Result[T]` written in the public surface of `statements`,
    /// the top-level statements of one source file.
    pub(super) fn check_public_result_types(
        &mut self,
        program: &ArenaProgram,
        statements: &[StmtId],
    ) {
        let arena = &program.arena;
        // Source ranges whose written types are public, and return types the
        // parser supplied for a signature that wrote none.
        let mut public = Vec::new();
        let mut supplied = Vec::new();
        for &statement in statements {
            let outer = arena.stmt(statement);
            let (inner, exported) = match outer.kind {
                ArenaStmtKind::Export(inner) => (arena.stmt(inner), true),
                _ => (outer.clone(), false),
            };
            match inner.kind {
                ArenaStmtKind::ProcDef(id) | ArenaStmtKind::PureDef(id) | ArenaStmtKind::StreamDef(id)
                    if exported =>
                {
                    let definition = arena.function_def(id);
                    if definition.return_ty_defaulted {
                        supplied.push(definition.return_ty);
                    }
                    // Annotations inside the body are private to it.
                    let body = arena.span(arena.block(definition.body).span);
                    public.push(inner.span.start()..body.start());
                }
                ArenaStmtKind::TypeDef(id) => {
                    let contract = matches!(
                        arena.type_def(id).body,
                        ArenaTypeDefBody::ModuleContract(_)
                    );
                    if exported || contract {
                        public.push(inner.span.range());
                    }
                }
                ArenaStmtKind::Let { ty: Some(ty), .. } | ArenaStmtKind::Const { ty: Some(ty), .. }
                    if exported =>
                {
                    public.push(arena.type_expr_span(ty).range());
                }
                _ => {}
            }
        }
        let Some(source_id) = statements
            .first()
            .map(|statement| arena.stmt(*statement).span.source_id)
        else {
            return;
        };
        if public.is_empty() {
            return;
        }
        for index in 0..arena.type_expr_tags.len() {
            if arena.type_expr_tags[index] != ArenaTypeExprTag::Result {
                continue;
            }
            let data = arena.type_expr_data[index];
            if TypeExprId::from_optional_raw(data.rhs).is_some() {
                continue;
            }
            let id = TypeExprId::from_index(index);
            let span = arena.type_expr_span(id);
            if span.source_id != source_id
                || span.start() == span.end()
                || supplied.contains(&id)
                || !public
                    .iter()
                    .any(|range| range.start <= span.start() && span.end() <= range.end)
            {
                continue;
            }
            let ok_end = arena
                .type_expr_span(TypeExprId::from_index(data.lhs as usize))
                .end();
            self.diagnostics.push(
                Diagnostic::warning(
                    "a public signature must spell the error type of its Result",
                )
                .with_code(DiagnosticCode::CheckPublicResultError)
                .with_label(Label::primary(
                    span,
                    "write `Result[T, Error]`, or name the error family callers can rely on",
                ))
                .with_fix_hint(FixHint::replacement(
                    Span::new(source_id, ok_end, ok_end),
                    "spell the broad error type this signature already has",
                    ", Error",
                )),
            );
        }
    }
}
