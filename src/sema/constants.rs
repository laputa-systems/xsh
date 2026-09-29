use crate::sema::records::standard_record_type;
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
use crate::symbol::{Name, Symbol};
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaModuleContractEntryKind,
    ArenaRecordFieldKind, ArenaStmtKind, ArenaTypeDefBody, ArenaTypeExprTag, ArenaProgram,
    AstArena, ExprId, StmtId, TypeDefId, TypeExprId,
};
use crate::syntax::node::UnaryOp;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A bounded literal value, independent of runtime bindings or evaluation.
/// Float bits preserve signed zero when deciding whether an explicit default
/// can be omitted without changing the constructed value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LiteralConstant {
    Null,
    Bool(bool),
    Int(i64),
    Float(u64),
    Duration(u64),
    Str(Arc<str>),
    Path(Arc<str>),
    Bytes(Arc<[u8]>),
    EmptyMap,
    Map(Arc<BTreeMap<Arc<str>, LiteralConstant>>),
    List(Arc<Vec<LiteralConstant>>),
    Record(Arc<BTreeMap<Name, LiteralConstant>>),
}

impl LiteralConstant {
    pub fn in_type(self, expected: &Type) -> Self {
        match (self, expected) {
            (Self::Record(values), Type::Map(item)) => Self::Map(Arc::new(values.iter()
                .map(|(name, value)| (Arc::from(name.as_str().as_str()), value.clone().in_type(item))).collect())),
            (Self::Map(values), Type::Map(item)) => Self::Map(Arc::new(values.iter()
                .map(|(name, value)| (name.clone(), value.clone().in_type(item))).collect())),
            (Self::List(values), Type::List(item)) => Self::List(Arc::new(values.iter()
                .cloned().map(|value| value.in_type(item)).collect())),
            (Self::Record(values), Type::Record(fields)) => Self::Record(Arc::new(values.iter()
                .map(|(name, value)| (*name, fields.get(name).map_or_else(|| value.clone(), |ty| value.clone().in_type(ty))))
                .collect())),
            (value, Type::Optional(inner)) => value.in_type(inner),
            (value, _) => value,
        }
    }

    pub fn analyze(arena: &AstArena, expr: ExprId, bindings: &FxHashMap<Name, Self>) -> Option<Self> {
        Some(match arena.expr(expr).kind {
            ArenaExprKind::Null => Self::Null,
            ArenaExprKind::Bool(value) => Self::Bool(value),
            ArenaExprKind::Int(value) => Self::Int(arena.int_literal(value).value()?),
            ArenaExprKind::Float(value) => Self::Float(arena.float_literal(value).value()?.to_bits()),
            ArenaExprKind::Duration(value) => Self::Duration(arena.duration_literal(value).millis()?),
            ArenaExprKind::Str(value) => Self::Str(arena.string_literal(value).clone()),
            ArenaExprKind::PathStr(value) => Self::Path(arena.string_literal(value).clone()),
            ArenaExprKind::Bytes(value) => Self::Bytes(arena.bytes_literal(value).clone()),
            ArenaExprKind::Ident(name) => bindings.get(&name)?.clone(),
            ArenaExprKind::Unary { op: UnaryOp::Neg, expr } => match arena.expr(expr).kind {
                ArenaExprKind::Int(value) => Self::Int(arena.int_literal(value).value()?.checked_neg()?),
                ArenaExprKind::Float(value) => Self::Float((-arena.float_literal(value).value()?).to_bits()),
                _ => return None,
            },
            ArenaExprKind::List(items) => Self::List(Arc::new(
                arena.list_elements(items).map(|item| {
                    if item.splice_span.is_some() { None }
                    else { Self::analyze(arena, item.value, bindings) }
                }).collect::<Option<Vec<_>>>()?,
            )),
            ArenaExprKind::Record(fields) if arena.record_fields(fields).iter().any(|field|
                matches!(field.kind, ArenaRecordFieldKind::Computed { .. })) => {
                let mut values = BTreeMap::new();
                for field in arena.record_fields(fields) {
                    let (key, value) = match field.kind {
                        ArenaRecordFieldKind::Computed { key, value, .. } => {
                            let Self::Str(key) = Self::analyze(arena, key, bindings)? else { return None; };
                            (key, Self::analyze(arena, value, bindings)?)
                        }
                        ArenaRecordFieldKind::Named { name, value, .. } =>
                            (Arc::from(name.as_str().as_str()), Self::analyze(arena, value, bindings)?),
                        ArenaRecordFieldKind::Shorthand { name, .. } =>
                            (Arc::from(name.as_str().as_str()), bindings.get(&name)?.clone()),
                        ArenaRecordFieldKind::Spread { expr: value, .. } => {
                            match Self::analyze(arena, value, bindings)? {
                                Self::Map(entries) => values.extend(entries.iter().map(|(key, value)| (key.clone(), value.clone()))),
                                Self::EmptyMap => {},
                                _ => return None,
                            }
                            continue;
                        }
                    };
                    values.insert(key, value);
                }
                Self::Map(Arc::new(values))
            }
            ArenaExprKind::Record(fields) => {
                let mut values = BTreeMap::new();
                for field in arena.record_fields(fields) {
                    let (name, value) = match field.kind {
                        ArenaRecordFieldKind::Named { name, value, .. } => (name, Self::analyze(arena, value, bindings)?),
                        ArenaRecordFieldKind::Shorthand { name, .. } => (name, bindings.get(&name)?.clone()),
                        ArenaRecordFieldKind::Spread { .. } | ArenaRecordFieldKind::Computed { .. } => return None,
                    };
                    if values.insert(name, value).is_some() {
                        return None;
                    }
                }
                Self::Record(Arc::new(values))
            }
            _ => return None,
        })
    }
}

/// Schema symbols are resolved in their defining lexical module. Imported
/// aliases select an exported symbol; they never inspect a runtime record.
#[derive(Clone, Debug, Default)]
pub struct RecordConstructors {
    definitions: FxHashMap<(Option<Name>, Name), TypeDefId>,
    imports: FxHashMap<(Option<Name>, Name), Name>,
    exports: FxHashSet<(Option<Name>, Name)>,
    error_types: FxHashMap<(Option<Name>, Name), Type>,
    namespaces: FxHashMap<TypeDefId, Option<Name>>,
    defaults: FxHashMap<TypeDefId, BTreeMap<Name, LiteralConstant>>,
}

impl RecordConstructors {
    pub fn collect(program: &ArenaProgram) -> Self {
        let mut index = Self::default();
        index.declare_scope(program, None, program.statement_ids().collect());
        for module in &program.modules {
            index.declare_scope(program, Some(module.name), program.module_statements(module).collect());
        }
        index.collect_scope(program, None, program.statement_ids().collect());
        for module in &program.modules {
            index.collect_scope(program, Some(module.name), program.module_statements(module).collect());
        }
        index
    }

    fn declare_scope(&mut self, program: &ArenaProgram, namespace: Option<Name>, statements: Vec<StmtId>) {
        for statement in statements {
            let (kind, exported) = match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => (program.arena.stmt(inner).kind, true),
                kind => (kind, false),
            };
            match kind {
                ArenaStmtKind::TypeDef(id) => {
                    let name = program.arena.type_def(id).name;
                    self.definitions.insert((namespace, name), id);
                    self.namespaces.insert(id, namespace);
                    if exported { self.exports.insert((namespace, name)); }
                }
                ArenaStmtKind::ErrorDef(id) => {
                    let definition = program.arena.error_def(id);
                    self.error_types.insert((namespace, definition.name), Type::ErrorFamily(definition.name));
                    for variant in program.arena.error_variants(definition.variants) {
                        for facet in program.arena.names(variant.facets) {
                            self.error_types.insert((namespace, facet), Type::ErrorFacet(facet));
                        }
                    }
                }
                ArenaStmtKind::Use(id) => {
                    let import = program.arena.use_stmt(id);
                    if let Some(alias) = import.alias.or_else(|| program.arena.names(import.path).last())
                        && let Some(key) = import.resolved.as_deref()
                        && let Some(module) = program.modules.iter().find(|module| module.key.as_str() == key)
                    { self.imports.insert((namespace, alias), module.name); }
                }
                _ => {}
            }
        }
    }

    pub fn definition(&self, namespace: Option<Name>, name: Name) -> Option<TypeDefId> {
        self.definitions.get(&(namespace, name)).copied()
    }

    fn collect_scope(&mut self, program: &ArenaProgram, namespace: Option<Name>, statements: Vec<StmtId>) {
        let arena = &program.arena;
        let mut constants = FxHashMap::default();
        for statement in statements {
            let kind = match arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => arena.stmt(inner).kind,
                kind => kind,
            };
            match kind {
                ArenaStmtKind::Let { target, ty, initializer: ArenaExprOrRun::Expr(expr) } => {
                    if let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind
                        && let Some(value) = LiteralConstant::analyze(arena, expr, &constants)
                    {
                        let value = ty.map_or_else(|| value.clone(), |ty|
                            value.clone().in_type(&self.annotation_type(arena, ty, namespace, 0)));
                        constants.insert(name, value);
                    }
                }
                ArenaStmtKind::TypeDef(id) => {
                    let definition = arena.type_def(id);
                    if let ArenaTypeDefBody::RecordSchema(fields) = definition.body {
                        let mut defaults = BTreeMap::new();
                        for field in arena.schema_fields(fields) {
                            if let Some(expr) = field.default
                                && let Some(value) = LiteralConstant::analyze(arena, expr, &constants)
                            {
                                defaults.insert(field.name, value.in_type(&self.annotation_type(arena, field.ty, namespace, 0)));
                            }
                        }
                        self.defaults.insert(id, defaults);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn namespace(&self, id: TypeDefId) -> Option<Name> {
        self.namespaces.get(&id).copied().flatten()
    }

    pub fn defaults(&self, id: TypeDefId) -> Option<&BTreeMap<Name, LiteralConstant>> {
        self.defaults.get(&id)
    }

    pub fn resolve_call(&self, arena: &AstArena, callee: ExprId, namespace: Option<Name>) -> Option<TypeDefId> {
        match arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => self.resolve_name(arena, namespace, name, 0),
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(alias) = arena.expr(base).kind else { return None; };
                let namespace = Some(*self.imports.get(&(namespace, alias))?);
                if !self.exports.contains(&(namespace, name)) { return None; }
                self.resolve_name(arena, namespace, name, 0)
            }
            _ => None,
        }
    }

    pub fn resolve_annotation(&self, arena: &AstArena, ty: TypeExprId, namespace: Option<Name>) -> Option<TypeDefId> {
        self.resolve_annotation_inner(arena, ty, namespace, 0)
    }

    fn resolve_annotation_inner(&self, arena: &AstArena, ty: TypeExprId, namespace: Option<Name>, depth: usize) -> Option<TypeDefId> {
        let data = arena.type_expr_data[ty.index()];
        match arena.type_expr_tags[ty.index()] {
            ArenaTypeExprTag::Named => self.resolve_name(arena, namespace, Name::from_symbol(Symbol::from_raw(data.lhs)), depth),
            ArenaTypeExprTag::Qualified => {
                let alias = Name::from_symbol(Symbol::from_raw(data.lhs));
                let name = Name::from_symbol(Symbol::from_raw(data.rhs));
                let namespace = Some(*self.imports.get(&(namespace, alias))?);
                if !self.exports.contains(&(namespace, name)) { return None; }
                self.resolve_name(arena, namespace, name, depth)
            }
            _ => None,
        }
    }

    fn resolve_name(&self, arena: &AstArena, namespace: Option<Name>, name: Name, depth: usize) -> Option<TypeDefId> {
        if depth > self.definitions.len() { return None; }
        let id = *self.definitions.get(&(namespace, name))?;
        match arena.type_def(id).body {
            ArenaTypeDefBody::RecordSchema(_) => Some(id),
            ArenaTypeDefBody::Alias(ty) => self.resolve_annotation_inner(arena, ty, namespace, depth + 1),
            _ => None,
        }
    }

    pub fn resolve_type(&self, arena: &AstArena, ty: TypeExprId, namespace: Option<Name>) -> Type {
        self.annotation_type(arena, ty, namespace, 0)
    }

    pub fn schema_type(&self, arena: &AstArena, id: TypeDefId) -> Type {
        self.definition_type(arena, id, 0)
    }

    fn definition_type(&self, arena: &AstArena, id: TypeDefId, depth: usize) -> Type {
        if depth > self.definitions.len() + arena.type_expr_tags.len() { return Type::Unknown; }
        let namespace = self.namespace(id);
        match arena.type_def(id).body {
            ArenaTypeDefBody::RecordSchema(fields) => Type::Record(arena.schema_fields(fields).iter()
                .map(|field| (field.name, self.annotation_type(arena, field.ty, namespace, depth + 1))).collect()),
            ArenaTypeDefBody::Alias(ty) => self.annotation_type(arena, ty, namespace, depth + 1),
            ArenaTypeDefBody::TagUnion(_) => Type::Tag(arena.type_def(id).name),
            ArenaTypeDefBody::ModuleContract(entries) => Type::Module(arena.module_contract_entries(entries).iter().map(|entry| {
                let export = match entry.kind {
                    ArenaModuleContractEntryKind::Value(ty) => ModuleExportType::Value { ty: self.annotation_type(arena, ty, namespace, depth + 1), optional: entry.optional },
                    ArenaModuleContractEntryKind::Proc { params, effects, return_ty } => ModuleExportType::Proc {
                        sig: self.callable_type(arena, params, return_ty, namespace, depth, effects.map(|effects| arena.effects(effects).collect())), optional: entry.optional },
                    ArenaModuleContractEntryKind::Pure { params, return_ty } => ModuleExportType::Pure {
                        sig: self.callable_type(arena, params, return_ty, namespace, depth, None), optional: entry.optional },
                };
                (entry.name, export)
            }).collect()),
        }
    }

    fn callable_type(&self, arena: &AstArena, params: crate::syntax::arena::ArenaRange, return_ty: TypeExprId, namespace: Option<Name>, depth: usize, effects: Option<Vec<crate::syntax::node::Effect>>) -> CallableType {
        CallableType {
            params: arena.params(params).iter().map(|param| CallableParamType { name: param.name,
                ty: self.annotation_type(arena, param.ty, namespace, depth + 1), defaulted: param.default.is_some(), rest: param.rest }).collect(),
            return_ty: Box::new(self.annotation_type(arena, return_ty, namespace, depth + 1)), effects,
        }
    }

    fn annotation_type(&self, arena: &AstArena, ty: TypeExprId, namespace: Option<Name>, depth: usize) -> Type {
        if depth > self.definitions.len() + arena.type_expr_tags.len() { return Type::Unknown; }
        let data = arena.type_expr_data[ty.index()];
        match arena.type_expr_tags[ty.index()] {
            ArenaTypeExprTag::Named => {
                let name = Name::from_symbol(Symbol::from_raw(data.lhs));
                if let Some(ty) = Type::builtin_from_name(&name.as_str()).or_else(|| standard_record_type(&name.as_str())) { return ty; }
                if xsh_registry::errors::builtin_error_families().iter().any(|family| family.name == name.as_str().as_str()) {
                    return Type::ErrorFamily(name);
                }
                if xsh_registry::errors::builtin_error_families().iter().any(|family| family.variants.iter().any(|variant| variant.facets.iter().any(|facet| *facet == name.as_str().as_str()))) {
                    return Type::ErrorFacet(name);
                }
                if let Some(ty) = self.error_types.get(&(namespace, name)) { return ty.clone(); }
                self.definitions.get(&(namespace, name)).map_or(Type::Unknown, |id| self.definition_type(arena, *id, depth + 1))
            }
            ArenaTypeExprTag::Qualified => {
                let alias = Name::from_symbol(Symbol::from_raw(data.lhs));
                let name = Name::from_symbol(Symbol::from_raw(data.rhs));
                let Some(owner) = self.imports.get(&(namespace, alias)) else { return Type::Unknown; };
                if let Some(ty) = self.error_types.get(&(Some(*owner), name)) { return ty.clone(); }
                self.definitions.get(&(Some(*owner), name)).map_or(Type::Unknown, |id| self.definition_type(arena, *id, depth + 1))
            }
            ArenaTypeExprTag::List => Type::List(Box::new(self.annotation_type(arena, TypeExprId::from_index(data.lhs as usize), namespace, depth + 1))),
            ArenaTypeExprTag::Map => Type::Map(Box::new(self.annotation_type(arena, TypeExprId::from_index(data.lhs as usize), namespace, depth + 1))),
            ArenaTypeExprTag::Stream => Type::Stream(Box::new(self.annotation_type(arena, TypeExprId::from_index(data.lhs as usize), namespace, depth + 1))),
            ArenaTypeExprTag::Optional => Type::Optional(Box::new(self.annotation_type(arena, TypeExprId::from_index(data.lhs as usize), namespace, depth + 1))),
            ArenaTypeExprTag::Result => Type::Result(Box::new(self.annotation_type(arena, TypeExprId::from_index(data.lhs as usize), namespace, depth + 1)),
                Box::new(TypeExprId::from_optional_raw(data.rhs).map_or(Type::Error, |id| self.annotation_type(arena, id, namespace, depth + 1)))),
            ArenaTypeExprTag::Module => match self.annotation_type(arena, TypeExprId::from_index(data.lhs as usize), namespace, depth + 1) {
                Type::Module(exports) => Type::Module(exports),
                Type::Record(fields) => Type::Module(fields.into_iter().map(|(name, ty)| (name, ModuleExportType::Value { ty, optional: false })).collect()),
                _ => Type::Invalid,
            },
        }
    }
}
