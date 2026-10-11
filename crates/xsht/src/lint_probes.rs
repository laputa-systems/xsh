use super::{
    ArenaExprKind, ArenaProgram, ArenaStmtKind, BTreeMap, Effect, ExprId, Linter, Severity, Span,
    Type, list_splice_element_type_is_precise, shift_after_deletion,
};
#[cfg(test)]
use super::{annotation_probe_tests, local_annotation_probe_tests};

#[derive(Eq, PartialEq)]
pub(super) struct CheckedReturnRemovalFacts {
    pub(super) expressions: Vec<(usize, usize, String)>,
    pub(super) statements: Vec<(usize, usize, xsh::frontend::check::StatementPosition)>,
    pub(super) returns: Vec<(usize, usize, String)>,
    pub(super) parameters: Vec<(usize, usize, String)>,
    pub(super) effects: BTreeMap<String, Option<Vec<Effect>>>,
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

pub(super) fn checked_local_annotation_facts(
    source: &str,
    source_id: xsh::frontend::source::SourceId,
) -> Option<CheckedLocalAnnotationFacts> {
    #[cfg(test)]
    local_annotation_probe_tests::record_original();
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(source_id, source);
    if !parsed.diagnostics.is_empty() || !standalone_annotation_imports_available(&parsed.arena) {
        return None;
    }
    #[cfg(test)]
    local_annotation_probe_tests::record_original_check();
    let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, source);
    if checked
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return None;
    }
    parsed.arena.symbol_owner().with_current(|| {
        let mut bindings = BTreeMap::new();
        for (span, ty) in &checked.local_binding_types {
            bindings
                .entry(span.start())
                .or_insert_with(|| checked_return_type_shape(ty));
        }
        let expressions = checked
            .expr_types
            .iter()
            .filter(|(span, _)| span.source_id == source_id)
            .map(|(span, ty)| ((span.start(), span.end()), checked_return_type_shape(ty)))
            .collect();
        Some(CheckedLocalAnnotationFacts {
            bindings,
            expressions,
        })
    })
}

/// Source edits must preserve every checked expression and statement purpose,
/// including caller overload selection, conversions, and implicit Result tails.
pub(super) fn checked_return_removal_facts(
    source: &str,
    source_id: xsh::frontend::source::SourceId,
    removed: Option<(usize, usize)>,
) -> Option<CheckedReturnRemovalFacts> {
    #[cfg(test)]
    annotation_probe_tests::record_probe();
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(source_id, source);
    if !parsed.diagnostics.is_empty() || !standalone_annotation_imports_available(&parsed.arena) {
        return None;
    }
    let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, source);
    if !checked.diagnostics.is_empty() {
        return None;
    }
    let original_offset = |offset: usize| match removed {
        Some((start, length)) if offset >= start => offset + length,
        _ => offset,
    };
    parsed.arena.symbol_owner().with_current(|| {
        Some(CheckedReturnRemovalFacts {
            expressions: checked
                .expr_types
                .iter()
                .map(|(span, ty)| {
                    (
                        original_offset(span.start()),
                        original_offset(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            statements: checked
                .statement_positions
                .iter()
                .map(|(span, position)| {
                    (
                        original_offset(span.start()),
                        original_offset(span.end()),
                        *position,
                    )
                })
                .collect(),
            returns: checked
                .function_return_types
                .iter()
                .map(|(span, ty)| {
                    (
                        original_offset(span.start()),
                        original_offset(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            parameters: checked
                .parameter_types
                .iter()
                .map(|(span, ty)| {
                    (
                        original_offset(span.start()),
                        original_offset(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            effects: checked
                .callable_effects
                .into_iter()
                .map(|(name, effects)| {
                    let effects = effects.map(|mut effects| {
                        effects.sort_by_key(Effect::as_str);
                        effects.dedup();
                        effects
                    });
                    (name, effects)
                })
                .collect(),
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
                            format!(
                                "{}:{optional}:{params:?}:{}:{:?}",
                                if matches!(export, ModuleExportType::Pure { .. }) {
                                    "pure"
                                } else {
                                    "proc"
                                },
                                checked_return_type_shape(&sig.return_ty),
                                sig.effects
                            )
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
    pub(super) fn local_annotation_removal_preserves_contract(
        &mut self,
        deletion: Span,
        binding: Span,
    ) -> bool {
        if self.local_annotation_before.is_none() {
            self.local_annotation_before = Some(checked_local_annotation_facts(
                self.source,
                deletion.source_id,
            ));
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
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            deletion.source_id,
            &rewritten,
        );
        if !parsed.diagnostics.is_empty() {
            return false;
        }
        #[cfg(test)]
        local_annotation_probe_tests::record_candidate();
        let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, &rewritten);
        if checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return false;
        }
        parsed.arena.symbol_owner().with_current(|| {
            let shape = checked
                .local_binding_types
                .iter()
                .find(|(span, _)| span.start() == binding.start())
                .map(|(_, ty)| checked_return_type_shape(ty));
            let facts = checked
                .expr_types
                .iter()
                .filter(|(span, _)| span.source_id == deletion.source_id)
                .map(|(span, ty)| ((span.start(), span.end()), checked_return_type_shape(ty)))
                .collect::<BTreeMap<_, _>>();
            shape.as_ref() == Some(old_shape) && facts == old_facts
        })
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
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            edit.source_id,
            &rewritten,
        );
        if !parsed.diagnostics.is_empty() {
            return false;
        }
        let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, &rewritten);
        if !checked.diagnostics.is_empty() {
            return false;
        }
        parsed.arena.symbol_owner().with_current(|| {
            (0..parsed.arena.arena.expr_tags.len()).any(|raw| {
                let expr = parsed.arena.arena.expr(ExprId::from_index(raw));
                let ArenaExprKind::ValuePipelineCall { input, .. } = expr.kind else {
                    return false;
                };
                let input_span = parsed.arena.arena.expr(input).span;
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
