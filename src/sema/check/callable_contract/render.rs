use super::{CheckedCallableContract, SchemaIdentity};
use crate::sema::scheme_callable::SchemeCallableTable;
use crate::syntax::arena::TypeDefId;
use crate::sema::constants::{SchemaComponent, SchemaExpectation};
use crate::sema::schemes::{
    CallableMonotype, SchemeParameter, SchemeRowParameterId, SchemeType,
    SchemeTypeParameterId, TypeScheme,
};
use crate::sema::types::{CallableType, ModuleExportType, Type};
use crate::sema::validated::Validation;
use crate::syntax::node::Effect;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Default)]
struct BinderNames {
    types: BTreeMap<SchemeTypeParameterId, usize>,
    effects: BTreeMap<crate::sema::schemes::SchemeEffectParameterId, usize>,
    rows: BTreeMap<SchemeRowParameterId, usize>,
}

impl BinderNames {
    fn from_scheme(scheme: &TypeScheme) -> Self {
        let mut names = Self::default();
        for (index, &id) in scheme.callable().requirements().effect_parameters.iter().enumerate() {
            names.effects.insert(id, index);
        }
        for parameter in scheme.params() {
            names.visit_scheme_type(&parameter.ty);
        }
        names.visit_scheme_type(scheme.return_type());
        for wrapper in &scheme.callable().requirements().wrappers {
            for ty in wrapper.types() { names.visit_scheme_type(ty); }
        }
        for operation in &scheme.callable().requirements().operations {
            for ty in operation.types() { names.visit_scheme_type(ty); }
        }
        for (source, iteration) in &scheme.callable().requirements().iteration_parameters {
            names.visit_scheme_type(&SchemeType::Parameter(*source));
            for ty in iteration.types() { names.visit_scheme_type(ty); }
        }
        names
    }

    fn visit_scheme_type(&mut self, ty: &SchemeType) {
        match ty {
            SchemeType::Ground(_) | SchemeType::SchemeCallable(_) => {}
            SchemeType::Parameter(id) => {
                let next = self.types.len();
                self.types.entry(*id).or_insert(next);
            }
            SchemeType::Callable { monotype, .. } => self.visit_monotype(monotype),
            SchemeType::List(item)
            | SchemeType::Stream(item)
            | SchemeType::Set(item)
            | SchemeType::Optional(item)
            | SchemeType::Validated(_, item) => self.visit_scheme_type(item),
            SchemeType::Map(key, value) | SchemeType::Result(key, value) => {
                self.visit_scheme_type(key);
                self.visit_scheme_type(value);
            }
            SchemeType::Record { fields, tail } => {
                for field in fields.values() {
                    self.visit_scheme_type(field);
                }
                if let crate::sema::schemes::SchemeRowTail::Parameter(id) = tail {
                    let next = self.rows.len();
                    self.rows.entry(*id).or_insert(next);
                }
            }
            SchemeType::Union(members) => {
                for member in members {
                    self.visit_scheme_type(member);
                }
            }
        }
    }

    fn visit_monotype(&mut self, monotype: &CallableMonotype) {
        for parameter in monotype.params() {
            self.visit_scheme_type(&parameter.ty);
        }
        self.visit_scheme_type(monotype.return_type());
    }

    fn type_name(&self, id: SchemeTypeParameterId) -> String {
        format!("T{}", self.types[&id])
    }

    fn row_name(&self, id: SchemeRowParameterId) -> String {
        format!("R{}", self.rows[&id])
    }

    fn quantifiers(&self, scheme: &TypeScheme) -> Vec<String> {
        let mut types = self
            .types
            .iter()
            .map(|(id, index)| {
                let name = self.type_name(*id);
                let name = if scheme.callable().requirements().map_key_parameters.contains(id) {
                    format!("{name}: MapKey")
                } else {
                    name
                };
                (*index, name)
            })
            .collect::<Vec<_>>();
        types.sort_by_key(|(index, _)| *index);
        let mut binders = types
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>();
        let mut rows = self
            .rows
            .iter()
            .map(|(id, index)| (*index, *id))
            .collect::<Vec<_>>();
        rows.sort_by_key(|(index, _)| *index);
        for (_, id) in rows {
            let name = self.row_name(id);
            let parameter = scheme
                .row_parameters()
                .iter()
                .find(|parameter| parameter.id == id)
                .expect("checked callable row binder");
            let lacks = parameter
                .lacks
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            binders.push(if lacks.is_empty() {
                name
            } else {
                format!("{name}\\{{{}}}", lacks.join(", "))
            });
        }
        let mut effects = self.effects.values().copied().collect::<Vec<_>>();
        effects.sort_unstable();
        binders.extend(effects.into_iter().map(|index| format!("E{index}")));
        binders
    }
}

impl fmt::Display for CheckedCallableContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.symbol_owner.with_current(|| {
            let names = BinderNames::from_scheme(&self.scheme);
            let mut context = CallableRenderContext {
                table: self.scheme_callables.as_deref(),
                labels: Default::default(),
                active: Default::default(),
            };
            f.write_str(&render_scheme(&self.scheme, &names, &self.schema_identities, &mut context))
        })
    }
}

struct CallableRenderContext<'a> {
    table: Option<&'a SchemeCallableTable>,
    labels: BTreeMap<crate::sema::scheme_callable::SchemeCallableId, usize>,
    active: std::collections::BTreeSet<crate::sema::scheme_callable::SchemeCallableId>,
}

fn render_scheme(scheme: &TypeScheme, names: &BinderNames, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
    let mut parts = Vec::new();
    let quantifiers = names.quantifiers(scheme);
    if !quantifiers.is_empty() {
        parts.push(format!("forall {}.", quantifiers.join(", ")));
    }
    parts.push(format!(
        "{}({}) -> {}",
        callable_kind(scheme.kind()),
        scheme
            .params()
            .iter()
            .map(|parameter| render_parameter(parameter, names, identities, context))
            .collect::<Vec<_>>()
            .join(", "),
        render_scheme_type(scheme.return_type(), names, identities, context)
    ));
    if let Some(schema) = scheme.return_schema() {
        parts.push(format!("return-schema={}", render_schema(schema, identities, context)));
    }
    for (source, iteration) in &scheme.callable().requirements().iteration_parameters {
        let item = render_scheme_type(&iteration.item, names, identities, context);
        let failure = match &iteration.failure {
            crate::sema::schemes::IterationFailure::Infallible => "infallible".to_owned(),
            crate::sema::schemes::IterationFailure::Propagates(error) => format!("error={}", render_scheme_type(error, names, identities, context)),
        };
        let source_kind = match iteration.source_kind {
            crate::sema::schemes::IterationSourceKind::Direct => "direct-iterable",
            crate::sema::schemes::IterationSourceKind::DirectOrResult => "iterable",
        };
        parts.push(format!("{source_kind}({}, item={item}, {failure})", names.type_name(*source)));
    }
    parts.push(format!("effects={}", render_bound_effects(scheme.effects().contract(), names)));
    for operation in &scheme.callable().requirements().operations {
        let inputs = operation.inputs().map(|ty| render_scheme_type(ty, names, identities, context)).collect::<Vec<_>>().join(", ");
        let result = render_scheme_type(operation.result(), names, identities, context);
        parts.push(match operation {
            crate::sema::sealed_operation::SealedOperation::Binary(operation) => format!("arithmetic({:?}, {inputs}) -> {result}", operation.operator),
            crate::sema::sealed_operation::SealedOperation::Index { syntax, constant_key, .. } => {
                let literal = match syntax { crate::sema::sealed_operation::IndexSyntax::Ordinary => String::new(), crate::sema::sealed_operation::IndexSyntax::NegativeLiteral(distance) => format!(", negative-literal={distance}") };
                let field = constant_key.map(|name| format!(", field={:?}", name.as_str().as_str())).unwrap_or_default();
                format!("index({inputs}{literal}{field}) -> {result}")
            }
            crate::sema::sealed_operation::SealedOperation::Slice { bounds, .. } => format!("slice({bounds:?}, {inputs}) -> {result}"),
            crate::sema::sealed_operation::SealedOperation::RecordOverride { overrides, .. } => {
                let names = overrides.iter().map(|field| field.name.as_str().to_string()).collect::<Vec<_>>().join(", ");
                format!("record-override([{names}], {inputs}) -> {result}")
            }
        });
    }
    for wrapper in &scheme.callable().requirements().wrappers {
        let types = wrapper.types().into_iter().map(|ty| render_scheme_type(ty, names, identities, context)).collect::<Vec<_>>().join(", ");
        let effects = match &wrapper.kind {
            crate::sema::wrapper_operation::WrapperRequirementKind::Field { effects, .. } => format!(", effects={}", render_bound_effects(effects, names)),
            _ => String::new(),
        };
        parts.push(format!("wrapper({:?}, {types}{effects})", wrapper.shape()));
    }
    parts.join(" ")
}

fn render_parameter(parameter: &SchemeParameter, names: &BinderNames, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
    let mut rendered = String::new();
    if parameter.rest {
        rendered.push_str("...");
    }
    rendered.push_str(&parameter.name.to_string());
    rendered.push_str(": ");
    rendered.push_str(&render_scheme_type(&parameter.ty, names, identities, context));
    if parameter.defaulted {
        rendered.push_str(" = default");
    }
    if let Some(schema) = &parameter.schema_expectation {
        rendered.push_str(" schema=");
        rendered.push_str(&render_schema(schema, identities, context));
    }
    rendered
}

fn render_scheme_type(ty: &SchemeType, names: &BinderNames, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
    match ty {
        SchemeType::Ground(ty) => render_type(ty, context),
        SchemeType::SchemeCallable(id) => render_scheme_callable(*id, context),
        SchemeType::Parameter(id) => names.type_name(*id),
        SchemeType::Callable {
            kind,
            monotype,
            effects,
        } => {
            let mut rendered = format!(
                "{}({}) -> {}",
                callable_kind(*kind),
                monotype
                    .params()
                    .iter()
                    .map(|parameter| render_parameter(parameter, names, identities, context))
                    .collect::<Vec<_>>()
                    .join(", "),
                render_scheme_type(monotype.return_type(), names, identities, context)
            );
            if let Some(schema) = monotype.return_schema() {
                rendered.push_str(" return-schema=");
                rendered.push_str(&render_schema(schema, identities, context));
            }
            rendered.push_str(" effects=");
            rendered.push_str(&render_bound_effects(effects, names));
            rendered
        }
        SchemeType::List(item) => format!("List[{}]", render_scheme_type(item, names, identities, context)),
        SchemeType::Map(key, value) if matches!(key.as_ref(), SchemeType::Ground(Type::Str)) => {
            format!("Map[{}]", render_scheme_type(value, names, identities, context))
        }
        SchemeType::Map(key, value) => format!(
            "Map[{}, {}]",
            render_scheme_type(key, names, identities, context),
            render_scheme_type(value, names, identities, context)
        ),
        SchemeType::Stream(item) => format!("Stream[{}]", render_scheme_type(item, names, identities, context)),
        SchemeType::Set(item) => format!("Set[{}]", render_scheme_type(item, names, identities, context)),
        SchemeType::Result(ok, error) => format!(
            "Result[{}, {}]",
            render_scheme_type(ok, names, identities, context),
            render_scheme_type(error, names, identities, context)
        ),
        SchemeType::Optional(item) => format!("{}?", render_scheme_type(item, names, identities, context)),
        SchemeType::Record { fields, tail } => {
            let mut entries = fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", render_scheme_type(ty, names, identities, context)))
                .collect::<Vec<_>>();
            if let crate::sema::schemes::SchemeRowTail::Parameter(id) = tail {
                entries.push(format!("..{}", names.row_name(*id)));
            }
            format!("{{{}}}", entries.join(", "))
        }
        SchemeType::Union(members) => format!(
            "Union[{}]",
            members
                .iter()
                .map(|member| render_scheme_type(member, names, identities, context))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SchemeType::Validated(validation, base) => {
            render_validation(*validation, render_scheme_type(base, names, identities, context))
        }
    }
}

fn render_scheme_callable(id: crate::sema::scheme_callable::SchemeCallableId, context: &mut CallableRenderContext<'_>) -> String {
    use crate::sema::constraints::MAX_TYPE_DEPTH;
    if let Some(label) = context.labels.get(&id) { return format!("callable#{label}"); }
    if context.active.len() >= MAX_TYPE_DEPTH { return "callable{<recursive>}".to_owned(); }
    let Some(table) = context.table else {
        return "callable".to_owned();
    };
    let Ok(relationship) = table.relationship(id) else {
        return "callable{<unavailable>}".to_owned();
    };
    let label = context.labels.len();
    context.labels.insert(id, label);
    context.active.insert(id);
    let rendered = relationship.targets().iter().map(|target| {
        let contract = target.contract();
        contract.symbol_owner.with_current(|| {
            let identity = target.target();
            let name = identity.namespace.map_or_else(|| identity.name.to_string(), |namespace| format!("{namespace}::{}", identity.name));
            let names = BinderNames::from_scheme(contract.scheme());
            format!("{name}: {}", render_scheme(contract.scheme(), &names, &contract.schema_identities, context))
        })
    }).collect::<Vec<_>>().join("; ");
    context.active.remove(&id);
    format!("callable#{label}{{{rendered}}}")
}

fn render_callable_type(kind: &str, signature: &CallableType, context: &mut CallableRenderContext<'_>) -> String {
    let mut rendered = format!(
        "{}({}) -> {}",
        kind,
        signature
            .params
            .iter()
            .map(|parameter| {
                let rest = if parameter.rest { "..." } else { "" };
                let default = if parameter.defaulted { " = default" } else { "" };
                format!(
                    "{rest}{}: {}{default}",
                    parameter.name,
                    render_type(&parameter.ty, context)
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
        render_type(&signature.return_ty, context)
    );
    rendered.push_str(" effects=");
    match signature.effects.concrete() {
        Ok(effects) => rendered.push_str(&render_effects(effects.as_deref())),
        Err(_) => rendered.push_str("<effects need inference>"),
    }
    rendered
}

fn render_type(ty: &Type, context: &mut CallableRenderContext<'_>) -> String {
    match ty {
        Type::SchemeCallable(id) => render_scheme_callable(*id, context),
        Type::List(item) => format!("List[{}]", render_type(item, context)),
        Type::Map(key, value) if matches!(key.as_ref(), Type::Str) => {
            format!("Map[{}]", render_type(value, context))
        }
        Type::Map(key, value) => {
            format!("Map[{}, {}]", render_type(key, context), render_type(value, context))
        }
        Type::Stream(item) => format!("Stream[{}]", render_type(item, context)),
        Type::Set(item) => format!("Set[{}]", render_type(item, context)),
        Type::Result(ok, error) => {
            format!("Result[{}, {}]", render_type(ok, context), render_type(error, context))
        }
        Type::Optional(item) => format!("{}?", render_type(item, context)),
        Type::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", render_type(ty, context)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Module(module) => {
            let openness = if module.exact { "exact" } else { "open" };
            format!(
                "module {openness} {{{}}}",
                module
                    .exports
                    .iter()
                    .map(|(name, export)| render_module_export(*name, export, context))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Type::Union(members) => format!(
            "Union[{}]",
            members.iter().map(|member| render_type(member, context)).collect::<Vec<_>>().join(", ")
        ),
        Type::Callable(callable) => render_callable_type(
            callable_kind(callable.kind),
            &callable.sig,
            context,
        ),
        Type::Validated(validated) => {
            render_validation(validated.validation(), render_type(validated.base(), context))
        }
        Type::Tag(name) | Type::ErrorFamily(name) | Type::ErrorFacet(name) => name.to_string(),
        Type::ErrorVariant { family, variant } => format!("{family}.{variant}"),
        Type::Inference(_) => "<inference>".to_owned(),
        other => other.to_string(),
    }
}

fn render_module_export(name: crate::symbol::Name, export: &ModuleExportType, context: &mut CallableRenderContext<'_>) -> String {
    match export {
        ModuleExportType::Value { ty, optional } => format!(
            "{name}{}: {}",
            if *optional { "?" } else { "" },
            render_type(ty, context)
        ),
        ModuleExportType::Callable { kind, sig, optional } => format!(
            "{name}{}: {}",
            if *optional { "?" } else { "" },
            render_callable_type(callable_kind(*kind), sig, context)
        ),
        ModuleExportType::GenericCallable { kind, optional } => format!(
            "{name}{}: generic {}",
            if *optional { "?" } else { "" },
            callable_kind(*kind)
        ),
    }
}

fn render_validation(validation: Validation, base: String) -> String {
    match validation {
        Validation::NonEmpty => format!("NonEmpty<{base}>"),
        Validation::RelPath => format!("RelPath<{base}>"),
        Validation::Nominal(name) => format!("{name}<{base}>"),
        Validation::Range(range) => format!("{base} range {range}"),
    }
}

fn render_schema(expectation: &SchemaExpectation, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
    let mut entries = expectation
        .instances
        .iter()
        .map(|instance| {
            format!(
                "schema {}[{}]",
                render_schema_identity(&identities[&instance.definition]),
                instance
                    .arguments
                    .iter()
                    .map(|ty| render_type(ty, context))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>();
    entries.extend(expectation.children.iter().map(|(component, child)| {
        format!("{}: {}", render_schema_component(*component), render_schema(child, identities, context))
    }));
    format!("{{{}}}", entries.join(", "))
}

fn render_schema_component(component: SchemaComponent) -> String {
    match component {
        SchemaComponent::Field(name) => format!("field({name})"),
        SchemaComponent::Item => "item".to_owned(),
        SchemaComponent::Value => "value".to_owned(),
        SchemaComponent::Key => "key".to_owned(),
        SchemaComponent::Optional => "optional".to_owned(),
        SchemaComponent::Success => "success".to_owned(),
        SchemaComponent::Error => "error".to_owned(),
    }
}

fn render_bound_effects(effects: &crate::sema::effects::EffectContract<crate::sema::schemes::SchemeEffectParameterId>, names: &BinderNames) -> String {
    match effects {
        crate::sema::effects::EffectContract::Unknown => "unknown".to_owned(),
        crate::sema::effects::EffectContract::Known(expression) => {
            let mut terms = expression.direct().iter().map(|effect| effect.as_str().to_owned()).collect::<Vec<_>>();
            terms.extend(expression.variables().map(|(id, mask)| format!("E{}&{}", names.effects[id], render_effects(Some(&mask.iter().collect::<Vec<_>>())))));
            format!("[{}]", terms.join(", "))
        }
    }
}

fn render_effects(effects: Option<&[Effect]>) -> String {
    let Some(effects) = effects else {
        return "*".to_owned();
    };
    format!(
        "[{}]",
        Effect::ALL
            .into_iter()
            .filter(|effect| effects.contains(effect))
            .map(|effect| effect.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn callable_kind(kind: crate::sema::types::CallableKind) -> &'static str {
    match kind {
        crate::sema::types::CallableKind::Pure => "pure",
        crate::sema::types::CallableKind::Proc => "proc",
        crate::sema::types::CallableKind::Stream => "stream",
    }
}

fn render_schema_identity(identity: &SchemaIdentity) -> String {
    match &identity.module {
        Some(module) => format!("{module}::{}", identity.name),
        None => identity.name.to_string(),
    }
}
