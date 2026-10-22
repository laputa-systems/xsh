use super::{BTreeMap, CheckBundleView, Effect, Name, Span, Type};
use crate::diagnostic::DiagnosticCode;
use std::collections::BTreeSet;
use crate::sema::effects::{CheckedEffectScope, CheckedEffects, EffectContract, EffectExpr, EffectSet};
use crate::sema::schemes::{CallInstantiation, SchemeEffectParameterId};
use crate::sema::constraints::{EffectVariableId, SchemeOperationError};
use crate::sema::constraints::ConstraintCounters;
use crate::sema::inference_work;
use std::sync::Arc;

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
    pub effective: CheckedEffects,
    pub required: CheckedEffects,
    pub scope: CheckedEffectScope,
    pub inferred: bool,
    pub inference_allowed: bool,
    /// The clause names exactly the set its declaration would infer without
    /// it, so deleting the clause changes no caller-visible contract.
    pub redundant_clause: bool,
    pub unknown_chain: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EffectSummary<V = SchemeEffectParameterId> {
    pub known: EffectExpr<V>,
    pub unknown_chain: Vec<String>,
    scope: CheckedEffectScope,
}

impl<V: Ord> Default for EffectSummary<V> {
    fn default() -> Self {
        Self { known: EffectExpr::finite(EffectSet::default()), unknown_chain: Vec::new(), scope: CheckedEffectScope::default() }
    }
}

impl<V: Clone + Ord> EffectSummary<V> {
    fn add(&mut self, effect: Effect) -> bool {
        self.join_expression(EffectExpr::finite(EffectSet::singleton(effect)))
    }

    fn join_expression(&mut self, expression: EffectExpr<V>) -> bool {
        let joined = self.known.clone().union(expression);
        if self.known == joined { return false; }
        self.known = joined;
        true
    }

    fn specialize_contributions<W: Clone + Ord, E>(
        &self,
        scope: CheckedEffectScope,
        unknown_name: &str,
        mut map: impl FnMut(&V) -> Result<EffectContract<W>, E>,
    ) -> Result<EffectSummary<W>, E> {
        let mut summary = EffectSummary { known: EffectExpr::finite(self.known.direct()), unknown_chain: self.unknown_chain.clone(), scope };
        for (id, mask) in self.known.variables() {
            match map(id)?.masked(mask) {
                EffectContract::Known(expression) => { summary.join_expression(expression); }
                EffectContract::Unknown => { summary.unknown(vec![unknown_name.to_owned()]); }
            }
        }
        Ok(summary)
    }

    fn unknown(&mut self, chain: Vec<String>) -> bool {
        if chain.is_empty() { return false; }
        if self.unknown_chain.is_empty() || (chain.len(), &chain) < (self.unknown_chain.len(), &self.unknown_chain) {
            self.unknown_chain = chain;
            return true;
        }
        false
    }
}

impl EffectSummary {
    fn validate_owner(&self, owner: EffectDeclarationId) -> Result<(), SchemeOperationError> {
        if self.known.variables().next().is_some() && self.scope.owner() != Some(owner)
            || !self.known.variables().all(|(id, _)| self.scope.parameters().contains(id))
        {
            return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter));
        }
        Ok(())
    }

    pub(super) fn from_effect_fact(owner: EffectDeclarationId, fact: &FunctionEffectFact) -> Result<Self, SchemeOperationError> {
        let mut unknown_chain = fact.unknown_chain.clone();
        let known = match fact.required.contract() {
            EffectContract::Known(expression) => expression.clone(),
            EffectContract::Unknown => {
                if unknown_chain.is_empty() { unknown_chain.push("unknown effect contract".to_owned()); }
                EffectExpr::finite(EffectSet::default())
            }
        };
        if known.variables().next().is_some() && fact.scope.owner() != Some(owner) {
            return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter));
        }
        if !known.variables().all(|(id, _)| fact.scope.parameters().contains(id)) {
            return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter));
        }
        Ok(Self { known, unknown_chain, scope: fact.scope.clone() })
    }

    pub fn effects(&self) -> CheckedEffects {
        CheckedEffects::from_contract(if self.unknown_chain.is_empty() {
            EffectContract::Known(self.known.clone())
        } else { EffectContract::Unknown })
    }
}

#[derive(Clone, Debug)]
pub(super) struct EffectScopeFailure {
    pub(super) owner: EffectDeclarationId,
    pub(super) cause: SchemeOperationError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EffectContributions {
    Checking(EffectSummary<EffectVariableId>),
    Checked(EffectSummary),
}

impl Default for EffectContributions {
    fn default() -> Self { Self::Checked(EffectSummary::default()) }
}

impl EffectContributions {
    fn checked(&self) -> Result<&EffectSummary, SchemeOperationError> {
        match self {
            Self::Checked(summary) => Ok(summary),
            Self::Checking(_) => Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects)),
        }
    }

    fn add(&mut self, effect: Effect) {
        match self { Self::Checking(summary) => { summary.add(effect); } Self::Checked(summary) => { summary.add(effect); } }
    }

    fn unknown(&mut self, chain: Vec<String>) {
        match self { Self::Checking(summary) => { summary.unknown(chain); } Self::Checked(summary) => { summary.unknown(chain); } }
    }

    fn require(&mut self, expression: &EffectExpr<EffectVariableId>) -> Result<(), SchemeOperationError> {
        if matches!(self, Self::Checked(_)) {
            let Self::Checked(summary) = self else { unreachable!() };
            let known = summary.known.substitute(|_| Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter)))?;
            let EffectContract::Known(known) = known else { unreachable!() };
            *self = Self::Checking(EffectSummary { known, unknown_chain: summary.unknown_chain.clone(), scope: CheckedEffectScope::default() });
        }
        let Self::Checking(summary) = self else { unreachable!() };
        summary.join_expression(expression.clone());
        Ok(())
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
    site: Option<Span>,
    instantiation: Option<Arc<CallInstantiation>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EffectNode {
    pub name: String,
    pub inferred: bool,
    pub inference_allowed: bool,
    pub declared: Option<Vec<Effect>>,
    direct: EffectContributions,
    defaults: Vec<Span>,
    edges: Vec<EffectEdge>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct EffectGraph {
    nodes: BTreeMap<EffectDeclarationId, EffectNode>,
    work: Option<Arc<ConstraintCounters>>,
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
        let mut graph = Self {
            work: inference_work::current_counters(),
            ..Self::default()
        };
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
            graph.nodes.get_mut(&id).expect("declared effect owner").defaults = program.arena.params(def.params)
                .iter().filter_map(|parameter| parameter.default)
                .map(|expression| program.arena.expr(expression).span).collect();
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
            direct: EffectContributions::default(),
            defaults: Vec::new(),
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
        site: Option<Span>,
    ) {
        let contract = self
            .nodes
            .get(&target)
            .is_some_and(|node| !node.inferred && node.declared.is_some());
        let work = self.work.as_deref();
        if let Some(node) = owner.and_then(|owner| self.nodes.get_mut(&owner))
            && !node.edges.iter().any(|edge| {
                if let Some(work) = &work {
                    work.record_effect_edge_visit();
                }
                edge.target == target
                    && edge.captures_error == captures_error
                    && edge.contract == contract
                    && edge.site == site
            })
        {
            node.edges.push(EffectEdge {
                target,
                captures_error,
                contract,
                site,
                instantiation: None,
            });
        }
    }

    fn require_contract(&mut self, owner: Option<EffectDeclarationId>, contract: &EffectContract<EffectVariableId>, captures_error: bool, name: &str) -> Result<(), SchemeOperationError> {
        let contract = if captures_error { contract.clone().capture_error() } else { contract.clone() };
        match contract {
            EffectContract::Unknown => self.unknown(owner, name.to_owned()),
            EffectContract::Known(expression) => {
                if let Some(node) = owner.and_then(|owner| self.nodes.get_mut(&owner)) { node.direct.require(&expression)?; }
            }
        }
        Ok(())
    }

    fn bind_scopes(
        &mut self,
        mut bind: impl FnMut(EffectDeclarationId, &EffectContract<EffectVariableId>) -> Result<EffectContract<SchemeEffectParameterId>, SchemeOperationError>,
        mut parameters: impl FnMut(EffectDeclarationId) -> Arc<[SchemeEffectParameterId]>,
        calls: &BTreeMap<Span, CallInstantiation>,
    ) -> Result<(), EffectScopeFailure> {
        let mut nodes = BTreeMap::new();
        for (&owner, node) in &self.nodes {
            let node = (|| -> Result<EffectNode, SchemeOperationError> {
            let mut node = node.clone();
            let summary = match &node.direct {
                EffectContributions::Checking(source) => {
                    source.specialize_contributions(CheckedEffectScope::new(owner, parameters(owner)), &node.name, |id| bind(owner, &EffectContract::variable(*id)))?
                }
                EffectContributions::Checked(summary) => {
                    let mut summary = summary.clone();
                    summary.scope = CheckedEffectScope::new(owner, parameters(owner));
                    summary
                }
            };
            let scope = summary.scope.parameters().iter().copied().collect::<BTreeSet<_>>();
            if !summary.known.variables().all(|(id, _)| scope.contains(id)) {
                return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter));
            }
            for edge in &mut node.edges {
                if let Some(work) = &self.work {
                    work.record_effect_edge_visit();
                }
                if let Some(site) = edge.site {
                    if !std::iter::once(&owner.body).chain(&node.defaults).any(|region| {
                        site.source_id == region.source_id && site.start() >= region.start() && site.end() <= region.end()
                    }) {
                        return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter));
                    }
                    if let Some(instance) = calls.get(&site) { edge.instantiation = Some(Arc::new(instance.clone())); }
                }
            }
            node.direct = EffectContributions::Checked(summary);
            Ok(node)
            })().map_err(|cause| EffectScopeFailure { owner, cause })?;
            nodes.insert(owner, node);
        }
        self.nodes = nodes;
        Ok(())
    }

    pub(super) fn solve_with_external(
        &self,
        external: &BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> Result<BTreeMap<EffectDeclarationId, EffectSummary>, EffectScopeFailure> {
        self.try_solve_with(&BTreeSet::new(), external)
    }

    #[cfg(test)]
    fn solve_with(&self, unclaused: &BTreeSet<EffectDeclarationId>, external: &BTreeMap<EffectDeclarationId, EffectSummary>) -> BTreeMap<EffectDeclarationId, EffectSummary> {
        self.try_solve_with(unclaused, external).expect("finite graph fixtures carry complete effect evidence")
    }

    fn try_solve_with(
        &self,
        unclaused: &BTreeSet<EffectDeclarationId>,
        external: &BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> Result<BTreeMap<EffectDeclarationId, EffectSummary>, EffectScopeFailure> {
        assert!(unclaused.iter().all(|id| self.nodes.contains_key(id)), "unclaused declarations belong to the effect graph");
        let mut summaries = self
            .nodes
            .iter()
            .map(|(&id, node)| {
                let summary = node.direct.checked().and_then(|summary| { summary.validate_owner(id)?; Ok((id, summary.clone())) });
                summary.map_err(|cause| EffectScopeFailure { owner: id, cause })
            })
            .collect::<Result<BTreeMap<_, _>, EffectScopeFailure>>()?;
        loop {
            let mut changed = false;
            for (&id, node) in &self.nodes {
                for edge in &node.edges {
                    if let Some(work) = &self.work {
                        work.record_effect_edge_visit();
                    }
                    let target = if edge.contract && !unclaused.contains(&edge.target) {
                        EffectSummary {
                            known: EffectExpr::finite(EffectSet::new(self.nodes[&edge.target].declared.clone().unwrap_or_default())),
                            unknown_chain: Vec::new(),
                            scope: CheckedEffectScope::default(),
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
                    target.validate_owner(edge.target).map_err(|cause| EffectScopeFailure { owner: id, cause })?;
                    let summary = summaries
                        .get_mut(&id)
                        .expect("every declaration has an initial summary");
                    let target = target.specialize_contributions(summary.scope.clone(), &node.name, |parameter| {
                        if let Some(instance) = &edge.instantiation {
                            instance.effect_arguments().get(parameter).cloned()
                        } else if edge.target == id {
                            Some(EffectContract::variable(*parameter))
                        } else { None }.ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter))
                    }).map_err(|cause| EffectScopeFailure { owner: id, cause })?;
                    let target_known = if edge.captures_error {
                        target.known.masked(EffectSet::all().without(EffectSet::singleton(Effect::Error)))
                    } else { target.known };
                    if !target_known.variables().all(|(id, _)| summary.scope.parameters().contains(id)) {
                        return Err(EffectScopeFailure { owner: id, cause: SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter) });
                    }
                    changed |= summary.join_expression(target_known);
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
                return Ok(summaries);
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
        if !node.inference_allowed || !summary.effects().concrete().is_ok_and(|known| known.is_some_and(|known| same(&known))) {
            return false;
        }
        if !self.reaches_own_contract(id) {
            return true;
        }
        self.try_solve_with(&BTreeSet::from([id]), &BTreeMap::new()).is_ok_and(|summaries| {
            summaries[&id].effects().concrete().is_ok_and(|known| known.is_some_and(|known| same(&known)))
        })
    }

    fn reaches_own_contract(&self, id: EffectDeclarationId) -> bool {
        let mut pending = vec![id];
        let mut seen = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current) {
                continue;
            }
            for edge in self.nodes.get(&current).map_or(&[][..], |node| &node.edges) {
                if let Some(work) = &self.work {
                    work.record_effect_edge_visit();
                }
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
                            CheckedEffects::from_clause(node.declared.clone())
                        },
                        required: summary.effects(),
                        scope: summary.scope.clone(),
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

use super::Checker;
use super::effect_bounds::ExcludedEffect;
use crate::syntax::arena::{ArenaFunctionDef, ArenaProgram, ArenaStmtKind, BlockId};

impl Checker {
    pub(super) fn bind_effect_graph(&mut self) -> Result<(), EffectScopeFailure> {
        let constraints = &self.type_constraints;
        let declarations = &self.declaration_inference;
        self.effect_graph.bind_scopes(
            |owner, contract| {
                let scope = declarations.pending(super::FunctionDeclarationId::new(owner)).and_then(|declaration| declaration.generalized_scope());
                match scope {
                    Some(scope) => constraints.resolve_generalized_effect(contract, scope),
                    None => constraints.resolve_effect(contract)?.substitute(|_| Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects))),
                }
            },
            |owner| declarations.pending(super::FunctionDeclarationId::new(owner))
                .and_then(|declaration| declaration.generalized())
                .map_or_else(|| Arc::from([]), |callable| Arc::from(callable.requirements().effect_parameters.as_slice())),
            &self.call_instantiations,
        )
    }
}


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
        graph.call(Some(first), second, false, None);
        graph.call(Some(second), first, false, None);
        (graph, first, second)
    }

    fn scoped_instantiation(parameter: SchemeEffectParameterId, argument: EffectContract<SchemeEffectParameterId>, caller: &[SchemeEffectParameterId]) -> CallInstantiation {
        let callable = crate::sema::schemes::CallableScheme::new(
            Vec::new(), Vec::new(), Vec::new(), crate::sema::schemes::SchemeType::Ground(crate::sema::types::Type::Int), None, false,
            crate::sema::schemes::CallableRequirements {
                effect_parameters: vec![parameter],
                effect_predicates: vec![crate::sema::effects::EffectPredicate::Equal(EffectContract::variable(parameter), EffectContract::variable(parameter))],
                applications: Vec::new(),
            },
        ).unwrap();
        CallInstantiation::new(&callable, BTreeMap::new(), BTreeMap::new(), BTreeMap::from([(parameter, argument)]), Vec::new(), &[], &[], caller, &[]).unwrap()
    }

    fn symbolic_owner(graph: &mut EffectGraph, owner: EffectDeclarationId, parameter: SchemeEffectParameterId) {
        graph.declare(owner, "symbolic".to_owned(), true, None);
        graph.nodes.get_mut(&owner).unwrap().direct = EffectContributions::Checked(EffectSummary {
            known: EffectExpr::variable(parameter), unknown_chain: Vec::new(),
            scope: CheckedEffectScope::new(owner, Arc::from([parameter])),
        });
    }

    #[test]
    fn symbolic_effect_graph_siblings_specialize_their_own_bound_ordinals() {
        let parameter = SchemeEffectParameterId(0);
        for reverse in [false, true] {
            let (root, first, second) = (declaration(1), declaration(2), declaration(3));
            let mut graph = EffectGraph::default();
            graph.declare(root, "root".to_owned(), true, None);
            symbolic_owner(&mut graph, first, parameter);
            symbolic_owner(&mut graph, second, parameter);
            let targets = if reverse { [(second, Effect::Time), (first, Effect::Fs)] } else { [(first, Effect::Fs), (second, Effect::Time)] };
            for (target, effect) in targets {
                graph.call(Some(root), target, false, None);
                graph.nodes.get_mut(&root).unwrap().edges.last_mut().unwrap().instantiation = Some(Arc::new(scoped_instantiation(parameter, EffectContract::finite(EffectSet::singleton(effect)), &[])));
            }
            let summaries = graph.solve_with_external(&BTreeMap::new()).unwrap();
            assert_eq!(summaries[&root].effects(), CheckedEffects::finite([Effect::Fs, Effect::Time]));
            graph.nodes.get_mut(&root).unwrap().edges[0].instantiation = None;
            assert!(matches!(graph.solve_with_external(&BTreeMap::new()), Err(EffectScopeFailure { owner, .. }) if owner == root));
        }
    }

    #[test]
    fn symbolic_effect_graph_mutual_edges_apply_actual_binder_renaming() {
        let (first, second) = (declaration(1), declaration(2));
        let (left, right) = (SchemeEffectParameterId(0), SchemeEffectParameterId(1));
        let mut graph = EffectGraph::default();
        symbolic_owner(&mut graph, first, left);
        symbolic_owner(&mut graph, second, right);
        for (owner, target, source, destination) in [(first, second, right, left), (second, first, left, right)] {
            graph.call(Some(owner), target, false, None);
            graph.nodes.get_mut(&owner).unwrap().edges[0].instantiation = Some(Arc::new(scoped_instantiation(source, EffectContract::variable(destination), &[destination])));
        }
        let summaries = graph.solve_with_external(&BTreeMap::new()).unwrap();
        assert_eq!(summaries[&first].known, EffectExpr::variable(left));
        assert_eq!(summaries[&second].known, EffectExpr::variable(right));
        graph.nodes.get_mut(&first).unwrap().edges[0].instantiation = Some(Arc::new(scoped_instantiation(right, EffectContract::variable(right), &[right])));
        assert!(matches!(graph.solve_with_external(&BTreeMap::new()), Err(EffectScopeFailure { owner, .. }) if owner == first));
    }

    #[test]
    fn joint_clause_counterfactual_drops_mutually_supported_effects() {
        let (graph, first, second) = recursive_pair(true);
        let original = graph.solve_with_external(&BTreeMap::new()).unwrap();
        let facts = graph.facts(&original);
        assert!(facts[&first].redundant_clause);
        assert!(facts[&second].redundant_clause);
        let (inferred, _, _) = recursive_pair(false);
        let selected = BTreeSet::from([first, second]);
        let counterfactual = graph.solve_with(&selected, &BTreeMap::new());
        assert_eq!(counterfactual, inferred.solve_with_external(&BTreeMap::new()).unwrap());
        assert!(selected.iter().all(|id| counterfactual[id].effects().concrete().unwrap() == Some(Vec::new())));
        assert_eq!(graph.facts(&original), facts);
        assert_eq!(graph.facts(&graph.solve_with_external(&BTreeMap::new()).unwrap()), facts);
    }

    #[test]
    fn joint_clause_counterfactual_preserves_empty_and_singleton_contracts() {
        let (mut graph, first, second) = recursive_pair(true);
        let recursive = declaration(3);
        graph.declare(recursive, "recursive".into(), true, Some(vec![Effect::Time]));
        graph.call(Some(recursive), recursive, false, None);
        let ordinary = graph.solve_with(&BTreeSet::new(), &BTreeMap::new());
        for id in [first, second, recursive] { assert_eq!(ordinary[&id].effects().concrete().unwrap(), Some(vec![Effect::Time])); }
        let singleton = graph.solve_with(&BTreeSet::from([first]), &BTreeMap::new());
        assert_eq!(singleton[&first].effects().concrete().unwrap(), Some(vec![Effect::Time]));
        assert_eq!(singleton[&second].effects().concrete().unwrap(), Some(vec![Effect::Time]));
        assert!(!graph.facts(&ordinary)[&recursive].redundant_clause);
        let self_removed = graph.solve_with(&BTreeSet::from([recursive]), &BTreeMap::new());
        assert_eq!(self_removed[&recursive].effects().concrete().unwrap(), Some(Vec::new()));
        assert_eq!(self_removed[&first].effects().concrete().unwrap(), Some(vec![Effect::Time]));
    }

    #[test]
    fn joint_clause_counterfactual_retains_recursive_clock_requirement() {
        let (mut graph, first, second) = recursive_pair(true);
        graph.require(Some(second), Effect::Time, false);
        let original = graph.solve_with_external(&BTreeMap::new()).unwrap();
        let counterfactual = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
        assert_eq!(counterfactual, original);
        assert_eq!(counterfactual[&first].effects().concrete().unwrap(), Some(vec![Effect::Time]));
        assert_eq!(counterfactual[&second].effects().concrete().unwrap(), Some(vec![Effect::Time]));
        assert!(graph.facts(&original).values().all(|fact| fact.redundant_clause));
    }

    #[test]
    fn joint_clause_counterfactual_retains_unselected_authored_bound() {
        let (mut graph, first, second) = recursive_pair(true);
        let boundary = declaration(3);
        graph.declare(boundary, "boundary".into(), true, Some(vec![Effect::Time]));
        graph.call(Some(second), boundary, false, None);
        let original = graph.solve_with_external(&BTreeMap::new()).unwrap();
        let facts = graph.facts(&original);
        let counterfactual = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
        assert_eq!(counterfactual[&first].effects().concrete().unwrap(), Some(vec![Effect::Time]));
        assert_eq!(counterfactual[&second].effects().concrete().unwrap(), Some(vec![Effect::Time]));
        assert_eq!(counterfactual[&boundary].effects().concrete().unwrap(), Some(Vec::new()));
        assert_eq!(facts[&boundary].effective.concrete().unwrap(), Some(vec![Effect::Time]));
        assert!(!facts[&boundary].redundant_clause);
        assert_eq!(graph.facts(&graph.solve_with_external(&BTreeMap::new()).unwrap()), facts);
        let all_removed = graph.solve_with(&BTreeSet::from([first, second, boundary]), &BTreeMap::new());
        assert!(all_removed.values().all(|summary| summary.effects().concrete().unwrap() == Some(Vec::new())));
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
        graph.call(Some(first), second, true, None);
        graph.call(Some(second), first, false, None);
        let counterfactual = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
        assert_eq!(counterfactual[&first].effects().concrete().unwrap(), Some(vec![Effect::Fs]));
        assert_eq!(counterfactual[&second].effects().concrete().unwrap(), Some(vec![Effect::Fs, Effect::Error]));
    }

    #[test]
    fn joint_clause_counterfactual_refuses_finite_proof_for_unknown_dependency() {
        let (mut graph, first, second) = recursive_pair(true);
        graph.unknown(Some(second), "opaque".into());
        let original = graph.solve_with_external(&BTreeMap::new()).unwrap();
        let facts = graph.facts(&original);
        let counterfactual = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
        assert_eq!(counterfactual[&first].effects().concrete().unwrap(), None);
        assert_eq!(counterfactual[&second].effects().concrete().unwrap(), None);
        assert_eq!(counterfactual[&first].unknown_chain, ["second", "opaque"]);
        assert_eq!(counterfactual[&second].unknown_chain, ["opaque"]);
        assert_eq!(graph.facts(&graph.solve_with_external(&BTreeMap::new()).unwrap()), facts);
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
            graph.call(Some(first), second, false, None);
            graph.call(Some(second), first, false, None);
            let singleton = graph.solve_with(&BTreeSet::from([first]), &BTreeMap::new());
            assert_eq!(singleton[&first].effects().concrete().unwrap(), Some(vec![Effect::Time]));
            assert_eq!(singleton[&second].effects().concrete().unwrap(), Some(vec![Effect::Time]));
            let joint = graph.solve_with(&BTreeSet::from([first, second]), &BTreeMap::new());
            assert_eq!(joint[&first].effects().concrete().unwrap(), Some(Vec::new()));
            assert_eq!(joint[&second].effects().concrete().unwrap(), Some(Vec::new()));
        });
    }

    #[test]
    #[should_panic(expected = "unclaused declarations belong to the effect graph")]
    fn joint_clause_counterfactual_rejects_external_body_selection() {
        let (mut graph, first, _) = recursive_pair(true);
        let remote = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(1), 1, 1) };
        graph.call(Some(first), remote, false, None);
        let external = BTreeMap::from([(remote, EffectSummary { known: EffectExpr::finite(EffectSet::singleton(Effect::Time)), unknown_chain: Vec::new(), scope: CheckedEffectScope::default() })]);
        let local = graph.solve_with(&BTreeSet::from([first]), &external);
        assert_eq!(local[&first].effects().concrete().unwrap(), Some(vec![Effect::Time]));
        graph.solve_with(&BTreeSet::from([first, remote]), &external);
    }

    #[test]
    fn joint_clause_counterfactual_preserves_external_unknown_boundary() {
        let local = declaration(1);
        let remote = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(1), 1, 1) };
        let mut graph = EffectGraph::default();
        graph.declare(local, "local".into(), true, None);
        graph.call(Some(local), remote, false, None);
        let external = BTreeMap::from([(remote, EffectSummary { known: EffectExpr::finite(EffectSet::singleton(Effect::Fs)), unknown_chain: vec!["remote".into(), "opaque".into()], scope: CheckedEffectScope::default() })]);
        let summaries = graph.solve_with_external(&external).unwrap();
        assert_eq!(summaries[&local].effects().concrete().unwrap(), None);
        assert_eq!(summaries[&local].known, EffectExpr::finite(EffectSet::singleton(Effect::Fs)));
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
    Invocation {
        effects: super::InvocationEffects,
        instance: Option<super::PendingCallInstantiation>,
        stream_item: Option<Type>,
        unknown_chain: Vec<String>,
        name: String,
    },
}

#[derive(Clone, Debug)]
struct DeferredEffectContext {
    position: usize,
    owner: Option<EffectDeclarationId>,
    caller: Option<Vec<Effect>>,
    excluded: Vec<ExcludedEffect>,
    in_pure: bool,
    retry_attempt_depth: usize,
}

#[derive(Clone, Debug)]
pub(super) struct DeferredEffectCheck {
    context: DeferredEffectContext,
    span: Span,
    kind: DeferredEffectKind,
    notes: Vec<String>,
}

#[derive(Clone)]
pub(super) struct HostMethodContext {
    effect: DeferredEffectContext,
    recording: bool,
    collecting: bool,
    deferred: bool,
}

impl Checker {
    pub(super) fn defer_recursive_stream_error_flow(&mut self, effects: &super::InvocationEffects, return_ty: &Type, span: Span) -> bool {
        let super::InvocationEffects::Pending(target) = effects else { return false; };
        let Type::Stream(item) = return_ty.unvalidated() else { return false; };
        if !self.defer_effect_diagnostics { return false; }
        let Some(&(caller, callee)) = self.provisional_function_calls.get(&span) else { return false; };
        if Some(caller) != self.stream_origin_owner || callee.effect_declaration() != *target { return false; }
        let Some(check) = self.deferred_effect_checks.iter_mut().rev().find(|check| {
            check.span == span && check.context.owner == Some(caller)
                && matches!(&check.kind, DeferredEffectKind::Invocation { effects: super::InvocationEffects::Pending(owner), .. } if *owner == *target)
        }) else { return false; };
        let DeferredEffectKind::Invocation { stream_item, .. } = &mut check.kind else { unreachable!() };
        assert!(stream_item.replace(item.as_ref().clone()).is_none(), "a recursive stream call records error flow once");
        true
    }

    pub(super) fn prepare_recursive_stream_error_flows(
        &self,
        members: &BTreeMap<EffectDeclarationId, crate::syntax::arena::FunctionDefId>,
        instances: &BTreeMap<Span, super::PendingCallInstantiation>,
    ) -> Result<BTreeSet<Span>, (Span, SchemeOperationError)> {
        let mut latent = BTreeSet::new();
        if self.collecting_effects { return Ok(latent); }
        for check in &self.deferred_effect_checks {
            if !check.context.owner.is_some_and(|owner| members.contains_key(&owner)) { continue; }
            let DeferredEffectKind::Invocation { effects: super::InvocationEffects::Pending(target), stream_item: Some(item), .. } = &check.kind else { continue; };
            if !members.contains_key(target) { continue; }
            let invalid = || (check.span, SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects));
            let &(caller, callee) = self.provisional_function_calls.get(&check.span).ok_or_else(invalid)?;
            if check.context.owner != Some(caller) || callee.effect_declaration() != *target { return Err(invalid()); }
            let instance = instances.get(&check.span).ok_or_else(invalid)?;
            let effects = self.resolve_invocation_effects(super::InvocationEffects::Pending(*target), Some(instance)).map_err(|error| (check.span, error))?;
            let flow = self.invocation_error_flow(&effects, &Type::Stream(Box::new(item.clone()))).map_err(|error| (check.span, error.into()))?;
            if matches!(flow, crate::sema::types::CalleeErrorFlow::Latent) { latent.insert(check.span); }
        }
        Ok(latent)
    }

    pub(super) fn publish_recursive_invocation_instances(
        &mut self,
        instances: &BTreeMap<Span, super::PendingCallInstantiation>,
    ) {
        for check in &mut self.deferred_effect_checks {
            if let Some(instantiation) = instances.get(&check.span)
                && let DeferredEffectKind::Invocation { effects: super::InvocationEffects::Pending(owner), instance, .. } = &mut check.kind
            {
                let (_, callee) = self.provisional_function_calls.get(&check.span)
                    .expect("a recursive effect check retains its selected call");
                assert_eq!(*owner, callee.effect_declaration());
                assert!(instance.is_none());
                *instance = Some(instantiation.clone());
            }
        }
    }

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

    fn deferred_effect_context(&self) -> DeferredEffectContext {
        DeferredEffectContext {
            position: self.diagnostics.len(),
            owner: self.boundary.effect_owner,
            caller: self.boundary.current_effects.clone(),
            excluded: self.boundary.excluded_effects.clone(),
            in_pure: self.boundary.in_pure,
            retry_attempt_depth: self.boundary.retry_attempt_depth,
        }
    }

    fn push_deferred_effect_check(&mut self, span: Span, kind: DeferredEffectKind) {
        self.deferred_effect_checks.push(DeferredEffectCheck {
            context: self.deferred_effect_context(), span, kind, notes: Vec::new(),
        });
    }

    pub(super) fn host_method_context(&self) -> HostMethodContext {
        HostMethodContext {
            effect: self.deferred_effect_context(),
            recording: self.record_effects,
            collecting: self.collecting_effects,
            deferred: self.defer_effect_diagnostics,
        }
    }

    pub(super) fn finalize_host_method_effects(
        &mut self,
        context: &HostMethodContext,
        signature: &super::ModuleFnSig,
        name: &str,
        span: Span,
    ) {
        let saved_owner = std::mem::replace(&mut self.boundary.effect_owner, context.effect.owner);
        let saved_caller = std::mem::replace(&mut self.boundary.current_effects, context.effect.caller.clone());
        let saved_excluded = std::mem::replace(&mut self.boundary.excluded_effects, context.effect.excluded.clone());
        let saved_pure = std::mem::replace(&mut self.boundary.in_pure, context.effect.in_pure);
        let saved_retry_depth = std::mem::replace(&mut self.boundary.retry_attempt_depth, context.effect.retry_attempt_depth);
        let saved_recording = self.record_effects;
        let saved_collecting = self.collecting_effects;
        let saved_deferred = self.defer_effect_diagnostics;
        self.record_effects = context.recording;
        self.collecting_effects = context.collecting;
        self.defer_effect_diagnostics = context.deferred;
        if self.boundary.in_pure && !signature.pure {
            self.error(span, "effectful method is not allowed in pure functions", DiagnosticCode::CheckPureEffect);
        }
        for required in signature.effects {
            self.require_effect(required.clone(), span, &format!("method `{name}`"));
        }
        self.boundary.effect_owner = saved_owner;
        self.boundary.current_effects = saved_caller;
        self.boundary.excluded_effects = saved_excluded;
        self.boundary.in_pure = saved_pure;
        self.boundary.retry_attempt_depth = saved_retry_depth;
        self.record_effects = saved_recording;
        self.collecting_effects = saved_collecting;
        self.defer_effect_diagnostics = saved_deferred;
    }

    pub(super) fn finish_host_method_diagnostics(
        &mut self,
        context: &HostMethodContext,
        diagnostics_before: usize,
        deferred_before: usize,
    ) {
        let generated = self.diagnostics.drain(diagnostics_before..).collect::<Vec<_>>();
        let insertion = context.effect.position.min(self.diagnostics.len());
        for (index, check) in self.deferred_effect_checks.iter_mut().enumerate() {
            if index >= deferred_before {
                check.context.position = insertion + check.context.position.saturating_sub(diagnostics_before);
            } else if check.context.position >= insertion {
                check.context.position += generated.len();
            }
        }
        self.diagnostics.splice(insertion..insertion, generated);
    }

    pub(super) fn shift_host_method_context(context: &mut HostMethodContext, offset: usize) {
        context.effect.position += offset;
    }

    pub(super) fn defer_callable_effects(&mut self, contract: Option<Vec<Effect>>, unknown_chain: Vec<String>, name: &str, span: Span) {
        if !self.defer_effect_diagnostics { return; }
        self.push_deferred_effect_check(span, DeferredEffectKind::Invocation {
            effects: super::InvocationEffects::Checked(crate::sema::types::CallableEffects::from_clause(contract)),
            instance: None, stream_item: None, unknown_chain, name: name.to_owned(),
        });
    }

    fn constrain_callable_effects(&mut self, effects: &crate::sema::types::CallableEffects, name: &str, span: Span) {
        let resolved = match self.type_constraints.resolve_effect(effects.contract()) {
            Ok(contract) => contract,
            Err(_) => {
                self.error(span, "callable effects escaped their declaration inference scope", DiagnosticCode::CheckLocalInference);
                return;
            }
        };
        if let Some(caller) = self.boundary.current_effects.clone() {
            let outward = if self.boundary.retry_attempt_depth > 0 { resolved.clone().capture_error() } else { resolved.clone() };
            if let Err(error) = self.type_constraints.constrain_effect(crate::sema::effects::EffectPredicate::Covered(outward, EffectContract::finite(EffectSet::new(caller.iter().cloned())))) {
                match (error, resolved.concrete()) {
                    (crate::sema::constraints::EffectConstraintError::Mismatch, Ok(effects)) => self.check_callee_effects(&caller, &effects, name, span),
                    (crate::sema::constraints::EffectConstraintError::Mismatch, Err(_)) => self.error(span, &format!("effect relationship required by `{name}` is not in caller's declared effects"), DiagnosticCode::CheckEffectViolation),
                    _ => self.error(span, "callable effect relationships cannot be resolved", DiagnosticCode::CheckLocalInference),
                }
            }
        }
        self.constrain_callable_effect_exclusions(&resolved, name, span);
    }

    pub(super) fn owns_effect_predicate(
        &self,
        contract: &EffectContract<EffectVariableId>,
        predicate: impl FnOnce(EffectContract<SchemeEffectParameterId>) -> crate::sema::effects::EffectPredicate<SchemeEffectParameterId>,
    ) -> bool {
        let Some(owner) = self.boundary.effect_owner else { return false; };
        let Some(declaration) = self.declaration_inference.pending(super::FunctionDeclarationId::new(owner)) else { return false; };
        let (Some(scope), Some(callable)) = (declaration.generalized_scope(), declaration.generalized()) else { return false; };
        self.type_constraints.resolve_generalized_effect(contract, scope).is_ok_and(|contract| callable.requirements().effect_predicates.contains(&predicate(contract)))
    }

    fn validate_callable_effects(&mut self, effects: &crate::sema::types::CallableEffects, name: &str, span: Span) {
        let resolved = match self.type_constraints.resolve_effect(effects.contract()) {
            Ok(contract) => contract,
            Err(_) => {
                self.error(span, "callable effects escaped their declaration inference scope", DiagnosticCode::CheckLocalInference);
                return;
            }
        };
        match resolved.concrete() {
            Ok(effects) => {
                if let Some(caller) = self.boundary.current_effects.clone() { self.check_callee_effects(&caller, &effects, name, span); }
                self.check_callee_not_excluded(&effects, &[], name, span);
            }
            Err(_) => {
                if let Some(caller) = self.boundary.current_effects.clone() {
                    let outward = if self.boundary.retry_attempt_depth > 0 { resolved.clone().capture_error() } else { resolved.clone() };
                    if !self.owns_effect_predicate(&outward, |contract| crate::sema::effects::EffectPredicate::Covered(contract, EffectContract::finite(EffectSet::new(caller)))) {
                        self.error(span, "callable effect permission is outside its checked scheme", DiagnosticCode::CheckLocalInference);
                    }
                }
                self.validate_symbolic_callable_effect_exclusions(&resolved, span);
            }
        }
    }

    pub(super) fn check_callable_invocation_contract(&mut self, effects: &crate::sema::types::CallableEffects, name: &str, span: Span) {
        if self.record_effects || self.collecting_effects {
            if let Err(error) = self.effect_graph.require_contract(self.boundary.effect_owner, effects.contract(), self.boundary.retry_attempt_depth > 0, name) {
                self.error(span, &format!("callable effect contract is not scoped: {error:?}"), DiagnosticCode::CheckLocalInference);
            }
        }
        let diagnostic_count = self.diagnostics.len();
        self.constrain_callable_effects(effects, name, span);
        if self.defer_effect_diagnostics && self.diagnostics.len() == diagnostic_count {
            self.push_deferred_effect_check(span, DeferredEffectKind::Invocation {
                effects: super::InvocationEffects::Checked(effects.clone()), instance: None, stream_item: None,
                unknown_chain: Vec::new(), name: name.to_owned(),
            });
        }
    }

    pub(super) fn check_function_invocation_effects(&mut self, contract: &super::FunctionCallContract, name: &str, span: Span) {
        if self.record_effects || self.collecting_effects {
            if let Some(target) = contract.effect_declaration.filter(|target| self.effect_graph.contains(*target)) {
                self.effect_graph.call(self.boundary.effect_owner, target, self.boundary.retry_attempt_depth > 0, Some(span));
            } else if let super::InvocationEffects::Checked(effects) = &contract.effects {
                if let Err(error) = self.effect_graph.require_contract(self.boundary.effect_owner, effects.contract(), self.boundary.retry_attempt_depth > 0, name) {
                    self.error(span, &format!("callable effect contract is not scoped: {error:?}"), DiagnosticCode::CheckLocalInference);
                }
            }
        }
        let diagnostic_count = self.diagnostics.len();
        if let super::InvocationEffects::Checked(effects) = &contract.effects { self.constrain_callable_effects(effects, name, span); }
        if self.defer_effect_diagnostics && self.diagnostics.len() == diagnostic_count {
            self.push_deferred_effect_check(span, DeferredEffectKind::Invocation {
                effects: contract.effects.clone(), instance: contract.instantiation.clone(), stream_item: None,
                unknown_chain: contract.effect_declaration.and_then(|owner| self.effect_summaries.get(&owner)).map_or_else(Vec::new, |summary| summary.unknown_chain.clone()),
                name: name.to_owned(),
            });
        }
    }

    fn resolve_invocation_effects(&self, effects: super::InvocationEffects, instance: Option<&super::PendingCallInstantiation>) -> Result<crate::sema::types::CallableEffects, SchemeOperationError> {
        let effects = match effects {
            super::InvocationEffects::Checked(effects) => effects,
            super::InvocationEffects::Pending(owner) => {
                let summary = self.effect_summaries.get(&owner).ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects))?;
                if let Some((_, instance)) = instance { self.type_constraints.instantiate_effect_contract(&summary.effects(), instance)? }
                else {
                    crate::sema::types::CallableEffects::from_contract(summary.effects().contract().substitute(|_| Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter)))?)
                }
            }
        };
        Ok(crate::sema::types::CallableEffects::from_contract(self.type_constraints.resolve_effect(effects.contract())?))
    }

    pub(super) fn validate_deferred_effects(&mut self) {
        if self.deferred_effect_checks.is_empty() {
            return;
        }
        let mut checks = std::mem::take(&mut self.deferred_effect_checks);
        checks.sort_by_key(|check| check.context.position);
        let mut inserted = 0;
        for check in checks {
            let start = self.diagnostics.len();
            let deferred_span = check.span;
            let notes = check.notes.clone();
            let saved_boundary = self.boundary.clone();
            let saved_collecting = self.collecting_effects;
            let saved_recording = self.record_effects;
            let saved_deferred = self.defer_effect_diagnostics;
            self.boundary.current_effects = self.caller_effect_contract(check.context.owner, &check.context.caller);
            self.boundary.effect_owner = check.context.owner;
            self.boundary.excluded_effects = check.context.excluded;
            self.boundary.in_pure = check.context.in_pure;
            self.boundary.retry_attempt_depth = check.context.retry_attempt_depth;
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
                DeferredEffectKind::Invocation { effects, instance, stream_item, unknown_chain, name } => {
                    match self.resolve_invocation_effects(effects, instance.as_ref()) {
                        Ok(effects) => {
                            if let Some(item) = stream_item {
                                self.record_invocation_propagation(&super::InvocationEffects::Checked(effects.clone()), &Type::Stream(Box::new(item)), check.span);
                            }
                            self.validate_callable_effects(&effects, &name, check.span);
                            if matches!(effects.contract(), EffectContract::Unknown) && !unknown_chain.is_empty() && self.boundary.current_effects.is_some() {
                                self.error(check.span, &format!("proc `{name}` has an unknown effect summary: {}", unknown_chain.join(" -> ")), DiagnosticCode::CheckEffectViolation);
                            }
                        }
                        Err(_) => self.error(check.span, "callable invocation has no settled declaration effect contract", DiagnosticCode::CheckLocalInference),
                    }
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
            let insertion = check.context.position.saturating_add(inserted).min(self.diagnostics.len());
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

    fn caller_effect_contract(&self, owner: Option<EffectDeclarationId>, fallback: &Option<Vec<Effect>>) -> Option<Vec<Effect>> {
        owner.and_then(|owner| self.effect_graph.nodes.get(&owner)).and_then(|node| node.declared.clone()).or_else(|| fallback.clone())
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

    pub(super) fn effective_function_effects(&self, program: &ArenaProgram, def: &ArenaFunctionDef) -> Option<Vec<Effect>> {
        def.effects.map(|effects| program.arena.effects(effects).collect())
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

    pub(super) fn declaration_effects(&self, declaration: Option<EffectDeclarationId>) -> super::DeclarationEffects {
        let Some((id, node)) = declaration.and_then(|id| self.effect_graph.nodes.get(&id).map(|node| (id, node))) else {
            return super::DeclarationEffects::Checked(CheckedEffects::unknown());
        };
        if let Some(declared) = &node.declared {
            return super::DeclarationEffects::Checked(CheckedEffects::finite(declared.iter().cloned()));
        }
        if !node.inferred { return super::DeclarationEffects::Checked(CheckedEffects::unknown()); }
        if self.collecting_effects { self.provisional_effects_read.set(true); }
        match self.effect_summaries.get(&id) {
            Some(summary) => super::DeclarationEffects::Checked(summary.effects()),
            None => super::DeclarationEffects::Pending(id),
        }
    }

    pub(super) fn instantiate_invocation_effects(
        &self,
        kind: crate::sema::types::CallableKind,
        declaration: Option<EffectDeclarationId>,
        effects: super::DeclarationEffects,
        instance: Option<&crate::sema::constraints::InstantiatedScheme>,
    ) -> Result<super::InvocationEffects, SchemeOperationError> {
        if kind == crate::sema::types::CallableKind::Pure {
            return Ok(super::InvocationEffects::Checked(crate::sema::types::CallableEffects::finite([])));
        }
        let effects = match effects {
            super::DeclarationEffects::Pending(owner) => return Ok(super::InvocationEffects::Pending(owner)),
            super::DeclarationEffects::Checked(effects) => effects,
        };
        let effects = if let Some(instance) = instance {
            self.type_constraints.instantiate_effect_contract(&effects, instance)?
        } else if effects.is_symbolic() {
            return declaration.map(super::InvocationEffects::Pending)
                .ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter));
        } else {
            let contract = effects.contract().substitute(|_| Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter)))?;
            crate::sema::types::CallableEffects::from_contract(contract)
        };
        Ok(super::InvocationEffects::Checked(effects))
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
            self.defer_callable_effects(None, Vec::new(), name, span);
            return;
        }
        if let Some(caller) = self.boundary.current_effects.clone() {
            self.check_callee_effects(&caller, &None, name, span);
        }
        self.check_callee_not_excluded(&None, &[], name, span);
    }
}
