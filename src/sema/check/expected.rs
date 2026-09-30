use super::{Type, Name};
use crate::sema::constants::{SchemaComponent, SchemaExpectation};
use crate::sema::constraints::TypeConstraints;
use crate::syntax::arena::AstArena;
use std::sync::Arc;

/// Validation keeps its concrete target and application identities independently
/// of runtime values. An expected boundary supplies constraints, never evidence
/// that untrusted input already satisfies them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementTarget {
    pub ty: Type,
    pub context: SchemaExpectation,
    pub name: Arc<str>,
}

pub(super) fn infer_requirement_target(
    arena: &AstArena, expected: Option<&Type>, context: Option<&SchemaExpectation>,
    constraints: &TypeConstraints,
) -> Option<RequirementTarget> {
    let expected = constraints.resolve(expected?).ok()?;
    let context = context?;
    let (ty, context) = match expected {
        Type::Result(ok, _) => (*ok, context.children.get(&SchemaComponent::Success).cloned().unwrap_or_default()),
        ty => (ty, context.clone()),
    };
    if matches!(&ty, Type::Any | Type::DynamicModule | Type::Pure | Type::Proc) || matches!(&ty, Type::Record(fields) if fields.is_empty()) { return None; }
    if !concrete_validation_type(&ty, 0) { return None; }
    let context = resolve_context(context, constraints, 0)?;
    Some(requirement_target(arena, ty, context))
}

pub(super) fn requirement_target(arena: &AstArena, ty: Type, context: SchemaExpectation) -> RequirementTarget {
    let name = if let Some(instance) = context.instances.first() {
        let name: Name = arena.type_def(instance.definition).name;
        if instance.arguments.is_empty() { name.to_string() }
        else { format!("{name}[{}]", instance.arguments.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")) }
    } else { ty.to_string() };
    RequirementTarget { ty, context, name: Arc::from(name) }
}

fn resolve_context(mut context: SchemaExpectation, constraints: &TypeConstraints, depth: usize) -> Option<SchemaExpectation> {
    if depth > 128 { return None; }
    for instance in &mut context.instances {
        for argument in &mut instance.arguments {
            *argument = constraints.resolve(argument).ok()?;
            if !concrete_validation_type(argument, depth + 1) { return None; }
        }
    }
    for child in context.children.values_mut() { *child = resolve_context(child.clone(), constraints, depth + 1)?; }
    Some(context)
}

fn concrete_validation_type(ty: &Type, depth: usize) -> bool {
    if depth > 128 { return false; }
    match ty {
        Type::Unknown | Type::Invalid | Type::Inference(_) => false,
        Type::List(item) | Type::Stream(item) | Type::Optional(item) => concrete_validation_type(item, depth + 1),
        Type::Map(key, value) | Type::Result(key, value) => concrete_validation_type(key, depth + 1) && concrete_validation_type(value, depth + 1),
        Type::Record(fields) => fields.values().all(|field| concrete_validation_type(field, depth + 1)),
        _ => !ty.contains_inference(),
    }
}
