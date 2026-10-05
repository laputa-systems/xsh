//! Position-free canonical form of a parsed program.
//!
//! The formatter regenerates source from the AST, so its output must reparse
//! to the same tree. Arena tables cannot be compared directly: lowering such as
//! value-pipeline method sugar leaves unreachable rows and allocates in a
//! different order than the canonical spelling. This walks the tree from the
//! root statements instead, writing every node kind and payload but no spans.

use super::{ArenaTypeExprKind, canonical_effects, type_expr_kind};
use std::fmt::Write as _;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArgKind,
    ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaCompQualifier,
    ArenaEnvAssignmentValue, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart,
    ArenaModuleContractEntryKind, ArenaPatternKind, ArenaPipeStageKind, ArenaProgram, ArenaRange,
    ArenaRecordFieldKind, ArenaRedirectionTarget, ArenaSpawnTarget, ArenaStmtKind, ArenaSugarOperand,
    ArenaStreamStage, ArenaText, ArenaTypeDefBody, ArenaWordPart, AssignTargetId, AstArena,
    BindingTargetId, BlockId, BuilderBlockId, ExprId, FunctionDefId, PatternId, RunFormId, StmtId,
    TypeExprId,
};

/// Canonical text plus the original-source offset at which each statement or
/// expression begins, so a mismatch can be reported at a source location.
pub(super) struct Canonical {
    pub text: String,
    pub marks: Vec<(usize, usize)>,
}

impl Canonical {
    /// Source offset of the innermost node whose canonical text contains `index`.
    pub fn source_offset_at(&self, index: usize) -> Option<usize> {
        let position = self
            .marks
            .partition_point(|&(text_index, _)| text_index <= index);
        position
            .checked_sub(1)
            .map(|position| self.marks[position].1)
    }
}

pub(super) fn canonical(program: &ArenaProgram, source: &str) -> Canonical {
    let mut writer = CanonicalWriter {
        arena: &program.arena,
        source,
        out: String::new(),
        marks: Vec::new(),
        expanded: None,
    };
    for stmt in program.statement_ids() {
        writer.stmt(stmt);
    }
    Canonical {
        text: writer.out,
        marks: writer.marks,
    }
}

/// Layout-independent key for one subtree: two blocks or expressions get the
/// same key exactly when `xsht fmt` could print one as the other.
pub(crate) fn canonical_subtree(
    arena: &AstArena,
    source: &str,
    root: Result<BlockId, ExprId>,
) -> String {
    let mut writer = CanonicalWriter {
        arena,
        source,
        out: String::new(),
        marks: Vec::new(),
        expanded: None,
    };
    match root {
        Ok(block) => writer.block(block),
        Err(expr) => writer.expr(expr),
    }
    writer.out
}

/// The walk a sugar statement's meaning is read from: its expansion instead
/// of its operands, with every node entered recorded in walk order.
#[cfg(test)]
pub(super) struct ExpandedWalk {
    pub text: String,
    pub visited: Vec<ArenaSugarOperand>,
}

#[cfg(test)]
pub(super) fn expanded_walk(arena: &AstArena, source: &str, statements: &[StmtId]) -> ExpandedWalk {
    let mut writer = CanonicalWriter {
        arena,
        source,
        out: String::new(),
        marks: Vec::new(),
        expanded: Some(Vec::new()),
    };
    for stmt in statements {
        writer.stmt(*stmt);
    }
    ExpandedWalk {
        text: writer.out,
        visited: writer.expanded.unwrap_or_default(),
    }
}

struct CanonicalWriter<'a> {
    arena: &'a AstArena,
    source: &'a str,
    out: String,
    marks: Vec<(usize, usize)>,
    /// Set only by the expansion tests. The formatter's walk leaves it unset
    /// and compares what the user wrote: a sugar statement's form and operands.
    expanded: Option<Vec<ArenaSugarOperand>>,
}

impl CanonicalWriter<'_> {
    fn put(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// Interned symbol ids are reused once their owner drops, so names are
    /// written by spelling to compare trees from separate parses.
    fn debug(&mut self, value: &impl std::fmt::Debug) {
        let text = format!("{value:?};");
        let mut rest = text.as_str();
        while let Some(at) = rest.find("Symbol(") {
            let digits = &rest[at + 7..];
            let end = digits.find(')').unwrap_or(0);
            let Ok(raw) = digits[..end].parse::<u32>() else {
                break;
            };
            self.out.push_str(&rest[..at]);
            let name = xsh::frontend::symbols::Name::from_symbol(
                xsh::frontend::symbols::Symbol::from_raw(raw),
            );
            let _ = write!(self.out, "{:?}", name.as_str().as_str());
            rest = &digits[end + 1..];
        }
        self.out.push_str(rest);
    }

    fn mark(&mut self, offset: usize) {
        self.marks.push((self.out.len(), offset));
    }

    fn text(&mut self, text: &ArenaText) {
        let value = self.arena.text_value(text, self.source);
        self.debug(&value);
    }

    fn opt_expr(&mut self, expr: Option<ExprId>) {
        match expr {
            Some(expr) => self.expr(expr),
            None => self.put("_;"),
        }
    }

    fn opt_type(&mut self, ty: Option<TypeExprId>) {
        match ty {
            Some(ty) => self.ty(ty),
            None => self.put("_;"),
        }
    }

    fn opt_block(&mut self, block: Option<BlockId>) {
        match block {
            Some(block) => self.block(block),
            None => self.put("_;"),
        }
    }

    fn expr_or_run(&mut self, value: &ArenaExprOrRun) {
        match value {
            ArenaExprOrRun::Expr(expr) => self.expr(*expr),
            ArenaExprOrRun::Run(run) => self.run(*run),
        }
    }

    fn stmt(&mut self, id: StmtId) {
        let stmt = self.arena.stmt(id);
        if let Some(visited) = &mut self.expanded {
            if let ArenaStmtKind::Sugar { expansion, .. } = stmt.kind {
                return self.stmt(expansion);
            }
            visited.push(ArenaSugarOperand::Stmt(id));
        }
        self.mark(stmt.span.start());
        self.put("S(");
        match &stmt.kind {
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = self.arena.use_stmt(*use_id);
                self.put("use;");
                for name in self.arena.names(use_stmt.path) {
                    self.debug(&name);
                }
                self.debug(&use_stmt.alias);
            }
            ArenaStmtKind::Export(inner) => {
                self.put("export;");
                self.stmt(*inner);
            }
            ArenaStmtKind::TypeDef(id) => {
                let def = self.arena.type_def(*id);
                self.put("type;");
                self.debug(&def.name);
                for name in self.arena.names(def.type_parameters) {
                    self.debug(&name);
                }
                match def.body {
                    ArenaTypeDefBody::Alias(ty) => {
                        self.put("alias;");
                        self.ty(ty);
                    }
                    ArenaTypeDefBody::RecordSchema(fields) => {
                        self.put("record;");
                        self.schema_fields(fields);
                    }
                    ArenaTypeDefBody::ModuleContract { entries, exact } => {
                        self.put(if exact { "exact module;" } else { "module;" });
                        for entry in self.arena.module_contract_entries(entries) {
                            self.debug(&(entry.name, entry.optional));
                            match &entry.kind {
                                ArenaModuleContractEntryKind::Value(ty) => {
                                    self.put("value;");
                                    self.ty(*ty);
                                }
                                ArenaModuleContractEntryKind::Proc {
                                    params,
                                    effects,
                                    return_ty,
                                } => {
                                    self.put("proc;");
                                    self.params(*params);
                                    self.effects(*effects);
                                    self.ty(*return_ty);
                                }
                                ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                                    self.put("pure;");
                                    self.params(*params);
                                    self.ty(*return_ty);
                                }
                            }
                        }
                    }
                    ArenaTypeDefBody::TagUnion(variants) => {
                        self.put("tags;");
                        for variant in self.arena.tag_variants(variants) {
                            self.debug(&variant.name);
                            for raw in self.arena.extra_range(variant.fields) {
                                self.ty(TypeExprId::from_index(*raw as usize));
                            }
                            self.put("|");
                            self.opt_expr(variant.wire_value);
                        }
                    }
                }
            }
            ArenaStmtKind::ErrorDef(id) => {
                let def = self.arena.error_def(*id);
                self.put("error;");
                self.debug(&def.name);
                for variant in self.arena.error_variants(def.variants) {
                    self.debug(&variant.name);
                    for field in self.arena.error_fields(variant.fields) {
                        self.debug(&field.name);
                        self.ty(field.ty);
                    }
                    for name in self.arena.names(variant.facets) {
                        self.debug(&name);
                    }
                }
            }
            ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            } => self.binding_stmt("const", *target, *ty, initializer),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            } => self.binding_stmt("let", *target, *ty, initializer),
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => self.binding_stmt("var", *target, *ty, initializer),
            ArenaStmtKind::Assign { target, op, value } => {
                self.put("assign;");
                self.assign_target(*target);
                self.debug(op);
                self.expr_or_run(value);
            }
            ArenaStmtKind::ProcDef(id) => self.function("proc", *id),
            ArenaStmtKind::CliMain(id) => self.function("cli", *id),
            ArenaStmtKind::PureDef(id) => self.function("pure", *id),
            ArenaStmtKind::StreamDef(id) => self.function("stream", *id),
            ArenaStmtKind::SignalHook(id) => {
                let hook = self.arena.signal_hook(*id);
                self.put("signal;");
                self.debug(&(hook.signal, &hook.options));
                self.effect_set(hook.effects);
                self.block(hook.body);
            }
            ArenaStmtKind::Return(value) => {
                self.put("return;");
                match value {
                    Some(value) => self.expr_or_run(value),
                    None => self.put("_;"),
                }
            }
            ArenaStmtKind::Yield(value) => {
                self.put("yield;");
                self.expr_or_run(value);
            }
            ArenaStmtKind::YieldDelegate(expr) => {
                self.put("yield*;");
                self.expr(*expr);
            }
            ArenaStmtKind::Defer(value) => {
                self.put("defer;");
                self.expr_or_run(value);
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                self.put("if;");
                for branch in self.arena.if_branches(*branches) {
                    self.expr(branch.condition);
                    self.block(branch.block);
                }
                self.opt_block(*else_block);
            }
            ArenaStmtKind::While { condition, block } => {
                self.put("while;");
                self.expr(*condition);
                self.block(*block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                self.put("for;");
                self.binding_target(*target);
                self.expr(*iter);
                self.block(*block);
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
            } => {
                self.put("with;");
                for binding in self.arena.with_bindings(*bindings) {
                    self.debug(&binding.name);
                    self.expr(binding.initializer);
                }
                self.block(*body);
                self.block(*else_block);
            }
            ArenaStmtKind::Loop { block } => {
                self.put("loop;");
                self.block(*block);
            }
            ArenaStmtKind::Sugar { form, operands, .. } => {
                self.put("sugar;");
                self.debug(form);
                for operand in self.arena.sugar_operands(*operands).to_vec() {
                    match operand {
                        ArenaSugarOperand::Expr(expr) => self.expr(expr),
                        ArenaSugarOperand::Block(block) => self.block(block),
                        ArenaSugarOperand::Stmt(stmt) => self.stmt(stmt),
                        ArenaSugarOperand::BindingTarget(target) => self.binding_target(target),
                        ArenaSugarOperand::TypeExpr(ty) => self.ty(ty),
                        ArenaSugarOperand::Name(name) => self.debug(&name),
                    }
                }
            }
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_block,
            } => {
                self.binding_stmt("guard", *target, *ty, initializer);
                self.block(*else_block);
            }
            ArenaStmtKind::Assert { condition, message } => {
                self.put("assert;");
                self.expr(*condition);
                self.opt_expr(*message);
            }
            ArenaStmtKind::Break { value } => {
                self.put("break;");
                self.opt_expr(*value);
            }
            ArenaStmtKind::Continue => self.put("continue;"),
            ArenaStmtKind::Match { value, arms } => {
                self.put("match;");
                self.expr(*value);
                for arm in self.arena.match_arms(*arms) {
                    self.pattern(arm.pattern);
                    self.opt_expr(arm.guard);
                    self.block(arm.block);
                }
            }
            ArenaStmtKind::Command(id) => {
                let command = self.arena.command_stmt(*id);
                self.put("command;");
                self.debug(&command.propagate);
                match &command.command {
                    ArenaCommand::Proc { name, args } => {
                        self.put("proc;");
                        self.debug(name);
                        self.command_args(*args);
                    }
                    ArenaCommand::Core {
                        name,
                        args,
                        env,
                        block,
                    } => {
                        self.put("core;");
                        self.debug(name);
                        self.command_args(*args);
                        self.env_assignments(*env);
                        self.opt_block(*block);
                    }
                    ArenaCommand::Run(run) => self.run(*run),
                }
            }
            ArenaStmtKind::TailBareIdent(name) => {
                self.put("tail-ident;");
                self.debug(name);
            }
            ArenaStmtKind::Expr(expr) => self.expr(*expr),
        }
        self.put(")");
    }

    fn binding_stmt(
        &mut self,
        keyword: &str,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: &ArenaExprOrRun,
    ) {
        self.put(keyword);
        self.put(";");
        self.binding_target(target);
        self.opt_type(ty);
        self.expr_or_run(initializer);
    }

    fn function(&mut self, keyword: &str, id: FunctionDefId) {
        let def = self.arena.function_def(id);
        self.put(keyword);
        self.put(";");
        self.debug(&(def.test_declaration, def.name, def.return_ty_defaulted));
        self.params(def.params);
        self.effects(def.effects);
        self.ty(def.return_ty);
        self.block(def.body);
    }

    fn params(&mut self, range: ArenaRange) {
        for param in self.arena.params(range) {
            self.debug(&(param.name, param.ty_defaulted, param.rest));
            self.ty(param.ty);
            self.opt_expr(param.default);
        }
        self.put("|");
    }

    fn effects(&mut self, effects: Option<ArenaRange>) {
        match effects {
            Some(range) => self.effect_set(range),
            None => self.put("_;"),
        }
    }

    /// Effect lists are sets; the formatter writes them in canonical order.
    fn effect_set(&mut self, range: ArenaRange) {
        let effects = self.arena.effects(range).collect::<Vec<_>>();
        for effect in canonical_effects(&effects) {
            self.debug(effect);
        }
        self.put("|");
    }

    fn schema_fields(&mut self, range: ArenaRange) {
        for field in self.arena.schema_fields(range) {
            self.debug(&field.name);
            self.ty(field.ty);
            self.opt_expr(field.default);
        }
        self.put("|");
    }

    fn block(&mut self, id: BlockId) {
        if let Some(visited) = &mut self.expanded {
            visited.push(ArenaSugarOperand::Block(id));
        }
        let block = self.arena.block(id);
        if let Some(bound) = self.arena.block_effect_bound(id) {
            self.put("without(");
            for effect in self.arena.effects(bound.effects).collect::<Vec<_>>() {
                self.put(effect.as_str());
                self.put(",");
            }
            self.put(")");
        }
        self.put("B(");
        for param in self.arena.block_params(block.params) {
            self.debug(&param.name);
        }
        self.put("|");
        for stmt in self.arena.stmt_ids(block.statements).collect::<Vec<_>>() {
            self.stmt(stmt);
        }
        self.put(")");
    }

    fn binding_target(&mut self, id: BindingTargetId) {
        if let Some(visited) = &mut self.expanded {
            visited.push(ArenaSugarOperand::BindingTarget(id));
        }
        match &self.arena.binding_target(id).kind {
            ArenaBindingTargetKind::Name(name) => self.debug(name),
            ArenaBindingTargetKind::Record { fields, rest } => {
                self.put("{");
                self.debug(rest);
                for field in self.arena.destructure_fields(*fields) {
                    self.debug(&field.name);
                    self.binding_target(field.target);
                }
                self.put("}");
            }
        }
    }

    fn assign_target(&mut self, id: AssignTargetId) {
        match &self.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(name) => self.debug(name),
            ArenaAssignTargetKind::Env(name) => {
                self.put("env:");
                self.debug(name);
            }
            ArenaAssignTargetKind::Field { base, name } => {
                self.put("field;");
                self.assign_target(*base);
                self.debug(name);
            }
            ArenaAssignTargetKind::Index { base, index } => {
                self.put("index;");
                self.assign_target(*base);
                self.expr(*index);
            }
        }
    }

    fn pattern(&mut self, id: PatternId) {
        self.put("P(");
        match &self.arena.pattern(id).kind {
            ArenaPatternKind::Group(inner) => {
                self.put("group;");
                self.pattern(*inner);
            }
            ArenaPatternKind::Alias { pattern, name, .. } => {
                self.put("alias;");
                self.pattern(*pattern);
                self.debug(name);
            }
            ArenaPatternKind::Wildcard => self.put("_;"),
            ArenaPatternKind::TestName { name, ty } => {
                self.put("test;");
                self.debug(name);
                self.ty(*ty);
            }
            ArenaPatternKind::Binding(name) => {
                self.put("bind;");
                self.debug(name);
            }
            ArenaPatternKind::Type { binding, ty } => {
                self.put("type;");
                self.debug(binding);
                self.ty(*ty);
            }
            ArenaPatternKind::Literal(expr) => {
                self.put("lit;");
                self.expr(*expr);
            }
            ArenaPatternKind::Record { fields, rest } => {
                self.put("record;");
                self.debug(rest);
                for field in self.arena.pattern_fields(*fields) {
                    self.debug(&field.name);
                    self.pattern(field.pattern);
                }
            }
            ArenaPatternKind::List { elements, rest } => {
                self.put("list;");
                for element in self.arena.pattern_ids(*elements).collect::<Vec<_>>() {
                    self.pattern(element);
                }
                self.put("|");
                if let Some(rest) = rest {
                    self.pattern(*rest);
                }
            }
            ArenaPatternKind::Alternation(range) => {
                self.put("alt;");
                for element in self.arena.pattern_ids(*range).collect::<Vec<_>>() {
                    self.pattern(element);
                }
            }
            ArenaPatternKind::Constructor { name, arg } => {
                self.put("ctor;");
                self.debug(name);
                if let Some(arg) = arg {
                    self.pattern(*arg);
                }
            }
            ArenaPatternKind::ErrorVariant {
                family,
                variant,
                fields,
            } => {
                self.put("error;");
                self.debug(&(family, variant));
                for field in self.arena.pattern_fields(*fields) {
                    self.debug(&field.name);
                    self.pattern(field.pattern);
                }
            }
            ArenaPatternKind::Facet(name) => {
                self.put("facet;");
                self.debug(name);
            }
            ArenaPatternKind::Tuple(range) => {
                self.put("tuple;");
                for element in self.arena.pattern_ids(*range).collect::<Vec<_>>() {
                    self.pattern(element);
                }
            }
        }
        self.put(")");
    }

    fn ty(&mut self, id: TypeExprId) {
        if let Some(visited) = &mut self.expanded {
            visited.push(ArenaSugarOperand::TypeExpr(id));
        }
        self.put("T(");
        match type_expr_kind(self.arena, id) {
            ArenaTypeExprKind::Applied { base, arguments } => {
                self.put("applied;");
                self.ty(base);
                for argument in arguments {
                    self.ty(argument);
                }
            }
            ArenaTypeExprKind::Named(name) => self.debug(&name),
            ArenaTypeExprKind::Qualified { namespace, name } => self.debug(&(namespace, name)),
            ArenaTypeExprKind::List(inner) => {
                self.put("list;");
                self.ty(inner);
            }
            ArenaTypeExprKind::Map(key, value) => {
                self.put("map;");
                self.opt_type(key);
                self.ty(value);
            }
            ArenaTypeExprKind::Stream(inner) => {
                self.put("stream;");
                self.ty(inner);
            }
            ArenaTypeExprKind::Module(inner) => {
                self.put("module;");
                self.ty(inner);
            }
            ArenaTypeExprKind::Result { ok, err } => {
                self.put("result;");
                self.ty(ok);
                self.opt_type(err);
            }
            ArenaTypeExprKind::Optional(inner) => {
                self.put("optional;");
                self.ty(inner);
            }
        }
        self.put(")");
    }

    fn expr(&mut self, id: ExprId) {
        if let Some(visited) = &mut self.expanded {
            visited.push(ArenaSugarOperand::Expr(id));
        }
        let expr = self.arena.expr(id);
        self.mark(expr.span.start());
        self.put("E(");
        match &expr.kind {
            ArenaExprKind::Null => self.put("null;"),
            ArenaExprKind::Bool(value) => self.debug(value),
            ArenaExprKind::Int(value) => self.debug(self.arena.int_literal(*value)),
            ArenaExprKind::Float(value) => self.debug(self.arena.float_literal(*value)),
            ArenaExprKind::Duration(value) => self.debug(self.arena.duration_literal(*value)),
            ArenaExprKind::Str(value) => {
                self.put("str;");
                self.debug(self.arena.string_literal(*value));
            }
            ArenaExprKind::PathStr(value) => {
                self.put("path;");
                self.debug(self.arena.string_literal(*value));
            }
            ArenaExprKind::GlobStr(value) => {
                self.put("glob;");
                self.debug(self.arena.string_literal(*value));
            }
            ArenaExprKind::FmtString(parts) => {
                self.put("fmt;");
                self.fmt_parts(*parts);
            }
            ArenaExprKind::PathFmtString(parts) => {
                self.put("path-fmt;");
                self.fmt_parts(*parts);
            }
            ArenaExprKind::Bytes(value) => self.debug(self.arena.bytes_literal(*value)),
            ArenaExprKind::Regex(value) => {
                self.put("regex;");
                self.debug(&self.arena.regex_literal(*value).pattern);
            }
            ArenaExprKind::Ident(name) => self.debug(name),
            ArenaExprKind::Item => self.put("item;"),
            ArenaExprKind::LastStatus => self.put("status;"),
            ArenaExprKind::List(items) => {
                self.put("list;");
                for element in self.arena.list_elements(*items).collect::<Vec<_>>() {
                    self.debug(&element.splice_span.is_some());
                    self.expr(element.value);
                }
            }
            ArenaExprKind::ListComp { expr, qualifiers } => {
                self.put("list-comp;");
                self.expr(*expr);
                self.comp_qualifiers(*qualifiers);
            }
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => {
                self.put("map-comp;");
                self.expr(*key);
                self.expr(*value);
                self.comp_qualifiers(*qualifiers);
            }
            ArenaExprKind::Record(fields) => {
                self.put("record;");
                for field in self.arena.record_fields(*fields) {
                    match &field.kind {
                        ArenaRecordFieldKind::Computed { key, value, .. } => {
                            self.put("computed;");
                            self.expr(*key);
                            self.expr(*value);
                        }
                        ArenaRecordFieldKind::Path { path, value, .. } => {
                            self.put("path;");
                            for name in self.arena.names(*path) {
                                self.debug(&name);
                            }
                            self.expr(*value);
                        }
                        ArenaRecordFieldKind::Named { name, value, .. } => {
                            self.put("named;");
                            self.debug(name);
                            self.expr(*value);
                        }
                        ArenaRecordFieldKind::Shorthand { name, .. } => {
                            self.put("shorthand;");
                            self.debug(name);
                        }
                        ArenaRecordFieldKind::Spread { expr, .. } => {
                            self.put("spread;");
                            self.expr(*expr);
                        }
                    }
                }
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                self.put("if;");
                for branch in self.arena.if_expr_branches(*branches) {
                    self.expr(branch.condition);
                    self.expr(branch.value);
                }
                self.expr(*else_value);
            }
            ArenaExprKind::Match { value, arms } => self.match_expr("match", *value, *arms),
            ArenaExprKind::PatternTest { value, arms } => self.match_expr("is", *value, *arms),
            ArenaExprKind::PatternCondition { value, arms } => {
                self.match_expr("let", *value, *arms)
            }
            ArenaExprKind::Unary { op, expr } => {
                self.debug(op);
                self.expr(*expr);
            }
            ArenaExprKind::ComparisonChain(pairs) => {
                self.put("chain;");
                for pair in self.arena.expr_ids(*pairs).collect::<Vec<_>>() {
                    self.expr(pair);
                }
            }
            ArenaExprKind::Binary { op, left, right } => {
                self.debug(op);
                self.expr(*left);
                self.expr(*right);
            }
            ArenaExprKind::Call { callee, args } => {
                self.put("call;");
                self.expr(*callee);
                self.call_args(*args);
            }
            ArenaExprKind::ValuePipelineCall { input, call, .. } => {
                self.put("value-pipe;");
                self.expr(*input);
                self.expr(*call);
            }
            ArenaExprKind::Field { base, name } => {
                self.put("field;");
                self.expr(*base);
                self.debug(name);
            }
            ArenaExprKind::NullSafeField { base, name } => {
                self.put("field?;");
                self.expr(*base);
                self.debug(name);
            }
            ArenaExprKind::Index {
                base,
                index,
                guarded,
            } => {
                self.put("index;");
                self.debug(guarded);
                self.expr(*base);
                self.expr(*index);
            }
            ArenaExprKind::Slice {
                base,
                start,
                end,
                guarded,
            } => {
                self.put("slice;");
                self.debug(guarded);
                self.expr(*base);
                self.opt_expr(*start);
                self.opt_expr(*end);
            }
            ArenaExprKind::EnvString(name) => {
                self.put("env-string:");
                self.debug(name);
            }
            ArenaExprKind::EnvPathList => self.put("env-path;"),
            ArenaExprKind::Pipeline { input, stages } => {
                self.put("pipeline;");
                self.expr(*input);
                for stage in self.arena.pipe_stages(*stages) {
                    match &stage.kind {
                        ArenaPipeStageKind::Expr(expr) => self.expr(*expr),
                        ArenaPipeStageKind::Stream(stage) => self.stream_stage(stage),
                    }
                }
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.put("structured;");
                self.expr(*input);
                for stage in self.arena.stream_stages(*stages) {
                    self.stream_stage(stage);
                }
            }
            ArenaExprKind::Run(run) => self.run(*run),
            ArenaExprKind::Spawn(form) => {
                self.put("spawn;");
                match &form.target {
                    ArenaSpawnTarget::Run(run) => self.run(*run),
                    ArenaSpawnTarget::Command(expr) => self.expr(*expr),
                }
            }
            ArenaExprKind::Wait(form) => {
                self.put("wait;");
                self.expr(form.target);
            }
            ArenaExprKind::BuilderCall { call, block } => {
                self.put("builder;");
                self.expr(*call);
                self.builder_block(*block);
            }
            ArenaExprKind::Try(inner) => {
                self.put("try;");
                self.expr(*inner);
            }
            ArenaExprKind::Capture(block) => {
                self.put("capture;");
                self.block(*block);
            }
            ArenaExprKind::Require { value, schema } => {
                self.put("require;");
                self.expr(*value);
                self.opt_type(*schema);
            }
            ArenaExprKind::Loop { block } => {
                self.put("loop;");
                self.block(*block);
            }
            ArenaExprKind::Retry {
                delays,
                pattern,
                block,
            } => {
                self.put("retry;");
                for delay in self.arena.expr_ids(*delays).collect::<Vec<_>>() {
                    self.expr(delay);
                }
                if let Some(pattern) = pattern {
                    self.pattern(*pattern);
                }
                self.block(*block);
            }
            ArenaExprKind::ValueBlock(block) => {
                self.put("value-block;");
                self.block(*block);
            }
            ArenaExprKind::ErrorContext { message, block } => {
                self.put("ctx;");
                self.expr(*message);
                self.block(*block);
            }
            ArenaExprKind::ContextScope {
                kind,
                input,
                block,
                value_body,
            } => {
                self.put("scope;");
                self.debug(&(kind, value_body));
                self.expr(*input);
                self.block(*block);
            }
        }
        self.put(")");
    }

    fn match_expr(&mut self, keyword: &str, value: ExprId, arms: ArenaRange) {
        self.put(keyword);
        self.put(";");
        self.expr(value);
        for arm in self.arena.match_expr_arms(arms) {
            self.pattern(arm.pattern);
            self.opt_expr(arm.guard);
            self.expr(arm.value);
        }
    }

    fn comp_qualifiers(&mut self, range: ArenaRange) {
        for qualifier in self.arena.comp_qualifiers(range) {
            match *qualifier {
                ArenaCompQualifier::For { target, iter, .. } => {
                    self.put("for;");
                    self.binding_target(target);
                    self.expr(iter);
                }
                ArenaCompQualifier::If { condition, .. } => {
                    self.put("if;");
                    self.expr(condition);
                }
            }
        }
    }

    fn fmt_parts(&mut self, range: ArenaRange) {
        for part in self.arena.fmt_parts(range).collect::<Vec<_>>() {
            match part {
                ArenaFmtPart::Text(text) => self.text(&text),
                ArenaFmtPart::Expr(expr, spec) => {
                    self.debug(&spec);
                    self.expr(expr);
                }
            }
        }
        self.put("|");
    }

    fn stream_stage(&mut self, stage: &ArenaStreamStage) {
        self.put("stage;");
        self.debug(&stage.kind);
        self.opt_block(stage.block);
        self.call_args(stage.args);
    }

    fn call_args(&mut self, range: ArenaRange) {
        for arg in self.arena.call_args(range) {
            match &arg.kind {
                ArenaCallArgKind::Positional(value) => self.expr(*value),
                ArenaCallArgKind::NamedSpread { value, .. } => {
                    self.put("named-spread;");
                    self.expr(*value);
                }
                ArenaCallArgKind::Splice { value, .. } => {
                    self.put("splice;");
                    self.expr(*value);
                }
                ArenaCallArgKind::Named { name, value, .. } => {
                    self.put("named;");
                    self.debug(name);
                    self.expr(*value);
                }
            }
        }
        self.put("|");
    }

    fn builder_block(&mut self, id: BuilderBlockId) {
        let block = self.arena.builder_block(id);
        self.put("BB(");
        for entry in self.arena.builder_entries(block.entries) {
            match &entry.kind {
                ArenaBuilderEntryKind::Field { name, value } => {
                    self.put("field;");
                    self.debug(name);
                    self.expr(*value);
                }
                ArenaBuilderEntryKind::Entry { name, args, block } => {
                    self.put("entry;");
                    self.debug(name);
                    self.command_args(*args);
                    if let Some(block) = block {
                        self.builder_block(*block);
                    }
                }
                ArenaBuilderEntryKind::Task { name, block } => {
                    self.put("task;");
                    self.debug(name);
                    self.block(*block);
                }
                ArenaBuilderEntryKind::Stmt(stmt) => self.stmt(*stmt),
            }
        }
        self.put(")");
    }

    fn run(&mut self, id: RunFormId) {
        let run = self.arena.run_form(id);
        self.put("R(");
        self.debug(&run.propagate);
        for segment in self.arena.run_segments(run.segments) {
            self.debug(&(segment.kind, segment.grouped));
            self.opt_expr(segment.timeout);
            self.opt_expr(segment.cpu_max);
            self.opt_expr(segment.accept);
            self.env_assignments(segment.env);
            self.command_arg(&segment.target);
            self.command_args(segment.args);
            for redirection in self.arena.redirections(segment.redirections) {
                self.debug(&redirection.kind);
                match &redirection.target {
                    ArenaRedirectionTarget::Path(arg) => {
                        self.put("path;");
                        self.command_arg(arg);
                    }
                    ArenaRedirectionTarget::Fd(arg) => {
                        self.put("fd;");
                        self.command_arg(arg);
                    }
                }
            }
            self.put("|");
        }
        self.put(")");
    }

    fn env_assignments(&mut self, range: ArenaRange) {
        for assignment in self.arena.env_assignments(range) {
            self.debug(&assignment.name);
            match &assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => self.command_arg(arg),
                ArenaEnvAssignmentValue::Expr(expr) => self.expr(*expr),
            }
        }
        self.put("|");
    }

    fn command_args(&mut self, range: ArenaRange) {
        for arg in self.arena.command_args(range) {
            self.command_arg(arg);
        }
        self.put("|");
    }

    fn command_arg(&mut self, arg: &ArenaCommandArg) {
        self.put("A(");
        match &arg.kind {
            ArenaCommandArgKind::Word(parts) => {
                for part in self.arena.word_parts(*parts).collect::<Vec<_>>() {
                    match part {
                        ArenaWordPart::Bare(text) => {
                            self.put("bare;");
                            self.text(&text);
                        }
                        ArenaWordPart::Quoted(text) => {
                            self.put("quoted;");
                            self.text(&text);
                        }
                        ArenaWordPart::Shorthand(expr) => {
                            self.put("shorthand;");
                            self.expr(expr);
                        }
                        ArenaWordPart::Interpolation(expr) => {
                            self.put("interp;");
                            self.expr(expr);
                        }
                    }
                }
            }
            ArenaCommandArgKind::SpliceName(name) => {
                self.put("splice-name;");
                self.debug(name);
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                self.put("splice;");
                self.expr(*expr);
            }
            ArenaCommandArgKind::Typed(expr) => {
                self.put("typed;");
                self.expr(*expr);
            }
        }
        self.put(")");
    }
}

#[cfg(test)]
#[path = "sugar_expansion_tests.rs"]
mod sugar_expansion_tests;
