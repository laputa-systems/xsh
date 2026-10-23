use super::super::{CheckOutput, UserModuleSig};
use crate::sema::scheme_callable::{SchemeCallableError, SchemeCallableRebaser, SchemeCallableTable};
use crate::sema::types::{CallableType, Type};
use std::sync::Arc;

struct Mapper<'a> {
    rebaser: &'a mut SchemeCallableRebaser,
    source: &'a SchemeCallableTable,
}

impl Mapper<'_> {
    fn ty(&mut self, ty: &mut Type) -> Result<(), SchemeCallableError> {
        if ty.contains_scheme_callable() { *ty = self.rebaser.ty(ty, self.source)?; }
        Ok(())
    }

    fn scheme_type(&mut self, source: &crate::sema::schemes::SchemeType) -> Result<crate::sema::schemes::SchemeType, SchemeCallableError> {
        let mut mapped = source.clone();
        let mut failure = None;
        mapped.map_ground_types(&mut |ty| if failure.is_none() { failure = self.ty(ty).err(); });
        if let Some(error) = failure { return Err(error); }
        Ok(mapped)
    }

    fn instance(&mut self, source: &crate::sema::schemes::CallInstantiation) -> Result<crate::sema::schemes::CallInstantiation, SchemeCallableError> {
        let mut failure = None;
        let mapped = source.map_ground_types(&mut |ty| if failure.is_none() { failure = self.ty(ty).err(); });
        if let Some(error) = failure { return Err(error); }
        Ok(mapped)
    }

    fn callable(&mut self, callable: &mut CallableType) -> Result<(), SchemeCallableError> {
        for parameter in &mut callable.params { self.ty(&mut parameter.ty)?; }
        self.ty(&mut callable.return_ty)
    }

    fn schema(&mut self, schema: &mut crate::sema::constants::SchemaExpectation) -> Result<(), SchemeCallableError> {
        let mut failure = None;
        schema.map_ground_types(&mut |ty| if failure.is_none() { failure = self.ty(ty).err(); });
        failure.map_or(Ok(()), Err)
    }

    fn compact_signature(&mut self, signature: &mut super::super::CompactFunctionSig) -> Result<(), SchemeCallableError> {
        for parameter in &mut signature.params { self.ty(&mut parameter.ty)?; }
        for schema in signature.parameter_schemas.iter_mut().flatten() { self.schema(schema)?; }
        self.ty(&mut signature.return_ty)
    }

    fn constructor(&mut self, constructor: &mut crate::sema::constants::CheckedRecordConstructor) -> Result<(), SchemeCallableError> {
        self.ty(&mut constructor.ty)?;
        for argument in &mut constructor.instance.arguments { self.ty(argument)?; }
        Ok(())
    }

    fn family(&mut self, family: &mut super::super::ErrorFamilyInfo) -> Result<(), SchemeCallableError> {
        for variant in family.variants.values_mut() {
            for (_, ty) in &mut variant.fields.0 { self.ty(ty)?; }
        }
        Ok(())
    }

    fn api(&mut self, call: &mut super::super::CheckedApiCall) -> Result<(), SchemeCallableError> {
        for parameter in &mut call.params { self.ty(parameter)?; }
        Ok(())
    }

    fn record_constructors(&mut self, constructors: &mut crate::sema::constants::RecordConstructors) -> Result<(), SchemeCallableError> {
        let mut failure = None;
        let mapped = constructors.map_ground_types(&mut |ty| if failure.is_none() { failure = self.ty(ty).err(); });
        if let Some(error) = failure { return Err(error); }
        *constructors = mapped;
        Ok(())
    }
}

pub(in crate::sema::check) fn module_signature(module: &mut UserModuleSig, rebaser: &mut SchemeCallableRebaser) -> Result<(), SchemeCallableError> {
    let Some(source) = module.scheme_callables.as_ref().map(Arc::clone) else {
        if !module.values.is_empty() || !module.procs.is_empty() || !module.pures.is_empty() || !module.streams.is_empty()
            || module.resolved_types.values().any(Type::contains_scheme_callable)
        { return Err(SchemeCallableError::MissingSourceTable); }
        return Ok(());
    };
    rebaser.import_table(&source)?;
    let mut map = Mapper { rebaser, source: &source };
    for ty in module.values.values_mut().chain(module.resolved_types.values_mut()) { map.ty(ty)?; }
    for signature in module.procs.values_mut().chain(module.pures.values_mut()).chain(module.streams.values_mut()) {
        signature.scheme = map.rebaser.scheme(&signature.scheme, &source)?;
    }
    for variant in module.tag_variants.values_mut() {
        for ty in &mut variant.field_types { map.ty(ty)?; }
    }
    for family in module.error_families.values_mut() { map.family(family)?; }
    module.scheme_callables = Some(map.rebaser.snapshot());
    Ok(())
}

pub(in crate::sema::check) fn output(output: &mut CheckOutput, rebaser: &mut SchemeCallableRebaser) -> Result<(), SchemeCallableError> {
    let source = Arc::clone(&output.scheme_callables);
    rebaser.import_table(&source)?;
    let mut map = Mapper { rebaser, source: &source };
    for ty in output.expr_types.values_mut().chain(output.local_binding_types.values_mut())
        .chain(output.function_return_types.values_mut()).chain(output.parameter_types.values_mut())
        .chain(output.handler_input_types.values_mut()).chain(output.unvalidated_command_vectors.values_mut())
    { map.ty(ty)?; }
    for annotation in &mut output.annotation_facts { map.ty(&mut annotation.ty)?; }
    for stage in output.stream_stage_types.values_mut() { map.ty(&mut stage.input)?; map.ty(&mut stage.output)?; }
    for target in output.requirement_targets.values_mut().chain(output.requirement_expected_targets.values_mut()) {
        map.ty(&mut target.ty)?;
        map.schema(&mut target.context)?;
    }
    for alias in output.static_callable_aliases.values_mut() { map.callable(&mut alias.signature)?; }
    for callable in output.typed_callable_calls.values_mut() { map.callable(&mut Arc::make_mut(callable).sig)?; }
    for projection in output.projections.values_mut() {
        map.ty(&mut projection.value_type)?;
        if let Some(export) = projection.callable.take() {
            let mut module = std::collections::BTreeMap::new();
            module.insert(projection.field, export);
            let mut ty = Type::Module(Arc::new(crate::sema::types::ModuleType::exact(module)));
            map.ty(&mut ty)?;
            let Type::Module(module) = ty else { unreachable!() };
            projection.callable = module.exports.get(&projection.field).cloned();
        }
    }
    for constructor in output.record_constructor_instances.values_mut().chain(output.prepared_constants.record_constructor_instances.values_mut()) { map.constructor(constructor)?; }
    for ty in output.prepared_constants.types.values_mut() { map.ty(ty)?; }
    for ty in output.resolved_type_expressions.annotations.values_mut().chain(output.resolved_type_expressions.templates.values_mut()) { map.ty(ty)?; }
    for instance in output.resolved_type_expressions.instantiations.values_mut() {
        for argument in &mut instance.instance.arguments { map.ty(argument)?; }
        for ty in instance.expressions.values_mut() { map.ty(ty)?; }
    }
    for call in output.api_calls.values_mut() { map.api(call)?; }
    for command in output.module_commands.values_mut() {
        map.api(&mut command.call)?;
        for argument in &mut command.arguments {
            if let super::super::CheckedCommandArgument::Word(ty) = argument { map.ty(ty)?; }
        }
    }
    for declaration in output.checked_declarations.types.values_mut() {
        if let Some(info) = &mut declaration.info {
            match info {
                super::super::CompactTypeDefInfo::Record(fields) => for ty in fields.values_mut() { map.ty(ty)?; },
                super::super::CompactTypeDefInfo::Module(module) => {
                    let mut ty = Type::Module(Arc::clone(module));
                    map.ty(&mut ty)?;
                    let Type::Module(mapped) = ty else { unreachable!() };
                    *module = mapped;
                }
                _ => {}
            }
        }
        for (_, variant) in &mut declaration.variants { for ty in &mut variant.field_types { map.ty(ty)?; } }
    }
    for declaration in output.checked_declarations.errors.values_mut() { map.family(&mut declaration.family)?; }
    for family in output.checked_declarations.builtin_error_families.values_mut() { map.family(family)?; }
    map.record_constructors(&mut output.checked_declarations.record_constructors)?;
    for declaration in output.checked_declarations.functions.values_mut() {
        if let Some(signature) = &mut declaration.signature { map.compact_signature(signature)?; }
        if let Some(scheme) = &mut declaration.scheme {
            *scheme = map.rebaser.scheme(scheme, &source)?;
            if let Some(body) = &declaration.scheme_body_facts {
                let mut failure = None;
                let mapped = body.map_ground_types(Arc::clone(scheme), &mut |ty| if failure.is_none() { failure = map.ty(ty).err(); });
                if let Some(error) = failure { return Err(error); }
                declaration.scheme_body_facts = Some(mapped);
            }
        }
    }
    for instantiation in output.call_instantiations.values_mut() {
        *instantiation = instantiation.try_map(&mut map,
            &mut |map, ty| map.scheme_type(ty),
            &mut |map, relationship, instance| {
                let relationship = map.rebaser.relationship(relationship, map.source, 0)?;
                Ok((relationship, map.instance(instance)?))
            },
            &mut |map, instance| map.instance(instance),
        ).map_err(|error| match error {
            crate::sema::value_materialization::MaterializationMapError::TypeDepth => SchemeCallableError::TypeDepth,
            crate::sema::value_materialization::MaterializationMapError::Mapping(error) => error,
        })?;
    }
    for relationship in output.scheme_callable_calls.values_mut() { *relationship = map.rebaser.relationship(*relationship, &source, 0)?; }
    Ok(())
}
