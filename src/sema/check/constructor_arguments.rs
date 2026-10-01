use super::{Checker, ExpressionIdentity, FunctionParamSig, Type};
use crate::sema::arguments::{ArgumentValueSource, ExpandedArgument, StaticArgumentCursor, expand_named_arguments};
use crate::sema::constants::SchemaExpectation;
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaExprOrRun, ArenaProgram};
use std::collections::BTreeMap;

/// A partial named spread receives each destination field's context without
/// promising that this one record supplies the constructor's complete shape.
#[derive(Clone, Debug, Default)]
pub(super) struct ConstructorSpreadContext {
    pub fields: BTreeMap<Name, Type>,
    pub schemas: BTreeMap<Name, SchemaExpectation>,
}

impl Checker {
    pub(super) fn check_expanded_constructor_arguments(
        &mut self, arena: &ArenaProgram, source: &str, args: &[ArenaCallArg],
        parameters: &[FunctionParamSig], _span: Span,
    ) -> Result<Vec<ExpandedArgument>, ()> {
        let mut checked = BTreeMap::new();
        let callable = parameters.iter().map(|parameter| crate::sema::types::CallableParamType {
            name: parameter.name, ty: parameter.ty.clone(), defaulted: parameter.defaulted, rest: parameter.rest,
        }).collect::<Vec<_>>();
        let mut cursor = StaticArgumentCursor::new(&callable);
        for (entry_index, argument) in args.iter().enumerate() {
            let value = super::call_arg_expr_id_arena(&argument.kind);
            let actual = if matches!(argument.kind, ArenaCallArgKind::NamedSpread { .. }) {
                let context = ConstructorSpreadContext {
                    fields: parameters.iter().filter(|parameter| !parameter.rest).map(|parameter| (parameter.name, parameter.ty.clone())).collect(),
                    schemas: parameters.iter().filter_map(|parameter| parameter.schema_expectation.clone().map(|schema| (parameter.name, schema))).collect(),
                };
                let identity: ExpressionIdentity = self.expression_identity(arena, value);
                let previous = self.constructor_spread_contexts.insert(identity, context);
                let actual = self.check_expr_arena(arena, source, value, None);
                if let Some(previous) = previous { self.constructor_spread_contexts.insert(identity, previous); }
                else { self.constructor_spread_contexts.remove(&identity); }
                if let Type::Record(fields) = &actual {
                    for (&field, ty) in fields {
                        let projected = ExpandedArgument { entry_index, name: Some(field), value: ArgumentValueSource::RecordField { record: value, field }, ty: ty.clone(), span: arena.arena.expr(value).span };
                        let _ = cursor.bind(&projected);
                    }
                }
                actual
            } else {
                let name = match argument.kind { ArenaCallArgKind::Named { name, .. } => Some(name), _ => None };
                let splice = matches!(argument.kind, ArenaCallArgKind::Splice { .. });
                let provisional = ExpandedArgument { entry_index, name, value: if splice { ArgumentValueSource::PositionalSplice(value) } else { ArgumentValueSource::Expression(value) }, ty: Type::Unknown, span: arena.arena.expr(value).span };
                let parameter = cursor.bind(&provisional).ok().and_then(|slot| parameters.get(slot));
                let expected = parameter.map(|parameter| if parameter.rest && !splice {
                    match &parameter.ty { Type::List(item) => item.as_ref(), other => other }
                } else { &parameter.ty }).filter(|ty| !matches!(ty, Type::Unknown | Type::Invalid));
                let schema = parameter.and_then(|parameter| if parameter.rest && !splice {
                    parameter.schema_expectation.as_ref().and_then(|schema| schema.children.get(&crate::sema::constants::SchemaComponent::Item)).cloned()
                } else { parameter.schema_expectation.clone() });
                self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(value),
                    expected, schema)
            };
            checked.insert(value, actual);
        }
        expand_named_arguments(arena, args, |expression| checked.get(&expression).cloned()).map_err(|error| {
            self.error(error.span, &error.message, "check.named-spread");
        })
    }
}
