use super::{CheckBundleView, FxHashMap, FxHashSet, Name, Span, assignment_facts::AssignmentFacts};
use crate::source::SourceId;
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaChild, ArenaCompQualifier, ArenaExprKind, ArenaPatternKind,
    ArenaProgram, ArenaRange, ArenaRecordFieldKind, ArenaStmtKind, ArenaSugarOperand, ArenaTypeDefBody,
    BindingTargetId, BlockId, ExprId, FunctionDefId, PatternId, SpanId, StmtId, SugarView, UseStmtId,
};

/// A declaration's identity in its arena. Source offsets never determine scope,
/// and identities remain the same when a workspace selects another root.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BindingId(BindingOrigin);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum BindingOrigin {
    Target(BindingTargetId),
    Parameter(FunctionDefId, u32),
    BlockParameter(BlockId, u32),
    Pattern(PatternId),
    PatternAlias(PatternId),
    PatternAlternative(PatternId, Name),
    With(SpanId),
    Import(UseStmtId),
    Item(BlockId),
    Args,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingKind {
    Constant(StmtId),
    Runtime,
    Module(Name),
}

#[derive(Clone, Debug)]
pub struct BindingDeclaration {
    pub name: Name,
    pub span: Span,
    pub namespace: Option<Name>,
    pub kind: BindingKind,
    pub global: bool,
    pub exported: bool,
}

/// Lexical decisions shared by checking, constant evaluation, and slot binding.
/// Initializers resolve before their runtime declaration becomes visible;
/// constants are visible throughout the block that declares them.
#[derive(Clone, Debug, Default)]
pub struct ResolvedBindings {
    pub declarations: FxHashMap<BindingId, BindingDeclaration>,
    pub declaration_sites: FxHashMap<(Span, Name), BindingId>,
    pub expressions: FxHashMap<ExprId, BindingId>,
    pub tails: FxHashMap<StmtId, BindingId>,
    pub value_sites: FxHashMap<(Span, Name), BindingId>,
    pub expression_namespaces: FxHashMap<ExprId, Option<Name>>,
}

impl BindingId {
    pub(crate) fn args() -> Self { Self(BindingOrigin::Args) }
}

impl ResolvedBindings {
    pub fn collect(program: &ArenaProgram) -> Self {
        Self::collect_bundle(&CheckBundleView::whole_program(program))
    }

    pub(super) fn collect_bundle(view: &CheckBundleView<'_>) -> Self {
        Self::collect_bundle_with_assignment_facts(view).0
    }

    pub(super) fn collect_bundle_with_assignment_facts(
        view: &CheckBundleView<'_>,
    ) -> (Self, AssignmentFacts) {
        let program = view.program;
        let mut resolver = ScopeChecker {
            program,
            facts: Self::default(),
            assignment_facts: AssignmentFacts::default(),
            scopes: Vec::new(),
            namespace: None,
            globals: FxHashMap::default(),
            zero_variants: FxHashSet::default(),
            assignment_owner: None,
            assignment_capture: false,
            assignment_raw_blocks: Vec::new(),
            assignment_raw_statements: Vec::new(),
        };
        resolver.predeclare(view.statement_ids(), true);
        for module in view.modules.iter().copied() {
            resolver.namespace = Some(module.name);
            resolver.predeclare(program.module_statements(module), true);
        }
        // Imports can name constants in a module that is visited later.
        for module in view.modules.iter().copied() {
            resolver.namespace = Some(module.name);
            resolver.file(
                program.module_statements(module).collect(),
                super::range_source_id(program, module.statements)
                    .map(|source_id| (source_id, module.statements)),
            );
        }
        resolver.namespace = None;
        resolver.file(
            view.statement_ids().collect(),
            view.assignment_root_owner,
        );
        (resolver.facts, resolver.assignment_facts)
    }

    pub(super) fn merge_visible(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for (&id, declaration) in &other.declarations {
            if let Some(existing) = merged.declarations.get(&id) {
                let same_args = id == BindingId::args()
                    && existing.name == declaration.name
                    && existing.span == declaration.span
                    && existing.kind == declaration.kind
                    && existing.global == declaration.global
                    && existing.exported == declaration.exported;
                assert!(
                    same_args || existing.name == declaration.name
                        && existing.span == declaration.span
                        && existing.namespace == declaration.namespace
                        && existing.kind == declaration.kind
                        && existing.global == declaration.global
                        && existing.exported == declaration.exported,
                    "binding identity {id:?} has conflicting module declarations"
                );
            } else {
                merged.declarations.insert(id, declaration.clone());
            }
        }
        merge_binding_map(&mut merged.declaration_sites, &other.declaration_sites);
        merge_binding_map(&mut merged.expressions, &other.expressions);
        merge_binding_map(&mut merged.tails, &other.tails);
        merge_binding_map(&mut merged.value_sites, &other.value_sites);
        merge_binding_map(
            &mut merged.expression_namespaces,
            &other.expression_namespaces,
        );
        merged
    }

    pub(super) fn resolve_module_fields(
        &mut self,
        program: &ArenaProgram,
        declarations: &FxHashMap<BindingId, BindingDeclaration>,
    ) {
        let globals = declarations
            .iter()
            .filter_map(|(&id, declaration)| {
                declaration
                    .global
                    .then_some(((declaration.namespace, declaration.name), id))
            })
            .collect::<FxHashMap<_, _>>();
        for &expression in self.expression_namespaces.keys() {
            let ArenaExprKind::Field { base, name } = program.arena.expr(expression).kind else {
                continue;
            };
            let Some(binding) = self.expressions.get(&base).copied() else {
                continue;
            };
            let Some(BindingDeclaration {
                kind: BindingKind::Module(namespace),
                ..
            }) = declarations.get(&binding)
            else {
                continue;
            };
            if let Some(&target) = globals.get(&(Some(*namespace), name)) {
                if let Some(existing) = self.expressions.get(&expression) {
                    assert_eq!(
                        *existing, target,
                        "qualified module field resolved to conflicting declarations"
                    );
                } else {
                    self.expressions.insert(expression, target);
                }
            }
        }
    }

    /// Only physical targets that lexical checking declared have identities.
    pub fn target(&self, target: BindingTargetId) -> Option<BindingId> {
        let id = BindingId(BindingOrigin::Target(target));
        self.declarations.contains_key(&id).then_some(id)
    }

    pub fn at(&self, span: Span, name: Name) -> Option<BindingId> {
        self.value_sites.get(&(span, name)).copied()
    }

    pub fn constant(&self, id: BindingId) -> Option<StmtId> {
        match self.declarations[&id].kind {
            BindingKind::Constant(statement) => Some(statement),
            BindingKind::Runtime | BindingKind::Module(_) => None,
        }
    }
}

fn merge_binding_map<K, V>(target: &mut FxHashMap<K, V>, source: &FxHashMap<K, V>)
where
    K: Copy + Eq + std::hash::Hash + std::fmt::Debug,
    V: Clone + Eq + std::fmt::Debug,
{
    for (&key, value) in source {
        if let Some(existing) = target.get(&key) {
            assert_eq!(
                existing, value,
                "binding fact {key:?} has conflicting owner records"
            );
        } else {
            target.insert(key, value.clone());
        }
    }
}

struct ScopeChecker<'a> {
    program: &'a ArenaProgram,
    facts: ResolvedBindings,
    assignment_facts: AssignmentFacts,
    scopes: Vec<FxHashMap<Name, BindingId>>,
    namespace: Option<Name>,
    globals: FxHashMap<(Option<Name>, Name), BindingId>,
    zero_variants: FxHashSet<(Option<Name>, Name)>,
    assignment_owner: Option<(SourceId, ArenaRange)>,
    assignment_capture: bool,
    assignment_raw_blocks: Vec<BlockId>,
    assignment_raw_statements: Vec<StmtId>,
}

impl ScopeChecker<'_> {
    fn declare(&mut self, origin: BindingOrigin, name: Name, span: Span, kind: BindingKind,
        global: bool, exported: bool) -> BindingId {
        let id = BindingId(origin);
        self.facts.declarations.insert(id, BindingDeclaration {
            name, span, namespace: self.namespace, kind, global, exported,
        });
        self.facts.declaration_sites.insert((span, name), id);
        if global { self.globals.insert((self.namespace, name), id); }
        id
    }

    fn predeclare(&mut self, statements: impl IntoIterator<Item = StmtId>, global: bool) {
        for statement in statements {
            let mut inner = statement;
            while let ArenaStmtKind::Export(exported) = self.program.arena.stmt(inner).kind {
                inner = exported;
            }
            if let ArenaStmtKind::TypeDef(definition) = self.program.arena.stmt(inner).kind
                && let ArenaTypeDefBody::TagUnion(variants) = self.program.arena.type_def(definition).body
            {
                for variant in self.program.arena.tag_variants(variants) {
                    if variant.fields.is_empty() { self.zero_variants.insert((self.namespace, variant.name)); }
                }
            }
            if global && let ArenaStmtKind::Use(import) = self.program.arena.stmt(inner).kind {
                self.import(import, self.program.arena.stmt(inner).span, true);
            }
            if let ArenaStmtKind::Const { target, .. } = self.program.arena.stmt(inner).kind {
                self.target(target, self.program.arena.stmt(inner).span,
                    BindingKind::Constant(inner), global, inner != statement);
            }
        }
    }

    fn file(
        &mut self,
        statements: Vec<StmtId>,
        assignment_owner: Option<(SourceId, ArenaRange)>,
    ) {
        let previous_owner = self.assignment_owner;
        let previous_capture = self.assignment_capture;
        let block_anchor_count = self.assignment_raw_blocks.len();
        let statement_anchor_count = self.assignment_raw_statements.len();
        self.assignment_owner = assignment_owner;
        self.assignment_capture = assignment_owner.is_some();
        if let Some((source_id, statements)) = assignment_owner {
            self.assignment_facts.initialize_owner(source_id, statements);
        }
        self.scopes.push(self.globals.iter().filter_map(|(&(owner, name), &id)|
            (owner == self.namespace).then_some((name, id))).collect());
        let args = Name::intern("args");
        let id = self.declare(BindingOrigin::Args, args,
            Span::at(crate::source::SourceId::new(0), 0), BindingKind::Runtime, false, false);
        self.bind(args, id);
        for statement in statements { self.stmt(statement); }
        self.scopes.pop();
        self.assignment_raw_blocks.truncate(block_anchor_count);
        self.assignment_raw_statements.truncate(statement_anchor_count);
        self.assignment_capture = previous_capture;
        self.assignment_owner = previous_owner;
    }

    fn bind(&mut self, name: Name, id: BindingId) {
        self.scopes.last_mut().expect("resolution has a lexical scope").insert(name, id);
    }

    fn lookup(&self, name: Name) -> Option<BindingId> {
        self.scopes.iter().rev().find_map(|scope| scope.get(&name).copied())
    }

    fn target(&mut self, target: BindingTargetId, span: Span, kind: BindingKind,
        global: bool, exported: bool) {
        match self.program.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => {
                if name == "_" { return; }
                let id = self.declare(BindingOrigin::Target(target), name, span, kind, global, exported);
                if !self.scopes.is_empty() { self.bind(name, id); }
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in self.program.arena.destructure_fields(fields) {
                    self.target(field.target, self.program.arena.span(field.span), kind, global, exported);
                }
            }
        }
    }

    fn block(&mut self, block: BlockId) {
        self.block_with_item(block, false);
    }

    fn block_with_item(&mut self, block: BlockId, callback: bool) {
        let previous_capture = self.assignment_capture;
        if self.assignment_raw_blocks.contains(&block) {
            self.assignment_capture = true;
        }
        self.scopes.push(FxHashMap::default());
        let body = self.program.arena.block(block);
        for (index, parameter) in self.program.arena.block_params(body.params).iter().enumerate() {
            let id = self.declare(BindingOrigin::BlockParameter(block, index as u32), parameter.name,
                self.program.arena.span(parameter.span), BindingKind::Runtime, false, false);
            self.bind(parameter.name, id);
        }
        if callback && self.program.arena.block_params(body.params).is_empty() {
            let name = super::item_binding();
            let id = self.declare(BindingOrigin::Item(block), name,
                self.program.arena.span(body.span), BindingKind::Runtime, false, false);
            self.bind(name, id);
        }
        let statements: Vec<_> = self.program.arena.stmt_ids(body.statements).collect();
        self.predeclare(statements.iter().copied(), false);
        for statement in statements { self.stmt(statement); }
        self.scopes.pop();
        self.assignment_capture = previous_capture;
    }

    fn condition_bindings(&mut self, condition: ExprId) {
        if let ArenaExprKind::PatternCondition { arms, .. } = self.program.arena.expr(condition).kind {
            for arm in self.program.arena.match_expr_arms(arms) { self.pattern(arm.pattern); }
        }
    }

    fn pattern(&mut self, pattern: PatternId) { self.pattern_capture(pattern, None); }
    fn pattern_capture(&mut self, pattern: PatternId, alternative: Option<PatternId>) {
        let node = self.program.arena.pattern(pattern);
        let alternative = alternative.or_else(|| matches!(node.kind, ArenaPatternKind::Alternation(_)).then_some(pattern));
        let span = self.program.arena.span(node.span);
        let binding = match node.kind {
            ArenaPatternKind::Binding(name) if self.zero_variants.contains(&(self.namespace, name))
                || name.as_str().starts_with(|first: char| first.is_ascii_uppercase()) => None,
            ArenaPatternKind::Binding(name) | ArenaPatternKind::TestName { name, .. } => Some(name),
            ArenaPatternKind::Type { binding, .. } | ArenaPatternKind::TextHole { binding, .. } => binding,
            _ => None,
        };
        if let Some(name) = binding {
            let origin = alternative.map_or(BindingOrigin::Pattern(pattern), |parent| BindingOrigin::PatternAlternative(parent, name));
            let id = self.declare(origin, name, span,
                BindingKind::Runtime, false, false);
            self.bind(name, id);
        }
        if let ArenaPatternKind::Alias { name, name_span, .. } = node.kind {
            let origin = alternative.map_or(BindingOrigin::PatternAlias(pattern), |parent| BindingOrigin::PatternAlternative(parent, name));
            let id = self.declare(origin, name,
                self.program.arena.span(name_span), BindingKind::Runtime, false, false);
            self.bind(name, id);
        }
        let mut children = Vec::new();
        self.program.arena.for_each_pattern_child(pattern, |child| children.push(child));
        for child in children {
            if let ArenaChild::Pattern(child) = child { self.pattern_capture(child, alternative); }
            else { self.child(child); }
        }
        if alternative == Some(pattern) {
            let captures = self.scopes.last().expect("pattern has a scope").clone();
            for (name, id) in captures {
                if matches!(id.0, BindingOrigin::PatternAlternative(parent, _) if parent == pattern) {
                    self.facts.declaration_sites.insert((span, name), id);
                }
            }
        }
    }

    fn with_bindings(&mut self, bindings: ArenaRange) {
        for binding in self.program.arena.with_bindings(bindings) {
            self.expr(binding.initializer);
            let id = self.declare(BindingOrigin::With(binding.span), binding.name,
                self.program.arena.span(binding.span), BindingKind::Runtime, false, false);
            self.bind(binding.name, id);
        }
    }

    fn function(&mut self, function: FunctionDefId) {
        let def = self.program.arena.function_def(function);
        for parameter in self.program.arena.params(def.params) {
            if let Some(default) = parameter.default { self.expr(default); }
        }
        self.scopes.push(FxHashMap::default());
        for (index, parameter) in self.program.arena.params(def.params).iter().enumerate() {
            let id = self.declare(BindingOrigin::Parameter(function, index as u32), parameter.name,
                self.program.arena.span(parameter.span), BindingKind::Runtime, false, false);
            self.bind(parameter.name, id);
        }
        self.block(def.body);
        self.scopes.pop();
    }

    fn import(&mut self, import: UseStmtId, span: Span, global: bool) {
        let entry = self.program.arena.use_stmt(import);
        if entry.resolved.is_none() && entry.alias.is_none() {
            let mut path = self.program.arena.names(entry.path);
            if let Some(name) = path.next()
                && path.next().is_none()
                && super::api_spec().module(&name.as_str()).is_some()
            {
                return;
            }
        }
        if let Some(name) = entry.alias.or_else(|| self.program.arena.names(entry.path).last()) {
            let kind = entry.resolved.as_deref().map_or(BindingKind::Runtime,
                |key| BindingKind::Module(Name::intern(key)));
            let id = self.declare(BindingOrigin::Import(import), name, span, kind, global, false);
            if !self.scopes.is_empty() { self.bind(name, id); }
        }
    }

    fn stmt(&mut self, statement: StmtId) {
        let previous_capture = self.assignment_capture;
        if self.assignment_raw_statements.contains(&statement) {
            self.assignment_capture = true;
        }
        let node = self.program.arena.stmt(statement);
        match node.kind {
            ArenaStmtKind::Const { .. } => self.excluded_stmt_children(statement),
            ArenaStmtKind::Assign { target, .. } => {
                if self.assignment_capture
                    && let Some(name) = super::assignment_target_root_name(self.program, target)
                    && let Some((source_id, statements)) = self.assignment_owner
                {
                    self.assignment_facts.insert(source_id, statements, name);
                }
                let mut target = target;
                loop {
                    match self.program.arena.assign_target(target).kind {
                        ArenaAssignTargetKind::Name(name) => {
                            if let Some(id) = self.lookup(name) {
                                self.facts.value_sites.insert((node.span, name), id);
                            }
                            break;
                        }
                        ArenaAssignTargetKind::Field { base, .. }
                        | ArenaAssignTargetKind::Index { base, .. } => target = base,
                        ArenaAssignTargetKind::Env(_) => break,
                    }
                }
                self.stmt_children(statement);
            }
            ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
                self.excluded_stmt_children(statement);
                self.target(target, node.span, BindingKind::Runtime, self.scopes.len() == 1, false);
            }
            ArenaStmtKind::Guard { target, .. } => {
                self.stmt_children(statement);
                self.target(target, node.span, BindingKind::Runtime, self.scopes.len() == 1, false);
            }
            ArenaStmtKind::Use(import) => self.import(import, node.span, self.scopes.len() == 1),
            ArenaStmtKind::ProcDef(def) | ArenaStmtKind::PureDef(def)
            | ArenaStmtKind::StreamDef(def) | ArenaStmtKind::CliMain(def) => self.function(def),
            ArenaStmtKind::For { target, iter, block } => {
                self.expr(iter);
                self.scopes.push(FxHashMap::default());
                self.target(target, node.span, BindingKind::Runtime, false, false);
                self.block(block);
                self.scopes.pop();
            }
            ArenaStmtKind::With { bindings, body, else_block } => {
                self.scopes.push(FxHashMap::default());
                self.with_bindings(bindings);
                self.block(body);
                self.scopes.pop();
                self.block(else_block);
            }
            ArenaStmtKind::If { branches, else_block } => {
                for branch in self.program.arena.if_branches(branches) {
                    self.expr(branch.condition);
                    self.scopes.push(FxHashMap::default());
                    self.condition_bindings(branch.condition);
                    self.block(branch.block);
                    self.scopes.pop();
                }
                if let Some(block) = else_block { self.block(block); }
            }
            ArenaStmtKind::While { condition, block } => {
                self.expr(condition);
                self.scopes.push(FxHashMap::default());
                self.condition_bindings(condition);
                self.block(block);
                self.scopes.pop();
            }
            ArenaStmtKind::Match { value, arms } => {
                self.expr(value);
                for arm in self.program.arena.match_arms(arms) {
                    self.scopes.push(FxHashMap::default());
                    self.pattern(arm.pattern);
                    if let Some(guard) = arm.guard { self.expr(guard); }
                    self.block(arm.block);
                    self.scopes.pop();
                }
            }
            ArenaStmtKind::TailBareIdent(name) => {
                if let Some(id) = self.lookup(name) {
                    self.facts.tails.insert(statement, id);
                    self.facts.value_sites.insert((node.span, name), id);
                }
            }
            ArenaStmtKind::Export(_)
            | ArenaStmtKind::SignalHook(_)
            | ArenaStmtKind::Loop { .. } => self.stmt_children(statement),
            ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::Defer(_, _)
            | ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::Exit(_)
            | ArenaStmtKind::Command(_)
            | ArenaStmtKind::Expr(_) => self.excluded_stmt_children(statement),
            ArenaStmtKind::Sugar { operands, .. } => {
                let block_anchor_count = self.assignment_raw_blocks.len();
                let statement_anchor_count = self.assignment_raw_statements.len();
                if self.assignment_capture {
                    for operand in self.program.arena.sugar_operands(operands) {
                        match operand {
                            ArenaSugarOperand::Block(block) => self.assignment_raw_blocks.push(*block),
                            ArenaSugarOperand::Stmt(statement) => {
                                self.assignment_raw_statements.push(*statement);
                            }
                            _ => {}
                        }
                    }
                }
                let previous = std::mem::replace(&mut self.assignment_capture, false);
                self.stmt_children(statement);
                self.assignment_capture = previous;
                self.assignment_raw_blocks.truncate(block_anchor_count);
                self.assignment_raw_statements.truncate(statement_anchor_count);
            }
        }
        self.assignment_capture = previous_capture;
    }

    fn stmt_children(&mut self, statement: StmtId) {
        let mut children = Vec::new();
        self.program.arena.for_each_stmt_child(statement, SugarView::Core, |child| children.push(child));
        for child in children { self.child(child); }
    }

    fn excluded_stmt_children(&mut self, statement: StmtId) {
        let previous_capture = std::mem::replace(&mut self.assignment_capture, false);
        self.stmt_children(statement);
        self.assignment_capture = previous_capture;
    }

    fn expr(&mut self, expression: ExprId) {
        let previous_capture = std::mem::replace(&mut self.assignment_capture, false);
        self.expr_inner(expression);
        self.assignment_capture = previous_capture;
    }

    fn expr_inner(&mut self, expression: ExprId) {
        let node = self.program.arena.expr(expression);
        self.facts.expression_namespaces.insert(expression, self.namespace);
        match node.kind {
            ArenaExprKind::Ident(name) => {
                if let Some(id) = self.lookup(name) {
                    self.facts.expressions.insert(expression, id);
                    self.facts.value_sites.insert((node.span, name), id);
                }
            }
            ArenaExprKind::Item => {
                if let Some(id) = self.lookup(super::item_binding()) {
                    self.facts.expressions.insert(expression, id);
                }
            }
            ArenaExprKind::Binary { op: crate::syntax::node::BinaryOp::ResultFallback, left, right } => {
                self.expr(left);
                if let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(right).kind {
                    self.facts.expression_namespaces.insert(right, self.namespace);
                    self.block_with_item(block, true);
                } else { self.expr(right); }
            }
            ArenaExprKind::ResourceScope { bindings, block, .. } => {
                self.scopes.push(FxHashMap::default());
                self.with_bindings(bindings);
                self.block(block);
                self.scopes.pop();
            }
            ArenaExprKind::ListComp { expr, qualifiers } | ArenaExprKind::SetComp { expr, qualifiers } => {
                self.comprehension(qualifiers, &[expr]);
            }
            ArenaExprKind::MapComp { key, value, qualifiers } => {
                self.comprehension(qualifiers, &[key, value]);
            }
            ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms }
            | ArenaExprKind::PatternCondition { value, arms } => {
                self.expr(value);
                for arm in self.program.arena.match_expr_arms(arms) {
                    self.scopes.push(FxHashMap::default());
                    self.pattern(arm.pattern);
                    if let Some(guard) = arm.guard { self.expr(guard); }
                    self.expr(arm.value);
                    self.scopes.pop();
                }
            }
            ArenaExprKind::If { branches, else_value } => {
                for branch in self.program.arena.if_expr_branches(branches) {
                    self.expr(branch.condition);
                    self.scopes.push(FxHashMap::default());
                    self.condition_bindings(branch.condition);
                    self.expr(branch.value);
                    self.scopes.pop();
                }
                self.expr(else_value);
            }
            ArenaExprKind::Record(fields) => {
                for field in self.program.arena.record_fields(fields) {
                    if let ArenaRecordFieldKind::Shorthand { name, span } = field.kind
                        && let Some(id) = self.lookup(name)
                    {
                        self.facts.value_sites.insert((node.span, name), id);
                        self.facts.value_sites.insert((self.program.arena.span(span), name), id);
                    }
                }
                self.expr_children(expression);
            }
            ArenaExprKind::Null | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_) | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_) | ArenaExprKind::Str(_)
            | ArenaExprKind::PathStr(_) | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::FmtString(_) | ArenaExprKind::PathFmtString(_)
            | ArenaExprKind::Bytes(_) | ArenaExprKind::Regex(_)
            | ArenaExprKind::LastStatus | ArenaExprKind::List(_)
            | ArenaExprKind::Set(_) | ArenaExprKind::Unary { .. }
            | ArenaExprKind::ComparisonChain(_) | ArenaExprKind::Binary { .. }
            | ArenaExprKind::Call { .. } | ArenaExprKind::ValuePipelineCall { .. }
            | ArenaExprKind::Field { .. } | ArenaExprKind::NullSafeField { .. }
            | ArenaExprKind::Index { .. } | ArenaExprKind::Slice { .. }
            | ArenaExprKind::EnvString(_) | ArenaExprKind::EnvPathList
            | ArenaExprKind::Pipeline { .. } | ArenaExprKind::StructuredPipeline { .. }
            | ArenaExprKind::Run(_) | ArenaExprKind::Spawn(_)
            | ArenaExprKind::Wait(_) | ArenaExprKind::BuilderCall { .. }
            | ArenaExprKind::Try(_) | ArenaExprKind::Capture(_)
            | ArenaExprKind::Require { .. } | ArenaExprKind::Convert { .. }
            | ArenaExprKind::Collect { .. } | ArenaExprKind::Loop { .. }
            | ArenaExprKind::Retry { .. } | ArenaExprKind::ValueBlock(_)
            | ArenaExprKind::ErrorContext { .. } | ArenaExprKind::ContextScope { .. }
            | ArenaExprKind::TempDirScope { .. } => self.expr_children(expression),
        }
        if let ArenaExprKind::Field { base, name } = node.kind
            && let Some(base) = self.facts.expressions.get(&base)
            && let BindingKind::Module(owner) = self.facts.declarations[base].kind
            && let Some(&id) = self.globals.get(&(Some(owner), name))
        {
            self.facts.expressions.insert(expression, id);
        }
    }

    fn comprehension(&mut self, qualifiers: ArenaRange, values: &[ExprId]) {
        self.scopes.push(FxHashMap::default());
        for qualifier in self.program.arena.comp_qualifiers(qualifiers).iter().copied() {
            match qualifier {
                ArenaCompQualifier::For { target, iter, span } => {
                    self.expr(iter);
                    self.target(target, span, BindingKind::Runtime, false, false);
                }
                ArenaCompQualifier::If { condition, .. } => self.expr(condition),
            }
        }
        for &value in values { self.expr(value); }
        self.scopes.pop();
    }

    fn expr_children(&mut self, expression: ExprId) {
        let mut children = Vec::new();
        self.program.arena.for_each_expr_child(expression, |child| children.push(child));
        let callback = matches!(self.program.arena.expr(expression).kind,
            ArenaExprKind::Pipeline { .. } | ArenaExprKind::StructuredPipeline { .. });
        for child in children {
            if callback && let ArenaChild::Block(block) = child {
                self.block_with_item(block, true);
            } else { self.child(child); }
        }
    }

    fn child(&mut self, child: ArenaChild) {
        match child {
            ArenaChild::Stmt(statement) => self.stmt(statement),
            ArenaChild::Expr(expression) => self.expr(expression),
            ArenaChild::Block(block) => self.block(block),
            ArenaChild::Pattern(_) | ArenaChild::BindingTarget(_) => {}
            ArenaChild::AssignTarget(target) => {
                let mut children = Vec::new();
                self.program.arena.for_each_assign_target_child(target, |child| children.push(child));
                for child in children { self.child(child); }
            }
            ArenaChild::TypeExpr(ty) => {
                let mut children = Vec::new();
                self.program.arena.for_each_type_expr_child(ty, |child| children.push(child));
                for child in children { self.child(child); }
            }
            ArenaChild::BuilderBlock(block) => {
                let mut children = Vec::new();
                self.program.arena.for_each_builder_child(block, |child| children.push(child));
                for child in children { self.child(child); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::{arena::ArenaProgramBuilder, parser::Parser};

    fn check(source: &str, verify: impl FnOnce(&ResolvedBindings)) {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| verify(&ResolvedBindings::collect(&parsed.arena)));
    }

    #[test]
    fn sibling_blocks_keep_distinct_declaration_identities() {
        check("const value = 0\nif true { let value = 1\nassert value == 1 }\nif true { let value = 2\nassert value == 2 }\nassert value == 0\n", |facts| {
            let ids: FxHashSet<_> = facts.expressions.values().copied().collect();
            assert_eq!(ids.len(), 3);
            assert_eq!(ids.iter().filter(|&&id| facts.constant(id).is_some()).count(), 1);
        });
    }

    #[test]
    fn sequential_with_initializers_publish_runtime_identity() {
        check("const value = 0\nwith value = Ok(1), copied = Ok(value) { assert copied == value } else { assert value == 0 }\n", |facts| {
            let uses: Vec<_> = facts.expressions.values().map(|id| facts.declarations[id].kind).collect();
            assert_eq!(uses.iter().filter(|kind| matches!(kind, BindingKind::Runtime)).count(), 3);
            assert_eq!(uses.iter().filter(|kind| matches!(kind, BindingKind::Constant(_))).count(), 1);
        });
    }

    #[test]
    fn unrelated_arena_roots_do_not_enter_lexical_facts() {
        use crate::syntax::arena::ArenaProgramBuilder;
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        Parser::parse_source_into_arena_builder(SourceId::new(0), "const value = 1\n", &mut builder);
        let root = Parser::parse_source_into_arena_builder(SourceId::new(1), "const value = 2\nassert value == 2\n", &mut builder);
        let program = builder.finish_with_statements(root.statements);
        program.symbol_owner().with_current(|| {
            let facts = ResolvedBindings::collect(&program);
            assert_eq!(facts.expressions.len(), 1);
            let id = *facts.expressions.values().next().unwrap();
            assert_eq!(facts.declarations[&id].span.source_id, SourceId::new(1));
            assert_eq!(facts.declarations.values().filter(|decl| matches!(decl.kind, BindingKind::Constant(_))).count(), 1);
        });
    }

    #[test]
    fn assignment_facts_keep_exact_source_and_statement_range_owners() {
        let source = "first = 1\nsecond = 2\n";
        let mut builder = ArenaProgramBuilder::with_token_capacity(64);
        let parsed = Parser::parse_source_into_arena_builder(SourceId::new(3), source, &mut builder);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let internal_source = Parser::parse_source_into_arena_builder(
            SourceId::new(4),
            "built_in = 1\n",
            &mut builder,
        );
        assert!(internal_source.diagnostics.is_empty(), "{:?}", internal_source.diagnostics);
        let empty_internal_source =
            Parser::parse_source_into_arena_builder(SourceId::new(5), "", &mut builder);
        assert!(empty_internal_source.diagnostics.is_empty(), "{:?}", empty_internal_source.diagnostics);
        let internal_name = builder.name("internal");
        builder.push_internal_arena_module(
            "internal-key".to_owned(),
            internal_name,
            internal_source.statements,
        );
        let empty_internal_name = builder.name("empty_internal");
        builder.push_internal_arena_module(
            "empty-internal-key".to_owned(),
            empty_internal_name,
            empty_internal_source.statements,
        );
        let program = builder.finish_with_statements(parsed.statements);
        let first = ArenaRange { start: program.statements.start, len: 1 };
        let second = ArenaRange { start: first.start + first.len, len: 1 };

        program.symbol_owner().with_current(|| {
            let first_view = CheckBundleView::for_module(
                &program,
                first,
                None,
                Some(SourceId::new(3)),
            );
            let (first_facts, first_assignment_facts) =
                ResolvedBindings::collect_bundle_with_assignment_facts(&first_view);
            let first_names = std::collections::BTreeSet::from([Name::intern("first")]);
            assert_eq!(first_assignment_facts.names(SourceId::new(3), first), Some(&first_names));
            assert_eq!(first_assignment_facts.names(SourceId::new(4), first), None);
            assert_eq!(first_assignment_facts.names(SourceId::new(3), second), None);

            let empty = ArenaRange { start: first.start, len: 0 };
            let empty_view = CheckBundleView::for_root(
                &program,
                empty,
                None,
                None,
            );
            let (_, empty_assignment_facts) =
                ResolvedBindings::collect_bundle_with_assignment_facts(&empty_view);
            let source_text_source_id = program
                .source_text_source_id()
                .expect("program source-text owner");
            let no_names = std::collections::BTreeSet::new();
            assert_eq!(empty_assignment_facts.names(source_text_source_id, empty), Some(&no_names));
            let internal_names = std::collections::BTreeSet::from([Name::intern("built_in")]);
            assert_eq!(
                empty_assignment_facts.names(SourceId::new(4), internal_source.statements),
                Some(&internal_names),
            );
            assert_eq!(program.module_doc_for(empty_internal_source.statements), None);
            assert_eq!(
                empty_assignment_facts.names(SourceId::new(5), empty_internal_source.statements),
                None,
            );
            assert_eq!(
                empty_assignment_facts.names(SourceId::new(3), empty_internal_source.statements),
                None,
            );

            let second_view = CheckBundleView::for_module(
                &program,
                second,
                None,
                Some(SourceId::new(3)),
            );
            let (second_facts, second_assignment_facts) =
                ResolvedBindings::collect_bundle_with_assignment_facts(&second_view);
            let second_names = std::collections::BTreeSet::from([Name::intern("second")]);
            assert_eq!(second_assignment_facts.names(SourceId::new(3), second), Some(&second_names));

            let _ = first_facts.merge_visible(&second_facts);
            let mut merged_assignment_facts = first_assignment_facts;
            merged_assignment_facts.merge(&second_assignment_facts);
            assert_eq!(merged_assignment_facts.names(SourceId::new(3), first), Some(&first_names));
            assert_eq!(merged_assignment_facts.names(SourceId::new(3), second), Some(&second_names));
        });
    }

    #[test]
    fn assignment_facts_follow_statement_regions_and_raw_sugar_operands() {
        let source = concat!(
            "root.field = 1\n",
            "items[0] = 2\n",
            "e\"XSH_ASSIGNMENT_FACT\" = 3\n",
            "let result = try {\n",
            "  nested = 4\n",
            "  0\n",
            "}\n",
            "atomically replace output as temporary {\n",
            "  repeat 1 times { atomic_written = 6 }\n",
            "}\n",
            "total = run.text make ? when ready\n",
            "wait until ready within 5s backoff 1s..4s\n",
            "let expression_result = try {\n",
            "  atomically replace expression_output as expression_temp { expression_hidden = 7 }\n",
            "  0\n",
            "}\n",
            "proc show() {\n",
            "  defer {\n",
            "    atomically replace deferred_output as deferred_temp { deferred_hidden = 8 }\n",
            "  }\n",
            "}\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let view = CheckBundleView::whole_program(&parsed.arena);
            let (_, assignment_facts) = ResolvedBindings::collect_bundle_with_assignment_facts(&view);
            let names = assignment_facts.names(SourceId::new(0), parsed.arena.statements).unwrap();
            let expected = std::collections::BTreeSet::from([
                Name::intern("root"),
                Name::intern("items"),
                Name::intern("atomic_written"),
                Name::intern("total"),
            ]);
            assert_eq!(names, &expected);
        });
    }

    #[test]
    fn invalid_compound_module_statements_contribute_their_source_owner_facts() {
        let root_source = "use helper\n";
        let module_source = "if true { missing = 1 }\n";
        let root_id = SourceId::new(10);
        let module_id = SourceId::new(11);
        let mut builder = ArenaProgramBuilder::with_token_capacity(64);
        let root = Parser::parse_source_into_arena_builder(root_id, root_source, &mut builder);
        let use_statement = builder
            .ast_arena()
            .stmt_ids(root.statements)
            .next()
            .expect("root import statement");
        let ArenaStmtKind::Use(use_id) = builder.ast_arena().stmt(use_statement).kind else {
            panic!("first root statement is the helper import");
        };
        builder.set_use_resolved(use_id, std::sync::Arc::from("helper-key"));
        let module_name = builder.name("helper");
        let module =
            Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
        builder.push_arena_module("helper-key".to_owned(), module_name, module.statements);
        let program = builder.finish_with_statements(root.statements);

        program.symbol_owner().with_current(|| {
            let output = Checker::check_arena(&program, root_source);
            let module_names = std::collections::BTreeSet::from([Name::intern("missing")]);
            let root_names = std::collections::BTreeSet::new();
            assert!(output.diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == crate::diagnostic::Severity::Error
            }));
            assert_eq!(output.assignment_names(module_id, module.statements), Some(&module_names));
            assert_eq!(output.assignment_names(root_id, root.statements), Some(&root_names));
            assert_eq!(output.assignment_names(module_id, ArenaRange::default()), None);
        });
    }

    #[test]
    fn empty_imported_module_without_source_identity_has_no_assignment_owner() {
        let root_id = SourceId::new(15);
        let empty_module_id = SourceId::new(16);
        let mut builder = ArenaProgramBuilder::with_token_capacity(64);
        let root = Parser::parse_source_into_arena_builder(
            root_id,
            "use empty\nroot = 1\n",
            &mut builder,
        );
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        let use_statement = builder
            .ast_arena()
            .stmt_ids(root.statements)
            .next()
            .expect("root import statement");
        let ArenaStmtKind::Use(use_id) = builder.ast_arena().stmt(use_statement).kind else {
            panic!("first root statement is the empty module import");
        };
        builder.set_use_resolved(use_id, std::sync::Arc::from("empty-key"));
        let empty_module = Parser::parse_source_into_arena_builder(empty_module_id, "", &mut builder);
        assert!(empty_module.diagnostics.is_empty(), "{:?}", empty_module.diagnostics);
        let empty_name = builder.name("empty");
        builder.push_arena_module("empty-key".to_owned(), empty_name, empty_module.statements);
        let program = builder.finish_with_statements(root.statements);

        program.symbol_owner().with_current(|| {
            let view = CheckBundleView::for_root(&program, root.statements, None, Some(root_id));
            let (_, assignment_facts) = ResolvedBindings::collect_bundle_with_assignment_facts(&view);
            let root_names = std::collections::BTreeSet::from([Name::intern("root")]);
            assert_eq!(assignment_facts.names(root_id, root.statements), Some(&root_names));
            assert_eq!(assignment_facts.names(empty_module_id, empty_module.statements), None);
            assert_eq!(assignment_facts.names(root_id, empty_module.statements), None);
        });
    }

    #[test]
    fn checked_output_keeps_assignment_facts_when_semantic_checking_fails() {
        let source = "missing = 1\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(12), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let output = Checker::check_arena(&parsed.arena, source);
            let expected = std::collections::BTreeSet::from([Name::intern("missing")]);

            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error));
            assert_eq!(
                output.assignment_names(SourceId::new(12), parsed.arena.statements),
                Some(&expected),
            );
        });
    }

}
