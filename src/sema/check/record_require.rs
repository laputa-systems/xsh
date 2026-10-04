use super::{Checker, Diagnostic, Label, Name, QualifiedName, Span, Type};
use crate::diagnostic::{DiagnosticCode, FixHint};
use crate::sema::constants::LiteralConstant;
use crate::syntax::grouping;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaTypeDefBody, ArenaTypeExprTag, ExprId};
use std::collections::BTreeMap;

impl Checker {
    pub(super) fn check_removed_record_require_arena(
        &mut self, arena: &ArenaProgram, source: &str, callee: ExprId,
        args: &[ArenaCallArg], span: Span,
    ) -> bool {
        let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind else { return false; };
        let ArenaExprKind::Ident(module) = arena.arena.expr(base).kind else { return false; };
        if module != "record" || name != "require" || self.lookup(module).is_some()
            || self.user_modules.contains_key("record")
            || self.qualified_procs.contains_key(&QualifiedName::new(module, name))
            || self.qualified_pures.contains_key(&QualifiedName::new(module, name))
            || self.qualified_streams.contains_key(&QualifiedName::new(module, name))
        { return false; }
        let types = args.iter().map(|arg|
            self.check_call_arg_arena(arena, source, &arg.kind, None)).collect::<Vec<_>>();
        let mut diagnostic = Diagnostic::error(
            "`record.require` was removed; declare a named schema and use `.require(Schema)`; optional keys, callable contracts, and dynamic policies need explicit application validation",
        ).with_code(DiagnosticCode::CheckRemovedRecordRequire)
            .with_label(Label::primary(span, "removed string contract API"));
        if let Some(replacement) = self.record_require_identity_migration(arena, source, args, &types, span) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span, "validate the existing named schema", replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
        true
    }

    // A safe automatic edit cannot change a possible legacy failure or an opaque
    // record accessor. Restrict it to plain values whose required fields are
    // already proved, and whose selected schema performs identity checks.
    fn record_require_identity_migration(
        &self, arena: &ArenaProgram, source: &str, args: &[ArenaCallArg],
        types: &[Type], span: Span,
    ) -> Option<String> {
        let [receiver_arg, required_arg] = args else { return None; };
        let ArenaCallArgKind::Positional(receiver) = receiver_arg.kind else { return None; };
        let required = match required_arg.kind {
            ArenaCallArgKind::Positional(value) => value,
            ArenaCallArgKind::Named { name, value, .. } if name == "required" => value,
            _ => return None,
        };
        if source.get(span.start()..span.end())?.contains('#') { return None; }
        let is_plain = matches!(arena.arena.expr(receiver).kind, ArenaExprKind::Record(_))
            || self.prepared_constants.analyze_expression(&arena.arena, receiver)
                .is_some_and(|value| matches!(value, LiteralConstant::Record(_)))
            || matches!(arena.arena.expr(receiver).kind, ArenaExprKind::Call { callee, .. }
                if self.record_constructors.resolve_call(&arena.arena, callee, self.current_namespace).is_some());
        if !is_plain { return None; }
        let Type::Record(actual) = types.first()? else { return None; };
        let LiteralConstant::Record(required) = self.prepared_constants.analyze_expression(&arena.arena, required)? else { return None; };
        if required.is_empty() { return None; }
        let mut fields = BTreeMap::new();
        for (name, value) in required.iter() {
            let LiteralConstant::Str(value) = value else { return None; };
            // This finite migration table is not an executable type grammar.
            let ty = match value.as_ref() {
                "Str" => Type::Str, "Int" => Type::Int, "Float" => Type::Float,
                "Bool" => Type::Bool, "Duration" => Type::Duration,
                "Bytes" => Type::Bytes, "Digest" => Type::Digest, "Regex" => Type::Regex,
                _ => return None,
            };
            if actual.get(name) != Some(&ty) { return None; }
            fields.insert(*name, ty);
        }
        let expected = Type::Record(fields);
        let mut names = self.type_defs.keys().copied().collect::<Vec<Name>>();
        names.sort_by_key(|name| name.as_str());
        let schema = names.into_iter().find(|name| {
            let Some(definition) = self.record_constructors.definition(self.current_namespace, *name) else { return false; };
            if self.record_constructors.schema_type(&arena.arena, definition) != expected { return false; }
            let ArenaTypeDefBody::RecordSchema(fields) = arena.arena.type_def(definition).body else { return false; };
            arena.arena.schema_fields(fields).iter().all(|field| {
                if arena.arena.type_expr_tags[field.ty.index()] != ArenaTypeExprTag::Named { return false; }
                let spelling = Name::from_symbol(crate::symbol::Symbol::from_raw(arena.arena.type_expr_data[field.ty.index()].lhs));
                matches!(required.get(&field.name), Some(LiteralConstant::Str(value)) if spelling.as_str().as_str() == value.as_ref())
            })
        })?;
        let receiver_span = arena.arena.expr(receiver).span;
        let text = source.get(receiver_span.start()..receiver_span.end())?;
        let receiver_context = grouping::Context {
            slot: grouping::Slot::Postfix { dotted: false },
            ..grouping::Context::open(grouping::Follow::adjacent(grouping::FollowToken::Require))
        };
        Some(if grouping::needs_parens(&arena.arena, source, receiver, receiver_context) {
            format!("({text}).require({schema})")
        } else {
            format!("{text}.require({schema})")
        })
    }
}
