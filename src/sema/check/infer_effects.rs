use super::{BTreeMap, Effect, Name, Span};

/// A declaring module distinguishes separately parsed arenas whose local spans
/// overlap. Bundled source spans remain usable as the tooling projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EffectDeclarationId {
    pub namespace: Option<Name>,
    pub body: Span,
}

/// Missing private clauses infer a contract; an explicit clause remains the
/// caller-visible upper bound even when its body needs fewer effects.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionEffectFact {
    pub effective: Option<Vec<Effect>>,
    pub required: Option<Vec<Effect>>,
    pub inferred: bool,
    pub inference_allowed: bool,
    pub unknown_chain: Vec<String>,
}

use super::{Checker, FunctionSig};
use crate::syntax::arena::{ArenaFunctionDef, ArenaProgram, BlockId};

pub(super) fn graph_effects(summary: crate::sema::inference::EffectSummary) -> Option<Vec<Effect>> {
    let crate::sema::inference::EffectSummary::Closed(bits) = summary else { return None; };
    let mut effects = Vec::new();
    for (bit, effect) in [
        (crate::sema::inference::EffectSet::ERROR, Effect::Error),
        (crate::sema::inference::EffectSet::FS, Effect::Fs),
        (crate::sema::inference::EffectSet::NET, Effect::Net),
        (crate::sema::inference::EffectSet::PROCESS, Effect::Process),
        (crate::sema::inference::EffectSet::ENV, Effect::Env),
        (crate::sema::inference::EffectSet::TIME, Effect::Time),
        (crate::sema::inference::EffectSet::IO, Effect::Io),
    ] { if bits.0 & bit.0 != 0 { effects.push(effect); } }
    effects.sort_by_key(Effect::as_str);
    Some(effects)
}

impl Checker {
    pub(super) fn function_effect_facts_from_solved(&self, solved: &super::SolvedTypes, program: &ArenaProgram) -> BTreeMap<EffectDeclarationId, FunctionEffectFact> {
        let mut facts: BTreeMap<_, _> = self.diagnostic_effect_facts.iter().map(|(identity, fact)| {
            let def = program.arena.function_def(identity.declaration);
            (EffectDeclarationId { namespace: identity.namespace, body: program.arena.span(program.arena.block(def.body).span) }, fact.clone())
        }).collect();
        facts.extend(solved.declarations.iter().filter_map(|(identity, declaration)| {
            if declaration.kind == crate::sema::inference::CallableKind::Pure { return None; }
            let def = program.arena.function_def(identity.declaration);
            let inference_allowed = !def.test_declaration && def.name != "main";
            let effective = graph_effects(declaration.effective_effects);
            let required = graph_effects(declaration.required_effects);
            Some((EffectDeclarationId { namespace: identity.namespace, body: program.arena.span(program.arena.block(def.body).span) }, FunctionEffectFact {
                effective, required: required.clone(), inferred: inference_allowed && def.effects.is_none(), inference_allowed,
                unknown_chain: if required.is_none() { vec![def.name.to_string()] } else { Vec::new() },
            }))
        }));
        facts
    }

    /// Effect diagnostics remain available when unrelated typing facts cannot
    /// publish. Only a calculated closed execution summary justifies an edit;
    /// an unresolved or unrestricted dependency keeps the summary unknown.
    pub(super) fn retain_diagnostic_effect_facts(&mut self) {
        let mut state = self.generic.borrow_mut();
        let roots: Vec<_> = state.pending.values().map(|declaration| declaration.required_effects).collect();
        let complete = state.facts.graph.seal_derived_effects(&roots).is_ok();
        for (identity, fact) in &mut self.diagnostic_effect_facts {
            let Some(declaration) = state.pending.get(identity) else { continue; };
            fact.required = if complete { state.facts.graph.resolved_effect_summary(declaration.required_effects).ok().and_then(graph_effects) } else { None };
            if fact.inferred {
                fact.effective = fact.required.clone();
            }
            if fact.required.is_some() { fact.unknown_chain.clear(); }
        }
    }

    pub(super) fn callable_effects_from_solved(&self, solved: &super::SolvedTypes, program: &ArenaProgram) -> super::FxHashMap<String, Option<Vec<Effect>>> {
        let facts = self.function_effect_facts_from_solved(solved, program);
        let mut effects = super::FxHashMap::default();
        for (name, sig) in self.procs.iter().chain(self.streams.iter()).map(|(name, sig)| (name.to_string(), sig))
            .chain(self.qualified_procs.iter().chain(self.qualified_streams.iter()).map(|(name, sig)| (name.to_string(), sig))) {
            let summary = sig.effect_declaration.and_then(|identity| facts.get(&identity)).map(|fact| fact.effective.clone()).unwrap_or_else(|| sig.effects.clone());
            effects.insert(name, summary);
        }
        effects
    }
}

impl Checker {
    pub(super) fn effect_declaration_id(&self, program: &ArenaProgram, body: BlockId) -> EffectDeclarationId {
        EffectDeclarationId { namespace: self.current_namespace, body: program.arena.span(program.arena.block(body).span) }
    }

    pub(super) fn effective_function_effects(&self, program: &ArenaProgram, def: &ArenaFunctionDef) -> Option<Vec<Effect>> {
        if let Some(effects) = def.effects { return Some(program.arena.effects(effects).collect()); }
        let identity = self.graph_declaration(def.body)?;
        let state = self.generic.borrow();
        let pending = state.pending.get(&identity)?;
        let crate::sema::inference::TypeNode::Arrow(arrow) = state.facts.graph.node(pending.signature).ok()? else { return None; };
        graph_effects(state.facts.graph.resolved_effect_summary(arrow.effects).ok()?)
    }

    pub(super) fn record_required_effect(&mut self, effect: Effect) {
        let span = self.effect_owner.map(|owner| owner.body);
        if let Some(span) = span { self.record_graph_effect_requirement(&[effect], span); }
    }

    pub(super) fn record_graph_effect_summary(&mut self, effects: crate::sema::inference::EffectSummary, span: Span) {
        if !self.graph_generation { return; }
        if self.current_generic.is_none() && self.stage_callback_effects.is_none() { return; }
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let (raw, expected) = if let Some(sink) = self.stage_callback_effects { (sink, sink) } else {
                let owner = self.current_generic.ok_or(crate::sema::inference::InferenceError::InvalidScheme)?;
                let signature = state.pending[&owner].signature;
                let crate::sema::inference::TypeNode::Arrow(arrow) = state.facts.graph.node(state.facts.graph.resolved(signature)?)? else {
                    return Err(crate::sema::inference::InferenceError::InvalidScheme);
                };
                let declaration = &state.pending[&owner];
                (declaration.required_effects, declaration.producer_effects.map(|producer| if self.in_defer_block { producer.close } else { producer.pull }).unwrap_or(arrow.effects))
            };
            let reason = state.facts.graph.reason(span, None)?;
            if raw != expected {
                if self.retry_attempt_depth > 0 {
                    let requirement = state.facts.graph.include_effects_masked(effects, raw, crate::sema::inference::EffectSet::ERROR, reason)?;
                    if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
                } else { state.facts.graph.include_effects(effects, raw, reason)?; }
            }
            if self.retry_attempt_depth > 0 {
                let requirement = state.facts.graph.include_effects_masked(effects, expected, crate::sema::inference::EffectSet::ERROR, reason)?;
                if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
                Ok(())
            } else { state.facts.graph.include_effects(effects, expected, reason) }
        })();
        if let Err(error) = outcome {
            let (known, minimum) = {
                use crate::sema::inference::{EffectSummary, EffectSet};
                let state = self.generic.borrow();
                let graph = &state.facts.graph;
                match graph.resolved_effect_summary(effects) {
                    Ok(EffectSummary::Closed(bits)) => (Some(bits), false),
                    Ok(EffectSummary::Variable(id)) => (graph.effect_value(id).ok().filter(|bits| *bits != EffectSet::EMPTY), true),
                    Ok(EffectSummary::Rigid { scope, index }) => (graph.scheme(scope).ok().and_then(|scheme| scheme.effect_quantifiers.get(index as usize)).map(|quantifier| quantifier.lower).filter(|bits| *bits != EffectSet::EMPTY), true),
                    _ => (None, false),
                }
            };
            let required = known.and_then(|bits| graph_effects(crate::sema::inference::EffectSummary::Closed(bits)))
                .map(|effects| effects.iter().map(Effect::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| "a retained latent or unrestricted effect".to_string());
            let quantity = if minimum && known.is_some() { "at least " } else { "" };
            self.error(span, &format!("callable requires {quantity}[{required}], which exceeds its checked effect boundary: {error:?}"), "check.effect-violation");
        }
    }

    fn record_graph_effect_requirement(&mut self, effects: &[Effect], span: Span) {
        let retained: Vec<_> = effects.iter().filter(|effect| !(self.retry_attempt_depth > 0 && **effect == Effect::Error)).cloned().collect();
        self.record_graph_effect_summary(crate::sema::inference::EffectSummary::Closed(super::generic::graph_effect_bits(&retained)), span);
    }

    pub(super) fn require_effect(&mut self, effect: Effect, span: Span, subject: &str) {
        let captured = self.retry_attempt_depth > 0;
        if captured && effect == Effect::Error { return; }
        self.record_graph_effect_requirement(&[effect.clone()], span);
        if let Some(caller) = &self.current_effects
            && !Self::effects_covers(caller, &effect)
        {
            self.error(span, &format!("{subject} requires the `{}` effect", effect.as_str()), "check.effect-violation");
        }
    }

    pub(super) fn check_resolved_callable_effects(&mut self, sig: &FunctionSig, name: &str, span: Span) {
        if !sig.inferred_effects { self.record_effect_contract(&sig.effects, name); }
        if let Some(caller) = self.current_effects.clone() {
            if sig.effects.is_none() && sig.inferred_effects {
                let mut chain = vec![name.to_string()];
                self.error(span, &format!("proc `{name}` has an unknown effect summary: {}; call a named callable with a checked effect contract instead of an opaque or unrestricted dependency", chain.join(" -> ")), "check.effect-violation");
            } else {
                self.check_callee_effects(&caller, &sig.effects, name, span);
            }
        }
    }

    pub(super) fn record_effect_contract(&mut self, effects: &Option<Vec<Effect>>, name: &str) {
        if let Some(span) = self.effect_owner.map(|owner| owner.body) {
            match effects {
                Some(effects) => self.record_graph_effect_requirement(effects, span),
                None => self.record_graph_effect_summary(crate::sema::inference::EffectSummary::Unknown, span),
            }
        }

    }

    pub(super) fn check_opaque_callable_effects(&mut self, name: &str, span: Span) {
        // Pure bodies retain an empty graph budget without a legacy effect owner.
        self.record_graph_effect_summary(crate::sema::inference::EffectSummary::Unknown, span);
        if let Some(caller) = self.current_effects.clone() {
            self.check_callee_effects(&caller, &None, name, span);
        } else if self.in_pure && !self.graph_generation {
            self.check_callee_effects(&[], &None, name, span);
        }
    }
}
