use crate::map_key::MapKey;
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
use std::sync::{Arc, Mutex};

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
    Regex(crate::syntax::arena::ArenaRegexLiteral),
    Tag { family: Name, variant: Name, fields: Arc<Vec<LiteralConstant>> },
    EmptyMap,
    Map(Arc<BTreeMap<MapKey, LiteralConstant>>),
    List(Arc<Vec<LiteralConstant>>),
    Record(Arc<BTreeMap<Name, LiteralConstant>>),
}

impl LiteralConstant {
    fn map_key(&self) -> Option<MapKey> {
        Some(match self {
            Self::Str(value) => MapKey::Str(value.clone()),
            Self::Int(value) => MapKey::Int(*value),
            Self::Bool(value) => MapKey::Bool(*value),
            Self::Bytes(value) => MapKey::Bytes(value.clone()),
            Self::Path(value) => MapKey::Path(Arc::from(value.as_bytes())),
            Self::Duration(value) => MapKey::Duration(*value),
            _ => return None,
        })
    }

    pub fn in_type(self, expected: &Type) -> Self {
        match (self, expected) {
            (Self::Record(values), Type::Map(key, item)) if matches!(key.as_ref(), Type::Str) => Self::Map(Arc::new(values.iter()
                .map(|(name, value)| (MapKey::from(name.as_str().as_str()), value.clone().in_type(item))).collect())),
            (Self::Map(values), Type::Map(_, item)) => Self::Map(Arc::new(values.iter()
                .map(|(name, value)| (name.clone(), value.clone().in_type(item))).collect())),
            (Self::List(values), Type::List(item)) => Self::List(Arc::new(values.iter()
                .cloned().map(|value| value.in_type(item)).collect())),
            (Self::Record(values), Type::Record(fields)) => Self::Record(Arc::new(values.iter()
                .map(|(name, value)| (*name, fields.get(name).map_or_else(|| value.clone(), |ty| value.clone().in_type(ty))))
                .collect())),
            (Self::Str(text), Type::Path) => Self::Path(text),
            (value, Type::Optional(inner)) => value.in_type(inner),
            (value, _) => value,
        }
    }

    pub fn analyze(arena: &AstArena, expr: ExprId, bindings: &FxHashMap<Name, Self>) -> Option<Self> {
        Self::analyze_prepared(arena, expr, bindings, &FxHashMap::default())
    }

    fn analyze_prepared(arena: &AstArena, expr: ExprId, bindings: &FxHashMap<Name, Self>, prepared: &FxHashMap<ExprId, Self>) -> Option<Self> {
        Self::analyze_depth(arena, expr, bindings, prepared, 0)
    }

    fn analyze_depth(arena: &AstArena, expr: ExprId, bindings: &FxHashMap<Name, Self>, prepared: &FxHashMap<ExprId, Self>, depth: usize) -> Option<Self> {
        if depth > 128 { return None; }
        if let Some(value) = prepared.get(&expr) { return Some(value.clone()); }
        let value = match arena.expr(expr).kind {
            ArenaExprKind::Null => Self::Null,
            ArenaExprKind::Bool(value) => Self::Bool(value),
            ArenaExprKind::Int(value) => Self::Int(arena.int_literal(value).value()?),
            ArenaExprKind::Float(value) => Self::Float(arena.float_literal(value).value()?.to_bits()),
            ArenaExprKind::Duration(value) => Self::Duration(arena.duration_literal(value).millis()?),
            ArenaExprKind::Str(value) => Self::Str(arena.string_literal(value).clone()),
            ArenaExprKind::PathStr(value) => Self::Path(arena.string_literal(value).clone()),
            ArenaExprKind::Bytes(value) => Self::Bytes(arena.bytes_literal(value).clone()),
            ArenaExprKind::Regex(id) => Self::Regex(arena.regex_literal(id).clone()),
            ArenaExprKind::Ident(name) => bindings.get(&name)?.clone(),
            ArenaExprKind::Field { base, name } => {
                let Self::Record(values) = Self::analyze_depth(arena, base, bindings, prepared, depth + 1)? else { return None; };
                values.get(&name)?.clone()
            }
            ArenaExprKind::Unary { op, expr } => match (op, Self::analyze_depth(arena, expr, bindings, prepared, depth + 1)?) {
                (UnaryOp::Neg, Self::Int(value)) => Self::Int(value.checked_neg()?),
                (UnaryOp::Neg, Self::Float(value)) => Self::Float((-f64::from_bits(value)).to_bits()),
                (UnaryOp::Not, Self::Bool(value)) => Self::Bool(!value),
                _ => return None,
            },
            ArenaExprKind::Binary { op, left, right } => fold_constant_binary(op,
                Self::analyze_depth(arena, left, bindings, prepared, depth + 1)?, Self::analyze_depth(arena, right, bindings, prepared, depth + 1)?)?,
            ArenaExprKind::List(items) => Self::List(Arc::new(
                arena.list_elements(items).map(|item| {
                    if item.splice_span.is_some() { None }
                    else { Self::analyze_depth(arena, item.value, bindings, prepared, depth + 1) }
                }).collect::<Option<Vec<_>>>()?,
            )),
            ArenaExprKind::Record(fields) if arena.record_fields(fields).iter().any(|field|
                matches!(field.kind, ArenaRecordFieldKind::Computed { .. })) => {
                let mut values = BTreeMap::new();
                for field in arena.record_fields(fields) {
                    let (key, value) = match field.kind {
                        ArenaRecordFieldKind::Path { .. } => return None,
                        ArenaRecordFieldKind::Computed { key, value, .. } => {
                            let key = Self::analyze_depth(arena, key, bindings, prepared, depth + 1)?.map_key()?;
                            (key, Self::analyze_depth(arena, value, bindings, prepared, depth + 1)?)
                        }
                        ArenaRecordFieldKind::Named { name, value, .. } =>
                            (MapKey::from(name.as_str().as_str()), Self::analyze_depth(arena, value, bindings, prepared, depth + 1)?),
                        ArenaRecordFieldKind::Shorthand { name, .. } =>
                            (MapKey::from(name.as_str().as_str()), bindings.get(&name)?.clone()),
                        ArenaRecordFieldKind::Spread { expr: value, .. } => {
                            match Self::analyze_depth(arena, value, bindings, prepared, depth + 1)? {
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
                        ArenaRecordFieldKind::Named { name, value, .. } => (name, Self::analyze_depth(arena, value, bindings, prepared, depth + 1)?),
                        ArenaRecordFieldKind::Shorthand { name, .. } => (name, bindings.get(&name)?.clone()),
                        ArenaRecordFieldKind::Spread { .. } | ArenaRecordFieldKind::Computed { .. } | ArenaRecordFieldKind::Path { .. } => return None,
                    };
                    if values.insert(name, value).is_some() {
                        return None;
                    }
                }
                Self::Record(Arc::new(values))
            }
            _ => return None,
        };
        constant_size_within_limit(&value).then_some(value)
    }
}

/// A checked schema application failed before a concrete type was published.
#[derive(Clone, Debug)]
pub struct SchemaTypeError {
    pub code: &'static str,
    pub message: String,
}

impl SchemaTypeError {
    fn new(code: &'static str, message: impl Into<String>) -> Self { Self { code, message: message.into() } }
}

/// Schema symbols are resolved in their defining lexical module. Imported
/// aliases select an exported symbol; they never inspect a runtime record.
#[derive(Clone, Debug, Default)]
pub struct RecordConstructors {
    instances: Arc<Mutex<Vec<(TypeDefId, Vec<Type>, Type)>>>,
    definitions: FxHashMap<(Option<Name>, Name), TypeDefId>,
    imports: FxHashMap<(Option<Name>, Name), Name>,
    exports: FxHashSet<(Option<Name>, Name)>,
    error_types: FxHashMap<(Option<Name>, Name), Type>,
    namespaces: FxHashMap<TypeDefId, Option<Name>>,
    nominal_names: FxHashMap<TypeDefId, Name>,
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
                    self.nominal_names.insert(id, crate::sema::wire_enums::declaring_enum_name(program, id));
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

    pub fn apply_prepared_defaults(&mut self, program: &ArenaProgram, prepared: &PreparedConstants) {
        self.collect_scope_prepared(program, None, program.statement_ids().collect(), Some(prepared));
        for module in &program.modules {
            self.collect_scope_prepared(program, Some(module.name), program.module_statements(module).collect(), Some(prepared));
        }
    }

    fn collect_scope(&mut self, program: &ArenaProgram, namespace: Option<Name>, statements: Vec<StmtId>) {
        self.collect_scope_prepared(program, namespace, statements, None);
    }

    fn collect_scope_prepared(&mut self, program: &ArenaProgram, namespace: Option<Name>, statements: Vec<StmtId>, prepared: Option<&PreparedConstants>) {
        let arena = &program.arena;
        let mut constants = FxHashMap::default();
        let empty = FxHashMap::default();
        let prepared_values = prepared.map_or(&empty, |prepared| &prepared.values);
        for statement in &statements {
            let kind = match arena.stmt(*statement).kind { ArenaStmtKind::Export(inner) => arena.stmt(inner).kind, kind => kind };
            if let ArenaStmtKind::Const { target, initializer: ArenaExprOrRun::Expr(expr), .. } = kind {
                if let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind {
                    if let Some(value) = prepared_values.get(&expr) { constants.insert(name, value.clone()); }
                }
            }
        }
        for statement in statements {
            let kind = match arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => arena.stmt(inner).kind,
                kind => kind,
            };
            match kind {
                ArenaStmtKind::Let { target, ty, initializer: ArenaExprOrRun::Expr(expr) } | ArenaStmtKind::Const { target, ty, initializer: ArenaExprOrRun::Expr(expr) } => {
                    if let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind
                        && let Some(value) = LiteralConstant::analyze_prepared(arena, expr, &constants, prepared_values)
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
                                && let Some(value) = LiteralConstant::analyze_prepared(arena, expr, &constants, prepared_values)
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
            ArenaTypeExprTag::Applied => self.resolve_annotation_inner(arena, TypeExprId::from_index(data.lhs as usize), namespace, depth + 1),
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
        self.resolve_type_checked(arena, ty, namespace).unwrap_or(Type::Invalid)
    }

    pub fn resolve_type_checked(&self, arena: &AstArena, ty: TypeExprId, namespace: Option<Name>) -> Result<Type, SchemaTypeError> {
        self.resolve_instance_annotation(arena, ty, namespace, &FxHashMap::default(), &mut Vec::new())
    }

    /// Rigid parameter identities make defaults valid only when they work for every substitution.
    pub fn template_field_types(&self, arena: &AstArena, definition: &crate::syntax::arena::ArenaTypeDef, namespace: Option<Name>) -> Result<BTreeMap<Name, Type>, SchemaTypeError> {
        let bindings = arena.names(definition.type_parameters).map(|name| (name, Type::Tag(Name::intern(format!("type parameter {name}"))))).collect();
        let mut active = self.definition(namespace, definition.name).into_iter().collect();
        match definition.body {
            ArenaTypeDefBody::RecordSchema(fields) => arena.schema_fields(fields).iter().map(|field| self.resolve_instance_annotation(arena, field.ty, namespace, &bindings, &mut active).map(|ty| (field.name, ty))).collect(),
            ArenaTypeDefBody::Alias(ty) => { self.resolve_instance_annotation(arena, ty, namespace, &bindings, &mut active)?; Ok(BTreeMap::new()) }
            _ => Err(SchemaTypeError::new("check.type-parameters", "type parameters are supported only on record schemas and aliases")),
        }
    }

    pub fn resolve_definition_checked(&self, arena: &AstArena, definition: TypeDefId) -> Result<Type, SchemaTypeError> {
        self.instantiate(arena, definition, &[], &mut Vec::new())
    }

    pub fn schema_type(&self, arena: &AstArena, id: TypeDefId) -> Type {
        self.instantiate(arena, id, &[], &mut Vec::new()).unwrap_or(Type::Invalid)
    }

    pub fn constructor_type(&self, arena: &AstArena, callee: ExprId, namespace: Option<Name>) -> Option<Type> {
        let id = match arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => self.definition(namespace, name)?,
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(alias) = arena.expr(base).kind else { return None; };
                let owner = Some(*self.imports.get(&(namespace, alias))?);
                if !self.exports.contains(&(owner, name)) { return None; }
                self.definition(owner, name)?
            }
            _ => return None,
        };
        let ty = self.instantiate(arena, id, &[], &mut Vec::new()).ok()?;
        matches!(ty, Type::Record(_)).then_some(ty)
    }

    fn contains_template_parameter(ty: &Type) -> bool {
        match ty {
            Type::Inference(_) => true,
            Type::Tag(name) => name.as_str().starts_with("type parameter "),
            Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => Self::contains_template_parameter(inner),
            Type::Map(key, value) => Self::contains_template_parameter(key) || Self::contains_template_parameter(value),
            Type::Result(ok, error) => Self::contains_template_parameter(ok) || Self::contains_template_parameter(error),
            Type::Record(fields) => fields.values().any(Self::contains_template_parameter),
            Type::Module(exports) => exports.values().any(|export| match export {
                ModuleExportType::Value { ty, .. } => Self::contains_template_parameter(ty),
                ModuleExportType::Proc { sig, .. } | ModuleExportType::Pure { sig, .. } => Self::contains_template_parameter(&sig.return_ty) || sig.params.iter().any(|param| Self::contains_template_parameter(&param.ty)),
            }),
            _ => false,
        }
    }

    fn instantiate(&self, arena: &AstArena, id: TypeDefId, arguments: &[Type], active: &mut Vec<TypeDefId>) -> Result<Type, SchemaTypeError> {
        let definition = arena.type_def(id);
        if definition.type_parameters.len() != arguments.len() {
            return Err(SchemaTypeError::new("check.type-arity", format!("type `{}` requires {} type arguments, found {}", definition.name, definition.type_parameters.len(), arguments.len())));
        }
        if active.contains(&id) {
            return Err(SchemaTypeError::new("check.recursive-type", format!("recursive or expanding application of `{}` is not supported", definition.name)));
        }
        let cacheable = !arguments.iter().any(Self::contains_template_parameter);
        if cacheable && let Some((_, _, ty)) = self.instances.lock().expect("schema instance cache").iter().find(|(cached, args, _)| *cached == id && args == arguments) { return Ok(ty.clone()); }
        let bindings = arena.names(definition.type_parameters).zip(arguments.iter().cloned()).collect();
        let namespace = self.namespace(id);
        active.push(id);
        let result = match definition.body {
            ArenaTypeDefBody::RecordSchema(fields) => arena.schema_fields(fields).iter().map(|field| {
                self.resolve_instance_annotation(arena, field.ty, namespace, &bindings, active).map(|ty| (field.name, ty))
            }).collect::<Result<BTreeMap<_, _>, _>>().map(Type::Record),
            ArenaTypeDefBody::Alias(ty) => self.resolve_instance_annotation(arena, ty, namespace, &bindings, active),
            ArenaTypeDefBody::TagUnion(_) if arguments.is_empty() => Ok(Type::Tag(self.nominal_names[&id])),
            ArenaTypeDefBody::ModuleContract(entries) if arguments.is_empty() => arena.module_contract_entries(entries).iter().map(|entry| {
                let export = match entry.kind {
                    ArenaModuleContractEntryKind::Value(ty) => ModuleExportType::Value { ty: self.resolve_instance_annotation(arena, ty, namespace, &bindings, active)?, optional: entry.optional },
                    ArenaModuleContractEntryKind::Proc { params, effects, return_ty } => ModuleExportType::Proc { sig: self.instance_callable_type(arena, params, return_ty, namespace, &bindings, active, effects.map(|effects| arena.effects(effects).collect()))?, optional: entry.optional },
                    ArenaModuleContractEntryKind::Pure { params, return_ty } => ModuleExportType::Pure { sig: self.instance_callable_type(arena, params, return_ty, namespace, &bindings, active, None)?, optional: entry.optional },
                };
                Ok((entry.name, export))
            }).collect::<Result<BTreeMap<_, _>, SchemaTypeError>>().map(Type::Module),
            _ => Err(SchemaTypeError::new("check.type-parameters", "type parameters are supported only on record schemas and aliases")),
        };
        active.pop();
        if cacheable && let Ok(ty) = &result { self.instances.lock().expect("schema instance cache").push((id, arguments.to_vec(), ty.clone())); }
        result
    }

    fn application_definition(&self, arena: &AstArena, base: TypeExprId, namespace: Option<Name>) -> Result<TypeDefId, SchemaTypeError> {
        let data = arena.type_expr_data[base.index()];
        let (owner, name) = match arena.type_expr_tags[base.index()] {
            ArenaTypeExprTag::Named => (namespace, Name::from_symbol(Symbol::from_raw(data.lhs))),
            ArenaTypeExprTag::Qualified => {
                let alias = Name::from_symbol(Symbol::from_raw(data.lhs));
                let name = Name::from_symbol(Symbol::from_raw(data.rhs));
                let owner = self.imports.get(&(namespace, alias)).copied().ok_or_else(|| SchemaTypeError::new("check.unknown-type", "unknown type namespace"))?;
                if !self.exports.contains(&(Some(owner), name)) { return Err(SchemaTypeError::new("check.unknown-type", "unknown exported type")); }
                (Some(owner), name)
            }
            _ => return Err(SchemaTypeError::new("check.type-application", "type applications require a named record schema or alias")),
        };
        self.definition(owner, name).ok_or_else(|| SchemaTypeError::new("check.type-application", format!("`{name}` is not a user record schema or alias")))
    }

    fn resolve_instance_annotation(&self, arena: &AstArena, ty: TypeExprId, namespace: Option<Name>, bindings: &FxHashMap<Name, Type>, active: &mut Vec<TypeDefId>) -> Result<Type, SchemaTypeError> {
        let data = arena.type_expr_data[ty.index()];
        let inner = TypeExprId::from_index(data.lhs as usize);
        Ok(match arena.type_expr_tags[ty.index()] {
            ArenaTypeExprTag::Applied => {
                let id = self.application_definition(arena, inner, namespace)?;
                if !matches!(arena.type_def(id).body, ArenaTypeDefBody::RecordSchema(_) | ArenaTypeDefBody::Alias(_)) { return Err(SchemaTypeError::new("check.type-application", "only record schemas and aliases accept type arguments")); }
                let arguments = arena.applied_type_arguments(ty).map(|argument| self.resolve_instance_annotation(arena, argument, namespace, bindings, active)).collect::<Result<Vec<_>, _>>()?;
                self.instantiate(arena, id, &arguments, active)?
            }
            ArenaTypeExprTag::Named => {
                let name = Name::from_symbol(Symbol::from_raw(data.lhs));
                if let Some(ty) = bindings.get(&name) { ty.clone() }
                else if let Some(ty) = Type::builtin_from_name(&name.as_str()).or_else(|| standard_record_type(&name.as_str())) { ty }
                else if let Some(ty) = self.error_types.get(&(namespace, name)) { ty.clone() }
                else if let Some(id) = self.definition(namespace, name) { self.instantiate(arena, id, &[], active)? }
                else if xsh_registry::errors::builtin_error_families().iter().any(|family| family.name == name.as_str().as_str()) { Type::ErrorFamily(name) }
                else if xsh_registry::errors::builtin_error_families().iter().any(|family| family.variants.iter().any(|variant| variant.facets.iter().any(|facet| *facet == name.as_str().as_str()))) { Type::ErrorFacet(name) }
                else { return Err(SchemaTypeError::new("check.unknown-type", format!("unknown type `{name}`"))); }
            }
            ArenaTypeExprTag::Qualified => {
                let alias = Name::from_symbol(Symbol::from_raw(data.lhs));
                let name = Name::from_symbol(Symbol::from_raw(data.rhs));
                let owner = self.imports.get(&(namespace, alias)).copied().ok_or_else(|| SchemaTypeError::new("check.unknown-type", "unknown type namespace"))?;
                if let Some(ty) = self.error_types.get(&(Some(owner), name)) { ty.clone() }
                else { let id = self.application_definition(arena, ty, namespace)?; self.instantiate(arena, id, &[], active)? }
            }
            ArenaTypeExprTag::List => Type::List(Box::new(self.resolve_instance_annotation(arena, inner, namespace, bindings, active)?)),
            ArenaTypeExprTag::Map => Type::Map(Box::new(TypeExprId::from_optional_raw(data.rhs).map_or(Ok(Type::Str), |key| self.resolve_instance_annotation(arena, key, namespace, bindings, active))?), Box::new(self.resolve_instance_annotation(arena, inner, namespace, bindings, active)?)),
            ArenaTypeExprTag::Stream => Type::Stream(Box::new(self.resolve_instance_annotation(arena, inner, namespace, bindings, active)?)),
            ArenaTypeExprTag::Optional => Type::Optional(Box::new(self.resolve_instance_annotation(arena, inner, namespace, bindings, active)?)),
            ArenaTypeExprTag::Result => Type::Result(Box::new(self.resolve_instance_annotation(arena, inner, namespace, bindings, active)?), Box::new(TypeExprId::from_optional_raw(data.rhs).map_or(Ok(Type::Error), |error| self.resolve_instance_annotation(arena, error, namespace, bindings, active))?)),
            ArenaTypeExprTag::Module => match self.resolve_instance_annotation(arena, inner, namespace, bindings, active)? {
                Type::Module(exports) => Type::Module(exports),
                Type::Record(fields) => Type::Module(fields.into_iter().map(|(name, ty)| (name, ModuleExportType::Value { ty, optional: false })).collect()),
                _ => Type::Invalid,
            },
        })
    }

    fn instance_callable_type(&self, arena: &AstArena, params: crate::syntax::arena::ArenaRange, return_ty: TypeExprId, namespace: Option<Name>, bindings: &FxHashMap<Name, Type>, active: &mut Vec<TypeDefId>, effects: Option<Vec<crate::syntax::node::Effect>>) -> Result<CallableType, SchemaTypeError> {
        Ok(CallableType {
            params: arena.params(params).iter().map(|param| Ok(CallableParamType { name: param.name,
                ty: self.resolve_instance_annotation(arena, param.ty, namespace, bindings, active)?, defaulted: param.default.is_some(), rest: param.rest })).collect::<Result<_, SchemaTypeError>>()?,
            return_ty: Box::new(self.resolve_instance_annotation(arena, return_ty, namespace, bindings, active)?), effects,
        })
    }

    fn annotation_type(&self, arena: &AstArena, ty: TypeExprId, namespace: Option<Name>, _depth: usize) -> Type {
        self.resolve_type(arena, ty, namespace)
    }
}

#[cfg(test)]
mod instance_tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn record_schema_instances_share_canonical_resolved_arguments() {
        let source = "type Box[T] = {value: T}\ntype Count = Box[Int]\ntype Same = Box[Int]\ntype Nested = Box[Box[Int]]\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty());
        parsed.arena.symbol_owner().with_current(|| {
            let index = RecordConstructors::collect(&parsed.arena);
            let definition = |name: &str| index.definition(None, Name::intern(name)).unwrap();
            let count = index.resolve_definition_checked(&parsed.arena.arena, definition("Count")).unwrap();
            let first = index.instances.lock().unwrap().len();
            assert_eq!(index.resolve_definition_checked(&parsed.arena.arena, definition("Count")).unwrap(), count);
            assert_eq!(index.instances.lock().unwrap().len(), first);
            assert_eq!(index.resolve_definition_checked(&parsed.arena.arena, definition("Same")).unwrap(), count);
            let box_id = definition("Box");
            assert_eq!(index.instances.lock().unwrap().iter().filter(|(id, arguments, _)| *id == box_id && arguments == &[Type::Int]).count(), 1);
            let nested = index.resolve_definition_checked(&parsed.arena.arena, definition("Nested")).unwrap();
            assert_eq!(nested, Type::Record(BTreeMap::from([(Name::intern("value"), count)])));
        });
    }
}

impl LiteralConstant {
    pub fn matches_data_type(&self, expected: &Type) -> bool {
        constant_type_allowed(expected) && constant_matches_type(self, expected)
    }

    pub fn value_type(&self) -> Type {
        match self {
            Self::Null => Type::Null, Self::Bool(_) => Type::Bool, Self::Int(_) => Type::Int,
            Self::Float(_) => Type::Float, Self::Duration(_) => Type::Duration,
            Self::Str(_) => Type::Str, Self::Path(_) => Type::Path, Self::Bytes(_) => Type::Bytes,
            Self::Regex(_) => Type::Regex, Self::Tag { family, .. } => Type::Tag(*family),
            Self::EmptyMap => Type::Map(Box::new(Type::Str), Box::new(Type::Unknown)),
            Self::Map(values) => Type::Map(Box::new(constant_map_key_type(values.keys())), Box::new(constant_item_type(values.values()))),
            Self::List(values) => Type::List(Box::new(constant_item_type(values.iter()))),
            Self::Record(values) => Type::Record(values.iter().map(|(key, value)| (*key, value.value_type())).collect()),
        }
    }
}

/// Prepared declarations own data only. Lexical runtime bindings remain barriers
/// to constant lookup even when their initializer happens to be a literal.
#[derive(Clone, Debug, Default)]
pub struct PreparedConstants {
    pub values: FxHashMap<ExprId, LiteralConstant>,
    pub types: FxHashMap<ExprId, Type>,
    pub origins: FxHashMap<ExprId, ExprId>,
    pub tail_bindings: BTreeMap<crate::source::Span, ExprId>,
    pub global_bindings: FxHashMap<(Option<Name>, Name), ExprId>,
    pub diagnostics: Vec<crate::diagnostic::Diagnostic>,
}

#[derive(Default)]
struct ConstantScope {
    parent: Option<usize>,
    namespace: Option<Name>,
    bindings: FxHashMap<Name, Option<StmtId>>,
}

struct ConstantPreparation<'a> {
    program: &'a ArenaProgram,
    constructors: &'a RecordConstructors,
    scopes: Vec<ConstantScope>,
    statement_scopes: FxHashMap<StmtId, usize>,
    module_scopes: FxHashMap<Name, usize>,
    active: FxHashSet<StmtId>,
    prepared: PreparedConstants,
    steps: usize,
}

impl PreparedConstants {
    pub fn analyze_expression(&self, arena: &AstArena, expr: ExprId) -> Option<LiteralConstant> {
        let value = LiteralConstant::analyze_prepared(arena, expr, &FxHashMap::default(), &self.values)?;
        constant_size_within_limit(&value).then_some(value)
    }

    pub fn collect(program: &ArenaProgram, constructors: &RecordConstructors) -> Self {
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let arena = &program.arena;
        if !arena.stmt_tags.contains(&crate::syntax::arena::ArenaStmtTag::Const) { return Self::default(); }
        let block_count = arena.blocks.len();
        let main_scope = block_count;
        let mut preparation = ConstantPreparation {
            program, constructors,
            scopes: (0..block_count + 1).map(|_| ConstantScope::default()).collect(),
            statement_scopes: FxHashMap::default(), module_scopes: FxHashMap::default(),
            active: FxHashSet::default(), prepared: Self::default(), steps: 0,
        };
        let mut sources = FxHashMap::default();
        for module in &program.modules {
            let scope = preparation.scopes.len();
            preparation.scopes.push(ConstantScope { namespace: Some(module.name), ..ConstantScope::default() });
            preparation.module_scopes.insert(module.name, scope);
            for statement in program.module_statements(module) { sources.insert(arena.stmt(statement).span.source_id, scope); }
        }
        for index in 0..block_count {
            let span = arena.span(arena.block(BlockId::from_index(index)).span);
            let parent = (0..block_count).filter(|other| *other != index).filter(|other| {
                let outer = arena.span(arena.block(BlockId::from_index(*other)).span);
                outer.source_id == span.source_id && outer.start() <= span.start() && outer.end() >= span.end()
                    && (outer.start() < span.start() || outer.end() > span.end())
            }).min_by_key(|other| { let span = arena.span(arena.block(BlockId::from_index(*other)).span); span.end() - span.start() })
                .unwrap_or_else(|| sources.get(&span.source_id).copied().unwrap_or(main_scope));
            preparation.scopes[index].parent = Some(parent);
            preparation.scopes[index].namespace = sources.get(&span.source_id).map(|scope| preparation.scopes[*scope].namespace).flatten();
            for param in arena.block_params(arena.block(BlockId::from_index(index)).params) {
                preparation.scopes[index].bindings.insert(param.name, None);
            }
        }
        for index in 0..arena.function_defs.len() {
            let function = arena.function_def(FunctionDefId::from_index(index));
            for param in arena.params(function.params) { preparation.scopes[function.body.index()].bindings.insert(param.name, None); }
        }
        for index in 0..arena.stmt_tags.len() {
            let id = StmtId::from_index(index);
            let statement = arena.stmt(id);
            let scope = (0..block_count).filter(|block| {
                let span = arena.span(arena.block(BlockId::from_index(*block)).span);
                span.source_id == statement.span.source_id && span.start() <= statement.span.start() && span.end() >= statement.span.end()
            }).min_by_key(|block| { let span = arena.span(arena.block(BlockId::from_index(*block)).span); span.end() - span.start() })
                .unwrap_or_else(|| sources.get(&statement.span.source_id).copied().unwrap_or(main_scope));
            preparation.statement_scopes.insert(id, scope);
            match statement.kind {
                ArenaStmtKind::Const { target, .. } | ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
                    if let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind {
                        let declaration = matches!(statement.kind, ArenaStmtKind::Const { .. }).then_some(id);
                        preparation.scopes[scope].bindings.insert(name, declaration);
                    }
                }
                _ => {}
            }
        }
        for index in 0..arena.stmt_tags.len() {
            let id = StmtId::from_index(index);
            let scope = preparation.statement_scopes[&id];
            match arena.stmt(id).kind {
                ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } | ArenaStmtKind::Guard { target, .. } => {
                    for name in constant_runtime_target_names(arena, target) { preparation.scopes[scope].bindings.insert(name, None); }
                }
                ArenaStmtKind::For { target, block, .. } => {
                    for name in constant_runtime_target_names(arena, target) { preparation.scopes[block.index()].bindings.insert(name, None); }
                }
                ArenaStmtKind::With { bindings, body, .. } => {
                    for binding in arena.with_bindings(bindings) { preparation.scopes[body.index()].bindings.insert(binding.name, None); }
                }
                _ => {}
            }
        }
        for arm in &arena.match_arms {
            for name in constant_pattern_names(arena, arm.pattern) { preparation.scopes[arm.block.index()].bindings.insert(name, None); }
        }
        for index in 0..arena.stmt_tags.len() {
            let id = StmtId::from_index(index);
            if matches!(arena.stmt(id).kind, ArenaStmtKind::Const { .. }) {
                preparation.steps = 0;
                if let Err((span, message)) = preparation.declaration(id, 0) {
                    preparation.prepared.diagnostics.push(crate::diagnostic::Diagnostic::error(message)
                        .with_code("check.const").with_label(crate::diagnostic::Label::primary(span, "constant preparation failed")));
                }
            }
        }
        for index in 0..arena.expr_tags.len() {
            let id = ExprId::from_index(index);
            let expression = arena.expr(id);
            if !matches!(expression.kind, ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. }) { continue; }
            let scope = (0..block_count).filter(|block| {
                let span = arena.span(arena.block(BlockId::from_index(*block)).span);
                span.source_id == expression.span.source_id && span.start() <= expression.span.start() && span.end() >= expression.span.end()
            }).min_by_key(|block| { let span = arena.span(arena.block(BlockId::from_index(*block)).span); span.end() - span.start() })
                .unwrap_or_else(|| sources.get(&expression.span.source_id).copied().unwrap_or(main_scope));
            preparation.steps = 0;
            if let Ok(value) = preparation.expression(id, scope, None, 0) {
                preparation.prepared.types.entry(id).or_insert_with(|| value.value_type());
                preparation.prepared.values.insert(id, value);
            }
        }
        for index in 0..arena.stmt_tags.len() {
            let id = StmtId::from_index(index);
            let statement = arena.stmt(id);
            let ArenaStmtKind::TailBareIdent(name) = statement.kind else { continue; };
            let mut current = Some(preparation.statement_scopes[&id]);
            while let Some(scope) = current {
                if let Some(binding) = preparation.scopes[scope].bindings.get(&name) {
                    if let Some(declaration) = binding {
                        if let ArenaStmtKind::Const { initializer: ArenaExprOrRun::Expr(expr), .. } = arena.stmt(*declaration).kind {
                            if preparation.prepared.values.contains_key(&expr) {
                                preparation.prepared.tail_bindings.insert(statement.span, expr);
                            }
                        }
                    }
                    break;
                }
                current = preparation.scopes[scope].parent;
            }
        }
        for scope in block_count..preparation.scopes.len() {
            for (name, binding) in &preparation.scopes[scope].bindings {
                if let Some(declaration) = binding {
                    if let ArenaStmtKind::Const { initializer: ArenaExprOrRun::Expr(expr), .. } = arena.stmt(*declaration).kind {
                        preparation.prepared.global_bindings.insert((preparation.scopes[scope].namespace, *name), expr);
                    }
                }
            }
        }
        preparation.prepared
    }
}

impl ConstantPreparation<'_> {
    fn tag_constructor(&self, callee: ExprId, scope: usize) -> Option<(Name, Name, Vec<TypeExprId>)> {
        let arena = &self.program.arena;
        let (namespace, name, qualified) = match arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => (self.scopes[scope].namespace, name, false),
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(alias) = arena.expr(base).kind else { return None; };
                (Some(*self.constructors.imports.get(&(self.scopes[scope].namespace, alias))?), name, true)
            }
            _ => return None,
        };
        for ((owner, family), definition) in &self.constructors.definitions {
            if *owner != namespace || (qualified && !self.constructors.exports.contains(&(*owner, *family))) { continue; }
            if let ArenaTypeDefBody::TagUnion(variants) = arena.type_def(*definition).body {
                for variant in arena.tag_variants(variants) {
                    if variant.name == name {
                        return Some((self.constructors.nominal_names[definition], name, arena.extra_range(variant.fields).iter().map(|raw| TypeExprId::from_index(*raw as usize)).collect()));
                    }
                }
            }
        }
        None
    }

    fn declaration(&mut self, id: StmtId, depth: usize) -> Result<LiteralConstant, (crate::source::Span, String)> {
        let statement = self.program.arena.stmt(id);
        let ArenaStmtKind::Const { target, ty, initializer: ArenaExprOrRun::Expr(expr) } = statement.kind else {
            return Err((statement.span, "const requires a data expression".into()));
        };
        if let Some(value) = self.prepared.values.get(&expr) { return Ok(value.clone()); }
        if !matches!(self.program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name.as_str().as_str() != "_") {
            return Err((statement.span, "const requires one ordinary name".into()));
        }
        if !self.active.insert(id) { return Err((statement.span, "constant dependency cycle".into())); }
        let scope = self.statement_scopes[&id];
        let expected = ty.map(|ty| self.constructors.resolve_type(&self.program.arena, ty, self.scopes[scope].namespace));
        let result = self.expression(expr, scope, expected.as_ref(), depth + 1);
        self.active.remove(&id);
        let value = result?;
        if expected.as_ref().is_some_and(|ty| !constant_type_allowed(ty)) {
            return Err((statement.span, "const requires a concrete data type".into()));
        }
        if expected.is_none() && !constant_type_allowed(self.prepared.types.get(&expr).unwrap_or(&value.value_type())) {
            return Err((statement.span, "const empty containers require a concrete type annotation".into()));
        }
        if expected.as_ref().is_some_and(|ty| !constant_matches_type(&value, ty)) {
            return Err((statement.span, "constant value does not match its declared type".into()));
        }
        let inferred = self.prepared.types.get(&expr).cloned().unwrap_or_else(|| value.value_type());
        self.prepared.types.insert(expr, expected.unwrap_or(inferred));
        self.prepared.origins.entry(expr).or_insert(expr);
        self.prepared.values.insert(expr, value.clone());
        Ok(value)
    }

    fn reference(&mut self, scope: usize, name: Name, span: crate::source::Span, depth: usize) -> Result<LiteralConstant, (crate::source::Span, String)> {
        let mut current = Some(scope);
        while let Some(scope) = current {
            if let Some(binding) = self.scopes[scope].bindings.get(&name).copied() {
                return match binding { Some(id) => self.declaration(id, depth + 1), None => Err((span, "const cannot reference a runtime binding".into())) };
            }
            current = self.scopes[scope].parent;
        }
        Err((span, "const reference must resolve to a const declaration".into()))
    }

    fn expression(&mut self, id: ExprId, scope: usize, expected: Option<&Type>, depth: usize) -> Result<LiteralConstant, (crate::source::Span, String)> {
        use crate::syntax::arena::ArenaCallArgKind;
        let arena = &self.program.arena;
        let expr = arena.expr(id);
        let failure = || (expr.span, "expression is outside the bounded constant data subset".to_string());
        self.steps += 1;
        if depth > 128 || self.steps > 100_000 { return Err((expr.span, "constant preparation limit exceeded".into())); }
        let value = match expr.kind {
            ArenaExprKind::Ident(name) => {
                if let Some((family, variant, fields)) = self.tag_constructor(id, scope) {
                    if !fields.is_empty() { return Err(failure()); }
                    LiteralConstant::Tag { family, variant, fields: Arc::new(Vec::new()) }
                } else {
                    let value = self.reference(scope, name, expr.span, depth)?;
                    let mut current = Some(scope);
                    while let Some(scope) = current {
                        if let Some(Some(declaration)) = self.scopes[scope].bindings.get(&name) {
                            if let ArenaStmtKind::Const { initializer: ArenaExprOrRun::Expr(initializer), .. } = arena.stmt(*declaration).kind {
                                if let Some(ty) = self.prepared.types.get(&initializer).cloned() { self.prepared.types.insert(id, ty); }
                                self.prepared.origins.insert(id, self.prepared.origins.get(&initializer).copied().unwrap_or(initializer));
                            }
                            break;
                        }
                        current = self.scopes[scope].parent;
                    }
                    value
                }
            },
            ArenaExprKind::Field { base, name } => {
                let module_reference = if let ArenaExprKind::Ident(alias) = arena.expr(base).kind {
                    let mut current = Some(scope);
                    let mut lexical = false;
                    while let Some(scope) = current {
                        if self.scopes[scope].bindings.contains_key(&alias) { lexical = true; break; }
                        current = self.scopes[scope].parent;
                    }
                    !lexical && self.constructors.imports.contains_key(&(self.scopes[scope].namespace, alias))
                } else { false };
                if module_reference {
                if let Some((family, variant, fields)) = self.tag_constructor(id, scope) {
                    if !fields.is_empty() { return Err(failure()); }
                    return Ok(LiteralConstant::Tag { family, variant, fields: Arc::new(Vec::new()) });
                }
                let ArenaExprKind::Ident(alias) = arena.expr(base).kind else { return Err(failure()); };
                let owner = self.constructors.imports.get(&(self.scopes[scope].namespace, alias)).copied().ok_or_else(failure)?;
                let module_scope = self.module_scopes[&owner];
                let declaration = self.scopes[module_scope].bindings.get(&name).copied().flatten().ok_or_else(failure)?;
                let exported = self.program.module_statements(self.program.modules.iter().find(|module| module.name == owner).unwrap())
                    .any(|stmt| matches!(arena.stmt(stmt).kind, ArenaStmtKind::Export(inner) if inner == declaration));
                if !exported { return Err((expr.span, "constant is private to its module".into())); }
                let value = self.declaration(declaration, depth + 1)?;
                if let ArenaStmtKind::Const { initializer: ArenaExprOrRun::Expr(initializer), .. } = arena.stmt(declaration).kind {
                    if let Some(ty) = self.prepared.types.get(&initializer).cloned() { self.prepared.types.insert(id, ty); }
                    self.prepared.origins.insert(id, self.prepared.origins.get(&initializer).copied().unwrap_or(initializer));
                }
                value
                } else {
                    let value = self.expression(base, scope, None, depth + 1)?;
                    let source_type = self.prepared.types.get(&base).cloned().unwrap_or_else(|| value.value_type());
                    let (Type::Record(fields), LiteralConstant::Record(values)) = (source_type, value) else { return Err(failure()); };
                    let ty = fields.get(&name).ok_or_else(failure)?.clone();
                    self.prepared.types.insert(id, ty);
                    values.get(&name).ok_or_else(failure)?.clone()
                }
            }
            ArenaExprKind::Unary { op, expr: child } => {
                let child = self.expression(child, scope, None, depth + 1)?;
                match (op, child) {
                    (UnaryOp::Not, LiteralConstant::Bool(value)) => LiteralConstant::Bool(!value),
                    (UnaryOp::Neg, LiteralConstant::Int(value)) => LiteralConstant::Int(value.checked_neg().ok_or_else(failure)?),
                    (UnaryOp::Neg, LiteralConstant::Float(value)) => LiteralConstant::Float((-f64::from_bits(value)).to_bits()),
                    _ => return Err(failure()),
                }
            }
            ArenaExprKind::Binary { op, left, right } => {
                let (left_id, right_id) = (left, right);
                let left = self.expression(left, scope, None, depth + 1)?;
                // Both operands must belong to the subset; short circuiting cannot admit runtime dependencies.
                let right = self.expression(right, scope, None, depth + 1)?;
                if matches!(op, crate::syntax::node::BinaryOp::Eq | crate::syntax::node::BinaryOp::Ne) {
                    let left_type = self.prepared.types.get(&left_id).cloned().unwrap_or_else(|| left.value_type());
                    let right_type = self.prepared.types.get(&right_id).cloned().unwrap_or_else(|| right.value_type());
                    if !right_type.matches_expected(&left_type) { return Err((expr.span, "constant equality operands have incompatible types".into())); }
                    let equal = constant_values_equal(&left, &right);
                    LiteralConstant::Bool(if op == crate::syntax::node::BinaryOp::Eq { equal } else { !equal })
                } else {
                    fold_constant_binary(op, left, right).ok_or_else(|| (expr.span, "invalid constant operation or arithmetic overflow".into()))?
                }
            }
            ArenaExprKind::List(items) => {
                let item_ty = match expected { Some(Type::List(item)) => Some(item.as_ref()), _ => None };
                let mut values = Vec::new();
                for item in arena.list_elements(items) {
                    if item.splice_span.is_some() {
                        let LiteralConstant::List(items) = self.expression(item.value, scope, expected, depth + 1)? else { return Err(failure()); };
                        values.extend(items.iter().cloned());
                    } else { values.push(self.expression(item.value, scope, item_ty, depth + 1)?); }
                }
                LiteralConstant::List(Arc::new(values))
            }
            ArenaExprKind::Record(fields) => {
                let map = matches!(expected, Some(Type::Map(_, _))) || arena.record_fields(fields).iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Computed { .. }));
                let mut values = BTreeMap::new();
                let mut map_values = BTreeMap::new();
                let (key_context, value_context) = match expected { Some(Type::Map(key, value)) => (Some(key.as_ref()), Some(value.as_ref())), _ => (None, None) };
                for field in arena.record_fields(fields) {
                    let (name, key, value) = match field.kind {
                        ArenaRecordFieldKind::Path { .. } => return Err(failure()),
                        ArenaRecordFieldKind::Computed { key, value, .. } => {
                            let key = self.expression(key, scope, key_context, depth + 1)?.map_key().ok_or_else(failure)?;
                            (None, key, value)
                        }
                        ArenaRecordFieldKind::Named { name, value, .. } => (Some(name), MapKey::from(name.as_str().as_str()), value),
                        ArenaRecordFieldKind::Shorthand { name, .. } => {
                            let value = self.reference(scope, name, expr.span, depth + 1)?;
                            if map { map_values.insert(MapKey::from(name.as_str().as_str()), value); }
                            else if values.insert(name, value).is_some() { return Err(failure()); }
                            continue;
                        }
                        ArenaRecordFieldKind::Spread { expr: child, .. } => {
                            match self.expression(child, scope, expected, depth + 1)? {
                                LiteralConstant::Record(entries) if !map => values.extend(entries.iter().map(|(key, value)| (*key, value.clone()))),
                                LiteralConstant::Map(entries) if map => map_values.extend(entries.iter().map(|(key, value)| (key.clone(), value.clone()))),
                                _ => return Err(failure()),
                            }
                            continue;
                        }
                    };
                    let child_ty = if map { value_context } else { match (expected, name) { (Some(Type::Record(fields)), Some(name)) => fields.get(&name), _ => None } };
                    let value = self.expression(value, scope, child_ty, depth + 1)?;
                    if map { map_values.insert(key, value); }
                    else if values.insert(name.ok_or_else(failure)?, value).is_some() { return Err(failure()); }
                }
                if map {
                    if !map_values.is_empty() && !constant_map_key_type(map_values.keys()).is_map_key() { return Err((expr.span, "constant Map keys must have one scalar domain".into())); }
                    LiteralConstant::Map(Arc::new(map_values))
                } else { LiteralConstant::Record(Arc::new(values)) }
            }
            ArenaExprKind::Call { callee, args } => {
                if let Some((family, variant, fields)) = self.tag_constructor(callee, scope) {
                    let args = arena.call_args(args);
                    if args.len() != fields.len() { return Err(failure()); }
                    let mut values = Vec::new();
                    for (arg, field) in args.iter().zip(fields) {
                        let ArenaCallArgKind::Positional(value) = arg.kind else { return Err(failure()); };
                        let ty = self.constructors.resolve_type(arena, field, self.scopes[scope].namespace);
                        let value = self.expression(value, scope, Some(&ty), depth + 1)?;
                        if !constant_matches_type(&value, &ty) { return Err(failure()); }
                        values.push(value);
                    }
                    return Ok(LiteralConstant::Tag { family, variant, fields: Arc::new(values) });
                }
                let definition = self.constructors.resolve_call(arena, callee, self.scopes[scope].namespace).ok_or_else(failure)?;
                let Some(Type::Record(field_types)) = self.constructors.constructor_type(arena, callee, self.scopes[scope].namespace) else { return Err(failure()); };
                let mut values = self.constructors.defaults(definition).cloned().unwrap_or_default();
                let owner = self.constructors.namespace(definition);
                let default_scope = owner.and_then(|owner| self.module_scopes.get(&owner).copied()).unwrap_or(self.program.arena.blocks.len());
                if let ArenaTypeDefBody::RecordSchema(fields) = arena.type_def(definition).body {
                    for field in arena.schema_fields(fields) {
                        if !values.contains_key(&field.name) {
                            if let Some(default) = field.default {
                                values.insert(field.name, self.expression(default, default_scope, field_types.get(&field.name), depth + 1)?);
                            }
                        }
                    }
                }
                let mut supplied = FxHashSet::default();
                for arg in arena.call_args(args) {
                    match arg.kind {
                        ArenaCallArgKind::Named { name, value, .. } => {
                            let ty = field_types.get(&name).ok_or_else(failure)?;
                            if !supplied.insert(name) { return Err(failure()); }
                            values.insert(name, self.expression(value, scope, Some(ty), depth + 1)?);
                        }
                        ArenaCallArgKind::NamedSpread { value, .. } => {
                            let prepared = self.expression(value, scope, None, depth + 1)?;
                            let visible = self.prepared.types.get(&value).cloned().unwrap_or_else(|| prepared.value_type());
                            let (Type::Record(visible), LiteralConstant::Record(record)) = (visible, prepared) else { return Err(failure()); };
                            for (name, actual) in visible {
                                let ty = field_types.get(&name).ok_or_else(failure)?;
                                if !actual.matches_expected(ty) || !supplied.insert(name) { return Err(failure()); }
                                let value = record.get(&name).ok_or_else(failure)?.clone().in_type(ty);
                                if !constant_matches_type(&value, ty) { return Err(failure()); }
                                values.insert(name, value);
                            }
                        }
                        _ => return Err(failure()),
                    }
                }
                if field_types.keys().any(|name| !values.contains_key(name)) { return Err(failure()); }
                for (name, value) in &mut values {
                    let ty = field_types.get(name).ok_or_else(failure)?;
                    *value = value.clone().in_type(ty);
                    if !constant_matches_type(value, ty) { return Err(failure()); }
                }
                self.prepared.types.insert(id, Type::Record(field_types));
                LiteralConstant::Record(Arc::new(values))
            }
            _ => LiteralConstant::analyze(arena, id, &FxHashMap::default()).ok_or_else(failure)?,
        };
        if matches!(expr.kind, ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. }) {
            if let (Some(expected), Some(actual)) = (expected, self.prepared.types.get(&id)) {
                if !actual.matches_expected(expected) { return Err((expr.span, "constant reference does not match its expected type".into())); }
            }
        }
        let value = expected.map_or_else(|| value.clone(), |ty| value.clone().in_type(ty));
        if expected.is_some_and(|ty| !constant_matches_type(&value, ty)) { return Err((expr.span, "constant value does not match its expected type".into())); }
        if let Some(ty) = expected { self.prepared.types.insert(id, ty.clone()); }
        if expected.is_none() && !constant_type_allowed(self.prepared.types.get(&id).unwrap_or(&value.value_type())) {
            return Err((expr.span, "constant containers require concrete type context".into()));
        }
        if !constant_size_within_limit(&value) { return Err((expr.span, "constant data exceeds the preparation limit".into())); }
        if let LiteralConstant::Path(path) = &value {
            if path.as_bytes().contains(&0) { return Err((expr.span, "constant path contains a NUL byte".into())); }
        }
        Ok(value)
    }
}

fn constant_type_allowed(ty: &Type) -> bool {
    match ty {
        Type::Null | Type::Bool | Type::Int | Type::UInt | Type::Float | Type::Duration | Type::Str | Type::Bytes | Type::Path | Type::Regex | Type::Tag(_) => true,
        Type::List(item) | Type::Optional(item) => constant_type_allowed(item),
        Type::Map(key, value) => key.is_map_key() && constant_type_allowed(value),
        Type::Record(fields) => fields.values().all(constant_type_allowed),
        _ => false,
    }
}

fn fold_constant_binary(op: crate::syntax::node::BinaryOp, left: LiteralConstant, right: LiteralConstant) -> Option<LiteralConstant> {
    use crate::syntax::node::BinaryOp as B;
    use LiteralConstant as C;
    if matches!(op, B::Add | B::Sub | B::Mul | B::Div) && (matches!(left, C::Duration(_)) || matches!(right, C::Duration(_))) {
        use crate::duration::{checked_duration_binary, DurationOperand as D, DurationResult};
        let operand = |value: C| match value { C::Duration(ms) => Some(D::Millis(ms)), C::Int(count) => Some(D::Count(count)), _ => None };
        return match checked_duration_binary(op, operand(left)?, operand(right)?).ok()? {
            DurationResult::Millis(ms) => Some(C::Duration(ms)), DurationResult::Count(count) => Some(C::Int(count)),
        };
    }
    Some(match (op, left, right) {
        (B::Lt, C::Duration(a), C::Duration(b)) => C::Bool(a < b),
        (B::Le, C::Duration(a), C::Duration(b)) => C::Bool(a <= b),
        (B::Gt, C::Duration(a), C::Duration(b)) => C::Bool(a > b),
        (B::Ge, C::Duration(a), C::Duration(b)) => C::Bool(a >= b),
        (B::Add, C::Int(a), C::Int(b)) => C::Int(a.checked_add(b)?),
        (B::Sub, C::Int(a), C::Int(b)) => C::Int(a.checked_sub(b)?),
        (B::Mul, C::Int(a), C::Int(b)) => C::Int(a.checked_mul(b)?),
        (B::Div, C::Int(a), C::Int(b)) => C::Int(a.checked_div(b)?),
        (B::Rem, C::Int(a), C::Int(b)) => C::Int(a.checked_rem(b)?),
        (B::Lt, C::Int(a), C::Int(b)) => C::Bool(a < b), (B::Le, C::Int(a), C::Int(b)) => C::Bool(a <= b),
        (B::Gt, C::Int(a), C::Int(b)) => C::Bool(a > b), (B::Ge, C::Int(a), C::Int(b)) => C::Bool(a >= b),
        (B::And, C::Bool(a), C::Bool(b)) => C::Bool(a && b), (B::Or, C::Bool(a), C::Bool(b)) => C::Bool(a || b),
        (B::Add, C::Str(a), C::Str(b)) => C::Str(Arc::from(format!("{a}{b}"))),
        (B::Lt, C::Str(a), C::Str(b)) => C::Bool(a < b), (B::Le, C::Str(a), C::Str(b)) => C::Bool(a <= b),
        (B::Gt, C::Str(a), C::Str(b)) => C::Bool(a > b), (B::Ge, C::Str(a), C::Str(b)) => C::Bool(a >= b),
        (B::In, C::Str(a), C::Str(b)) => C::Bool(b.contains(a.as_ref())),
        (B::NotIn, C::Str(a), C::Str(b)) => C::Bool(!b.contains(a.as_ref())),
        (B::Eq, C::Float(a), C::Float(b)) => C::Bool(f64::from_bits(a) == f64::from_bits(b)),
        (B::Ne, C::Float(a), C::Float(b)) => C::Bool(f64::from_bits(a) != f64::from_bits(b)),
        (B::Eq, a, b) if b.value_type().matches_expected(&a.value_type()) => C::Bool(constant_values_equal(&a, &b)),
        (B::Ne, a, b) if b.value_type().matches_expected(&a.value_type()) => C::Bool(!constant_values_equal(&a, &b)),
        (op, C::Float(a), C::Float(b)) => {
            let (a, b) = (f64::from_bits(a), f64::from_bits(b));
            match op { B::Add => C::Float((a+b).to_bits()), B::Sub => C::Float((a-b).to_bits()),
                B::Mul => C::Float((a*b).to_bits()), B::Div => C::Float((a/b).to_bits()),
                B::Lt => C::Bool(a<b), B::Le => C::Bool(a<=b), B::Gt => C::Bool(a>b), B::Ge => C::Bool(a>=b), _ => return None }
        }
        _ => return None,
    })
}

fn constant_item_type<'a>(mut values: impl Iterator<Item = &'a LiteralConstant>) -> Type {
    let Some(first) = values.next() else { return Type::Unknown; };
    let ty = first.value_type();
    if values.all(|value| value.value_type() == ty) { ty } else { Type::Unknown }
}

fn constant_map_key_type<'a>(mut keys: impl Iterator<Item = &'a MapKey>) -> Type {
    let Some(first) = keys.next() else { return Type::Str; };
    let ty = constant_key_type(first);
    if keys.all(|key| constant_key_type(key) == ty) { ty } else { Type::Unknown }
}

fn constant_key_type(key: &MapKey) -> Type {
    match key {
        MapKey::Str(_) => Type::Str, MapKey::Int(_) => Type::Int,
        MapKey::Bool(_) => Type::Bool, MapKey::Bytes(_) => Type::Bytes,
        MapKey::Path(_) => Type::Path, MapKey::Duration(_) => Type::Duration,
    }
}

fn constant_key_matches_type(key: &MapKey, ty: &Type) -> bool {
    if let (MapKey::Int(value), Type::UInt) = (key, ty) { *value >= 0 }
    else { constant_key_type(key) == *ty }
}

fn constant_key_size(key: &MapKey) -> usize {
    match key {
        MapKey::Str(value) => value.len(),
        MapKey::Bytes(value) | MapKey::Path(value) => value.len(),
        MapKey::Int(_) | MapKey::Duration(_) => 8, MapKey::Bool(_) => 1,
    }
}

fn constant_matches_type(value: &LiteralConstant, ty: &Type) -> bool {
    match (value, ty) {
        (LiteralConstant::List(values), Type::List(item)) => values.iter().all(|value| constant_matches_type(value, item)),
        (LiteralConstant::Map(values), Type::Map(key, item)) => values.iter().all(|(actual_key, value)| constant_key_matches_type(actual_key, key) && constant_matches_type(value, item)),
        (LiteralConstant::EmptyMap, Type::Map(_, _)) => true,
        (LiteralConstant::Int(value), Type::UInt) => *value >= 0,
        (LiteralConstant::Record(values), Type::Record(fields)) => fields.len() == values.len() && fields.iter().all(|(name, ty)| values.get(name).is_some_and(|value| constant_matches_type(value, ty))),
        (LiteralConstant::Null, Type::Optional(_)) => true,
        (value, Type::Optional(inner)) => constant_matches_type(value, inner),
        (value, ty) => &value.value_type() == ty,
    }
}

fn constant_size_within_limit(value: &LiteralConstant) -> bool {
    let mut pending = vec![value];
    let mut units = 0usize;
    while let Some(value) = pending.pop() {
        units = units.saturating_add(1);
        match value {
            LiteralConstant::Str(text) | LiteralConstant::Path(text) => units = units.saturating_add(text.len()),
            LiteralConstant::Bytes(bytes) => units = units.saturating_add(bytes.len()),
            LiteralConstant::Regex(regex) => units = units.saturating_add(regex.pattern.len()),
            LiteralConstant::List(values) | LiteralConstant::Tag { fields: values, .. } => pending.extend(values.iter()),
            LiteralConstant::Record(values) => pending.extend(values.values()),
            LiteralConstant::Map(values) => { units = units.saturating_add(values.keys().map(constant_key_size).sum::<usize>()); pending.extend(values.values()); },
            _ => {},
        }
        if units > 1_048_576 { return false; }
    }
    true
}

fn constant_runtime_target_names(arena: &AstArena, target: crate::syntax::arena::BindingTargetId) -> Vec<Name> {
    let mut names = Vec::new();
    let mut pending = vec![target];
    while let Some(target) = pending.pop() {
        match arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => names.push(name),
            ArenaBindingTargetKind::Record { fields, .. } => pending.extend(arena.destructure_fields(fields).iter().map(|field| field.target)),
        }
    }
    names
}

fn constant_pattern_names(arena: &AstArena, pattern: crate::syntax::arena::PatternId) -> Vec<Name> {
    use crate::syntax::arena::ArenaPatternKind as P;
    let mut names = Vec::new();
    let mut pending = vec![pattern];
    while let Some(pattern) = pending.pop() {
        match arena.pattern(pattern).kind {
            P::Binding(name) | P::TestName { name, .. } => names.push(name),
            P::Type { binding: Some(name), .. } => names.push(name),
            P::Record { fields, .. } | P::ErrorVariant { fields, .. } => pending.extend(arena.pattern_fields(fields).iter().map(|field| field.pattern)),
            P::List { elements, rest } => { pending.extend(arena.pattern_ids(elements)); pending.extend(rest); },
            P::Alternation(patterns) | P::Tuple(patterns) => pending.extend(arena.pattern_ids(patterns)),
            P::Constructor { arg, .. } => pending.extend(arg),
            _ => {},
        }
    }
    names
}

fn constant_values_equal(left: &LiteralConstant, right: &LiteralConstant) -> bool {
    use LiteralConstant as C;
    match (left, right) {
        (C::Float(a), C::Float(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        (C::Regex(a), C::Regex(b)) => a.pattern == b.pattern,
        (C::List(a), C::List(b)) => a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| constant_values_equal(a, b)),
        (C::Record(a), C::Record(b)) => a.len() == b.len() && a.iter().all(|(key, value)| b.get(key).is_some_and(|other| constant_values_equal(value, other))),
        (C::Map(a), C::Map(b)) => a.len() == b.len() && a.iter().all(|(key, value)| b.get(key).is_some_and(|other| constant_values_equal(value, other))),
        (C::Tag { family: a, variant: av, fields: af }, C::Tag { family: b, variant: bv, fields: bf }) => a == b && av == bv && af.len() == bf.len() && af.iter().zip(bf.iter()).all(|(a, b)| constant_values_equal(a, b)),
        (a, b) => a == b,
    }
}
