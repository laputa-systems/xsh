use super::{CheckedFunctionDeclaration, CheckedFunctionKind, Checker, CheckOutput};
use crate::sema::constants::SchemaExpectation;
use crate::sema::schemes::{CallableMonotype, SchemeCallableKind, SchemeRowParameterId,
    SchemeRowTail, SchemeType, SchemeTypeParameterId, TypeScheme};
use crate::symbol::SymbolOwner;
use crate::syntax::arena::{ArenaProgram, TypeDefId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod render;

/// An immutable view of the checker's selected callable contract. Cloning the
/// view preserves its symbol spellings after its source arena is dropped.
#[derive(Clone, Debug)]
pub struct CheckedCallableContract {
    scheme: Arc<TypeScheme>,
    symbol_owner: SymbolOwner,
    schema_identities: Arc<BTreeMap<TypeDefId, SchemaIdentity>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SchemaIdentity {
    pub(super) module: Option<Arc<str>>,
    pub(super) name: crate::symbol::Name,
}

impl Checker {
    pub(super) fn callable_schema_identities(&self, program: &ArenaProgram, scheme: &TypeScheme) -> Arc<BTreeMap<TypeDefId, SchemaIdentity>> {
        let mut types = scheme.params().iter().map(|parameter| &parameter.ty)
            .chain(std::iter::once(scheme.return_type())).collect::<Vec<_>>();
        let mut schemas = scheme.params().iter().filter_map(|parameter| parameter.schema_expectation.as_ref())
            .chain(scheme.return_schema()).collect::<Vec<_>>();
        while let Some(ty) = types.pop() {
            match ty {
                SchemeType::Callable { monotype, .. } => {
                    types.extend(monotype.params().iter().map(|parameter| &parameter.ty));
                    types.push(monotype.return_type());
                    schemas.extend(monotype.params().iter().filter_map(|parameter| parameter.schema_expectation.as_ref()));
                    schemas.extend(monotype.return_schema());
                }
                SchemeType::List(inner) | SchemeType::Stream(inner) | SchemeType::Set(inner)
                | SchemeType::Optional(inner) | SchemeType::Validated(_, inner) => types.push(inner),
                SchemeType::Map(key, value) | SchemeType::Result(key, value) => { types.push(key); types.push(value); }
                SchemeType::Record { fields, .. } => types.extend(fields.values()),
                SchemeType::Union(members) => types.extend(members),
                SchemeType::Ground(_) | SchemeType::Parameter(_) => {}
            }
        }
        let mut definitions = BTreeSet::new();
        while let Some(schema) = schemas.pop() {
            definitions.extend(schema.instances.iter().map(|instance| instance.definition));
            schemas.extend(schema.children.values());
        }
        Arc::new(definitions.into_iter().map(|definition| {
            let name = program.arena.type_def(definition).name;
            let namespace = self.record_constructors.namespace(definition);
            assert_eq!(self.record_constructors.definition(namespace, name), Some(definition), "checked schema identity matches its declaration");
            let module = namespace.map(|namespace| {
                Arc::from(program.modules.iter().find(|module| module.name == namespace)
                    .expect("checked schema belongs to a declared module").key.as_str())
            });
            (definition, SchemaIdentity { module, name })
        }).collect())
    }
}

impl CheckedCallableContract {
    pub fn kind(&self) -> CheckedFunctionKind {
        match self.scheme.kind() {
            SchemeCallableKind::Pure => CheckedFunctionKind::Pure,
            SchemeCallableKind::Proc => CheckedFunctionKind::Proc,
            SchemeCallableKind::Stream => CheckedFunctionKind::Stream,
        }
    }

    pub fn is_generic(&self) -> bool {
        !self.scheme.type_parameters().is_empty() || !self.scheme.row_parameters().is_empty()
    }

    /// Compares checked structure modulo bound type and row parameter names.
    /// Free and nominal identities, labels, schemas, layout, and effects remain
    /// part of the contract; annotation and inference provenance do not.
    pub fn equivalent_to(&self, other: &Self) -> bool {
        same_effects(self.scheme.effects(), other.scheme.effects()) && self.same_shape(other)
    }

    /// Joins finite effect requirements for the same checked callable shape.
    /// An unrestricted effect clause remains unrestricted in the result.
    pub fn join_effects(&self, other: &Self) -> Option<Self> {
        if !self.same_shape(other) { return None; }
        let effects = match (self.scheme.effects(), other.scheme.effects()) {
            (Some(left), Some(right)) => Some(crate::syntax::node::Effect::ALL.into_iter().filter(|effect| left.contains(effect) || right.contains(effect)).collect()),
            _ => None,
        };
        Some(Self {
            scheme: Arc::new(TypeScheme::with_callable(self.scheme.kind(), Arc::clone(self.scheme.callable()), effects,
                self.scheme.inferred_effects() || other.scheme.inferred_effects()).ok()?),
            symbol_owner: self.symbol_owner.clone(),
            schema_identities: Arc::clone(&self.schema_identities),
        })
    }

    /// Returns an annotation only when the source callable-type grammar can
    /// express the checked contract. Callers still recheck it in its scope.
    pub fn source_annotation(&self) -> Option<String> {
        use crate::sema::types::{CallableParamType, CallableType, Type, TypedCallable};
        if self.is_generic() || self.scheme.return_schema().is_some() { return None; }
        let pure = match self.scheme.kind() {
            SchemeCallableKind::Pure => true,
            SchemeCallableKind::Proc => false,
            SchemeCallableKind::Stream => return None,
        };
        let params = self.scheme.params().iter().map(|parameter| {
            if parameter.defaulted || parameter.rest || parameter.schema_expectation.is_some() { return None; }
            Some(CallableParamType { name: parameter.name, ty: parameter.ty.closed_ground_type()?, defaulted: false, rest: false })
        }).collect::<Option<Vec<_>>>()?;
        let ty = Type::Callable(Arc::new(TypedCallable { pure, sig: CallableType {
            params, return_ty: Box::new(self.scheme.return_type().closed_ground_type()?),
            effects: if pure { None } else { self.scheme.effects().map(<[_]>::to_vec) },
        } }));
        let mut pending = vec![&ty];
        while let Some(ty) = pending.pop() {
            if matches!(ty, Type::Callable(callable) if callable.sig.params.iter().any(|parameter| parameter.defaulted || parameter.rest)) {
                return None;
            }
            ty.for_each_child(|child| pending.push(child));
        }
        self.symbol_owner.with_current(|| ty.annotation_source())
    }

    fn same_shape(&self, other: &Self) -> bool {
        if self.scheme.kind() != other.scheme.kind()
            || self.scheme.type_parameters().len() != other.scheme.type_parameters().len()
            || self.scheme.row_parameters().len() != other.scheme.row_parameters().len()
        {
            return false;
        }
        let mut equivalence = BoundEquivalence {
            types: BTreeMap::new(), rows: BTreeMap::new(),
            left_schemas: &self.schema_identities, right_schemas: &other.schema_identities,
        };
        if !equivalence.parameters(self.scheme.params(), other.scheme.params())
            || !equivalence.ty(self.scheme.return_type(), other.scheme.return_type())
            || !equivalence.schema(self.scheme.return_schema(), other.scheme.return_schema())
        {
            return false;
        }
        self.scheme.row_parameters().iter().all(|left| {
            equivalence.rows.get(&left.id).and_then(|right| {
                other.scheme.row_parameters().iter().find(|parameter| parameter.id == *right)
            }).is_some_and(|right| left.lacks == right.lacks)
        })
    }
}

impl CheckedFunctionDeclaration {
    pub fn callable_contract(&self) -> Option<CheckedCallableContract> {
        Some(CheckedCallableContract {
            scheme: Arc::clone(self.scheme.as_ref()?),
            symbol_owner: self.symbol_owner.clone(),
            schema_identities: Arc::clone(&self.schema_identities),
        })
    }
}

impl CheckOutput {
    /// Returns the selected declaration contract before per-call generic
    /// instantiation. An invocation span selects its callee's contract.
    pub fn callable_contract_at(&self, span: crate::source::Span) -> Option<CheckedCallableContract> {
        let target = self.selected_function_calls.get(&span)?;
        self.checked_declarations.functions.get(&target.definition)
            .filter(|declaration| declaration.name == target.name && declaration.namespace == target.namespace)?
            .callable_contract()
    }

    /// Returns the declaration contract of a checked function value reference.
    /// Invocation spans have no function value reference contract.
    pub fn callable_value_contract_at(&self, span: crate::source::Span) -> Option<CheckedCallableContract> {
        self.selected_function_value_references.contains(&span)
            .then(|| self.callable_contract_at(span)).flatten()
    }
}

fn same_effects(left: Option<&[crate::syntax::node::Effect]>, right: Option<&[crate::syntax::node::Effect]>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.iter().all(|effect| right.contains(effect)) && right.iter().all(|effect| left.contains(effect)),
        _ => false,
    }
}

struct BoundEquivalence<'a> {
    types: BTreeMap<SchemeTypeParameterId, SchemeTypeParameterId>,
    rows: BTreeMap<SchemeRowParameterId, SchemeRowParameterId>,
    left_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>,
    right_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>,
}

fn bind<Key: Copy + Ord>(mapping: &mut BTreeMap<Key, Key>, left: Key, right: Key) -> bool {
    if let Some(mapped) = mapping.get(&left) { return *mapped == right; }
    if mapping.values().any(|mapped| *mapped == right) { return false; }
    mapping.insert(left, right);
    true
}

impl BoundEquivalence<'_> {
    fn schema(&self, left: Option<&SchemaExpectation>, right: Option<&SchemaExpectation>) -> bool {
        match (left, right) {
            (None, None) => true,
            (Some(left), Some(right)) => left.instances.len() == right.instances.len()
                && left.instances.iter().zip(&right.instances).all(|(left, right)| {
                    self.left_schemas.get(&left.definition).is_some_and(|identity| Some(identity) == self.right_schemas.get(&right.definition))
                        && left.arguments == right.arguments
                })
                && left.children.len() == right.children.len()
                && left.children.iter().all(|(component, left)| right.children.get(component)
                    .is_some_and(|right| self.schema(Some(left), Some(right)))),
            _ => false,
        }
    }

    fn parameters(&mut self, left: &[crate::sema::schemes::SchemeParameter], right: &[crate::sema::schemes::SchemeParameter]) -> bool {
        left.len() == right.len() && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name && left.defaulted == right.defaulted && left.rest == right.rest
                && self.schema(left.schema_expectation.as_ref(), right.schema_expectation.as_ref()) && self.ty(&left.ty, &right.ty)
        })
    }

    fn monotype(&mut self, left: &CallableMonotype, right: &CallableMonotype) -> bool {
        self.parameters(left.params(), right.params()) && self.ty(left.return_type(), right.return_type())
            && self.schema(left.return_schema(), right.return_schema())
    }

    fn ty(&mut self, left: &SchemeType, right: &SchemeType) -> bool {
        if let (Some(left), Some(right)) = (left.closed_ground_type(), right.closed_ground_type()) {
            return left == right;
        }
        match (left, right) {
            (SchemeType::Ground(left), SchemeType::Ground(right)) => left == right,
            (SchemeType::Parameter(left), SchemeType::Parameter(right)) => bind(&mut self.types, *left, *right),
            (SchemeType::Callable { kind: left_kind, monotype: left, effects: left_effects },
             SchemeType::Callable { kind: right_kind, monotype: right, effects: right_effects }) => {
                left_kind == right_kind && same_effects(left_effects.as_deref(), right_effects.as_deref()) && self.monotype(left, right)
            }
            (SchemeType::List(left), SchemeType::List(right))
            | (SchemeType::Stream(left), SchemeType::Stream(right))
            | (SchemeType::Set(left), SchemeType::Set(right))
            | (SchemeType::Optional(left), SchemeType::Optional(right)) => self.ty(left, right),
            (SchemeType::Map(left_key, left_value), SchemeType::Map(right_key, right_value))
            | (SchemeType::Result(left_key, left_value), SchemeType::Result(right_key, right_value)) => {
                self.ty(left_key, right_key) && self.ty(left_value, right_value)
            }
            (SchemeType::Record { fields: left, tail: left_tail }, SchemeType::Record { fields: right, tail: right_tail }) => {
                left.len() == right.len() && left.iter().all(|(name, left)| right.get(name).is_some_and(|right| self.ty(left, right)))
                    && match (left_tail, right_tail) {
                        (SchemeRowTail::Closed, SchemeRowTail::Closed) => true,
                        (SchemeRowTail::Parameter(left), SchemeRowTail::Parameter(right)) => bind(&mut self.rows, *left, *right),
                        _ => false,
                    }
            }
            (SchemeType::Union(left), SchemeType::Union(right)) => left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| self.ty(left, right)),
            (SchemeType::Validated(left_validation, left), SchemeType::Validated(right_validation, right)) => {
                left_validation == right_validation && self.ty(left, right)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::check::{CheckOptions, Checker};
    use crate::sema::schemes::{SchemeParameter, SchemeRowParameter};
    use crate::sema::types::Type;
    use crate::source::SourceId;
    use crate::symbol::Name;
    use crate::syntax::parser::Parser;
    use std::collections::BTreeSet;

    fn identity(owner: &SymbolOwner, binder: u32, result: Option<Type>) -> CheckedCallableContract {
        owner.with_current(|| {
            let binder = SchemeTypeParameterId(binder);
            CheckedCallableContract {
                scheme: Arc::new(TypeScheme::new(SchemeCallableKind::Pure, vec![binder], Vec::new(),
                    vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Parameter(binder),
                        schema_expectation: None, defaulted: false, rest: false }],
                    result.map_or(SchemeType::Parameter(binder), SchemeType::Ground), None, false, None, false).unwrap()),
                symbol_owner: owner.clone(),
                schema_identities: Default::default(),
            }
        })
    }

    fn root_schema_contract(source: &str) -> CheckedCallableContract {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        parsed.arena.symbol_owner().with_current(|| checked.checked_declarations.functions.values()
            .find(|declaration| declaration.name.as_str() == "describe").unwrap().callable_contract().unwrap())
    }

    #[test]
    fn checked_callable_value_contract_distinguishes_calls_from_references() {
        let source = "pure integer(value: Int) -> Int { value }\nlet action = integer\nlet number = integer(1)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let reference = source.find("= integer\n").unwrap() + 2;
        let reference = crate::source::Span::new(SourceId::new(0), reference, reference + "integer".len());
        let invocation = source.find("integer(1)").unwrap();
        let invocation = crate::source::Span::new(SourceId::new(0), invocation, invocation + "integer(1)".len());
        assert!(checked.callable_value_contract_at(reference).is_some());
        assert!(checked.callable_value_contract_at(invocation).is_none());
        assert!(checked.callable_contract_at(invocation).is_some());
    }

    #[test]
    fn checked_callable_value_contract_does_not_replace_factory_result_with_callee() {
        let source = "type Callback = pure(value: Int) -> Int\npure integer(value: Int) -> Int { value }\npure factory() -> Callback { integer }\nlet action = factory()\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let invocation = source.rfind("factory()").unwrap();
        let invocation = crate::source::Span::new(SourceId::new(0), invocation, invocation + "factory()".len());
        assert!(matches!(checked.expr_types.get(&invocation), Some(Type::Callable(_))));
        assert!(checked.callable_value_contract_at(invocation).is_none());
        assert!(checked.callable_contract_at(invocation).is_some());
    }

    #[test]
    fn checked_callable_value_contract_preserves_generic_references_without_tagging_results() {
        let source = "pure identity(value) { value }\nlet action = identity\nlet number = identity(1)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let reference = source.find("= identity\n").unwrap() + 2;
        let reference = crate::source::Span::new(SourceId::new(0), reference, reference + "identity".len());
        let invocation = source.find("identity(1)").unwrap();
        let invocation = crate::source::Span::new(SourceId::new(0), invocation, invocation + "identity(1)".len());
        assert!(checked.callable_value_contract_at(reference).unwrap().is_generic());
        assert!(checked.callable_value_contract_at(invocation).is_none());
        assert!(checked.callable_contract_at(invocation).unwrap().is_generic());
    }

    #[test]
    fn checked_callable_contract_distinguishes_independently_parsed_schema_names() {
        let first = root_schema_contract("type First[T] = {label: Str}\npure describe(value: First[Int]) -> Str { value.label }\n");
        let second = root_schema_contract("type Second[T] = {label: Str}\npure describe(value: Second[Int]) -> Str { value.label }\n");
        assert!(!first.equivalent_to(&second));
        assert_ne!(first.to_string(), second.to_string());
    }

    #[test]
    fn checked_callable_contract_preserves_schema_identity_when_arena_indices_shift() {
        let first = root_schema_contract("type Entry[T] = {label: Str}\npure describe(value: Entry[Int]) -> Str { value.label }\n");
        let shifted = root_schema_contract("type Unrelated = Int\ntype Entry[T] = {label: Str}\npure describe(value: Entry[Int]) -> Str { value.label }\n");
        assert!(first.equivalent_to(&shifted));
        assert_eq!(first.to_string(), shifted.to_string());
        assert!(first.join_effects(&shifted).unwrap().equivalent_to(&first));
    }

    fn imported_schema_contract(key: &str, namespace: &str) -> CheckedCallableContract {
        use crate::syntax::arena::ArenaProgramBuilder;
        let source = "use shared as api\n";
        let module_source = "##! Schema module.\n## A phantom schema.\nexport type Entry[T] = {label: Str}\n## Reads its label.\nexport pure describe(value: Entry[Int]) -> Str { value.label }\n";
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let root = Parser::parse_source_into_arena_builder(SourceId::new(0), source, &mut builder);
        let module = Parser::parse_source_into_arena_builder(SourceId::new(1), module_source, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let name = builder.symbol_owner().intern(namespace);
        builder.push_arena_module(key.to_owned(), name, module.statements);
        let statement = builder.statement_ids(root.statements)[0];
        let (use_id, _, _) = builder.use_stmt_for_statement(statement).unwrap();
        builder.set_use_resolved(use_id, Arc::from(key));
        let program = builder.finish_with_statements(root.statements);
        let checked = Checker::check_arena_with_options(&program, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        program.symbol_owner().with_current(|| checked.checked_declarations.functions.values()
            .find(|declaration| declaration.name.as_str() == "describe").unwrap().callable_contract().unwrap())
    }

    #[test]
    fn checked_callable_contract_anchors_imported_schemas_to_canonical_module_keys() {
        let first = imported_schema_contract("/library/first.xsh", "shared");
        let second = imported_schema_contract("/library/second.xsh", "shared");
        let renamed = imported_schema_contract("/library/first.xsh", "renamed");
        assert!(!first.equivalent_to(&second));
        assert_ne!(first.to_string(), second.to_string());
        assert!(first.equivalent_to(&renamed));
        assert_eq!(first.to_string(), renamed.to_string());
    }

    #[test]
    fn checked_callable_contract_preserves_binder_relations_and_nominal_identities() {
        let owner = SymbolOwner::new();
        let left = identity(&owner, 7, None);
        let right = identity(&owner, 91, None);
        assert!(left.equivalent_to(&right));
        assert_eq!(left.to_string(), right.to_string());
        assert_eq!(left.kind(), CheckedFunctionKind::Pure);
        assert!(left.is_generic());
        assert!(left.source_annotation().is_none());
        assert!(!left.equivalent_to(&identity(&owner, 91, Some(Type::Int))));
        owner.with_current(|| {
            let first = identity(&owner, 7, Some(Type::Tag(Name::intern("ContractFirstNominal"))));
            let second = identity(&owner, 7, Some(Type::Tag(Name::intern("ContractSecondNominal"))));
            assert!(!first.equivalent_to(&second));
            assert_ne!(first.to_string(), second.to_string());
        });
    }

    #[test]
    fn checked_callable_contract_compares_row_binders_and_lacks() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let build = |id, extra| {
                let field = Name::intern("value");
                let mut lacks = BTreeSet::from([field]);
                if extra { lacks.insert(Name::intern("other")); }
                let row = SchemeRowParameterId(id);
                CheckedCallableContract {
                    scheme: Arc::new(TypeScheme::new(SchemeCallableKind::Pure, Vec::new(),
                        vec![SchemeRowParameter { id: row, lacks }],
                        vec![SchemeParameter { name: Name::intern("record"),
                            ty: SchemeType::Record { fields: BTreeMap::from([(field, SchemeType::Ground(Type::Int))]), tail: SchemeRowTail::Parameter(row) },
                            schema_expectation: None, defaulted: false, rest: false }],
                        SchemeType::Ground(Type::Int), None, false, None, false).unwrap()),
                    symbol_owner: owner.clone(),
                    schema_identities: Default::default(),
                }
            };
            let first = build(3, false);
            let renamed = build(42, false);
            assert!(first.equivalent_to(&renamed));
            assert_eq!(first.to_string(), renamed.to_string());
            assert!(!first.equivalent_to(&build(3, true)));
        });
    }

    #[test]
    fn checked_callable_contract_survives_frontend_disposal_and_selected_calls() {
        let (generic, ground, call_contract) = {
            let source = "pure contract_identity(value) { value }\npure contract_integer(value: Int) -> Int { value }\nlet result: Int = contract_identity(42)\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source,
                CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let declaration = |name| checked.checked_declarations.functions.values()
                    .find(|declaration| declaration.name.as_str() == name).unwrap().callable_contract().unwrap();
                let generic = declaration("contract_identity");
                let ground = declaration("contract_integer");
                let call = checked.selected_function_calls.iter().find(|(_, target)| target.name.as_str() == "contract_identity").unwrap().0;
                let selected = checked.callable_contract_at(*call).unwrap();
                assert!(generic.equivalent_to(&selected));
                (generic, ground, selected)
            })
        };
        let unrelated_owner = SymbolOwner::new();
        unrelated_owner.with_current(|| {
            assert!(generic.is_generic());
            assert!(!ground.is_generic());
            assert!(generic.equivalent_to(&call_contract));
            assert!(generic.to_string().contains("value"));
            assert!(ground.to_string().contains("Int"));
        });
    }
}

#[cfg(test)]
mod effect_join_tests {
    use super::*;
    use crate::sema::schemes::SchemeParameter;
    use crate::sema::types::Type;
    use crate::symbol::Name;
    use crate::syntax::node::Effect;

    #[test]
    fn checked_callable_contract_joins_effects_without_widening_shape() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let build = |effects, return_type| CheckedCallableContract {
                scheme: Arc::new(TypeScheme::new(SchemeCallableKind::Proc, Vec::new(), Vec::new(),
                    vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Ground(Type::Str),
                        schema_expectation: None, defaulted: false, rest: false }],
                    SchemeType::Ground(return_type), None, false, effects, false).unwrap()),
                symbol_owner: owner.clone(),
                schema_identities: Default::default(),
            };
            let fs = build(Some(vec![Effect::Fs, Effect::Error]), Type::Str);
            let process = build(Some(vec![Effect::Process, Effect::Error]), Type::Str);
            assert!(!fs.equivalent_to(&process));
            let joined = fs.join_effects(&process).unwrap();
            assert!(joined.equivalent_to(&build(Some(vec![Effect::Fs, Effect::Process, Effect::Error]), Type::Str)));
            assert!(joined.equivalent_to(&process.join_effects(&fs).unwrap()));
            assert_eq!(joined.source_annotation().as_deref(), Some("proc(value: Str) [fs, process, error] -> Str"));
            assert!(fs.join_effects(&build(Some(vec![Effect::Process]), Type::Int)).is_none());
            assert!(fs.join_effects(&build(None, Type::Str)).unwrap().equivalent_to(&build(None, Type::Str)));
        });
    }
}
