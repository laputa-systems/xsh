#![allow(clippy::single_call_fn)]

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, FixHint, Label, Severity};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::symbols::Symbol;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaEnvAssignment,
    ArenaEnvAssignmentValue, ArenaExpr, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart,
    ArenaFunctionDef, ArenaMatchExprArm, ArenaModuleContractEntryKind, ArenaPatternKind,
    ArenaPipeStage, ArenaPipeStageKind, ArenaProgram, ArenaRange, ArenaRecordField,
    ArenaRecordFieldKind, ArenaRedirection, ArenaRedirectionTarget, ArenaSpawnTarget,
    ArenaStmtKind, ArenaStreamStage, ArenaTypeDefBody, ArenaTypeExprTag, ArenaWordPart,
    AssignTargetId, AstArena, BindingTargetId, BlockId, BuilderBlockId, CommandStmtId, ExprId,
    FunctionDefId, PatternId, RunFormId, StmtId, TypeExprId,
};
use xsh::frontend::syntax::node::{
    AssignOp, BinaryOp, CoreCommand, Effect, RunKind, StreamStageKind, UnaryOp,
    parse_command_word_reference,
};

fn insertion_sort_by<T>(items: &mut [T], mut compare: impl FnMut(&T, &T) -> std::cmp::Ordering) {
    for index in 1..items.len() {
        let mut current = index;
        while current > 0 && compare(&items[current], &items[current - 1]).is_lt() {
            items.swap(current, current - 1);
            current -= 1;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LintOutput {
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Default)]
pub struct LintOptions {
    pub runless: bool,
    pub runless_except: Vec<String>,
    pub interactive_command_replacement: Option<fn(&str) -> Option<&'static str>>,
    pub expr_types: BTreeMap<Span, Type>,
    pub callable_effects: FxHashMap<String, Option<Vec<Effect>>>,
}

#[derive(Clone, Debug)]
struct Binding {
    span: Span,
    used: bool,
    report_unused: bool,
}

pub struct Linter<'a> {
    arena: &'a AstArena,
    source: &'a str,
    runless: bool,
    runless_except: Vec<String>,
    interactive_command_replacement: Option<fn(&str) -> Option<&'static str>>,
    scopes: Vec<FxHashMap<String, Binding>>,
    diagnostics: Vec<Diagnostic>,
    expr_types: BTreeMap<Span, Type>,
    result_unit_functions: Vec<bool>,
    result_path_functions: Vec<bool>,
    result_return_ok_types: Vec<Option<Type>>,
    function_return_types: Vec<Type>,
    function_effects: FxHashMap<String, Option<Vec<Effect>>>,
    tag_variants: FxHashSet<String>,
    type_declarations: FxHashMap<String, Span>,
    used_type_names: FxHashSet<String>,
    assigned_names: FxHashSet<Name>,
}

/// A decoded type expression node, mirroring the arena's compact type-expr
/// encoding without referencing the old recursive AST.
enum ArenaTypeExprKind {
    Named(Name),
    Qualified,
    List(TypeExprId),
    Map(TypeExprId),
    Stream(TypeExprId),
    Module(TypeExprId),
    Result {
        ok: TypeExprId,
        err: Option<TypeExprId>,
    },
    Optional(TypeExprId),
}

fn type_expr_kind(arena: &AstArena, id: TypeExprId) -> ArenaTypeExprKind {
    let index = id.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Named => {
            ArenaTypeExprKind::Named(Name::from_symbol(Symbol::from_raw(data.lhs)))
        }
        ArenaTypeExprTag::Qualified => ArenaTypeExprKind::Qualified,
        ArenaTypeExprTag::List => {
            ArenaTypeExprKind::List(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Map => ArenaTypeExprKind::Map(TypeExprId::from_index(data.lhs as usize)),
        ArenaTypeExprTag::Stream => {
            ArenaTypeExprKind::Stream(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Module => {
            ArenaTypeExprKind::Module(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Result => ArenaTypeExprKind::Result {
            ok: TypeExprId::from_index(data.lhs as usize),
            err: TypeExprId::from_optional_raw(data.rhs),
        },
        ArenaTypeExprTag::Optional => {
            ArenaTypeExprKind::Optional(TypeExprId::from_index(data.lhs as usize))
        }
    }
}

impl<'a> Linter<'a> {
    pub fn lint(program: &'a ArenaProgram, source: &'a str, options: LintOptions) -> LintOutput {
        let mut linter = Self {
            arena: &program.arena,
            source,
            runless: options.runless,
            runless_except: options.runless_except,
            interactive_command_replacement: options.interactive_command_replacement,
            scopes: vec![FxHashMap::default()],
            diagnostics: Vec::new(),
            expr_types: options.expr_types,
            result_unit_functions: Vec::new(),
            result_path_functions: Vec::new(),
            result_return_ok_types: Vec::new(),
            function_return_types: Vec::new(),
            function_effects: options.callable_effects,
            tag_variants: FxHashSet::default(),
            type_declarations: FxHashMap::default(),
            used_type_names: FxHashSet::default(),
            assigned_names: FxHashSet::default(),
        };
        linter.define(
            "args",
            Span::new(xsh::frontend::source::SourceId::new(0), 0, 0),
            false,
        );
        linter.define(
            "ARGV",
            Span::new(xsh::frontend::source::SourceId::new(0), 0, 0),
            false,
        );
        let statements: Vec<StmtId> = program.statement_ids().collect();
        linter.lint_program(&statements);
        LintOutput {
            diagnostics: linter.diagnostics,
        }
    }

    fn lint_program(&mut self, statements: &[StmtId]) {
        self.collect_assigned_names(statements);
        self.lint_import_blocks(statements);
        self.lint_top_level_const_order(statements);
        for &stmt_id in statements {
            let stmt = self.arena.stmt(stmt_id);
            let (inner_id, inner, is_exported) = match &stmt.kind {
                ArenaStmtKind::Export(inner) => (*inner, self.arena.stmt(*inner).kind, true),
                _ => (stmt_id, stmt.kind.clone(), false),
            };
            let _ = inner_id;
            match &inner {
                ArenaStmtKind::TypeDef(def_id) => {
                    let def = self.arena.type_def(*def_id).clone();
                    self.define(def.name.as_str().as_str(), stmt.span, false);
                    if !def.name.as_str().starts_with('_') && !is_exported {
                        self.type_declarations
                            .insert(def.name.to_string(), stmt.span);
                    }
                    if let ArenaTypeDefBody::TagUnion(variants) = &def.body {
                        self.lint_single_line_tag_union(
                            stmt.span,
                            def.name.as_str().as_str(),
                            *variants,
                        );
                        for variant in self.arena.tag_variants(*variants) {
                            if variant.fields.is_empty() {
                                self.tag_variants.insert(variant.name.to_string());
                            }
                        }
                    }
                    self.collect_type_def_refs(&def.body);
                }
                ArenaStmtKind::ProcDef(def_id)
                | ArenaStmtKind::StreamDef(def_id)
                | ArenaStmtKind::PureDef(def_id) => {
                    let name = self.arena.function_def(*def_id).name;
                    self.define(name.as_str().as_str(), stmt.span, false);
                }
                _ => {}
            }
        }
        self.lint_list_comp_suggestions(statements);
        self.lint_stream_producer_suggestions(statements);
        for &stmt_id in statements {
            self.lint_stmt(stmt_id, false);
        }
        self.lint_implicit_main(statements);
        self.lint_unused_types();
    }

    fn collect_assigned_names(&mut self, statements: &[StmtId]) {
        for &stmt in statements {
            self.collect_assigned_names_stmt(stmt);
        }
    }

    fn collect_assigned_names_block(&mut self, block: BlockId) {
        let statements = self.arena.block(block).statements;
        for stmt in self.arena.stmt_ids(statements).collect::<Vec<_>>() {
            self.collect_assigned_names_stmt(stmt);
        }
    }

    fn collect_assigned_names_stmt(&mut self, stmt_id: StmtId) {
        match self.arena.stmt(stmt_id).kind {
            ArenaStmtKind::Export(inner) | ArenaStmtKind::GuardedStmt { stmt: inner, .. } => {
                self.collect_assigned_names_stmt(inner)
            }
            ArenaStmtKind::Assign { target, .. } => self.collect_assigned_names_target(target),
            ArenaStmtKind::ProcDef(def)
            | ArenaStmtKind::PureDef(def)
            | ArenaStmtKind::StreamDef(def) => {
                self.collect_assigned_names_block(self.arena.function_def(def).body);
            }
            ArenaStmtKind::SignalHook(hook) => {
                self.collect_assigned_names_block(self.arena.signal_hook(hook).body);
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                for branch in self.arena.if_branches(branches).to_vec() {
                    self.collect_assigned_names_block(branch.block);
                }
                if let Some(block) = else_block {
                    self.collect_assigned_names_block(block);
                }
            }
            ArenaStmtKind::While { block, .. }
            | ArenaStmtKind::For { block, .. }
            | ArenaStmtKind::Loop { block } => self.collect_assigned_names_block(block),
            ArenaStmtKind::With {
                body, else_block, ..
            } => {
                self.collect_assigned_names_block(body);
                self.collect_assigned_names_block(else_block);
            }
            ArenaStmtKind::Guard { else_block, .. } => {
                self.collect_assigned_names_block(else_block);
            }
            ArenaStmtKind::Match { arms, .. } => {
                for arm in self.arena.match_arms(arms).to_vec() {
                    self.collect_assigned_names_block(arm.block);
                }
            }
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::Let { .. }
            | ArenaStmtKind::Var { .. }
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::Defer(_)
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::Command(_)
            | ArenaStmtKind::TailBareIdent(_)
            | ArenaStmtKind::Expr(_) => {}
        }
    }

    fn collect_assigned_names_target(&mut self, target: AssignTargetId) {
        match self.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(name) => {
                self.assigned_names.insert(name);
            }
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => {
                self.collect_assigned_names_target(base);
            }
        }
    }

    fn lint_import_blocks(&mut self, statements: &[StmtId]) {
        let mut index = 0;
        while index < statements.len() {
            if !matches!(
                self.arena.stmt(statements[index]).kind,
                ArenaStmtKind::Use(_)
            ) {
                index += 1;
                continue;
            }
            let start = index;
            while index < statements.len()
                && matches!(
                    self.arena.stmt(statements[index]).kind,
                    ArenaStmtKind::Use(_)
                )
            {
                index += 1;
            }
            self.lint_import_block(&statements[start..index]);
        }
    }

    fn lint_import_block(&mut self, block: &[StmtId]) {
        if block.len() < 2 {
            return;
        }
        let original = block
            .iter()
            .map(|&id| import_sort_key(self.arena, id))
            .collect::<Vec<_>>();
        let mut sorted = original.clone();
        insertion_sort_by(&mut sorted, |left, right| left.cmp(right));
        if sorted == original {
            return;
        }

        let first_span = self.arena.stmt(block[0]).span;
        let last_span = self.arena.stmt(block[block.len() - 1]).span;
        let start = first_span.start();
        let end = last_span.end();
        let span = Span::new(first_span.source_id, start, end);
        let has_comment = self.source[start..end].contains('#');
        let mut diagnostic = Diagnostic::new(Severity::Warning, "import block is not sorted")
            .with_code("lint.unsorted-imports")
            .with_label(Label::secondary(
                span,
                "sort this contiguous import block by module path and alias",
            ));
        if has_comment {
            diagnostic =
                diagnostic.with_note("comments in the import block make this a manual fix for now");
        } else {
            let replacement = sorted
                .iter()
                .map(|key| import_text(&key.path, key.alias.as_deref()))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "sort import block",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_top_level_const_order(&mut self, statements: &[StmtId]) {
        let mut saw_function = false;
        for &stmt_id in statements {
            let phase = top_level_phase(self.arena, stmt_id, self.source);
            if phase == TopLevelPhase::SafeConst && saw_function {
                self.diagnostics.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "top-level constant should be grouped after imports",
                    )
                    .with_code("lint.organize-top-level-consts")
                    .with_label(Label::secondary(
                        self.arena.stmt(stmt_id).span,
                        "move this safe immutable binding before top-level functions",
                    )),
                );
            }
            if phase == TopLevelPhase::Function {
                saw_function = true;
            }
        }
    }

    fn lint_unused_types(&mut self) {
        let mut unused: Vec<_> = self
            .type_declarations
            .iter()
            .filter(|(name, _)| !self.used_type_names.contains(*name))
            .map(|(name, &span)| (name.clone(), span))
            .collect();
        insertion_sort_by(&mut unused, |(_, left), (_, right)| {
            left.start().cmp(&right.start())
        });
        for (name, span) in unused {
            let deletion_span = scan_return_stmt_span(self.source, span);
            self.diagnostics.push(
                xsh::diagnostic::Diagnostic::new(
                    xsh::diagnostic::Severity::Warning,
                    format!("unused type declaration `{name}`"),
                )
                .with_code("lint.unused-type")
                .with_label(xsh::diagnostic::Label::secondary(
                    span,
                    "type is declared but never referenced",
                ))
                .with_fix_hint(
                    FixHint::deletion(
                        deletion_span,
                        "remove unused type declaration (apply manually)",
                    )
                    .dangerous(),
                ),
            );
        }
    }

    fn collect_type_expr_refs(&mut self, ty: TypeExprId) {
        match type_expr_kind(self.arena, ty) {
            ArenaTypeExprKind::Named(name) => {
                self.used_type_names.insert(name.to_string());
            }
            ArenaTypeExprKind::Qualified => {}
            ArenaTypeExprKind::List(inner)
            | ArenaTypeExprKind::Map(inner)
            | ArenaTypeExprKind::Stream(inner)
            | ArenaTypeExprKind::Module(inner)
            | ArenaTypeExprKind::Optional(inner) => self.collect_type_expr_refs(inner),
            ArenaTypeExprKind::Result { ok, err } => {
                self.collect_type_expr_refs(ok);
                if let Some(err) = err {
                    self.collect_type_expr_refs(err);
                }
            }
        }
    }

    fn collect_type_def_refs(&mut self, body: &ArenaTypeDefBody) {
        match body {
            ArenaTypeDefBody::Alias(ty) => self.collect_type_expr_refs(*ty),
            ArenaTypeDefBody::RecordSchema(fields) => {
                for field in self.arena.schema_fields(*fields).to_vec() {
                    self.collect_type_expr_refs(field.ty);
                }
            }
            ArenaTypeDefBody::ModuleContract(entries) => {
                for entry in self.arena.module_contract_entries(*entries).to_vec() {
                    match &entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => self.collect_type_expr_refs(*ty),
                        ArenaModuleContractEntryKind::Proc {
                            params, return_ty, ..
                        }
                        | ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            for param in self.arena.params(*params).to_vec() {
                                self.collect_type_expr_refs(param.ty);
                            }
                            self.collect_type_expr_refs(*return_ty);
                        }
                    }
                }
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                for variant in self.arena.tag_variants(*variants).to_vec() {
                    for ty in self.arena.extra_range(variant.fields).to_vec() {
                        self.collect_type_expr_refs(TypeExprId::from_index(ty as usize));
                    }
                }
            }
        }
    }

    fn lint_implicit_main(&mut self, statements: &[StmtId]) {
        let has_main_proc = statements.iter().any(|&stmt_id| {
            let stmt = self.arena.stmt(stmt_id);
            let inner = match &stmt.kind {
                ArenaStmtKind::Export(inner) => self.arena.stmt(*inner).kind,
                _ => stmt.kind,
            };
            matches!(&inner, ArenaStmtKind::ProcDef(def_id) if self.arena.function_def(*def_id).name == "main")
        });
        if !has_main_proc {
            return;
        }
        let Some(&last_id) = statements.last() else {
            return;
        };
        let last_stmt = self.arena.stmt(last_id);
        // Check if the last statement is `main(@args)` — parses as an expr statement wrapping a call.
        let is_explicit_call = match &last_stmt.kind {
            ArenaStmtKind::Expr(expr_id) => match self.arena.expr(*expr_id).kind {
                ArenaExprKind::Call { callee, args } => {
                    matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(n) if n == "main")
                        && args.len() == 1
                        && matches!(
                            &self.arena.call_args(args)[0].kind,
                            ArenaCallArgKind::Splice { value, .. } if matches!(self.arena.expr(*value).kind, ArenaExprKind::Ident(n) if n == "args")
                        )
                }
                _ => false,
            },
            _ => false,
        };
        if !is_explicit_call {
            return;
        }
        let deletion_span = scan_return_stmt_span(self.source, last_stmt.span);
        self.diagnostics.push(
            xsh::diagnostic::Diagnostic::new(
                xsh::diagnostic::Severity::Warning,
                "redundant `main(@args)` — main is invoked implicitly",
            )
            .with_code("lint.redundant-main-call")
            .with_label(xsh::diagnostic::Label::secondary(
                last_stmt.span,
                "main is called automatically after all top-level statements run",
            ))
            .with_fix_hint(FixHint::deletion(
                deletion_span,
                "remove explicit invocation — main is called implicitly",
            )),
        );
    }

    fn lint_stmt(&mut self, stmt_id: StmtId, exported: bool) {
        let stmt = self.arena.stmt(stmt_id);
        match stmt.kind {
            ArenaStmtKind::Use(_) | ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_) => {}
            ArenaStmtKind::Export(inner) => self.lint_stmt(inner, true),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            } => {
                self.lint_empty_map_initializer(ty, &initializer);
                if let Some(type_expr) = ty {
                    self.collect_type_expr_refs(type_expr);
                    self.lint_needless_annotation(target, false, type_expr, &initializer, exported);
                }
                self.lint_expr_or_run(&initializer);
                self.define_binding_target(target, stmt.span, true);
            }
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                self.lint_empty_map_initializer(ty, &initializer);
                if let Some(type_expr) = ty {
                    self.collect_type_expr_refs(type_expr);
                    self.lint_needless_annotation(target, true, type_expr, &initializer, exported);
                }
                self.lint_expr_or_run(&initializer);
                self.define_binding_target(target, stmt.span, true);
            }
            ArenaStmtKind::Assign { target, value, .. } => {
                self.lint_assign_target(target);
                self.lint_expr_or_run(&value);
            }
            ArenaStmtKind::ProcDef(def) => {
                self.lint_proc_function(def, exported);
                self.lint_effect_annotation(def, stmt.span);
            }
            ArenaStmtKind::PureDef(def) => self.lint_function(def),
            ArenaStmtKind::StreamDef(def) => {
                self.lint_proc_function(def, exported);
                self.lint_effect_annotation(def, stmt.span);
            }
            ArenaStmtKind::SignalHook(hook_id) => {
                let body = self.arena.signal_hook(hook_id).body;
                self.lint_block(body);
            }
            ArenaStmtKind::Return(value) => {
                if let Some(value) = value {
                    self.lint_return_value(&value);
                    self.lint_return_path_parse_roundtrip(&value);
                    self.lint_return_redundant_require(&value);
                    self.lint_expr_or_run(&value);
                }
            }
            ArenaStmtKind::Defer(value) => self.lint_expr_or_run(&value),
            ArenaStmtKind::Yield(value) => self.lint_expr_or_run(&value),
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                self.lint_if_as_guard(branches, else_block, stmt.span);
                for branch in self.arena.if_branches(branches).to_vec() {
                    self.lint_expr(branch.condition);
                    self.lint_block(branch.block);
                }
                if let Some(block) = else_block {
                    self.lint_block(block);
                }
            }
            ArenaStmtKind::While { condition, block } => {
                self.lint_expr(condition);
                self.lint_block(block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                self.lint_prefer_file_lines(iter);
                self.lint_expr(iter);
                self.push_scope();
                self.define_binding_target(target, stmt.span, true);
                self.lint_block_statements(block);
                self.pop_scope();
            }
            ArenaStmtKind::Break { value } => {
                if let Some(expr) = value {
                    self.lint_expr(expr);
                }
            }
            ArenaStmtKind::Continue => {}
            ArenaStmtKind::Loop { block } => self.lint_block(block),
            ArenaStmtKind::Guard {
                target,
                initializer,
                else_block,
                ..
            } => {
                self.lint_expr_or_run(&initializer);
                self.define_binding_target(target, stmt.span, true);
                self.lint_block(else_block);
            }
            ArenaStmtKind::GuardedStmt {
                stmt: inner,
                condition,
                ..
            } => {
                self.lint_expr(condition);
                self.lint_stmt(inner, false);
            }
            ArenaStmtKind::Match { value, arms } => {
                self.lint_expr(value);
                for arm in self.arena.match_arms(arms).to_vec() {
                    self.push_scope();
                    self.lint_pattern(arm.pattern);
                    if let Some(guard) = arm.guard {
                        self.lint_expr(guard);
                    }
                    self.lint_block_statements(arm.block);
                    self.pop_scope();
                }
                let str_literal_arms = self
                    .arena
                    .match_arms(arms)
                    .iter()
                    .filter(|arm| {
                        matches!(
                            &self.arena.pattern(arm.pattern).kind,
                            ArenaPatternKind::Literal(expr) if matches!(self.arena.expr(*expr).kind, ArenaExprKind::Str(_))
                        )
                    })
                    .count();
                let has_catch_all = self.arena.match_arms(arms).iter().any(|arm| {
                    matches!(
                        self.arena.pattern(arm.pattern).kind,
                        ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_)
                    )
                });
                if str_literal_arms >= 3 && !has_catch_all {
                    self.warning(
                        stmt.span,
                        "3+ string-literal match arms — consider defining a tag union type",
                        "lint.stringly-typed-match",
                        "tag unions are safer and exhaustiveness-checked",
                    );
                }
            }
            ArenaStmtKind::Command(command) => self.lint_command_stmt(command),
            ArenaStmtKind::TailBareIdent(name) => self.mark_used(name.as_str().as_str()),
            ArenaStmtKind::Expr(expr) => self.lint_expr(expr),
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
                ..
            } => {
                for binding in self.arena.with_bindings(bindings).to_vec() {
                    self.lint_expr_or_run(&ArenaExprOrRun::Expr(binding.initializer));
                }
                self.lint_block(body);
                self.lint_block(else_block);
            }
        }
    }

    fn lint_proc_function(&mut self, def_id: FunctionDefId, exported: bool) {
        let def = self.arena.function_def(def_id).clone();
        if !def.return_ty_defaulted && !exported && result_unit_type_expr(self.arena, def.return_ty)
        {
            let ty_span = self.arena.type_expr_span(def.return_ty);
            let deletion_start = scan_before_arrow(self.source, ty_span.start());
            let deletion_span = Span::new(ty_span.source_id, deletion_start, ty_span.end());
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "redundant `Result[Unit]` return annotation",
                )
                .with_code("lint.redundant-result-unit")
                .with_label(Label::secondary(
                    ty_span,
                    "annotation-free procs default to `Result[Unit]`",
                ))
                .with_fix_hint(FixHint::deletion(
                    deletion_span,
                    "remove return type annotation",
                )),
            );
        }
        self.lint_function(def_id);
    }

    fn lint_effect_annotation(&mut self, def_id: FunctionDefId, stmt_span: Span) {
        let def = self.arena.function_def(def_id).clone();
        let mut effects = FxHashSet::default();
        collect_block_effects(
            self.arena,
            def.body,
            &mut effects,
            Some(&self.function_effects),
        );
        if effects.is_empty() {
            return;
        }
        if let Some(declared_range) = def.effects {
            let declared: Vec<Effect> = self.arena.effects(declared_range).collect();
            let missing = effects
                .iter()
                .filter(|effect| !effects_covers_any(&declared, effect))
                .cloned()
                .collect::<Vec<_>>();
            if missing.is_empty() {
                return;
            }
            let mut union = FxHashSet::default();
            for effect in &declared {
                union.insert(effect.clone());
            }
            for effect in missing {
                union.insert(effect);
            }
            let annotation = effects_annotation(&union);
            let Some(effect_span) = scan_effect_list_span(self.arena, &def, stmt_span, self.source)
            else {
                return;
            };
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!("proc `{}` is missing declared effects", def.name),
                )
                .with_code("lint.missing-effects")
                .with_label(Label::secondary(
                    stmt_span,
                    format!("suggest [{annotation}]"),
                ))
                .with_fix_hint(FixHint::replacement(
                    effect_span,
                    format!("replace effect annotation with `[{annotation}]`"),
                    format!("[{annotation}]"),
                )),
            );
            return;
        }
        let annotation = effects_annotation(&effects);
        let body_span = self.arena.block(def.body).span;
        let body_span = self.arena.span(body_span);
        let (insert_pos, source_id) = if def.return_ty_defaulted {
            (body_span.start(), body_span.source_id)
        } else {
            let return_ty_span = self.arena.type_expr_span(def.return_ty);
            let pos = scan_before_arrow(self.source, return_ty_span.start());
            (pos, return_ty_span.source_id)
        };
        let insert_span = Span::new(source_id, insert_pos, insert_pos);
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("proc `{}` has effects but no annotation", def.name),
            )
            .with_code("lint.unannotated-effects")
            .with_label(Label::secondary(
                stmt_span,
                format!("suggest [{annotation}]"),
            ))
            .with_fix_hint(FixHint::replacement(
                insert_span,
                format!("add effect annotation `[{annotation}]`"),
                format!("[{annotation}] "),
            )),
        );
    }

    fn lint_function(&mut self, def_id: FunctionDefId) {
        let def = self.arena.function_def(def_id).clone();
        self.push_scope();
        let result_unit = result_unit_type_expr(self.arena, def.return_ty);
        let result_path = result_path_type_expr(self.arena, def.return_ty);
        let result_ok = result_ok_type_expr(self.arena, def.return_ty);
        let return_ty = Type::from_arena(self.arena, def.return_ty);
        self.result_unit_functions.push(result_unit);
        self.result_path_functions.push(result_path);
        self.result_return_ok_types.push(result_ok.clone());
        self.function_return_types.push(return_ty);
        if result_unit {
            self.lint_redundant_bare_return(def.body);
        }
        if result_path {
            self.lint_tail_path_parse_roundtrip(def.body);
        }
        if result_ok.is_some() {
            self.lint_tail_redundant_require(def.body);
            self.lint_tail_redundant_ok_return(def.body, result_ok.as_ref());
        }
        self.lint_redundant_tail_return_binding(def.body);
        if !def.return_ty_defaulted {
            self.collect_type_expr_refs(def.return_ty);
        }
        for param in self.arena.params(def.params).to_vec() {
            if !param.ty_defaulted {
                self.collect_type_expr_refs(param.ty);
            }
            if let Some(default) = param.default {
                self.lint_expr(default);
            }
            self.define(
                param.name.as_str().as_str(),
                self.arena.span(param.span),
                true,
            );
        }
        self.lint_block_statements(def.body);
        self.lint_unreachable_trailing_return(def.body);
        self.result_unit_functions.pop();
        self.result_path_functions.pop();
        self.result_return_ok_types.pop();
        self.function_return_types.pop();
        self.pop_scope();
    }

    fn lint_redundant_bare_return(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let last_stmt = self.arena.stmt(last_id);
        if matches!(last_stmt.kind, ArenaStmtKind::Return(None)) {
            let deletion_span = scan_return_stmt_span(self.source, last_stmt.span);
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "redundant `return` at end of `Result[Unit]` function",
                )
                .with_code("lint.redundant-bare-return")
                .with_label(Label::secondary(
                    last_stmt.span,
                    "falling off the end also returns `Ok()`",
                ))
                .with_fix_hint(FixHint::deletion(deletion_span, "remove trailing `return`")),
            );
        }
    }

    fn lint_tail_path_parse_roundtrip(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(last_id).kind else {
            return;
        };
        self.lint_result_path_parse_roundtrip(expr);
    }

    fn lint_tail_redundant_require(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(last_id).kind else {
            return;
        };
        self.lint_result_redundant_require(expr);
    }

    fn lint_return_path_parse_roundtrip(&mut self, value: &ArenaExprOrRun) {
        if !self.result_path_functions.last().copied().unwrap_or(false) {
            return;
        }
        let ArenaExprOrRun::Expr(expr) = value else {
            return;
        };
        self.lint_result_path_parse_roundtrip(*expr);
    }

    fn lint_return_redundant_require(&mut self, value: &ArenaExprOrRun) {
        let ArenaExprOrRun::Expr(expr) = value else {
            return;
        };
        self.lint_result_redundant_require(*expr);
    }

    fn lint_path_roundtrip(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        if !matches!(self.expr_types.get(&expr_span), Some(Type::Path)) {
            return;
        }
        let Some((label_span, replacement)) = self.path_roundtrip_replacement(expr) else {
            return;
        };
        self.push_path_roundtrip_diagnostic(expr_span, label_span, replacement);
    }

    fn lint_redundant_require(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Try(inner) = arena_expr.kind else {
            return;
        };
        let Some((label_span, replacement)) = self.require_replacement(inner) else {
            return;
        };
        self.push_redundant_require_diagnostic(arena_expr.span, label_span, replacement);
    }

    fn lint_result_redundant_require(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        let Some(expected_ok) = self
            .result_return_ok_types
            .last()
            .and_then(|ty| ty.as_ref())
        else {
            return;
        };
        let Some(Type::Result(ok, _)) = self.expr_types.get(&expr_span) else {
            return;
        };
        if !ok.matches_expected(expected_ok) {
            return;
        }
        let Some((label_span, replacement)) = self.require_replacement(expr) else {
            return;
        };
        self.push_redundant_require_diagnostic(expr_span, label_span, replacement);
    }

    fn lint_redundant_single_interpolation(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        let Some((label_span, replacement, code, message)) =
            self.single_interpolation_replacement(expr)
        else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, message)
                .with_code(code)
                .with_label(Label::secondary(
                    label_span,
                    "the interpolation already has the target type",
                ))
                .with_fix_hint(FixHint::replacement(
                    expr_span,
                    "use the interpolated expression directly",
                    replacement,
                )),
        );
    }

    fn lint_scalar_display_parse_roundtrip(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Try(inner) = arena_expr.kind else {
            return;
        };
        let Some((label_span, replacement)) = self.scalar_display_parse_replacement(inner) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "redundant display/parse round trip")
                .with_code("lint.redundant-display-parse")
                .with_label(Label::secondary(
                    label_span,
                    "this value already has the parsed type",
                ))
                .with_fix_hint(FixHint::replacement(
                    arena_expr.span,
                    "use the original value",
                    replacement,
                )),
        );
    }

    fn lint_redundant_newline_triple_string(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Str(value_id) = arena_expr.kind else {
            return;
        };
        let value = self.arena.string_literal(value_id).clone();
        if value.as_ref() != "\n" {
            return;
        }
        let expr_span = arena_expr.span;
        let Some(source) = self.source.get(expr_span.range()) else {
            return;
        };
        if source != "\"\"\"\n\"\"\"" && source != "\"\"\"\r\n\"\"\"" {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "single-newline triple string can be written as `\"\\n\"`",
            )
            .with_code("lint.redundant-newline-triple-string")
            .with_label(Label::secondary(
                expr_span,
                "use the escaped newline literal",
            ))
            .with_fix_hint(FixHint::replacement(
                expr_span,
                "replace with `\"\\n\"`",
                "\"\\n\"".to_string(),
            )),
        );
    }

    fn lint_dollar_in_expression_string(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Str(_) = arena_expr.kind else {
            return;
        };
        let expr_span = arena_expr.span;
        let Some(source) = self.source.get(expr_span.range()) else {
            return;
        };
        // Raw strings are the documented way to keep `$` as literal text; the
        // trap is a plain `$name` inside an ordinary (non-raw) expression string.
        if source.starts_with('r') {
            return;
        }
        let bytes = source.as_bytes();
        let mut index = 0;
        while index < bytes.len() && bytes[index] == b'"' {
            index += 1;
        }
        let mut end = bytes.len();
        while end > index && bytes[end - 1] == b'"' {
            end -= 1;
        }
        while index < end {
            match bytes[index] {
                b'\\' => {
                    // `\$` is an explicit literal dollar; skip the escape and its
                    // target rather than treating it as an interpolation marker.
                    index += 2;
                }
                b'$' => {
                    let name_start = index + 1;
                    if bytes
                        .get(name_start)
                        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
                    {
                        let mut name_end = name_start;
                        while name_end < end
                            && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
                        {
                            name_end += 1;
                        }
                        let name = &source[name_start..name_end];
                        if self.is_binding_in_scope_or_assigned(name) {
                            let dollar_span = Span::new(
                                expr_span.source_id,
                                expr_span.start() + index,
                                expr_span.start() + name_end,
                            );
                            self.diagnostics.push(
                                Diagnostic::new(
                                    Severity::Warning,
                                    format!(
                                        "expression string literals never interpolate; `${name}` is literal text here"
                                    ),
                                )
                                .with_code("lint.dollar-in-expression-string")
                                .with_label(Label::primary(
                                    dollar_span,
                                    format!(
                                        "use an f-string or `+` concatenation to interpolate `{name}`"
                                    ),
                                ))
                                .with_note(
                                    "write `r\"...\"` or `\\$` when a literal dollar sign is intended",
                                ),
                            );
                        }
                        index = name_end;
                        continue;
                    }
                    index += 1;
                }
                _ => index += 1,
            }
        }
    }

    fn is_binding_in_scope_or_assigned(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .any(|scope| scope.contains_key(name))
            || self
                .assigned_names
                .iter()
                .any(|assigned| assigned.as_str().as_str() == name)
    }

    fn lint_json_encode_decode_roundtrip(&mut self, expr: ExprId) {
        let Some(label_span) = self.json_encode_decode_label(expr) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "JSON encode/decode round trip is usually redundant",
            )
            .with_code("lint.json-roundtrip")
            .with_label(Label::secondary(
                label_span,
                "review whether JSON normalization is intentional",
            )),
        );
    }

    fn lint_empty_map_initializer(&mut self, ty: Option<TypeExprId>, initializer: &ArenaExprOrRun) {
        let is_map = ty
            .is_some_and(|ty| matches!(type_expr_kind(self.arena, ty), ArenaTypeExprKind::Map(_)));
        if !is_map {
            return;
        }
        let ArenaExprOrRun::Expr(expr) = initializer else {
            return;
        };
        if !is_map_empty_call(self.arena, *expr) {
            return;
        }
        let expr_span = self.arena.expr(*expr).span;
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "use `{}` for an empty map")
                .with_code("lint.prefer-empty-map-literal")
                .with_label(Label::secondary(
                    expr_span,
                    "`{}` is the empty-map literal in map-typed contexts",
                ))
                .with_fix_hint(FixHint::replacement(
                    expr_span,
                    "replace `map.empty()` with `{}`",
                    "{}".to_string(),
                )),
        );
    }

    fn lint_needless_annotation(
        &mut self,
        target: BindingTargetId,
        mutable: bool,
        ty: TypeExprId,
        initializer: &ArenaExprOrRun,
        exported: bool,
    ) {
        if mutable
            && let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind
            && self.assigned_names.contains(&name)
        {
            return;
        }
        let annotation_ty = Type::from_arena(self.arena, ty);
        if matches!(annotation_ty, Type::Any | Type::Unknown | Type::Invalid) {
            return;
        }
        let ArenaExprOrRun::Expr(init_expr_id) = initializer else {
            return;
        };
        let init = self.arena.expr(*init_expr_id);
        if !self.annotation_is_needless(&annotation_ty, &init, exported) {
            return;
        }
        if self.annotation_refs_user_type(ty) {
            return;
        }
        let ty_span = self.arena.type_expr_span(ty);
        let deletion_start = scan_before_colon(self.source, ty_span.start());
        let deletion_end = scan_after_type(self.source, ty_span.end());
        if deletion_start >= deletion_end {
            return;
        }
        // Don't fix across comments
        if self.source[deletion_start..deletion_end].contains('#') {
            return;
        }
        let deletion_span = Span::new(ty_span.source_id, deletion_start, deletion_end);
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "needless type annotation")
                .with_code("lint.needless-annotation")
                .with_label(Label::secondary(
                    ty_span,
                    "this type annotation is redundant with the initializer",
                ))
                .with_fix_hint(FixHint::deletion(
                    deletion_span,
                    "remove needless annotation",
                )),
        );
    }

    fn annotation_is_needless(&self, annotation: &Type, init: &ArenaExpr, exported: bool) -> bool {
        if self.is_empty_collection(init) {
            return false;
        }
        let Some(actual) = self.expr_types.get(&init.span) else {
            return false;
        };
        if matches!(actual, Type::Any | Type::Unknown | Type::Invalid) {
            return false;
        }
        if exported {
            *actual == *annotation
        } else {
            actual.matches_expected(annotation) && annotation.matches_expected(actual)
        }
    }

    fn annotation_refs_user_type(&self, ty: TypeExprId) -> bool {
        self.type_expr_refs_user_type(ty)
    }

    fn type_expr_refs_user_type(&self, ty: TypeExprId) -> bool {
        match type_expr_kind(self.arena, ty) {
            ArenaTypeExprKind::Named(name) => {
                self.type_declarations.contains_key(name.as_str().as_str())
            }
            ArenaTypeExprKind::List(inner)
            | ArenaTypeExprKind::Map(inner)
            | ArenaTypeExprKind::Stream(inner)
            | ArenaTypeExprKind::Module(inner)
            | ArenaTypeExprKind::Optional(inner) => self.type_expr_refs_user_type(inner),
            ArenaTypeExprKind::Result { ok, err } => {
                self.type_expr_refs_user_type(ok)
                    || err.is_some_and(|err| self.type_expr_refs_user_type(err))
            }
            ArenaTypeExprKind::Qualified => false,
        }
    }

    fn is_empty_collection(&self, init: &ArenaExpr) -> bool {
        matches!(&init.kind, ArenaExprKind::List(items) if items.is_empty())
            || matches!(&init.kind, ArenaExprKind::Record(fields) if fields.is_empty())
    }

    fn lint_result_path_parse_roundtrip(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        if !matches!(
            self.expr_types.get(&expr_span),
            Some(Type::Result(ok, _)) if matches!(ok.as_ref(), Type::Path)
        ) {
            return;
        }
        let Some((label_span, replacement)) = self.path_parse_result_replacement(expr) else {
            return;
        };
        self.push_path_roundtrip_diagnostic(expr_span, label_span, replacement);
    }

    fn path_roundtrip_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Try(inner) => self.path_parse_result_replacement(inner),
            ArenaExprKind::Binary {
                op: BinaryOp::ResultFallback,
                left,
                ..
            } => self.path_parse_result_replacement(left),
            _ => self.path_constructor_call_replacement(expr),
        }
    }

    fn path_parse_result_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        self.path_parse_call_replacement(expr)
            .or_else(|| self.path_parse_literal_replacement(expr))
    }

    fn path_parse_call_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        if name != "parse"
            || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "Path")
        {
            return None;
        }
        self.path_display_arg_replacement(args)
    }

    fn path_parse_literal_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        if name != "parse"
            || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "Path")
        {
            return None;
        }
        self.path_literal_arg_replacement(args)
    }

    fn path_constructor_call_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        if !matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Path") {
            return None;
        }
        self.path_display_arg_replacement(args)
            .or_else(|| self.single_path_interpolation_arg_replacement(args))
    }

    fn path_display_arg_replacement(&self, args: ArenaRange) -> Option<(Span, String)> {
        let args = self.arena.call_args(args);
        let [arg] = args else {
            return None;
        };
        let ArenaCallArgKind::Positional(arg) = arg.kind else {
            return None;
        };
        let arg_span = self.arena.expr(arg).span;
        let ArenaExprKind::Call {
            callee: display_callee,
            args: display_args,
        } = self.arena.expr(arg).kind
        else {
            return None;
        };
        if !display_args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(display_callee).kind else {
            return None;
        };
        let base_span = self.arena.expr(base).span;
        if name != "display" || !matches!(self.expr_types.get(&base_span), Some(Type::Path)) {
            return None;
        }
        let replacement = self.source.get(base_span.range())?.to_string();
        Some((arg_span, replacement))
    }

    fn path_literal_arg_replacement(&self, args: ArenaRange) -> Option<(Span, String)> {
        let args = self.arena.call_args(args);
        let [arg] = args else {
            return None;
        };
        let ArenaCallArgKind::Positional(arg) = arg.kind else {
            return None;
        };
        let arg_span = self.arena.expr(arg).span;
        match self.arena.expr(arg).kind {
            ArenaExprKind::Str(_) => {
                let literal_text = self.source.get(arg_span.range())?;
                Some((arg_span, format!("p{literal_text}")))
            }
            ArenaExprKind::FmtString(_) => {
                let literal_text = self.source.get(arg_span.range())?;
                Some((arg_span, path_fmt_literal_text(literal_text)?))
            }
            _ => None,
        }
    }

    fn single_path_interpolation_arg_replacement(
        &self,
        args: ArenaRange,
    ) -> Option<(Span, String)> {
        let args = self.arena.call_args(args);
        let [arg] = args else {
            return None;
        };
        let ArenaCallArgKind::Positional(arg) = arg.kind else {
            return None;
        };
        let ArenaExprKind::FmtString(parts) = self.arena.expr(arg).kind else {
            return None;
        };
        self.single_path_interpolation_parts_replacement(parts)
    }

    fn single_path_interpolation_parts_replacement(
        &self,
        parts: ArenaRange,
    ) -> Option<(Span, String)> {
        let inner = single_interpolation_expr(self.arena, parts)?;
        let inner_span = self.arena.expr(inner).span;
        if !matches!(self.expr_types.get(&inner_span), Some(Type::Path)) {
            return None;
        }
        let replacement = self.source.get(inner_span.range())?.to_string();
        Some((inner_span, replacement))
    }

    fn single_interpolation_replacement(
        &self,
        expr: ExprId,
    ) -> Option<(Span, String, &'static str, &'static str)> {
        let expr_span = self.arena.expr(expr).span;
        match self.arena.expr(expr).kind {
            ArenaExprKind::FmtString(parts)
                if matches!(self.expr_types.get(&expr_span), Some(Type::Str)) =>
            {
                let inner = single_interpolation_expr(self.arena, parts)?;
                let inner_span = self.arena.expr(inner).span;
                if !matches!(self.expr_types.get(&inner_span), Some(Type::Str)) {
                    return None;
                }
                let replacement = self.source.get(inner_span.range())?.to_string();
                Some((
                    inner_span,
                    replacement,
                    "lint.redundant-string-interpolation",
                    "redundant single-value string interpolation",
                ))
            }
            ArenaExprKind::PathFmtString(parts)
                if matches!(self.expr_types.get(&expr_span), Some(Type::Path)) =>
            {
                let (label_span, replacement) =
                    self.single_path_interpolation_parts_replacement(parts)?;
                Some((
                    label_span,
                    replacement,
                    "lint.redundant-path-interpolation",
                    "redundant single-value path interpolation",
                ))
            }
            _ => None,
        }
    }

    fn command_single_fmt_replacement(&self, expr: ExprId) -> Option<String> {
        let ArenaExprKind::FmtString(parts) = self.arena.expr(expr).kind else {
            return None;
        };
        let inner = single_interpolation_expr(self.arena, parts)?;
        if simple_command_value_expr(self.arena, inner) {
            return Some(command_value_replacement(self.arena, inner));
        }
        let ArenaExprKind::Call { callee, args } = self.arena.expr(inner).kind else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        let base_span = self.arena.expr(base).span;
        if name != "display"
            || !matches!(self.expr_types.get(&base_span), Some(Type::Path))
            || !simple_command_value_expr(self.arena, base)
        {
            return None;
        }
        Some(command_value_replacement(self.arena, base))
    }

    fn require_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let expr_span = self.arena.expr(expr).span;
        let ArenaExprKind::Require { value, .. } = self.arena.expr(expr).kind else {
            return None;
        };
        let Some(Type::Result(ok, _)) = self.expr_types.get(&expr_span) else {
            return None;
        };
        let value_span = self.arena.expr(value).span;
        let value_ty = self.expr_types.get(&value_span)?;
        if value_ty.is_recovery() || value_ty.contains_any() {
            return None;
        }
        if expr_is_dynamic_require_boundary(self.arena, value) {
            return None;
        }
        if !value_ty.matches_expected(ok) {
            return None;
        }
        let replacement = self.source.get(value_span.range())?.to_string();
        Some((value_span, replacement))
    }

    fn scalar_display_parse_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let expr_span = self.arena.expr(expr).span;
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field {
            base: parse_base,
            name: parse_name,
        } = self.arena.expr(callee).kind
        else {
            return None;
        };
        let (expected_input, expected_output) = match parse_name.as_str().as_str() {
            "parse_int" => (Type::Int, Type::Int),
            "parse_float" => (Type::Float, Type::Float),
            _ => return None,
        };
        if !matches!(self.expr_types.get(&expr_span), Some(ty) if ty.matches_expected(&Type::Result(Box::new(expected_output), Box::new(Type::Error))))
        {
            return None;
        }
        if let Some((span, replacement)) =
            self.scalar_display_method_replacement(parse_base, &expected_input)
        {
            return Some((span, replacement));
        }
        self.scalar_fmt_parse_replacement(parse_base, &expected_input)
    }

    fn scalar_display_method_replacement(
        &self,
        expr: ExprId,
        expected_input: &Type,
    ) -> Option<(Span, String)> {
        let expr_span = self.arena.expr(expr).span;
        let ArenaExprKind::Call {
            callee: display_callee,
            args: display_args,
        } = self.arena.expr(expr).kind
        else {
            return None;
        };
        if !display_args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field {
            base: original,
            name: display_name,
        } = self.arena.expr(display_callee).kind
        else {
            return None;
        };
        let original_span = self.arena.expr(original).span;
        if display_name != "display"
            || !matches!(self.expr_types.get(&original_span), Some(ty) if ty.matches_expected(expected_input))
        {
            return None;
        }
        let replacement = self.source.get(original_span.range())?.to_string();
        Some((expr_span, replacement))
    }

    fn scalar_fmt_parse_replacement(
        &self,
        expr: ExprId,
        expected_input: &Type,
    ) -> Option<(Span, String)> {
        let expr_span = self.arena.expr(expr).span;
        let ArenaExprKind::FmtString(parts) = self.arena.expr(expr).kind else {
            return None;
        };
        let original = single_interpolation_expr(self.arena, parts)?;
        let original_span = self.arena.expr(original).span;
        if !matches!(self.expr_types.get(&original_span), Some(ty) if ty.matches_expected(expected_input))
        {
            return None;
        }
        let replacement = self.source.get(original_span.range())?.to_string();
        Some((expr_span, replacement))
    }

    fn json_encode_decode_label(&self, expr: ExprId) -> Option<Span> {
        let ArenaExprKind::Try(decode_call) = self.arena.expr(expr).kind else {
            return None;
        };
        let ArenaExprKind::Call {
            callee: decode_callee,
            args: decode_args,
        } = self.arena.expr(decode_call).kind
        else {
            return None;
        };
        if !is_module_call(self.arena, decode_callee, "json", "decode") {
            return None;
        }
        let [decode_arg] = self.arena.call_args(decode_args) else {
            return None;
        };
        let ArenaCallArgKind::Positional(decode_arg) = decode_arg.kind else {
            return None;
        };
        let ArenaExprKind::Try(encode_call) = self.arena.expr(decode_arg).kind else {
            return None;
        };
        let encode_span = self.arena.expr(encode_call).span;
        let ArenaExprKind::Call {
            callee: encode_callee,
            args: encode_args,
        } = self.arena.expr(encode_call).kind
        else {
            return None;
        };
        if !is_module_call(self.arena, encode_callee, "json", "encode") || encode_args.is_empty() {
            return None;
        }
        Some(encode_span)
    }

    fn push_redundant_require_diagnostic(
        &mut self,
        span: Span,
        label_span: Span,
        replacement: String,
    ) {
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "redundant schema require")
                .with_code("lint.redundant-require")
                .with_label(Label::secondary(
                    label_span,
                    "this expression already has the required type",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "use the already-typed value",
                    replacement,
                )),
        );
    }

    /// If `expr` is a `.display()` call with no args, returns the base expression's span and source text.
    fn path_display_base(&self, expr: ExprId) -> Option<(Span, String)> {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        if name != "display" {
            return None;
        }
        let base_span = self.arena.expr(base).span;
        Some((base_span, self.source.get(base_span.range())?.to_string()))
    }

    fn lint_redundant_path_display(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        let Some((_base_span, replacement)) = self.path_display_base(expr) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "redundant `.display()` on a Path value")
                .with_code("lint.redundant-path-display")
                .with_label(Label::secondary(
                    expr_span,
                    "Path values display automatically in command arguments",
                ))
                .with_fix_hint(FixHint::replacement(
                    expr_span,
                    "remove `.display()`",
                    replacement,
                )),
        );
    }

    fn push_path_roundtrip_diagnostic(
        &mut self,
        span: Span,
        label_span: Span,
        replacement: String,
    ) {
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "redundant path display/parse round trip")
                .with_code("lint.redundant-path-parse")
                .with_label(Label::secondary(
                    label_span,
                    "this value is already a Path before `.display()`",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "use the original Path value",
                    replacement,
                )),
        );
    }

    fn lint_unreachable_trailing_return(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        if stmts.len() < 2 {
            return;
        }
        let len = stmts.len();
        let match_stmt = self.arena.stmt(stmts[len - 2]);
        let return_stmt = self.arena.stmt(stmts[len - 1]);
        let ArenaStmtKind::Match { arms, .. } = match_stmt.kind else {
            return;
        };
        if !matches!(return_stmt.kind, ArenaStmtKind::Return(_)) {
            return;
        }
        if !self
            .arena
            .match_arms(arms)
            .iter()
            .all(|arm| lint_block_always_returns(self.arena, arm.block))
        {
            return;
        }
        let deletion_span = scan_return_stmt_span(self.source, return_stmt.span);
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "unreachable `return` after match with all-returning arms",
            )
            .with_code("lint.unreachable-after-match")
            .with_label(Label::secondary(
                return_stmt.span,
                "every match arm already returns; this is unreachable",
            ))
            .with_fix_hint(FixHint::deletion(
                deletion_span,
                "remove unreachable trailing return",
            )),
        );
    }

    fn lint_redundant_tail_return_binding(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        if stmts.len() < 2 {
            return;
        }
        let len = stmts.len();
        let binding_stmt = self.arena.stmt(stmts[len - 2]);
        let return_stmt = self.arena.stmt(stmts[len - 1]);
        let (target, ty, initializer) = match binding_stmt.kind {
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            } => (target, ty, initializer),
            _ => return,
        };
        let ArenaBindingTargetKind::Name(binding_name) = self.arena.binding_target(target).kind
        else {
            return;
        };
        let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(returned))) = return_stmt.kind else {
            return;
        };
        if !matches!(self.arena.expr(returned).kind, ArenaExprKind::Ident(name) if name == binding_name)
        {
            return;
        }
        let initializer_span = self.arena.expr(initializer).span;
        let replacement = match self.source.get(initializer_span.range()) {
            Some(source) => format!("{source}\n"),
            None => return,
        };
        let replacement_span = Span::new(
            binding_stmt.span.source_id,
            binding_stmt.span.start(),
            span_end_after_following_newlines(self.source, return_stmt.span.end()),
        );
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            format!("tail binding `{binding_name}` can be returned implicitly"),
        )
        .with_code("lint.redundant-tail-return-binding")
        .with_label(Label::secondary(
            return_stmt.span,
            "make the initializer the final expression",
        ));
        let between = &self.source[binding_stmt.span.end()..return_stmt.span.start()];
        if !between.contains('#') && self.tail_return_binding_autofix_safe(ty, initializer) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                replacement_span,
                "replace binding and return with tail expression",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn tail_return_binding_autofix_safe(
        &self,
        annotation: Option<TypeExprId>,
        initializer: ExprId,
    ) -> bool {
        let Some(annotation) = annotation else {
            return true;
        };
        let expected = Type::from_arena(self.arena, annotation);
        if matches!(expected, Type::Any | Type::Unknown | Type::Invalid) {
            return false;
        }
        if self.expr_is_source_empty_list(initializer)
            && matches!(expected, Type::List(_))
            && self
                .function_return_types
                .last()
                .is_some_and(|return_ty| tail_type_matches_lint_expected(return_ty, &expected))
        {
            return true;
        }
        let initializer_span = self.arena.expr(initializer).span;
        let Some(actual) = self.expr_types.get(&initializer_span) else {
            return false;
        };
        actual.matches_expected(&expected) && expected.matches_expected(actual)
    }

    fn expr_is_source_empty_list(&self, expr: ExprId) -> bool {
        let arena_expr = self.arena.expr(expr);
        matches!(&arena_expr.kind, ArenaExprKind::List(items) if items.is_empty())
            || self
                .source
                .get(arena_expr.span.range())
                .is_some_and(|source| source.trim() == "[]")
    }

    fn lint_single_line_tag_union(&mut self, stmt_span: Span, name: &str, variants: ArenaRange) {
        let variant_spans: Vec<Span> = self
            .arena
            .tag_variants(variants)
            .iter()
            .map(|v| self.arena.span(v.span))
            .collect();
        if variant_spans.len() < 3 {
            return;
        }
        let raw_end = stmt_span.end();
        let body_end = if raw_end > 0 && self.source.as_bytes().get(raw_end - 1) == Some(&b'\n') {
            raw_end - 1
        } else {
            raw_end
        };
        let src = &self.source[stmt_span.start()..body_end];
        if src.contains('\n') {
            return;
        }
        let line_start = self.source[..stmt_span.start()]
            .rfind('\n')
            .map_or(0, |p| p + 1);
        let cur_indent = stmt_span.start() - line_start;
        let variant_indent = " ".repeat(cur_indent + 4);
        let cont_prefix = format!("{}  | ", " ".repeat(cur_indent));
        let variant_texts: Vec<&str> = variant_spans
            .iter()
            .map(|v| &self.source[v.start()..v.end()])
            .collect();
        let mut lines = vec![format!(
            "type {name} =\n{variant_indent}{}",
            variant_texts[0]
        )];
        for v_text in &variant_texts[1..] {
            lines.push(format!("{cont_prefix}{v_text}"));
        }
        let replacement = lines.join("\n") + "\n";
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "tag union fits on one line; prefer multi-line for readability",
            )
            .with_code("lint.multiline-tag-union")
            .with_label(Label::secondary(stmt_span, "break across lines"))
            .with_fix_hint(FixHint::replacement(
                stmt_span,
                "reformat to multi-line",
                replacement,
            )),
        );
    }

    fn lint_return_value(&mut self, value: &ArenaExprOrRun) {
        if !self.result_unit_functions.last().copied().unwrap_or(false)
            || !return_value_is_ok_unit(self.arena, value)
        {
            return;
        }
        let val_span = self.expr_or_run_span(value);
        // Include the whitespace before `Ok()` so deletion leaves a clean `return`.
        let deletion_start = scan_back_space(self.source, val_span.start());
        let deletion_span = Span::new(val_span.source_id, deletion_start, val_span.end());
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "redundant `return Ok()` in `Result[Unit]` function",
            )
            .with_code("lint.redundant-ok-return")
            .with_label(Label::secondary(
                val_span,
                "use bare `return`, or omit the final return",
            ))
            .with_fix_hint(FixHint::deletion(deletion_span, "remove `Ok()`")),
        );
    }

    fn lint_tail_redundant_ok_return(&mut self, body: BlockId, expected_ok: Option<&Type>) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let last_stmt = self.arena.stmt(last_id);
        let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) = last_stmt.kind else {
            return;
        };
        let expr_span = self.arena.expr(expr).span;
        let Some(ok_expr) = ok_call_arg(self.arena, expr) else {
            return;
        };
        // Get the full source text of the return value by slicing from
        // `return Ok(` to the matching `)` at the end, then stripping the
        // `return ` prefix for the tail expression. Using the enclosing
        // statement span keeps grouping parens that the expression AST
        // drops (e.g. `Ok((n * 3) + 1)` → `(n * 3) + 1`).
        let replacement = match self.source.get(last_stmt.span.range()) {
            Some(stmt_source) => {
                let inner = stmt_source
                    .strip_prefix("return Ok(")
                    .and_then(|rest| rest.strip_suffix(")\n").or_else(|| rest.strip_suffix(")")));
                match inner {
                    Some(inner) => format!("{inner}\n"),
                    None => return,
                }
            }
            None => return,
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "redundant `return Ok(...)` at function tail",
        )
        .with_code("lint.redundant-ok-tail")
        .with_label(Label::secondary(
            expr_span,
            "plain tail values are wrapped in `Ok(...)` automatically",
        ));
        if self.tail_ok_return_autofix_safe(expected_ok, ok_expr) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                Span::new(
                    last_stmt.span.source_id,
                    last_stmt.span.start(),
                    span_end_after_following_newlines(self.source, last_stmt.span.end()),
                ),
                "use the plain tail value",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn tail_ok_return_autofix_safe(&self, expected_ok: Option<&Type>, ok_expr: ExprId) -> bool {
        let Some(expected_ok) = expected_ok else {
            return false;
        };
        let ok_span = self.arena.expr(ok_expr).span;
        let Some(actual) = self.expr_types.get(&ok_span) else {
            return false;
        };
        actual.matches_expected(expected_ok) && expected_ok.matches_expected(actual)
    }

    fn lint_pattern(&mut self, pattern: PatternId) {
        let arena_pattern = self.arena.pattern(pattern).clone();
        let span = self.arena.span(arena_pattern.span);
        match arena_pattern.kind {
            ArenaPatternKind::Binding(name) => {
                if !self.tag_variants.contains(name.as_str().as_str()) {
                    self.define(name.as_str().as_str(), span, true);
                }
            }
            ArenaPatternKind::Type { binding, ty } => {
                self.collect_type_expr_refs(ty);
                if let Some(name) = binding {
                    self.define(name.as_str().as_str(), span, true);
                }
            }
            ArenaPatternKind::Constructor { arg, .. } => {
                if let Some(arg) = arg {
                    self.lint_pattern(arg);
                }
            }
            ArenaPatternKind::Record { fields, .. } => {
                for field in self.arena.pattern_fields(fields).to_vec() {
                    self.lint_pattern(field.pattern);
                }
            }
            ArenaPatternKind::Alternation(patterns) | ArenaPatternKind::Tuple(patterns) => {
                for pat in self.arena.pattern_ids(patterns).collect::<Vec<_>>() {
                    self.lint_pattern(pat);
                }
            }
            ArenaPatternKind::Wildcard
            | ArenaPatternKind::Literal(_)
            | ArenaPatternKind::ErrorVariant { .. }
            | ArenaPatternKind::Facet(_) => {}
        }
    }

    fn define_binding_target(&mut self, target: BindingTargetId, span: Span, report_unused: bool) {
        match self.arena.binding_target(target).kind.clone() {
            ArenaBindingTargetKind::Name(name) => {
                self.define(name.as_str().as_str(), span, report_unused)
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in self.arena.destructure_fields(fields).to_vec() {
                    self.define(
                        field.name.as_str().as_str(),
                        self.arena.span(field.span),
                        report_unused,
                    );
                }
            }
        }
    }

    fn lint_block(&mut self, block: BlockId) {
        self.push_scope();
        self.lint_block_statements(block);
        self.pop_scope();
    }

    fn lint_block_statements(&mut self, block: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        self.lint_list_comp_suggestions(&stmts);
        for &stmt in &stmts {
            self.lint_stmt(stmt, false);
        }
    }

    fn lint_list_comp_suggestions(&mut self, stmts: &[StmtId]) {
        for pair in stmts.windows(2) {
            self.lint_suggest_list_comp(pair[0], pair[1]);
            self.lint_suggest_map_comp(pair[0], pair[1]);
        }
    }

    fn lint_stream_producer_suggestions(&mut self, stmts: &[StmtId]) {
        let mut candidates = Vec::new();
        collect_stream_producer_candidates(self.arena, stmts, &mut candidates);
        if candidates.is_empty() {
            return;
        }
        let mut consumed = FxHashSet::default();
        collect_lazy_consumed_calls(self.arena, stmts, &mut consumed);
        for candidate in candidates {
            if !consumed.contains(&candidate.function_name) {
                continue;
            }
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "proc `{}` builds list `{}` item-by-item and is consumed lazily; consider a `stream` producer",
                        candidate.function_name, candidate.accumulator_name,
                    ),
                )
                .with_code("lint.prefer-stream-producer")
                .with_label(Label::secondary(
                    candidate.span,
                    "`yield` can avoid materializing this list for direct stream consumers",
                )),
            );
        }
    }

    fn lint_suggest_list_comp(&mut self, var_id: StmtId, for_id: StmtId) {
        let var_stmt = self.arena.stmt(var_id);
        let for_stmt = self.arena.stmt(for_id);
        // Match: var <name> = []
        let ArenaStmtKind::Var {
            target,
            initializer: ArenaExprOrRun::Expr(init),
            ..
        } = var_stmt.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(var_name) = self.arena.binding_target(target).kind else {
            return;
        };
        let ArenaExprKind::List(items) = self.arena.expr(init).kind else {
            return;
        };
        if !items.is_empty() {
            return;
        }
        // Match: for <target> in <iter> { <name> = <name>.push(<push_expr>) }
        // or:    for <target> in <iter> { if <guard> { <name> = <name>.push(<push_expr>) } }
        let ArenaStmtKind::For {
            target: for_target,
            iter,
            block,
        } = for_stmt.kind
        else {
            return;
        };
        let block_data = self.arena.block(block);
        let block_stmts: Vec<StmtId> = self.arena.stmt_ids(block_data.statements).collect();
        if block_stmts.len() != 1 || !block_data.params.is_empty() {
            return;
        }
        let mut condition = None;
        let mut push_stmt_id = block_stmts[0];
        if let ArenaStmtKind::If {
            branches,
            else_block: None,
        } = self.arena.stmt(push_stmt_id).kind
        {
            let branch_slice = self.arena.if_branches(branches);
            if branch_slice.len() != 1 {
                return;
            }
            let branch = branch_slice[0].clone();
            let branch_block = self.arena.block(branch.block);
            let branch_stmts: Vec<StmtId> = self.arena.stmt_ids(branch_block.statements).collect();
            if !branch_block.params.is_empty() || branch_stmts.len() != 1 {
                return;
            }
            condition = Some(branch.condition);
            push_stmt_id = branch_stmts[0];
        }
        let ArenaStmtKind::Assign {
            target: assign_target,
            op: AssignOp::Set,
            value: ArenaExprOrRun::Expr(rhs),
        } = self.arena.stmt(push_stmt_id).kind
        else {
            return;
        };
        let ArenaAssignTargetKind::Name(assign_name) = self.arena.assign_target(assign_target).kind
        else {
            return;
        };
        if assign_name != var_name {
            return;
        }
        let ArenaExprKind::Call { callee, args } = self.arena.expr(rhs).kind else {
            return;
        };
        let ArenaExprKind::Field {
            base,
            name: method_name,
        } = self.arena.expr(callee).kind
        else {
            return;
        };
        if method_name != "push" {
            return;
        }
        let ArenaExprKind::Ident(base_name) = self.arena.expr(base).kind else {
            return;
        };
        if base_name != var_name || args.len() != 1 {
            return;
        }
        let ArenaCallArgKind::Positional(push_expr) = self.arena.call_args(args)[0].kind else {
            return;
        };
        if expr_references_name(self.arena, push_expr, var_name)
            || condition
                .is_some_and(|condition| expr_references_name(self.arena, condition, var_name))
        {
            return;
        }
        let push_span = self.arena.expr(push_expr).span;
        let iter_span = self.arena.expr(iter).span;
        let Some(push_src) = self.source.get(push_span.start()..push_span.end()) else {
            return;
        };
        let Some(iter_src) = self.source.get(iter_span.start()..iter_span.end()) else {
            return;
        };
        let target_src = format_binding_target(self.arena, for_target);
        let replacement = if let Some(condition) = condition {
            let cond_span = self.arena.expr(condition).span;
            let Some(guard_src) = self.source.get(cond_span.start()..cond_span.end()) else {
                return;
            };
            format!("var {var_name} = [{push_src} for {target_src} in {iter_src} if {guard_src}]\n")
        } else {
            format!("var {var_name} = [{push_src} for {target_src} in {iter_src}]\n")
        };
        let combined = Span::new(
            var_stmt.span.source_id,
            var_stmt.span.start(),
            span_end_after_following_newlines(self.source, for_stmt.span.end()),
        );
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!(
                    "use a list comprehension instead of building `{var_name}` with a for loop"
                ),
            )
            .with_code("lint.prefer-list-comp")
            .with_label(Label::secondary(
                for_stmt.span,
                "this for loop only builds a list",
            ))
            .with_fix_hint(FixHint::replacement(
                combined,
                "convert to list comprehension",
                replacement,
            )),
        );
    }

    fn lint_suggest_map_comp(&mut self, var_id: StmtId, for_id: StmtId) {
        let var_stmt = self.arena.stmt(var_id);
        let for_stmt = self.arena.stmt(for_id);
        let ArenaStmtKind::Var {
            target,
            initializer: ArenaExprOrRun::Expr(init),
            ..
        } = var_stmt.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(var_name) = self.arena.binding_target(target).kind else {
            return;
        };
        if !is_map_empty_call(self.arena, init) {
            return;
        }
        let ArenaStmtKind::For {
            target: for_target,
            iter,
            block,
        } = for_stmt.kind
        else {
            return;
        };
        let block_data = self.arena.block(block);
        let block_stmts: Vec<StmtId> = self.arena.stmt_ids(block_data.statements).collect();
        if block_stmts.len() != 1 || !block_data.params.is_empty() {
            return;
        }
        let ArenaStmtKind::Assign {
            target: assign_target,
            op: AssignOp::Set,
            value: ArenaExprOrRun::Expr(value),
        } = self.arena.stmt(block_stmts[0]).kind
        else {
            return;
        };
        let ArenaAssignTargetKind::Index { base, index } =
            self.arena.assign_target(assign_target).kind
        else {
            return;
        };
        let ArenaAssignTargetKind::Name(assign_name) = self.arena.assign_target(base).kind else {
            return;
        };
        if assign_name != var_name {
            return;
        }
        if expr_references_name(self.arena, index, var_name)
            || expr_references_name(self.arena, value, var_name)
        {
            return;
        }
        let index_span = self.arena.expr(index).span;
        let value_span = self.arena.expr(value).span;
        let iter_span = self.arena.expr(iter).span;
        let Some(key_src) = self.source.get(index_span.start()..index_span.end()) else {
            return;
        };
        let Some(value_src) = self.source.get(value_span.start()..value_span.end()) else {
            return;
        };
        let Some(iter_src) = self.source.get(iter_span.start()..iter_span.end()) else {
            return;
        };
        if !map_comp_key_can_be_bare(self.arena, index) {
            return;
        }
        let target_src = format_binding_target(self.arena, for_target);
        let replacement =
            format!("var {var_name} = {{{key_src}: {value_src} for {target_src} in {iter_src}}}\n");
        let combined = Span::new(
            var_stmt.span.source_id,
            var_stmt.span.start(),
            span_end_after_following_newlines(self.source, for_stmt.span.end()),
        );
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("use a map comprehension instead of building `{var_name}` with a for loop"),
            )
            .with_code("lint.prefer-map-comp")
            .with_label(Label::secondary(
                for_stmt.span,
                "this for loop only builds a map",
            ))
            .with_fix_hint(FixHint::replacement(
                combined,
                "convert to map comprehension",
                replacement,
            )),
        );
    }

    fn lint_command_stmt(&mut self, stmt_id: CommandStmtId) {
        let stmt = self.arena.command_stmt(stmt_id).clone();
        let span = self.arena.span(stmt.span);
        match stmt.command {
            ArenaCommand::Proc { name, args } => {
                self.lint_interactive_command(name.as_str().as_str(), span);
                self.lint_proc_command_args(args);
            }
            ArenaCommand::Core {
                name: _,
                args,
                env,
                block,
            } => {
                self.lint_command_args(args);
                for assignment in self.arena.env_assignments(env).to_vec() {
                    self.lint_env_assignment_value(&assignment.value);
                }
                if let Some(block) = block {
                    self.lint_block(block);
                }
            }
            ArenaCommand::Run(run) => self.lint_run(run),
        }
    }

    fn lint_interactive_command(&mut self, name: &str, span: Span) {
        let Some(replacement) = self
            .interactive_command_replacement
            .and_then(|replacement| replacement(name))
        else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Error,
                format!(
                    "`{name}` is for interactive use; use {} in scripts",
                    replacement
                ),
            )
            .with_code("lint.interactive-command")
            .with_label(Label::secondary(span, "interactive compatibility command")),
        );
    }

    fn lint_redundant_command_arg_interpolation(&mut self, arg: &ArenaCommandArg) {
        let arg_span = self.arena.span(arg.span);
        let ArenaCommandArgKind::Word(parts) = arg.kind else {
            return;
        };
        let parts: Vec<ArenaWordPart> = self.arena.word_parts(parts).collect();
        let (single_interp, is_shorthand) = match parts.as_slice() {
            [ArenaWordPart::Interpolation(expr)] => (*expr, false),
            [ArenaWordPart::Shorthand(expr)] => (*expr, true),
            _ => return,
        };
        if matches!(self.arena.expr(single_interp).kind, ArenaExprKind::Ident(_)) {
            return;
        }
        if self.can_bare_in_command_arg(single_interp) {
            let mut span = self.arena.expr(single_interp).span;
            if is_shorthand {
                span.set_start(span.start() + 1); // skip `$`
            }
            let Some(replacement) = self.source.get(span.range()) else {
                return;
            };
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "command args can use expression syntax directly",
                )
                .with_code("lint.redundant-command-interpolation")
                .with_label(Label::secondary(
                    arg_span,
                    "this interpolation is unnecessary",
                ))
                .with_fix_hint(FixHint::replacement(
                    arg_span,
                    "use the expression directly",
                    replacement.to_string(),
                )),
            );
        }
    }

    fn can_bare_in_command_arg(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Call { callee, .. } => self.can_bare_chain_base(callee),
            ArenaExprKind::Index { base, index } => {
                self.can_bare_chain_base(base)
                    && matches!(self.arena.expr(index).kind, ArenaExprKind::Int(_))
            }
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.command_chain_has_call_or_index(expr) && self.can_bare_chain_base(base)
            }
            _ => false,
        }
    }

    fn can_bare_chain_base(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Ident(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::FmtString(_)
            | ArenaExprKind::PathFmtString(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::GlobStr(_) => true,
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.can_bare_chain_base(base)
            }
            ArenaExprKind::Call { callee, .. } => self.can_bare_chain_base(callee),
            ArenaExprKind::Index { base, index } => {
                self.can_bare_chain_base(base)
                    && matches!(self.arena.expr(index).kind, ArenaExprKind::Int(_))
            }
            _ => false,
        }
    }

    fn command_chain_has_call_or_index(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Call { .. } | ArenaExprKind::Index { .. } => true,
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.command_chain_has_call_or_index(base)
            }
            _ => false,
        }
    }
}

impl<'a> Linter<'a> {
    fn lint_run(&mut self, run_id: RunFormId) {
        let run = self.arena.run_form(run_id).clone();
        if self.runless {
            for segment in self.arena.run_segments(run.segments).to_vec() {
                let seg_span = self.arena.span(segment.span);
                let name = command_name(self.arena, self.source, &segment.target);
                let exempt = name
                    .as_deref()
                    .is_some_and(|n| self.runless_except.iter().any(|e| e == n));
                if !exempt {
                    let label = match &name {
                        Some(n) => format!("`{n}` is an external command"),
                        None => "external command not allowed in runless mode".to_string(),
                    };
                    self.diagnostics.push(
                        Diagnostic::new(
                            Severity::Error,
                            "external command not permitted (--runless)",
                        )
                        .with_code("lint.runless")
                        .with_label(Label::secondary(seg_span, label)),
                    );
                }
            }
        }
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_run_form(run_id);
    }

    fn lint_command_args(&mut self, args: ArenaRange) {
        for arg in self.arena.command_args(args).to_vec() {
            self.lint_command_arg(&arg);
        }
    }

    fn lint_proc_command_args(&mut self, args: ArenaRange) {
        for arg in self.arena.command_args(args).to_vec() {
            self.lint_proc_command_arg(&arg);
        }
    }

    fn lint_env_assignment_value(&mut self, value: &ArenaEnvAssignmentValue) {
        match value {
            ArenaEnvAssignmentValue::CommandArg(arg) => self.lint_proc_command_arg(arg),
            ArenaEnvAssignmentValue::Expr(expr) => self.lint_expr(*expr),
        }
    }

    fn lint_assign_target(&mut self, target: AssignTargetId) {
        match self.arena.assign_target(target).kind.clone() {
            ArenaAssignTargetKind::Name(name) => self.mark_used(name.as_str().as_str()),
            ArenaAssignTargetKind::Field { base, .. } => self.lint_assign_target(base),
            ArenaAssignTargetKind::Index { base, index } => {
                self.lint_assign_target(base);
                self.lint_expr(index);
            }
        }
    }

    fn lint_command_arg(&mut self, arg: &ArenaCommandArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: true,
        }
        .visit_command_arg(arg, false);
    }

    fn lint_proc_command_arg(&mut self, arg: &ArenaCommandArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: true,
        }
        .visit_command_arg(arg, true);
    }

    fn lint_expr_or_run(&mut self, value: &ArenaExprOrRun) {
        match value {
            ArenaExprOrRun::Expr(expr) => self.lint_expr(*expr),
            ArenaExprOrRun::Run(run) => self.lint_run(*run),
        }
    }

    fn lint_expr(&mut self, expr: ExprId) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_expr(expr);
    }

    fn lint_builder_block(&mut self, block: BuilderBlockId) {
        self.push_scope();
        let entries: Vec<_> = self
            .arena
            .builder_entries(self.arena.builder_block(block).entries)
            .to_vec();
        for entry in entries {
            match entry.kind {
                ArenaBuilderEntryKind::Field { value, .. } => self.lint_expr(value),
                ArenaBuilderEntryKind::Entry { args, block, .. } => {
                    for arg in self.arena.command_args(args).to_vec() {
                        self.lint_command_arg(&arg);
                    }
                    if let Some(block) = block {
                        self.lint_builder_block(block);
                    }
                }
                ArenaBuilderEntryKind::Task { block, .. } => self.lint_block_statements(block),
                ArenaBuilderEntryKind::Stmt(stmt) => self.lint_stmt(stmt, false),
            }
        }
        self.pop_scope();
    }

    fn lint_stream_block(&mut self, block: BlockId) {
        self.push_scope();
        for param in self
            .arena
            .block_params(self.arena.block(block).params)
            .to_vec()
        {
            self.define(
                param.name.as_str().as_str(),
                self.arena.span(param.span),
                true,
            );
        }
        self.lint_block_statements(block);
        self.pop_scope();
    }

    fn lint_stream_stage(&mut self, stage: &ArenaStreamStage) {
        for option in self.arena.stream_options(stage.options).to_vec() {
            if let Some(value) = option.value {
                self.lint_expr(value);
            }
        }
        for arg in self.arena.call_args(stage.args).to_vec() {
            self.lint_call_arg(&arg);
        }
        if let Some(block) = stage.block {
            self.lint_stream_block(block);
        }
    }

    fn lint_call_arg(&mut self, arg: &ArenaCallArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_call_arg(arg);
    }

    fn lint_call_style(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        self.lint_path_constructor(callee, args, span);
        self.lint_redundant_defaults(callee, args);
        self.lint_prefer_in(callee, args, span);
        self.lint_prefer_method(callee, args, span);
        self.lint_join_to_concat(callee, args, span);
    }

    fn lint_path_constructor(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let callee_expr = self.arena.expr(callee);
        let is_path_ctor = matches!(callee_expr.kind, ArenaExprKind::Ident(name) if name == "Path");
        if !is_path_ctor {
            return;
        }
        let Some(first) = self.arena.call_args(args).first() else {
            return;
        };
        let ArenaCallArgKind::Positional(expr) = first.kind else {
            return;
        };
        let expr_span = self.arena.expr(expr).span;
        if callee_expr.span == span && expr_span == span {
            return;
        }
        let replacement = match self.arena.expr(expr).kind {
            ArenaExprKind::Str(_) => {
                let literal_text = &self.source[expr_span.start()..expr_span.end()];
                Some(format!("p{literal_text}"))
            }
            ArenaExprKind::FmtString(parts) => {
                if self
                    .single_path_interpolation_parts_replacement(parts)
                    .is_some()
                {
                    None
                } else {
                    path_fmt_literal_text(&self.source[expr_span.start()..expr_span.end()])
                }
            }
            _ => self
                .source
                .get(expr_span.range())
                .map(|source| format!("fp\"${{{source}}}\"")),
        };
        let message = if matches!(
            self.arena.expr(expr).kind,
            ArenaExprKind::Str(_) | ArenaExprKind::FmtString(_)
        ) {
            "prefer path literal syntax for path construction"
        } else {
            "prefer p-string interpolation over `Path(...)`"
        };
        let mut diagnostic =
            xsh::diagnostic::Diagnostic::new(xsh::diagnostic::Severity::Warning, message)
                .with_code("lint.path-constructor")
                .with_label(xsh::diagnostic::Label::secondary(
                    span,
                    "use path string syntax instead",
                ));
        if let Some(replacement) = replacement {
            diagnostic = diagnostic.with_fix_hint(xsh::diagnostic::FixHint::replacement(
                span,
                "replace with path string",
                replacement,
            ));
        }
        self.diagnostics.push(
            diagnostic.with_note(
                "`Path(...)` remains a cast, but p-strings are the preferred path syntax",
            ),
        );
    }

    fn lint_join_to_concat(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "join" {
            return;
        }
        let ArenaExprKind::List(items) = self.arena.expr(base).kind else {
            return;
        };
        let separator_is_empty = args.is_empty()
            || {
                match self.arena.call_args(args).first().map(|a| a.kind.clone()) {
                    Some(ArenaCallArgKind::Positional(e)) => {
                        matches!(self.arena.expr(e).kind, ArenaExprKind::Str(id) if self.arena.string_literal(id).is_empty())
                    }
                    _ => false,
                }
            };
        if !separator_is_empty {
            return;
        }
        if items.is_empty() {
            return;
        }
        let replacement = self
            .arena
            .expr_ids(items)
            .map(|item| {
                let s = self.arena.expr(item).span;
                &self.source[s.start()..s.end()]
            })
            .collect::<Vec<_>>()
            .join(" + ");
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "prefer `+` over `[a, b].join(\"\")` for string concatenation",
            )
            .with_code("lint.prefer-string-concat")
            .with_label(Label::secondary(span, "use `+` instead"))
            .with_fix_hint(FixHint::replacement(span, "rewrite with `+`", replacement)),
        );
    }

    fn lint_redundant_defaults(&mut self, callee: ExprId, args: ArenaRange) {
        if is_fs_call(self.arena, callee, "mkdir") || is_method_call(self.arena, callee, "mkdir") {
            self.lint_redundant_named_bool(
                args,
                "parents",
                true,
                "`mkdir` creates parent directories by default",
            );
        } else if is_fs_call(self.arena, callee, "touch")
            || is_method_call(self.arena, callee, "touch")
        {
            self.lint_redundant_named_bool(
                args,
                "create",
                true,
                "`touch` creates the file by default",
            );
        } else if is_fs_call(self.arena, callee, "walk")
            || is_method_call(self.arena, callee, "walk")
        {
            self.lint_redundant_named_bool(
                args,
                "gitignore",
                true,
                "`walk` respects .gitignore by default",
            );
        } else if is_fs_call(self.arena, callee, "remove_manifest") {
            self.lint_redundant_named_bool(
                args,
                "prune_dirs",
                true,
                "`remove_manifest` prunes empty directories by default",
            );
        } else if is_fs_call(self.arena, callee, "install")
            || is_method_call(self.arena, callee, "install")
        {
            self.lint_redundant_named_bool(
                args,
                "parents",
                true,
                "`install` creates parent directories by default",
            );
        } else if is_fs_call(self.arena, callee, "install_as")
            || is_method_call(self.arena, callee, "install_as")
        {
            self.lint_redundant_named_bool(
                args,
                "parents",
                true,
                "`install_as` creates parent directories by default",
            );
        } else if is_fs_call(self.arena, callee, "copy_tree")
            || is_method_call(self.arena, callee, "copy_tree")
        {
            self.lint_redundant_named_bool(
                args,
                "parents",
                true,
                "`copy_tree` creates parent directories by default",
            );
        }
    }

    fn lint_prefer_method(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field { base, name: func } = self.arena.expr(callee).kind else {
            return;
        };
        let ArenaExprKind::Ident(module) = self.arena.expr(base).kind else {
            return;
        };
        // Check module.func is a known module-function with a method equivalent.
        // The first positional arg becomes the receiver; the rest become method args.
        let min_args = module_func_min_args(module.as_str().as_str(), func.as_str().as_str());
        let Some(min_args) = min_args else {
            return;
        };
        if args.len() < min_args {
            return;
        }
        let arg_list = self.arena.call_args(args).to_vec();
        let Some(ArenaCallArgKind::Positional(receiver_expr)) =
            arg_list.first().map(|a| a.kind.clone())
        else {
            return;
        };
        let receiver_span = self.arena.expr(receiver_expr).span;
        let receiver_text = &self.source[receiver_span.start()..receiver_span.end()];
        let rest: Vec<&str> = arg_list[1..]
            .iter()
            .filter_map(|a| match a.kind {
                ArenaCallArgKind::Positional(e) => {
                    let s = self.arena.expr(e).span;
                    Some(&self.source[s.start()..s.end()])
                }
                _ => None,
            })
            .collect();
        let replacement = if rest.is_empty() {
            format!("{receiver_text}.{func}()")
        } else {
            format!("{receiver_text}.{func}({})", rest.join(", "))
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("prefer method form `{replacement}`"),
            )
            .with_code("lint.prefer-method")
            .with_label(Label::secondary(span, "use method syntax instead"))
            .with_fix_hint(FixHint::replacement(
                span,
                "rewrite as method call",
                replacement,
            )),
        );
    }

    fn lint_prefer_in(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field {
            base: receiver,
            name,
        } = self.arena.expr(callee).kind
        else {
            return;
        };
        if name != "contains" || args.len() != 1 {
            return;
        }
        let Some(arg) = self.arena.call_args(args).first() else {
            return;
        };
        let ArenaCallArgKind::Positional(needle) = arg.kind else {
            return;
        };
        let Some(receiver_ty) = self.expr_types.get(&self.arena.expr(receiver).span) else {
            return;
        };
        if !prefer_in_receiver_type(receiver_ty) {
            return;
        }
        if expr_may_have_effects(self.arena, receiver) || expr_may_have_effects(self.arena, needle)
        {
            return;
        }
        let Some(receiver_text) = self.source.get(self.arena.expr(receiver).span.range()) else {
            return;
        };
        let Some(needle_text) = self.source.get(self.arena.expr(needle).span.range()) else {
            return;
        };
        if receiver_text.contains('#') || needle_text.contains('#') {
            return;
        }
        let (fix_span, replacement) =
            if let Some(negation_start) = directly_negated_start(self.source, span) {
                (
                    Span::new(span.source_id, negation_start, span.end()),
                    format!("{needle_text} not in {receiver_text}"),
                )
            } else {
                (span, format!("{needle_text} in {receiver_text}"))
            };
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "prefer `in` over `.contains(...)`")
                .with_code("lint.prefer-in")
                .with_label(Label::secondary(span, "use membership syntax instead"))
                .with_fix_hint(FixHint::replacement(
                    fix_span,
                    "rewrite with membership syntax",
                    replacement,
                )),
        );
    }

    fn lint_if_as_guard(&mut self, branches: ArenaRange, else_block: Option<BlockId>, span: Span) {
        // Only single-branch if with no else
        if branches.len() != 1 || else_block.is_some() {
            return;
        }
        let branch = self.arena.if_branches(branches)[0].clone();
        // Only single-statement body
        let branch_stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(branch.block).statements)
            .collect();
        let [only_stmt] = branch_stmts.as_slice() else {
            return;
        };
        let keyword = match self.arena.stmt(*only_stmt).kind {
            ArenaStmtKind::Break { value: None } => "break",
            ArenaStmtKind::Continue => "continue",
            _ => return,
        };
        let cond_span = self.arena.expr(branch.condition).span;
        let cond_text = self.source.get(cond_span.start()..cond_span.end());
        let (guard_word, replacement_cond) = match self.arena.expr(branch.condition).kind {
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                expr: inner,
            } => {
                let inner_span = self.arena.expr(inner).span;
                let inner_text = self.source.get(inner_span.start()..inner_span.end());
                ("unless", inner_text)
            }
            _ => ("when", cond_text),
        };
        if let Some(cond) = replacement_cond {
            let replacement = format!("{keyword} {guard_word} {cond}");
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!("use `{keyword} {guard_word}` instead of `if {{ {keyword} }}`"),
                )
                .with_code("lint.prefer-guard")
                .with_label(Label::secondary(span, "replace with postfix guard"))
                .with_fix_hint(FixHint::replacement(
                    span,
                    format!("use `{keyword} {guard_word}`"),
                    replacement,
                )),
            );
        }
    }

    fn lint_prefer_fs_files(&mut self, input: ExprId, stages: ArenaRange) {
        let stage_slice = self.arena.stream_stages(stages);
        let Some(first_stage) = stage_slice.first().cloned() else {
            return;
        };
        let first_stage_span = self.arena.span(first_stage.span);
        if first_stage.kind != StreamStageKind::Where {
            return;
        }
        let Some(block) = first_stage.block else {
            return;
        };
        let block_data = self.arena.block(block);
        let block_stmts: Vec<StmtId> = self.arena.stmt_ids(block_data.statements).collect();
        if !(block_data.params.is_empty() && block_stmts.len() == 1) {
            return;
        }
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(block_stmts[0]).kind else {
            return;
        };
        if !is_kind_eq_file_expr(self.arena, expr) {
            return;
        }
        let input_span = self.arena.expr(input).span;
        let ArenaExprKind::Call { callee, args } = self.arena.expr(input).kind else {
            return;
        };
        if !is_fs_call(self.arena, callee, "walk") {
            return;
        }
        // Extract source text for args to reconstruct fs.files(ARGS)
        let args_src: Vec<&str> = self
            .arena
            .call_args(args)
            .iter()
            .filter_map(|a| {
                let span = match a.kind {
                    ArenaCallArgKind::Positional(e) => self.arena.expr(e).span,
                    ArenaCallArgKind::Named { span, .. }
                    | ArenaCallArgKind::Splice { span, .. } => self.arena.span(span),
                };
                self.source.get(span.start()..span.end())
            })
            .collect();
        let replacement = format!("fs.files({})", args_src.join(", "));
        let replace_span = Span::new(
            input_span.source_id,
            input_span.start(),
            first_stage_span.end(),
        );
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "prefer `fs.files()` over `fs.walk() |> where .kind == \"file\"`",
            )
            .with_code("lint.prefer-fs-files")
            .with_label(Label::secondary(
                first_stage_span,
                "this stage is redundant with fs.files",
            ))
            .with_fix_hint(FixHint::replacement(
                replace_span,
                "use fs.files()",
                replacement,
            )),
        );
    }

    fn lint_redundant_stream_stages(&mut self, stages: ArenaRange) {
        for stage in self.arena.stream_stages(stages).to_vec() {
            let stage_span = self.arena.span(stage.span);
            if !stage.args.is_empty() || !stage.options.is_empty() {
                continue;
            }
            let Some(block) = stage.block else {
                continue;
            };
            let block_data = self.arena.block(block);
            let block_stmts: Vec<StmtId> = self.arena.stmt_ids(block_data.statements).collect();
            if !block_data.params.is_empty() || block_stmts.len() != 1 {
                continue;
            }
            let ArenaStmtKind::Expr(expr) = self.arena.stmt(block_stmts[0]).kind else {
                continue;
            };
            let (code, message) = match stage.kind {
                StreamStageKind::Where
                    if matches!(self.arena.expr(expr).kind, ArenaExprKind::Bool(true)) =>
                {
                    (
                        "lint.redundant-pipeline-stage",
                        "redundant `where true` pipeline stage",
                    )
                }
                StreamStageKind::Map
                    if matches!(self.arena.expr(expr).kind, ArenaExprKind::Item) =>
                {
                    (
                        "lint.redundant-pipeline-stage",
                        "redundant `map .` pipeline stage",
                    )
                }
                _ => continue,
            };
            let deletion_span = scan_pipe_stage_deletion_span(self.source, stage_span);
            self.diagnostics.push(
                Diagnostic::new(Severity::Warning, message)
                    .with_code(code)
                    .with_label(Label::secondary(
                        stage_span,
                        "this stage does not change items",
                    ))
                    .with_fix_hint(FixHint::deletion(
                        deletion_span,
                        "remove redundant pipeline stage",
                    )),
            );
        }
    }

    fn lint_prefer_file_lines(&mut self, iter: ExprId) {
        if !expr_contains_read_text_lines_call(self.arena, iter) {
            return;
        }
        let iter_span = self.arena.expr(iter).span;
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "prefer file-backed lines in line-by-line loops",
            )
            .with_code("lint.prefer-file-lines")
            .with_label(Label::secondary(
                iter_span,
                "`read_text()?.lines()` reads the full file first; use `path.lines()?` when consuming once",
            )),
        );
    }

    fn lint_redundant_named_bool(
        &mut self,
        args: ArenaRange,
        name: &str,
        value: bool,
        label: &'static str,
    ) {
        let Some((arg_span, deletion_span)) = named_bool_arg_info(self.arena, args, name, value)
        else {
            return;
        };
        let val_str = if value { "true" } else { "false" };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("`{name}: {val_str}` is redundant"),
            )
            .with_code("lint.redundant-default")
            .with_label(Label::secondary(arg_span, label))
            .with_fix_hint(FixHint::deletion(
                deletion_span,
                "remove redundant argument",
            )),
        );
    }

    fn define(&mut self, name: &str, span: Span, report_unused: bool) {
        if self.is_defined_in_outer_scope(name) && !is_predeclared_script_args(name) && name != "_"
        {
            self.diagnostics.push(
                Diagnostic::error("binding shadows an outer name")
                    .with_code("lint.shadowing")
                    .with_label(Label::secondary(span, "shadowed binding starts here")),
            );
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                Binding {
                    span,
                    used: false,
                    report_unused: report_unused && name != "_",
                },
            );
        }
    }

    fn mark_used(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                binding.used = true;
                break;
            }
        }
    }

    fn is_defined_in_outer_scope(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .skip(1)
            .any(|scope| scope.contains_key(name))
    }

    fn push_scope(&mut self) {
        self.scopes.push(FxHashMap::default());
    }

    fn pop_scope(&mut self) {
        let Some(scope) = self.scopes.pop() else {
            return;
        };
        let mut unused: Vec<_> = scope
            .into_iter()
            .filter(|(_, binding)| binding.report_unused && !binding.used)
            .collect();
        insertion_sort_by(&mut unused, |(_, left), (_, right)| {
            left.span.start().cmp(&right.span.start())
        });
        for (name, binding) in unused {
            self.warning(
                binding.span,
                format!("unused local variable `{name}`"),
                "lint.unused-local",
                "binding is never read",
            );
        }
    }

    fn warning(
        &mut self,
        span: Span,
        message: impl Into<String>,
        code: &'static str,
        label: &'static str,
    ) {
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, message)
                .with_code(code)
                .with_label(Label::secondary(span, label)),
        );
    }

    fn expr_or_run_span(&self, value: &ArenaExprOrRun) -> Span {
        match value {
            ArenaExprOrRun::Expr(expr) => self.arena.expr(*expr).span,
            ArenaExprOrRun::Run(run) => self.arena.span(self.arena.run_form(*run).span),
        }
    }
}

fn result_unit_type_expr(arena: &AstArena, ty: TypeExprId) -> bool {
    matches!(
        type_expr_kind(arena, ty),
        ArenaTypeExprKind::Result { ok, .. }
            if matches!(type_expr_kind(arena, ok), ArenaTypeExprKind::Named(name) if name == "Unit")
    )
}

fn result_path_type_expr(arena: &AstArena, ty: TypeExprId) -> bool {
    matches!(
        type_expr_kind(arena, ty),
        ArenaTypeExprKind::Result { ok, .. }
            if matches!(type_expr_kind(arena, ok), ArenaTypeExprKind::Named(name) if name == "Path")
    )
}

fn result_ok_type_expr(arena: &AstArena, ty: TypeExprId) -> Option<Type> {
    match type_expr_kind(arena, ty) {
        ArenaTypeExprKind::Result { ok, .. } => {
            let ty = Type::from_arena(arena, ok);
            if matches!(ty, Type::Any | Type::Unknown | Type::Invalid) {
                None
            } else {
                Some(ty)
            }
        }
        _ => None,
    }
}

fn tail_type_matches_lint_expected(return_ty: &Type, value_ty: &Type) -> bool {
    value_ty.matches_expected(return_ty)
        || matches!(return_ty, Type::Result(ok, _) if value_ty.matches_expected(ok))
}

fn single_interpolation_expr(arena: &AstArena, parts: ArenaRange) -> Option<ExprId> {
    let parts: Vec<ArenaFmtPart> = arena.fmt_parts(parts).collect();
    match parts.as_slice() {
        [ArenaFmtPart::Expr(expr, None)] => Some(*expr),
        _ => None,
    }
}

fn path_fmt_literal_text(text: &str) -> Option<String> {
    text.strip_prefix("f\"").map(|rest| format!("fp\"{rest}"))
}

fn return_list_type_expr(arena: &AstArena, ty: TypeExprId) -> bool {
    match type_expr_kind(arena, ty) {
        ArenaTypeExprKind::List(_) => true,
        ArenaTypeExprKind::Result { ok, .. } => {
            matches!(type_expr_kind(arena, ok), ArenaTypeExprKind::List(_))
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
struct StreamProducerCandidate {
    function_name: xsh::frontend::symbols::Name,
    accumulator_name: xsh::frontend::symbols::Name,
    span: Span,
}

fn collect_stream_producer_candidates(
    arena: &AstArena,
    stmts: &[StmtId],
    out: &mut Vec<StreamProducerCandidate>,
) {
    for &stmt_id in stmts {
        let inner = match arena.stmt(stmt_id).kind {
            ArenaStmtKind::Export(inner) => arena.stmt(inner).kind,
            other => other,
        };
        if let ArenaStmtKind::ProcDef(def_id) = inner {
            let def = arena.function_def(def_id);
            if return_list_type_expr(arena, def.return_ty)
                && let Some((accumulator_name, span)) = stream_producer_candidate(arena, def.body)
            {
                out.push(StreamProducerCandidate {
                    function_name: def.name,
                    accumulator_name,
                    span,
                });
            }
        }
    }
}

fn collect_lazy_consumed_calls(
    arena: &AstArena,
    stmts: &[StmtId],
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for &stmt_id in stmts {
        lazy_visit_stmt(arena, stmt_id, out);
    }
}

fn lazy_visit_stmt(
    arena: &AstArena,
    stmt_id: StmtId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.stmt(stmt_id).kind {
        ArenaStmtKind::Use(_)
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::Continue
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Return(None)
        // Old visitor::walk_stmt treats Break (with or without value) as a leaf.
        | ArenaStmtKind::Break { .. } => {}
        ArenaStmtKind::Export(inner) => lazy_visit_stmt(arena, inner, out),
        ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Var { initializer, .. } => {
            lazy_visit_expr_or_run(arena, &initializer, out);
        }
        ArenaStmtKind::Assign { target, value, .. } => {
            lazy_visit_assign_target(arena, target, out);
            lazy_visit_expr_or_run(arena, &value, out);
        }
        ArenaStmtKind::Return(Some(v)) | ArenaStmtKind::Defer(v) | ArenaStmtKind::Yield(v) => {
            lazy_visit_expr_or_run(arena, &v, out);
        }
        ArenaStmtKind::ProcDef(def) | ArenaStmtKind::PureDef(def) | ArenaStmtKind::StreamDef(def) => {
            lazy_visit_block(arena, arena.function_def(def).body, out);
        }
        ArenaStmtKind::SignalHook(hook) => {
            lazy_visit_block(arena, arena.signal_hook(hook).body, out);
        }
        ArenaStmtKind::If { branches, else_block } => {
            for branch in arena.if_branches(branches).to_vec() {
                lazy_visit_expr(arena, branch.condition, out);
                lazy_visit_block(arena, branch.block, out);
            }
            if let Some(block) = else_block {
                lazy_visit_block(arena, block, out);
            }
        }
        ArenaStmtKind::While { condition, block } => {
            lazy_visit_expr(arena, condition, out);
            lazy_visit_block(arena, block, out);
        }
        ArenaStmtKind::For { iter, block, .. } => {
            if let Some(name) = direct_call_name(arena, iter) {
                out.insert(name);
            }
            lazy_visit_expr(arena, iter, out);
            lazy_visit_block(arena, block, out);
        }
        ArenaStmtKind::Loop { block } => lazy_visit_block(arena, block, out),
        ArenaStmtKind::Guard { initializer, else_block, .. } => {
            lazy_visit_expr_or_run(arena, &initializer, out);
            lazy_visit_block(arena, else_block, out);
        }
        ArenaStmtKind::GuardedStmt { stmt, condition, .. } => {
            lazy_visit_expr(arena, condition, out);
            lazy_visit_stmt(arena, stmt, out);
        }
        ArenaStmtKind::With { bindings, body, else_block, .. } => {
            for binding in arena.with_bindings(bindings).to_vec() {
                lazy_visit_expr(arena, binding.initializer, out);
            }
            lazy_visit_block(arena, body, out);
            lazy_visit_block(arena, else_block, out);
        }
        ArenaStmtKind::Match { value, arms } => {
            lazy_visit_expr(arena, value, out);
            for arm in arena.match_arms(arms).to_vec() {
                if let Some(guard) = arm.guard {
                    lazy_visit_expr(arena, guard, out);
                }
                lazy_visit_block(arena, arm.block, out);
            }
        }
        ArenaStmtKind::Expr(expr) => {
            lazy_visit_expr(arena, expr, out);
        }
        ArenaStmtKind::Command(cmd_id) => {
            lazy_visit_command(arena, cmd_id, out);
        }
    }
}

fn lazy_visit_assign_target(
    arena: &AstArena,
    target: AssignTargetId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.assign_target(target).kind.clone() {
        ArenaAssignTargetKind::Name(_) => {}
        ArenaAssignTargetKind::Field { base, .. } => lazy_visit_assign_target(arena, base, out),
        ArenaAssignTargetKind::Index { base, index } => {
            lazy_visit_assign_target(arena, base, out);
            lazy_visit_expr(arena, index, out);
        }
    }
}

fn lazy_visit_command(
    arena: &AstArena,
    cmd_id: CommandStmtId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.command_stmt(cmd_id).command.clone() {
        ArenaCommand::Proc { args, .. } => {
            for arg in arena.command_args(args).to_vec() {
                lazy_visit_command_arg(arena, &arg, out);
            }
        }
        ArenaCommand::Core {
            args, env, block, ..
        } => {
            for arg in arena.command_args(args).to_vec() {
                lazy_visit_command_arg(arena, &arg, out);
            }
            for assignment in arena.env_assignments(env).to_vec() {
                match assignment.value {
                    ArenaEnvAssignmentValue::CommandArg(arg) => {
                        lazy_visit_command_arg(arena, &arg, out);
                    }
                    ArenaEnvAssignmentValue::Expr(e) => lazy_visit_expr(arena, e, out),
                }
            }
            if let Some(block) = block {
                lazy_visit_block(arena, block, out);
            }
        }
        ArenaCommand::Run(run) => lazy_visit_run(arena, run, out),
    }
}

fn lazy_visit_run(
    arena: &AstArena,
    run: RunFormId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    let segments = arena.run_form(run).segments;
    for segment in arena.run_segments(segments).to_vec() {
        if let Some(timeout) = segment.timeout {
            lazy_visit_expr(arena, timeout, out);
        }
        if let Some(cpu_max) = segment.cpu_max {
            lazy_visit_expr(arena, cpu_max, out);
        }
        for assignment in arena.env_assignments(segment.env).to_vec() {
            match assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => {
                    lazy_visit_command_arg(arena, &arg, out);
                }
                ArenaEnvAssignmentValue::Expr(e) => lazy_visit_expr(arena, e, out),
            }
        }
        lazy_visit_command_arg(arena, &segment.target, out);
        for arg in arena.command_args(segment.args).to_vec() {
            lazy_visit_command_arg(arena, &arg, out);
        }
        for redirection in arena.redirections(segment.redirections).to_vec() {
            match redirection.target {
                ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => {
                    lazy_visit_command_arg(arena, &arg, out);
                }
            }
        }
    }
}

fn lazy_visit_command_arg(
    arena: &AstArena,
    arg: &ArenaCommandArg,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            for part in arena.word_parts(parts).collect::<Vec<_>>() {
                if let ArenaWordPart::Interpolation(e) | ArenaWordPart::Shorthand(e) = part {
                    lazy_visit_expr(arena, e, out);
                }
            }
        }
        ArenaCommandArgKind::SpliceExpr(e) | ArenaCommandArgKind::Typed(e) => {
            lazy_visit_expr(arena, e, out);
        }
        ArenaCommandArgKind::SpliceName(_) => {}
    }
}

fn lazy_visit_block(
    arena: &AstArena,
    block: BlockId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for stmt in arena
        .stmt_ids(arena.block(block).statements)
        .collect::<Vec<_>>()
    {
        lazy_visit_stmt(arena, stmt, out);
    }
}

fn lazy_visit_expr_or_run(
    arena: &AstArena,
    value: &ArenaExprOrRun,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    if let ArenaExprOrRun::Expr(expr) = value {
        lazy_visit_expr(arena, *expr, out);
    }
}

fn lazy_visit_expr(
    arena: &AstArena,
    expr: ExprId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.expr(expr).kind {
        ArenaExprKind::StructuredPipeline { input, .. } => {
            if let Some(name) = direct_call_name(arena, input) {
                out.insert(name);
            }
        }
        ArenaExprKind::Run(run) => lazy_visit_run(arena, run, out),
        ArenaExprKind::Spawn(form) => {
            if let ArenaSpawnTarget::Run(run) = form.target {
                lazy_visit_run(arena, run, out);
            }
        }
        ArenaExprKind::BuilderCall { block, .. } => {
            lazy_visit_builder_block(arena, block, out);
        }
        _ => {}
    }
    for child in expr_child_exprs(arena, expr) {
        lazy_visit_expr(arena, child, out);
    }
    for block in expr_child_blocks(arena, expr) {
        lazy_visit_block(arena, block, out);
    }
}

fn lazy_visit_builder_block(
    arena: &AstArena,
    block: BuilderBlockId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for entry in arena
        .builder_entries(arena.builder_block(block).entries)
        .to_vec()
    {
        match entry.kind {
            ArenaBuilderEntryKind::Field { value, .. } => lazy_visit_expr(arena, value, out),
            ArenaBuilderEntryKind::Entry { args, block, .. } => {
                for arg in arena.command_args(args).to_vec() {
                    lazy_visit_command_arg(arena, &arg, out);
                }
                if let Some(block) = block {
                    lazy_visit_builder_block(arena, block, out);
                }
            }
            ArenaBuilderEntryKind::Task { block, .. } => lazy_visit_block(arena, block, out),
            ArenaBuilderEntryKind::Stmt(stmt) => lazy_visit_stmt(arena, stmt, out),
        }
    }
}

/// Enumerate the immediate child expressions of an expression for structural
/// traversal (mirrors the old `visitor::walk_expr` descent).
fn expr_child_exprs(arena: &AstArena, expr: ExprId) -> Vec<ExprId> {
    let mut out = Vec::new();
    match arena.expr(expr).kind {
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            for part in arena.fmt_parts(parts).collect::<Vec<_>>() {
                if let ArenaFmtPart::Expr(e, _) = part {
                    out.push(e);
                }
            }
        }
        ArenaExprKind::List(items) => out.extend(arena.expr_ids(items)),
        ArenaExprKind::ListComp {
            expr,
            iter,
            condition,
            ..
        } => {
            out.push(expr);
            out.push(iter);
            out.extend(condition);
        }
        ArenaExprKind::MapComp {
            key,
            value,
            iter,
            condition,
            ..
        } => {
            out.push(key);
            out.push(value);
            out.push(iter);
            out.extend(condition);
        }
        ArenaExprKind::Record(fields) => {
            for field in arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Named { value, .. } => out.push(value),
                    ArenaRecordFieldKind::Spread { expr, .. } => out.push(expr),
                    ArenaRecordFieldKind::Shorthand { .. } => {}
                }
            }
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            for branch in arena.if_expr_branches(branches) {
                out.push(branch.condition);
                out.push(branch.value);
            }
            out.push(else_value);
        }
        ArenaExprKind::Match { value, arms } => {
            out.push(value);
            for arm in arena.match_expr_arms(arms) {
                out.extend(arm.guard);
                out.push(arm.value);
            }
        }
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => out.push(expr),
        ArenaExprKind::Binary { left, right, .. } => {
            out.push(left);
            out.push(right);
        }
        ArenaExprKind::Call { callee, args } => {
            out.push(callee);
            for arg in arena.call_args(args) {
                match arg.kind {
                    ArenaCallArgKind::Positional(e)
                    | ArenaCallArgKind::Named { value: e, .. }
                    | ArenaCallArgKind::Splice { value: e, .. } => out.push(e),
                }
            }
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            out.push(base);
        }
        ArenaExprKind::Index { base, index } => {
            out.push(base);
            out.push(index);
        }
        ArenaExprKind::Slice { base, start, end } => {
            out.push(base);
            out.extend(start);
            out.extend(end);
        }
        ArenaExprKind::Pipeline { input, stages } => {
            out.push(input);
            for stage in arena.pipe_stages(stages).to_vec() {
                match stage.kind {
                    ArenaPipeStageKind::Expr(e) => out.push(e),
                    ArenaPipeStageKind::Stream(stage) => {
                        for arg in arena.call_args(stage.args) {
                            match arg.kind {
                                ArenaCallArgKind::Positional(e)
                                | ArenaCallArgKind::Named { value: e, .. }
                                | ArenaCallArgKind::Splice { value: e, .. } => out.push(e),
                            }
                        }
                    }
                }
            }
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            out.push(input);
            for stage in arena.stream_stages(stages).to_vec() {
                for option in arena.stream_options(stage.options) {
                    out.extend(option.value);
                }
                for arg in arena.call_args(stage.args) {
                    match arg.kind {
                        ArenaCallArgKind::Positional(e)
                        | ArenaCallArgKind::Named { value: e, .. }
                        | ArenaCallArgKind::Splice { value: e, .. } => out.push(e),
                    }
                }
            }
        }
        ArenaExprKind::Spawn(form) => {
            if let ArenaSpawnTarget::Command(e) = form.target {
                out.push(e);
            }
        }
        ArenaExprKind::Wait(form) => out.push(form.target),
        ArenaExprKind::BuilderCall { call, .. } => out.push(call),
        ArenaExprKind::Require { value, .. } => out.push(value),
        ArenaExprKind::Retry { delays, .. } => out.extend(arena.expr_ids(delays)),
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvGet { .. }
        | ArenaExprKind::EnvPathList
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Loop { .. } => {}
    }
    out
}

/// Enumerate the immediate child statement-blocks of an expression.
fn expr_child_blocks(arena: &AstArena, expr: ExprId) -> Vec<BlockId> {
    let mut out = Vec::new();
    match arena.expr(expr).kind {
        ArenaExprKind::Loop { block } | ArenaExprKind::Retry { block, .. } => out.push(block),
        ArenaExprKind::Pipeline { stages, .. } => {
            for stage in arena.pipe_stages(stages).to_vec() {
                if let ArenaPipeStageKind::Stream(stage) = stage.kind
                    && let Some(block) = stage.block
                {
                    out.push(block);
                }
            }
        }
        ArenaExprKind::StructuredPipeline { stages, .. } => {
            for stage in arena.stream_stages(stages).to_vec() {
                if let Some(block) = stage.block {
                    out.push(block);
                }
            }
        }
        _ => {}
    }
    out
}

fn direct_call_name(arena: &AstArena, expr: ExprId) -> Option<xsh::frontend::symbols::Name> {
    let expr = match arena.expr(expr).kind {
        ArenaExprKind::Try(inner) => inner,
        _ => expr,
    };
    let ArenaExprKind::Call { callee, .. } = arena.expr(expr).kind else {
        return None;
    };
    let ArenaExprKind::Ident(name) = arena.expr(callee).kind else {
        return None;
    };
    Some(name)
}

fn expr_is_dynamic_require_boundary(arena: &AstArena, expr: ExprId) -> bool {
    let expr = match arena.expr(expr).kind {
        ArenaExprKind::Try(inner) => inner,
        _ => expr,
    };
    let ArenaExprKind::Call { callee, .. } = arena.expr(expr).kind else {
        return false;
    };
    is_module_call(arena, callee, "module", "load")
        || is_module_call(arena, callee, "json", "decode")
        || is_module_call(arena, callee, "json", "read")
}

fn stream_producer_candidate(
    arena: &AstArena,
    body: BlockId,
) -> Option<(xsh::frontend::symbols::Name, Span)> {
    let stmts: Vec<StmtId> = arena.stmt_ids(arena.block(body).statements).collect();
    let &final_stmt = stmts.last()?;
    stmts.iter().enumerate().find_map(|(index, &stmt)| {
        let (name, span) = empty_list_var(arena, stmt)?;
        let rest = &stmts[index + 1..];
        if !rest.iter().any(|&stmt| stmt_pushes_to(arena, stmt, name)) {
            return None;
        }
        if rest
            .iter()
            .any(|&stmt| stmt_assigns_non_push_to(arena, stmt, name))
        {
            return None;
        }
        if !stmt_returns_value_from(arena, final_stmt, name) {
            return None;
        }
        Some((name, span))
    })
}

fn empty_list_var(arena: &AstArena, stmt: StmtId) -> Option<(xsh::frontend::symbols::Name, Span)> {
    let arena_stmt = arena.stmt(stmt);
    let ArenaStmtKind::Var {
        target,
        initializer: ArenaExprOrRun::Expr(init),
        ..
    } = arena_stmt.kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
        return None;
    };
    let ArenaExprKind::List(items) = arena.expr(init).kind else {
        return None;
    };
    if items.is_empty() {
        Some((name, arena_stmt.span))
    } else {
        None
    }
}

fn stmt_pushes_to(arena: &AstArena, stmt: StmtId, name: xsh::frontend::symbols::Name) -> bool {
    if stmt_is_push_assignment(arena, stmt, name) {
        return true;
    }
    match arena.stmt(stmt).kind {
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .if_branches(branches)
                .iter()
                .any(|branch| block_pushes_to(arena, branch.block, name))
                || else_block.is_some_and(|block| block_pushes_to(arena, block, name))
        }
        ArenaStmtKind::While { block, .. }
        | ArenaStmtKind::For { block, .. }
        | ArenaStmtKind::Loop { block } => block_pushes_to(arena, block, name),
        ArenaStmtKind::Guard { else_block, .. } => block_pushes_to(arena, else_block, name),
        ArenaStmtKind::GuardedStmt { stmt, .. } | ArenaStmtKind::Export(stmt) => {
            stmt_pushes_to(arena, stmt, name)
        }
        ArenaStmtKind::With {
            body, else_block, ..
        } => block_pushes_to(arena, body, name) || block_pushes_to(arena, else_block, name),
        ArenaStmtKind::Match { arms, .. } => arena
            .match_arms(arms)
            .iter()
            .any(|arm| block_pushes_to(arena, arm.block, name)),
        _ => false,
    }
}

fn block_pushes_to(arena: &AstArena, block: BlockId, name: xsh::frontend::symbols::Name) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .any(|stmt| stmt_pushes_to(arena, stmt, name))
}

fn stmt_assigns_non_push_to(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Assign { target, .. } if assign_target_root_name(arena, target) == name => {
            !stmt_is_push_assignment(arena, stmt, name)
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .if_branches(branches)
                .iter()
                .any(|branch| block_assigns_non_push_to(arena, branch.block, name))
                || else_block.is_some_and(|block| block_assigns_non_push_to(arena, block, name))
        }
        ArenaStmtKind::While { block, .. }
        | ArenaStmtKind::For { block, .. }
        | ArenaStmtKind::Loop { block } => block_assigns_non_push_to(arena, block, name),
        ArenaStmtKind::Guard { else_block, .. } => {
            block_assigns_non_push_to(arena, else_block, name)
        }
        ArenaStmtKind::GuardedStmt { stmt, .. } | ArenaStmtKind::Export(stmt) => {
            stmt_assigns_non_push_to(arena, stmt, name)
        }
        ArenaStmtKind::With {
            body, else_block, ..
        } => {
            block_assigns_non_push_to(arena, body, name)
                || block_assigns_non_push_to(arena, else_block, name)
        }
        ArenaStmtKind::Match { arms, .. } => arena
            .match_arms(arms)
            .iter()
            .any(|arm| block_assigns_non_push_to(arena, arm.block, name)),
        _ => false,
    }
}

fn assign_target_root_name(
    arena: &AstArena,
    target: AssignTargetId,
) -> xsh::frontend::symbols::Name {
    match arena.assign_target(target).kind.clone() {
        ArenaAssignTargetKind::Name(name) => name,
        ArenaAssignTargetKind::Field { base, .. } | ArenaAssignTargetKind::Index { base, .. } => {
            assign_target_root_name(arena, base)
        }
    }
}

fn block_assigns_non_push_to(
    arena: &AstArena,
    block: BlockId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .any(|stmt| stmt_assigns_non_push_to(arena, stmt, name))
}

fn stmt_is_push_assignment(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    let ArenaStmtKind::Assign {
        target,
        op: AssignOp::Set,
        value: ArenaExprOrRun::Expr(rhs),
    } = arena.stmt(stmt).kind
    else {
        return false;
    };
    let ArenaAssignTargetKind::Name(assign_name) = arena.assign_target(target).kind else {
        return false;
    };
    if assign_name != name {
        return false;
    }
    let ArenaExprKind::Call { callee, args } = arena.expr(rhs).kind else {
        return false;
    };
    let ArenaExprKind::Field {
        base,
        name: method_name,
    } = arena.expr(callee).kind
    else {
        return false;
    };
    if method_name != "push" || args.len() != 1 {
        return false;
    }
    matches!(arena.expr(base).kind, ArenaExprKind::Ident(base_name) if base_name == name)
}

fn stmt_returns_value_from(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) => {
            expr_is_ident_or_pipeline_from(arena, expr, name)
        }
        ArenaStmtKind::Expr(expr) => expr_is_ident_or_pipeline_from(arena, expr, name),
        _ => false,
    }
}

fn expr_is_ident_or_pipeline_from(
    arena: &AstArena,
    expr: ExprId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(ident) => ident == name,
        ArenaExprKind::StructuredPipeline { input, .. } | ArenaExprKind::Pipeline { input, .. } => {
            matches!(arena.expr(input).kind, ArenaExprKind::Ident(ident) if ident == name)
        }
        _ => false,
    }
}

fn return_value_is_ok_unit(arena: &AstArena, value: &ArenaExprOrRun) -> bool {
    let ArenaExprOrRun::Expr(expr) = value else {
        return false;
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(*expr).kind else {
        return false;
    };
    matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Ok") && args.is_empty()
}

fn ok_call_arg(arena: &AstArena, expr: ExprId) -> Option<ExprId> {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return None;
    };
    let [arg] = arena.call_args(args) else {
        return None;
    };
    let ArenaCallArgKind::Positional(arg) = arg.kind else {
        return None;
    };
    matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Ok").then_some(arg)
}

fn expr_references_name(arena: &AstArena, expr: ExprId, name: Name) -> bool {
    let refs = |id: ExprId| expr_references_name(arena, id, name);
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(candidate) => candidate == name,
        ArenaExprKind::List(items) => arena.expr_ids(items).any(refs),
        ArenaExprKind::ListComp {
            expr,
            iter,
            condition,
            ..
        } => refs(expr) || refs(iter) || condition.is_some_and(refs),
        ArenaExprKind::MapComp {
            key,
            value,
            iter,
            condition,
            ..
        } => refs(key) || refs(value) || refs(iter) || condition.is_some_and(refs),
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .any(|field| match field.kind {
                    ArenaRecordFieldKind::Named { value, .. } => refs(value),
                    ArenaRecordFieldKind::Spread { expr, .. } => refs(expr),
                    ArenaRecordFieldKind::Shorthand { name: field, .. } => field == name,
                })
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            arena.fmt_parts(parts).any(|part| match part {
                ArenaFmtPart::Expr(expr, _) => refs(expr),
                ArenaFmtPart::Text(_) => false,
            })
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            arena
                .if_expr_branches(branches)
                .iter()
                .any(|branch| refs(branch.condition) || refs(branch.value))
                || refs(else_value)
        }
        ArenaExprKind::Match { value, arms } => {
            refs(value)
                || arena
                    .match_expr_arms(arms)
                    .iter()
                    .any(|arm| arm.guard.is_some_and(refs) || refs(arm.value))
        }
        ArenaExprKind::Unary { expr, .. }
        | ArenaExprKind::Try(expr)
        | ArenaExprKind::Require { value: expr, .. } => refs(expr),
        ArenaExprKind::Binary { left, right, .. } => refs(left) || refs(right),
        ArenaExprKind::Call { callee, args } => {
            refs(callee)
                || arena.call_args(args).iter().any(|arg| match arg.kind {
                    ArenaCallArgKind::Positional(expr)
                    | ArenaCallArgKind::Named { value: expr, .. }
                    | ArenaCallArgKind::Splice { value: expr, .. } => refs(expr),
                })
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => refs(base),
        ArenaExprKind::Index { base, index } => refs(base) || refs(index),
        ArenaExprKind::Slice { base, start, end } => {
            refs(base) || start.is_some_and(refs) || end.is_some_and(refs)
        }
        ArenaExprKind::Pipeline { input, stages } => {
            refs(input)
                || arena
                    .pipe_stages(stages)
                    .to_vec()
                    .iter()
                    .any(|stage| match stage.kind {
                        ArenaPipeStageKind::Expr(expr) => refs(expr),
                        ArenaPipeStageKind::Stream(ref stage) => {
                            stream_stage_references_name(arena, stage, name)
                        }
                    })
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            refs(input)
                || arena
                    .stream_stages(stages)
                    .to_vec()
                    .iter()
                    .any(|stage| stream_stage_references_name(arena, stage, name))
        }
        ArenaExprKind::Run(run) => arena
            .run_segments(arena.run_form(run).segments)
            .to_vec()
            .iter()
            .any(|segment| {
                arena
                    .command_args(segment.args)
                    .iter()
                    .any(|arg| command_arg_references_name(arena, arg, name))
                    || command_arg_references_name(arena, &segment.target, name)
            }),
        ArenaExprKind::Spawn(form) => match form.target {
            ArenaSpawnTarget::Run(run) => arena
                .run_segments(arena.run_form(run).segments)
                .to_vec()
                .iter()
                .any(|segment| {
                    command_arg_references_name(arena, &segment.target, name)
                        || arena
                            .command_args(segment.args)
                            .iter()
                            .any(|arg| command_arg_references_name(arena, arg, name))
                }),
            ArenaSpawnTarget::Command(expr) => refs(expr),
        },
        ArenaExprKind::Wait(form) => refs(form.target),
        ArenaExprKind::BuilderCall { call, block } => {
            refs(call)
                || arena
                    .builder_entries(arena.builder_block(block).entries)
                    .to_vec()
                    .iter()
                    .any(|entry| match entry.kind {
                        ArenaBuilderEntryKind::Field { value, .. } => refs(value),
                        ArenaBuilderEntryKind::Entry { args, block, .. } => {
                            arena
                                .command_args(args)
                                .iter()
                                .any(|arg| command_arg_references_name(arena, arg, name))
                                || block.is_some_and(|block| {
                                    arena
                                        .builder_entries(arena.builder_block(block).entries)
                                        .to_vec()
                                        .iter()
                                        .any(|entry| match entry.kind {
                                            ArenaBuilderEntryKind::Field { value, .. } => {
                                                refs(value)
                                            }
                                            _ => false,
                                        })
                                })
                        }
                        ArenaBuilderEntryKind::Task { .. } | ArenaBuilderEntryKind::Stmt(_) => {
                            false
                        }
                    })
        }
        ArenaExprKind::Loop { .. } | ArenaExprKind::Retry { .. } => false,
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvGet { .. }
        | ArenaExprKind::EnvPathList => false,
    }
}

fn stream_stage_references_name(arena: &AstArena, stage: &ArenaStreamStage, name: Name) -> bool {
    arena
        .call_args(stage.args)
        .iter()
        .any(|arg| match arg.kind {
            ArenaCallArgKind::Positional(expr)
            | ArenaCallArgKind::Named { value: expr, .. }
            | ArenaCallArgKind::Splice { value: expr, .. } => {
                expr_references_name(arena, expr, name)
            }
        })
        || arena.stream_options(stage.options).iter().any(|option| {
            option
                .value
                .is_some_and(|value| expr_references_name(arena, value, name))
        })
        || stage.block.is_some_and(|block| {
            arena
                .stmt_ids(arena.block(block).statements)
                .any(|stmt| match arena.stmt(stmt).kind {
                    ArenaStmtKind::Expr(expr) => expr_references_name(arena, expr, name),
                    ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) => {
                        expr_references_name(arena, expr, name)
                    }
                    _ => false,
                })
        })
}

fn command_arg_references_name(arena: &AstArena, arg: &ArenaCommandArg, name: Name) -> bool {
    match arg.kind {
        ArenaCommandArgKind::SpliceName(candidate) => candidate == name,
        ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
            expr_references_name(arena, expr, name)
        }
        ArenaCommandArgKind::Word(parts) => arena.word_parts(parts).any(|part| match part {
            ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) => {
                expr_references_name(arena, expr, name)
            }
            ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => false,
        }),
    }
}

fn call_arg_span(arena: &AstArena, arg: &ArenaCallArg) -> Option<Span> {
    match arg.kind {
        ArenaCallArgKind::Positional(expr) => Some(arena.expr(expr).span),
        ArenaCallArgKind::Named { span, .. } | ArenaCallArgKind::Splice { span, .. } => {
            Some(arena.span(span))
        }
    }
}

fn named_bool_arg_info(
    arena: &AstArena,
    args: ArenaRange,
    name: &str,
    value: bool,
) -> Option<(Span, Span)> {
    let args = arena.call_args(args).to_vec();
    args.iter().enumerate().find_map(|(i, arg)| {
        let ArenaCallArgKind::Named {
            name: arg_name,
            value: expr,
            span,
        } = arg.kind
        else {
            return None;
        };
        let span = arena.span(span);
        if arg_name != name {
            return None;
        }
        if !matches!(arena.expr(expr).kind, ArenaExprKind::Bool(found) if found == value) {
            return None;
        }
        // Only safe to remove when this is the last argument. xsh resolves named
        // arguments by their positional slot, so removing a non-last arg would shift
        // any subsequent args to the wrong parameter positions.
        if i + 1 < args.len() {
            return None;
        }
        // Deletion span includes the separator (comma + whitespace) so the result
        // is syntactically valid. Prefer consuming the preceding separator; if this
        // is the only argument, just span the arg itself.
        let deletion_span = if i > 0 {
            let prev = call_arg_span(arena, &args[i - 1])?;
            Span::new(prev.source_id, prev.end(), span.end())
        } else {
            span
        };
        Some((span, deletion_span))
    })
}

// Returns the minimum expected argument count for a module.function call that
// has a method equivalent, or None if no such mapping exists. The first argument
// is always the receiver, so min_args >= 1.
fn module_func_min_args(module: &str, func: &str) -> Option<usize> {
    match (module, func) {
        ("list", "len") => Some(1),
        ("list", "push") => Some(2),
        ("list", "extend") => Some(2),
        ("list", "contains") => Some(2),
        ("list", "get") => Some(2),
        ("map", "has") => Some(2),
        ("map", "get") => Some(2),
        ("map", "set") => Some(3),
        ("map", "remove") => Some(2),
        ("map", "keys") => Some(1),
        ("map", "values") => Some(1),
        // record.* stays as module functions — designed for Unknown/dynamic data (e.g. json.decode)
        ("text", "trim") => Some(1),
        ("text", "lines") => Some(1),
        ("text", "words") => Some(1),
        ("text", "split") => Some(2),
        ("text", "fields") => Some(1),
        ("text", "join") => Some(2),
        ("text", "replace") => Some(3),
        ("text", "starts_with") => Some(2),
        ("text", "ends_with") => Some(2),
        ("text", "wrap") => Some(2),
        ("text", "translate") => Some(3),
        ("text", "delete") => Some(2),
        ("text", "squeeze") => Some(1),
        ("text", "reverse") => Some(1),
        ("text", "count_lines") => Some(1),
        ("text", "count_words") => Some(1),
        ("text", "count_chars") => Some(1),
        ("text", "count_bytes") => Some(1),
        _ => None,
    }
}

/// Returns true if the expression is `.kind == "file"` — the inline Where
/// expression that `fs.walk |> where .kind == "file"` generates.
fn is_kind_eq_file_expr(arena: &AstArena, expr: ExprId) -> bool {
    let ArenaExprKind::Binary { op, left, right } = arena.expr(expr).kind else {
        return false;
    };
    if op != BinaryOp::Eq {
        return false;
    }
    let ArenaExprKind::Field { base, name } = arena.expr(left).kind else {
        return false;
    };
    if name != "kind" {
        return false;
    }
    if !matches!(arena.expr(base).kind, ArenaExprKind::Item) {
        return false;
    }
    matches!(arena.expr(right).kind, ArenaExprKind::Str(s) if arena.string_literal(s).as_ref() == "file")
}

fn is_fs_call(arena: &AstArena, callee: ExprId, method: &str) -> bool {
    is_module_call(arena, callee, "fs", method)
}

fn is_module_call(arena: &AstArena, callee: ExprId, module_name: &str, method: &str) -> bool {
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    name == method
        && matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == module_name)
}

fn is_method_call(arena: &AstArena, callee: ExprId, method: &str) -> bool {
    matches!(arena.expr(callee).kind, ArenaExprKind::Field { name, .. } if name == method)
}

fn expr_contains_read_text_lines_call(arena: &AstArena, expr: ExprId) -> bool {
    let rec = |id: ExprId| expr_contains_read_text_lines_call(arena, id);
    match arena.expr(expr).kind {
        ArenaExprKind::Call { callee, args } => {
            (matches!(arena.expr(callee).kind, ArenaExprKind::Field { base, name } if name == "lines" && expr_is_read_text_result(arena, base)))
                || rec(callee)
                || arena
                    .call_args(args)
                    .iter()
                    .any(|arg| call_arg_contains_read_text_lines_call(arena, arg))
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => rec(base),
        ArenaExprKind::Index { base, index } => rec(base) || rec(index),
        ArenaExprKind::Slice { base, start, end } => {
            rec(base) || start.is_some_and(rec) || end.is_some_and(rec)
        }
        ArenaExprKind::Pipeline { input, stages } => {
            rec(input)
                || arena
                    .pipe_stages(stages)
                    .to_vec()
                    .iter()
                    .any(|stage| match stage.kind {
                        ArenaPipeStageKind::Expr(expr) => rec(expr),
                        ArenaPipeStageKind::Stream(ref stage) => {
                            arena
                                .call_args(stage.args)
                                .iter()
                                .any(|arg| call_arg_contains_read_text_lines_call(arena, arg))
                                || stage.block.is_some_and(|block| {
                                    block_contains_read_text_lines_call(arena, block)
                                })
                        }
                    })
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            rec(input)
                || arena.stream_stages(stages).to_vec().iter().any(|stage| {
                    arena
                        .call_args(stage.args)
                        .iter()
                        .any(|arg| call_arg_contains_read_text_lines_call(arena, arg))
                        || stage
                            .block
                            .is_some_and(|block| block_contains_read_text_lines_call(arena, block))
                })
        }
        ArenaExprKind::List(items) => arena.expr_ids(items).any(rec),
        ArenaExprKind::ListComp {
            expr,
            iter,
            condition,
            ..
        } => rec(expr) || rec(iter) || condition.is_some_and(rec),
        ArenaExprKind::MapComp {
            key,
            value,
            iter,
            condition,
            ..
        } => rec(key) || rec(value) || rec(iter) || condition.is_some_and(rec),
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .any(|field| match field.kind {
                    ArenaRecordFieldKind::Named { value, .. } => rec(value),
                    ArenaRecordFieldKind::Spread { expr, .. } => rec(expr),
                    ArenaRecordFieldKind::Shorthand { .. } => false,
                })
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            arena
                .if_expr_branches(branches)
                .iter()
                .any(|branch| rec(branch.condition) || rec(branch.value))
                || rec(else_value)
        }
        ArenaExprKind::Match { value, arms } => {
            rec(value)
                || arena
                    .match_expr_arms(arms)
                    .iter()
                    .any(|arm| arm.guard.is_some_and(rec) || rec(arm.value))
        }
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => rec(expr),
        ArenaExprKind::Binary { left, right, .. } => rec(left) || rec(right),
        ArenaExprKind::Spawn(form) => match form.target {
            ArenaSpawnTarget::Command(expr) => rec(expr),
            ArenaSpawnTarget::Run(_) => false,
        },
        ArenaExprKind::Wait(form) => rec(form.target),
        ArenaExprKind::BuilderCall { call, block } => {
            rec(call)
                || arena
                    .builder_entries(arena.builder_block(block).entries)
                    .to_vec()
                    .iter()
                    .any(|entry| match entry.kind {
                        ArenaBuilderEntryKind::Field { value, .. } => rec(value),
                        ArenaBuilderEntryKind::Task { block, .. } => {
                            block_contains_read_text_lines_call(arena, block)
                        }
                        ArenaBuilderEntryKind::Stmt(stmt) => {
                            stmt_contains_read_text_lines_call(arena, stmt)
                        }
                        ArenaBuilderEntryKind::Entry { block, .. } => block.is_some_and(|block| {
                            arena
                                .builder_entries(arena.builder_block(block).entries)
                                .to_vec()
                                .iter()
                                .any(|entry| match entry.kind {
                                    ArenaBuilderEntryKind::Field { value, .. } => rec(value),
                                    ArenaBuilderEntryKind::Task { block, .. } => {
                                        block_contains_read_text_lines_call(arena, block)
                                    }
                                    ArenaBuilderEntryKind::Stmt(stmt) => {
                                        stmt_contains_read_text_lines_call(arena, stmt)
                                    }
                                    ArenaBuilderEntryKind::Entry { .. } => false,
                                })
                        }),
                    })
        }
        ArenaExprKind::Require { value, .. } => rec(value),
        ArenaExprKind::Loop { block } => block_contains_read_text_lines_call(arena, block),
        ArenaExprKind::Retry { delays, block } => {
            arena.expr_ids(delays).any(rec) || block_contains_read_text_lines_call(arena, block)
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            arena.fmt_parts(parts).any(|part| match part {
                ArenaFmtPart::Expr(expr, _) => rec(expr),
                ArenaFmtPart::Text(_) => false,
            })
        }
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvGet { .. }
        | ArenaExprKind::EnvPathList
        | ArenaExprKind::Run(_) => false,
    }
}

fn expr_is_read_text_result(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Try(inner) => expr_is_read_text_call(arena, inner),
        _ => expr_is_read_text_call(arena, expr),
    }
}

fn expr_is_read_text_call(arena: &AstArena, expr: ExprId) -> bool {
    matches!(
        arena.expr(expr).kind,
        ArenaExprKind::Call { callee, .. }
            if matches!(arena.expr(callee).kind, ArenaExprKind::Field { name, .. } if name == "read_text")
    )
}

fn call_arg_contains_read_text_lines_call(arena: &AstArena, arg: &ArenaCallArg) -> bool {
    match arg.kind {
        ArenaCallArgKind::Positional(expr)
        | ArenaCallArgKind::Named { value: expr, .. }
        | ArenaCallArgKind::Splice { value: expr, .. } => {
            expr_contains_read_text_lines_call(arena, expr)
        }
    }
}

fn block_contains_read_text_lines_call(arena: &AstArena, block: BlockId) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .any(|stmt| stmt_contains_read_text_lines_call(arena, stmt))
}

fn stmt_contains_read_text_lines_call(arena: &AstArena, stmt: StmtId) -> bool {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Let { initializer, .. }
        | ArenaStmtKind::Var { initializer, .. }
        | ArenaStmtKind::Assign {
            value: initializer, ..
        }
        | ArenaStmtKind::Defer(initializer)
        | ArenaStmtKind::Return(Some(initializer))
        | ArenaStmtKind::Yield(initializer) => match initializer {
            ArenaExprOrRun::Expr(expr) => expr_contains_read_text_lines_call(arena, expr),
            ArenaExprOrRun::Run(_) => false,
        },
        ArenaStmtKind::Expr(expr) | ArenaStmtKind::Break { value: Some(expr) } => {
            expr_contains_read_text_lines_call(arena, expr)
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena.if_branches(branches).iter().any(|branch| {
                expr_contains_read_text_lines_call(arena, branch.condition)
                    || block_contains_read_text_lines_call(arena, branch.block)
            }) || else_block.is_some_and(|block| block_contains_read_text_lines_call(arena, block))
        }
        ArenaStmtKind::While { condition, block } => {
            expr_contains_read_text_lines_call(arena, condition)
                || block_contains_read_text_lines_call(arena, block)
        }
        ArenaStmtKind::For { iter, block, .. } => {
            expr_contains_read_text_lines_call(arena, iter)
                || block_contains_read_text_lines_call(arena, block)
        }
        ArenaStmtKind::Loop { block } => block_contains_read_text_lines_call(arena, block),
        ArenaStmtKind::Guard {
            initializer,
            else_block,
            ..
        } => {
            matches!(initializer, ArenaExprOrRun::Expr(expr) if expr_contains_read_text_lines_call(arena, expr))
                || block_contains_read_text_lines_call(arena, else_block)
        }
        ArenaStmtKind::GuardedStmt {
            stmt, condition, ..
        } => {
            expr_contains_read_text_lines_call(arena, condition)
                || stmt_contains_read_text_lines_call(arena, stmt)
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
            ..
        } => {
            arena
                .with_bindings(bindings)
                .iter()
                .any(|binding| expr_contains_read_text_lines_call(arena, binding.initializer))
                || block_contains_read_text_lines_call(arena, body)
                || block_contains_read_text_lines_call(arena, else_block)
        }
        ArenaStmtKind::Match { value, arms } => {
            expr_contains_read_text_lines_call(arena, value)
                || arena.match_arms(arms).iter().any(|arm| {
                    arm.guard
                        .is_some_and(|g| expr_contains_read_text_lines_call(arena, g))
                        || block_contains_read_text_lines_call(arena, arm.block)
                })
        }
        ArenaStmtKind::Return(None)
        | ArenaStmtKind::Break { value: None }
        | ArenaStmtKind::Continue
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Use(_)
        | ArenaStmtKind::Export(_)
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_)
        | ArenaStmtKind::SignalHook(_)
        | ArenaStmtKind::Command(_) => false,
    }
}

fn simple_command_value_expr(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_) => true,
        ArenaExprKind::Field { base, .. } => simple_command_value_expr(arena, base),
        _ => false,
    }
}

struct LintExprVisitor<'a, 'b> {
    linter: &'a mut Linter<'b>,
    suppress_expr_autofixes: bool,
}

impl LintExprVisitor<'_, '_> {
    fn visit_expr(&mut self, expr: ExprId) {
        if !self.suppress_expr_autofixes {
            self.linter.lint_path_roundtrip(expr);
            self.linter.lint_redundant_require(expr);
            self.linter.lint_redundant_single_interpolation(expr);
            self.linter.lint_scalar_display_parse_roundtrip(expr);
            self.linter.lint_json_encode_decode_roundtrip(expr);
        }
        let arena_expr = self.linter.arena.expr(expr);
        match arena_expr.kind {
            ArenaExprKind::Ident(name) => self.linter.mark_used(name.as_str().as_str()),
            ArenaExprKind::Call { callee, args } => {
                self.linter.lint_call_style(callee, args, arena_expr.span);
                self.walk_expr(expr);
            }
            _ => self.walk_expr(expr),
        }
    }

    fn walk_expr(&mut self, expr: ExprId) {
        let arena = self.linter.arena;
        match arena.expr(expr).kind {
            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
                for part in arena.fmt_parts(parts).collect::<Vec<_>>() {
                    if let ArenaFmtPart::Expr(e, _) = part {
                        self.visit_expr(e);
                    }
                }
            }
            ArenaExprKind::List(items) => {
                for item in arena.expr_ids(items).collect::<Vec<_>>() {
                    self.visit_expr(item);
                }
            }
            ArenaExprKind::ListComp {
                expr,
                iter,
                condition,
                ..
            } => {
                self.visit_expr(expr);
                self.visit_expr(iter);
                if let Some(cond) = condition {
                    self.visit_expr(cond);
                }
            }
            ArenaExprKind::MapComp {
                key,
                value,
                iter,
                condition,
                ..
            } => {
                self.visit_expr(key);
                self.visit_expr(value);
                self.visit_expr(iter);
                if let Some(cond) = condition {
                    self.visit_expr(cond);
                }
            }
            ArenaExprKind::Record(fields) => {
                for field in arena.record_fields(fields).to_vec() {
                    self.visit_record_field(&field);
                }
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                for branch in arena.if_expr_branches(branches).to_vec() {
                    self.visit_expr(branch.condition);
                    self.visit_expr(branch.value);
                }
                self.visit_expr(else_value);
            }
            ArenaExprKind::Match { value, arms } => {
                self.visit_expr(value);
                for arm in arena.match_expr_arms(arms).to_vec() {
                    self.visit_match_expr_arm(&arm);
                }
            }
            ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => self.visit_expr(expr),
            ArenaExprKind::Binary { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ArenaExprKind::Call { callee, args } => {
                self.visit_expr(callee);
                for arg in arena.call_args(args).to_vec() {
                    self.visit_call_arg(&arg);
                }
            }
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.visit_expr(base)
            }
            ArenaExprKind::Index { base, index } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            ArenaExprKind::Slice { base, start, end } => {
                self.visit_expr(base);
                if let Some(start) = start {
                    self.visit_expr(start);
                }
                if let Some(end) = end {
                    self.visit_expr(end);
                }
            }
            ArenaExprKind::Pipeline { input, stages } => {
                self.visit_expr(input);
                for stage in arena.pipe_stages(stages).to_vec() {
                    self.visit_pipe_stage(&stage);
                }
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.visit_expr(input);
                for stage in arena.stream_stages(stages).to_vec() {
                    self.visit_stream_stage(&stage);
                }
                // fs.walk(X) |> where .kind == "file" → fs.files(X)
                self.linter.lint_prefer_fs_files(input, stages);
                self.linter.lint_redundant_stream_stages(stages);
            }
            ArenaExprKind::Run(run) => self.visit_run_form(run),
            ArenaExprKind::Spawn(form) => match form.target {
                ArenaSpawnTarget::Run(run) => self.visit_run_form(run),
                ArenaSpawnTarget::Command(expr) => self.visit_expr(expr),
            },
            ArenaExprKind::Wait(form) => self.visit_expr(form.target),
            ArenaExprKind::BuilderCall { call, block } => {
                self.visit_expr(call);
                self.visit_builder_block(block);
            }
            ArenaExprKind::Require { value, schema } => {
                self.visit_expr(value);
                self.linter.collect_type_expr_refs(schema);
            }
            ArenaExprKind::Loop { block } => self.linter.lint_block(block),
            ArenaExprKind::Retry { delays, block } => {
                for delay in arena.expr_ids(delays).collect::<Vec<_>>() {
                    self.visit_expr(delay);
                }
                self.linter.lint_block(block);
            }
            ArenaExprKind::Str(_) => {
                self.linter.lint_dollar_in_expression_string(expr);
                if !self.suppress_expr_autofixes {
                    self.linter.lint_redundant_newline_triple_string(expr);
                }
            }
            ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::Ident(_)
            | ArenaExprKind::EnvGet { .. }
            | ArenaExprKind::EnvPathList
            | ArenaExprKind::Item
            | ArenaExprKind::LastStatus => {}
        }
    }

    fn visit_record_field(&mut self, field: &ArenaRecordField) {
        match field.kind {
            ArenaRecordFieldKind::Shorthand { name, .. } => {
                self.linter.mark_used(name.as_str().as_str())
            }
            ArenaRecordFieldKind::Named { value, .. } => self.visit_expr(value),
            ArenaRecordFieldKind::Spread { expr, .. } => self.visit_expr(expr),
        }
    }

    fn visit_match_expr_arm(&mut self, arm: &ArenaMatchExprArm) {
        self.linter.push_scope();
        self.linter.lint_pattern(arm.pattern);
        if let Some(guard) = arm.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(arm.value);
        self.linter.pop_scope();
    }

    fn visit_call_arg(&mut self, arg: &ArenaCallArg) {
        match arg.kind {
            ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. } => {
                self.visit_expr(expr);
            }
            ArenaCallArgKind::Splice { value, .. } => self.visit_expr(value),
        }
    }

    fn visit_pipe_stage(&mut self, stage: &ArenaPipeStage) {
        match stage.kind {
            ArenaPipeStageKind::Expr(expr) => self.visit_expr(expr),
            ArenaPipeStageKind::Stream(ref stage) => self.visit_stream_stage(stage),
        }
    }

    fn visit_command_arg(&mut self, arg: &ArenaCommandArg, allow_bare_refs: bool) {
        let arg_span = self.linter.arena.span(arg.span);
        match arg.kind {
            ArenaCommandArgKind::SpliceName(name) => self.linter.mark_used(name.as_str().as_str()),
            ArenaCommandArgKind::Word(parts) => {
                self.linter.lint_redundant_command_arg_interpolation(arg);
                let part_list: Vec<ArenaWordPart> = self.linter.arena.word_parts(parts).collect();
                if allow_bare_refs
                    && let Some(text) =
                        bare_command_word_parts(self.linter.arena, self.linter.source, &part_list)
                    && let Some((root, _)) = parse_command_word_reference(&text)
                {
                    self.linter.mark_used(root);
                }
                // ${f"..."} is a Word with a single Interpolation whose expr is an FmtString.
                // Since f"..." is now accepted directly as a typed command arg, the wrapper is
                // redundant.
                if let [ArenaWordPart::Interpolation(expr)] = part_list.as_slice()
                    && matches!(
                        self.linter.arena.expr(*expr).kind,
                        ArenaExprKind::FmtString(_)
                    )
                    && self.linter.source.as_bytes().get(arg_span.start()) == Some(&b'$')
                {
                    let expr_span = self.linter.arena.expr(*expr).span;
                    let replacement =
                        self.linter.source[expr_span.start()..expr_span.end()].to_string();
                    self.linter.diagnostics.push(
                        Diagnostic::new(Severity::Warning, "redundant `${}` around f-string")
                            .with_code("lint.redundant-fmt-wrapper")
                            .with_span(arg_span)
                            .with_fix_hint(FixHint::replacement(
                                arg_span,
                                "remove the `${}` wrapper",
                                replacement,
                            )),
                    );
                }
                for part in &part_list {
                    if let ArenaWordPart::Interpolation(expr) = *part {
                        let expr_span = self.linter.arena.expr(expr).span;
                        // ${foo.display()} → $foo (single interp, simple ident base)
                        if let Some((_base_span, base_text)) = self.linter.path_display_base(expr) {
                            if part_list.len() == 1
                                && matches!(
                                    base_text.as_bytes().first(),
                                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')
                                )
                            {
                                let replacement = format!("${base_text}");
                                self.linter.diagnostics.push(
                                    Diagnostic::new(
                                        Severity::Warning,
                                        "redundant `.display()` on a Path value",
                                    )
                                    .with_code("lint.redundant-path-display")
                                    .with_label(Label::secondary(
                                        expr_span,
                                        "Path values display automatically in command arguments",
                                    ))
                                    .with_fix_hint(
                                        FixHint::replacement(
                                            arg_span,
                                            "use `$` shorthand",
                                            replacement,
                                        ),
                                    ),
                                );
                            } else {
                                self.linter.lint_redundant_path_display(expr);
                            }
                        }
                        self.visit_command_embedded_expr(expr);
                    } else if let ArenaWordPart::Shorthand(expr) = *part {
                        self.linter.lint_redundant_path_display(expr);
                        self.visit_command_embedded_expr(expr);
                    }
                }
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                self.linter.lint_redundant_path_display(expr);
                self.visit_command_embedded_expr(expr);
            }
            ArenaCommandArgKind::Typed(expr) => {
                let expr_span = self.linter.arena.expr(expr).span;
                // Implicit typed args (no parens, detected by at_call_or_index_chain)
                // need `$` prefix after .display() removal so the result stays a
                // variable reference. E.g. `input.display()` → `$input`.
                if self.linter.source.as_bytes().get(arg_span.start()) != Some(&b'(') {
                    if let Some(base_text) = self
                        .linter
                        .path_display_base(expr)
                        .map(|(_, t)| t)
                        .filter(|t| {
                            !t.starts_with('$')
                                && t.as_bytes()
                                    .first()
                                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
                                && t.bytes()
                                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
                        })
                    {
                        self.linter.diagnostics.push(
                            Diagnostic::new(
                                Severity::Warning,
                                "redundant `.display()` on a Path value",
                            )
                            .with_code("lint.redundant-path-display")
                            .with_label(Label::secondary(
                                expr_span,
                                "Path values display automatically in command arguments",
                            ))
                            .with_fix_hint(FixHint::replacement(
                                arg_span,
                                "use `$` shorthand",
                                format!("${base_text}"),
                            )),
                        );
                    } else {
                        self.linter.lint_redundant_path_display(expr);
                    }
                } else {
                    self.linter.lint_redundant_path_display(expr);
                }
                // (f"...") → f"..." : f-strings don't need paren wrapping
                if matches!(
                    self.linter.arena.expr(expr).kind,
                    ArenaExprKind::FmtString(_) | ArenaExprKind::PathFmtString(_)
                ) && self.linter.source.as_bytes().get(arg_span.start()) == Some(&b'(')
                {
                    let replacement =
                        self.linter.source[expr_span.start()..expr_span.end()].to_string();
                    self.linter.diagnostics.push(
                        Diagnostic::new(Severity::Warning, "redundant `()` around f-string")
                            .with_code("lint.redundant-fmt-wrapper")
                            .with_span(arg_span)
                            .with_fix_hint(FixHint::replacement(
                                arg_span,
                                "remove the `()` wrapper",
                                replacement,
                            )),
                    );
                } else if let Some(replacement) = self.linter.command_single_fmt_replacement(expr) {
                    self.linter.diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "redundant single-value command f-string",
                        )
                        .with_code("lint.redundant-command-fmt")
                        .with_label(Label::secondary(
                            expr_span,
                            "use command value syntax directly",
                        ))
                        .with_fix_hint(FixHint::replacement(
                            arg_span,
                            "use command value syntax",
                            replacement,
                        )),
                    );
                } else if simple_command_value_expr(self.linter.arena, expr) {
                    let replacement = command_value_replacement(self.linter.arena, expr);
                    self.linter.diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "stale parenthesized command value syntax",
                        )
                        .with_code("lint.command-value")
                        .with_label(Label::secondary(
                            arg_span,
                            "use `$name`, `$record.field`, or `${expr}`",
                        ))
                        .with_fix_hint(FixHint::replacement(
                            arg_span,
                            "replace with modern syntax",
                            replacement,
                        )),
                    );
                }
                self.visit_command_embedded_expr(expr);
            }
        }
    }

    fn visit_command_embedded_expr(&mut self, expr: ExprId) {
        let old = self.suppress_expr_autofixes;
        self.suppress_expr_autofixes = true;
        self.visit_expr(expr);
        self.suppress_expr_autofixes = old;
    }

    fn visit_run_form(&mut self, run: RunFormId) {
        let arena = self.linter.arena;
        let run_form = arena.run_form(run).clone();
        for segment in arena.run_segments(run_form.segments).to_vec() {
            let seg_span = arena.span(segment.span);
            if segment.kind == RunKind::Plain
                && run_form.propagate
                && let Some(target) =
                    literal_command_word(arena, self.linter.source, &segment.target)
                && expects_nonzero_status(&target)
            {
                let deletion_span =
                    scan_run_propagate_deletion_span(self.linter.source, arena.span(run_form.span));
                self.linter.diagnostics.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "remove `?` when inspecting an expected nonzero status",
                    )
                    .with_code("lint.run-status")
                    .with_label(Label::secondary(
                        seg_span,
                        "nonzero status is expected for this command",
                    ))
                    .with_fix_hint(FixHint::deletion(
                        deletion_span,
                        "remove status propagation",
                    )),
                );
            }
        }
        for segment in arena.run_segments(run_form.segments).to_vec() {
            if let Some(timeout) = segment.timeout {
                self.visit_expr(timeout);
            }
            if let Some(cpu_max) = segment.cpu_max {
                self.visit_expr(cpu_max);
            }
            for assignment in arena.env_assignments(segment.env).to_vec() {
                self.visit_env_assignment(&assignment);
            }
            self.visit_command_arg(&segment.target, false);
            for arg in arena.command_args(segment.args).to_vec() {
                self.visit_command_arg(&arg, false);
            }
            for redirection in arena.redirections(segment.redirections).to_vec() {
                self.visit_redirection(&redirection);
            }
        }
    }

    fn visit_env_assignment(&mut self, assignment: &ArenaEnvAssignment) {
        match assignment.value {
            ArenaEnvAssignmentValue::CommandArg(ref arg) => self.visit_command_arg(arg, true),
            ArenaEnvAssignmentValue::Expr(expr) => self.visit_expr(expr),
        }
    }

    fn visit_redirection(&mut self, redirection: &ArenaRedirection) {
        match redirection.target {
            ArenaRedirectionTarget::Path(ref arg) | ArenaRedirectionTarget::Fd(ref arg) => {
                self.visit_command_arg(arg, false);
            }
        }
    }

    fn visit_stream_stage(&mut self, stage: &ArenaStreamStage) {
        self.linter.lint_stream_stage(stage);
    }

    fn visit_builder_block(&mut self, block: BuilderBlockId) {
        self.linter.lint_builder_block(block);
    }
}

fn is_predeclared_script_args(name: &str) -> bool {
    matches!(name, "args" | "ARGV")
}

fn literal_command_word(arena: &AstArena, source: &str, arg: &ArenaCommandArg) -> Option<String> {
    let ArenaCommandArgKind::Word(parts) = arg.kind else {
        return None;
    };
    let parts: Vec<ArenaWordPart> = arena.word_parts(parts).collect();
    literal_command_word_parts(arena, source, &parts)
}

fn command_name(arena: &AstArena, source: &str, arg: &ArenaCommandArg) -> Option<String> {
    match arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            let parts: Vec<ArenaWordPart> = arena.word_parts(parts).collect();
            match parts.as_slice() {
                [ArenaWordPart::Bare(text)] | [ArenaWordPart::Quoted(text)] => {
                    arena.text_value(text, source).map(str::to_string)
                }
                [ArenaWordPart::Shorthand(expr)] => {
                    if let ArenaExprKind::Ident(name) = arena.expr(*expr).kind {
                        Some(name.to_string())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        ArenaCommandArgKind::SpliceName(name) => Some(name.to_string()),
        _ => None,
    }
}

fn literal_command_word_parts(
    arena: &AstArena,
    source: &str,
    parts: &[ArenaWordPart],
) -> Option<String> {
    match parts {
        [ArenaWordPart::Bare(value)] | [ArenaWordPart::Quoted(value)] => {
            arena.text_value(value, source).map(str::to_string)
        }
        _ => None,
    }
}

fn bare_command_word_parts(
    arena: &AstArena,
    source: &str,
    parts: &[ArenaWordPart],
) -> Option<String> {
    match parts {
        [ArenaWordPart::Bare(value)] => arena.text_value(value, source).map(str::to_string),
        _ => None,
    }
}

fn expects_nonzero_status(target: &str) -> bool {
    matches!(target, "false" | "grep" | "test" | "[" | "cmp" | "diff")
}

fn command_value_replacement(arena: &AstArena, expr: ExprId) -> String {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(name) => format!("${name}"),
        ArenaExprKind::Field { base, name } => {
            format!("{}.{name}", command_value_replacement(arena, base))
        }
        _ => String::new(),
    }
}

fn scan_effect_list_span(
    arena: &AstArena,
    def: &ArenaFunctionDef,
    stmt_span: Span,
    source: &str,
) -> Option<Span> {
    let signature_end = if def.return_ty_defaulted {
        arena.span(arena.block(def.body).span).start()
    } else {
        scan_before_arrow(source, arena.type_expr_span(def.return_ty).start())
    };
    let signature = source.get(stmt_span.start()..signature_end)?;
    let open = signature.rfind('[')? + stmt_span.start();
    let close = source.get(open..signature_end)?.find(']')? + open + 1;
    Some(Span::new(stmt_span.source_id, open, close))
}

/// Scan backward from `ty_start` past whitespace and the `->` arrow to find the
/// start of the ` -> TypeName` annotation so it can be deleted in one span.
fn scan_before_arrow(source: &str, ty_start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = ty_start;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    if i >= 2 && bytes[i - 2] == b'-' && bytes[i - 1] == b'>' {
        i -= 2;
        while i > 0 && bytes[i - 1] == b' ' {
            i -= 1;
        }
    }
    i
}

fn scan_before_colon(source: &str, ty_start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = ty_start;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    if i > 0 && bytes[i - 1] == b':' {
        i -= 1;
        while i > 0 && bytes[i - 1] == b' ' {
            i -= 1;
        }
    }
    i
}

fn scan_after_type(_source: &str, ty_end: usize) -> usize {
    ty_end
}

fn scan_run_propagate_deletion_span(source: &str, run_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut end = run_span.end();
    while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'?' {
        end += 1;
        Span::new(run_span.source_id, run_span.end(), end)
    } else {
        Span::new(run_span.source_id, run_span.end(), run_span.end())
    }
}

fn span_end_after_following_newlines(source: &str, mut end: usize) -> usize {
    let bytes = source.as_bytes();
    while end < bytes.len() && matches!(bytes[end], b'\r' | b'\n') {
        end += 1;
    }
    end
}

/// Compute a deletion span for a bare `return` statement covering the full source
/// line: leading indentation, the keyword, and the trailing newline.
fn scan_return_stmt_span(source: &str, stmt_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut start = stmt_span.start();
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    let mut end = stmt_span.end();
    while end < bytes.len() && bytes[end] == b' ' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'\r' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'\n' {
        end += 1;
    }
    Span::new(stmt_span.source_id, start, end)
}

/// Scan backward from `pos` over any spaces, to include the separator whitespace
/// before a token in its deletion span.
fn scan_back_space(source: &str, pos: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = pos;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    i
}

fn scan_pipe_stage_deletion_span(source: &str, stage_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut start = stage_span.start();
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    if start >= 2 && bytes[start - 2] == b'|' && bytes[start - 1] == b'>' {
        start -= 2;
        while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
            start -= 1;
        }
    }
    Span::new(stage_span.source_id, start, stage_span.end())
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ImportSortKey {
    path: String,
    alias: Option<String>,
}

fn import_sort_key(arena: &AstArena, stmt: StmtId) -> ImportSortKey {
    let ArenaStmtKind::Use(use_id) = arena.stmt(stmt).kind else {
        unreachable!("import block contains only use statements");
    };
    let use_stmt = arena.use_stmt(use_id);
    ImportSortKey {
        path: arena
            .names(use_stmt.path)
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join("."),
        alias: use_stmt.alias.map(|name| name.to_string()),
    }
}

fn import_text(path: &str, alias: Option<&str>) -> String {
    let mut text = format!("use {path}");
    if let Some(alias) = alias {
        text.push_str(" as ");
        text.push_str(alias);
    }
    text
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TopLevelPhase {
    Use,
    SafeConst,
    Type,
    Function,
    Body,
}

fn top_level_phase(arena: &AstArena, stmt: StmtId, source: &str) -> TopLevelPhase {
    let arena_stmt = arena.stmt(stmt);
    match arena_stmt.kind {
        ArenaStmtKind::Use(_) => TopLevelPhase::Use,
        ArenaStmtKind::TypeDef(_) => TopLevelPhase::Type,
        ArenaStmtKind::ProcDef(_) | ArenaStmtKind::PureDef(_) | ArenaStmtKind::SignalHook(_) => {
            TopLevelPhase::Function
        }
        ArenaStmtKind::Let {
            target,
            initializer,
            ..
        } if is_safe_top_level_const(arena, target, &initializer, arena_stmt.span, source) => {
            TopLevelPhase::SafeConst
        }
        ArenaStmtKind::Export(inner) => match arena.stmt(inner).kind {
            ArenaStmtKind::Let {
                target,
                initializer,
                ..
            } if is_safe_top_level_const(arena, target, &initializer, arena_stmt.span, source) => {
                TopLevelPhase::SafeConst
            }
            ArenaStmtKind::TypeDef(_) => TopLevelPhase::Type,
            ArenaStmtKind::ProcDef(_)
            | ArenaStmtKind::PureDef(_)
            | ArenaStmtKind::SignalHook(_) => TopLevelPhase::Function,
            _ => TopLevelPhase::Body,
        },
        _ => TopLevelPhase::Body,
    }
}

fn is_safe_top_level_const(
    arena: &AstArena,
    target: BindingTargetId,
    initializer: &ArenaExprOrRun,
    span: Span,
    source: &str,
) -> bool {
    matches!(
        arena.binding_target(target).kind,
        ArenaBindingTargetKind::Name(_)
    ) && !source[span.start()..span.end()].contains('#')
        && matches!(initializer, ArenaExprOrRun::Expr(expr) if is_safe_const_expr(arena, *expr))
}

fn is_safe_const_expr(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_) => true,
        ArenaExprKind::List(items) => arena
            .expr_ids(items)
            .all(|item| is_safe_const_expr(arena, item)),
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .all(|field| match field.kind {
                    ArenaRecordFieldKind::Named { value, .. } => is_safe_const_expr(arena, value),
                    ArenaRecordFieldKind::Shorthand { .. }
                    | ArenaRecordFieldKind::Spread { .. } => false,
                })
        }
        ArenaExprKind::Unary { expr, .. } => is_safe_const_expr(arena, expr),
        ArenaExprKind::Binary { left, right, .. } => {
            is_safe_const_expr(arena, left) && is_safe_const_expr(arena, right)
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .all(|part| matches!(part, ArenaFmtPart::Text(_))),
        ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::ListComp { .. }
        | ArenaExprKind::MapComp { .. }
        | ArenaExprKind::If { .. }
        | ArenaExprKind::Match { .. }
        | ArenaExprKind::Call { .. }
        | ArenaExprKind::Field { .. }
        | ArenaExprKind::NullSafeField { .. }
        | ArenaExprKind::Index { .. }
        | ArenaExprKind::Slice { .. }
        | ArenaExprKind::EnvGet { .. }
        | ArenaExprKind::EnvPathList
        | ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Spawn(_)
        | ArenaExprKind::Wait(_)
        | ArenaExprKind::BuilderCall { .. }
        | ArenaExprKind::Try(_)
        | ArenaExprKind::Require { .. }
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::Retry { .. } => false,
    }
}

fn prefer_in_receiver_type(ty: &Type) -> bool {
    matches!(ty, Type::List(_) | Type::Str | Type::Bytes | Type::Path)
}

fn directly_negated_start(source: &str, span: Span) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut pos = span.start();
    while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
        pos -= 1;
    }
    pos.checked_sub(1).filter(|&bang| bytes[bang] == b'!')
}

fn expr_may_have_effects(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvPathList => false,
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .any(|part| matches!(part, ArenaFmtPart::Expr(expr, _) if expr_may_have_effects(arena, expr))),
        ArenaExprKind::List(items) => arena.expr_ids(items).any(|item| expr_may_have_effects(arena, item)),
        ArenaExprKind::Record(fields) => arena.record_fields(fields).iter().any(|field| match field.kind {
            ArenaRecordFieldKind::Named { value, .. } => expr_may_have_effects(arena, value),
            ArenaRecordFieldKind::Spread { expr, .. } => expr_may_have_effects(arena, expr),
            ArenaRecordFieldKind::Shorthand { .. } => false,
        }),
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => {
            expr_may_have_effects(arena, expr)
        }
        ArenaExprKind::Binary { left, right, .. } => {
            expr_may_have_effects(arena, left) || expr_may_have_effects(arena, right)
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            expr_may_have_effects(arena, base)
        }
        ArenaExprKind::Index { base, index } => {
            expr_may_have_effects(arena, base) || expr_may_have_effects(arena, index)
        }
        ArenaExprKind::Slice { base, start, end } => {
            expr_may_have_effects(arena, base)
                || start.is_some_and(|start| expr_may_have_effects(arena, start))
                || end.is_some_and(|end| expr_may_have_effects(arena, end))
        }
        ArenaExprKind::Call { callee, args } => {
            !pure_method_call_for_prefer_in(arena, callee)
                || arena
                    .call_args(args)
                    .iter()
                    .any(|arg| call_arg_may_have_effects(arena, arg))
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            arena.if_expr_branches(branches).iter().any(|branch| {
                expr_may_have_effects(arena, branch.condition)
                    || expr_may_have_effects(arena, branch.value)
            }) || expr_may_have_effects(arena, else_value)
        }
        ArenaExprKind::Match { value, arms } => {
            expr_may_have_effects(arena, value)
                || arena.match_expr_arms(arms).iter().any(|arm| {
                    arm.guard
                        .is_some_and(|guard| expr_may_have_effects(arena, guard))
                        || expr_may_have_effects(arena, arm.value)
                })
        }
        ArenaExprKind::ListComp { .. }
        | ArenaExprKind::MapComp { .. }
        | ArenaExprKind::EnvGet { .. }
        | ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Spawn(_)
        | ArenaExprKind::Wait(_)
        | ArenaExprKind::BuilderCall { .. }
        | ArenaExprKind::Require { .. }
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::Retry { .. } => true,
    }
}

fn call_arg_may_have_effects(arena: &AstArena, arg: &ArenaCallArg) -> bool {
    match arg.kind {
        ArenaCallArgKind::Positional(expr)
        | ArenaCallArgKind::Named { value: expr, .. }
        | ArenaCallArgKind::Splice { value: expr, .. } => expr_may_have_effects(arena, expr),
    }
}

fn pure_method_call_for_prefer_in(arena: &AstArena, callee: ExprId) -> bool {
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    matches!(
        name.as_str().as_str(),
        "display"
            | "name"
            | "stem"
            | "ext"
            | "parent"
            | "join"
            | "len"
            | "lower"
            | "upper"
            | "trim"
            | "starts_with"
            | "ends_with"
            | "split"
            | "fields"
            | "words"
            | "replace"
    ) && !expr_may_have_effects(arena, base)
}

fn effects_annotation(effects: &FxHashSet<Effect>) -> String {
    let canonical = [
        Effect::Fs,
        Effect::Net,
        Effect::Process,
        Effect::Env,
        Effect::Time,
        Effect::Error,
        Effect::Io,
    ];
    canonical
        .iter()
        .filter(|e| effects.contains(*e))
        .map(|e| e.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn effects_covers_any(declared: &[Effect], required: &Effect) -> bool {
    declared.contains(required)
        || declared.contains(&Effect::Io)
            && matches!(
                required,
                Effect::Fs | Effect::Net | Effect::Process | Effect::Env
            )
}

fn collect_block_effects(
    arena: &AstArena,
    block: BlockId,
    effects: &mut FxHashSet<Effect>,
    proc_effects: Option<&FxHashMap<String, Option<Vec<Effect>>>>,
) {
    for stmt in arena
        .stmt_ids(arena.block(block).statements)
        .collect::<Vec<_>>()
    {
        collect_stmt_effects(arena, stmt, effects, proc_effects);
    }
}

fn collect_stmt_effects(
    arena: &AstArena,
    stmt: StmtId,
    effects: &mut FxHashSet<Effect>,
    proc_effects: Option<&FxHashMap<String, Option<Vec<Effect>>>>,
) {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Var { initializer, .. } => {
            collect_expr_or_run_effects(arena, &initializer, effects, proc_effects);
        }
        ArenaStmtKind::Assign { value, .. } => {
            collect_expr_or_run_effects(arena, &value, effects, proc_effects)
        }
        ArenaStmtKind::Return(Some(v)) | ArenaStmtKind::Defer(v) | ArenaStmtKind::Yield(v) => {
            collect_expr_or_run_effects(arena, &v, effects, proc_effects);
        }
        ArenaStmtKind::Return(None)
        | ArenaStmtKind::Break { .. }
        | ArenaStmtKind::Continue
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Use(_)
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::StreamDef(_) => {}
        ArenaStmtKind::SignalHook(hook) => {
            collect_block_effects(arena, arena.signal_hook(hook).body, effects, proc_effects)
        }
        ArenaStmtKind::Loop { block } => collect_block_effects(arena, block, effects, proc_effects),
        ArenaStmtKind::Guard {
            initializer,
            else_block,
            ..
        } => {
            collect_expr_or_run_effects(arena, &initializer, effects, proc_effects);
            collect_block_effects(arena, else_block, effects, proc_effects);
        }
        ArenaStmtKind::GuardedStmt {
            stmt: inner,
            condition,
            ..
        } => {
            collect_expr_effects(arena, condition, effects, proc_effects);
            collect_stmt_effects(arena, inner, effects, proc_effects);
        }
        ArenaStmtKind::Export(inner) => collect_stmt_effects(arena, inner, effects, proc_effects),
        // Don't descend into nested function defs — they have their own effect scope
        ArenaStmtKind::ProcDef(_) | ArenaStmtKind::PureDef(_) => {}
        ArenaStmtKind::Expr(e) => collect_expr_effects(arena, e, effects, proc_effects),
        ArenaStmtKind::Command(cmd) => {
            collect_command_effects(&arena.command_stmt(cmd).command, effects)
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            for b in arena.if_branches(branches).to_vec() {
                collect_expr_effects(arena, b.condition, effects, proc_effects);
                collect_block_effects(arena, b.block, effects, proc_effects);
            }
            if let Some(block) = else_block {
                collect_block_effects(arena, block, effects, proc_effects);
            }
        }
        ArenaStmtKind::While { condition, block } => {
            collect_expr_effects(arena, condition, effects, proc_effects);
            collect_block_effects(arena, block, effects, proc_effects);
        }
        ArenaStmtKind::For { iter, block, .. } => {
            collect_expr_effects(arena, iter, effects, proc_effects);
            collect_block_effects(arena, block, effects, proc_effects);
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
            ..
        } => {
            for b in arena.with_bindings(bindings).to_vec() {
                collect_expr_effects(arena, b.initializer, effects, proc_effects);
            }
            collect_block_effects(arena, body, effects, proc_effects);
            collect_block_effects(arena, else_block, effects, proc_effects);
        }
        ArenaStmtKind::Match { value, arms } => {
            collect_expr_effects(arena, value, effects, proc_effects);
            for arm in arena.match_arms(arms).to_vec() {
                if let Some(g) = arm.guard {
                    collect_expr_effects(arena, g, effects, proc_effects);
                }
                collect_block_effects(arena, arm.block, effects, proc_effects);
            }
        }
    }
}

fn collect_command_effects(cmd: &ArenaCommand, effects: &mut FxHashSet<Effect>) {
    match cmd {
        ArenaCommand::Run(_) => {
            effects.insert(Effect::Process);
        }
        ArenaCommand::Core {
            name: CoreCommand::Cd | CoreCommand::Env,
            ..
        } => {
            effects.insert(Effect::Env);
        }
        ArenaCommand::Core { .. } | ArenaCommand::Proc { .. } => {}
    }
}

fn collect_expr_or_run_effects(
    arena: &AstArena,
    v: &ArenaExprOrRun,
    effects: &mut FxHashSet<Effect>,
    proc_effects: Option<&FxHashMap<String, Option<Vec<Effect>>>>,
) {
    match v {
        ArenaExprOrRun::Expr(e) => collect_expr_effects(arena, *e, effects, proc_effects),
        ArenaExprOrRun::Run(_) => {
            effects.insert(Effect::Process);
        }
    }
}

fn collect_call_arg_effects(
    arena: &AstArena,
    args: ArenaRange,
    effects: &mut FxHashSet<Effect>,
    proc_effects: Option<&FxHashMap<String, Option<Vec<Effect>>>>,
) {
    for arg in arena.call_args(args).to_vec() {
        let e = match arg.kind {
            ArenaCallArgKind::Positional(e)
            | ArenaCallArgKind::Splice { value: e, .. }
            | ArenaCallArgKind::Named { value: e, .. } => e,
        };
        collect_expr_effects(arena, e, effects, proc_effects);
    }
}

fn collect_expr_effects(
    arena: &AstArena,
    expr: ExprId,
    effects: &mut FxHashSet<Effect>,
    proc_effects: Option<&FxHashMap<String, Option<Vec<Effect>>>>,
) {
    match arena.expr(expr).kind {
        ArenaExprKind::Call { callee, args } => {
            if let ArenaExprKind::Ident(name) = arena.expr(callee).kind
                && let Some(Some(callee_effects)) =
                    proc_effects.and_then(|known| known.get(name.as_str().as_str()))
            {
                for effect in callee_effects {
                    effects.insert(effect.clone());
                }
            }
            if let ArenaExprKind::Field { base, name: func } = arena.expr(callee).kind
                && let ArenaExprKind::Ident(module) = arena.expr(base).kind
                && let Some(Some(callee_effects)) =
                    proc_effects.and_then(|known| known.get(&format!("{module}.{func}")))
            {
                for effect in callee_effects {
                    effects.insert(effect.clone());
                }
            }
            if let ArenaExprKind::Field { base, name: func } = arena.expr(callee).kind
                && let ArenaExprKind::Ident(module) = arena.expr(base).kind
                && let Some(eff) =
                    Effect::from_module_call(module.as_str().as_str(), func.as_str().as_str())
            {
                effects.insert(eff);
            }
            collect_expr_effects(arena, callee, effects, proc_effects);
            collect_call_arg_effects(arena, args, effects, proc_effects);
        }
        ArenaExprKind::Try(inner) => {
            effects.insert(Effect::Error);
            collect_expr_effects(arena, inner, effects, proc_effects);
        }
        ArenaExprKind::Run(_) => {
            effects.insert(Effect::Process);
        }
        ArenaExprKind::Spawn(form) => {
            effects.insert(Effect::Process);
            if let ArenaSpawnTarget::Command(expr) = form.target {
                collect_expr_effects(arena, expr, effects, proc_effects);
            }
        }
        ArenaExprKind::Wait(form) => {
            effects.insert(Effect::Process);
            collect_expr_effects(arena, form.target, effects, proc_effects);
        }
        ArenaExprKind::Unary { expr, .. } => {
            collect_expr_effects(arena, expr, effects, proc_effects)
        }
        ArenaExprKind::Binary { left, right, .. } => {
            collect_expr_effects(arena, left, effects, proc_effects);
            collect_expr_effects(arena, right, effects, proc_effects);
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            collect_expr_effects(arena, base, effects, proc_effects);
        }
        ArenaExprKind::Index { base, index } => {
            collect_expr_effects(arena, base, effects, proc_effects);
            collect_expr_effects(arena, index, effects, proc_effects);
        }
        ArenaExprKind::List(items) => {
            for item in arena.expr_ids(items).collect::<Vec<_>>() {
                collect_expr_effects(arena, item, effects, proc_effects);
            }
        }
        ArenaExprKind::ListComp {
            expr,
            iter,
            condition,
            ..
        } => {
            collect_expr_effects(arena, expr, effects, proc_effects);
            collect_expr_effects(arena, iter, effects, proc_effects);
            if let Some(cond) = condition {
                collect_expr_effects(arena, cond, effects, proc_effects);
            }
        }
        ArenaExprKind::MapComp {
            key,
            value,
            iter,
            condition,
            ..
        } => {
            collect_expr_effects(arena, key, effects, proc_effects);
            collect_expr_effects(arena, value, effects, proc_effects);
            collect_expr_effects(arena, iter, effects, proc_effects);
            if let Some(cond) = condition {
                collect_expr_effects(arena, cond, effects, proc_effects);
            }
        }
        ArenaExprKind::Record(fields) => {
            for field in arena.record_fields(fields).to_vec() {
                if let ArenaRecordFieldKind::Named { value, .. } = field.kind {
                    collect_expr_effects(arena, value, effects, proc_effects);
                }
            }
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            for b in arena.if_expr_branches(branches).to_vec() {
                collect_expr_effects(arena, b.condition, effects, proc_effects);
                collect_expr_effects(arena, b.value, effects, proc_effects);
            }
            collect_expr_effects(arena, else_value, effects, proc_effects);
        }
        ArenaExprKind::Match { value, arms } => {
            collect_expr_effects(arena, value, effects, proc_effects);
            for arm in arena.match_expr_arms(arms).to_vec() {
                if let Some(g) = arm.guard {
                    collect_expr_effects(arena, g, effects, proc_effects);
                }
                collect_expr_effects(arena, arm.value, effects, proc_effects);
            }
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            for part in arena.fmt_parts(parts).collect::<Vec<_>>() {
                if let ArenaFmtPart::Expr(e, _) = part {
                    collect_expr_effects(arena, e, effects, proc_effects);
                }
            }
        }
        ArenaExprKind::Pipeline { input, stages } => {
            collect_expr_effects(arena, input, effects, proc_effects);
            for stage in arena.pipe_stages(stages).to_vec() {
                match stage.kind {
                    ArenaPipeStageKind::Expr(e) => {
                        collect_expr_effects(arena, e, effects, proc_effects)
                    }
                    ArenaPipeStageKind::Stream(s) => {
                        if let Some(block) = s.block {
                            collect_block_effects(arena, block, effects, proc_effects);
                        }
                        collect_call_arg_effects(arena, s.args, effects, proc_effects);
                    }
                }
            }
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            collect_expr_effects(arena, input, effects, proc_effects);
            for stage in arena.stream_stages(stages).to_vec() {
                if let Some(block) = stage.block {
                    collect_block_effects(arena, block, effects, proc_effects);
                }
                collect_call_arg_effects(arena, stage.args, effects, proc_effects);
            }
        }
        ArenaExprKind::BuilderCall { call, block } => {
            collect_expr_effects(arena, call, effects, proc_effects);
            for entry in arena
                .builder_entries(arena.builder_block(block).entries)
                .to_vec()
            {
                match entry.kind {
                    ArenaBuilderEntryKind::Task { block, .. } => {
                        collect_block_effects(arena, block, effects, proc_effects);
                    }
                    ArenaBuilderEntryKind::Stmt(stmt) => {
                        collect_stmt_effects(arena, stmt, effects, proc_effects)
                    }
                    ArenaBuilderEntryKind::Field { value, .. } => {
                        collect_expr_effects(arena, value, effects, proc_effects);
                    }
                    ArenaBuilderEntryKind::Entry { .. } => {}
                }
            }
        }
        ArenaExprKind::Require { value, .. } => {
            collect_expr_effects(arena, value, effects, proc_effects)
        }
        ArenaExprKind::Loop { block } => collect_block_effects(arena, block, effects, proc_effects),
        ArenaExprKind::Retry { delays, block } => {
            for delay in arena.expr_ids(delays).collect::<Vec<_>>() {
                collect_expr_effects(arena, delay, effects, proc_effects);
            }
            if !delays.is_empty() {
                effects.insert(Effect::Time);
            }
            collect_retry_block_effects(arena, block, effects, proc_effects);
        }
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvGet { .. }
        | ArenaExprKind::EnvPathList => {}
        ArenaExprKind::Slice { base, start, end } => {
            collect_expr_effects(arena, base, effects, proc_effects);
            if let Some(s) = start {
                collect_expr_effects(arena, s, effects, proc_effects);
            }
            if let Some(e) = end {
                collect_expr_effects(arena, e, effects, proc_effects);
            }
        }
    }
}

fn collect_retry_block_effects(
    arena: &AstArena,
    block: BlockId,
    effects: &mut FxHashSet<Effect>,
    proc_effects: Option<&FxHashMap<String, Option<Vec<Effect>>>>,
) {
    for stmt in arena
        .stmt_ids(arena.block(block).statements)
        .collect::<Vec<_>>()
    {
        collect_retry_stmt_effects(arena, stmt, effects, proc_effects);
    }
}

fn collect_retry_stmt_effects(
    arena: &AstArena,
    stmt: StmtId,
    effects: &mut FxHashSet<Effect>,
    proc_effects: Option<&FxHashMap<String, Option<Vec<Effect>>>>,
) {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Let { initializer, .. }
        | ArenaStmtKind::Var { initializer, .. }
        | ArenaStmtKind::Assign {
            value: initializer, ..
        }
        | ArenaStmtKind::Defer(initializer)
        | ArenaStmtKind::Return(Some(initializer))
        | ArenaStmtKind::Yield(initializer) => match initializer {
            ArenaExprOrRun::Expr(expr) => {
                collect_retry_expr_effects(arena, expr, effects, proc_effects)
            }
            ArenaExprOrRun::Run(_) => {
                effects.insert(Effect::Process);
            }
        },
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            for branch in arena.if_branches(branches).to_vec() {
                collect_retry_expr_effects(arena, branch.condition, effects, proc_effects);
                collect_retry_block_effects(arena, branch.block, effects, proc_effects);
            }
            if let Some(block) = else_block {
                collect_retry_block_effects(arena, block, effects, proc_effects);
            }
        }
        ArenaStmtKind::While { condition, block } => {
            collect_retry_expr_effects(arena, condition, effects, proc_effects);
            collect_retry_block_effects(arena, block, effects, proc_effects);
        }
        ArenaStmtKind::For { iter, block, .. } => {
            collect_retry_expr_effects(arena, iter, effects, proc_effects);
            collect_retry_block_effects(arena, block, effects, proc_effects);
        }
        ArenaStmtKind::Loop { block } => {
            collect_retry_block_effects(arena, block, effects, proc_effects)
        }
        ArenaStmtKind::Guard {
            initializer,
            else_block,
            ..
        } => {
            match initializer {
                ArenaExprOrRun::Expr(expr) => {
                    collect_retry_expr_effects(arena, expr, effects, proc_effects)
                }
                ArenaExprOrRun::Run(_) => {
                    effects.insert(Effect::Process);
                }
            }
            collect_retry_block_effects(arena, else_block, effects, proc_effects);
        }
        ArenaStmtKind::GuardedStmt {
            stmt, condition, ..
        } => {
            collect_retry_stmt_effects(arena, stmt, effects, proc_effects);
            collect_retry_expr_effects(arena, condition, effects, proc_effects);
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
            ..
        } => {
            for binding in arena.with_bindings(bindings).to_vec() {
                collect_retry_expr_effects(arena, binding.initializer, effects, proc_effects);
            }
            collect_retry_block_effects(arena, body, effects, proc_effects);
            collect_retry_block_effects(arena, else_block, effects, proc_effects);
        }
        ArenaStmtKind::Break { value: Some(expr) } | ArenaStmtKind::Expr(expr) => {
            collect_retry_expr_effects(arena, expr, effects, proc_effects);
        }
        ArenaStmtKind::Match { value, arms } => {
            collect_retry_expr_effects(arena, value, effects, proc_effects);
            for arm in arena.match_arms(arms).to_vec() {
                if let Some(guard) = arm.guard {
                    collect_retry_expr_effects(arena, guard, effects, proc_effects);
                }
                collect_retry_block_effects(arena, arm.block, effects, proc_effects);
            }
        }
        ArenaStmtKind::Command(_) => {
            effects.insert(Effect::Process);
        }
        ArenaStmtKind::Return(None)
        | ArenaStmtKind::Break { value: None }
        | ArenaStmtKind::Continue
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Use(_)
        | ArenaStmtKind::Export(_)
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_)
        | ArenaStmtKind::SignalHook(_) => {}
    }
}

fn collect_retry_expr_effects(
    arena: &AstArena,
    expr: ExprId,
    effects: &mut FxHashSet<Effect>,
    proc_effects: Option<&FxHashMap<String, Option<Vec<Effect>>>>,
) {
    if let ArenaExprKind::Try(inner) = arena.expr(expr).kind {
        collect_retry_expr_effects(arena, inner, effects, proc_effects);
        return;
    }
    collect_expr_effects(arena, expr, effects, proc_effects);
}

fn format_binding_target(arena: &AstArena, target: BindingTargetId) -> String {
    match arena.binding_target(target).kind.clone() {
        ArenaBindingTargetKind::Name(name) => name.to_string(),
        ArenaBindingTargetKind::Record { fields, rest } => {
            let fields = arena.destructure_fields(fields).to_vec();
            let mut s = String::from("{");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(f.name.as_str().as_str());
            }
            if rest {
                if !fields.is_empty() {
                    s.push_str(", ");
                }
                s.push_str("..");
            }
            s.push('}');
            s
        }
    }
}

fn is_map_empty_call(arena: &AstArena, expr: ExprId) -> bool {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    name == "empty"
        && matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "map")
}

fn map_comp_key_can_be_bare(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_) => true,
        ArenaExprKind::Field { base, .. } => map_comp_key_can_be_bare(arena, base),
        _ => false,
    }
}

fn lint_block_always_returns(arena: &AstArena, block: BlockId) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .last()
        .is_some_and(|stmt| lint_stmt_always_returns(arena, stmt))
}

fn lint_stmt_always_returns(arena: &AstArena, stmt: StmtId) -> bool {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Return(_) => true,
        ArenaStmtKind::If {
            branches,
            else_block: Some(else_block),
        } => {
            arena
                .if_branches(branches)
                .iter()
                .all(|b| lint_block_always_returns(arena, b.block))
                && lint_block_always_returns(arena, else_block)
        }
        ArenaStmtKind::Match { arms, .. } => arena
            .match_arms(arms)
            .iter()
            .all(|arm| lint_block_always_returns(arena, arm.block)),
        _ => false,
    }
}
