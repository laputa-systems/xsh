use super::check::{CheckedCallableContract, CheckedFunctionTarget, SchemeBinderCorrespondence};
use std::num::NonZeroU64;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SchemeCallableOwner(NonZeroU64);

impl Default for SchemeCallableOwner {
    fn default() -> Self { Self::fresh() }
}

impl SchemeCallableOwner {
    pub(crate) fn fresh() -> Self {
        let owner = NEXT_OWNER.try_update(Ordering::Relaxed, Ordering::Relaxed, |owner| owner.checked_add(1))
            .expect("scheme callable owner identities are exhausted");
        Self(NonZeroU64::new(owner).expect("scheme callable owners are nonzero"))
    }

    pub(crate) fn id(self, index: u32) -> SchemeCallableId { SchemeCallableId { owner: self, index } }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemeCallableId {
    owner: SchemeCallableOwner,
    index: u32,
}

impl SchemeCallableId {
    pub(crate) fn index_in(self, owner: SchemeCallableOwner) -> Result<u32, SchemeCallableError> {
        if self.owner != owner { return Err(SchemeCallableError::ForeignOwner); }
        Ok(self.index)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemeCallableError { ForeignOwner, UnknownRelationship, EmptyTargets, DuplicateTarget, IncompatibleContract, ExhaustedRelationships, TypeDepth, RecursiveRelationship, MissingSourceTable }

pub(crate) fn map_type_ids(
    ty: &super::types::Type,
    map: &mut impl FnMut(SchemeCallableId) -> Result<SchemeCallableId, SchemeCallableError>,
) -> Result<super::types::Type, SchemeCallableError> {
    fn nested(ty: &super::types::Type, map: &mut impl FnMut(SchemeCallableId) -> Result<SchemeCallableId, SchemeCallableError>, depth: usize) -> Result<super::types::Type, SchemeCallableError> {
        use super::types::{Type, ModuleExportType};
        if depth > super::constraints::MAX_TYPE_DEPTH { return Err(SchemeCallableError::TypeDepth); }
        Ok(match ty {
            Type::SchemeCallable(id) => Type::SchemeCallable(map(*id)?),
            Type::List(item) => Type::List(Box::new(nested(item, map, depth + 1)?)),
            Type::Stream(item) => Type::Stream(Box::new(nested(item, map, depth + 1)?)),
            Type::Set(item) => Type::Set(Box::new(nested(item, map, depth + 1)?)),
            Type::Optional(item) => Type::Optional(Box::new(nested(item, map, depth + 1)?)),
            Type::Map(key, value) => Type::Map(Box::new(nested(key, map, depth + 1)?), Box::new(nested(value, map, depth + 1)?)),
            Type::Result(value, error) => Type::Result(Box::new(nested(value, map, depth + 1)?), Box::new(nested(error, map, depth + 1)?)),
            Type::Record(fields) => Type::Record(Arc::new(fields.iter().map(|(&name, ty)| Ok((name, nested(ty, map, depth + 1)?))).collect::<Result<_, _>>()?)),
            Type::Union(members) => Type::Union(members.iter().map(|ty| nested(ty, map, depth + 1)).collect::<Result<_, _>>()?),
            Type::Validated(validated) => Type::Validated(Box::new(super::validated::ValidatedType::new(validated.validation(), nested(validated.base(), map, depth + 1)?).map_err(|_| SchemeCallableError::IncompatibleContract)?)),
            Type::Callable(callable) => {
                let mut callable = callable.as_ref().clone();
                for parameter in &mut callable.sig.params { parameter.ty = nested(&parameter.ty, map, depth + 1)?; }
                callable.sig.return_ty = Box::new(nested(&callable.sig.return_ty, map, depth + 1)?);
                Type::Callable(Arc::new(callable))
            }
            Type::Module(module) => {
                let mut module = module.as_ref().clone();
                for export in module.exports.values_mut() {
                    match export {
                        ModuleExportType::Value { ty, .. } => *ty = nested(ty, map, depth + 1)?,
                        ModuleExportType::Callable { sig, .. } => {
                            for parameter in &mut sig.params { parameter.ty = nested(&parameter.ty, map, depth + 1)?; }
                            *sig.return_ty = nested(&sig.return_ty, map, depth + 1)?;
                        }
                        ModuleExportType::GenericCallable { .. } => {}
                    }
                }
                Type::Module(Arc::new(module))
            }
            ty => ty.clone(),
        })
    }
    nested(ty, map, 0)
}

#[derive(Clone, Debug)]
pub(crate) struct SchemeCallableTarget {
    target: CheckedFunctionTarget,
    contract: CheckedCallableContract,
    correspondence: SchemeBinderCorrespondence,
}

impl SchemeCallableTarget {
    pub(crate) fn target(&self) -> CheckedFunctionTarget { self.target }
    pub(crate) fn contract(&self) -> &CheckedCallableContract { &self.contract }
    pub(crate) fn correspondence(&self) -> &SchemeBinderCorrespondence { &self.correspondence }
}

#[derive(Clone, Debug)]
pub(crate) struct SchemeCallableRelationship {
    canonical: CheckedCallableContract,
    targets: Vec<SchemeCallableTarget>,
}

impl SchemeCallableRelationship {
    pub(crate) fn canonical(&self) -> &CheckedCallableContract { &self.canonical }
    pub(crate) fn targets(&self) -> &[SchemeCallableTarget] { &self.targets }

    pub(crate) fn new(
        mut targets: Vec<(CheckedFunctionTarget, CheckedCallableContract)>,
        callables: &SchemeCallableTable,
    ) -> Result<Self, SchemeCallableError> {
        targets.sort_by_key(|(target, _)| (target.namespace, target.name, target.definition.index()));
        if targets.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(SchemeCallableError::DuplicateTarget); }
        let canonical = targets.first().ok_or(SchemeCallableError::EmptyTargets)?.1.clone();
        let targets = targets.into_iter().map(|(target, contract)| {
            let correspondence = canonical.binder_correspondence(&contract, callables).ok_or(SchemeCallableError::IncompatibleContract)?;
            Ok(SchemeCallableTarget { target, contract, correspondence })
        }).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { canonical, targets })
    }

    fn key(&self) -> Vec<(Option<crate::symbol::Name>, crate::symbol::Name, usize)> {
        self.targets.iter().map(|target| (target.target.namespace, target.target.name, target.target.definition.index())).collect()
    }

    pub(crate) fn agrees_with(&self, other: &Self, callables: &SchemeCallableTable) -> bool {
        self.canonical.equivalent_to_in_table(&other.canonical, callables)
            && self.targets.len() == other.targets.len()
            && self.targets.iter().zip(&other.targets).all(|(left, right)| left.target == right.target && left.correspondence == right.correspondence)
    }
}

#[derive(Default)]
pub(crate) struct SchemeCallableRetainedAllocations {
    tables: std::collections::BTreeSet<usize>,
    relationships: std::collections::BTreeSet<usize>,
    schemes: std::collections::BTreeSet<usize>,
    schemas: std::collections::BTreeSet<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct SchemeCallableTable {
    owner: SchemeCallableOwner,
    relationships: Vec<Arc<SchemeCallableRelationship>>,
    targets: BTreeMap<Vec<(Option<crate::symbol::Name>, crate::symbol::Name, usize)>, u32>,
}

impl SchemeCallableTable {
    pub(crate) fn retained_bytes(&self, allocations: &mut SchemeCallableRetainedAllocations, counted_schemes: &std::collections::BTreeSet<usize>) -> usize {
        use std::mem::size_of;
        allocations.schemes.extend(counted_schemes);
        if !allocations.tables.insert(self as *const Self as usize) { return 0; }
        let mut bytes = size_of::<Self>()
            + self.relationships.capacity() * size_of::<Arc<SchemeCallableRelationship>>()
            + self.targets.len() * size_of::<(Vec<(Option<crate::symbol::Name>, crate::symbol::Name, usize)>, u32)>()
            + self.targets.keys().map(|key| key.capacity() * size_of::<(Option<crate::symbol::Name>, crate::symbol::Name, usize)>()).sum::<usize>();
        for relationship in &self.relationships {
            if !allocations.relationships.insert(Arc::as_ptr(relationship) as usize) { continue; }
            bytes += size_of::<SchemeCallableRelationship>()
                + relationship.targets.capacity() * size_of::<SchemeCallableTarget>()
                + relationship.canonical.retained_heap_bytes(&mut allocations.schemes, &mut allocations.schemas);
            for target in &relationship.targets {
                bytes += target.correspondence.retained_heap_bytes()
                    + target.contract.retained_heap_bytes(&mut allocations.schemes, &mut allocations.schemas);
            }
        }
        bytes
    }

    #[cfg(test)]
    pub(crate) fn owner(&self) -> SchemeCallableOwner { self.owner }

    pub(crate) fn get(&self, id: SchemeCallableId) -> Result<&SchemeCallableRelationship, SchemeCallableError> {
        self.relationships.get(id.index_in(self.owner)? as usize).map(Arc::as_ref).ok_or(SchemeCallableError::UnknownRelationship)
    }

    pub(crate) fn relationship(&self, id: SchemeCallableId) -> Result<Arc<SchemeCallableRelationship>, SchemeCallableError> {
        self.relationships.get(id.index_in(self.owner)? as usize).cloned().ok_or(SchemeCallableError::UnknownRelationship)
    }

    #[cfg(test)]
    pub(crate) fn insert(&mut self, targets: Vec<(CheckedFunctionTarget, CheckedCallableContract)>) -> Result<SchemeCallableId, SchemeCallableError> {
        let relationship = Arc::new(SchemeCallableRelationship::new(targets, self)?);
        let (id, actual) = self.intern_prepared(Arc::clone(&relationship))?;
        if !actual.agrees_with(&relationship, self) { return Err(SchemeCallableError::IncompatibleContract); }
        Ok(id)
    }

    pub(crate) fn intern_prepared(&mut self, relationship: Arc<SchemeCallableRelationship>) -> Result<(SchemeCallableId, Arc<SchemeCallableRelationship>), SchemeCallableError> {
        let key = relationship.key();
        if let Some(&index) = self.targets.get(&key) { return Ok((self.owner.id(index), Arc::clone(&self.relationships[index as usize]))); }
        let index = u32::try_from(self.relationships.len()).ok().filter(|index| *index != u32::MAX).ok_or(SchemeCallableError::ExhaustedRelationships)?;
        self.relationships.push(Arc::clone(&relationship));
        self.targets.insert(key, index);
        Ok((self.owner.id(index), relationship))
    }

    pub(crate) fn snapshot(&self) -> Self { Self { owner: self.owner, relationships: self.relationships.clone(), targets: self.targets.clone() } }
}

pub(crate) struct SchemeCallableRebaser {
    destination: Arc<std::sync::Mutex<SchemeCallableTable>>,
    relationships: BTreeMap<SchemeCallableId, SchemeCallableId>,
    active: std::collections::BTreeSet<SchemeCallableId>,
    schemes: BTreeMap<(SchemeCallableOwner, usize), (Arc<super::schemes::TypeScheme>, Arc<super::schemes::TypeScheme>)>,
}

impl SchemeCallableRebaser {
    pub(crate) fn new(destination: Arc<std::sync::Mutex<SchemeCallableTable>>) -> Self {
        Self { destination, relationships: BTreeMap::new(), active: Default::default(), schemes: BTreeMap::new() }
    }

    pub(crate) fn snapshot(&self) -> Arc<SchemeCallableTable> {
        Arc::new(self.destination.lock().expect("checked scheme callable table is poisoned").snapshot())
    }

    pub(crate) fn import_table(&mut self, source: &SchemeCallableTable) -> Result<(), SchemeCallableError> {
        for index in 0..source.relationships.len() {
            self.relationship(source.owner.id(u32::try_from(index).map_err(|_| SchemeCallableError::ExhaustedRelationships)?), source, 0)?;
        }
        Ok(())
    }

    pub(crate) fn ty(&mut self, ty: &super::types::Type, source: &SchemeCallableTable) -> Result<super::types::Type, SchemeCallableError> {
        self.ty_at_depth(ty, source, 0)
    }

    fn ty_at_depth(&mut self, ty: &super::types::Type, source: &SchemeCallableTable, depth: usize) -> Result<super::types::Type, SchemeCallableError> {
        if depth > super::constraints::MAX_TYPE_DEPTH { return Err(SchemeCallableError::TypeDepth); }
        map_type_ids(ty, &mut |id| self.relationship(id, source, depth + 1))
    }

    pub(crate) fn scheme(&mut self, scheme: &Arc<super::schemes::TypeScheme>, source: &SchemeCallableTable) -> Result<Arc<super::schemes::TypeScheme>, SchemeCallableError> {
        self.scheme_at_depth(scheme, source, 0)
    }

    fn scheme_at_depth(&mut self, scheme: &Arc<super::schemes::TypeScheme>, source: &SchemeCallableTable, depth: usize) -> Result<Arc<super::schemes::TypeScheme>, SchemeCallableError> {
        if depth > super::constraints::MAX_TYPE_DEPTH { return Err(SchemeCallableError::TypeDepth); }
        let key = (source.owner, Arc::as_ptr(scheme) as usize);
        if let Some((original, mapped)) = self.schemes.get(&key) {
            assert!(Arc::ptr_eq(original, scheme), "rebasing retains the exact source allocation");
            return Ok(Arc::clone(mapped));
        }
        let mut failure = None;
        let mapped = scheme.map_ground_types(|ty| match self.ty_at_depth(ty, source, depth + 1) {
            Ok(mapped) => *ty = mapped,
            Err(error) => failure = Some(error),
        });
        if let Some(error) = failure { return Err(error); }
        let mapped = Arc::new(mapped);
        self.schemes.insert(key, (Arc::clone(scheme), Arc::clone(&mapped)));
        Ok(mapped)
    }

    pub(crate) fn relationship(&mut self, id: SchemeCallableId, source: &SchemeCallableTable, depth: usize) -> Result<SchemeCallableId, SchemeCallableError> {
        if depth > super::constraints::MAX_TYPE_DEPTH { return Err(SchemeCallableError::TypeDepth); }
        if let Some(&mapped) = self.relationships.get(&id) { return Ok(mapped); }
        let relationship = source.relationship(id)?;
        if !self.active.insert(id) { return Err(SchemeCallableError::RecursiveRelationship); }
        let mapped = (|| {
            let mut targets = Vec::with_capacity(relationship.targets.len());
            for target in &relationship.targets {
                let scheme = self.scheme_at_depth(target.contract.scheme(), source, depth + 1)?;
                targets.push((target.target, target.contract.with_rebased_scheme(scheme)));
            }
            let prepared = {
                let destination = self.destination.lock().expect("checked scheme callable table is poisoned");
                Arc::new(SchemeCallableRelationship::new(targets, &destination)?)
            };
            let id = {
                let mut destination = self.destination.lock().expect("checked scheme callable table is poisoned");
                let (id, actual) = destination.intern_prepared(Arc::clone(&prepared))?;
                if !actual.agrees_with(&prepared, &destination) { return Err(SchemeCallableError::IncompatibleContract); }
                id
            };
            Ok(id)
        })();
        self.active.remove(&id);
        let mapped = mapped?;
        self.relationships.insert(id, mapped);
        Ok(mapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::{CheckOptions, Checker};
    use crate::sema::types::Type;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn identities() -> Vec<(CheckedFunctionTarget, CheckedCallableContract)> {
        let source = "pure first(value) { value }\npure second(value) { value }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty());
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        checked.checked_declarations.functions.iter().map(|(&definition, function)| (
            CheckedFunctionTarget { definition, namespace: function.namespace, name: function.name },
            function.callable_contract().unwrap(),
        )).collect()
    }

    #[test]
    fn independent_tables_reject_equal_index_foreign_relationships() {
        let targets = identities();
        let mut first = SchemeCallableTable::default();
        let mut second = SchemeCallableTable::default();
        let left = first.insert(vec![targets[0].clone()]).unwrap();
        let right = second.insert(vec![targets[0].clone()]).unwrap();
        assert_eq!(left.index_in(first.owner()).unwrap(), 0);
        assert_eq!(right.index_in(second.owner()).unwrap(), 0);
        assert_ne!(left, right);
        assert_eq!(first.get(right).unwrap_err(), SchemeCallableError::ForeignOwner);
        assert_eq!(second.get(left).unwrap_err(), SchemeCallableError::ForeignOwner);
    }

    #[test]
    fn finite_targets_retain_complete_binder_correspondences() {
        let mut targets = identities();
        targets.reverse();
        let mut table = SchemeCallableTable::default();
        let id = table.insert(targets.clone()).unwrap();
        let relationship = table.get(id).unwrap();
        assert_eq!(relationship.targets().len(), 2);
        for target in relationship.targets() {
            assert_eq!(target.correspondence().types().len(), relationship.canonical().scheme().type_parameters().len());
            assert!(target.correspondence().rows().is_empty());
            assert!(target.correspondence().effects().is_empty());
            assert!(targets.iter().any(|(actual, _)| *actual == target.target()));
        }
        assert_eq!(table.insert(Vec::new()), Err(SchemeCallableError::EmptyTargets));
        assert_eq!(table.insert(vec![targets[0].clone(), targets[0].clone()]), Err(SchemeCallableError::DuplicateTarget));
    }

    #[test]
    fn independent_guarded_handlers_execute_after_frontend_and_module_disposal() {
        use crate::runtime::value::{ResultValue, Value};
        let directory = tempfile::Builder::new().prefix("guarded-correspondence-").tempdir_in(".work/tmp").unwrap();
        for (name, source) in [
            ("left_handler", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/modules/guarded-correspondence/left_handler.xsh"))),
            ("right_handler", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/modules/guarded-correspondence/right_handler.xsh"))),
        ] {
            std::fs::write(directory.path().join(format!("{name}.xsh")), source).unwrap();
        }
        let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/runtime/guarded-correspondence.xsh"));
        let path = directory.path().canonicalize().unwrap().join("entry.xsh");
        let file = path.to_str().unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(file, crate::loader::entry_source_from_text(file, source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources)
            .prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        drop(directory);
        for force_frames in [false, true] {
            let result = prepared.eval_test("verify_guarded_correspondence", Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(result.output.stdout.is_empty(), "{:?}", result.output.stdout);
            assert!(result.output.stderr.is_empty(), "{:?}", result.output.stderr);
            assert!(matches!(result.result, Some(Value::Result(ResultValue::Ok(value))) if matches!(*value, Value::Unit)));
        }
    }

    #[test]
    fn rebase_preserves_nested_factory_relationships_and_rejects_foreign_mismatch() {
        fn checked_factory(target: &str) -> (crate::sema::check::CheckOutput, crate::source::Span) {
            let source = format!(
                "pure identity(value) {{ value }}\npure alternative(value) {{ value }}\npure factory(marker) {{ let _ = marker; let forwarded = {target}; forwarded }}\nlet saved = factory\n"
            );
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source,
                CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let start = source.rfind("factory").unwrap();
            let span = crate::source::Span::new(SourceId::new(0), start, start + "factory".len());
            (checked, span)
        }

        let (checked, span) = checked_factory("identity");
        let source_contract = checked.callable_value_contract_at(span).unwrap();
        let Type::SchemeCallable(source_outer) = &checked.expr_types[&span] else { panic!("the factory value retains its relationship"); };
        let outer = checked.scheme_callables.get(*source_outer).unwrap();
        let crate::sema::schemes::SchemeType::SchemeCallable(source_inner) = outer.canonical().scheme().return_type() else {
            panic!("the factory returns a nested generic callable relationship");
        };

        let mut rebaser = SchemeCallableRebaser::new(Arc::new(std::sync::Mutex::new(SchemeCallableTable::default())));
        let destination_outer = rebaser.relationship(*source_outer, &checked.scheme_callables, 0).unwrap();
        let destination = rebaser.snapshot();
        let destination_relationship = destination.get(destination_outer).unwrap();
        let crate::sema::schemes::SchemeType::SchemeCallable(destination_inner) = destination_relationship.canonical().scheme().return_type() else {
            panic!("the rebased factory retains its nested relationship");
        };
        assert_ne!(*source_inner, *destination_inner);
        assert!(checked.scheme_callables.get(*source_inner).is_ok());
        assert!(destination.get(*destination_inner).is_ok());
        assert_eq!(checked.scheme_callables.get(*destination_inner).unwrap_err(), SchemeCallableError::ForeignOwner);
        assert_eq!(destination.get(*source_inner).unwrap_err(), SchemeCallableError::ForeignOwner);
        assert!(source_contract.equivalent_to(&source_contract));

        let (foreign, foreign_span) = checked_factory("alternative");
        let foreign_contract = foreign.callable_value_contract_at(foreign_span).unwrap();
        assert!(!source_contract.equivalent_to(&foreign_contract));
    }
}
