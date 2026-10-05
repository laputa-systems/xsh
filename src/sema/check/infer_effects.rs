use super::{BTreeMap, Effect, Name, Span};
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
#[derive(Clone, Debug)]
struct EffectEdge {
    target: EffectDeclarationId,
    captures_error: bool,
    contract: bool,
}

#[derive(Clone, Debug)]
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

    pub fn solve(&self) -> BTreeMap<EffectDeclarationId, EffectSummary> {
        self.solve_with(None)
    }

    /// Effect sets only grow within the finite domain. Unknown witnesses choose
    /// the shortest stable chain, so recursive calls cannot grow diagnostic paths.
    /// `unclaused` solves as if that declaration had no clause: contract edges
    /// to it read its summary instead.
    fn solve_with(
        &self,
        unclaused: Option<EffectDeclarationId>,
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
                            known: self.nodes[&edge.target].declared.clone().unwrap_or_default(),
                            unknown_chain: Vec::new(),
                        }
                    } else {
                        let Some(target) = summaries.get(&edge.target).cloned() else {
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
        self.solve_with(Some(id))[&id]
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
use crate::syntax::arena::{ArenaFunctionDef, ArenaProgram, ArenaStmtKind, BlockId, StmtId};

impl Checker {
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

    pub(super) fn prepare_effect_declarations(
        &mut self,
        program: &ArenaProgram,
        namespace: Option<Name>,
    ) {
        let mut namespaces = super::FxHashMap::default();
        for module in &program.modules {
            for statement in program.arena.stmt_ids(module.statements) {
                namespaces.insert(program.arena.stmt(statement).span.source_id, module.name);
            }
        }
        for raw in 0..program.arena.stmt_tags.len() {
            let statement = program.arena.stmt(StmtId::from_index(raw));
            let definition = match statement.kind {
                ArenaStmtKind::ProcDef(definition)
                | ArenaStmtKind::StreamDef(definition)
                | ArenaStmtKind::CliMain(definition) => definition,
                _ => continue,
            };
            {
                let def = program.arena.function_def(definition);
                let id = EffectDeclarationId {
                    namespace: namespaces
                        .get(&statement.span.source_id)
                        .copied()
                        .or(namespace),
                    body: program.arena.span(program.arena.block(def.body).span),
                };
                let declared = def
                    .effects
                    .map(|effects| program.arena.effects(effects).collect());
                // Exports, streams, and entry points infer like private procs:
                // an importer reads the inferred set, which depends only on the
                // body and its callees. A native test has no callers, so
                // without a clause it stays unrestricted.
                let inference_allowed = !def.test_declaration;
                let name = id
                    .namespace
                    .map(|namespace| format!("{namespace}.{}", def.name))
                    .unwrap_or_else(|| def.name.to_string());
                self.effect_graph
                    .declare(id, name, inference_allowed, declared);
            }
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
        if self.collecting_effects {
            self.effect_graph
                .require(self.effect_owner, effect, self.retry_attempt_depth > 0);
        }
    }

    pub(super) fn require_effect(&mut self, effect: Effect, span: Span, subject: &str) {
        let captured = self.retry_attempt_depth > 0;
        if self.collecting_effects {
            self.effect_graph
                .require(self.effect_owner, effect.clone(), captured);
        }
        if captured && effect == Effect::Error {
            return;
        }
        if let Some(caller) = &self.current_effects
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
        if self.collecting_effects {
            if let Some(declaration) = sig.effect_declaration
                && (sig.inferred_effects || sig.effects.is_some())
            {
                self.effect_graph.call(
                    self.effect_owner,
                    declaration,
                    self.retry_attempt_depth > 0,
                );
            } else {
                self.record_effect_contract(&sig.effects, name);
            }
        }
        if let Some(caller) = self.current_effects.clone() {
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
        if !self.collecting_effects {
            return;
        }
        match effects {
            Some(effects) => {
                for effect in effects {
                    self.effect_graph.require(
                        self.effect_owner,
                        effect.clone(),
                        self.retry_attempt_depth > 0,
                    );
                }
            }
            None => self
                .effect_graph
                .unknown(self.effect_owner, name.to_string()),
        }
    }

    pub(super) fn check_opaque_callable_effects(&mut self, name: &str, span: Span) {
        self.record_effect_contract(&None, name);
        if let Some(caller) = self.current_effects.clone() {
            self.check_callee_effects(&caller, &None, name, span);
        }
        self.check_callee_not_excluded(&None, &[], name, span);
    }
}
