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
    Require(LoweredTypeCheck),
    Assignment(LoweredTypeCheck),
    DriverBinding { check: Option<LoweredTypeCheck>, kind: Option<LoweredType> },
}

impl PreparedValidationContract {
    fn interface_gate(&self) -> bool {
        match self {
            Self::Parameter { check, formal, .. } | Self::Return { check, formal, .. } => check.as_ref().is_none_or(|check| check.schema.is_none() && formal.as_ref() == Some(&check.ty)),
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
        if !self.semantic.preserved_by(&candidate.semantic) { return false; }
        let interface = match (&self.interface, &candidate.interface) {
            (Some(original), Some(candidate)) => original.equivalent_to(candidate) || original.is_instance_of(candidate) == Ok(true),
            (None, None) => true,
            _ => false,
        };
        if !interface || self.validations.len() != candidate.validations.len() { return false; }
        self.validations.iter().all(|(site, original)| candidate.validations.get(site).is_some_and(|candidate| {
            original == candidate || original.len() == candidate.len() && original.iter().zip(candidate).all(|(original, candidate)| original.interface_gate() && candidate.interface_gate())
        }))
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
    for declaration in checked.checked_declarations.functions.values().filter(|declaration| !declaration.internal) {
        if let Some((index, _)) = pending.canonical_site(BodyContractSite::Return { declaration: declaration.body }).map_err(CheckedBodyContractError::Semantic)? {
            interfaces.insert(index, declaration.callable_contract());
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
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{:?}", checked.diagnostics);
        checked_body_contracts(checked, &parsed.arena, &sources, source_id).unwrap()
    }

    fn declaration(source: &str) -> CheckedBodyContract {
        snapshots(source).into_iter().find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap()
    }

    fn entry(source: &str) -> CheckedBodyContract {
        snapshots(source).into_iter().find(|body| body.owner() == BodyContractOwner::Entry).unwrap()
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
        assert!(!first.preserved_by(&second));
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
}
