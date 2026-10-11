use super::{FxHashMap, FxHashSet, Name, Span};
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaChild, ArenaCompQualifier, ArenaExprKind, ArenaPatternKind,
    ArenaProgram, ArenaRange, ArenaRecordFieldKind, ArenaStmtKind, ArenaTypeDefBody, BindingTargetId, BlockId,
    ExprId, FunctionDefId, PatternId, SpanId, StmtId, SugarView, UseStmtId,
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
    pub(super) fn args() -> Self { Self(BindingOrigin::Args) }
}

impl ResolvedBindings {
    pub fn collect(program: &ArenaProgram) -> Self {
        let mut resolver = ScopeChecker {
            program,
            facts: Self::default(),
            scopes: Vec::new(),
            namespace: None,
            globals: FxHashMap::default(),
            zero_variants: FxHashSet::default(),
        };
        resolver.predeclare(program.statement_ids(), true);
        for module in &program.modules {
            resolver.namespace = Some(module.name);
            resolver.predeclare(program.module_statements(module), true);
        }
        // Imports can name constants in a module that is visited later.
        for module in &program.modules {
            resolver.namespace = Some(module.name);
            resolver.file(program.module_statements(module).collect());
        }
        resolver.namespace = None;
        resolver.file(program.statement_ids().collect());
        resolver.facts
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

struct ScopeChecker<'a> {
    program: &'a ArenaProgram,
    facts: ResolvedBindings,
    scopes: Vec<FxHashMap<Name, BindingId>>,
    namespace: Option<Name>,
    globals: FxHashMap<(Option<Name>, Name), BindingId>,
    zero_variants: FxHashSet<(Option<Name>, Name)>,
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

    fn file(&mut self, statements: Vec<StmtId>) {
        self.scopes.push(self.globals.iter().filter_map(|(&(owner, name), &id)|
            (owner == self.namespace).then_some((name, id))).collect());
        let args = Name::intern("args");
        let id = self.declare(BindingOrigin::Args, args,
            Span::at(crate::source::SourceId::new(0), 0), BindingKind::Runtime, false, false);
        self.bind(args, id);
        for statement in statements { self.stmt(statement); }
        self.scopes.pop();
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
        if let Some(name) = entry.alias.or_else(|| self.program.arena.names(entry.path).last()) {
            let kind = entry.resolved.as_deref().map_or(BindingKind::Runtime,
                |key| BindingKind::Module(Name::intern(key)));
            let id = self.declare(BindingOrigin::Import(import), name, span, kind, global, false);
            if !self.scopes.is_empty() { self.bind(name, id); }
        }
    }

    fn stmt(&mut self, statement: StmtId) {
        let node = self.program.arena.stmt(statement);
        match node.kind {
            ArenaStmtKind::Const { .. } => self.stmt_children(statement),
            ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. }
            | ArenaStmtKind::Guard { target, .. } => {
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
            _ => self.stmt_children(statement),
        }
    }

    fn stmt_children(&mut self, statement: StmtId) {
        let mut children = Vec::new();
        self.program.arena.for_each_stmt_child(statement, SugarView::Core, |child| children.push(child));
        for child in children { self.child(child); }
    }

    fn expr(&mut self, expression: ExprId) {
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
            _ => self.expr_children(expression),
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
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

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
}
