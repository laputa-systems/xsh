use super::{CheckedCallableContract, SchemaIdentity};
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
    rows: BTreeMap<SchemeRowParameterId, usize>,
}

impl BinderNames {
    fn from_scheme(scheme: &TypeScheme) -> Self {
        let mut names = Self::default();
        for parameter in scheme.params() {
            names.visit_scheme_type(&parameter.ty);
        }
        names.visit_scheme_type(scheme.return_type());
        names
    }

    fn visit_scheme_type(&mut self, ty: &SchemeType) {
        match ty {
            SchemeType::Ground(_) => {}
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
            .map(|(id, index)| (*index, self.type_name(*id)))
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
        binders
    }
}

impl fmt::Display for CheckedCallableContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.symbol_owner.with_current(|| {
            let names = BinderNames::from_scheme(&self.scheme);
            f.write_str(&render_scheme(&self.scheme, &names, &self.schema_identities))
        })
    }
}

fn render_scheme(scheme: &TypeScheme, names: &BinderNames, identities: &BTreeMap<TypeDefId, SchemaIdentity>) -> String {
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
            .map(|parameter| render_parameter(parameter, names, identities))
            .collect::<Vec<_>>()
            .join(", "),
        render_scheme_type(scheme.return_type(), names, identities)
    ));
    if let Some(schema) = scheme.return_schema() {
        parts.push(format!("return-schema={}", render_schema(schema, identities)));
    }
    parts.push(format!("effects={}", render_effects(scheme.effects())));
    parts.join(" ")
}

fn render_parameter(parameter: &SchemeParameter, names: &BinderNames, identities: &BTreeMap<TypeDefId, SchemaIdentity>) -> String {
    let mut rendered = String::new();
    if parameter.rest {
        rendered.push_str("...");
    }
    rendered.push_str(&parameter.name.to_string());
    rendered.push_str(": ");
    rendered.push_str(&render_scheme_type(&parameter.ty, names, identities));
    if parameter.defaulted {
        rendered.push_str(" = default");
    }
    if let Some(schema) = &parameter.schema_expectation {
        rendered.push_str(" schema=");
        rendered.push_str(&render_schema(schema, identities));
    }
    rendered
}

fn render_scheme_type(ty: &SchemeType, names: &BinderNames, identities: &BTreeMap<TypeDefId, SchemaIdentity>) -> String {
    match ty {
        SchemeType::Ground(ty) => render_type(ty),
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
                    .map(|parameter| render_parameter(parameter, names, identities))
                    .collect::<Vec<_>>()
                    .join(", "),
                render_scheme_type(monotype.return_type(), names, identities)
            );
            if let Some(schema) = monotype.return_schema() {
                rendered.push_str(" return-schema=");
                rendered.push_str(&render_schema(schema, identities));
            }
            rendered.push_str(" effects=");
            rendered.push_str(&render_effects(effects.as_deref()));
            rendered
        }
        SchemeType::List(item) => format!("List[{}]", render_scheme_type(item, names, identities)),
        SchemeType::Map(key, value) if matches!(key.as_ref(), SchemeType::Ground(Type::Str)) => {
            format!("Map[{}]", render_scheme_type(value, names, identities))
        }
        SchemeType::Map(key, value) => format!(
            "Map[{}, {}]",
            render_scheme_type(key, names, identities),
            render_scheme_type(value, names, identities)
        ),
        SchemeType::Stream(item) => format!("Stream[{}]", render_scheme_type(item, names, identities)),
        SchemeType::Set(item) => format!("Set[{}]", render_scheme_type(item, names, identities)),
        SchemeType::Result(ok, error) => format!(
            "Result[{}, {}]",
            render_scheme_type(ok, names, identities),
            render_scheme_type(error, names, identities)
        ),
        SchemeType::Optional(item) => format!("{}?", render_scheme_type(item, names, identities)),
        SchemeType::Record { fields, tail } => {
            let mut entries = fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", render_scheme_type(ty, names, identities)))
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
                .map(|member| render_scheme_type(member, names, identities))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SchemeType::Validated(validation, base) => {
            render_validation(*validation, render_scheme_type(base, names, identities))
        }
    }
}

fn render_callable_type(kind: &str, signature: &CallableType) -> String {
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
                    render_type(&parameter.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
        render_type(&signature.return_ty)
    );
    rendered.push_str(" effects=");
    rendered.push_str(&render_effects(signature.effects.as_deref()));
    rendered
}

fn render_type(ty: &Type) -> String {
    match ty {
        Type::List(item) => format!("List[{}]", render_type(item)),
        Type::Map(key, value) if matches!(key.as_ref(), Type::Str) => {
            format!("Map[{}]", render_type(value))
        }
        Type::Map(key, value) => {
            format!("Map[{}, {}]", render_type(key), render_type(value))
        }
        Type::Stream(item) => format!("Stream[{}]", render_type(item)),
        Type::Set(item) => format!("Set[{}]", render_type(item)),
        Type::Result(ok, error) => {
            format!("Result[{}, {}]", render_type(ok), render_type(error))
        }
        Type::Optional(item) => format!("{}?", render_type(item)),
        Type::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", render_type(ty)))
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
                    .map(|(name, export)| render_module_export(*name, export))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Type::Union(members) => format!(
            "Union[{}]",
            members.iter().map(render_type).collect::<Vec<_>>().join(", ")
        ),
        Type::Callable(callable) => render_callable_type(
            if callable.pure { "pure" } else { "proc" },
            &callable.sig,
        ),
        Type::Validated(validated) => {
            render_validation(validated.validation(), render_type(validated.base()))
        }
        Type::Tag(name) | Type::ErrorFamily(name) | Type::ErrorFacet(name) => name.to_string(),
        Type::ErrorVariant { family, variant } => format!("{family}.{variant}"),
        Type::Inference(_) => "<inference>".to_owned(),
        other => other.to_string(),
    }
}

fn render_module_export(name: crate::symbol::Name, export: &ModuleExportType) -> String {
    match export {
        ModuleExportType::Value { ty, optional } => format!(
            "{name}{}: {}",
            if *optional { "?" } else { "" },
            render_type(ty)
        ),
        ModuleExportType::Proc { sig, optional } => format!(
            "{name}{}: {}",
            if *optional { "?" } else { "" },
            render_callable_type("proc", sig)
        ),
        ModuleExportType::Pure { sig, optional } => format!(
            "{name}{}: {}",
            if *optional { "?" } else { "" },
            render_callable_type("pure", sig)
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

fn render_schema(expectation: &SchemaExpectation, identities: &BTreeMap<TypeDefId, SchemaIdentity>) -> String {
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
                    .map(render_type)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>();
    entries.extend(expectation.children.iter().map(|(component, child)| {
        format!("{}: {}", render_schema_component(*component), render_schema(child, identities))
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
