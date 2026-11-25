use crate::sema::check::{BindingKind, CompactBodyFacts};
use crate::sema::types::Type;
use crate::source::SourceMap;
use crate::symbol::Name;
use crate::syntax::arena::{
    ArenaChild, ArenaCommand, ArenaExprKind, ArenaProgram, ArenaRecordFieldKind,
    ArenaStmtKind, ArenaUserModule, FunctionDefId, StmtId, SugarView,
};
use rustc_hash::{FxHashMap, FxHashSet};

use super::CompactRootScope;

struct ImportFunction {
    module: usize,
    definition: FunctionDefId,
    statement: StmtId,
}

struct ImportModule {
    functions: FxHashMap<String, Vec<usize>>,
    exports: FxHashSet<Name>,
    context: ImportContext,
}

struct ImportContext {
    module: Option<usize>,
    aliases: FxHashMap<Name, usize>,
    alias_words: FxHashMap<String, usize>,
}

impl ImportContext {
    fn new(
        program: &ArenaProgram,
        module: Option<usize>,
        statements: impl Iterator<Item = StmtId>,
        module_index: &FxHashMap<&str, usize>,
    ) -> Self {
        let aliases = statement_aliases(program, statements, module_index);
        let alias_words = aliases.iter().map(|(alias, &module)| (alias.as_str().to_string(), module)).collect();
        Self { module, aliases, alias_words }
    }
}

pub(super) fn unreachable_import_functions(
    program: &ArenaProgram,
    bodies: &CompactBodyFacts,
    scope: &CompactRootScope<'_>,
    sources: &SourceMap,
) -> FxHashSet<FunctionDefId> {
    let user_modules = scope
        .modules
        .iter()
        .copied()
        .filter(|module| !module.internal)
        .collect::<Vec<_>>();
    if user_modules.is_empty() {
        return FxHashSet::default();
    }
    let module_index = user_modules
        .iter()
        .enumerate()
        .map(|(index, module)| (module.key.as_str(), index))
        .collect::<FxHashMap<_, _>>();
    let mut functions = Vec::new();
    let mut modules = Vec::with_capacity(user_modules.len());
    for (module, user_module) in user_modules.iter().enumerate() {
        let mut by_name = FxHashMap::<String, Vec<usize>>::default();
        for statement in program.module_statements(user_module) {
            let Some(definition) = module_function_definition(program, statement) else { continue };
            let name = program.arena.function_def(definition).name.as_str().to_string();
            if !name.bytes().all(is_word_byte) {
                return FxHashSet::default();
            }
            by_name.entry(name).or_default().push(functions.len());
            functions.push(ImportFunction { module, definition, statement });
        }
        modules.push(ImportModule {
            functions: by_name,
            exports: module_exports(program, user_module),
            context: ImportContext::new(program, Some(module), program.module_statements(user_module), &module_index),
        });
    }
    if functions.is_empty() {
        return FxHashSet::default();
    }
    let root = ImportContext::new(program, None, program.arena.stmt_ids(scope.root_statements), &module_index);
    let typed_namespaces = program
        .arena
        .stmt_ids(scope.root_statements)
        .chain(user_modules.iter().flat_map(|module| program.module_statements(module)))
        .filter_map(|statement| typed_namespace_module(program, bodies, statement, &module_index))
        .collect::<Vec<_>>();

    let mut reach = Reach {
        program,
        sources,
        functions: &functions,
        modules: &modules,
        kept: vec![false; functions.len()],
        whole: vec![false; modules.len()],
        pending: Vec::new(),
    };
    reach.keep_modules(typed_namespaces);
    for statement in program.arena.stmt_ids(scope.root_statements) {
        reach.visit(statement, &root);
    }
    for (module, user_module) in user_modules.iter().enumerate() {
        for statement in program.module_statements(user_module) {
            if module_function_definition(program, statement).is_none() {
                reach.visit(statement, &modules[module].context);
            }
        }
    }
    while let Some(function) = reach.pending.pop() {
        let function = &functions[function];
        reach.visit(function.statement, &modules[function.module].context);
    }
    functions
        .iter()
        .zip(&reach.kept)
        .filter(|(_, kept)| !**kept)
        .map(|(function, _)| function.definition)
        .collect()
}

struct Reach<'a> {
    program: &'a ArenaProgram,
    sources: &'a SourceMap,
    functions: &'a [ImportFunction],
    modules: &'a [ImportModule],
    kept: Vec<bool>,
    whole: Vec<bool>,
    pending: Vec<usize>,
}

impl Reach<'_> {
    fn visit(&mut self, statement: StmtId, context: &ImportContext) {
        self.keep_named_in(statement, context);
        let mut escapes = Vec::new();
        namespace_escapes(self.program, ArenaChild::Stmt(statement), &context.aliases, self.modules, &mut escapes);
        self.keep_modules(escapes);
    }

    fn keep_modules(&mut self, modules: Vec<usize>) {
        for module in modules {
            if std::mem::replace(&mut self.whole[module], true) {
                continue;
            }
            for (function, candidate) in self.functions.iter().enumerate() {
                if candidate.module == module {
                    self.keep(function);
                }
            }
        }
    }

    fn keep_named_in(&mut self, statement: StmtId, context: &ImportContext) {
        let span = self.program.arena.stmt_span(statement);
        let text = self
            .sources
            .get(span.source_id)
            .and_then(|source| source.text().get(span.start()..span.end()));
        let Some(text) = text else {
            for function in 0..self.functions.len() {
                self.keep(function);
            }
            return;
        };
        let bytes = text.as_bytes();
        let mut namespace = None;
        let mut after_word = None;
        let mut position = 0;
        while position < bytes.len() {
            let byte = bytes[position];
            if is_word_byte(byte) {
                let start = position;
                while position < bytes.len() && is_word_byte(bytes[position]) {
                    position += 1;
                }
                let word = &text[start..position];
                if let Some(module) = namespace.take() {
                    self.keep_word(module, word);
                }
                if let Some(module) = context.module {
                    self.keep_word(module, word);
                }
                after_word = context.alias_words.get(word).copied();
                continue;
            }
            position += 1;
            match byte {
                b'.' => namespace = after_word.take(),
                b'?' if bytes.get(position) == Some(&b'.') => {}
                b' ' | b'\t' | b'\r' | b'\n' => {}
                _ => {
                    namespace = None;
                    after_word = None;
                }
            }
        }
    }

    fn keep_word(&mut self, module: usize, word: &str) {
        if let Some(functions) = self.modules[module].functions.get(word) {
            for &function in functions {
                self.keep(function);
            }
        }
    }

    fn keep(&mut self, function: usize) {
        if !std::mem::replace(&mut self.kept[function], true) {
            self.pending.push(function);
        }
    }
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(super) fn module_function_definition(program: &ArenaProgram, statement: StmtId) -> Option<FunctionDefId> {
    match program.arena.stmt_kind(statement) {
        ArenaStmtKind::Export(inner) => module_function_definition(program, inner),
        ArenaStmtKind::PureDef(definition)
        | ArenaStmtKind::ProcDef(definition)
        | ArenaStmtKind::StreamDef(definition)
        | ArenaStmtKind::CliMain(definition) => Some(definition),
        _ => None,
    }
}

fn typed_namespace_module(
    program: &ArenaProgram,
    bodies: &CompactBodyFacts,
    statement: StmtId,
    module_index: &FxHashMap<&str, usize>,
) -> Option<usize> {
    let ArenaStmtKind::Use(use_id) = program.arena.stmt_kind(statement) else { return None };
    let module = *module_index.get(program.arena.use_stmt(use_id).resolved.as_deref()?)?;
    let namespace = super::compact_use_import_namespace(program, use_id)?;
    let bindings = &bodies.resolved_bindings;
    let binding = bindings.declaration_sites.get(&(program.arena.stmt_span(statement), namespace))?;
    if !matches!(bindings.declarations.get(binding)?.kind, BindingKind::Module(_)) {
        return None;
    }
    bodies
        .binding_reads
        .expressions_reading(*binding)
        .iter()
        .any(|(_, expression)| matches!(bodies.expr_types.get(expression), Some(Type::Module(_))))
        .then_some(module)
}

fn statement_aliases(
    program: &ArenaProgram,
    statements: impl Iterator<Item = StmtId>,
    module_index: &FxHashMap<&str, usize>,
) -> FxHashMap<Name, usize> {
    let mut aliases = FxHashMap::default();
    for statement in statements {
        let ArenaStmtKind::Use(use_id) = program.arena.stmt_kind(statement) else { continue };
        let Some(key) = program.arena.use_stmt(use_id).resolved.as_deref() else { continue };
        let Some(&module) = module_index.get(key) else { continue };
        if let Some(namespace) = super::compact_use_import_namespace(program, use_id) {
            aliases.insert(namespace, module);
        }
    }
    aliases
}

fn module_exports(program: &ArenaProgram, module: &ArenaUserModule) -> FxHashSet<Name> {
    let mut exports = FxHashSet::default();
    for statement in program.module_statements(module) {
        let ArenaStmtKind::Export(inner) = program.arena.stmt_kind(statement) else { continue };
        match program.arena.stmt_kind(inner) {
            ArenaStmtKind::Let { target, .. }
            | ArenaStmtKind::Const { target, .. }
            | ArenaStmtKind::Var { target, .. } => {
                if let crate::syntax::arena::ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind {
                    exports.insert(name);
                }
            }
            ArenaStmtKind::PureDef(definition)
            | ArenaStmtKind::ProcDef(definition)
            | ArenaStmtKind::StreamDef(definition)
            | ArenaStmtKind::CliMain(definition) => {
                exports.insert(program.arena.function_def(definition).name);
            }
            ArenaStmtKind::TypeDef(definition) => {
                exports.insert(program.arena.type_def(definition).name);
            }
            ArenaStmtKind::ErrorDef(definition) => {
                exports.insert(program.arena.error_def(definition).name);
            }
            _ => {}
        }
    }
    exports
}

fn namespace_escapes(
    program: &ArenaProgram,
    root: ArenaChild,
    aliases: &FxHashMap<Name, usize>,
    modules: &[ImportModule],
    escapes: &mut Vec<usize>,
) {
    let arena = &program.arena;
    let mut stack = vec![root];
    while let Some(child) = stack.pop() {
        let mut push = |child| stack.push(child);
        match child {
            ArenaChild::Stmt(statement) => {
                match arena.stmt_kind(statement) {
                    ArenaStmtKind::TailBareIdent(name) => escapes.extend(aliases.get(&name)),
                    ArenaStmtKind::Command(command) => {
                        if let ArenaCommand::Proc { name, .. } = &arena.command_stmt(command).command {
                            escapes.extend(aliases.get(name));
                        }
                    }
                    ArenaStmtKind::Sugar { .. } => arena.for_each_stmt_child(statement, SugarView::Source, &mut push),
                    _ => {}
                }
                arena.for_each_stmt_child(statement, SugarView::Core, push);
            }
            ArenaChild::Expr(expression) => match arena.expr_kind(expression) {
                ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name }
                    if let ArenaExprKind::Ident(namespace) = arena.expr_kind(base)
                        && let Some(&module) = aliases.get(&namespace) =>
                {
                    if !modules[module].exports.contains(&name) {
                        escapes.push(module);
                    }
                }
                ArenaExprKind::Ident(name) => escapes.extend(aliases.get(&name)),
                ArenaExprKind::Record(fields) => {
                    for field in arena.record_fields(fields) {
                        if let ArenaRecordFieldKind::Shorthand { name, .. } = field.kind {
                            escapes.extend(aliases.get(&name));
                        }
                    }
                    arena.for_each_expr_child(expression, push);
                }
                _ => arena.for_each_expr_child(expression, push),
            },
            ArenaChild::Block(block) => arena.for_each_block_child(block, push),
            ArenaChild::Pattern(pattern) => arena.for_each_pattern_child(pattern, push),
            ArenaChild::BindingTarget(target) => arena.for_each_binding_target_child(target, push),
            ArenaChild::AssignTarget(target) => arena.for_each_assign_target_child(target, push),
            ArenaChild::TypeExpr(ty) => arena.for_each_type_expr_child(ty, push),
        }
    }
}
