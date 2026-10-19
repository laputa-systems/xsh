use super::{CheckedFunctionDeclaration, CheckedFunctionKind, CheckOutput};
use crate::sema::schemes::{CallableMonotype, SchemeCallableKind, SchemeRowParameterId,
    SchemeRowTail, SchemeType, SchemeTypeParameterId, TypeScheme};
use crate::symbol::SymbolOwner;
use std::collections::BTreeMap;
use std::sync::Arc;

mod render;

/// An immutable view of the checker's selected callable contract. Cloning the
/// view preserves its symbol spellings after its source arena is dropped.
#[derive(Clone, Debug)]
pub struct CheckedCallableContract {
    scheme: Arc<TypeScheme>,
    symbol_owner: SymbolOwner,
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
        let mut equivalence = BoundEquivalence::default();
        if !equivalence.parameters(self.scheme.params(), other.scheme.params())
            || !equivalence.ty(self.scheme.return_type(), other.scheme.return_type())
            || self.scheme.return_schema() != other.scheme.return_schema()
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
        })
    }
}

impl CheckOutput {
    /// Returns the declaration contract selected for this exact callable
    /// expression or call span, before per-call generic instantiation.
    pub fn callable_contract_at(&self, span: crate::source::Span) -> Option<CheckedCallableContract> {
        let target = self.selected_function_calls.get(&span)?;
        self.checked_declarations.functions.get(&target.definition)
            .filter(|declaration| declaration.name == target.name && declaration.namespace == target.namespace)?
            .callable_contract()
    }
}

fn same_effects(left: Option<&[crate::syntax::node::Effect]>, right: Option<&[crate::syntax::node::Effect]>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.iter().all(|effect| right.contains(effect)) && right.iter().all(|effect| left.contains(effect)),
        _ => false,
    }
}

#[derive(Default)]
struct BoundEquivalence {
    types: BTreeMap<SchemeTypeParameterId, SchemeTypeParameterId>,
    rows: BTreeMap<SchemeRowParameterId, SchemeRowParameterId>,
}

fn bind<Key: Copy + Ord>(mapping: &mut BTreeMap<Key, Key>, left: Key, right: Key) -> bool {
    if let Some(mapped) = mapping.get(&left) { return *mapped == right; }
    if mapping.values().any(|mapped| *mapped == right) { return false; }
    mapping.insert(left, right);
    true
}

impl BoundEquivalence {
    fn parameters(&mut self, left: &[crate::sema::schemes::SchemeParameter], right: &[crate::sema::schemes::SchemeParameter]) -> bool {
        left.len() == right.len() && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name && left.defaulted == right.defaulted && left.rest == right.rest
                && left.schema_expectation == right.schema_expectation && self.ty(&left.ty, &right.ty)
        })
    }

    fn monotype(&mut self, left: &CallableMonotype, right: &CallableMonotype) -> bool {
        self.parameters(left.params(), right.params()) && self.ty(left.return_type(), right.return_type())
            && left.return_schema() == right.return_schema()
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
            }
        })
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
