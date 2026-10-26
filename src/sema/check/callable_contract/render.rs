use super::{CheckedCallableContract, SchemaIdentity};
use crate::sema::scheme_callable::SchemeCallableTable;
use crate::syntax::arena::TypeDefId;
use crate::sema::constants::{SchemaComponent, SchemaExpectation};
use crate::sema::schemes::{
    CallableMonotype, CallableRequirements, SchemeParameter, SchemeRowParameterId, SchemeType,
    SchemeTypeParameterId, TypeScheme,
};
use crate::sema::types::{CallableType, ModuleExportType, Type};
use crate::sema::validated::Validation;
use crate::syntax::node::Effect;
use bbtree::unsync::BBTreeMap as BTreeMap;
use std::fmt;

#[derive(Default)]
struct BinderNames<'a> {
    parent: Option<&'a BinderNames<'a>>,
    prefix: String,
    type_base: usize,
    row_base: usize,
    effect_base: usize,
    types: BTreeMap<SchemeTypeParameterId, usize>,
    effects: BTreeMap<crate::sema::schemes::SchemeEffectParameterId, usize>,
    rows: BTreeMap<SchemeRowParameterId, usize>,
}

impl BinderNames<'_> {
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
        for application in &scheme.callable().requirements().applications {
            for ty in application.types() { names.visit_scheme_type(ty); }
        }
        for method in &scheme.callable().requirements().methods {
            for ty in std::iter::once(&method.receiver).chain(method.arguments.iter()).chain(std::iter::once(&method.result)) {
                names.visit_scheme_type(ty);
            }
        }
        for parameter in scheme.callable().requirements().conversion_parameters.keys() {
            names.visit_scheme_type(&SchemeType::Parameter(*parameter));
        }
        for (source, iteration) in &scheme.callable().requirements().iteration_parameters {
            names.visit_scheme_type(&SchemeType::Parameter(*source));
            for ty in iteration.types() { names.visit_scheme_type(ty); }
        }
        names.visit_guard_inputs(scheme.callable().requirements());
        names
    }

    fn visit_guard_inputs(&mut self, requirements: &CallableRequirements) {
        for guard in &requirements.guards {
            self.visit_scheme_type(&guard.source);
            for read in &guard.reads { self.visit_scheme_type(&read.source); }
            if let Some(effects) = &guard.effects { self.visit_effects(&effects.outward); }
            if let Some(output) = &guard.output {
                self.visit_scheme_type(&output.outward);
                self.visit_scheme_type(&output.absent);
            }
        }
        for call in &requirements.guard_calls {
            for ty in call.type_arguments.values() { self.visit_scheme_type(ty); }
            for row in call.row_arguments.values() {
                for ty in row.fields().values() { self.visit_scheme_type(ty); }
                if let crate::sema::schemes::SchemeRowTail::Parameter(id) = row.tail() { self.bind_row(id); }
            }
            for effects in call.effect_arguments.values() { self.visit_effects(effects); }
        }
    }

    fn bind_row(&mut self, id: SchemeRowParameterId) {
        let next = self.row_base + self.rows.len();
        self.rows.entry(id).or_insert(next);
    }

    fn visit_effects(&mut self, effects: &crate::sema::effects::EffectContract<crate::sema::schemes::SchemeEffectParameterId>) {
        if let crate::sema::effects::EffectContract::Known(expression) = effects {
            for (id, _) in expression.variables() {
                let next = self.effect_base + self.effects.len();
                self.effects.entry(*id).or_insert(next);
            }
        }
    }

    fn for_guard(&self, guard: &crate::sema::guarded_requirements::GuardRequirements) -> BinderNames<'_> {
        let mut names = BinderNames {
            parent: Some(self), prefix: self.prefix.clone(),
            type_base: self.type_base + self.types.len(),
            row_base: self.row_base + self.rows.len(),
            effect_base: self.effect_base + self.effects.len(),
            ..BinderNames::default()
        };
        for &id in &guard.type_parameters {
            assert!(self.find_type(id).is_none(), "guard type binders are locally owned");
            let next = names.type_base + names.types.len();
            assert!(names.types.insert(id, next).is_none(), "guard type binders are unique");
        }
        for parameter in &guard.row_parameters {
            assert!(self.find_row(parameter.id).is_none(), "guard row binders are locally owned");
            let next = names.row_base + names.rows.len();
            assert!(names.rows.insert(parameter.id, next).is_none(), "guard row binders are unique");
        }
        for &id in &guard.requirements.effect_parameters {
            assert!(self.find_effect(id).is_none(), "guard effect binders are locally owned");
            let next = names.effect_base + names.effects.len();
            assert!(names.effects.insert(id, next).is_none(), "guard effect binders are unique");
        }
        names
    }

    fn local_quantifiers(&self, guard: &crate::sema::guarded_requirements::GuardRequirements) -> Vec<String> {
        let mut types = guard.type_parameters.iter().map(|id| {
            let name = self.type_name(*id);
            (self.find_type(*id).expect("checked local type binder"), if guard.requirements.map_key_parameters.contains(id) { format!("{name}: MapKey") } else { name })
        }).collect::<Vec<_>>();
        types.sort_by_key(|(index, _)| *index);
        let mut binders = types.into_iter().map(|(_, name)| name).collect::<Vec<_>>();
        let mut rows = guard.row_parameters.iter().collect::<Vec<_>>();
        rows.sort_by_key(|parameter| self.find_row(parameter.id).expect("checked local row binder"));
        for row in rows {
            let name = self.row_name(row.id);
            let lacks = row.lacks.iter().map(ToString::to_string).collect::<Vec<_>>();
            binders.push(if lacks.is_empty() { name } else { format!("{name}\\{{{}}}", lacks.join(", ")) });
        }
        let mut effects = guard.requirements.effect_parameters.iter().collect::<Vec<_>>();
        effects.sort_by_key(|id| self.find_effect(**id).expect("checked local effect binder"));
        binders.extend(effects.into_iter().map(|id| self.effect_name(*id)));
        binders
    }

    fn visit_scheme_type(&mut self, ty: &SchemeType) {
        match ty {
            SchemeType::Ground(_) | SchemeType::SchemeCallable(_) => {}
            SchemeType::Parameter(id) => {
                let next = self.type_base + self.types.len();
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
                    let next = self.row_base + self.rows.len();
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

    fn find_type(&self, id: SchemeTypeParameterId) -> Option<usize> {
        self.types.get(&id).copied().or_else(|| self.parent.and_then(|parent| parent.find_type(id)))
    }

    fn find_row(&self, id: SchemeRowParameterId) -> Option<usize> {
        self.rows.get(&id).copied().or_else(|| self.parent.and_then(|parent| parent.find_row(id)))
    }

    fn find_effect(&self, id: crate::sema::schemes::SchemeEffectParameterId) -> Option<usize> {
        self.effects.get(&id).copied().or_else(|| self.parent.and_then(|parent| parent.find_effect(id)))
    }

    fn type_name(&self, id: SchemeTypeParameterId) -> String {
        format!("{}T{}", self.prefix, self.find_type(id).expect("checked callable type binder"))
    }

    fn row_name(&self, id: SchemeRowParameterId) -> String {
        format!("{}R{}", self.prefix, self.find_row(id).expect("checked callable row binder"))
    }

    fn effect_name(&self, id: crate::sema::schemes::SchemeEffectParameterId) -> String {
        format!("{}E{}", self.prefix, self.find_effect(id).expect("checked callable effect binder"))
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
        binders.extend(effects.into_iter().map(|index| format!("{}E{index}", self.prefix)));
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
    active: bbtree::unsync::BBTreeSet<crate::sema::scheme_callable::SchemeCallableId>,
}

pub(in crate::sema::check) fn render_revealed_type(ty: &Type, table: &SchemeCallableTable) -> String {
    if !ty.contains_scheme_callable() { return ty.to_string(); }
    let mut context = CallableRenderContext { table: Some(table), labels: BTreeMap::new(), active: Default::default() };
    render_type(ty, &mut context)
}

fn render_scheme(scheme: &TypeScheme, names: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
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
        parts.push(format!("return-schema={}", render_schema(schema, names, identities, context)));
    }
    parts.extend(render_requirement_parts(scheme.callable().requirements(), Some(scheme.effects().contract()), names, identities, context));
    parts.join(" ")
}

fn render_requirement_parts(requirements: &CallableRequirements, effects: Option<&crate::sema::effects::EffectContract<crate::sema::schemes::SchemeEffectParameterId>>, names: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> Vec<String> {
    let mut parts = Vec::new();
    let mut conversions = requirements.conversion_parameters.iter().collect::<Vec<_>>();
    conversions.sort_by_key(|(parameter, _)| names.find_type(**parameter).expect("checked conversion type binder"));
    for (parameter, conversions) in conversions {
        let conversions = crate::sema::schemes::ValueConversion::ALL.into_iter().filter(|conversion| conversions.contains(*conversion))
            .map(|conversion| format!("{conversion:?}")).collect::<Vec<_>>().join(", ");
        parts.push(format!("convert({}, [{conversions}])", names.type_name(*parameter)));
    }
    for (source, iteration) in &requirements.iteration_parameters {
        let item = render_scheme_type(&iteration.item, names, identities, context);
        let failure = match &iteration.policy {
            crate::sema::schemes::IterationPolicy::IterableInfallible | crate::sema::schemes::IterationPolicy::PipelineInfallible => "infallible".to_owned(),
            crate::sema::schemes::IterationPolicy::IterablePropagates(error) | crate::sema::schemes::IterationPolicy::PipelinePropagates(error) => format!("error={}", render_scheme_type(error, names, identities, context)),
            crate::sema::schemes::IterationPolicy::PipelineTransport => "pipeline-transport".to_owned(),
        };
        let source_kind = match iteration.source_kind {
            crate::sema::schemes::IterationSourceKind::Direct => "direct-iterable",
            crate::sema::schemes::IterationSourceKind::DirectOrResult => "iterable",
        };
        let domain = if iteration.policy.is_pipeline() { ", domain=pipeline" } else { "" };
        parts.push(format!("{source_kind}({}, item={item}, {failure}{domain})", names.type_name(*source)));
    }
    if let Some(effects) = effects { parts.push(format!("effects={}", render_bound_effects(effects, names))); }
    for application in &requirements.applications {
        parts.push(render_application(application, names, identities, context));
    }
    for method in &requirements.methods {
        let receiver = render_scheme_type(&method.receiver, names, identities, context);
        let arguments = method.argument_slots.iter().zip(method.arguments.iter()).map(|(slot, argument)| {
            format!("{slot}: {}", render_scheme_type(argument, names, identities, context))
        }).collect::<Vec<_>>().join(", ");
        let result = render_scheme_type(&method.result, names, identities, context);
        parts.push(format!("method-family({:?}, receiver={receiver}, args=[{arguments}]) -> {result}", method.family));
    }
    for predicate in &requirements.effect_predicates {
        use crate::sema::effects::EffectPredicate;
        let rendered = match predicate {
            EffectPredicate::Equal(left, right) => format!("effects-equal({}, {})", render_bound_effects(left, names), render_bound_effects(right, names)),
            EffectPredicate::Included(left, right) => format!("effects-included({}, {})", render_bound_effects(left, names), render_bound_effects(right, names)),
            EffectPredicate::Covered(left, right) => format!("effects-covered({}, {})", render_bound_effects(left, names), render_bound_effects(right, names)),
            EffectPredicate::Excluded(contract, excluded) => format!("effects-excluded({}, {})", render_bound_effects(contract, names), render_effects(Some(&excluded.iter().collect::<Vec<_>>()))),
        };
        parts.push(rendered);
    }
    for operation in &requirements.operations {
        let inputs = operation.inputs().map(|ty| render_scheme_type(ty, names, identities, context)).collect::<Vec<_>>().join(", ");
        let result = operation.result().map(|ty| render_scheme_type(ty, names, identities, context));
        parts.push(match operation {
            crate::sema::sealed_operation::SealedOperation::Membership { .. } => format!("membership({inputs})"),
            crate::sema::sealed_operation::SealedOperation::TypePredicate { domain, .. } => format!("{domain:?}({inputs})"),
            crate::sema::sealed_operation::SealedOperation::Binary(operation) => format!("arithmetic({:?}, {inputs}) -> {}", operation.operator, result.unwrap()),
            crate::sema::sealed_operation::SealedOperation::Index { syntax, constant_key, .. } => {
                let literal = match syntax { crate::sema::sealed_operation::IndexSyntax::Ordinary => String::new(), crate::sema::sealed_operation::IndexSyntax::NegativeLiteral(distance) => format!(", negative-literal={distance}") };
                let field = constant_key.map(|name| format!(", field={:?}", name.as_str().as_str())).unwrap_or_default();
                format!("index({inputs}{literal}{field}) -> {}", result.unwrap())
            }
            crate::sema::sealed_operation::SealedOperation::Slice { bounds, .. } => format!("slice({bounds:?}, {inputs}) -> {}", result.unwrap()),
            crate::sema::sealed_operation::SealedOperation::RecordOverride { overrides, .. } => {
                let names = overrides.iter().map(|field| field.name.as_str().to_string()).collect::<Vec<_>>().join(", ");
                format!("record-override([{names}], {inputs}) -> {}", result.unwrap())
            }
        });
    }
    for wrapper in &requirements.wrappers {
        let types = wrapper.types().into_iter().map(|ty| render_scheme_type(ty, names, identities, context)).collect::<Vec<_>>().join(", ");
        let effects = match &wrapper.kind {
            crate::sema::wrapper_operation::WrapperRequirementKind::Field { effects, .. } => format!(", effects={}", render_bound_effects(effects, names)),
            _ => String::new(),
        };
        parts.push(format!("wrapper({:?}, {types}{effects})", wrapper.shape()));
    }
    for guard in &requirements.guards { parts.push(render_guard(guard, names, identities, context)); }
    for call in &requirements.guard_calls { parts.push(render_guard_call(call, names, identities, context)); }
    parts
}

fn render_guard(guard: &crate::sema::guarded_requirements::GuardRequirements, parent: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
    let crate::sema::guarded_requirements::GuardRequirements {
        scope: _, source, view, subject: _, comparison: _, reads, effects, output,
        type_parameters: _, row_parameters: _, requirements,
    } = guard;
    let source = render_scheme_type(source, parent, identities, context);
    let names = parent.for_guard(guard);
    let mut parts = Vec::new();
    let quantified = names.local_quantifiers(guard);
    if !quantified.is_empty() { parts.push(format!("forall {}.", quantified.join(", "))); }
    parts.push(format!("view={}", render_scheme_type(view, &names, identities, context)));
    for read in reads {
        parts.push(format!("identity({})={}", render_scheme_type(&read.source, parent, identities, context), render_scheme_type(&read.view, &names, identities, context)));
    }
    if let Some(effects) = effects {
        parts.push(format!("effects(local={}, outward={}, null=[])", render_bound_effects(&effects.local, &names), render_bound_effects(&effects.outward, parent)));
    }
    if let Some(output) = output {
        let outward = render_scheme_type(&output.outward, parent, identities, context);
        let present = render_scheme_type(&output.present, &names, identities, context);
        let absent = render_scheme_type(&output.absent, parent, identities, context);
        parts.push(format!("output(outward={outward}, non-null={present}, null={absent}, optional=join({present}, {absent}))"));
    }
    parts.extend(render_requirement_parts(requirements, None, &names, identities, context));
    format!("when-non-null({source}) {{ {} }}", parts.join(" "))
}

fn render_guard_call(call: &crate::sema::guarded_requirements::GuardCallRequirement, caller: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
    call.validate_target().expect("a published guard call retains its actual callee template");
    let mut callee = BinderNames { prefix: format!("{}callee.", caller.prefix), ..BinderNames::default() };
    callee.visit_scheme_type(&call.template.source);
    for read in &call.template.reads { callee.visit_scheme_type(&read.source); }
    if let Some(effects) = &call.template.effects { callee.visit_effects(&effects.outward); }
    if let Some(output) = &call.template.output {
        callee.visit_scheme_type(&output.outward);
        callee.visit_scheme_type(&output.absent);
    }
    for &id in call.type_arguments.keys() {
        let next = callee.types.len();
        callee.types.entry(id).or_insert(next);
    }
    for &id in call.row_arguments.keys() { callee.bind_row(id); }
    for &id in call.effect_arguments.keys() {
        let next = callee.effects.len();
        callee.effects.entry(id).or_insert(next);
    }
    let mut type_arguments = call.type_arguments.iter().collect::<Vec<_>>();
    type_arguments.sort_by_key(|(id, _)| callee.find_type(**id).expect("checked callee type argument"));
    let types = type_arguments.into_iter().map(|(id, ty)| format!("{}={}", callee.type_name(*id), render_scheme_type(ty, caller, identities, context))).collect::<Vec<_>>().join(", ");
    let mut row_arguments = call.row_arguments.iter().collect::<Vec<_>>();
    row_arguments.sort_by_key(|(id, _)| callee.find_row(**id).expect("checked callee row argument"));
    let rows = row_arguments.into_iter().map(|(id, row)| format!("{}={}", callee.row_name(*id), render_record(row.fields(), row.tail(), caller, identities, context))).collect::<Vec<_>>().join(", ");
    let mut effect_arguments = call.effect_arguments.iter().collect::<Vec<_>>();
    effect_arguments.sort_by_key(|(id, _)| callee.find_effect(**id).expect("checked callee effect argument"));
    let effects = effect_arguments.into_iter().map(|(id, effects)| format!("{}={}", callee.effect_name(*id), render_bound_effects(effects, caller))).collect::<Vec<_>>().join(", ");
    let role = match call.source.role {
        crate::sema::schemes::InstantiationRole::Call => "guard-call",
        crate::sema::schemes::InstantiationRole::Value => "guard-value",
    };
    format!("{role}(types=[{types}], rows=[{rows}], effects=[{effects}]) {{ {} }}", render_guard(&call.template, &callee, identities, context))
}

fn render_record(fields: &BTreeMap<crate::symbol::Name, SchemeType>, tail: crate::sema::schemes::SchemeRowTail, names: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
    let mut entries = fields.iter().map(|(name, ty)| format!("{name}: {}", render_scheme_type(ty, names, identities, context))).collect::<Vec<_>>();
    if let crate::sema::schemes::SchemeRowTail::Parameter(id) = tail { entries.push(format!("..{}", names.row_name(id))); }
    format!("{{{}}}", entries.join(", "))
}

fn render_application(application: &crate::sema::callable_application::CallableApplication<SchemeType, crate::sema::schemes::SchemeEffectParameterId>, names: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
    use crate::sema::callable_application::{CallableApplication, CallableKindRequirement};
    use crate::sema::arguments::StaticArgumentKind;
    let arguments = application.arguments().iter().map(|argument| {
        let ty = render_scheme_type(&argument.ty, names, identities, context);
        match argument.kind {
            StaticArgumentKind::Positional => ty,
            StaticArgumentKind::Named(name) => format!("{name}: {ty}"),
            StaticArgumentKind::Splice => format!("@{ty}"),
        }
    }).collect::<Vec<_>>().join(", ");
    match application {
        CallableApplication::Invocation(invocation) => {
            let callee = render_scheme_type(&invocation.callee, names, identities, context);
            let result = render_scheme_type(&invocation.result, names, identities, context);
            let kind = match invocation.kind { CallableKindRequirement::Pure => "pure", CallableKindRequirement::Effectful => "effectful" };
            let mut rendered = format!("invoke({callee}, [{arguments}]) -> {result} kind={kind} effects={}", render_bound_effects(&invocation.effects, names));
            for envelope in invocation.proc_returns.iter() {
                let result = render_scheme_type(&envelope.result, names, identities, context);
                let errors = envelope.propagated_errors.iter().map(|error| render_scheme_type(error, names, identities, context)).collect::<Vec<_>>().join(", ");
                rendered.push_str(&format!(" proc-return({result}, propagated=[{errors}])"));
            }
            rendered
        }
        CallableApplication::KnownArguments(known) => {
            let parameters = known.parameters.iter().map(|parameter| {
                let rest = if parameter.layout.rest { "..." } else { "" };
                let defaulted = if parameter.layout.defaulted { " = default" } else { "" };
                format!("{rest}{}: {}{defaulted}", parameter.layout.name, render_scheme_type(&parameter.ty, names, identities, context))
            }).collect::<Vec<_>>().join(", ");
            format!("known-arguments(params=[{parameters}], args=[{arguments}])")
        }
    }
}

fn render_parameter(parameter: &SchemeParameter, names: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
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
        rendered.push_str(&render_schema(schema, names, identities, context));
    }
    rendered
}

fn render_scheme_type(ty: &SchemeType, names: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
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
                rendered.push_str(&render_schema(schema, names, identities, context));
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
        SchemeType::Record { fields, tail } => render_record(fields, *tail, names, identities, context),
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

fn render_schema(expectation: &SchemaExpectation<SchemeType>, names: &BinderNames<'_>, identities: &BTreeMap<TypeDefId, SchemaIdentity>, context: &mut CallableRenderContext<'_>) -> String {
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
                    .map(|ty| render_scheme_type(ty, names, identities, context))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>();
    entries.extend(expectation.children.iter().map(|(component, child)| {
        format!("{}: {}", render_schema_component(*component), render_schema(child, names, identities, context))
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

fn render_bound_effects(effects: &crate::sema::effects::EffectContract<crate::sema::schemes::SchemeEffectParameterId>, names: &BinderNames<'_>) -> String {
    match effects {
        crate::sema::effects::EffectContract::Unknown => "unknown".to_owned(),
        crate::sema::effects::EffectContract::Known(expression) => {
            let mut terms = expression.direct().iter().map(|effect| effect.as_str().to_owned()).collect::<Vec<_>>();
            terms.extend(expression.variables().map(|(id, mask)| format!("{}&{}", names.effect_name(*id), render_effects(Some(&mask.iter().collect::<Vec<_>>())))));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::{CheckOptions, Checker};
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn exported_contract(source: &str) -> CheckedCallableContract {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let declaration = checked.checked_declarations.functions.values().find(|declaration| declaration.name.as_str() == "describe").unwrap();
            assert!(declaration.is_exported());
            declaration.callable_contract().unwrap()
        })
    }

    #[test]
    fn exported_guarded_outputs_show_null_live_and_optional_relationships() {
        let source = "export pure describe(value) { if value != null { value.name } else { null } }\n";
        let contract = exported_contract(source);
        assert!(contract.to_string().contains("output(outward=T1, non-null=T2, null=Null, optional=join(T2, Null))"), "{contract}");
        let relocated = exported_contract(&format!("pure unrelated(input) {{ [input] }}\n{}", source.replace(" { ", " {\n ")));
        assert!(contract.equivalent_to(&relocated));
        assert_eq!(contract.to_string(), relocated.to_string());
        let probe = format!("{source}type Row = {{name: Str}}\nlet row: Row? = {{name: \"live\"}}\nlet exact: Str = describe({{name: \"live\"}})\nlet absent: Null = describe(null)\nlet optional: Str? = describe(row)\nreveal_type(describe)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &probe);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &probe, CheckOptions::for_check_cli());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.reveal_types.len(), 1);
        assert!(checked.reveal_types[0].message.contains(&contract.to_string()), "{:?}", checked.reveal_types);
    }

    #[test]
    fn exported_guarded_outputs_keep_selected_method_results_local() {
        let source = "export pure describe(value) { if value == null { null } else { value.parent() } }\n";
        let contract = exported_contract(source);
        assert!(contract.to_string().contains("output(outward=T1, non-null=T2, null=Null, optional=join(T2, Null))"), "{contract}");
        assert!(contract.to_string().contains("method-family(PathParent, receiver=T3, args=[]) -> T2"), "{contract}");
        let probe = format!("{source}let relative: RelPath = p\"a/b\"\nlet maybe: Path? = p\"/a/b\"\nlet exact: Path = describe(p\"/a/b\")\nlet relative_parent: RelPath = describe(relative)\nlet absent: Null = describe(null)\nlet optional: Path? = describe(maybe)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &probe);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &probe, CheckOptions::for_check_cli());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    }

    #[test]
    fn exported_guarded_contracts_render_their_actual_local_domains() {
        let display = exported_contract("export pure describe(value) -> Str { if value == null { \"\" } else { f\"{value}\" } }\n");
        let record = exported_contract("export pure describe(value) -> Str { if value == null { \"\" } else { value.name } }\n");
        assert!(!display.equivalent_to(&record));
        assert_ne!(display.to_string(), record.to_string());
        assert_eq!(display.to_string(), "forall T0. pure(value: T0) -> Str return-schema={} effects=unknown when-non-null(T0) { forall T1. view=T1 convert(T1, [Display]) }");
    }

    #[test]
    fn exported_guarded_rows_and_methods_keep_scoped_results() {
        let row = exported_contract("export pure describe(value) -> Str { if value == null { \"\" } else { value.name } }\n");
        let path = exported_contract("export pure describe(value) -> Path { if value == null { p\"/absent\" } else { value.parent() } }\n");
        assert_eq!(row.scheme().callable().requirements().guards.len(), 1);
        assert_eq!(path.scheme().callable().requirements().guards.len(), 1);
        let row = row.to_string();
        let path = path.to_string();
        assert_eq!(row, "forall T0. pure(value: T0) -> Str return-schema={} effects=unknown when-non-null(T0) { forall R0\\{name}. view={name: Str, ..R0} }");
        assert_eq!(path, "forall T0. pure(value: T0) -> Path return-schema={} effects=unknown when-non-null(T0) { forall T1. view=T1 method-family(PathParent, receiver=T1, args=[]) -> Path }");
    }

    #[test]
    fn exported_guard_calls_render_callee_bindings_as_call_origins() {
        let contract = exported_contract("pure helper(value) -> Str { if value == null { \"\" } else { f\"{value}\" } }\nexport pure describe(value) -> Str { helper(value) }\n");
        assert!(contract.scheme().callable().requirements().guards.is_empty());
        assert_eq!(contract.scheme().callable().requirements().guard_calls.len(), 1);
        let rendered = contract.to_string();
        assert_eq!(rendered, "forall T0. pure(value: T0) -> Str return-schema={} effects=unknown guard-call(types=[callee.T0=T0], rows=[], effects=[]) { when-non-null(callee.T0) { forall callee.T1. view=callee.T1 convert(callee.T1, [Display]) } }");
        let relayed = exported_contract("pure helper(value) -> Str { if value == null { \"\" } else { f\"{value}\" } }\npure relay(value) -> Str { helper(value) }\nexport pure describe(value) -> Str { relay(value) }\n");
        assert_eq!(relayed.to_string(), rendered);
    }

    #[test]
    fn exported_guarded_effects_keep_outward_and_declared_bounds() {
        let source = include_str!("../../../../tests/fixtures/modules/guarded-requirements/guarded_preload.xsh")
            .replace("export proc preload(", "export proc describe(");
        let contract = exported_contract(&source);
        assert!(contract.scheme().callable().requirements().guards.iter().any(|guard| guard.effects.is_some()));
        let rendered = contract.to_string();
        let covered = "effects-covered([E0&[fs, net, process, env, time, error, io]], [env, error])";
        let bounds = [covered; 4].join(" ");
        let signature = "forall T0, E0. proc(value: T0) -> Result[Str, Error] return-schema={success: {}, error: {}} effects=[env, error]";
        assert_eq!(rendered, format!("{signature} {bounds} when-non-null(T0) {{ forall T1. view=T1 effects(local=[env, error], outward=[E0&[fs, net, process, env, time, error, io]], null=[]) convert(T1, [Env]) {bounds} }}"));
    }

    #[test]
    fn guarded_rendering_is_independent_of_physical_ids_and_source_spans() {
        let source = "export pure describe(value) -> Str { if value == null { \"\" } else { f\"{value}\" } }\n";
        let original = exported_contract(source);
        let relocated = exported_contract(&format!("pure unrelated(value) {{ [value] }}\n{}", source.replace(" { ", " {\n ")));
        assert!(original.equivalent_to(&relocated));
        assert_eq!(original.to_string(), relocated.to_string());
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &format!("{source}reveal_type(describe)\n"));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &format!("{source}reveal_type(describe)\n"), CheckOptions::for_check_cli());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.reveal_types.len(), 1);
        assert!(checked.reveal_types[0].message.contains(&original.to_string()), "{:?}", checked.reveal_types);
    }

    #[test]
    fn nested_guard_rendering_preserves_ancestor_conversion_identity() {
        let source = "export pure describe(value, other) -> Str { if value == null { \"\" } else { if other == null { \"\" } else { f\"{value}\" } } }\n";
        let ancestor = exported_contract(source);
        let local = exported_contract(&source.replace("f\"{value}\"", "f\"{other}\""));
        assert!(!ancestor.equivalent_to(&local));
        assert_ne!(ancestor.to_string(), local.to_string());
        let relocated = exported_contract(&format!("pure unrelated(value) {{ [value] }}\n{source}"));
        assert!(ancestor.equivalent_to(&relocated));
        assert_eq!(ancestor.to_string(), relocated.to_string());
    }

    #[test]
    fn guarded_api_preserves_authored_any_and_unknown_method_diagnostics() {
        let opaque = exported_contract("export pure describe(value: Any) -> Any { value }\n");
        assert_eq!(opaque.scheme().params()[0].ty, SchemeType::Ground(Type::Any));
        assert_eq!(opaque.to_string(), "pure(value: Any schema={}) -> Any return-schema={} effects=unknown");
        for (source, code, span) in [
            ("export pure describe(value: Any) -> Str { if value == null { \"\" } else { f\"{value}\" } }\n", crate::diagnostic::DiagnosticCode::CheckDynamicBoundary, "value"),
            ("export pure describe(value: Str?) -> Str { if value == null { \"\" } else { value.missing_guard_member() } }\n", crate::diagnostic::DiagnosticCode::CheckUnknownMethod, "value.missing_guard_member()"),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_check_cli());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(code)
                && diagnostic.labels.iter().any(|label| &source[label.span.range()] == span)), "{:?}", checked.diagnostics);
        }
    }

}
