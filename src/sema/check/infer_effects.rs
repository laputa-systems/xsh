use super::{BTreeMap, CheckBundleView, Effect, Name, Span};
use crate::diagnostic::DiagnosticCode;

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
        self.solve_with(None, external)
    }

    /// Effect sets only grow within the finite domain. Unknown witnesses choose
    /// the shortest stable chain, so recursive calls cannot grow diagnostic paths.
    /// `unclaused` solves as if that declaration had no clause: contract edges
    /// to it read its summary instead.
    fn solve_with(
        &self,
        unclaused: Option<EffectDeclarationId>,
        external: &BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> BTreeMap<EffectDeclarationId, EffectSummary> {
        let mut summaries = self
            .nodes
            .iter()
            .map(|(&id, node)| (id, node.direct.clone()))
            .collect::<BTreeMap<_, _>>();
        loop {
            let mut changed = false;
            for (&id, node) in &self.nodes {
                for edge in &node.edges {
                    let target = if edge.contract && unclaused != Some(edge.target) {
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
                        let mut chain = vec![self.nodes[&edge.target].name.clone()];
                        chain.extend(target.unknown_chain);
                        changed |= summary.unknown(chain);
                    }
                }
            }
            if !changed {
                return summaries;
            }
        }
    }

    /// A clause is redundant when it equals, as a set, what its declaration
    /// infers once every use of the clause, including recursive ones, reads the
    /// inferred summary instead.
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
        // Without a path back to the declaration, its clause feeds nothing it
        // depends on, so the solved summary already is the inferred set.
        if !self.reaches_own_contract(id) {
            return true;
        }
        self.solve_with(Some(id), &BTreeMap::new())[&id]
            .effects()
            .is_some_and(|known| same(&known))
    }

    /// Whether a call chain from `id` uses `id`'s own clause. Contract edges to
    /// other declarations end a chain because they read a fixed clause.
    fn reaches_own_contract(&self, id: EffectDeclarationId) -> bool {
        let mut pending = vec![id];
        let mut seen = std::collections::BTreeSet::new();
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
        if self.record_effects || self.collecting_effects {
            if let Some(declaration) = sig.effect_declaration
                && (sig.inferred_effects || sig.effects.is_some())
                && self.effect_graph.contains(declaration)
            {
                self.effect_graph.call(
                    self.boundary.effect_owner,
                    declaration,
                    self.boundary.retry_attempt_depth > 0,
                );
            } else {
                self.record_effect_contract(&sig.effects, name);
            }
        }
        if self.defer_effect_diagnostics {
            let unknown_chain = sig
                .effect_declaration
                .and_then(|declaration| self.effect_summaries.get(&declaration))
                .map_or_else(Vec::new, |summary| summary.unknown_chain.clone());
            self.defer_callable_effects(
                sig.effect_declaration,
                sig.effects.clone(),
                sig.inferred_effects,
                unknown_chain,
                name,
                span,
            );
            return;
        }
        if let Some(caller) = self.boundary.current_effects.clone() {
            if sig.effects.is_none() && sig.inferred_effects {
                let mut chain = vec![name.to_string()];
                if let Some(summary) = sig
                    .effect_declaration
                    .and_then(|declaration| self.effect_summaries.get(&declaration))
                {
                    chain.extend(summary.unknown_chain.clone());
                }
                self.error(span, &format!("proc `{name}` has an unknown effect summary: {}; call a named callable with a checked effect contract instead of an opaque or unrestricted dependency", chain.join(" -> ")), DiagnosticCode::CheckEffectViolation);
            } else {
                self.check_callee_effects(&caller, &sig.effects, name, span);
            }
        }
        let unknown_chain = if sig.effects.is_none() && sig.inferred_effects {
            let mut chain = vec![name.to_string()];
            if let Some(summary) = sig
                .effect_declaration
                .and_then(|declaration| self.effect_summaries.get(&declaration))
            {
                chain.extend(summary.unknown_chain.clone());
            }
            chain
        } else {
            Vec::new()
        };
        self.check_callee_not_excluded(&sig.effects, &unknown_chain, name, span);
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
