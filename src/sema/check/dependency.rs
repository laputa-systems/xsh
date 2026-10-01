use super::DeclarationIdentity;
use crate::symbol::Name;
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArgKind, ArenaCommand,
    ArenaCommandArg, ArenaCommandArgKind, ArenaCompQualifier, ArenaEnvAssignmentValue,
    ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaModuleContractEntryKind,
    ArenaPatternKind, ArenaPipeStageKind, ArenaProgram, ArenaRange, ArenaRecordFieldKind,
    ArenaRedirectionTarget, ArenaSpawnTarget, ArenaStmtKind, ArenaTypeDefBody, ArenaWordPart,
    AssignTargetId, ArenaAssignTargetKind, BindingTargetId, BlockId, BuilderBlockId,
    ExprId, FunctionDefId, PatternId, RunFormId, StmtId,
};
use std::collections::{BTreeMap, BTreeSet};

/// Read only reachable syntax children. Arena rows retained by parser speculation
/// do not read bindings, and source spans never establish declaration identity.
pub(super) fn declaration_dependencies(
    arena: &ArenaProgram,
    declarations: &[DeclarationIdentity],
    names: &BTreeMap<(Option<Name>, Name), DeclarationIdentity>,
) -> BTreeMap<DeclarationIdentity, BTreeSet<DeclarationIdentity>> {
    declarations.iter().map(|&declaration| {
        let mut walker = DependencyWalker {
            arena, namespace: declaration.namespace, names,
            dependencies: BTreeSet::new(), shadows: BTreeMap::new(), scopes: vec![Vec::new()],
            pending: vec![Visit::Function(declaration.declaration)],
        };
        walker.walk();
        (declaration, walker.dependencies)
    }).collect()
}

enum Visit {
    Function(FunctionDefId),
    Block(BlockId),
    Statement(StmtId),
    Expression(ExprId),
    AssignTarget(AssignTargetId),
    Run(RunFormId),
    CommandArgument(ArenaCommandArg),
    Builder(BuilderBlockId),
    Read(Name),
    Bind(Vec<Name>),
    Enter(Vec<Name>),
    Leave,
}

struct DependencyWalker<'a> {
    arena: &'a ArenaProgram,
    namespace: Option<Name>,
    names: &'a BTreeMap<(Option<Name>, Name), DeclarationIdentity>,
    dependencies: BTreeSet<DeclarationIdentity>,
    shadows: BTreeMap<Name, usize>,
    scopes: Vec<Vec<Name>>,
    pending: Vec<Visit>,
}

impl DependencyWalker<'_> {
    fn sequence(&mut self, visits: impl IntoIterator<Item = Visit>) {
        let visits: Vec<_> = visits.into_iter().collect();
        self.pending.extend(visits.into_iter().rev());
    }

    fn bind(&mut self, names: Vec<Name>) {
        for name in names {
            if name == "_" { continue; }
            *self.shadows.entry(name).or_default() += 1;
            self.scopes.last_mut().unwrap().push(name);
        }
    }

    fn read(&mut self, name: Name) {
        if !self.shadows.contains_key(&name)
            && let Some(&declaration) = self.names.get(&(self.namespace, name)) {
            self.dependencies.insert(declaration);
        }
    }

    fn expression_or_run(value: ArenaExprOrRun) -> Visit {
        match value { ArenaExprOrRun::Expr(id) => Visit::Expression(id), ArenaExprOrRun::Run(id) => Visit::Run(id) }
    }

    fn target_names(&self, target: BindingTargetId) -> Vec<Name> {
        let mut pending = vec![target];
        let mut names = Vec::new();
        while let Some(target) = pending.pop() {
            match self.arena.arena.binding_target(target).kind {
                ArenaBindingTargetKind::Name(name) => names.push(name),
                ArenaBindingTargetKind::Record { fields, .. } => {
                    pending.extend(self.arena.arena.destructure_fields(fields).iter().map(|field| field.target));
                }
            }
        }
        names
    }

    fn pattern(&self, pattern: PatternId) -> (Vec<Name>, Vec<Visit>) {
        let mut pending = vec![pattern];
        let mut names = BTreeSet::new();
        let mut expressions = Vec::new();
        while let Some(pattern) = pending.pop() {
            match self.arena.arena.pattern(pattern).kind {
                ArenaPatternKind::Group(pattern) => pending.push(pattern),
                ArenaPatternKind::Alias { pattern, name, .. } => {
                    pending.push(pattern); names.insert(name);
                }
                ArenaPatternKind::Binding(name) => { names.insert(name); }
                ArenaPatternKind::Type { binding, .. } => names.extend(binding),
                ArenaPatternKind::Literal(expression) => expressions.push(Visit::Expression(expression)),
                ArenaPatternKind::Record { fields, .. } | ArenaPatternKind::ErrorVariant { fields, .. } => {
                    pending.extend(self.arena.arena.pattern_fields(fields).iter().map(|field| field.pattern));
                }
                ArenaPatternKind::List { elements, rest } => {
                    pending.extend(self.arena.arena.pattern_ids(elements)); pending.extend(rest);
                }
                ArenaPatternKind::Alternation(patterns) | ArenaPatternKind::Tuple(patterns) => {
                    // Valid alternatives bind the same names. Their value
                    // children are all read before those captures enter scope.
                    pending.extend(self.arena.arena.pattern_ids(patterns));
                }
                ArenaPatternKind::Constructor { arg, .. } => pending.extend(arg),
                ArenaPatternKind::Wildcard | ArenaPatternKind::TestName { .. } | ArenaPatternKind::Facet(_) => {}
            }
        }
        (names.into_iter().collect(), expressions)
    }

    fn condition_names(&self, condition: ExprId) -> Vec<Name> {
        match self.arena.arena.expr(condition).kind {
            ArenaExprKind::PatternCondition { arms, .. } => {
                self.arena.arena.match_expr_arms(arms).first().map(|arm| self.pattern(arm.pattern).0).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    fn arguments(&self, args: ArenaRange) -> Vec<Visit> {
        self.arena.arena.call_args(args).iter().map(|argument| match argument.kind {
            ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. }
            | ArenaCallArgKind::NamedSpread { value, .. } | ArenaCallArgKind::Splice { value, .. } => Visit::Expression(value),
        }).collect()
    }

    fn environment(&self, assignments: ArenaRange) -> Vec<Visit> {
        self.arena.arena.env_assignments(assignments).iter().map(|assignment| match &assignment.value {
            ArenaEnvAssignmentValue::CommandArg(argument) => Visit::CommandArgument(argument.clone()),
            ArenaEnvAssignmentValue::Expr(expression) => Visit::Expression(*expression),
        }).collect()
    }

    fn walk(&mut self) {
        while let Some(visit) = self.pending.pop() {
            match visit {
                Visit::Read(name) => self.read(name),
                Visit::Bind(names) => self.bind(names),
                Visit::Enter(names) => { self.scopes.push(Vec::new()); self.bind(names); }
                Visit::Leave => {
                    for name in self.scopes.pop().unwrap() {
                        let count = self.shadows.get_mut(&name).unwrap();
                        *count -= 1;
                        if *count == 0 { self.shadows.remove(&name); }
                    }
                }
                Visit::Function(id) => {
                    let function = self.arena.arena.function_def(id);
                    let params = self.arena.arena.params(function.params);
                    // Every default belongs to the enclosing lexical scope.
                    // Parameters enter scope only after all defaults are read.
                    let mut visits: Vec<_> = params.iter().filter_map(|param| param.default.map(Visit::Expression)).collect();
                    visits.push(Visit::Enter(params.iter().map(|param| param.name).collect()));
                    visits.push(Visit::Block(function.body)); visits.push(Visit::Leave);
                    self.sequence(visits);
                }
                Visit::Block(id) => {
                    let block = self.arena.arena.block(id);
                    let mut visits = vec![Visit::Enter(self.arena.arena.block_params(block.params).iter().map(|param| param.name).collect())];
                    visits.extend(self.arena.arena.stmt_ids(block.statements).map(Visit::Statement));
                    visits.push(Visit::Leave); self.sequence(visits);
                }
                Visit::Statement(id) => self.statement(id),
                Visit::Expression(id) => self.expression(id),
                Visit::AssignTarget(id) => match self.arena.arena.assign_target(id).kind {
                    ArenaAssignTargetKind::Name(name) => self.read(name),
                    ArenaAssignTargetKind::Field { base, .. } => self.pending.push(Visit::AssignTarget(base)),
                    ArenaAssignTargetKind::Index { base, index } => self.sequence([Visit::AssignTarget(base), Visit::Expression(index)]),
                },
                Visit::Run(id) => self.run(id),
                Visit::CommandArgument(argument) => match argument.kind {
                    ArenaCommandArgKind::SpliceName(name) => self.read(name),
                    ArenaCommandArgKind::SpliceExpr(expression) | ArenaCommandArgKind::Typed(expression) => self.pending.push(Visit::Expression(expression)),
                    ArenaCommandArgKind::Word(parts) => {
                        let visits = self.arena.arena.word_parts(parts).filter_map(|part| match part {
                            ArenaWordPart::Shorthand(expression) | ArenaWordPart::Interpolation(expression) => Some(Visit::Expression(expression)),
                            ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => None,
                        }).collect::<Vec<_>>();
                        self.sequence(visits);
                    }
                },
                Visit::Builder(id) => {
                    let block = self.arena.arena.builder_block(id);
                    let mut visits = vec![Visit::Enter(Vec::new())];
                    for entry in self.arena.arena.builder_entries(block.entries) {
                        match entry.kind {
                            ArenaBuilderEntryKind::Field { value, .. } => visits.push(Visit::Expression(value)),
                            ArenaBuilderEntryKind::Entry { args, block, .. } => {
                                visits.extend(self.arguments(args)); visits.extend(block.map(Visit::Builder));
                            }
                            ArenaBuilderEntryKind::Task { block, .. } => visits.push(Visit::Block(block)),
                            ArenaBuilderEntryKind::Stmt(statement) => visits.push(Visit::Statement(statement)),
                        }
                    }
                    visits.push(Visit::Leave); self.sequence(visits);
                }
            }
        }
    }

    fn statement(&mut self, id: StmtId) {
        let mut visits = Vec::new();
        match self.arena.arena.stmt(id).kind {
            ArenaStmtKind::Use(_) | ArenaStmtKind::ErrorDef(_) | ArenaStmtKind::Continue => {}
            ArenaStmtKind::Export(statement) => visits.push(Visit::Statement(statement)),
            ArenaStmtKind::TypeDef(id) => match self.arena.arena.type_def(id).body {
                ArenaTypeDefBody::Alias(_) => {},
                ArenaTypeDefBody::RecordSchema(fields) => visits.extend(self.arena.arena.schema_fields(fields).iter().filter_map(|field| field.default.map(Visit::Expression))),
                ArenaTypeDefBody::TagUnion(variants) => visits.extend(self.arena.arena.tag_variants(variants).iter().filter_map(|variant| variant.wire_value.map(Visit::Expression))),
                ArenaTypeDefBody::ModuleContract(entries) => for entry in self.arena.arena.module_contract_entries(entries) {
                    match entry.kind {
                        ArenaModuleContractEntryKind::Value(_) => {},
                        ArenaModuleContractEntryKind::Proc { params, .. } | ArenaModuleContractEntryKind::Pure { params, .. } => {
                            visits.extend(self.arena.arena.params(params).iter().filter_map(|param| param.default.map(Visit::Expression)));
                        }
                    }
                },
            },
            ArenaStmtKind::Const { target, initializer, .. } | ArenaStmtKind::Let { target, initializer, .. }
            | ArenaStmtKind::Var { target, initializer, .. } => {
                visits.push(Self::expression_or_run(initializer)); visits.push(Visit::Bind(self.target_names(target)));
            }
            ArenaStmtKind::Assign { target, value, .. } => {
                visits.push(Visit::AssignTarget(target)); visits.push(Self::expression_or_run(value));
            }
            ArenaStmtKind::PureDef(id) | ArenaStmtKind::ProcDef(id) | ArenaStmtKind::StreamDef(id)
            | ArenaStmtKind::CliMain(id) => visits.push(Visit::Function(id)),
            ArenaStmtKind::SignalHook(id) => visits.push(Visit::Block(self.arena.arena.signal_hook(id).body)),
            ArenaStmtKind::Return(value) => visits.extend(value.map(Self::expression_or_run)),
            ArenaStmtKind::Yield(value) | ArenaStmtKind::Defer(value) => visits.push(Self::expression_or_run(value)),
            ArenaStmtKind::YieldDelegate(expression) | ArenaStmtKind::Expr(expression) => visits.push(Visit::Expression(expression)),
            ArenaStmtKind::TailBareIdent(name) => visits.push(Visit::Read(name)),
            ArenaStmtKind::If { branches, else_block } => {
                for branch in self.arena.arena.if_branches(branches) {
                    visits.push(Visit::Expression(branch.condition));
                    visits.push(Visit::Enter(self.condition_names(branch.condition)));
                    visits.push(Visit::Block(branch.block)); visits.push(Visit::Leave);
                }
                visits.extend(else_block.map(Visit::Block));
            }
            ArenaStmtKind::While { condition, block } => {
                visits.extend([Visit::Expression(condition), Visit::Enter(self.condition_names(condition)), Visit::Block(block), Visit::Leave]);
            }
            ArenaStmtKind::For { target, iter, block } => {
                visits.extend([Visit::Expression(iter), Visit::Enter(self.target_names(target)), Visit::Block(block), Visit::Leave]);
            }
            ArenaStmtKind::With { bindings, body, else_block } => {
                visits.push(Visit::Enter(Vec::new()));
                for binding in self.arena.arena.with_bindings(bindings) {
                    visits.push(Visit::Expression(binding.initializer)); visits.push(Visit::Bind(vec![binding.name]));
                }
                visits.extend([Visit::Block(body), Visit::Leave, Visit::Block(else_block)]);
            }
            ArenaStmtKind::Loop { block } => visits.push(Visit::Block(block)),
            ArenaStmtKind::Guard { target, initializer, else_block, .. } => {
                visits.extend([Self::expression_or_run(initializer), Visit::Block(else_block), Visit::Bind(self.target_names(target))]);
            }
            ArenaStmtKind::GuardedStmt { stmt, condition, negate } => {
                visits.extend([Visit::Expression(condition), Visit::Enter(if negate { Vec::new() } else { self.condition_names(condition) }), Visit::Statement(stmt), Visit::Leave]);
            }
            ArenaStmtKind::BooleanGuard { condition, else_block } => visits.extend([Visit::Expression(condition), Visit::Block(else_block)]),
            ArenaStmtKind::Assert { condition, message } => visits.extend([Visit::Expression(condition), Visit::Expression(message)]),
            ArenaStmtKind::Break { value } => visits.extend(value.map(Visit::Expression)),
            ArenaStmtKind::Match { value, arms } => {
                visits.push(Visit::Expression(value));
                for arm in self.arena.arena.match_arms(arms) {
                    let (names, expressions) = self.pattern(arm.pattern);
                    visits.extend(expressions); visits.push(Visit::Enter(names));
                    visits.extend(arm.guard.map(Visit::Expression)); visits.push(Visit::Block(arm.block)); visits.push(Visit::Leave);
                }
            }
            ArenaStmtKind::Command(id) => match &self.arena.arena.command_stmt(id).command {
                ArenaCommand::Proc { name, args } => {
                    visits.push(Visit::Read(*name)); visits.extend(self.arena.arena.command_args(*args).iter().cloned().map(Visit::CommandArgument));
                }
                ArenaCommand::Core { args, env, block, .. } => {
                    visits.extend(self.arena.arena.command_args(*args).iter().cloned().map(Visit::CommandArgument));
                    visits.extend(self.environment(*env)); visits.extend(block.map(Visit::Block));
                }
                ArenaCommand::Run(id) => visits.push(Visit::Run(*id)),
            },
        }
        self.sequence(visits);
    }

    fn expression(&mut self, id: ExprId) {
        let mut visits = Vec::new();
        match self.arena.arena.expr(id).kind {
            ArenaExprKind::Null | ArenaExprKind::Bool(_) | ArenaExprKind::Int(_) | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_) | ArenaExprKind::Str(_) | ArenaExprKind::PathStr(_) | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::Bytes(_) | ArenaExprKind::Regex(_) | ArenaExprKind::Item | ArenaExprKind::LastStatus
            | ArenaExprKind::EnvGet { .. } | ArenaExprKind::EnvPathList => {}
            ArenaExprKind::Ident(name) => visits.push(Visit::Read(name)),
            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => visits.extend(self.arena.arena.fmt_parts(parts).filter_map(|part| match part {
                ArenaFmtPart::Text(_) => None, ArenaFmtPart::Expr(expression, _) => Some(Visit::Expression(expression)),
            })),
            ArenaExprKind::List(elements) => visits.extend(self.arena.arena.list_element_exprs(elements).map(Visit::Expression)),
            ArenaExprKind::ListComp { expr, qualifiers } | ArenaExprKind::MapComp { key: expr, qualifiers, .. } => {
                visits.push(Visit::Enter(Vec::new()));
                for qualifier in self.arena.arena.comp_qualifiers(qualifiers) {
                    match *qualifier {
                        ArenaCompQualifier::For { target, iter, .. } => { visits.push(Visit::Expression(iter)); visits.push(Visit::Bind(self.target_names(target))); }
                        ArenaCompQualifier::If { condition, .. } => visits.push(Visit::Expression(condition)),
                    }
                }
                visits.push(Visit::Expression(expr));
                if let ArenaExprKind::MapComp { value, .. } = self.arena.arena.expr(id).kind { visits.push(Visit::Expression(value)); }
                visits.push(Visit::Leave);
            }
            ArenaExprKind::Record(fields) => for field in self.arena.arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => visits.extend([Visit::Expression(key), Visit::Expression(value)]),
                    ArenaRecordFieldKind::Named { value, .. } | ArenaRecordFieldKind::Path { value, .. } => visits.push(Visit::Expression(value)),
                    ArenaRecordFieldKind::Spread { expr, .. } => visits.push(Visit::Expression(expr)),
                    ArenaRecordFieldKind::Shorthand { name, .. } => visits.push(Visit::Read(name)),
                }
            },
            ArenaExprKind::If { branches, else_value } => {
                for branch in self.arena.arena.if_expr_branches(branches) {
                    visits.extend([Visit::Expression(branch.condition), Visit::Enter(self.condition_names(branch.condition)), Visit::Expression(branch.value), Visit::Leave]);
                }
                visits.push(Visit::Expression(else_value));
            }
            ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms }
            | ArenaExprKind::PatternCondition { value, arms } => {
                visits.push(Visit::Expression(value));
                for arm in self.arena.arena.match_expr_arms(arms) {
                    let (names, expressions) = self.pattern(arm.pattern);
                    visits.extend(expressions); visits.push(Visit::Enter(names));
                    visits.extend(arm.guard.map(Visit::Expression)); visits.push(Visit::Expression(arm.value)); visits.push(Visit::Leave);
                }
            }
            ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) | ArenaExprKind::Require { value: expr, .. } => visits.push(Visit::Expression(expr)),
            ArenaExprKind::ComparisonChain(pairs) => visits.extend(self.arena.arena.comparison_chain_operands(pairs).map(Visit::Expression)),
            ArenaExprKind::Binary { left, right, .. } => visits.extend([Visit::Expression(left), Visit::Expression(right)]),
            ArenaExprKind::Call { callee, args } => { visits.push(Visit::Expression(callee)); visits.extend(self.arguments(args)); }
            ArenaExprKind::ValuePipelineCall { input, call, .. } => visits.extend([Visit::Expression(input), Visit::Expression(call)]),
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                if let ArenaExprKind::Ident(namespace) = self.arena.arena.expr(base).kind
                    && !self.shadows.contains_key(&namespace)
                    && let Some(&identity) = self.names.get(&(Some(namespace), name)) {
                    self.dependencies.insert(identity);
                }
                visits.push(Visit::Expression(base));
            }
            ArenaExprKind::Index { base, index, .. } => visits.extend([Visit::Expression(base), Visit::Expression(index)]),
            ArenaExprKind::Slice { base, start, end, .. } => { visits.push(Visit::Expression(base)); visits.extend(start.map(Visit::Expression)); visits.extend(end.map(Visit::Expression)); }
            ArenaExprKind::Pipeline { input, stages } => {
                visits.push(Visit::Expression(input));
                for stage in self.arena.arena.pipe_stages(stages) {
                    match &stage.kind {
                        ArenaPipeStageKind::Expr(expression) => visits.push(Visit::Expression(*expression)),
                        ArenaPipeStageKind::Stream(stage) => { visits.extend(self.arguments(stage.args)); visits.extend(stage.block.map(Visit::Block)); }
                    }
                }
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                visits.push(Visit::Expression(input));
                for stage in self.arena.arena.stream_stages(stages) { visits.extend(self.arguments(stage.args)); visits.extend(stage.block.map(Visit::Block)); }
            }
            ArenaExprKind::Run(id) => visits.push(Visit::Run(id)),
            ArenaExprKind::Spawn(spawn) => match spawn.target { ArenaSpawnTarget::Run(id) => visits.push(Visit::Run(id)), ArenaSpawnTarget::Command(expression) => visits.push(Visit::Expression(expression)) },
            ArenaExprKind::Wait(wait) => visits.push(Visit::Expression(wait.target)),
            ArenaExprKind::BuilderCall { call, block } => visits.extend([Visit::Expression(call), Visit::Builder(block)]),
            ArenaExprKind::Capture(block) | ArenaExprKind::Loop { block } | ArenaExprKind::ValueBlock(block) => visits.push(Visit::Block(block)),
            ArenaExprKind::Retry { delays, pattern, block } => { visits.extend(self.arena.arena.expr_ids(delays).map(Visit::Expression)); if let Some(pattern) = pattern { visits.extend(self.pattern(pattern).1); } visits.push(Visit::Block(block)); }
            ArenaExprKind::ErrorContext { message, block } => visits.extend([Visit::Expression(message), Visit::Block(block)]),
            ArenaExprKind::ContextScope { input, block, .. } => visits.extend([Visit::Expression(input), Visit::Block(block)]),
        }
        self.sequence(visits);
    }

    fn run(&mut self, id: RunFormId) {
        let mut visits = Vec::new();
        for segment in self.arena.arena.run_segments(self.arena.arena.run_form(id).segments) {
            visits.extend(segment.timeout.into_iter().chain(segment.cpu_max).chain(segment.accept).map(Visit::Expression));
            visits.extend(self.environment(segment.env));
            visits.push(Visit::CommandArgument(segment.target.clone()));
            visits.extend(self.arena.arena.command_args(segment.args).iter().cloned().map(Visit::CommandArgument));
            for redirection in self.arena.arena.redirections(segment.redirections) {
                match &redirection.target {
                    ArenaRedirectionTarget::Path(argument) | ArenaRedirectionTarget::Fd(argument) => visits.push(Visit::CommandArgument(argument.clone())),
                }
            }
        }
        self.sequence(visits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn edges(source: &str, aliases: &[(&str, &str, &str)]) -> BTreeMap<String, BTreeSet<String>> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let arena = &parsed.arena;
        let _symbols = arena.symbol_owner().enter();
        let declarations: Vec<_> = (0..arena.arena.function_defs.len()).map(|index| {
            DeclarationIdentity { source: SourceId::new(0), namespace: None,
                declaration: FunctionDefId::from_index(index) }
        }).collect();
        let mut names: BTreeMap<_, _> = declarations.iter().map(|&identity| {
            ((None, arena.arena.function_def(identity.declaration).name), identity)
        }).collect();
        for &(namespace, alias, canonical) in aliases {
            let identity = names[&(None, Name::intern(canonical))];
            names.insert((Some(Name::intern(namespace)), Name::intern(alias)), identity);
        }
        declaration_dependencies(arena, &declarations, &names).into_iter().map(|(owner, deps)| {
            (arena.arena.function_def(owner.declaration).name.to_string(), deps.into_iter().map(|id|
                arena.arena.function_def(id.declaration).name.to_string()).collect())
        }).collect()
    }

    fn expected(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn defaults_read_outer_names_while_parameters_shadow_body_references() {
        let graph = edges("pure dependency() { 1 }\npure root(dependency = dependency()) { let _ = {dependency}; dependency }\n", &[]);
        assert_eq!(graph["root"], expected(&["dependency"]));
        assert!(graph["dependency"].is_empty());
    }

    #[test]
    fn initializers_precede_shadowing_and_record_labels_are_not_reads() {
        let graph = edges("pure dependency() { 1 }\npure other() { 2 }\npure root() { let _ = {dependency: other}; let dependency = dependency; let _ = {dependency}; dependency }\n", &[]);
        assert_eq!(graph["root"], expected(&["dependency", "other"]));
        let masked = edges("pure dependency() { 1 }\npure root(dependency) { let _ = {dependency: 1}; let _ = {dependency}; dependency }\n", &[]);
        assert!(masked["root"].is_empty());
    }

    #[test]
    fn qualified_aliases_resolve_canonical_ids_only_without_lexical_shadowing() {
        let graph = edges("pure canonical() { 1 }\npure visible() { imported.member() }\npure hidden(imported) { imported.member() }\n", &[("imported", "member", "canonical")]);
        assert_eq!(graph["visible"], expected(&["canonical"]));
        assert!(graph["hidden"].is_empty());
    }

    #[test]
    fn pattern_captures_are_local_to_the_selected_arm_and_loop_body() {
        let graph = edges(r#"
pure dependency() { 1 }
pure subject() { Ok(7) }
pure root() {
    if let Ok(dependency) = subject() { let _ = dependency } else { let _ = dependency }
    while let Ok(dependency) = subject() { let _ = dependency; break }
    match subject() { Ok(dependency) if dependency > 0 => { let _ = dependency }, _ => { let _ = dependency } }
}
pure masked() {
    if let Ok(dependency) = subject() { let _ = dependency }
    while let Ok(dependency) = subject() { let _ = dependency; break }
}
"#, &[]);
        assert_eq!(graph["root"], expected(&["dependency", "subject"]));
        assert_eq!(graph["masked"], expected(&["subject"]));
    }

    #[test]
    fn loop_with_and_guard_bindings_keep_initializer_and_failure_scopes_separate() {
        let graph = edges(r#"
pure dependency() { 1 }
pure subject() { Ok(7) }
pure root() {
    for dependency in [subject()] { let _ = dependency }
    with dependency = subject(), other = dependency { let _ = dependency }
    else { |dependency| let _ = dependency }
    guard let dependency = subject() else { |dependency| return dependency }
    dependency
}
"#, &[]);
        assert_eq!(graph["root"], expected(&["subject"]));
    }

    #[test]
    fn commands_and_pipeline_callbacks_visit_only_structural_expression_children() {
        let graph = edges(r#"
pure dependency(value) { value }
pure other(value) { value }
proc root() {
    print $dependency ${other(7)}
    run true ${dependency(7)}
    let _ = [1] |> map(dependency)
    let _ = [1] |> map { |other| dependency(other) }
}
"#, &[]);
        assert_eq!(graph["root"], expected(&["dependency", "other"]));
    }

    #[test]
    fn comprehension_clauses_bind_after_each_iterable_and_do_not_escape() {
        let graph = edges(r#"
pure dependency() { 1 }
pure other() { 2 }
pure source() { [{dependency: 7}] }
pure root() {
    let _ = [dependency for {dependency} in source() if dependency > 0 for other in [dependency] if other > 0]
    let _ = {dependency: other for dependency in source() for other in [dependency]}
    dependency
}
"#, &[]);
        assert_eq!(graph["root"], expected(&["dependency", "source"]));
    }

    #[test]
    fn leaving_nested_scopes_restores_value_and_namespace_references() {
        let graph = edges(r#"
pure dependency() { 1 }
pure other() { 2 }
pure canonical() { 3 }
pure root() {
    if true { let dependency = other; let _ = dependency }
    if true { let imported = 7; let _ = imported.member() }
    let _ = imported.member()
    dependency
}
"#, &[("imported", "member", "canonical")]);
        assert_eq!(graph["root"], expected(&["canonical", "dependency", "other"]));
    }

    #[test]
    fn namespace_lookups_preserve_foreign_canonical_source_identity() {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0),
            "pure root() { let _ = dependency; imported.member() }\n");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let arena = &parsed.arena;
        let _symbols = arena.symbol_owner().enter();
        let namespace = Name::intern("local");
        let root = DeclarationIdentity { source: SourceId::new(0), namespace: Some(namespace),
            declaration: FunctionDefId::from_index(0) };
        let foreign = DeclarationIdentity { source: SourceId::new(41), namespace: Some(Name::intern("canonical")),
            declaration: FunctionDefId::from_index(99) };
        let top_level = DeclarationIdentity { source: SourceId::new(42), namespace: None,
            declaration: FunctionDefId::from_index(100) };
        let names = BTreeMap::from([
            ((Some(namespace), Name::intern("dependency")), foreign),
            ((None, Name::intern("dependency")), top_level),
            ((Some(Name::intern("imported")), Name::intern("member")), foreign),
        ]);
        let graph = declaration_dependencies(arena, &[root], &names);
        assert_eq!(graph[&root], BTreeSet::from([foreign]));
    }
}
