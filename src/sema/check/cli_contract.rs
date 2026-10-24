use super::CheckedDeclarations;
use crate::sema::cli_entry::{CliEntryDefault, CliEntryPlan};
use crate::source::{SourceMap, Span};
use crate::symbol::{Name, SymbolOwner};
use crate::syntax::arena::ArenaProgram;
use crate::syntax::node::Effect;
use std::collections::BTreeMap;
use std::sync::Arc;

/// An immutable checked CLI parsing contract and declared effect bound,
/// independent of arena-local identities.
#[derive(Clone, Debug)]
pub struct CheckedCliEntryContract {
    source_name: Arc<str>,
    name: Name,
    plan: CliEntryPlan,
    effects: Option<Vec<Effect>>,
    computed_defaults: BTreeMap<usize, Arc<str>>,
    symbol_owner: SymbolOwner,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliContractError {
    MissingSource(Span),
    InvalidSourceSpan(Span),
}

impl CheckedCliEntryContract {
    pub fn source_name(&self) -> &str { &self.source_name }

    pub fn name(&self) -> Name {
        self.symbol_owner.with_current(|| self.name)
    }

    pub fn path(&self) -> &[String] { &self.plan.path }

    /// Compares source identity, command path, ordered checked parameters,
    /// parser domains, declared effect bounds, and defaults, ignoring arena
    /// IDs and source offsets. Help text and body effect summaries remain
    /// separate facts.
    pub fn equivalent_to(&self, other: &Self) -> bool {
        self.source_name == other.source_name && self.name == other.name && self.plan.path == other.plan.path
            && self.effects == other.effects
            && self.plan.parameters.len() == other.plan.parameters.len()
            && self.plan.parameters.iter().zip(&other.plan.parameters).enumerate().all(|(index, (left, right))| {
                left.name == right.name && left.ty == right.ty && left.parser_type == right.parser_type
                    && left.rest == right.rest && match (&left.default, &right.default) {
                        (None, None) => true,
                        (Some(CliEntryDefault::Constant(left)), Some(CliEntryDefault::Constant(right))) => left == right,
                        (Some(CliEntryDefault::Computed(_)), Some(CliEntryDefault::Computed(_))) => {
                            self.computed_defaults.get(&index) == other.computed_defaults.get(&index)
                        }
                        _ => false,
                    }
            })
    }
}

impl CheckedDeclarations {
    /// Captures CLI parsing contracts from the validator's prepared entry plans
    /// and declared effect bounds from their checked declarations.
    /// Every source slice must exist in the matching checked source map.
    pub fn cli_entry_contracts(&self, program: &ArenaProgram, sources: &SourceMap)
        -> Result<Vec<CheckedCliEntryContract>, CliContractError>
    {
        self.cli_entries.iter().map(|plan| {
            let span = program.arena.stmt(plan.statement).span;
            let source = sources.get(span.source_id).ok_or(CliContractError::MissingSource(span))?;
            source.text().get(span.start()..span.end()).ok_or(CliContractError::InvalidSourceSpan(span))?;
            let mut computed_defaults = BTreeMap::new();
            for (index, parameter) in plan.parameters.iter().enumerate() {
                if let Some(CliEntryDefault::Computed(expression)) = &parameter.default {
                    let span = program.arena.expr(*expression).span;
                    let source = sources.get(span.source_id).ok_or(CliContractError::MissingSource(span))?;
                    let text = source.text().get(span.start()..span.end()).ok_or(CliContractError::InvalidSourceSpan(span))?;
                    computed_defaults.insert(index, Arc::from(text));
                }
            }
            let declaration = program.arena.function_def(plan.definition);
            let effects = declaration.effects.map(|range| {
                let declared = program.arena.effects(range).collect::<Vec<_>>();
                Effect::ALL.into_iter().filter(|effect| declared.contains(effect)).collect()
            });
            Ok(CheckedCliEntryContract {
                source_name: Arc::from(source.name()), name: declaration.name,
                plan: plan.clone(), effects, computed_defaults, symbol_owner: program.symbol_owner().clone(),
            })
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::{CheckOptions, Checker};
    use crate::sema::constants::LiteralConstant;
    use crate::sema::types::Type;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn contracts(source: &str) -> Vec<CheckedCliEntryContract> {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("<cli-contract>", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        checked.checked_declarations.cli_entry_contracts(&parsed.arena, &sources).unwrap()
    }

    #[test]
    fn checked_cli_contract_ignores_arena_ids_spans_and_entry_order() {
        let first = contracts("cli main build(jobs: Int = 4) [] {}\ncli main status() [] {}\n");
        let reordered = contracts("type Unrelated = Int\ncli main status() [] {}\n\ncli main build(jobs: Int = 4) [] {}\n");
        assert!(first[0].equivalent_to(&reordered[1]));
        assert!(first[1].equivalent_to(&reordered[0]));
        assert_eq!(first[0].source_name(), "<cli-contract>");
        assert_eq!(first[0].path(), &["build".to_owned()]);
    }

    #[test]
    fn checked_cli_contract_captures_inferred_defaults_and_survives_arena_disposal() {
        let inferred = contracts("cli main(jobs = 4, project_name = \"xsh\") [] {}\n").remove(0);
        let explicit = contracts("cli main(jobs: Int = 4, project_name: Str = \"xsh\") [] {}\n").remove(0);
        SymbolOwner::new().with_current(|| {
            assert!(inferred.equivalent_to(&explicit));
            assert_eq!(inferred.name().as_str(), "main");
            assert_eq!(inferred.plan.parameters[1].name.as_str(), "project_name");
            assert_eq!(inferred.plan.parameters[0].ty, Type::Int);
            assert_eq!(inferred.plan.parameters[0].parser_type, "Int");
        });
    }

    #[test]
    fn checked_cli_contract_rejects_parameter_and_path_mutations() {
        let original = contracts("cli main build(jobs: Int = 4, ...files: List[Str]) [] {}\n").remove(0);
        for source in [
            "cli main build(jobs: Int = 5, ...files: List[Str]) [] {}\n",
            "cli main build(jobs: UInt = 4, ...files: List[Str]) [] {}\n",
            "cli main build(workers: Int = 4, ...files: List[Str]) [] {}\n",
            "cli main build(jobs: Int = 4, files: List[Str] = []) [] {}\n",
            "cli main check(jobs: Int = 4, ...files: List[Str]) [] {}\n",
        ] {
            assert!(!original.equivalent_to(&contracts(source).remove(0)), "{source}");
        }
        let mut parser_domain = original.clone();
        parser_domain.plan.parameters[0].parser_type = "UInt".to_owned();
        assert!(!original.equivalent_to(&parser_domain));
        let mut rest = original.clone();
        rest.plan.parameters[1].rest = false;
        assert!(!original.equivalent_to(&rest));
        let mut source_owner = original.clone();
        source_owner.source_name = Arc::from("<different-source>");
        assert!(!original.equivalent_to(&source_owner));
    }

    #[test]
    fn checked_cli_contract_preserves_computed_default_bytes_and_kind() {
        let first = contracts("pure jobs() -> Int { 4 }\ncli main(count: Int = jobs()) [] {}\n").remove(0);
        let shifted = contracts("\npure jobs() -> Int { 4 }\n\ncli main(count: Int = jobs()) [] {}\n").remove(0);
        let changed = contracts("pure jobs() -> Int { 4 }\ncli main(count: Int = jobs() + 0) [] {}\n").remove(0);
        assert!(first.equivalent_to(&shifted));
        assert_eq!(first.plan.parameters[0].ty, changed.plan.parameters[0].ty);
        assert!(!first.equivalent_to(&changed));
        let mut constant = first.clone();
        constant.plan.parameters[0].default = Some(CliEntryDefault::Constant(LiteralConstant::Int(4)));
        assert!(!first.equivalent_to(&constant));
    }

    #[test]
    fn checked_cli_contract_preserves_declared_effect_bounds() {
        let empty = contracts("cli main() [] {}\n").remove(0);
        let io = contracts("cli main() [io] {}\n").remove(0);
        let omitted = contracts("cli main() {}\n").remove(0);
        let first = contracts("cli main() [io, fs] {}\n").remove(0);
        let reordered = contracts("cli main() [fs, io] {}\n").remove(0);
        assert!(!empty.equivalent_to(&io));
        assert!(!empty.equivalent_to(&omitted));
        assert!(!io.equivalent_to(&omitted));
        assert!(first.equivalent_to(&reordered));
    }

    #[test]
    fn checked_cli_contract_reports_missing_and_invalid_source() {
        let source = "pure jobs() -> Int { 4 }\ncli main(count: Int = jobs()) [] {}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let span = parsed.arena.arena.stmt(checked.checked_declarations.cli_entries[0].statement).span;
        assert_eq!(checked.checked_declarations.cli_entry_contracts(&parsed.arena, &SourceMap::new()).unwrap_err(),
            CliContractError::MissingSource(span));
        let mut truncated = SourceMap::new();
        truncated.add_file("<cli-contract>", "");
        assert_eq!(checked.checked_declarations.cli_entry_contracts(&parsed.arena, &truncated).unwrap_err(),
            CliContractError::InvalidSourceSpan(span));
    }
}
