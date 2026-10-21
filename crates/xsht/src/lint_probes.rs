use super::{
    ArenaExprKind, ArenaProgram, ArenaStmtKind, BTreeMap, ExprId, Linter, Severity, Span,
    Type, list_splice_element_type_is_precise, shift_after_deletion,
};
#[cfg(test)]
use super::{annotation_probe_tests, local_annotation_probe_tests};

pub(crate) struct LintProbeSnapshot {
    pub(crate) program: ArenaProgram,
    pub(crate) sources: Option<xsh::frontend::source::SourceMap>,
    pub(crate) source_id: xsh::frontend::source::SourceId,
    pub(crate) parse_diagnostics: Vec<xsh::diagnostic::Diagnostic>,
    pub(crate) checked: Option<xsh::frontend::check::CheckOutput>,
}

pub(crate) struct LintProbeTransport;

pub(super) fn standalone_sources(
    source_id: xsh::frontend::source::SourceId,
    source: &str,
) -> xsh::frontend::source::SourceMap {
    let mut sources = xsh::frontend::source::SourceMap::new();
    for raw in 0..source_id.raw() {
        sources.add_file(format!("lint-probe-placeholder-{raw}.xsh"), String::new());
    }
    sources.add_file("lint-probe.xsh", source.to_owned());
    sources
}

impl LintProbeTransport {
    pub(crate) fn standalone(
        source_id: xsh::frontend::source::SourceId,
        source: &str,
        options: xsh::frontend::check::CheckOptions,
    ) -> LintProbeSnapshot {
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            source_id, source,
        );
        let checked = parsed.diagnostics.is_empty().then(|| {
            xsh::frontend::check::Checker::check_arena_with_options(
                &parsed.arena,
                source,
                options,
            )
        });
        LintProbeSnapshot {
            program: parsed.arena,
            sources: Some(standalone_sources(source_id, source)),
            source_id,
            parse_diagnostics: parsed.diagnostics,
            checked,
        }
    }

    pub(crate) fn loaded(
        context: &super::ReturnProofContext,
        source: &str,
        options: xsh::frontend::check::CheckOptions,
    ) -> Option<LintProbeSnapshot> {
        let entry = xsh::frontend::symbols::SymbolOwner::new().with_current(|| {
            xsh::frontend::load::parse_load_check_text(
                &context.file,
                source.to_owned(),
                context.module_roots.clone(),
                options,
            )
        });
        let source_id = entry.entry_source_id;
        let source_file = entry.sources.get(source_id)?;
        if source_file.id() != source_id
            || source_file.text().as_bytes() != source.as_bytes()
            || entry.parsed.arena.source_text_source_id() != Some(source_id)
        {
            return None;
        }
        Some(LintProbeSnapshot {
            program: entry.parsed.arena,
            sources: Some(entry.sources),
            source_id,
            parse_diagnostics: entry.parsed.diagnostics,
            checked: entry.checked,
        })
    }

}

pub(super) fn checked_source_effect_bodies(
    program: &ArenaProgram,
    statements: impl Iterator<Item = xsh::frontend::syntax::arena::StmtId>,
    source: &str,
) -> Option<(xsh::frontend::source::SourceId, Vec<Span>)> {
    let mut source_id = None;
    let mut bodies = Vec::new();
    for statement in statements {
        let span = program.arena.stmt(statement).span;
        if span.end() > source.len()
            || !source.is_char_boundary(span.start())
            || !source.is_char_boundary(span.end())
            || source_id
                .replace(span.source_id)
                .is_some_and(|id| id != span.source_id)
        {
            return None;
        }
        let id = match program.arena.stmt(statement).kind {
            ArenaStmtKind::Export(inner) => inner,
            _ => statement,
        };
        let kind = program.arena.stmt(id).kind;
        if let ArenaStmtKind::ProcDef(def)
        | ArenaStmtKind::StreamDef(def)
        | ArenaStmtKind::CliMain(def) = kind
        {
            let body = program.arena.function_def(def).body;
            bodies.push(program.arena.span(program.arena.block(body).span));
        }
    }
    Some((source_id?, bodies))
}

pub(super) fn checked_effect_facts_cover_source(
    checked: &xsh::frontend::check::CheckOutput,
    source_id: xsh::frontend::source::SourceId,
    expected_effect_bodies: &[Span],
) -> bool {
    let source_effect_facts = checked
        .function_effect_facts
        .iter()
        .filter(|(id, _)| id.body.source_id == source_id)
        .collect::<Vec<_>>();
    source_effect_facts.len() == expected_effect_bodies.len()
        && expected_effect_bodies.iter().all(|body| {
            source_effect_facts
                .iter()
                .any(|(id, _)| id.body == *body)
        })
}

pub(super) fn checked_local_annotation_facts_from_output(
    checked: &xsh::frontend::check::CheckOutput,
    source_id: xsh::frontend::source::SourceId,
) -> Option<CheckedLocalAnnotationFacts> {
    if checked
        .diagnostics
        .iter()
        .any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.code != Some(xsh::diagnostic::DiagnosticCode::CheckRedundantParens)
        })
    {
        return None;
    }
    let owner = xsh::frontend::symbols::SymbolOwner::current()?;
    owner.with_current(|| {
        let mut bindings = BTreeMap::new();
        for (span, ty) in &checked.local_binding_types {
            if span.source_id == source_id {
                bindings
                    .entry(span.start())
                    .or_insert_with(|| checked_return_type_shape(ty));
            }
        }
        let expressions = checked
            .expr_types
            .iter()
            .filter(|(span, _)| span.source_id == source_id)
            .map(|(span, ty)| {
                (
                    (span.start(), span.end()),
                    checked_return_type_shape(ty),
                )
            })
            .collect();
        Some(CheckedLocalAnnotationFacts {
            bindings,
            expressions,
        })
    })
}

fn source_facts<'a, T: 'a, R>(
    facts: impl Iterator<Item = (&'a Span, &'a T)>,
    source_id: xsh::frontend::source::SourceId,
    removed: &[(usize, usize)],
    mut project: impl FnMut(&T) -> R,
) -> Vec<(usize, usize, R)> {
    facts
        .filter(|(span, _)| span.source_id == source_id)
        .map(|(span, fact)| {
            (
                original_offset(span.start(), removed),
                original_offset(span.end(), removed),
                project(fact),
            )
        })
        .collect()
}

fn original_offset(offset: usize, removed: &[(usize, usize)]) -> usize {
    removed.iter().fold(offset, |offset, &(start, length)| {
        if offset >= start {
            offset + length
        } else {
            offset
        }
    })
}

pub(super) fn checked_return_removal_facts_from_output(
    checked: &xsh::frontend::check::CheckOutput,
    program: &ArenaProgram,
    sources: &xsh::frontend::source::SourceMap,
    source_id: xsh::frontend::source::SourceId,
    expected_effect_bodies: &[Span],
    removed: &[(usize, usize)],
    source_diagnostics_forbidden: bool,
) -> Option<CheckedReturnRemovalFacts> {
    if checked
        .diagnostics
        .iter()
        .any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.code != Some(xsh::diagnostic::DiagnosticCode::CheckRedundantParens)
        })
        || (source_diagnostics_forbidden
            && checked.diagnostics.iter().any(|diagnostic| {
                diagnostic.code != Some(xsh::diagnostic::DiagnosticCode::CheckRedundantParens)
            }))
    {
        return None;
    }
    if !checked_effect_facts_cover_source(checked, source_id, expected_effect_bodies) {
        return None;
    }
    let owner = xsh::frontend::symbols::SymbolOwner::current()?;
    let body_contracts = checked_source_body_contracts(
        checked,
        program,
        sources,
        source_id,
        expected_effect_bodies,
    )?;
    owner.with_current(|| {
        Some(CheckedReturnRemovalFacts {
            expressions: source_facts(
                checked.expr_types.iter(),
                source_id,
                removed,
                checked_return_type_shape,
            ),
            statements: source_facts(
                checked.statement_positions.iter(),
                source_id,
                removed,
                |position| *position,
            ),
            returns: source_facts(
                checked.function_return_types.iter(),
                source_id,
                removed,
                checked_return_type_shape,
            ),
            parameters: source_facts(
                checked.parameter_types.iter(),
                source_id,
                removed,
                checked_return_type_shape,
            ),
            body_contracts,
        })
    })
}

pub(super) struct CheckedReturnRemovalFacts {
    pub(super) expressions: Vec<(usize, usize, String)>,
    pub(super) statements: Vec<(usize, usize, xsh::frontend::check::StatementPosition)>,
    pub(super) returns: Vec<(usize, usize, String)>,
    pub(super) parameters: Vec<(usize, usize, String)>,
    pub(super) body_contracts: BTreeMap<(String, String), xsh::execution::evaluator::CheckedBodyContract>,
}

impl CheckedReturnRemovalFacts {
    pub(super) fn preserved_by(&self, candidate: &Self) -> bool {
        self.expressions == candidate.expressions
            && self.statements == candidate.statements
            && self.returns == candidate.returns
            && self.parameters == candidate.parameters
            && self.body_contracts.len() == candidate.body_contracts.len()
            && self.body_contracts.iter().all(|(key, original)| {
                candidate
                    .body_contracts
                    .get(key)
                    .is_some_and(|candidate| original.preserved_by(candidate))
            })
    }
}

pub(super) fn checked_source_body_contracts(
    checked: &xsh::frontend::check::CheckOutput,
    program: &ArenaProgram,
    sources: &xsh::frontend::source::SourceMap,
    source_id: xsh::frontend::source::SourceId,
    expected_effect_bodies: &[Span],
) -> Option<BTreeMap<(String, String), xsh::execution::evaluator::CheckedBodyContract>> {
    use xsh::execution::evaluator::checked_body_contracts;
    use xsh::frontend::check::BodyContractOwner;

    if !checked_effect_facts_cover_source(checked, source_id, expected_effect_bodies) {
        return None;
    }
    let execution_checked = (!checked.embedded_bodies_checked).then(|| {
        program.symbol_owner().with_current(|| {
            xsh::frontend::check::Checker::check_arena_with_sources_and_options(
                program,
                sources,
                source_id,
                xsh::frontend::check::CheckOptions::for_execution(),
            )
        })
    });
    let checked = execution_checked.as_ref().unwrap_or(checked);
    if !checked_effect_facts_cover_source(checked, source_id, expected_effect_bodies) {
        return None;
    }
    let source_effect_facts = checked
        .function_effect_facts
        .iter()
        .filter(|(id, _)| id.body.source_id == source_id)
        .collect::<Vec<_>>();
    let source_name = sources.get(source_id)?.name().to_owned();
    let contracts = checked_body_contracts(checked.clone(), program, sources, source_id).ok()?;
    program.symbol_owner().with_current(|| {
        let mut effect_body_count = 0;
        let mut source_contracts = BTreeMap::new();
        for contract in contracts {
            if contract.source_name() != source_name {
                continue;
            }
            let name = match contract.owner() {
                BodyContractOwner::Declaration(name) => name.as_str().to_string(),
                BodyContractOwner::Entry => continue,
            };
            effect_body_count += usize::from(contract.effect_fact().is_some());
            if source_contracts
                .insert((source_name.clone(), name), contract)
                .is_some()
            {
                return None;
            }
        }
        (effect_body_count == source_effect_facts.len()).then_some(source_contracts)
    })
}

// Original facts are independent of each deletion. Keep their source coordinates
// and complete type shapes so every candidate still proves the same contract.
pub(super) struct CheckedLocalAnnotationFacts {
    pub(super) bindings: BTreeMap<usize, String>,
    pub(super) expressions: BTreeMap<(usize, usize), String>,
}

// Standalone proof parses do not load user modules. An invalid unresolved
// top-level use always errors, so its source cannot supply a proof baseline.
pub(super) fn standalone_annotation_imports_available(program: &ArenaProgram) -> bool {
    program.symbol_owner().with_current(|| {
        program.statement_ids().all(|statement| {
            let kind = match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                other => other,
            };
            let ArenaStmtKind::Use(import) = kind else {
                return true;
            };
            let import = program.arena.use_stmt(import);
            if import.resolved.is_some() {
                return true;
            }
            let mut path = program.arena.names(import.path);
            let Some(name) = path.next() else {
                return false;
            };
            path.next().is_none()
                && import.alias.is_none()
                && xsh::api::api_spec().module(&name.as_str()).is_some()
        })
    })
}

// Type display intentionally hides structural fields. Compare their complete
// checked shapes with names rendered under each source's symbol owner.
pub(super) fn checked_return_type_shape(ty: &Type) -> String {
    match ty {
        Type::ErasedRecord => "ErasedRecord".to_string(),
        Type::Record(fields) => format!(
            "Record{:?}",
            fields
                .iter()
                .map(|(name, ty)| (name.to_string(), checked_return_type_shape(ty)))
                .collect::<BTreeMap<_, _>>()
        ),
        Type::Module(exports) => format!(
            "Module{:?}",
            exports
                .iter()
                .map(|(name, export)| {
                    use xsh::frontend::check::ModuleExportType;
                    let shape = match export {
                        ModuleExportType::Value { ty, optional } => {
                            format!("value:{optional}:{}", checked_return_type_shape(ty))
                        }
                        ModuleExportType::Pure { sig, optional }
                        | ModuleExportType::Proc { sig, optional } => {
                            let params = sig
                                .params
                                .iter()
                                .map(|param| {
                                    (
                                        param.name.to_string(),
                                        checked_return_type_shape(&param.ty),
                                        param.defaulted,
                                        param.rest,
                                    )
                                })
                                .collect::<Vec<_>>();
                            let effects = match sig.effects.concrete() {
                                Ok(Some(effects)) => format!("known:{effects:?}"),
                                Ok(None) => "unknown".to_owned(),
                                Err(_) => "symbolic".to_owned(),
                            };
                            format!(
                                "{}:{optional}:{params:?}:{}:{effects}",
                                if matches!(export, ModuleExportType::Pure { .. }) {
                                    "pure"
                                } else {
                                    "proc"
                                },
                                checked_return_type_shape(&sig.return_ty)
                            )
                        }
                        ModuleExportType::GenericCallable { kind, optional } => {
                            format!("generic-{kind:?}:{optional}")
                        }
                    };
                    (name.to_string(), shape)
                })
                .collect::<BTreeMap<_, _>>()
        ),
        Type::List(inner) => format!("List[{}]", checked_return_type_shape(inner)),
        Type::Map(key, inner) => {
            if matches!(key.as_ref(), Type::Str) {
                format!("Map[{}]", checked_return_type_shape(inner))
            } else {
                format!(
                    "Map[{}, {}]",
                    checked_return_type_shape(key),
                    checked_return_type_shape(inner)
                )
            }
        }
        Type::Stream(inner) => format!("Stream[{}]", checked_return_type_shape(inner)),
        Type::Optional(inner) => format!("Optional[{}]", checked_return_type_shape(inner)),
        Type::Result(ok, error) => format!(
            "Result[{}, {}]",
            checked_return_type_shape(ok),
            checked_return_type_shape(error)
        ),
        _ => ty.to_string(),
    }
}

impl<'a> Linter<'a> {
    pub(super) fn checked_return_removal_probe(
        &self,
        source: &str,
        source_id: xsh::frontend::source::SourceId,
        removed: Option<(usize, usize)>,
        source_diagnostics_forbidden: bool,
    ) -> Option<CheckedReturnRemovalFacts> {
        #[cfg(test)]
        annotation_probe_tests::record_probe();
        if self.source_id != Some(source_id) {
            return None;
        }
        let removed = removed.into_iter().collect::<Vec<_>>();
        if source == self.source && removed.is_empty() {
            if let Some(checked) = self.checked_output.as_deref() {
                if !checked_effect_facts_cover_source(
                    checked,
                    source_id,
                    &self.effect_body_spans,
                ) {
                    return None;
                }
            }
        }
        if source == self.source && removed.is_empty() && self.return_proof.is_none() {
            let checked = self.checked_output.as_deref()?;
            let sources = standalone_sources(source_id, source);
            return checked_return_removal_facts_from_output(
                checked,
                self.program,
                &sources,
                source_id,
                &self.effect_body_spans,
                &removed,
                source_diagnostics_forbidden,
            );
        }
        let snapshot = if let Some(context) = self.return_proof.as_ref() {
            LintProbeTransport::loaded(
                context,
                source,
                xsh::frontend::check::CheckOptions::for_execution(),
            )?
        } else {
            LintProbeTransport::standalone(
                source_id,
                source,
                xsh::frontend::check::CheckOptions::for_execution(),
            )
        };
        if !snapshot.parse_diagnostics.is_empty() {
            return None;
        }
        if self.return_proof.is_none()
            && !standalone_annotation_imports_available(&snapshot.program)
        {
            return None;
        }
        let checked = snapshot.checked.as_ref()?;
        let sources = snapshot.sources.as_ref()?;
        let (source_id, effect_body_spans) = checked_source_effect_bodies(
            &snapshot.program,
            snapshot.program.arena.stmt_ids(snapshot.program.statements),
            source,
        )?;
        snapshot.program.symbol_owner().with_current(|| {
            checked_return_removal_facts_from_output(
                checked,
                &snapshot.program,
                sources,
                source_id,
                &effect_body_spans,
                &removed,
                source_diagnostics_forbidden,
            )
        })
    }

    fn local_annotation_baseline_facts(
        &self,
        source_id: xsh::frontend::source::SourceId,
    ) -> Option<CheckedLocalAnnotationFacts> {
        #[cfg(test)]
        local_annotation_probe_tests::record_original();
        if self.source_id != Some(source_id) {
            return None;
        }
        if let Some(checked) = self.checked_output.as_deref() {
            return checked_local_annotation_facts_from_output(checked, source_id);
        }
        None
    }

    fn local_annotation_candidate_facts(
        &self,
        source: &str,
        source_id: xsh::frontend::source::SourceId,
    ) -> Option<CheckedLocalAnnotationFacts> {
        if self.source_id != Some(source_id) {
            return None;
        }
        let snapshot = if let Some(context) = self.return_proof.as_ref() {
            LintProbeTransport::loaded(
                context,
                source,
                xsh::frontend::check::CheckOptions::default(),
            )?
        } else {
            LintProbeTransport::standalone(
                source_id,
                source,
                xsh::frontend::check::CheckOptions::default(),
            )
        };
        if !snapshot.parse_diagnostics.is_empty() {
            return None;
        }
        if self.return_proof.is_none()
            && !standalone_annotation_imports_available(&snapshot.program)
        {
            return None;
        }
        let checked = snapshot.checked.as_ref()?;
        snapshot.program.symbol_owner().with_current(|| {
            checked_local_annotation_facts_from_output(checked, snapshot.source_id)
        })
    }

    pub(super) fn local_annotation_removal_preserves_contract(
        &mut self,
        deletion: Span,
        binding: Span,
    ) -> bool {
        if self.local_annotation_before.is_none() {
            self.local_annotation_before =
                Some(self.local_annotation_baseline_facts(deletion.source_id));
        }
        let Some(before) = self.local_annotation_before.as_ref().unwrap().as_ref() else {
            return false;
        };
        let Some(old_shape) = before.bindings.get(&binding.start()) else {
            return false;
        };
        let old_facts = before
            .expressions
            .iter()
            .map(|((start, end), shape)| {
                (
                    (
                        shift_after_deletion(*start, deletion),
                        shift_after_deletion(*end, deletion),
                    ),
                    shape.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut rewritten = self.source.to_string();
        rewritten.replace_range(deletion.range(), "");
        #[cfg(test)]
        local_annotation_probe_tests::record_candidate();
        let Some(after) = self.local_annotation_candidate_facts(&rewritten, deletion.source_id)
        else {
            return false;
        };
        after.bindings.get(&binding.start()) == Some(old_shape) && after.expressions == old_facts
    }

    pub(super) fn pipeline_rewrite_preserves_types(
        &self,
        edit: Span,
        replacement: &str,
        old_value: ExprId,
        old_input: ExprId,
        new_value_start: usize,
        new_input_start: usize,
    ) -> bool {
        let Some(old_type) = self.expr_types.get(&self.arena.expr(old_value).span) else {
            return false;
        };
        let Some(input_type) = self.expr_types.get(&self.arena.expr(old_input).span) else {
            return false;
        };
        if !list_splice_element_type_is_precise(old_type)
            || !list_splice_element_type_is_precise(input_type)
        {
            return false;
        }
        let old_shape = checked_return_type_shape(old_type);
        let input_shape = checked_return_type_shape(input_type);
        let mut rewritten = self.source.to_string();
        rewritten.replace_range(edit.range(), replacement);
        let snapshot = if let Some(context) = self.return_proof.as_ref() {
            let Some(snapshot) = LintProbeTransport::loaded(
                context,
                &rewritten,
                xsh::frontend::check::CheckOptions::default(),
            ) else {
                return false;
            };
            snapshot
        } else {
            LintProbeTransport::standalone(
                edit.source_id,
                &rewritten,
                xsh::frontend::check::CheckOptions::default(),
            )
        };
        if !snapshot.parse_diagnostics.is_empty() {
            return false;
        }
        let Some(checked) = snapshot.checked.as_ref() else {
            return false;
        };
        if !checked.diagnostics.is_empty() {
            return false;
        }
        snapshot.program.symbol_owner().with_current(|| {
            (0..snapshot.program.arena.expr_tags.len()).any(|raw| {
                let expr = snapshot.program.arena.expr(ExprId::from_index(raw));
                let ArenaExprKind::ValuePipelineCall { input, .. } = expr.kind else {
                    return false;
                };
                let input_span = snapshot.program.arena.expr(input).span;
                expr.span.start() == new_value_start
                    && input_span.start() == new_input_start
                    && input_span.range().len() == self.arena.expr(old_input).span.range().len()
                    && checked
                        .expr_types
                        .get(&expr.span)
                        .is_some_and(|ty| checked_return_type_shape(ty) == old_shape)
                    && checked
                        .expr_types
                        .get(&input_span)
                        .is_some_and(|ty| checked_return_type_shape(ty) == input_shape)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_return_removal_facts_from_output, checked_source_effect_bodies, standalone_sources};
    use std::collections::BTreeMap;
    use xsh::diagnostic::Diagnostic;
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::{SourceId, Span};
    use xsh::frontend::syntax::parser::Parser;

    fn checked_body_contracts(
        source: &str,
    ) -> BTreeMap<(String, String), xsh::execution::evaluator::CheckedBodyContract> {
        let source_id = SourceId::new(0);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = standalone_sources(source_id, source);
        let checked = Checker::check_arena_with_sources_and_options(
            &parsed.arena,
            &sources,
            source_id,
            xsh::frontend::check::CheckOptions::for_execution(),
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (_, effect_bodies) = checked_source_effect_bodies(
            &parsed.arena,
            parsed.arena.statement_ids(),
            source,
        )
        .expect("source declarations have effect facts");
        let source_name = sources.get(source_id).unwrap().name();
        let source_fact_count = checked
            .function_effect_facts
            .iter()
            .filter(|(id, _)| id.body.source_id == source_id)
            .count();
        let checked_contracts = parsed.arena.symbol_owner().with_current(|| {
            super::checked_source_body_contracts(
                &checked,
                &parsed.arena,
                &sources,
                source_id,
                &effect_bodies,
            )
        });
        assert!(
            checked_contracts.is_some(),
            "source={source_name} facts={source_fact_count} bodies={}",
            effect_bodies.len()
        );
        checked_contracts.unwrap()
    }

    #[test]
    fn checked_source_contracts_preserve_local_renaming_and_reject_contract_changes() {
        let source = "proc fallible(payload: Int) [env, error] -> Int { let _ = env.get(\"XSH_INFERRED_CALLBACK\"); assert payload > 0; payload }\nproc output(input: Int) [io] -> Int { print $input; input }\nproc combine(left, right, value) { let argument = value; let _ = try { left(argument) }; right(argument) }\nproc bounded() [env, io] -> Result[Int] { combine(fallible, output, 7) }\n";
        let renamed = source
            .replace("argument", "item");
        let renamed_formal = source
            .replace("combine(left, right, value)", "combine(first, right, value)")
            .replace("left(argument)", "first(argument)");
        let uncaptured = source
            .replace("try { left(argument) }", "left(argument)")
            .replace("[env, io] -> Result[Int]", "[env, error, io] -> Result[Int]");
        let original = checked_body_contracts(source);
        let renamed = checked_body_contracts(&renamed);
        let renamed_formal = checked_body_contracts(&renamed_formal);
        let uncaptured = checked_body_contracts(&uncaptured);
        let key = ("lint-probe.xsh".to_owned(), "combine".to_owned());

        assert!(original[&key].preserved_by(&renamed[&key]));
        assert!(!original[&key].preserved_by(&renamed_formal[&key]));
        assert!(!original[&key].preserved_by(&uncaptured[&key]));
    }

    #[test]
    fn body_contract_probe_does_not_repopulate_missing_original_effect_facts() {
        let source_id = SourceId::new(0);
        let source = "proc stable() [fs] -> Int { 1 }\n";
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = standalone_sources(source_id, source);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (_, effect_bodies) = checked_source_effect_bodies(
            &parsed.arena,
            parsed.arena.statement_ids(),
            source,
        )
        .expect("the source owns its statements");
        assert_eq!(checked.function_effect_facts.len(), effect_bodies.len());
        checked.function_effect_facts.clear();

        let contracts = parsed.arena.symbol_owner().with_current(|| {
            super::checked_source_body_contracts(
                &checked,
                &parsed.arena,
                &sources,
                source_id,
                &effect_bodies,
            )
        });
        assert!(contracts.is_none(), "incomplete original facts veto the proof");
    }

    #[test]
    fn return_proof_keeps_the_clean_entire_bundle_requirement() {
        let source_id = SourceId::new(0);
        let source = "let value = 1\n";
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        checked.diagnostics.push(
            Diagnostic::warning("diagnostic owned by another workspace source")
                .with_span(Span::new(SourceId::new(1), 0, 0)),
        );
        let (_, effect_bodies) = checked_source_effect_bodies(
            &parsed.arena,
            parsed.arena.statement_ids(),
            source,
        )
        .expect("the original source owns its statements");
        let sources = standalone_sources(source_id, source);

        let facts = parsed.arena.symbol_owner().with_current(|| {
            checked_return_removal_facts_from_output(
                &checked,
                &parsed.arena,
                &sources,
                source_id,
                &effect_bodies,
                &[],
                true,
            )
        });
        assert!(facts.is_none(), "workspace diagnostics still veto this proof");
    }

    #[test]
    fn return_proof_allows_redundant_parentheses_but_rejects_other_errors() {
        let source_id = SourceId::new(0);
        let source = "let value = (1) + (2)\n";
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(xsh::diagnostic::DiagnosticCode::CheckRedundantParens)
        }));
        let (_, effect_bodies) = checked_source_effect_bodies(
            &parsed.arena,
            parsed.arena.statement_ids(),
            source,
        )
        .expect("the original source owns its statements");
        let sources = standalone_sources(source_id, source);
        let facts = parsed.arena.symbol_owner().with_current(|| {
            checked_return_removal_facts_from_output(
                &checked,
                &parsed.arena,
                &sources,
                source_id,
                &effect_bodies,
                &[],
                true,
            )
        });
        assert!(facts.is_some(), "redundant parentheses preserve proof facts");

        let source = "let value = (1) + (true)\n";
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == xsh::diagnostic::Severity::Error
                && diagnostic.code
                    != Some(xsh::diagnostic::DiagnosticCode::CheckRedundantParens)
        }));
        let (_, effect_bodies) = checked_source_effect_bodies(
            &parsed.arena,
            parsed.arena.statement_ids(),
            source,
        )
        .expect("the invalid source still owns its statements");
        let sources = standalone_sources(source_id, source);
        let facts = parsed.arena.symbol_owner().with_current(|| {
            checked_return_removal_facts_from_output(
                &checked,
                &parsed.arena,
                &sources,
                source_id,
                &effect_bodies,
                &[],
                true,
            )
        });
        assert!(facts.is_none(), "other checker errors still veto proofs");
    }
}
