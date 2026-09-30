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
        if self.known.contains(&effect) { return false; }
        self.known.push(effect);
        self.known.sort_by_key(Effect::as_str);
        true
    }

    fn unknown(&mut self, chain: Vec<String>) -> bool {
        if chain.is_empty() { return false; }
        if self.unknown_chain.is_empty()
            || (chain.len(), &chain) < (self.unknown_chain.len(), &self.unknown_chain)
        {
            self.unknown_chain = chain;
            return true;
        }
        false
    }
}

#[derive(Clone, Debug)]
struct EffectEdge {
    target: EffectDeclarationId,
    captures_error: bool,
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
    pub fn declare(&mut self, id: EffectDeclarationId, name: String, inference_allowed: bool, declared: Option<Vec<Effect>>) {
        self.nodes.entry(id).or_insert_with(|| EffectNode {
            name, inferred: inference_allowed && declared.is_none(), inference_allowed, declared, direct: EffectSummary::default(), edges: Vec::new(),
        });
    }

    pub fn is_inferred(&self, id: EffectDeclarationId) -> bool {
        self.nodes.get(&id).is_some_and(|node| node.inferred)
    }

    pub fn require(&mut self, owner: Option<EffectDeclarationId>, effect: Effect, captures_error: bool) {
        if captures_error && effect == Effect::Error { return; }
        if let Some(node) = owner.and_then(|owner| self.nodes.get_mut(&owner)) {
            node.direct.add(effect);
        }
    }

    pub fn unknown(&mut self, owner: Option<EffectDeclarationId>, name: String) {
        if let Some(node) = owner.and_then(|owner| self.nodes.get_mut(&owner)) {
            node.direct.unknown(vec![name]);
        }
    }

    pub fn call(&mut self, owner: Option<EffectDeclarationId>, target: EffectDeclarationId, captures_error: bool) {
        if let Some(node) = owner.and_then(|owner| self.nodes.get_mut(&owner))
            && !node.edges.iter().any(|edge| edge.target == target && edge.captures_error == captures_error)
        {
            node.edges.push(EffectEdge { target, captures_error });
        }
    }

    /// Effect sets only grow within the finite domain. Unknown witnesses choose
    /// the shortest stable chain, so recursive calls cannot grow diagnostic paths.
    pub fn solve(&self) -> BTreeMap<EffectDeclarationId, EffectSummary> {
        let mut summaries = self.nodes.iter().map(|(&id, node)| (id, node.direct.clone())).collect::<BTreeMap<_, _>>();
        loop {
            let mut changed = false;
            for (&id, node) in &self.nodes {
                for edge in &node.edges {
                    let Some(target) = summaries.get(&edge.target).cloned() else { continue; };
                    let summary = summaries.get_mut(&id).expect("every declaration has an initial summary");
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
            if !changed { return summaries; }
        }
    }

    pub fn facts(&self, summaries: &BTreeMap<EffectDeclarationId, EffectSummary>) -> BTreeMap<EffectDeclarationId, FunctionEffectFact> {
        self.nodes.iter().map(|(id, node)| {
            let summary = &summaries[id];
            (*id, FunctionEffectFact {
                effective: if node.inferred { summary.effects() } else { node.declared.clone() },
                required: summary.effects(),
                inferred: node.inferred,
                inference_allowed: node.inference_allowed,
                unknown_chain: summary.unknown_chain.clone(),
            })
        }).collect()
    }
}

use super::{Checker, FunctionSig};
use crate::syntax::arena::{ArenaFunctionDef, ArenaProgram, ArenaStmtKind, BlockId, StmtId};

impl Checker {
    pub(super) fn check_separate_modules(&mut self, modules: &[(&str, &str, &ArenaProgram, &str)]) {
        for (key, name, arena, source) in modules {
            self.prepare_effect_declarations(arena, Some(Name::intern(name)));
            self.diagnostics.extend(Self::prepare_regex_literals(arena));
            let module_program = std::sync::Arc::new((*arena).clone());
            let module = crate::syntax::arena::ArenaUserModule {
                key: (*key).to_string(), name: Name::intern(name), statements: arena.statements, internal: false,
            };
            let sig = self.check_user_module_arena(arena, module_program, source, &module);
            self.user_modules.insert((*key).to_string(), sig);
        }
    }

    pub(super) fn effect_declaration_id(&self, program: &ArenaProgram, body: BlockId) -> EffectDeclarationId {
        EffectDeclarationId { namespace: self.current_namespace, body: program.arena.span(program.arena.block(body).span) }
    }

    pub(super) fn prepare_effect_declarations(&mut self, program: &ArenaProgram, namespace: Option<Name>) {
        let mut exported = super::FxHashSet::default();
        let mut namespaces = super::FxHashMap::default();
        for module in &program.modules {
            for statement in program.arena.stmt_ids(module.statements) {
                namespaces.insert(program.arena.stmt(statement).span.source_id, module.name);
            }
        }
        for raw in 0..program.arena.stmt_tags.len() {
            if let ArenaStmtKind::Export(inner) = program.arena.stmt(StmtId::from_index(raw)).kind
                && let ArenaStmtKind::ProcDef(definition) = program.arena.stmt(inner).kind
            { exported.insert(definition); }
        }
        for raw in 0..program.arena.stmt_tags.len() {
            let statement = program.arena.stmt(StmtId::from_index(raw));
            let (definition, ordinary) = match statement.kind {
                ArenaStmtKind::ProcDef(definition) => (definition, true),
                ArenaStmtKind::StreamDef(definition) | ArenaStmtKind::CliMain(definition) => (definition, false),
                _ => continue,
            };
            {
                let def = program.arena.function_def(definition);
                let id = EffectDeclarationId {
                    namespace: namespaces.get(&statement.span.source_id).copied().or(namespace),
                    body: program.arena.span(program.arena.block(def.body).span),
                };
                let declared = def.effects.map(|effects| program.arena.effects(effects).collect());
                let inference_allowed = ordinary && !def.test_declaration && !exported.contains(&definition) && def.name != "main";
                let name = id.namespace.map(|namespace| format!("{namespace}.{}", def.name)).unwrap_or_else(|| def.name.to_string());
                self.effect_graph.declare(id, name, inference_allowed, declared);
            }
        }
    }

    pub(super) fn effective_function_effects(&self, program: &ArenaProgram, def: &ArenaFunctionDef) -> Option<Vec<Effect>> {
        if let Some(effects) = def.effects { return Some(program.arena.effects(effects).collect()); }
        let id = self.effect_declaration_id(program, def.body);
        if !self.effect_graph.is_inferred(id) { return None; }
        if self.collecting_effects { return Some(Vec::new()); }
        self.effect_summaries.get(&id).and_then(EffectSummary::effects)
    }

    pub(super) fn record_required_effect(&mut self, effect: Effect) {
        if self.collecting_effects { self.effect_graph.require(self.effect_owner, effect, self.retry_attempt_depth > 0); }
    }

    pub(super) fn require_effect(&mut self, effect: Effect, span: Span, subject: &str) {
        let captured = self.retry_attempt_depth > 0;
        if self.collecting_effects { self.effect_graph.require(self.effect_owner, effect.clone(), captured); }
        if captured && effect == Effect::Error { return; }
        if let Some(caller) = &self.current_effects
            && !Self::effects_covers(caller, &effect)
        {
            self.error(span, &format!("{subject} requires the `{}` effect", effect.as_str()), "check.effect-violation");
        }
    }

    pub(super) fn check_resolved_callable_effects(&mut self, sig: &FunctionSig, name: &str, span: Span) {
        if self.collecting_effects {
            if sig.inferred_effects && let Some(declaration) = sig.effect_declaration {
                self.effect_graph.call(self.effect_owner, declaration, self.retry_attempt_depth > 0);
            } else {
                self.record_effect_contract(&sig.effects, name);
            }
        }
        if let Some(caller) = self.current_effects.clone() {
            if sig.effects.is_none() && sig.inferred_effects {
                let mut chain = vec![name.to_string()];
                if let Some(summary) = sig.effect_declaration.and_then(|declaration| self.effect_summaries.get(&declaration)) { chain.extend(summary.unknown_chain.clone()); }
                self.error(span, &format!("proc `{name}` has an unknown effect summary: {}; call a named callable with a checked effect contract instead of an opaque or unrestricted dependency", chain.join(" -> ")), "check.effect-violation");
            } else {
                self.check_callee_effects(&caller, &sig.effects, name, span);
            }
        }
    }

    pub(super) fn record_effect_contract(&mut self, effects: &Option<Vec<Effect>>, name: &str) {
        if !self.collecting_effects { return; }
        match effects {
            Some(effects) => for effect in effects {
                self.effect_graph.require(self.effect_owner, effect.clone(), self.retry_attempt_depth > 0);
            },
            None => self.effect_graph.unknown(self.effect_owner, name.to_string()),
        }
    }

    pub(super) fn check_opaque_callable_effects(&mut self, name: &str, span: Span) {
        self.record_effect_contract(&None, name);
        if let Some(caller) = self.current_effects.clone() { self.check_callee_effects(&caller, &None, name, span); }
    }
}
