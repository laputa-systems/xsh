use super::{BTreeMap, Checker, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
    ErrorFamilyInfo, FxHashMap, Name, QualifiedName, Span, TagVariantInfo, Type};
use crate::sema::constants::{CheckedRecordConstructor, RecordConstructors, SchemaInstance};
use crate::sema::schemes::{SchemeBodyFacts, TypeScheme};
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

/// Identity of a user function selected while checking one call. The arena
/// ID identifies its declaration within its source arena; namespace and name
/// distinguish arena-local IDs when module arenas are combined.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CheckedFunctionTarget {
    pub(crate) definition: FunctionDefId,
    pub(crate) namespace: Option<Name>,
    pub(crate) name: Name,
}

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
    pub(crate) exported: bool,
    pub kind: CheckedFunctionKind,
    pub signature: Option<CompactFunctionSig>,
    pub(crate) scheme: Option<std::sync::Arc<TypeScheme>>,
    pub(super) symbol_owner: crate::symbol::SymbolOwner,
    pub(super) schema_identities: std::sync::Arc<BTreeMap<TypeDefId, super::callable_contract::SchemaIdentity>>,
    pub(crate) scheme_body_facts: Option<SchemeBodyFacts>,
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
            let CheckedFunctionDeclaration {
                name,
                namespace,
                internal,
                exported,
                kind,
                signature,
                scheme,
                symbol_owner: _,
                schema_identities: _,
                scheme_body_facts,
                body,
                parameter_spans,
            } = declaration;
            output.function_defs += 1;
            output.params += parameter_spans.len();
            if let Some(scheme_body_facts) = scheme_body_facts {
                assert!(
                    scheme.as_ref().is_some_and(|scheme| {
                        std::sync::Arc::ptr_eq(scheme, scheme_body_facts.scheme())
                    }),
                    "generic body facts retain their published scheme"
                );
                assert!(output.scheme_bodies.insert(body, scheme_body_facts).is_none());
            }
            if exported
                && let (Some(namespace), Some(scheme)) = (namespace, scheme.as_ref())
            {
                let _ = output
                    .function_schemes
                    .insert(QualifiedName::new(namespace, name), std::sync::Arc::clone(scheme));
            }
            let Some(signature) = signature else { continue; };
            match output.function_signatures.entry(body) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(signature.clone());
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    assert_eq!(entry.get(), &signature, "one function body has one checked signature");
                }
            }
            if let Some(namespace) = namespace {
                let name = QualifiedName::new(namespace, name);
                match kind {
                    CheckedFunctionKind::Proc => { output.qualified_procs.insert(name, signature.clone()); }
                    CheckedFunctionKind::Pure => { output.qualified_pures.insert(name, signature.clone()); }
                    CheckedFunctionKind::Stream => { output.qualified_streams.insert(name, signature.clone()); }
                    CheckedFunctionKind::CliMain => unreachable!("CLI entries do not publish ordinary signatures"),
                }
            }
            if internal || namespace.is_some() { continue; }
            match kind {
                CheckedFunctionKind::Proc => { output.procs.insert(name, signature); }
                CheckedFunctionKind::Pure => { output.pures.insert(name, signature); }
                CheckedFunctionKind::Stream => { output.streams.insert(name, signature); }
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
        kind: CheckedFunctionKind, exported: bool, signature: Option<&super::FunctionSig>,
        scheme_body_facts: Option<SchemeBodyFacts>) {
        if self.collecting_effects { return; }
        let def = program.arena.function_def(id);
        let scheme = signature.map(|sig| std::sync::Arc::clone(sig.scheme()));
        let schema_identities = scheme.as_ref().map(|scheme| self.callable_schema_identities(program, scheme)).unwrap_or_default();
        let signature = signature.and_then(|sig| {
            let params = sig.scheme().params().iter().map(|param| {
                Some(CallableParamType {
                    name: param.name,
                    ty: param.ty.closed_ground_type()?,
                    defaulted: param.defaulted,
                    rest: param.rest,
                })
            }).collect::<Option<Vec<_>>>()?;
            Some(CompactFunctionSig {
                parameter_schemas: sig.scheme().params().iter()
                    .map(|param| param.schema_expectation.clone()).collect(),
                params,
                return_ty: sig.scheme().return_type().closed_ground_type()?,
                return_type_expr: def.return_ty,
                inferred_effects: sig.scheme().inferred_effects(),
                effects: sig.scheme().effects().map(<[_]>::to_vec),
            })
        });
        self.checked_declarations.functions.insert(id, CheckedFunctionDeclaration {
            name: def.name, namespace: self.current_namespace,
            internal: self.current_namespace.is_some_and(|owner| program.is_internal_namespace(owner)),
            exported,
            kind, signature, scheme, symbol_owner: program.symbol_owner().clone(), schema_identities, scheme_body_facts,
            body: program.arena.span(program.arena.block(def.body).span),
            parameter_spans: program.arena.params(def.params).iter().map(|param| program.arena.span(param.span)).collect(),
        });
    }

    pub(super) fn take_checked_declarations(&mut self) -> CheckedDeclarations {
        let mut declarations = std::mem::take(&mut self.checked_declarations);
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
    use super::{CheckedDeclarations, CheckedFunctionDeclaration, CheckedFunctionKind};
    use crate::diagnostic::DiagnosticCode;
    use crate::sema::schemes::{
        SchemeBodyFacts, SchemeParameter, SchemeType,
        SchemeTypeParameterId, TypeScheme,
    };
    use crate::source::{SourceId, Span};
    use crate::symbol::{Name, QualifiedName, SymbolOwner};
    use crate::syntax::arena::{ExprId, FunctionDefId};
    use crate::syntax::parser::Parser;
    use std::collections::BTreeMap;
    use std::sync::Arc;

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

    #[test]
    fn inferred_ground_return_matches_published_signature() {
        use crate::syntax::arena::ArenaStmtKind;
        let source = include_str!("../../../tests/fixtures/frontend-indexed/local-inference.xsh");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() },
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let function = parsed.arena.statement_ids().find_map(|statement| {
                match parsed.arena.arena.stmt(statement).kind {
                    ArenaStmtKind::PureDef(function)
                        if parsed.arena.arena.function_def(function).name.as_str() == "gather" =>
                    {
                        Some(function)
                    }
                    _ => None,
                }
            }).expect("the inference fixture declares `gather`");
            let body_span = parsed.arena.arena.span(
                parsed.arena.arena.block(parsed.arena.arena.function_def(function).body).span,
            );
            let compact = Checker::compact_declarations(&parsed.arena, checked);
            let inferred_return = &compact.function_return_types[&body_span];
            assert_eq!(*inferred_return, Type::List(Box::new(Type::Path)));
            assert_eq!(compact.function_signatures[&body_span].return_ty, *inferred_return);
        });
    }

    #[test]
    fn annotated_stream_return_matches_published_signature() {
        use crate::syntax::arena::ArenaStmtKind;
        let source = "stream failing_rows() [error] -> Stream[Int] { yield 1; let ignored = \"late\".parse_int()? }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() },
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let function = parsed.arena.statement_ids().find_map(|statement| {
                match parsed.arena.arena.stmt(statement).kind {
                    ArenaStmtKind::StreamDef(function)
                        if parsed.arena.arena.function_def(function).name.as_str() == "failing_rows" =>
                    {
                        Some(function)
                    }
                    _ => None,
                }
            }).expect("the stream fixture declares `failing_rows`");
            let definition = parsed.arena.arena.function_def(function);
            let body_span = parsed.arena.arena.span(parsed.arena.arena.block(definition.body).span);
            let compact = Checker::compact_declarations(&parsed.arena, checked);
            let annotated_return = &compact.bodies.resolved_type_expressions.annotations[
                &(None, definition.return_ty)
            ];
            assert_eq!(*annotated_return, Type::Stream(Box::new(Type::Int)));
            assert_eq!(compact.function_signatures[&body_span].return_ty, *annotated_return);
        });
    }

    #[test]
    fn compound_alias_parameters_and_returns_publish_a_ground_signature() {
        use crate::syntax::arena::ArenaStmtKind;
        let source = "type PathInfo = {path: Path}\npure preserve(value: PathInfo) -> PathInfo { value }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() },
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let function = parsed.arena.statement_ids().find_map(|statement| {
                match parsed.arena.arena.stmt(statement).kind {
                    ArenaStmtKind::PureDef(function)
                        if parsed.arena.arena.function_def(function).name.as_str() == "preserve" =>
                    {
                        Some(function)
                    }
                    _ => None,
                }
            }).expect("the alias fixture declares `preserve`");
            let body_span = parsed.arena.arena.span(
                parsed.arena.arena.block(parsed.arena.arena.function_def(function).body).span,
            );
            let compact = Checker::compact_declarations(&parsed.arena, checked);
            let expected = Type::Record(Arc::new(BTreeMap::from([
                (crate::symbol::Name::intern("path"), Type::Path),
            ])));
            let signature = &compact.function_signatures[&body_span];
            assert_eq!(signature.params[0].ty, expected);
            assert_eq!(signature.return_ty, expected);
        });
    }

    #[test]
    fn exported_identity_publishes_one_shared_type_binder() {
        use crate::syntax::arena::ArenaStmtKind;
        let source = "export pure identity(value) { value }\npure helper(value: Int) -> Int { value }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() },
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let mut statements = parsed.arena.statement_ids();
        let ArenaStmtKind::Export(inner) = parsed.arena.arena.stmt(
            statements.next().unwrap(),
        ).kind else { panic!("export declaration"); };
        let ArenaStmtKind::PureDef(function) = parsed.arena.arena.stmt(inner).kind else {
            panic!("pure function declaration");
        };
        let declaration = &checked.checked_declarations.functions[&function];
        assert!(declaration.exported);
        let scheme = declaration.scheme.as_ref().expect("checked function scheme");
        assert_eq!(scheme.type_parameters().len(), 1);
        assert!(scheme.row_parameters().is_empty());
        let parameter = scheme.type_parameters()[0];
        assert!(matches!(scheme.params()[0].ty, SchemeType::Parameter(id) if id == parameter));
        assert!(matches!(scheme.return_type(), SchemeType::Parameter(id) if *id == parameter));
        let body_facts = declaration
            .scheme_body_facts
            .as_ref()
            .expect("generic body facts");
        assert!(std::sync::Arc::ptr_eq(scheme, body_facts.scheme()));

        let ArenaStmtKind::PureDef(private_function) = parsed
            .arena
            .arena
            .stmt(statements.next().unwrap())
            .kind
        else {
            panic!("private pure function declaration");
        };
        let private_declaration = &checked.checked_declarations.functions[&private_function];
        assert!(!private_declaration.exported);
    }

    #[test]
    fn qualified_scheme_index_copies_only_module_exports() {
        SymbolOwner::new().with_current(|| {
            let namespace = Name::intern("sample");
            let exported_name = Name::intern("identity");
            let private_name = Name::intern("helper");
            let type_parameter = SchemeTypeParameterId(0);
            let scheme = std::sync::Arc::new(
                TypeScheme::new(
                    crate::sema::types::CallableKind::Pure,
                    vec![type_parameter],
                    Vec::new(),
                    vec![SchemeParameter {
                        name: Name::intern("value"),
                        ty: SchemeType::Parameter(type_parameter),
                        schema_expectation: None,
                        defaulted: false,
                        rest: false,
                    }],
                    SchemeType::Parameter(type_parameter),
                    None,
                    true,
                    None,
                    false,
                )
                .unwrap(),
            );
            let declaration = |name, exported, index, scheme_body_facts| CheckedFunctionDeclaration {
                name,
                namespace: Some(namespace),
                internal: false,
                exported,
                kind: CheckedFunctionKind::Pure,
                signature: None,
                scheme: Some(std::sync::Arc::clone(&scheme)),
                symbol_owner: crate::symbol::SymbolOwner::current().unwrap(),
                schema_identities: Default::default(),
                scheme_body_facts,
                body: Span::at(SourceId::new(0), index),
                parameter_spans: Vec::new(),
            };
            let mut declarations = CheckedDeclarations::default();
            declarations.functions.insert(
                FunctionDefId::from_index(0),
                declaration(exported_name, true, 0, None),
            );
            let body_facts = SchemeBodyFacts::new(
                std::sync::Arc::clone(&scheme),
                std::collections::BTreeMap::from([(
                    ExprId::from_index(0),
                    SchemeType::Parameter(type_parameter),
                )]),
                std::collections::BTreeMap::new(),
            )
            .unwrap();
            declarations.functions.insert(
                FunctionDefId::from_index(1),
                declaration(private_name, false, 1, Some(body_facts)),
            );

            let compact = declarations.into_compact();
            let exported = QualifiedName::new(namespace, exported_name);
            let private = QualifiedName::new(namespace, private_name);
            assert_eq!(compact.function_schemes.len(), 1);
            assert!(std::sync::Arc::ptr_eq(&compact.function_schemes[&exported], &scheme));
            assert!(!compact.function_schemes.contains_key(&private));
            assert_eq!(compact.scheme_bodies.len(), 1);
            assert!(std::sync::Arc::ptr_eq(
                compact.scheme_bodies[&Span::at(SourceId::new(0), 1)].scheme(),
                &scheme,
            ));
        });
    }
}
