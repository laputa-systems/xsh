use super::{BTreeMap, CheckBundleView, Effect, Name, Span};
use crate::diagnostic::DiagnosticCode;
use std::collections::BTreeSet;

/// A declaring module distinguishes separately parsed arenas whose local spans
/// overlap. Bundled source spans remain usable as the tooling projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EffectDeclarationId {
    pub namespace: Option<Name>,
    pub body: Span,
}

/// Missing clauses infer a contract; an explicit clause remains the
/// caller-visible upper bound even when its body needs fewer effects.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionEffectFact {
    pub effective: Option<Vec<Effect>>,
    pub required: Option<Vec<Effect>>,
    pub inferred: bool,
    pub inference_allowed: bool,
    /// The clause names exactly the set its declaration would infer without
    /// it, so deleting the clause changes no caller-visible contract.
    pub redundant_clause: bool,
    pub unknown_chain: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct EffectSummary {
    pub known: Vec<Effect>,
    pub unknown_chain: Vec<String>,
}

impl EffectSummary {
    pub fn effects(&self) -> Option<Vec<Effect>> {
        self.unknown_chain.is_empty().then(|| self.known.clone())
    }

    fn add(&mut self, effect: Effect) -> bool {
        if self.known.contains(&effect) {
            return false;
        }
        self.known.push(effect);
        self.known.sort_by_key(Effect::as_str);
        true
    }

    fn unknown(&mut self, chain: Vec<String>) -> bool {
        if chain.is_empty() {
            return false;
        }
        if self.unknown_chain.is_empty()
            || (chain.len(), &chain) < (self.unknown_chain.len(), &self.unknown_chain)
        {
            self.unknown_chain = chain;
            return true;
        }
        false
    }
}

/// A summary edge reads the target's solved summary; a contract edge reads the
/// target's declared clause. Keeping contract edges distinct lets the solver
/// ask what a declared callee would infer without its clause.
#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectEdge {
    target: EffectDeclarationId,
    captures_error: bool,
    contract: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EffectNode {
    pub name: String,
    pub inferred: bool,
    pub inference_allowed: bool,
    pub declared: Option<Vec<Effect>>,
    direct: EffectSummary,
    edges: Vec<EffectEdge>,
}

/// Checked calls populate this graph once. The solver never interprets syntax
/// or guesses which host operation a similarly named call might execute.
#[derive(Clone, Debug, Default)]
pub(super) struct EffectGraph {
    nodes: BTreeMap<EffectDeclarationId, EffectNode>,
}

impl EffectGraph {
    pub(super) fn contains(&self, id: EffectDeclarationId) -> bool {
        self.nodes.contains_key(&id)
    }

    pub(super) fn declarations_for_root(view: &CheckBundleView<'_>) -> Self {
        Self::declarations_for_source(view, view.root_source_id(), None)
    }

    pub(super) fn declarations_for_module(
        view: &CheckBundleView<'_>,
        module: &crate::syntax::arena::ArenaUserModule,
    ) -> Self {
        let source_id = view
            .program
            .module_statements(module)
            .next()
            .map(|statement| view.program.arena.stmt(statement).span.source_id)
            .or_else(|| view.program.module_doc_for(module.statements).map(|span| span.source_id));
        Self::declarations_for_source(view, source_id, Some(module.name))
    }

    /// Return only declaration-level facts for an owner whose body is not checked.
    pub(super) fn declaration_facts(&self) -> BTreeMap<EffectDeclarationId, FunctionEffectFact> {
        let summaries = self
            .nodes
            .keys()
            .map(|&identity| (identity, EffectSummary::default()))
            .collect();
        self.facts(&summaries)
    }

    fn declarations_for_source(
        view: &CheckBundleView<'_>,
        source_id: Option<crate::source::SourceId>,
        namespace: Option<Name>,
    ) -> Self {
        let Some(source_id) = source_id else {
            return Self::default();
        };
        let program = view.program;
        let mut graph = Self::default();
        for statement_id in program.arena.source_rows().stmt_ids(source_id) {
            let statement = program.arena.stmt(statement_id);
            let definition = match statement.kind {
                ArenaStmtKind::ProcDef(definition)
                | ArenaStmtKind::StreamDef(definition)
                | ArenaStmtKind::CliMain(definition) => definition,
                _ => continue,
            };
            let def = program.arena.function_def(definition);
            let id = EffectDeclarationId {
                namespace,
                body: program.arena.span(program.arena.block(def.body).span),
            };
            let declared = def
                .effects
                .map(|effects| program.arena.effects(effects).collect());
            let inference_allowed = !def.test_declaration;
            let name = namespace
                .map(|namespace| format!("{namespace}.{}", def.name))
                .unwrap_or_else(|| def.name.to_string());
            graph.declare(id, name, inference_allowed, declared);
        }
        graph
    }

    pub fn declare(
        &mut self,
        id: EffectDeclarationId,
        name: String,
        inference_allowed: bool,
        declared: Option<Vec<Effect>>,
    ) {
        self.nodes.entry(id).or_insert_with(|| EffectNode {
            name,
            inferred: inference_allowed && declared.is_none(),
            inference_allowed,
            declared,
            direct: EffectSummary::default(),
            edges: Vec::new(),
        });
    }

    pub fn is_inferred(&self, id: EffectDeclarationId) -> bool {
        self.nodes.get(&id).is_some_and(|node| node.inferred)
    }

    pub fn require(
        &mut self,
        owner: Option<EffectDeclarationId>,
        effect: Effect,
        captures_error: bool,
    ) {
        if captures_error && effect == Effect::Error {
            return;
        }
        if let Some(node) = owner.and_then(|owner| self.nodes.get_mut(&owner)) {
            node.direct.add(effect);
        }
    }

    pub fn unknown(&mut self, owner: Option<EffectDeclarationId>, name: String) {
        if let Some(node) = owner.and_then(|owner| self.nodes.get_mut(&owner)) {
            node.direct.unknown(vec![name]);
        }
    }

    pub fn call(
        &mut self,
        owner: Option<EffectDeclarationId>,
        target: EffectDeclarationId,
        captures_error: bool,
    ) {
        let contract = self
            .nodes
            .get(&target)
            .is_some_and(|node| !node.inferred && node.declared.is_some());
        if let Some(node) = owner.and_then(|owner| self.nodes.get_mut(&owner))
            && !node.edges.iter().any(|edge| {
                edge.target == target
                    && edge.captures_error == captures_error
                    && edge.contract == contract
            })
        {
            node.edges.push(EffectEdge {
                target,
                captures_error,
                contract,
            });
        }
    }

    pub(super) fn solve_with_external(
        &self,
        external: &BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> BTreeMap<EffectDeclarationId, EffectSummary> {
        self.solve_with(&BTreeSet::new(), external)
    }

    fn solve_with(
        &self,
        unclaused: &BTreeSet<EffectDeclarationId>,
        external: &BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> BTreeMap<EffectDeclarationId, EffectSummary> {
        assert!(unclaused.iter().all(|id| self.nodes.contains_key(id)), "unclaused declarations belong to the effect graph");
        let mut summaries = self
            .nodes
            .iter()
            .map(|(&id, node)| (id, node.direct.clone()))
            .collect::<BTreeMap<_, _>>();
        loop {
            let mut changed = false;
            for (&id, node) in &self.nodes {
                for edge in &node.edges {
                    let target = if edge.contract && !unclaused.contains(&edge.target) {
                        EffectSummary {
                            known: self.nodes[&edge.target]
                                .declared
                                .clone()
                                .unwrap_or_default(),
                            unknown_chain: Vec::new(),
                        }
                    } else {
                        let Some(target) = summaries
                            .get(&edge.target)
                            .or_else(|| external.get(&edge.target))
                            .cloned()
                        else {
                            continue;
                        };
                        target
                    };
                    let summary = summaries
                        .get_mut(&id)
                        .expect("every declaration has an initial summary");
                    for effect in target.known {
                        if !(edge.captures_error && effect == Effect::Error) {
                            changed |= summary.add(effect);
                        }
                    }
                    if !target.unknown_chain.is_empty() {
                        let mut chain = target.unknown_chain;
                        if let Some(target) = self.nodes.get(&edge.target) {
                            chain.insert(0, target.name.clone());
                        }
                        changed |= summary.unknown(chain);
                    }
                }
            }
            if !changed {
                return summaries;
            }
        }
    }

    fn redundant_clause(
        &self,
        id: EffectDeclarationId,
        node: &EffectNode,
        summary: &EffectSummary,
    ) -> bool {
        let Some(declared) = &node.declared else {
            return false;
        };
        let same = |known: &[Effect]| {
            known.len() == declared.len() && declared.iter().all(|effect| known.contains(effect))
        };
        if !node.inference_allowed || summary.effects().is_none_or(|known| !same(&known)) {
            return false;
        }
        if !self.reaches_own_contract(id) {
            return true;
        }
        self.solve_with(&BTreeSet::from([id]), &BTreeMap::new())[&id]
            .effects()
            .is_some_and(|known| same(&known))
    }

    fn reaches_own_contract(&self, id: EffectDeclarationId) -> bool {
        let mut pending = vec![id];
        let mut seen = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current) {
                continue;
            }
            for edge in self.nodes.get(&current).map_or(&[][..], |node| &node.edges) {
                if edge.target == id {
                    return true;
                }
                if !edge.contract {
                    pending.push(edge.target);
                }
            }
        }
        false
    }

    pub fn facts(
        &self,
        summaries: &BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> BTreeMap<EffectDeclarationId, FunctionEffectFact> {
        self.nodes
            .iter()
            .map(|(id, node)| {
                let summary = &summaries[id];
                (
                    *id,
                    FunctionEffectFact {
                        effective: if node.inferred {
                            summary.effects()
                        } else {
                            node.declared.clone()
                        },
                        required: summary.effects(),
                        inferred: node.inferred,
                        inference_allowed: node.inference_allowed,
                        redundant_clause: self.redundant_clause(*id, node, summary),
                        unknown_chain: summary.unknown_chain.clone(),
                    },
                )
            })
            .collect()
    }
}

use super::{Checker, FunctionSig};
use super::effect_bounds::ExcludedEffect;
use crate::syntax::arena::{ArenaFunctionDef, ArenaProgram, ArenaStmtKind, BlockId};

#[cfg(test)]
mod joint_effect_tests {
    use super::*;
    use crate::source::SourceId;

    fn declaration(offset: usize) -> EffectDeclarationId {
        EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), offset, offset + 1) }
    }

    fn recursive_pair(declared: bool) -> (EffectGraph, EffectDeclarationId, EffectDeclarationId) {
        let first = declaration(1);
        let second = declaration(2);
        let mut graph = EffectGraph::default();
        graph.declare(first, "first".into(), true, declared.then(|| vec![Effect::Time]));
        graph.declare(second, "second".into(), true, declared.then(|| vec![Effect::Time]));
        graph.call(Some(first), second, false);
        graph.call(Some(second), first, false);
        (graph, first, second)
    }

    #[test]
    fn joint_clause_counterfactual_drops_mutually_supported_effects() {
        let (graph, first, second) = recursive_pair(true);
        let original = graph.solve_with_external(&BTreeMap::new());
        let facts = graph.facts(&original);
        assert!(facts[&first].redundant_clause);
        assert!(facts[&second].redundant_clause);
        let (inferred, _, _) = recursive_pair(false);
        let selected = BTreeSet::from([first, second]);
        let counterfactual = graph.solve_with(&selected, &BTreeMap::new());
        assert_eq!(counterfactual, inferred.solve_with_external(&BTreeMap::new()));
        assert!(selected.iter().all(|id| counterfactual[id].effects() == Some(Vec::new())));
        assert_eq!(graph.facts(&original), facts);
        assert_eq!(graph.facts(&graph.solve_with_external(&BTreeMap::new())), facts);
    }

    #[test]
    fn joint_clause_counterfactual_preserves_empty_and_singleton_contracts() {
        let (mut graph, first, second) = recursive_pair(true);
        let recursive = declaration(3);
        graph.declare(recursive, "recursive".into(), true, Some(vec![Effect::Time]));
        graph.call(Some(recursive), recursive, false);
        let ordinary = graph.solve_with(&BTreeSet::new(), &BTreeMap::new());
        for id in [first, second, recursive] { assert_eq!(ordinary[&id].effects(), Some(vec![Effect::Time])); }
        let singleton = graph.solve_with(&BTreeSet::from([first]), &BTreeMap::new());
        assert_eq!(singleton[&first].effects(), Some(vec![Effect::Time]));
        assert_eq!(singleton[&second].effects(), Some(vec![Effect::Time]));
        assert!(!graph.facts(&ordinary)[&recursive].redundant_clause);
        let self_removed = graph.solve_with(&BTreeSet::from([recursive]), &BTreeMap::new());
        assert_eq!(self_removed[&recursive].effects(), Some(Vec::new()));
        assert_eq!(self_removed[&first].effects(), Some(vec![Effect::Time]));
    }

    #[test]
    fn joint_clause_counterfactual_retains_recursive_clock_requirement() {
        let (mut graph, first, second) = recursive_pair(true);
        graph.require(Some(second), Effect::Time, false);
        let original = graph.solve_with_external(&BTreeMap::new());
        let counterfactual = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
        assert_eq!(counterfactual, original);
        assert_eq!(counterfactual[&first].effects(), Some(vec![Effect::Time]));
        assert_eq!(counterfactual[&second].effects(), Some(vec![Effect::Time]));
        assert!(graph.facts(&original).values().all(|fact| fact.redundant_clause));
    }

    #[test]
    fn joint_clause_counterfactual_retains_unselected_authored_bound() {
        let (mut graph, first, second) = recursive_pair(true);
        let boundary = declaration(3);
        graph.declare(boundary, "boundary".into(), true, Some(vec![Effect::Time]));
        graph.call(Some(second), boundary, false);
        let original = graph.solve_with_external(&BTreeMap::new());
        let facts = graph.facts(&original);
        let counterfactual = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
        assert_eq!(counterfactual[&first].effects(), Some(vec![Effect::Time]));
        assert_eq!(counterfactual[&second].effects(), Some(vec![Effect::Time]));
        assert_eq!(counterfactual[&boundary].effects(), Some(Vec::new()));
        assert_eq!(facts[&boundary].effective, Some(vec![Effect::Time]));
        assert!(!facts[&boundary].redundant_clause);
        assert_eq!(graph.facts(&graph.solve_with_external(&BTreeMap::new())), facts);
        let all_removed = graph.solve_with(&BTreeSet::from([first, second, boundary]), &BTreeMap::new());
        assert!(all_removed.values().all(|summary| summary.effects() == Some(Vec::new())));
    }

    #[test]
    fn joint_clause_counterfactual_captures_error_without_erasing_filesystem_work() {
        let first = declaration(1);
        let second = declaration(2);
        let mut graph = EffectGraph::default();
        graph.declare(first, "capturing".into(), true, Some(vec![Effect::Fs]));
        graph.declare(second, "fallible".into(), true, Some(vec![Effect::Fs, Effect::Error]));
        graph.require(Some(second), Effect::Fs, false);
        graph.require(Some(second), Effect::Error, false);
        graph.require(Some(first), Effect::Error, true);
        graph.call(Some(first), second, true);
        graph.call(Some(second), first, false);
        let counterfactual = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
        assert_eq!(counterfactual[&first].effects(), Some(vec![Effect::Fs]));
        assert_eq!(counterfactual[&second].effects(), Some(vec![Effect::Error, Effect::Fs]));
    }

    #[test]
    fn joint_clause_counterfactual_refuses_finite_proof_for_unknown_dependency() {
        let (mut graph, first, second) = recursive_pair(true);
        graph.unknown(Some(second), "opaque".into());
        let original = graph.solve_with_external(&BTreeMap::new());
        let facts = graph.facts(&original);
        let counterfactual = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
        assert_eq!(counterfactual[&first].effects(), None);
        assert_eq!(counterfactual[&second].effects(), None);
        assert_eq!(counterfactual[&first].unknown_chain, ["second", "opaque"]);
        assert_eq!(counterfactual[&second].unknown_chain, ["opaque"]);
        assert_eq!(graph.facts(&graph.solve_with_external(&BTreeMap::new())), facts);
    }

    #[test]
    fn joint_clause_counterfactual_distinguishes_modules_with_identical_local_spans() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let body = declaration(1).body;
            let first = EffectDeclarationId { namespace: Some(Name::intern("first_module")), body };
            let second = EffectDeclarationId { namespace: Some(Name::intern("second_module")), body };
            let mut graph = EffectGraph::default();
            graph.declare(first, "first".into(), true, Some(vec![Effect::Time]));
            graph.declare(second, "second".into(), true, Some(vec![Effect::Time]));
            graph.call(Some(first), second, false);
            graph.call(Some(second), first, false);
            let singleton = graph.solve_with(&BTreeSet::from([first]), &BTreeMap::new());
            assert_eq!(singleton[&first].effects(), Some(vec![Effect::Time]));
            assert_eq!(singleton[&second].effects(), Some(vec![Effect::Time]));
            let joint = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
            assert_eq!(joint[&first].effects(), Some(Vec::new()));
            assert_eq!(joint[&second].effects(), Some(Vec::new()));
        });
    }

    #[test]
    #[should_panic(expected = "unclaused declarations belong to the effect graph")]
    fn joint_clause_counterfactual_rejects_external_body_selection() {
        let (mut graph, first, _) = recursive_pair(true);
        let remote = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(1), 1, 1) };
        graph.call(Some(first), remote, false);
        let external = BTreeMap::from([(remote, EffectSummary { known: vec![Effect::Time], unknown_chain: Vec::new() })]);
        let local = graph.solve_with(&BTreeSet::from([first]), &external);
        assert_eq!(local[&first].effects(), Some(vec![Effect::Time]));
        graph.solve_with(&BTreeSet::from([first, remote]), &external);
    }

    #[test]
    fn joint_clause_counterfactual_preserves_external_unknown_boundary() {
        let local = declaration(1);
        let remote = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(1), 1, 1) };
        let mut graph = EffectGraph::default();
        graph.declare(local, "local".into(), true, None);
        graph.call(Some(local), remote, false);
        let external = BTreeMap::from([(remote, EffectSummary { known: vec![Effect::Fs], unknown_chain: vec!["remote".into(), "opaque".into()] })]);
        let summaries = graph.solve_with_external(&external);
        assert_eq!(summaries[&local].effects(), None);
        assert_eq!(summaries[&local].known, vec![Effect::Fs]);
        assert_eq!(summaries[&local].unknown_chain, external[&remote].unknown_chain);
    }
}

#[derive(Clone, Debug)]
pub(super) enum DeferredEffectKind {
    Required {
        effect: Effect,
        message: String,
        subject: String,
    },
    Call {
        target: Option<EffectDeclarationId>,
        contract: Option<Vec<Effect>>,
        inferred: bool,
        unknown_chain: Vec<String>,
        name: String,
    },
}

#[derive(Clone, Debug)]
pub(super) struct DeferredEffectCheck {
    position: usize,
    owner: Option<EffectDeclarationId>,
    caller: Option<Vec<Effect>>,
    span: Span,
    kind: DeferredEffectKind,
    notes: Vec<String>,
    excluded: Vec<ExcludedEffect>,
    in_pure: bool,
    retry_attempt_depth: usize,
}

impl Checker {
    pub(super) fn defer_required_effect(
        &mut self,
        effect: Effect,
        span: Span,
        subject: &str,
    ) {
        if !self.defer_effect_diagnostics
            || (self.boundary.retry_attempt_depth > 0 && effect == Effect::Error)
        {
            return;
        }
        self.push_deferred_effect_check(
            span,
            DeferredEffectKind::Required {
                message: format!("{subject} requires the `{}` effect", effect.as_str()),
                effect,
                subject: subject.to_string(),
            },
        );
    }

    pub(super) fn check_required_effect_clause(
        &mut self,
        effect: Effect,
        span: Span,
        message: &str,
        subject: &str,
    ) {
        if self.defer_effect_diagnostics {
            self.push_deferred_effect_check(
                span,
                DeferredEffectKind::Required {
                    effect,
                    message: message.to_string(),
                    subject: subject.to_string(),
                },
            );
        } else if let Some(caller) = &self.boundary.current_effects
            && !Self::effects_covers(caller, &effect)
        {
            self.error(span, message, DiagnosticCode::CheckEffectViolation);
        }
    }

    fn push_deferred_effect_check(&mut self, span: Span, kind: DeferredEffectKind) {
        self.deferred_effect_checks.push(DeferredEffectCheck {
            position: self.diagnostics.len(),
            owner: self.boundary.effect_owner,
            caller: self.boundary.current_effects.clone(),
            span,
            kind,
            notes: Vec::new(),
            excluded: self.boundary.excluded_effects.clone(),
            in_pure: self.boundary.in_pure,
            retry_attempt_depth: self.boundary.retry_attempt_depth,
        });
    }

    pub(super) fn defer_callable_effects(
        &mut self,
        target: Option<EffectDeclarationId>,
        contract: Option<Vec<Effect>>,
        inferred: bool,
        unknown_chain: Vec<String>,
        name: &str,
        span: Span,
    ) {
        if !self.defer_effect_diagnostics {
            return;
        }
        self.push_deferred_effect_check(
            span,
            DeferredEffectKind::Call {
                target,
                contract,
                inferred,
                unknown_chain,
                name: name.to_string(),
            },
        );
    }

    pub(super) fn validate_deferred_effects(&mut self) {
        if self.deferred_effect_checks.is_empty() {
            return;
        }
        let checks = std::mem::take(&mut self.deferred_effect_checks);
        let summaries = self.effect_summaries.clone();
        let mut inserted = 0;
        for check in checks {
            let start = self.diagnostics.len();
            let deferred_span = check.span;
            let notes = check.notes.clone();
            let saved_boundary = self.boundary.clone();
            let saved_collecting = self.collecting_effects;
            let saved_recording = self.record_effects;
            let saved_deferred = self.defer_effect_diagnostics;
            self.boundary.current_effects = self.caller_effect_contract(check.owner, &check.caller, &summaries);
            self.boundary.effect_owner = check.owner;
            self.boundary.excluded_effects = check.excluded;
            self.boundary.in_pure = check.in_pure;
            self.boundary.retry_attempt_depth = check.retry_attempt_depth;
            self.collecting_effects = false;
            self.record_effects = false;
            self.defer_effect_diagnostics = false;
            match check.kind {
                DeferredEffectKind::Required {
                    effect,
                    message,
                    subject,
                } => {
                    self.check_required_effect_clause(
                        effect.clone(),
                        check.span,
                        &message,
                        &subject,
                    );
                    self.check_effect_not_excluded(&effect, check.span, &subject);
                }
                DeferredEffectKind::Call {
                    target,
                    contract,
                    inferred,
                    mut unknown_chain,
                    name,
                } => {
                    let (effects, target_unknown) = self.callee_effect_contract(
                        target,
                        contract,
                        inferred,
                        unknown_chain,
                        &summaries,
                    );
                    unknown_chain = target_unknown;
                    if let Some(caller) = self.boundary.current_effects.clone() {
                        if effects.is_none() && inferred {
                            let mut chain = vec![name.clone()];
                            chain.extend(unknown_chain.clone());
                            self.error(
                                check.span,
                                &format!("proc `{name}` has an unknown effect summary: {}; call a named callable with a checked effect contract instead of an opaque or unrestricted dependency", chain.join(" -> ")),
                                DiagnosticCode::CheckEffectViolation,
                            );
                        } else {
                            self.check_callee_effects(&caller, &effects, &name, check.span);
                        }
                    }
                    self.check_callee_not_excluded(&effects, &unknown_chain, &name, check.span);
                }
            }
            self.boundary = saved_boundary;
            self.collecting_effects = saved_collecting;
            self.record_effects = saved_recording;
            self.defer_effect_diagnostics = saved_deferred;
            let mut generated = self.diagnostics.drain(start..).collect::<Vec<_>>();
            if generated.iter().any(|diagnostic| {
                diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)
                    && diagnostic.labels.iter().any(|label| label.span == deferred_span)
            }) {
                // Deferred errors arrive after call checking has admitted static call facts.
                self.statically_resolved_call_spans.retain(|call| {
                    call.source_id != deferred_span.source_id
                        || call.start() > deferred_span.start()
                        || call.end() < deferred_span.end()
                });
            }
            for diagnostic in &mut generated {
                diagnostic.notes.extend(notes.iter().cloned());
            }
            let insertion = check.position.saturating_add(inserted).min(self.diagnostics.len());
            inserted += generated.len();
            self.diagnostics.splice(insertion..insertion, generated.drain(..));
        }
    }

    pub(super) fn add_deferred_required_effect_note(
        &mut self,
        from: usize,
        span: Span,
        note: String,
    ) {
        if !self.defer_effect_diagnostics {
            return;
        }
        if let Some(check) = self.deferred_effect_checks[from..]
            .iter_mut()
            .rev()
            .find(|check| {
                check.span == span
                    && matches!(
                        &check.kind,
                        DeferredEffectKind::Required {
                            effect: Effect::Error,
                            subject,
                            ..
                        } if subject == "`?`"
                    )
            })
        {
            check.notes.push(note);
        }
    }

    fn caller_effect_contract(
        &self,
        owner: Option<EffectDeclarationId>,
        fallback: &Option<Vec<Effect>>,
        summaries: &BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> Option<Vec<Effect>> {
        owner
            .and_then(|owner| self.effect_graph.nodes.get(&owner).map(|node| (owner, node)))
            .map_or_else(
                || fallback.clone(),
                |(owner, node)| {
                    node.declared.clone().or_else(|| {
                        node.inferred
                            .then(|| summaries.get(&owner).and_then(EffectSummary::effects))
                            .flatten()
                    })
                },
            )
    }

    fn callee_effect_contract(
        &self,
        target: Option<EffectDeclarationId>,
        contract: Option<Vec<Effect>>,
        inferred: bool,
        fallback_unknown_chain: Vec<String>,
        summaries: &BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> (Option<Vec<Effect>>, Vec<String>) {
        let Some((id, node)) = target.and_then(|id| self.effect_graph.nodes.get(&id).map(|node| (id, node))) else {
            return (contract, fallback_unknown_chain);
        };
        if let Some(declared) = &node.declared {
            return (Some(declared.clone()), Vec::new());
        }
        if inferred && let Some(summary) = summaries.get(&id) {
            let mut chain = vec![node.name.clone()];
            chain.extend(summary.unknown_chain.clone());
            return (summary.effects(), chain);
        }
        (contract, fallback_unknown_chain)
    }

    pub(super) fn effect_declaration_id(
        &self,
        program: &ArenaProgram,
        body: BlockId,
    ) -> EffectDeclarationId {
        EffectDeclarationId {
            namespace: self.current_namespace,
            body: program.arena.span(program.arena.block(body).span),
        }
    }

    pub(super) fn effective_function_effects(
        &self,
        program: &ArenaProgram,
        def: &ArenaFunctionDef,
    ) -> Option<Vec<Effect>> {
        if let Some(effects) = def.effects {
            return Some(program.arena.effects(effects).collect());
        }
        let id = self.effect_declaration_id(program, def.body);
        if !self.effect_graph.is_inferred(id) {
            return None;
        }
        // A probe reads the previous probe's solution; the first probe starts
        // from the empty set.
        if self.collecting_effects {
            return self
                .effect_summaries
                .get(&id)
                .map_or_else(|| Some(Vec::new()), EffectSummary::effects);
        }
        self.effect_summaries
            .get(&id)
            .and_then(EffectSummary::effects)
    }

    pub(super) fn record_required_effect(&mut self, effect: Effect) {
        if self.record_effects || self.collecting_effects {
            self.effect_graph
                .require(self.boundary.effect_owner, effect, self.boundary.retry_attempt_depth > 0);
        }
    }

    pub(super) fn require_effect(&mut self, effect: Effect, span: Span, subject: &str) {
        if effect == Effect::Error {
            self.propagating_error_spans.insert(span);
        }
        let captured = self.boundary.retry_attempt_depth > 0;
        if self.record_effects || self.collecting_effects {
            self.effect_graph
                .require(self.boundary.effect_owner, effect.clone(), captured);
        }
        if self.defer_effect_diagnostics {
            self.defer_required_effect(effect, span, subject);
            return;
        }
        if captured && effect == Effect::Error {
            return;
        }
        if let Some(caller) = &self.boundary.current_effects
            && !Self::effects_covers(caller, &effect)
        {
            self.error(
                span,
                &format!("{subject} requires the `{}` effect", effect.as_str()),
                DiagnosticCode::CheckEffectViolation,
            );
        }
        self.check_effect_not_excluded(&effect, span, subject);
    }

    pub(super) fn check_resolved_callable_effects(
        &mut self,
        sig: &FunctionSig,
        name: &str,
        span: Span,
    ) {
        let unknown_chain = sig
            .effect_declaration
            .and_then(|declaration| self.effect_summaries.get(&declaration))
            .map_or_else(Vec::new, |summary| summary.unknown_chain.clone());
        self.check_resolved_callable_effect_contract(
            sig.effect_declaration,
            sig.scheme.effects().map(<[_]>::to_vec),
            sig.scheme.inferred_effects(),
            unknown_chain,
            name,
            span,
        );
    }

    pub(super) fn check_resolved_declaration_effects(
        &mut self,
        declaration: Option<EffectDeclarationId>,
        name: &str,
        span: Span,
    ) {
        let (effects, inferred) = declaration
            .and_then(|id| self.effect_graph.nodes.get(&id).map(|node| (id, node)))
            .map_or((None, false), |(id, node)| {
                let effects = node.declared.clone().or_else(|| {
                    node.inferred
                        .then(|| self.effect_summaries.get(&id).and_then(EffectSummary::effects))
                        .flatten()
                });
                (effects, node.inferred)
            });
        let unknown_chain = declaration
            .and_then(|id| self.effect_summaries.get(&id))
            .map_or_else(Vec::new, |summary| summary.unknown_chain.clone());
        self.check_resolved_callable_effect_contract(
            declaration,
            effects,
            inferred,
            unknown_chain,
            name,
            span,
        );
    }

    pub(super) fn declaration_effects(
        &self,
        declaration: Option<EffectDeclarationId>,
    ) -> Option<Vec<Effect>> {
        let (id, node) = declaration.and_then(|id| self.effect_graph.nodes.get(&id).map(|node| (id, node)))?;
        node.declared.clone().or_else(|| {
            node.inferred
                .then(|| self.effect_summaries.get(&id).and_then(EffectSummary::effects))
                .flatten()
        })
    }

    fn check_resolved_callable_effect_contract(
        &mut self,
        declaration: Option<EffectDeclarationId>,
        effects: Option<Vec<Effect>>,
        inferred: bool,
        unknown_chain: Vec<String>,
        name: &str,
        span: Span,
    ) {
        if self.record_effects || self.collecting_effects {
            if let Some(declaration) = declaration
                && (inferred || effects.is_some())
                && self.effect_graph.contains(declaration)
            {
                self.effect_graph.call(
                    self.boundary.effect_owner,
                    declaration,
                    self.boundary.retry_attempt_depth > 0,
                );
            } else {
                self.record_effect_contract(&effects, name);
            }
        }
        if self.defer_effect_diagnostics {
            self.defer_callable_effects(
                declaration,
                effects.clone(),
                inferred,
                unknown_chain,
                name,
                span,
            );
            return;
        }
        if let Some(caller) = self.boundary.current_effects.clone() {
            if effects.is_none() && inferred {
                let mut chain = vec![name.to_string()];
                chain.extend(unknown_chain.iter().cloned());
                self.error(span, &format!("proc `{name}` has an unknown effect summary: {}; call a named callable with a checked effect contract instead of an opaque or unrestricted dependency", chain.join(" -> ")), DiagnosticCode::CheckEffectViolation);
            } else {
                self.check_callee_effects(&caller, &effects, name, span);
            }
        }
        let unknown_chain = if effects.is_none() && inferred {
            let mut chain = vec![name.to_string()];
            chain.extend(unknown_chain.iter().cloned());
            chain
        } else {
            Vec::new()
        };
        self.check_callee_not_excluded(&effects, &unknown_chain, name, span);
    }

    pub(super) fn record_effect_contract(&mut self, effects: &Option<Vec<Effect>>, name: &str) {
        if !self.record_effects && !self.collecting_effects {
            return;
        }
        match effects {
            Some(effects) => {
                for effect in effects {
                    self.effect_graph.require(
                        self.boundary.effect_owner,
                        effect.clone(),
                        self.boundary.retry_attempt_depth > 0,
                    );
                }
            }
            None => self
                .effect_graph
                .unknown(self.boundary.effect_owner, name.to_string()),
        }
    }

    pub(super) fn check_opaque_callable_effects(&mut self, name: &str, span: Span) {
        self.record_effect_contract(&None, name);
        if self.defer_effect_diagnostics {
            self.defer_callable_effects(None, None, false, Vec::new(), name, span);
            return;
        }
        if let Some(caller) = self.boundary.current_effects.clone() {
            self.check_callee_effects(&caller, &None, name, span);
        }
        self.check_callee_not_excluded(&None, &[], name, span);
    }
}
