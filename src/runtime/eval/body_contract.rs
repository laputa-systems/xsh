use super::{Diagnostic, DiagnosticCode, Evaluator, LoweredParamKind, LoweredReturnKind, LoweredType, LoweredTypeCheck, Severity, SourceId, SourceMap, Span, Type};
use crate::sema::check::{BodyContractError, BodyContractOwner, BodyContractSite, CanonicalBodyContractSite, CheckOutput, CheckedCallableContract, Checker, FunctionEffectFact, PendingBodyContracts, SemanticBodyContract};
use crate::syntax::arena::ArenaProgram;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PreparedValidationContract {
    Parameter { kind: LoweredParamKind, check: Option<LoweredTypeCheck>, formal: Option<Type> },
    Return { kind: LoweredReturnKind, check: Option<LoweredTypeCheck>, formal: Option<Type> },
    DefaultParameter { kind: LoweredType, check: Option<LoweredTypeCheck> },
    CheckedValue(LoweredTypeCheck),
    Materialization { source: crate::sema::schemes::SchemeType, destination: crate::sema::schemes::SchemeType },
    Require(LoweredTypeCheck),
    Assignment(LoweredTypeCheck),
    DriverBinding { check: Option<LoweredTypeCheck>, kind: Option<LoweredType> },
}

impl PreparedValidationContract {
    fn interface_gate(&self, site: &CanonicalBodyContractSite, interface: Option<&CheckedCallableContract>) -> bool {
        match self {
            Self::Parameter { check, formal, .. } | Self::Return { check, formal, .. } => check.as_ref().is_none_or(|check| {
                formal.as_ref() == Some(&check.ty) && (check.schema.is_none() || interface.is_some_and(|interface| {
                    site.implicit_interface_schema(interface.scheme()).is_some_and(|(ty, schema)| {
                        interface.structural_interface_schema(ty, schema, &check.ty)
                            && check.schema.as_ref().is_some_and(|schema| schema.matches_type(&check.ty))
                    })
                }))
            }),
            _ => false,
        }
    }
}

pub(super) struct PreparedValidationRecord {
    pub(super) site: BodyContractSite,
    pub(super) contract: PreparedValidationContract,
}

/// A checked body together with the validation boundaries emitted by preparation.
#[derive(Clone, Debug)]
pub struct CheckedBodyContract {
    semantic: SemanticBodyContract,
    interface: Option<CheckedCallableContract>,
    validations: BTreeMap<CanonicalBodyContractSite, Vec<PreparedValidationContract>>,
    symbols: crate::symbol::SymbolOwner,
}

impl CheckedBodyContract {
    pub fn source_name(&self) -> &str { self.semantic.source_name() }
    pub fn owner(&self) -> BodyContractOwner { self.semantic.owner() }
    pub fn effect_fact(&self) -> Option<&FunctionEffectFact> { self.semantic.effect_fact() }

    /// Requires preserved checked selections and every emitted validation boundary.
    pub fn preserved_by(&self, candidate: &Self) -> bool {
        let _symbols = self.symbols.enter();
        let (interface_proven, instance) = match (&self.interface, &candidate.interface) {
            (Some(original), Some(candidate)) => {
                if original.equivalent_to(candidate) { (true, None) }
                else {
                    let Ok(Some(instance)) = original.instance_evidence(candidate) else { return false; };
                    (true, Some(instance))
                }
            }
            (None, None) => (false, None),
            _ => return false,
        };
        let preserved = match instance.as_ref() {
            Some(instance) => self.semantic.preserved_by_instance(&candidate.semantic, Some(instance)),
            None => self.semantic.preserved_by(&candidate.semantic),
        };
        if !preserved { return false; }
        let original_validations = self.comparison_validations(candidate);
        let candidate_validations = candidate.comparison_validations(self);
        if original_validations.len() != candidate_validations.len() { return false; }
        original_validations.iter().all(|(site, original)| candidate_validations.get(site).is_some_and(|records| {
            original == records || interface_proven && original.len() == records.len() && original.iter().zip(records).all(|(original, record)| {
                original.interface_gate(site, self.interface.as_ref()) && record.interface_gate(site, candidate.interface.as_ref())
            })
        }))
    }

    fn comparison_validations<'a>(&'a self, candidate: &Self)
        -> BTreeMap<&'a CanonicalBodyContractSite, Vec<&'a PreparedValidationContract>>
    {
        self.validations.iter().filter_map(|(site, records)| {
            let records = records.iter().filter(|record| match record {
                PreparedValidationContract::CheckedValue(check) if check.schema.is_none() =>
                    !self.semantic.redundant_local_initializer_validation(&candidate.semantic, site, &check.ty),
                _ => true,
            }).collect::<Vec<_>>();
            (!records.is_empty()).then_some((site, records))
        }).collect()
    }
}

/// Keeps semantic mapping failures distinct from the original preparation diagnostic.
#[derive(Clone, Debug)]
pub enum CheckedBodyContractError {
    Semantic(BodyContractError),
    Preparation(Diagnostic),
}

/// Consumes one check and prepares once without executing script initialization.
pub fn checked_body_contracts(checked: CheckOutput, program: &ArenaProgram, sources: &SourceMap, source_id: SourceId) -> Result<Vec<CheckedBodyContract>, CheckedBodyContractError> {
    if let Some(diagnostic) = checked.diagnostics.iter().find(|diagnostic| diagnostic.severity == Severity::Error) {
        return Err(CheckedBodyContractError::Preparation(diagnostic.clone()));
    }
    let pending = PendingBodyContracts::capture(&checked, program, sources).map_err(CheckedBodyContractError::Semantic)?;
    let mut interfaces = BTreeMap::new();
    for (&definition, declaration) in checked.checked_declarations.functions.iter().filter(|(_, declaration)| !declaration.internal) {
        if let Some((index, _)) = pending.canonical_site(BodyContractSite::Return { declaration: declaration.body }).map_err(CheckedBodyContractError::Semantic)? {
            interfaces.insert(index, checked.callable_declaration_contract(definition));
        }
    }
    let declarations = Checker::compact_declarations(program, checked);
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    program.symbol_owner().with_current(|| evaluator.prepare_checked_compact_indexed_only(program, source_id, true, declarations))
        .map_err(CheckedBodyContractError::Preparation)?;
    let prepared = evaluator.indexed_program.as_ref().ok_or_else(|| CheckedBodyContractError::Preparation(super::compact_lowerability_diagnostic(
        Span::new(source_id, 0, 0), "preparation did not publish an indexed program", DiagnosticCode::CompactIndexedBuild,
    )))?;
    let records = prepared.validation_contracts().map_err(|error| CheckedBodyContractError::Preparation(super::compact_lowerability_diagnostic(
        Span::new(source_id, 0, 0), &error.message, DiagnosticCode::CompactIndexedBuild,
    )))?;
    let mut validations = BTreeMap::<usize, BTreeMap<CanonicalBodyContractSite, Vec<PreparedValidationContract>>>::new();
    for record in records {
        if let Some((index, site)) = pending.canonical_site(record.site).map_err(CheckedBodyContractError::Semantic)? {
            validations.entry(index).or_default().entry(site).or_default().push(record.contract);
        }
    }
    let symbols = prepared.symbol_owner().clone();
    Ok(pending.into_contracts().into_iter().enumerate().map(|(index, semantic)| CheckedBodyContract {
        semantic,
        interface: interfaces.remove(&index).flatten(),
        validations: validations.remove(&index).unwrap_or_default(),
        symbols: symbols.clone(),
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::CheckOptions;

    fn snapshots(source: &str) -> Vec<CheckedBodyContract> {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "body-contract.xsh", crate::loader::entry_source_from_text("body-contract.xsh", source.to_string()), Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = sources.files().first().unwrap().id();
        let checked = Checker::check_arena_with_sources_and_options(&parsed.arena, &sources, source_id, CheckOptions { embedded_bodies: true, reveal_types: true, ..CheckOptions::default() });
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{source}\n{:?}", checked.diagnostics);
        checked_body_contracts(checked, &parsed.arena, &sources, source_id).unwrap()
    }

    fn declaration(source: &str) -> CheckedBodyContract {
        snapshots(source).into_iter().find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap()
    }

    #[test]
    fn structural_interface_aliases_preserve_prepared_ground_boundaries() {
        for (original, candidate) in [
            ("type Pieces = {texts: List[Str], kinds: List[Int]}\npure keep() -> Pieces { {texts: [\"a\"], kinds: [0]} }\n",
             "type Pieces = {texts: List[Str], kinds: List[Int]}\npure keep() { {texts: [\"a\"], kinds: [0]} }\n"),
            ("type Entry = {value: Int}\npure keep(value: Entry) -> Int { value.value }\n",
             "type Other = {value: Int}\npure keep(value: Other) -> Int { value.value }\n"),
        ] {
            let original = declaration(original);
            let candidate = declaration(candidate);
            assert!(original.preserved_by(&candidate), "{original:?}\n{candidate:?}");
        }
    }

    #[test]
    fn structural_interface_projection_retains_external_and_explicit_checks() {
        for (original, candidate) in [
            ("type Entry = {value: Int}\npure keep(value: Entry) -> Int { 1 }\n",
             "pure keep(value) -> Int { 1 }\n"),
            ("type Entry = {value: Int}\nproc keep(value: Any) [error] -> Int { value.require(Entry)?.value }\n",
             "type Other = {value: Int}\nproc keep(value: Any) [error] -> Int { value.require(Other)?.value }\n"),
            ("type First = {value: Int}\ntype Other = {value: Int}\npure keep() -> First { First(value: 1) }\n",
             "type First = {value: Int}\ntype Other = {value: Int}\npure keep() -> First { Other(value: 1) }\n"),
            ("type Entry = {value: UInt}\npure keep(value: Int) -> Entry { {value} }\n",
             "type Entry = {value: UInt}\npure keep(value: Int) { {value} }\n"),
            ("pure keep() -> Result[Int] { 1 }\n", "pure keep() { 1 }\n"),
        ] {
            let original = declaration(original);
            let candidate = declaration(candidate);
            assert!(!original.preserved_by(&candidate), "{original:?}\n{candidate:?}");
        }
    }

    #[test]
    fn structural_local_storage_preserves_checked_operations_and_values() {
        for (original, candidate) in [
            ("type RawArgument = {marker: Str, value: Bytes}\npure keep(argument: Bytes) -> List[RawArgument] { var raw: List[RawArgument] = []; raw += [{marker: \"m\", value: argument}]; raw }\n",
             "type RawArgument = {marker: Str, value: Bytes}\npure keep(argument: Bytes) -> List[RawArgument] { var raw = []; raw += [{marker: \"m\", value: argument}]; raw }\n"),
            ("type Span = {first: Int, last: Int}\npure keep() -> List[Span] { let spans: List[Span] = collect { for first in [1, 3] { yield {first, last: first + 1} } }; spans }\n",
             "type Span = {first: Int, last: Int}\npure keep() -> List[Span] { let spans = collect { for first in [1, 3] { yield {first, last: first + 1} } }; spans }\n"),
        ] {
            let original = declaration(original);
            let candidate = declaration(candidate);
            assert!(original.interface.as_ref().unwrap().equivalent_to(candidate.interface.as_ref().unwrap()));
            assert!(original.semantic.preserved_by(&candidate.semantic), "{:?}\n{:?}", original.semantic, candidate.semantic);
            assert!(original.preserved_by(&candidate), "{:?}\n{:?}", original.validations, candidate.validations);
        }
    }

    #[test]
    fn structural_local_projection_retains_domains_effects_and_schema_obligations() {
        for (original, candidate) in [
            ("pure keep(value: Int) -> Int { let checked: UInt = value; checked }\n",
             "pure keep(value: Int) -> Int { let checked = value; checked }\n"),
            ("pure keep(value: Int) -> Any { let checked: Result[Int] = Ok(value); checked }\n",
             "pure keep(value: Int) -> Any { let checked = value; checked }\n"),
            ("nominal type Entry = {value: Int}\npure keep() -> List[Entry] { let entries: List[Entry] = [Entry(value: 1)]; entries }\n",
             "nominal type Entry = {value: Int}\npure keep() -> List[Entry] { let entries = [Entry(value: 1)]; entries }\n"),
            ("type Entry = {value: Int = 1}\npure keep() -> List[Entry] { let entries: List[Entry] = [{value: 1}]; entries }\n",
             "type Entry = {value: Int = 1}\npure keep() -> List[Entry] { let entries = [{value: 1}]; entries }\n"),
            ("type Entry[T] = {value: Int}\npure keep() -> List[Entry[Int]] { let entries: List[Entry[Int]] = [{value: 1}]; entries }\n",
             "type Entry[T] = {value: Int}\npure keep() -> List[Entry[Int]] { let entries = [{value: 1}]; entries }\n"),
            ("type Entry = {value: Int}\nproc keep() [io] -> List[Entry] { print \"observed\"; let entries: List[Entry] = [{value: 1}]; entries }\n",
             "type Entry = {value: Int}\nproc keep() [io] -> List[Entry] { let entries = [{value: 1}]; entries }\n"),
        ] {
            let original = declaration(original);
            let candidate = declaration(candidate);
            assert!(!original.preserved_by(&candidate));
        }
    }

    #[test]
    fn wire_enum_projection_preserves_header_and_local_body_facts() {
        for (original, candidate) in [
            ("enum State: Str { Ready = \"ready\" }\ntype Info = {state: State, count: Int}\npure keep() -> Info { {state: Ready, count: 1} }\n",
             "enum State: Str { Ready = \"ready\" }\ntype Info = {state: State, count: Int}\npure keep() { {state: Ready, count: 1} }\n"),
            ("enum State: Str { Ready = \"ready\" }\ntype Info = {state: State, count: Int}\npure keep() -> List[Info] { let values: List[Info] = [{state: Ready, count: 1}]; values }\n",
             "enum State: Str { Ready = \"ready\" }\ntype Info = {state: State, count: Int}\npure keep() -> List[Info] { let values = [{state: Ready, count: 1}]; values }\n"),
        ] {
            let original = declaration(original);
            let candidate = declaration(candidate);
            assert!(original.preserved_by(&candidate));
        }
    }

    fn entry(source: &str) -> CheckedBodyContract {
        snapshots(source).into_iter().find(|body| body.owner() == BodyContractOwner::Entry).unwrap()
    }

    #[test]
    fn checked_body_contract_specializes_identity_resource_transfer() {
        let original = declaration("pure identity(value: Int) -> Int { value }\n");
        let generic = declaration("pure identity(value) { value }\n");
        assert!(original.preserved_by(&original));
        assert!(generic.preserved_by(&generic));
        assert!(original.preserved_by(&generic));
        assert!(!generic.preserved_by(&original));
    }

    #[test]
    fn checked_body_contract_specializes_scalar_transfer_roles() {
        for (original, candidate) in [
            ("pure keep(value: Int) -> Int { return value }\n", "pure keep(value) { return value }\n"),
            ("pure keep(value: List[Int?]) -> List[Int?] { value }\n", "pure keep(value) { value }\n"),
            ("pure keep(value: Int) -> List[Int] { [value] }\n", "pure keep(value) { [value] }\n"),
            ("pure keep(value: Int) -> Int { if true { value } else { value } }\n",
             "pure keep(value) { if true { value } else { value } }\n"),
        ] {
            let original = declaration(original);
            let candidate = declaration(candidate);
            assert!(original.preserved_by(&candidate), "{:?} -> {:?}", original, candidate);
        }
    }

    #[test]
    fn checked_body_contract_preserves_proven_integer_driver_storage() {
        let original = entry("let value: Int = 1\n");
        let candidate = entry("let value = 1\n");
        assert!(original.preserved_by(&original));
        assert!(candidate.preserved_by(&candidate));
        assert!(original.preserved_by(&candidate));
        assert!(candidate.preserved_by(&original));
        let mapped = entry("let value: Map[Int] = {}\n");
        let record = entry("let value = {}\n");
        assert!(mapped.preserved_by(&mapped));
        assert!(record.preserved_by(&record));
        assert!(!mapped.preserved_by(&record));
    }

    #[test]
    fn checked_body_contract_preserves_checker_only_reveals() {
        let original = entry("reveal_type(1)\n");
        let candidate = entry("reveal_type(1)\n");
        assert!(original.preserved_by(&candidate));
    }

    #[test]
    fn checked_body_contract_rejects_removed_unsigned_storage_validation() {
        let original = declaration("proc bounded(value: Int) [] -> Int { let checked: UInt = value; checked }\n");
        let candidate = declaration("proc bounded(value: Int) [] -> Int { let checked = value; checked }\n");
        assert!(original.preserved_by(&original));
        assert!(candidate.preserved_by(&candidate));
        assert!(!original.preserved_by(&candidate));
    }

    #[test]
    fn checked_body_contract_preserves_unchecked_integer_storage() {
        let original = declaration("proc bounded(value: Int) [] -> Int { let checked: Int = value; checked }\n");
        let candidate = declaration("proc bounded(value: Int) [] -> Int { let checked = value; checked }\n");
        assert!(original.preserved_by(&candidate));
        assert!(candidate.preserved_by(&original));
    }

    #[test]
    fn checked_body_contract_specializes_only_proven_header_interfaces() {
        let original = declaration("pure keep(value: Int) -> Bool { \"x\".starts_with(\"x\") }\n");
        let generic = declaration("pure keep(value) { \"x\".starts_with(\"x\") }\n");
        assert!(original.preserved_by(&generic));
        let unsigned = declaration("pure keep(value: UInt) -> Bool { \"x\".starts_with(\"x\") }\n");
        assert!(unsigned.preserved_by(&unsigned));
        assert!(!unsigned.preserved_by(&original));
        let first = declaration("type First = {label: Str}\npure keep(value: First) -> Bool { \"x\".starts_with(\"x\") }\n");
        let second = declaration("type Second = {label: Str}\npure keep(value: Second) -> Bool { \"x\".starts_with(\"x\") }\n");
        assert!(first.preserved_by(&first));
        assert!(first.preserved_by(&second));
    }

    #[test]
    fn checked_body_contract_preserves_preparation_diagnostics() {
        let source = "pure invalid() -> Int { \"wrong\" }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("invalid-body-contract.xsh", source);
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty());
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::default());
        let expected = checked.diagnostics.iter().find(|diagnostic| diagnostic.severity == Severity::Error).unwrap().clone();
        let CheckedBodyContractError::Preparation(actual) = checked_body_contracts(checked, &parsed.arena, &sources, source_id).unwrap_err() else { panic!("preserve the checked diagnostic") };
        assert_eq!(actual, expected);
    }

    #[test]
    fn inferred_return_schema_preserves_prepared_constructor_body_contract() {
        let original = "type Disk = { mount: Str, used: Int }\nproc disk(mount: Str) -> Result[Disk] { Disk(mount: mount, used: mount.byte_len()) }\nproc used(mount: Str) [error] -> Result[Int] { disk(mount)?.used }\n";
        let candidate = original.replacen(" -> Result[Disk]", "", 1);
        let original = snapshots(original);
        let candidate = snapshots(&candidate);
        assert_eq!(original.len(), candidate.len());
        assert!(original.iter().zip(&candidate).all(|(old, new)| old.preserved_by(new)));
    }
}
