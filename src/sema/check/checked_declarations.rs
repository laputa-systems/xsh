use super::{BTreeMap, Checker, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
    ErrorFamilyInfo, FxHashMap, Name, QualifiedName, Span, TagVariantInfo, Type};
use crate::sema::constants::{CheckedRecordConstructor, RecordConstructors, SchemaInstance};
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType};
use crate::syntax::arena::{ArenaModuleContractEntryKind, ArenaProgram, ArenaTypeDefBody,
    ErrorDefId, FunctionDefId, TypeDefId, TypeExprId};

/// Type expressions retain their checking environment. A generic template is
/// never replaced by the concrete field type of its latest construction.
#[derive(Clone, Debug)]
pub struct ResolvedTypeExpressions<Site = Span> {
    pub annotations: FxHashMap<(Option<Name>, TypeExprId), Type>,
    pub templates: FxHashMap<(TypeDefId, TypeExprId), Type>,
    pub instantiations: FxHashMap<Site, InstantiatedTypeExpressions>,
}

impl<Site> Default for ResolvedTypeExpressions<Site> {
    fn default() -> Self {
        Self { annotations: FxHashMap::default(), templates: FxHashMap::default(), instantiations: FxHashMap::default() }
    }
}

#[derive(Clone, Debug)]
pub struct InstantiatedTypeExpressions {
    pub instance: SchemaInstance,
    pub expressions: FxHashMap<TypeExprId, Type>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedFunctionKind { Proc, Pure, Stream, CliMain }

#[derive(Clone, Debug)]
pub struct CheckedTypeDeclaration {
    pub name: Name,
    pub namespace: Option<Name>,
    pub internal: bool,
    /// An early declaration error leaves no resolved body to publish.
    pub info: Option<CompactTypeDefInfo>,
    pub schema_fields: BTreeMap<Name, TypeExprId>,
    pub variants: Vec<(Name, TagVariantInfo)>,
    pub schema_field_count: usize,
    pub tag_variant_count: usize,
    pub module_entry_count: usize,
    pub parameter_count: usize,
}

#[derive(Clone, Debug)]
pub struct CheckedErrorDeclaration {
    pub name: Name,
    pub namespace: Option<Name>,
    pub family: ErrorFamilyInfo,
    pub variant_count: usize,
    pub field_count: usize,
}

#[derive(Clone, Debug)]
pub struct CheckedFunctionDeclaration {
    pub name: Name,
    pub namespace: Option<Name>,
    pub internal: bool,
    pub kind: CheckedFunctionKind,
    pub signature: Option<CompactFunctionSig>,
    pub body: Span,
    pub parameter_spans: Vec<Span>,
}

/// Declaration data selected by the checker, before execution preparation.
/// Projection into the compact tables neither resolves types nor emits errors.
#[derive(Clone, Debug, Default)]
pub struct CheckedDeclarations {
    pub types: BTreeMap<TypeDefId, CheckedTypeDeclaration>,
    pub errors: BTreeMap<ErrorDefId, CheckedErrorDeclaration>,
    pub functions: BTreeMap<FunctionDefId, CheckedFunctionDeclaration>,
    pub record_constructors: RecordConstructors,
    pub wire_enums: crate::sema::wire_enums::PreparedWireEnums,
    pub builtin_error_families: FxHashMap<Name, ErrorFamilyInfo>,
    pub(crate) cli_entries: Vec<crate::sema::cli_entry::CliEntryPlan>,
}

impl CheckedDeclarations {
    pub(super) fn into_compact(self) -> CompactDeclOutput {
        let mut output = CompactDeclOutput {
            record_constructors: self.record_constructors,
            wire_enums: self.wire_enums,
            cli_entries: self.cli_entries,
            error_families_by_name: self.builtin_error_families,
            ..CompactDeclOutput::default()
        };
        for (_, declaration) in self.types {
            output.type_defs += 1;
            output.schema_fields += declaration.schema_field_count;
            output.module_contract_entries += declaration.module_entry_count;
            output.params += declaration.parameter_count;
            if !declaration.internal {
                if let Some(info) = declaration.info { output.types.insert(declaration.name, info); }
                if !declaration.schema_fields.is_empty() {
                    output.record_schema_fields.insert(declaration.name, declaration.schema_fields);
                }
            }
            output.tag_variants += declaration.tag_variant_count;
            for (name, variant) in declaration.variants {
                if let Some(namespace) = declaration.namespace {
                    output.qualified_tag_variants.insert(QualifiedName::new(namespace, name), variant);
                } else { output.tag_variants_by_name.insert(name, variant); }
            }
        }
        for (_, declaration) in self.errors {
            output.error_families += 1;
            output.error_variants += declaration.variant_count;
            output.error_fields += declaration.field_count;
            if let Some(namespace) = declaration.namespace {
                output.qualified_error_families.insert(QualifiedName::new(namespace, declaration.name), declaration.family.clone());
            }
            output.error_families_by_name.insert(declaration.name, declaration.family);
        }
        for (_, declaration) in self.functions {
            output.function_defs += 1;
            output.params += declaration.parameter_spans.len();
            let Some(signature) = declaration.signature else { continue; };
            match output.function_signatures.entry(declaration.body) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(signature.clone());
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    assert_eq!(entry.get(), &signature, "one function body has one checked signature");
                }
            }
            if let Some(namespace) = declaration.namespace {
                let name = QualifiedName::new(namespace, declaration.name);
                match declaration.kind {
                    CheckedFunctionKind::Proc => { output.qualified_procs.insert(name, signature.clone()); }
                    CheckedFunctionKind::Pure => { output.qualified_pures.insert(name, signature.clone()); }
                    CheckedFunctionKind::Stream => { output.qualified_streams.insert(name, signature.clone()); }
                    CheckedFunctionKind::CliMain => unreachable!("CLI entries do not publish ordinary signatures"),
                }
            }
            if declaration.internal { continue; }
            match declaration.kind {
                CheckedFunctionKind::Proc => { output.procs.insert(declaration.name, signature); }
                CheckedFunctionKind::Pure => { output.pures.insert(declaration.name, signature); }
                CheckedFunctionKind::Stream => { output.streams.insert(declaration.name, signature); }
                CheckedFunctionKind::CliMain => unreachable!("CLI entries do not publish ordinary signatures"),
            }
        }
        output
    }
}

impl Checker {
    pub(super) fn publish_type_declaration(&mut self, program: &ArenaProgram, id: TypeDefId, checked: bool) {
        if self.collecting_effects { return; }
        let def = program.arena.type_def(id);
        let namespace = self.current_namespace;
        let mut declaration = CheckedTypeDeclaration {
            name: def.name, namespace,
            internal: namespace.is_some_and(|owner| program.is_internal_namespace(owner)),
            info: None, schema_fields: BTreeMap::new(), variants: Vec::new(),
            schema_field_count: 0, tag_variant_count: 0, module_entry_count: 0, parameter_count: 0,
        };
        match def.body {
            ArenaTypeDefBody::RecordSchema(fields) => declaration.schema_field_count = fields.len(),
            ArenaTypeDefBody::TagUnion(variants) => declaration.tag_variant_count = variants.len(),
            ArenaTypeDefBody::ModuleContract { entries, .. } => {
                declaration.module_entry_count = entries.len();
                declaration.parameter_count = program.arena.module_contract_entries(entries).iter().map(|entry| match entry.kind {
                    ArenaModuleContractEntryKind::Proc { params, .. } | ArenaModuleContractEntryKind::Pure { params, .. } => params.len(),
                    ArenaModuleContractEntryKind::Value(_) => 0,
                }).sum();
            }
            ArenaTypeDefBody::Alias(_) => {}
        }
        if !checked { self.checked_declarations.types.insert(id, declaration); return; }
        declaration.info = match def.body {
            ArenaTypeDefBody::Alias(ty) => Some(match def.bounds
                .and_then(|bounds| program.arena.int_bounds_values(bounds))
                .and_then(|(low, high)| crate::sema::validated::IntRange::new(low, high)) {
                Some(range) => CompactTypeDefInfo::Bounded(ty, range),
                None => CompactTypeDefInfo::Alias(ty),
            }),
            ArenaTypeDefBody::RecordSchema(fields) => {
                let fields = program.arena.schema_fields(fields);
                declaration.schema_fields = fields.iter().map(|field| (field.name, field.ty)).collect();
                fields.iter().map(|field| {
                    let ty = if def.type_parameters.is_empty() {
                        self.resolved_type_expressions.annotations.get(&(namespace, field.ty))
                    } else { self.resolved_type_expressions.templates.get(&(id, field.ty)) };
                    Some((field.name, ty?.clone()))
                }).collect::<Option<BTreeMap<_, _>>>().map(CompactTypeDefInfo::Record)
            }
            ArenaTypeDefBody::ModuleContract { entries, exact } => {
                let entries = program.arena.module_contract_entries(entries);
                let exports = entries.iter().map(|entry| {
                    let value = match entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => ModuleExportType::Value {
                            ty: self.resolved_type_expressions.annotations.get(&(namespace, ty))?.clone(),
                            optional: entry.optional,
                        },
                        ArenaModuleContractEntryKind::Proc { params, return_ty, effects } => {
                            ModuleExportType::Proc {
                                sig: self.checked_contract_callable(program, params, return_ty, effects)?,
                                optional: entry.optional,
                            }
                        }
                        ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            ModuleExportType::Pure {
                                sig: self.checked_contract_callable(program, params, return_ty, None)?,
                                optional: entry.optional,
                            }
                        }
                    };
                    Some((entry.name, value))
                }).collect::<Option<BTreeMap<_, _>>>();
                exports.map(|exports| CompactTypeDefInfo::Module(std::sync::Arc::new(crate::sema::types::ModuleType { exports, exact })))
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                declaration.variants = program.arena.tag_variants(variants).iter().map(|variant|
                    (variant.name, self.tag_variants[&variant.name].clone())).collect();
                Some(CompactTypeDefInfo::TagUnion)
            }
        };
        self.checked_declarations.types.insert(id, declaration);
    }

    fn checked_contract_callable(&self, program: &ArenaProgram, params: crate::syntax::arena::ArenaRange,
        return_ty: TypeExprId, effects: Option<crate::syntax::arena::ArenaRange>) -> Option<CallableType> {
        Some(CallableType {
            params: program.arena.params(params).iter().map(|param| Some(CallableParamType {
                name: param.name, ty: self.parameter_types.get(&program.arena.span(param.span))?.clone(),
                defaulted: param.default.is_some(), rest: param.rest,
            })).collect::<Option<Vec<_>>>()?,
            return_ty: Box::new(self.resolved_type_expressions.annotations.get(&(self.current_namespace, return_ty))?.clone()),
            effects: effects.map(|effects| program.arena.effects(effects).collect()),
        })
    }

    pub(super) fn publish_error_declaration(&mut self, program: &ArenaProgram, id: ErrorDefId) {
        if self.collecting_effects { return; }
        let def = program.arena.error_def(id);
        let variants = program.arena.error_variants(def.variants);
        self.checked_declarations.errors.insert(id, CheckedErrorDeclaration {
            name: def.name, namespace: self.current_namespace,
            family: self.error_families[&def.name].clone(), variant_count: variants.len(),
            field_count: variants.iter().map(|variant| program.arena.error_fields(variant.fields).len()).sum(),
        });
    }

    pub(super) fn publish_function_declaration(&mut self, program: &ArenaProgram, id: FunctionDefId,
        kind: CheckedFunctionKind, signature: Option<&super::FunctionSig>) {
        if self.collecting_effects { return; }
        let def = program.arena.function_def(id);
        let signature = signature.map(|sig| CompactFunctionSig {
            params: sig.params.iter().map(|param| CallableParamType { name: param.name, ty: param.ty.clone(),
                defaulted: param.defaulted, rest: param.rest }).collect(),
            parameter_schemas: sig.params.iter().map(|param| param.schema_expectation.clone()).collect(),
            return_ty: sig.return_ty.clone(), return_type_expr: def.return_ty,
            inferred_effects: sig.inferred_effects, effects: sig.effects.clone(),
        });
        self.checked_declarations.functions.insert(id, CheckedFunctionDeclaration {
            name: def.name, namespace: self.current_namespace,
            internal: self.current_namespace.is_some_and(|owner| program.is_internal_namespace(owner)),
            kind, signature, body: program.arena.span(program.arena.block(def.body).span),
            parameter_spans: program.arena.params(def.params).iter().map(|param| program.arena.span(param.span)).collect(),
        });
    }

    pub(super) fn take_checked_declarations(&mut self) -> CheckedDeclarations {
        let mut declarations = std::mem::take(&mut self.checked_declarations);
        for declaration in declarations.functions.values_mut() {
            if let Some(signature) = &mut declaration.signature {
                for (param, span) in signature.params.iter_mut().zip(&declaration.parameter_spans) {
                    if let Some(ty) = self.parameter_types.get(span) { param.ty = ty.clone(); }
                }
                if declaration.kind != CheckedFunctionKind::Stream {
                    if let Some(ty) = self.function_return_types.get(&declaration.body) { signature.return_ty = ty.clone(); }
                }
            }
        }
        declarations.record_constructors = std::mem::take(&mut self.record_constructors);
        declarations.wire_enums = std::mem::take(&mut self.wire_enums);
        declarations
    }

    pub(super) fn publish_constructor_type_expressions(&mut self, program: &ArenaProgram,
        span: Span, fact: &CheckedRecordConstructor) {
        let def = program.arena.type_def(fact.instance.definition);
        let mut expressions = FxHashMap::default();
        match def.body {
            ArenaTypeDefBody::RecordSchema(fields) => {
                if let Type::Record(types) = fact.ty.unvalidated() {
                    for field in program.arena.schema_fields(fields) {
                        if let Some(ty) = types.get(&field.name) { expressions.insert(field.ty, ty.clone()); }
                    }
                }
            }
            ArenaTypeDefBody::Alias(ty) => { expressions.insert(ty, fact.ty.clone()); }
            ArenaTypeDefBody::ModuleContract { .. } | ArenaTypeDefBody::TagUnion(_) => {}
        }
        self.resolved_type_expressions.instantiations.insert(span, InstantiatedTypeExpressions {
            instance: fact.instance.clone(), expressions,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CheckOptions, Checker, CompactTypeDefInfo, Type};
    use crate::diagnostic::DiagnosticCode;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    // Native commands stop after a check error; this boundary exercises the
    // compact API with that same checked output, before any lowering runs.
    #[test]
    fn compact_declarations_preserve_one_checker_duplicate_diagnostic() {
        let source = "proc repeated() {}\nproc repeated() {}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty());
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        let before = checked.diagnostics.iter().filter(|diagnostic|
            diagnostic.code == Some(DiagnosticCode::CheckDuplicateName)).count();
        assert_eq!(before, 1);
        let compact = Checker::compact_declarations(&parsed.arena, checked);
        assert_eq!(compact.diagnostics.iter().filter(|diagnostic|
            diagnostic.code == Some(DiagnosticCode::CheckDuplicateName)).count(), before);
    }

    // A template type expression has several instantiation contexts; native
    // execution cannot inspect which arena identity each fact retains.
    #[test]
    fn generic_type_expression_facts_keep_both_constructions_and_the_template() {
        use crate::syntax::arena::{ArenaStmtKind, ArenaTypeDefBody};
        let source = "type Box[T] = {value: T}\nlet first: Box[Int] = Box(value: 7)\nlet second: Box[Str] = Box(value: \"seven\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let ArenaStmtKind::TypeDef(definition) = parsed.arena.arena.stmt(parsed.arena.statement_ids().next().unwrap()).kind else { panic!("schema declaration"); };
            let ArenaTypeDefBody::RecordSchema(fields) = parsed.arena.arena.type_def(definition).body else { panic!("record schema"); };
            let field = &parsed.arena.arena.schema_fields(fields)[0];
            let template = checked.resolved_type_expressions.templates[&(definition, field.ty)].clone();
            assert!(matches!(template, Type::Tag(_)));
            assert!(!checked.resolved_type_expressions.annotations.contains_key(&(None, field.ty)));
            let fields: Vec<_> = checked.resolved_type_expressions.instantiations.values()
                .map(|instance| instance.expressions[&field.ty].clone()).collect();
            assert_eq!(fields.len(), 2);
            assert!(fields.contains(&Type::Int));
            assert!(fields.contains(&Type::Str));
            let compact = Checker::compact_declarations(&parsed.arena, checked);
            assert_eq!(compact.bodies.resolved_type_expressions.instantiations.len(), 2);
            assert_eq!(compact.bodies.resolved_type_expressions.templates[&(definition, field.ty)], template);
        });
    }

    // Declaration metadata is an in-process contract and has no XSH surface.
    #[test]
    fn compact_schema_fields_keep_the_checker_resolved_alias_type() {
        let source = "type Count = Int\ntype Config = {count: Count}\nlet config = Config(count: 1)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty());
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() },
        );
        let compact = Checker::compact_declarations(&parsed.arena, checked);
        assert!(compact.diagnostics.is_empty(), "{:?}", compact.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let CompactTypeDefInfo::Record(fields) = &compact.types[&crate::symbol::Name::intern("Config")] else {
                panic!("checked schema did not publish record fields");
            };
            assert_eq!(fields[&crate::symbol::Name::intern("count")], Type::Int);
        });
    }

    #[test]
    fn checked_cli_entry_survives_compact_projection() {
        use crate::syntax::arena::ArenaStmtKind;
        let source = "cli main() [] {}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty());
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() },
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let ArenaStmtKind::CliMain(function) = parsed.arena.arena.stmt(
            parsed.arena.statement_ids().next().unwrap(),
        ).kind else { panic!("CLI main declaration"); };
        assert_eq!(
            checked.checked_declarations.functions[&function].kind,
            super::CheckedFunctionKind::CliMain,
        );
        assert!(checked.checked_declarations.functions[&function].signature.is_none());
        assert_eq!(checked.checked_declarations.cli_entries.len(), 1);
        let compact = Checker::compact_declarations(&parsed.arena, checked);
        assert_eq!(compact.cli_entries.len(), 1);
    }
}
