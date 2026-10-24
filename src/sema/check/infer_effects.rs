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

    fn pending_component_contract(
        &self,
        target: EffectDeclarationId,
        members: &BTreeMap<EffectDeclarationId, crate::syntax::arena::FunctionDefId>,
    ) -> Result<EffectContract<EffectVariableId>, SchemeOperationError> {
        let invalid = || SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter);
        let mut contract = EffectContract::finite(EffectSet::default());
        let mut pending = vec![(target, false)];
        let mut visited = BTreeSet::new();
        while let Some((owner, captured)) = pending.pop() {
            if !visited.insert((owner, captured)) { continue; }
            let node = self.nodes.get(&owner).ok_or_else(invalid)?;
            let contribution = if let Some(declared) = &node.declared {
                EffectContract::finite(EffectSet::new(declared.iter().cloned()))
            } else {
                let contribution = match &node.direct {
                    EffectContributions::Checking(summary) => {
                        if !members.contains_key(&owner) && summary.known.variables().next().is_some() { return Err(invalid()); }
                        if summary.unknown_chain.is_empty() { EffectContract::Known(summary.known.clone()) } else { EffectContract::Unknown }
                    }
                    EffectContributions::Checked(summary) => summary.effects().contract().substitute(|_| Err(invalid()))?,
                };
                for edge in &node.edges {
                    if let Some(work) = &self.work { work.record_effect_edge_visit(); }
                    pending.push((edge.target, captured || edge.captures_error));
                }
                contribution
            };
            contract = contract.union(if captured { contribution.capture_error() } else { contribution });
        }
        Ok(contract)
    }

    fn bind_scopes(
        &mut self,
        mut bind: impl FnMut(EffectDeclarationId, &EffectContract<EffectVariableId>) -> Result<EffectContract<SchemeEffectParameterId>, SchemeOperationError>,
        mut parameters: impl FnMut(EffectDeclarationId) -> Arc<[SchemeEffectParameterId]>,
        calls: &BTreeMap<crate::sema::schemes::InstantiationSite<Span>, crate::sema::value_materialization::CheckedInstantiation>,
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
                    if let Some(evidence) = calls.get(&crate::sema::schemes::InstantiationSite::call(site)) {
                        let instance = evidence.invocation().ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedCallableApplication))?;
                        edge.instantiation = Some(Arc::new(instance.clone()));
                    }
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
                map_key_parameters: Vec::new(),
                iteration_parameters: BTreeMap::new(), operations: Vec::new(),
                methods: Vec::new(),
                wrappers: Vec::new(),
                conversion_parameters: BTreeMap::new(),
                effect_parameters: vec![parameter],
                effect_predicates: vec![crate::sema::effects::EffectPredicate::Equal(EffectContract::variable(parameter), EffectContract::variable(parameter))],
                applications: Vec::new(),
                guards: Vec::new(), guard_calls: Vec::new(),
            },
        ).unwrap();
        CallInstantiation::new(&callable, BTreeMap::new(), BTreeMap::new(), BTreeMap::from([(parameter, argument)]), Vec::new(), Vec::new(), Vec::new(), &[], &[], caller, &[], &[], &[], &[], &BTreeMap::new()).unwrap()
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
    CallableFit {
        target: Arc<crate::sema::types::TypedCallable>,
        actual: super::CallableType,
        effects: super::InvocationEffects,
        kind: crate::sema::types::CallableKind,
        name: Name,
    },
    Invocation {
        effects: super::InvocationEffects,
        validation: InvocationEffectValidation,
        instance: Option<Box<super::PendingCallInstantiation>>,
        flow: InvocationFlow,
        unknown_chain: Vec<String>,
        name: String,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) enum InvocationEffectValidation {
    Pending,
    Diagnosed,
}

#[derive(Clone, Debug)]
pub(super) enum InvocationFlow {
    EffectsOnly,
    AwaitingReturn,
    Rejected,
    Return(Type),
}

pub(super) struct DeferredInvocationTicket(usize);

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

#[cfg(test)]
mod deferred_invocation_tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::arena::ArenaExprKind;
    use crate::syntax::parser::Parser;

    #[test]
    fn source_owned_invocation_close_preserves_pending_return_flows() {
        let source = concat!(
            "proc scalar() -> Int { assert false; 7 }\n",
            "proc wrapped() -> Result[Int] { assert false; Ok(7) }\n",
            "let _ = scalar()\nlet _ = wrapped()\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let program = &parsed.arena;
            let checked = Checker::check_arena_with_options(program, source, super::super::CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let view = super::super::CheckBundleView::for_root(program, program.statements, None, Some(SourceId::new(0)));
            let statements = program.statement_ids().collect::<Vec<_>>();
            let mut checker = Checker::new(super::super::CheckOptions::for_execution());
            checker.resolved_bindings = Arc::new(super::super::ResolvedBindings::collect(program));
            checker.effect_graph = EffectGraph::declarations_for_root(&view);
            checker.authoritative_effect_graph = true;
            checker.collecting_effects = true;
            checker.record_effects = true;
            checker.defer_effect_diagnostics = true;
            checker.collect_definitions_arena(program, Arc::new(program.clone()), source, statements.iter().copied());
            checker.infer_default_parameter_types(program, source, &statements);
            checker.infer_local_pure_returns(&view, source, &statements);
            for statement in statements { checker.check_stmt_arena(program, source, statement); }
            assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
            let call_span = |name: &str| view.expr_ids().into_iter().find_map(|expression| {
                let ArenaExprKind::Call { callee, .. } = program.arena.expr(expression).kind else { return None; };
                matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(actual) if actual == name)
                    .then_some(program.arena.expr(expression).span)
            }).expect("the source retains its actual invocation");
            let scalar = call_span("scalar");
            let wrapped = call_span("wrapped");
            assert!(checked.checked_control_flow.propagating_error_spans.contains(&scalar));
            assert!(!checked.checked_control_flow.propagating_error_spans.contains(&wrapped));
            for span in [scalar, wrapped] {
                let checks = checker.deferred_effect_checks.iter().filter(|check| check.span == span
                    && matches!(check.kind, DeferredEffectKind::Invocation { effects: super::super::InvocationEffects::Pending(_), .. })).count();
                assert_eq!(checks, 1);
            }
            checker.bind_effect_graph().unwrap();
            checker.effect_summaries.extend(checker.effect_graph.solve_with_external(&BTreeMap::new()).unwrap());
            checker.validate_deferred_effects();
            assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
            assert!(checker.propagating_error_spans.contains(&scalar));
            assert!(!checker.propagating_error_spans.contains(&wrapped));
            assert!(!checker.latent_stream_error_spans.contains(&scalar));
            assert!(!checker.latent_stream_error_spans.contains(&wrapped));
        });
    }

    #[test]
    fn pending_invocation_tickets_reject_wrong_occurrences_and_unclosed_returns() {
        let span = Span::new(SourceId::new(0), 1, 3);
        let target = EffectDeclarationId { namespace: None, body: span };
        let other = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), 8, 11) };
        let contract = super::super::FunctionCallContract {
            params: Vec::new(), return_ty: Type::Int,
            effects: super::super::InvocationEffects::Pending(target),
            effect_declaration: Some(target), definition_target: None, instantiation: None,
        };
        let setup = |caller| {
            let mut checker = Checker::new(super::super::CheckOptions::for_execution());
            checker.defer_effect_diagnostics = true;
            checker.boundary.current_effects = caller;
            checker.effect_summaries.insert(target, EffectSummary {
                known: EffectExpr::finite(EffectSet::singleton(Effect::Error)),
                unknown_chain: Vec::new(), scope: CheckedEffectScope::default(),
            });
            let ticket = checker.check_function_invocation_effects(&contract, "scalar", span);
            (checker, ticket)
        };
        let (mut valid, ticket) = setup(None);
        valid.record_invocation_propagation(&contract.effects, &Type::Int, span, ticket);
        valid.validate_deferred_effects();
        assert!(valid.diagnostics.is_empty(), "{:?}", valid.diagnostics);
        assert!(valid.propagating_error_spans.contains(&span));
        let (mut rejected, ticket) = setup(Some(Vec::new()));
        rejected.reject_invocation_return(ticket, &contract.effects, span);
        rejected.validate_deferred_effects();
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)));
        assert!(!rejected.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)), "{:?}", rejected.diagnostics);
        assert!(!rejected.propagating_error_spans.contains(&span));
        assert!(!rejected.latent_stream_error_spans.contains(&span));
        for mutation in 0..7 {
            let (mut checker, ticket) = setup(None);
            match mutation {
                0 => checker.attach_invocation_return(None, target, &Type::Int, span),
                1 => checker.attach_invocation_return(Some(DeferredInvocationTicket(1)), target, &Type::Int, span),
                2 => checker.attach_invocation_return(ticket, other, &Type::Int, span),
                3 => checker.attach_invocation_return(ticket, target, &Type::Int, other.body),
                4 => {
                    checker.boundary.effect_owner = Some(other);
                    checker.attach_invocation_return(ticket, target, &Type::Int, span);
                },
                5 => {
                    checker.attach_invocation_return(ticket, target, &Type::Int, span);
                    checker.attach_invocation_return(Some(DeferredInvocationTicket(0)), target, &Type::Int, span);
                },
                6 => {},
                _ => unreachable!(),
            }
            checker.validate_deferred_effects();
            assert!(checker.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)), "mutation {mutation}");
            if mutation != 5 { assert!(!checker.propagating_error_spans.contains(&span), "mutation {mutation}"); }
        }
        let (mut checker, ticket) = setup(None);
        checker.attach_invocation_return(ticket, target, &Type::Unknown, span);
        checker.validate_deferred_effects();
        assert!(checker.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)));
        assert!(!checker.propagating_error_spans.contains(&span));
    }

    #[test]
    fn pending_no_error_invocation_accepts_an_unresolved_generic_return_shape() {
        let span = Span::new(SourceId::new(0), 1, 3);
        let target = EffectDeclarationId { namespace: None, body: span };
        let contract = super::super::FunctionCallContract {
            params: Vec::new(), return_ty: Type::Unknown,
            effects: super::super::InvocationEffects::Pending(target),
            effect_declaration: Some(target), definition_target: None, instantiation: None,
        };
        for known in [EffectSet::default(), EffectSet::singleton(Effect::Io)] {
            let mut checker = Checker::new(super::super::CheckOptions::for_execution());
            checker.defer_effect_diagnostics = true;
            checker.effect_summaries.insert(target, EffectSummary {
                known: EffectExpr::finite(known),
                unknown_chain: Vec::new(), scope: CheckedEffectScope::default(),
            });
            let ticket = checker.check_function_invocation_effects(&contract, "generic", span);
            let return_ty = checker.type_constraints.fresh_at_level(span, 0);
            checker.record_invocation_propagation(&contract.effects, &return_ty, span, ticket);
            checker.validate_deferred_effects();
            assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
            assert!(!checker.propagating_error_spans.contains(&span));
            assert!(!checker.latent_stream_error_spans.contains(&span));
        }
    }

    #[test]
    fn pending_result_envelope_proof_requires_the_matching_callable_instance() {
        let span = Span::new(SourceId::new(0), 1, 3);
        let return_parameter = crate::sema::schemes::SchemeTypeParameterId(0);
        let result_parameter = crate::sema::schemes::SchemeTypeParameterId(1);
        let result_type = crate::sema::schemes::SchemeType::Parameter(result_parameter);
        let return_type = crate::sema::schemes::SchemeType::Parameter(return_parameter);
        let callback = crate::sema::schemes::SchemeType::Callable {
            kind: crate::sema::types::CallableKind::Pure,
            monotype: Arc::new(crate::sema::schemes::CallableMonotype::new(
                Vec::new(),
                result_type.clone(),
                None,
            )),
            effects: crate::sema::effects::EffectContract::finite(crate::sema::effects::EffectSet::default()),
        };
        let application = crate::sema::callable_application::CallableInvocation {
            callee: callback,
            arguments: Arc::from([]),
            result: result_type,
            effects: crate::sema::effects::EffectContract::finite(crate::sema::effects::EffectSet::default()),
            proc_returns: Arc::from([crate::sema::callable_application::ProcReturnEnvelope {
                result: return_type.clone(),
                propagated_errors: Arc::from([]),
                span,
            }]),
            kind: crate::sema::callable_application::CallableKindRequirement::Effectful,
            span,
        };
        let callable = Arc::new(crate::sema::schemes::CallableScheme::new(
            vec![return_parameter, result_parameter],
            Vec::new(),
            Vec::new(),
            return_type,
            None,
            false,
            crate::sema::schemes::CallableRequirements {
                applications: vec![application.into()],
                ..crate::sema::schemes::CallableRequirements::default()
            },
        ).unwrap());
        let other = Arc::new(crate::sema::schemes::CallableScheme::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            crate::sema::schemes::SchemeType::Ground(Type::Str),
            None,
            false,
            crate::sema::schemes::CallableRequirements::default(),
        ).unwrap());
        let mut checker = Checker::new(super::super::CheckOptions::for_execution());
        let raw_return = checker.type_constraints.fresh_at_level(span, 1);
        let instance = checker.type_constraints.instantiate_callable(
            &callable,
            1,
            span,
            crate::sema::constraints::CallableApplicationScope::Invocation,
        ).unwrap();
        checker.type_constraints.constrain(&raw_return, &instance.return_type, span).unwrap();
        checker.type_constraints.solve_callable_applications().unwrap();
        let resolved_return = checker.type_constraints.resolve(&raw_return).unwrap();
        let resolved_instance_return = checker.type_constraints.resolve(&instance.return_type).unwrap();
        assert_eq!(resolved_return, resolved_instance_return);
        assert_ne!(resolved_return, instance.return_type);
        let proof = (Arc::clone(&callable), instance);
        assert!(checker.invocation_return_has_checked_result_envelope(Some(&proof), &resolved_return).unwrap());
        assert!(!checker.invocation_return_has_checked_result_envelope(Some(&proof), &Type::Str).unwrap());
        assert!(!checker.invocation_return_has_checked_result_envelope(None, &resolved_return).unwrap());
        let other_instance = checker.type_constraints.instantiate_callable(
            &other,
            1,
            span,
            crate::sema::constraints::CallableApplicationScope::Invocation,
        ).unwrap();
        assert!(!checker.invocation_return_has_checked_result_envelope(
            Some(&(other, other_instance)),
            &Type::Str,
        ).unwrap());
    }

    #[test]
    fn explicit_recursive_effect_bounds_accept_pure_and_empty_proc_callbacks() {
        let source = concat!(
            "proc left(action, value, count) [] { if count == 0 { return value }; right(action, value, count - 1) }\n",
            "proc right(action, value, count) { if count == 0 { return action(value) }; left(action, value, count - 1) }\n",
            "pure number(value: Int) -> Int { value }\n",
            "pure text(value: Str) -> Str { value }\n",
            "proc quiet_number(value: Int) [] -> Int { value }\n",
            "proc quiet_text(value: Str) [] -> Str { value }\n",
            "var captured = 1\nproc capture(value: Int) [] -> Int { value + captured }\n",
            "let _ = left(number, 7, 1)\nlet _ = left(text, \"row\", 1)\n",
            "let _ = left(quiet_number, 7, 1)\nlet _ = left(quiet_text, \"row\", 1)\nlet _ = left(capture, 7, 1)\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, super::super::CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declarations = checked.checked_declarations.functions.values().filter(|declaration| matches!(declaration.name.as_str().as_ref(), "left" | "right")).collect::<Vec<_>>();
        assert_eq!(declarations.len(), 2);
        for declaration in declarations {
            assert!(declaration.scheme.as_ref().unwrap().callable().is_quantified());
            for (&span, target) in &checked.selected_function_calls {
                if matches!(target.name.as_str().as_ref(), "left" | "right") && declaration.body.source_id == span.source_id && declaration.body.start() <= span.start() && span.end() <= declaration.body.end() {
                    assert!(checked.call_instantiations.contains_key(&crate::sema::schemes::InstantiationSite::call(span)));
                }
            }
        }
        let forbidden = format!("{source}proc report(value: Int) [io] -> Int {{ print $value; value }}\nlet _ = left(report, 7, 1)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &forbidden);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &forbidden, super::super::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)), "{:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn callback_effect_mismatches_reject_calls_without_result_cascades() {
        let source = concat!(
            "proc report(value: Str) [io] -> Str { print $value; return value }\n",
            "proc deny(callback, value) [] { callback(value) }\n",
            "let result = deny(report, \"forbidden\")?\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, super::super::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, Some(DiagnosticCode::CheckEffectViolation));
        let view = super::super::CheckBundleView::for_root(&parsed.arena, parsed.arena.statements, None, Some(SourceId::new(0)));
        let denied = view.expr_ids().into_iter().find_map(|expression| {
            let node = parsed.arena.arena.expr(expression);
            let ArenaExprKind::Call { callee, .. } = node.kind else { return None; };
            matches!(parsed.arena.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "deny").then_some(node.span)
        }).unwrap();
        assert_eq!(checked.expr_types.get(&denied), Some(&Type::Invalid));
        assert!(!checked.call_instantiations.contains_key(&crate::sema::schemes::InstantiationSite::call(denied)));
        assert!(!checked.checked_control_flow.propagating_error_spans.contains(&denied));
        assert!(!checked.checked_control_flow.latent_stream_error_spans.contains(&denied));
        let pure_mismatch = "pure identity(value: Int) -> Int { value }\nlet _ = identity(\"wrong\")?\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), pure_mismatch);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, pure_mismatch, super::super::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, Some(DiagnosticCode::CheckTypeMismatch));
        let incompatible = "pure number(value: Int) -> Int { value }\nproc apply(callback, value) [] { callback(value) }\nlet _ = apply(number, \"wrong\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), incompatible);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, incompatible, super::super::CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckCallableMismatch)), "{:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn rejected_arguments_preserve_one_reported_effect_violation() {
        let source = concat!(
            "proc needs_io(value: Int) [io] { print \"x\"; value }\n",
            "proc caller(value: Str) [fs] { needs_io(value) }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, super::super::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert_eq!(errors.iter().filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)).count(), 1, "{errors:?}");
        assert_eq!(errors.iter().filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)).count(), 1, "{errors:?}");
        assert_eq!(errors.len(), 2, "{errors:?}");
        let valid_arguments = concat!(
            "pure identity(value: Int) -> Int { value }\n",
            "pure passthrough(value: Result[Int, Error]) -> Result[Int, Error] { value }\n",
            "proc needs_io(value: Int) [io] { print \"x\"; value }\n",
            "proc caller(value: Int) [fs] { let sibling = identity(1); passthrough(needs_io(value)) }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), valid_arguments);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, valid_arguments, super::super::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, Some(DiagnosticCode::CheckEffectViolation));
        let call_span = |call: &str| {
            let start = valid_arguments.find(call).unwrap();
            Span::new(SourceId::new(0), start, start + call.len())
        };
        assert!(checked.statically_resolved_call_spans.contains(&call_span("identity(1)")));
        assert!(!checked.statically_resolved_call_spans.contains(&call_span("needs_io(value)")));
        assert!(!checked.statically_resolved_call_spans.contains(&call_span("passthrough(needs_io(value))")));
    }

    #[test]
    fn recursive_proc_callback_flows_keep_actual_peer_instances() {
        let source = concat!(
            "proc fail(value: Int) [error] -> Int { assert false; value }\n",
            "pure pass(value: Int) -> Int { value }\n",
            "proc left(action, value, count = 1) -> Int { if count > 0 { return right(action, value, count - 1) }; action(value) }\n",
            "proc right(action, value, count = 1) -> Int { if count > 0 { return left(action, value, count - 1) }; action(value) }\n",
            "let _ = left(fail, 7, 2)\nlet _ = right(pass, 7, 2)\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let checked = Checker::check_arena_with_options(&parsed.arena, source, super::super::CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let mut peer_calls = Vec::new();
            for declaration in checked.checked_declarations.functions.values().filter(|declaration| matches!(declaration.name.as_str().as_ref(), "left" | "right")) {
                let scheme = declaration.scheme.as_ref().expect("the recursive callback has a checked scheme");
                assert!(!scheme.callable().requirements().applications.is_empty());
                peer_calls.extend(checked.selected_function_calls.iter().filter_map(|(&span, target)| {
                    (matches!(target.name.as_str().as_ref(), "left" | "right")
                        && declaration.body.source_id == span.source_id
                        && declaration.body.start() <= span.start() && span.end() <= declaration.body.end()).then_some(span)
                }));
            }
            assert_eq!(peer_calls.len(), 2);
            assert!(peer_calls.iter().all(|span| checked.call_instantiations.contains_key(&crate::sema::schemes::InstantiationSite::call(*span))
                && checked.checked_control_flow.propagating_error_spans.contains(span)
                && !checked.checked_control_flow.latent_stream_error_spans.contains(span)));
            let view = super::super::CheckBundleView::for_root(&parsed.arena, parsed.arena.statements, None, Some(SourceId::new(0)));
            for (callee_name, propagates) in [("left", true), ("right", false)] {
                let span = view.expr_ids().into_iter().filter_map(|expression| {
                    let expr = parsed.arena.arena.expr(expression);
                    let ArenaExprKind::Call { callee, .. } = expr.kind else { return None; };
                    let ArenaExprKind::Ident(name) = parsed.arena.arena.expr(callee).kind else { return None; };
                    (name == callee_name && !peer_calls.contains(&expr.span)).then_some(expr.span)
                }).next().expect("the source retains its actual root invocation");
                assert!(checked.call_instantiations.contains_key(&crate::sema::schemes::InstantiationSite::call(span)));
                assert_eq!(checked.checked_control_flow.propagating_error_spans.contains(&span), propagates);
            }
        });
    }

}


impl Checker {
    pub(super) fn defer_named_callable_fit(
        &mut self,
        named: &super::CallableAlias,
        target: Arc<crate::sema::types::TypedCallable>,
        actual: super::CallableType,
        span: Span,
    ) -> bool {
        let declaration = named.signature.effect_declaration();
        let effects = self.instantiate_invocation_effects(named.kind, declaration, || {
            if declaration.is_some_and(|owner| self.effect_graph.contains(owner)) {
                self.signature_declaration_effects(declaration)
            } else {
                super::DeclarationEffects::Checked(named.signature.scheme().effects().clone())
            }
        }, None);
        let effects = match effects {
            Ok(effects) => effects,
            Err(_) => {
                self.error(span, "callable assignment effects escaped their source scope", DiagnosticCode::CheckCallableMismatch);
                return false;
            }
        };
        self.push_deferred_effect_check(span, DeferredEffectKind::CallableFit {
            target, actual, effects, kind: named.kind, name: named.name,
        });
        true
    }

    pub(super) fn attach_invocation_return(
        &mut self,
        ticket: Option<DeferredInvocationTicket>,
        target: EffectDeclarationId,
        return_ty: &Type,
        span: Span,
    ) {
        let Some(check) = ticket.and_then(|ticket| self.deferred_effect_checks.get_mut(ticket.0)) else {
            self.error(span, "pending invocation has no owned effect check", DiagnosticCode::CheckLocalInference);
            return;
        };
        if check.span != span || check.context.owner != self.boundary.effect_owner {
            self.error(span, "pending invocation effect check belongs to another source owner", DiagnosticCode::CheckLocalInference);
            return;
        }
        let DeferredEffectKind::Invocation { effects: super::InvocationEffects::Pending(actual), flow, .. } = &mut check.kind else {
            self.error(span, "pending invocation effect check has another call contract", DiagnosticCode::CheckLocalInference);
            return;
        };
        if *actual != target || !matches!(flow, InvocationFlow::AwaitingReturn) {
            self.error(span, "pending invocation return does not match its effect check", DiagnosticCode::CheckLocalInference);
            return;
        }
        *flow = InvocationFlow::Return(return_ty.clone());
    }

    pub(super) fn reject_invocation_return(
        &mut self,
        ticket: Option<DeferredInvocationTicket>,
        effects: &super::InvocationEffects,
        span: Span,
    ) {
        let Some(ticket) = ticket else {
            if self.defer_effect_diagnostics {
                self.push_deferred_effect_check(span, DeferredEffectKind::Invocation {
                    effects: effects.clone(), validation: InvocationEffectValidation::Pending,
                    instance: None, flow: InvocationFlow::Rejected,
                    unknown_chain: Vec::new(), name: "rejected callable".to_owned(),
                });
            }
            return;
        };
        let Some(check) = self.deferred_effect_checks.get_mut(ticket.0) else {
            self.error(span, "rejected invocation has no owned effect check", DiagnosticCode::CheckLocalInference);
            return;
        };
        if check.span != span || check.context.owner != self.boundary.effect_owner {
            self.error(span, "rejected invocation effect check belongs to another source owner", DiagnosticCode::CheckLocalInference);
            return;
        }
        let DeferredEffectKind::Invocation { effects: actual, flow, .. } = &mut check.kind else {
            self.error(span, "rejected invocation effect check has another call contract", DiagnosticCode::CheckLocalInference);
            return;
        };
        let matches_contract = match (actual, effects, &*flow) {
            (super::InvocationEffects::Pending(actual), super::InvocationEffects::Pending(target), InvocationFlow::AwaitingReturn) => actual == target,
            (super::InvocationEffects::Checked(actual), super::InvocationEffects::Checked(expected), InvocationFlow::EffectsOnly) => actual == expected,
            _ => false,
        };
        if !matches_contract {
            self.error(span, "rejected invocation does not match its effect check", DiagnosticCode::CheckLocalInference);
            return;
        }
        *flow = InvocationFlow::Rejected;
    }

    pub(super) fn invocation_return_is_rejected(&self, span: Span) -> bool {
        self.deferred_effect_checks.iter().any(|check| {
            check.span == span && check.context.owner == self.boundary.effect_owner
                && matches!(check.kind, DeferredEffectKind::Invocation { flow: InvocationFlow::Rejected, .. })
        })
    }

    pub(super) fn constrain_pending_component_invocations(
        &mut self,
        members: &BTreeMap<EffectDeclarationId, crate::syntax::arena::FunctionDefId>,
    ) -> Option<()> {
        let checks = self.deferred_effect_checks.iter().filter_map(|check| {
            let DeferredEffectKind::Invocation { effects: super::InvocationEffects::Pending(target), name, .. } = &check.kind else { return None; };
            ((check.context.caller.is_some() || check.context.in_pure || !check.context.excluded.is_empty())
                && check.context.owner.is_some_and(|owner| members.contains_key(&owner)) && members.contains_key(target))
                .then(|| (check.context.clone(), check.span, *target, name.clone()))
        }).collect::<Vec<_>>();
        let before = self.diagnostics.len();
        for (context, span, target, name) in checks {
            let Some(&(caller, callee)) = self.provisional_function_calls.get(&span) else {
                self.error(span, "pending recursive permission has no selected call", DiagnosticCode::CheckLocalInference);
                continue;
            };
            if context.owner != Some(caller) || callee.effect_declaration() != target {
                self.error(span, "pending recursive permission belongs to another call", DiagnosticCode::CheckLocalInference);
                continue;
            }
            let contract = match self.effect_graph.pending_component_contract(target, members) {
                Ok(contract) => contract,
                Err(error) => {
                    self.error(span, &format!("pending recursive permission has no shared effect scope: {error:?}"), DiagnosticCode::CheckLocalInference);
                    continue;
                }
            };
            let saved = self.boundary.clone();
            self.boundary.current_effects = context.caller;
            self.boundary.effect_owner = context.owner;
            self.boundary.excluded_effects = context.excluded;
            self.boundary.in_pure = context.in_pure;
            self.boundary.retry_attempt_depth = context.retry_attempt_depth;
            self.constrain_callable_effects(&crate::sema::types::CallableEffects::from_contract(contract), &name, span);
            self.boundary = saved;
        }
        (self.diagnostics.len() == before).then_some(())
    }

    pub(super) fn prepare_recursive_stream_error_flows(
        &self,
        members: &BTreeMap<EffectDeclarationId, crate::syntax::arena::FunctionDefId>,
        instances: &BTreeMap<crate::sema::schemes::InstantiationSite<Span>, super::PendingInstantiation>,
    ) -> Result<BTreeSet<Span>, (Span, SchemeOperationError)> {
        let mut latent = BTreeSet::new();
        if self.collecting_effects { return Ok(latent); }
        for check in &self.deferred_effect_checks {
            if !check.context.owner.is_some_and(|owner| members.contains_key(&owner)) { continue; }
            let DeferredEffectKind::Invocation { effects: super::InvocationEffects::Pending(target), flow: InvocationFlow::Return(return_ty), .. } = &check.kind else { continue; };
            if !members.contains_key(target) || !matches!(return_ty.unvalidated(), Type::Stream(_)) { continue; }
            let invalid = || (check.span, SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects));
            let &(caller, callee) = self.provisional_function_calls.get(&check.span).ok_or_else(invalid)?;
            if check.context.owner != Some(caller) || callee.effect_declaration() != *target { return Err(invalid()); }
            let instance = instances.get(&crate::sema::schemes::InstantiationSite::call(check.span)).and_then(|evidence| evidence.invocation()).ok_or_else(invalid)?;
            let effects = self.resolve_invocation_effects(super::InvocationEffects::Pending(*target), Some(instance)).map_err(|error| (check.span, error))?;
            let flow = self.invocation_error_flow(&effects, return_ty).map_err(|error| (check.span, error.into()))?;
            if matches!(flow, crate::sema::types::CalleeErrorFlow::Latent) { latent.insert(check.span); }
        }
        Ok(latent)
    }

    pub(super) fn publish_recursive_invocation_instances(
        &mut self,
        instances: &BTreeMap<crate::sema::schemes::InstantiationSite<Span>, super::PendingInstantiation>,
    ) {
        for check in &mut self.deferred_effect_checks {
            if let Some(evidence) = instances.get(&crate::sema::schemes::InstantiationSite::call(check.span))
                && let DeferredEffectKind::Invocation { effects: super::InvocationEffects::Pending(owner), instance, .. } = &mut check.kind
            {
                let (_, callee) = self.provisional_function_calls.get(&check.span)
                    .expect("a recursive effect check retains its selected call");
                assert_eq!(*owner, callee.effect_declaration());
                assert!(instance.is_none());
                *instance = Some(Box::new(evidence.invocation().expect("a recursive call retains invocation evidence").clone()));
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

    fn push_deferred_effect_check(&mut self, span: Span, kind: DeferredEffectKind) -> usize {
        let index = self.deferred_effect_checks.len();
        self.deferred_effect_checks.push(DeferredEffectCheck {
            context: self.deferred_effect_context(), span, kind, notes: Vec::new(),
        });
        index
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
            validation: InvocationEffectValidation::Pending, instance: None, flow: InvocationFlow::EffectsOnly, unknown_chain, name: name.to_owned(),
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
        span: Span,
        predicate: impl FnOnce(EffectContract<SchemeEffectParameterId>) -> crate::sema::effects::EffectPredicate<SchemeEffectParameterId>,
    ) -> bool {
        let Some(owner) = self.boundary.effect_owner else { return false; };
        let Some(declaration) = self.declaration_inference.pending(super::FunctionDeclarationId::new(owner)) else { return false; };
        let (Some(scope), Some(callable)) = (declaration.generalized_scope(), declaration.generalized()) else { return false; };
        self.type_constraints.owns_generalized_effect_predicate(self.constraint_scope_at(owner, span), callable, scope, contract, predicate).unwrap_or(false)
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
                    if !self.owns_effect_predicate(&outward, span, |contract| crate::sema::effects::EffectPredicate::Covered(contract, EffectContract::finite(EffectSet::new(caller)))) {
                        self.error(span, "callable effect permission is outside its checked scheme", DiagnosticCode::CheckLocalInference);
                    }
                }
                self.validate_symbolic_callable_effect_exclusions(&resolved, span);
            }
        }
    }

    fn record_finite_effect(&mut self, effect: Effect, captured: bool, name: &str, span: Span) -> Result<(), SchemeOperationError> {
        let scope = self.boundary.effect_owner.map_or_else(|| self.constraint_scope(), |owner| self.constraint_scope_at(owner, span));
        if matches!(scope, crate::sema::constraints::CallableApplicationScope::Guard(_)) {
            self.record_scoped_effect_contract(&EffectContract::finite(EffectSet::singleton(effect)), captured, name, span)
        } else {
            self.effect_graph.require(self.boundary.effect_owner, effect, captured);
            Ok(())
        }
    }

    fn record_scoped_effect_contract(&mut self, effects: &EffectContract<EffectVariableId>, captured: bool, name: &str, span: Span) -> Result<(), SchemeOperationError> {
        let scope = self.boundary.effect_owner.map_or_else(|| self.constraint_scope(), |owner| self.constraint_scope_at(owner, span));
        let captures = captured || self.boundary.in_pure && matches!(scope, crate::sema::constraints::CallableApplicationScope::Guard(_));
        let actual = if captures { effects.clone().capture_error() } else { effects.clone() };
        let outward = self.type_constraints.guarded_effect_contract(scope, actual, self.callable_solver_level())?;
        if matches!(scope, crate::sema::constraints::CallableApplicationScope::Guard(_))
            && let Some(caller) = &self.boundary.current_effects
        {
            self.type_constraints.constrain_effect(crate::sema::effects::EffectPredicate::Covered(outward.clone(), EffectContract::finite(EffectSet::new(caller.iter().cloned()))))
                .map_err(SchemeOperationError::Effects)?;
        }
        self.effect_graph.require_contract(self.boundary.effect_owner, &outward, false, name)
    }

    pub(super) fn check_wrapper_effect_contract(&mut self, effects: &EffectContract<EffectVariableId>, span: Span) {
        let captures = self.boundary.retry_attempt_depth > 0 || self.boundary.in_pure;
        if (self.record_effects || self.collecting_effects)
            && let Err(error) = self.record_scoped_effect_contract(effects, captures, "guarded field", span)
        {
            self.error(span, &format!("guarded field effects are not scoped: {error:?}"), DiagnosticCode::CheckLocalInference);
        }
        let outward = if captures { effects.clone().capture_error() } else { effects.clone() };
        if self.boundary.in_pure
            && let Err(error) = self.type_constraints.constrain_effect(crate::sema::effects::EffectPredicate::Covered(outward.clone(), EffectContract::finite(EffectSet::default())))
        {
            self.error(span, &format!("guarded field exceeds its pure effect contract: {error:?}"), DiagnosticCode::CheckEffectViolation);
        }
        self.constrain_callable_effects(&crate::sema::types::CallableEffects::from_contract(outward), "guarded field", span);
    }

    pub(super) fn check_callable_invocation_contract(&mut self, effects: &crate::sema::types::CallableEffects, name: &str, span: Span) {
        if (self.record_effects || self.collecting_effects)
            && let Err(error) = self.record_scoped_effect_contract(effects.contract(), self.boundary.retry_attempt_depth > 0, name, span)
        {
            self.error(span, &format!("callable effect contract is not scoped: {error:?}"), DiagnosticCode::CheckLocalInference);
        }
        let diagnostic_count = self.diagnostics.len();
        self.constrain_callable_effects(effects, name, span);
        if self.defer_effect_diagnostics && self.diagnostics.len() == diagnostic_count {
            self.push_deferred_effect_check(span, DeferredEffectKind::Invocation {
                effects: super::InvocationEffects::Checked(effects.clone()), validation: InvocationEffectValidation::Pending,
                instance: None, flow: InvocationFlow::EffectsOnly,
                unknown_chain: Vec::new(), name: name.to_owned(),
            });
        }
    }

    pub(super) fn check_function_invocation_effects(&mut self, contract: &super::FunctionCallContract, name: &str, span: Span) -> Option<DeferredInvocationTicket> {
        if self.record_effects || self.collecting_effects {
            let guarded = self.boundary.effect_owner.is_some_and(|owner| matches!(self.constraint_scope_at(owner, span), crate::sema::constraints::CallableApplicationScope::Guard(_)));
            let scoped_effects = if guarded {
                if matches!(contract.effects, super::InvocationEffects::Pending(_)) { self.provisional_effects_read.set(true); }
                match self.resolve_invocation_effects(contract.effects.clone(), contract.instantiation.as_ref()) {
                    Ok(effects) => Some(effects),
                    Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects)) => None,
                    Err(error) => {
                        self.error(span, &format!("guarded callable has no actual scoped effect contract: {error:?}"), DiagnosticCode::CheckLocalInference);
                        return None;
                    }
                }
            } else { None };
            if let Some(effects) = scoped_effects {
                if let Err(error) = self.record_scoped_effect_contract(effects.contract(), self.boundary.retry_attempt_depth > 0, name, span) {
                    self.error(span, &format!("guarded callable effect contract is not scoped: {error:?}"), DiagnosticCode::CheckLocalInference);
                }
                if matches!(contract.effects, super::InvocationEffects::Pending(_)) { self.constrain_callable_effects(&effects, name, span); }
            } else if let Some(target) = contract.effect_declaration.filter(|target| self.effect_graph.contains(*target)) {
                self.effect_graph.call(self.boundary.effect_owner, target, self.boundary.retry_attempt_depth > 0, Some(span));
            } else if let super::InvocationEffects::Checked(effects) = &contract.effects
                && let Err(error) = self.record_scoped_effect_contract(effects.contract(), self.boundary.retry_attempt_depth > 0, name, span) {
                self.error(span, &format!("callable effect contract is not scoped: {error:?}"), DiagnosticCode::CheckLocalInference);
            }
        }
        let diagnostic_count = self.diagnostics.len();
        if let super::InvocationEffects::Checked(effects) = &contract.effects { self.constrain_callable_effects(effects, name, span); }
        if self.defer_effect_diagnostics {
            let validation = if self.diagnostics.len() == diagnostic_count {
                InvocationEffectValidation::Pending
            } else { InvocationEffectValidation::Diagnosed };
            let pending = matches!(contract.effects, super::InvocationEffects::Pending(_));
            let index = self.push_deferred_effect_check(span, DeferredEffectKind::Invocation {
                effects: contract.effects.clone(), validation, instance: contract.instantiation.clone().map(Box::new), flow: if pending { InvocationFlow::AwaitingReturn } else { InvocationFlow::EffectsOnly },
                unknown_chain: contract.effect_declaration.and_then(|owner| self.effect_summaries.get(&owner)).map_or_else(Vec::new, |summary| summary.unknown_chain.clone()),
                name: name.to_owned(),
            });
            return Some(DeferredInvocationTicket(index));
        }
        None
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

    fn invocation_return_has_checked_result_envelope(
        &self,
        instance: Option<&super::PendingCallInstantiation>,
        resolved_return: &Type,
    ) -> Result<bool, crate::sema::constraints::ConstraintResolutionError> {
        let Some((callable, instantiated)) = instance else { return Ok(false); };
        if self.type_constraints.resolve(&instantiated.return_type)? != *resolved_return { return Ok(false); }
        Ok(callable.requirements().applications.iter()
            .filter_map(crate::sema::callable_application::CallableApplication::invocation)
            .flat_map(|application| application.proc_returns.iter())
            .any(|envelope| &envelope.result == callable.return_type()))
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
            let invocation_diagnosed = matches!(check.kind, DeferredEffectKind::Invocation { validation: InvocationEffectValidation::Diagnosed, .. });
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
                DeferredEffectKind::CallableFit { target, mut actual, effects, kind, name } => {
                    match self.resolve_invocation_effects(effects, None) {
                        Ok(effects) => {
                            actual.effects = effects;
                            if let Some(reason) = target.mismatch(kind, &actual, true) {
                                let expected = Type::Callable(target);
                                self.error(check.span, &format!("`{name}` does not fit `{expected}`: {reason}"), DiagnosticCode::CheckCallableMismatch);
                            }
                        }
                        Err(_) => self.error(check.span, "callable assignment has no settled source effect contract", DiagnosticCode::CheckCallableMismatch),
                    }
                }
                DeferredEffectKind::Invocation { validation: InvocationEffectValidation::Diagnosed, .. } => {},
                DeferredEffectKind::Invocation { effects, validation: InvocationEffectValidation::Pending, instance, flow, unknown_chain, name } => {
                    let unknown_chain = match &effects {
                        super::InvocationEffects::Pending(owner) => self.effect_summaries.get(owner)
                            .map_or(unknown_chain, |summary| summary.unknown_chain.clone()),
                        super::InvocationEffects::Checked(_) => unknown_chain,
                    };
                    match self.resolve_invocation_effects(effects, instance.as_deref()) {
                        Ok(effects) => {
                            match flow {
                                InvocationFlow::EffectsOnly | InvocationFlow::Rejected => {},
                                InvocationFlow::AwaitingReturn => self.error(check.span, "pending invocation has no checked return flow", DiagnosticCode::CheckLocalInference),
                                InvocationFlow::Return(return_ty) => match self.invocation_may_error(&effects) {
                                    Ok(false) => {},
                                    Ok(true) => match self.type_constraints.resolve(&return_ty) {
                                        Ok(return_ty) => match self.invocation_return_has_checked_result_envelope(instance.as_deref(), &return_ty) {
                                            Err(_) => self.error(check.span, "invocation return flow escaped its inference scope", DiagnosticCode::CheckLocalInference),
                                            Ok(true) => {},
                                            Ok(false) if !matches!(return_ty.unvalidated(), Type::Inference(_) | Type::Unknown | Type::Invalid) => self.record_invocation_propagation(&super::InvocationEffects::Checked(effects.clone()), &return_ty, check.span, None),
                                            Ok(false) => self.error(check.span, "invocation return flow has no settled result shape", DiagnosticCode::CheckLocalInference),
                                        },
                                        Err(_) => self.error(check.span, "invocation return flow escaped its inference scope", DiagnosticCode::CheckLocalInference),
                                    },
                                    Err(_) => self.error(check.span, "invocation error flow escaped its effect scope", DiagnosticCode::CheckLocalInference),
                                },
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
            if invocation_diagnosed || generated.iter().any(|diagnostic| {
                diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)
                    && diagnostic.labels.iter().any(|label| label.span == deferred_span)
            }) {
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

    pub(super) fn record_required_effect(&mut self, effect: Effect, span: Span) {
        if (self.record_effects || self.collecting_effects)
            && let Err(error) = self.record_finite_effect(effect, self.boundary.retry_attempt_depth > 0, "required operation", span)
        { self.error(span, &format!("required operation effects are not scoped: {error:?}"), DiagnosticCode::CheckLocalInference); }
    }

    pub(super) fn require_effect(&mut self, effect: Effect, span: Span, subject: &str) {
        if effect == Effect::Error {
            self.propagating_error_spans.insert(span);
        }
        let captured = self.boundary.retry_attempt_depth > 0;
        if (self.record_effects || self.collecting_effects)
            && let Err(error) = self.record_finite_effect(effect.clone(), captured, subject, span)
        { self.error(span, &format!("required operation effects are not scoped: {error:?}"), DiagnosticCode::CheckLocalInference); }
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
        match self.effect_summaries.get(&id) {
            Some(summary) => super::DeclarationEffects::Checked(summary.effects()),
            None => super::DeclarationEffects::Pending(id),
        }
    }

    pub(super) fn invocation_declaration_effects(
        &self,
        declaration: Option<EffectDeclarationId>,
    ) -> super::DeclarationEffects {
        if self.collecting_effects
            && let Some(owner) = declaration.filter(|owner| self.effect_graph.is_inferred(*owner))
        {
            return super::DeclarationEffects::Pending(owner);
        }
        self.declaration_effects(declaration)
    }

    pub(super) fn signature_declaration_effects(
        &self,
        declaration: Option<EffectDeclarationId>,
    ) -> super::DeclarationEffects {
        if self.collecting_effects
            && declaration.is_some_and(|owner| self.effect_graph.is_inferred(owner))
        {
            self.provisional_effects_read.set(true);
        }
        self.declaration_effects(declaration)
    }

    pub(super) fn instantiate_invocation_effects(
        &self,
        kind: crate::sema::types::CallableKind,
        declaration: Option<EffectDeclarationId>,
        effects: impl FnOnce() -> super::DeclarationEffects,
        instance: Option<&crate::sema::constraints::InstantiatedScheme>,
    ) -> Result<super::InvocationEffects, SchemeOperationError> {
        if kind == crate::sema::types::CallableKind::Pure {
            return Ok(super::InvocationEffects::Checked(crate::sema::types::CallableEffects::finite([])));
        }
        let effects = match effects() {
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
