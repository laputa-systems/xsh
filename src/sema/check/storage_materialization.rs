use super::{Checker, PendingCallInstantiation, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::value_materialization::{InstantiationEvidence, Materialization, ValueMaterialization};
use crate::source::Span;
use crate::syntax::arena::{ArenaProgram, ExprId};
use std::sync::Arc;

type PendingPlan = ValueMaterialization<Type, PendingCallInstantiation>;

#[derive(Clone, Copy)]
enum StorageContext<'a> {
    Mutable,
    Expected(&'a Type),
    Opaque,
}

impl StorageContext<'_> {
    fn child(self, expected: Option<&Type>) -> StorageContext<'_> {
        match self {
            Self::Mutable => StorageContext::Mutable,
            Self::Expected(_) => expected.map(StorageContext::Expected).unwrap_or(StorageContext::Opaque),
            Self::Opaque => StorageContext::Opaque,
        }
    }
}

impl Checker {
    pub(super) fn materialize_mutable_storage(&mut self, arena: &ArenaProgram, id: ExprId, actual: Type) -> Type {
        if matches!(actual, Type::Function(_) | Type::SchemeCallable(_)) {
            return self.materialize_callable_value(arena, id, actual);
        }
        self.materialize_storage(arena, id, actual, StorageContext::Mutable)
    }

    pub(super) fn materialize_contextual_storage(&mut self, arena: &ArenaProgram, id: ExprId, actual: Type, expected: &Type) -> Type {
        self.materialize_storage(arena, id, actual, StorageContext::Expected(expected))
    }

    fn materialize_storage(&mut self, arena: &ArenaProgram, id: ExprId, actual: Type, context: StorageContext<'_>) -> Type {
        let span = arena.arena.expr(id).span;
        let plan = match self.storage_materialization_plan(&actual, context, span, 0) {
            Ok(plan) => plan,
            Err(message) => {
                self.error(span, &message, DiagnosticCode::CheckCallableMismatch);
                return Type::Invalid;
            }
        };
        if matches!(plan.shape, Materialization::Keep) { return actual; }
        let destination = plan.destination.clone();
        let previous = self.pending_call_instantiations.insert(
            crate::sema::schemes::InstantiationSite::value(span),
            InstantiationEvidence::Value(Box::new(plan)),
        );
        assert!(previous.is_none(), "a stored value has one owned materialization plan");
        self.record_expr_type(span, destination.clone());
        destination
    }

    fn storage_materialization_plan(
        &mut self,
        actual: &Type,
        context: StorageContext<'_>,
        span: Span,
        depth: usize,
    ) -> Result<PendingPlan, String> {
        if depth > crate::sema::constraints::MAX_TYPE_DEPTH {
            return Err("stored callable materialization exceeds the type depth bound".into());
        }
        let source = self.type_constraints.resolve(actual).map_err(|error| format!("stored callable source is unresolved: {error:?}"))?;
        let keep = || ValueMaterialization { source: source.clone(), destination: source.clone(), shape: Materialization::Keep };
        if matches!(context, StorageContext::Opaque) { return Ok(keep()); }
        let expected = match context {
            StorageContext::Expected(expected) => Some(self.type_constraints.resolve(expected)
                .map_err(|error| format!("stored callable context is unresolved: {error:?}"))?),
            _ => None,
        };
        let expected = expected.as_ref().map(Type::unvalidated);
        if let Type::SchemeCallable(relationship) = &source {
            if expected.is_some_and(|expected| !matches!(expected, Type::Inference(_) | Type::Callable(_) | Type::Optional(_))) {
                return Ok(keep());
            }
            let relationship = *relationship;
            let (destination, instance) = self.fresh_scheme_callable_value(relationship, span)?;
            return Ok(ValueMaterialization {
                source,
                destination,
                shape: Materialization::Callable { relationship, instance: Box::new(instance) },
            });
        }
        if let Type::Validated(validated) = &source {
            let mut plan = self.storage_materialization_plan(validated.base(), context, span, depth + 1)?;
            if matches!(plan.shape, Materialization::Keep) { return Ok(keep()); }
            let destination = crate::sema::validated::ValidatedType::new(validated.validation(), plan.destination)
                .map_err(|error| format!("stored callable materialization changes its validated container: {error}"))?;
            plan.source = source;
            plan.destination = Type::Validated(Box::new(destination));
            return Ok(plan);
        }
        let (destination, shape) = match &source {
            Type::Record(fields) => {
                let expected_fields = match context {
                    StorageContext::Expected(expected) => self.type_constraints.contextual_record_fields(expected)
                        .map_err(|error| format!("stored callable record context is unresolved: {error:?}"))?,
                    _ => None,
                };
                if matches!(context, StorageContext::Expected(_)) && expected_fields.is_none() { return Ok(keep()); }
                let mut plans = std::collections::BTreeMap::new();
                let mut destination = std::collections::BTreeMap::new();
                for (&name, field) in fields.iter() {
                    let plan = self.storage_materialization_plan(field, context.child(expected_fields.as_ref().and_then(|fields| fields.get(&name))), span, depth + 1)?;
                    destination.insert(name, plan.destination.clone());
                    plans.insert(name, plan);
                }
                (Type::Record(Arc::new(destination)), Materialization::Record(plans))
            }
            Type::List(item) => {
                let expected_item = match expected { Some(Type::List(item)) => Some(item.as_ref()), _ => None };
                if matches!(context, StorageContext::Expected(_)) && expected_item.is_none() { return Ok(keep()); }
                let plan = self.storage_materialization_plan(item, context.child(expected_item), span, depth + 1)?;
                (Type::List(Box::new(plan.destination.clone())), Materialization::List(Box::new(plan)))
            }
            Type::Map(key, value) => {
                let expected_value = match expected { Some(Type::Map(_, value)) => Some(value.as_ref()), _ => None };
                if matches!(context, StorageContext::Expected(_)) && expected_value.is_none() { return Ok(keep()); }
                let plan = self.storage_materialization_plan(value, context.child(expected_value), span, depth + 1)?;
                (Type::Map(key.clone(), Box::new(plan.destination.clone())), Materialization::MapValues(Box::new(plan)))
            }
            Type::Optional(value) => {
                let expected_value = match expected { Some(Type::Optional(value)) => Some(value.as_ref()), _ => None };
                if matches!(context, StorageContext::Expected(_)) && expected_value.is_none() { return Ok(keep()); }
                let plan = self.storage_materialization_plan(value, context.child(expected_value), span, depth + 1)?;
                (Type::Optional(Box::new(plan.destination.clone())), Materialization::Optional(Box::new(plan)))
            }
            Type::Result(ok, error) => {
                let expected_parts = match expected { Some(Type::Result(ok, error)) => Some((ok.as_ref(), error.as_ref())), _ => None };
                if matches!(context, StorageContext::Expected(_)) && expected_parts.is_none() { return Ok(keep()); }
                let ok = self.storage_materialization_plan(ok, context.child(expected_parts.map(|(ok, _)| ok)), span, depth + 1)?;
                let error = self.storage_materialization_plan(error, context.child(expected_parts.map(|(_, error)| error)), span, depth + 1)?;
                (Type::Result(Box::new(ok.destination.clone()), Box::new(error.destination.clone())), Materialization::Result { ok: Box::new(ok), error: Box::new(error) })
            }
            _ => return Ok(keep()),
        };
        let changes = match &shape {
            Materialization::Keep => false,
            Materialization::Callable { .. } => true,
            Materialization::Record(fields) => fields.values().any(|plan| !matches!(plan.shape, Materialization::Keep)),
            Materialization::List(plan) | Materialization::MapValues(plan) | Materialization::Optional(plan) => !matches!(plan.shape, Materialization::Keep),
            Materialization::Result { ok, error } => !matches!(ok.shape, Materialization::Keep) || !matches!(error.shape, Materialization::Keep),
        };
        if !changes { return Ok(keep()); }
        Ok(ValueMaterialization { source, destination, shape })
    }
}
