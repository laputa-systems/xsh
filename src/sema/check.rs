#![allow(clippy::single_call_fn)]

use crate::diagnostic::Severity;
pub(crate) use crate::diagnostic::{Diagnostic, FixHint, Label};
pub(crate) use crate::modules::{
    ApiArgCheck, MethodReceiver, MethodSig, ModuleFnSig, RuntimeOp, api_spec,
};
use crate::runtime::signal::{normalize_hook_signal, signal_rejection_message};
pub(crate) use crate::sema::records::standard_record_type;
pub(crate) use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
pub(crate) use crate::source::Span;
pub(crate) use crate::symbol::{Name, QualifiedName};
use crate::syntax::arena::{ArenaProgram, TypeExprId};
pub(crate) use crate::syntax::node::{BinaryOp, CoreCommand, Effect, RunKind, UnaryOp};

pub(crate) use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::sync::Arc;

#[path = "check/args.rs"]
mod args;
#[path = "check/builder.rs"]
mod builder;
#[path = "check/call.rs"]
mod call;
#[path = "check/command.rs"]
mod command;
#[path = "check/compact.rs"]
mod compact;
#[path = "check/decl.rs"]
mod decl;
#[path = "check/expr.rs"]
mod expr;
#[path = "check/method.rs"]
mod method;
#[path = "check/pattern.rs"]
mod pattern;
#[path = "check/stmt.rs"]
mod stmt;
#[path = "check/stream.rs"]
mod stream;
#[path = "check/types.rs"]
mod types;

use self::args::{
    call_arg_expr_id_arena, call_arg_span_arena, common_module_overload_expected_arena,
    module_overload_matches_arena, module_sig_accepts_arg_name_at_arena, module_sig_accepts_arity,
    module_sig_accepts_names_arena,
};
use self::command::{
    command_arg_can_be_path_like_arena, command_bool_flag_name_arena,
    command_stmt_asserts_success_arena, command_ty_auto_propagates,
};
pub use self::compact::{
    CompactBodyProbeOutput, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
};
use self::expr::expr_ty_auto_propagates;
use self::stmt::block_has_exit_point_arena;
use self::types::{
    collection_item_ty, map_item_ty, merge_collection_item_ty, result_types,
    tail_type_matches_expected,
};

#[derive(Clone, Debug, Default)]
pub struct CheckOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub annotation_facts: Vec<AnnotationFact>,
    pub reveal_types: Vec<Diagnostic>,
    pub expr_types: BTreeMap<Span, Type>,
    pub callable_effects: FxHashMap<String, Option<Vec<Effect>>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CheckOptions {
    pub interactive_commands: Option<fn(&str) -> bool>,
    pub strict_dynamic: bool,
    pub reveal_types: bool,
    pub migration_diagnostics: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnotationFact {
    pub kind: AnnotationFactKind,
    pub ty: Type,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnnotationFactKind {
    Binding {
        span: Span,
        initializer: Span,
        exported: bool,
    },
    DefaultedParam {
        span: Span,
        default: Span,
    },
    ExportedProcReturn {
        body: Span,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Binding {
    ty: Type,
    mutable: bool,
    pure_local_mutation: bool,
}

impl Binding {
    fn new(ty: Type, mutable: bool) -> Self {
        Self {
            ty,
            mutable,
            pure_local_mutation: false,
        }
    }

    fn pure_local_var(ty: Type) -> Self {
        Self {
            ty,
            mutable: true,
            pure_local_mutation: true,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FunctionSig {
    params: Vec<FunctionParamSig>,
    return_ty: Type,
    effects: Option<Vec<Effect>>,
}

#[derive(Clone, Debug)]
pub(super) struct FunctionParamSig {
    name: Name,
    ty: Type,
    defaulted: bool,
    rest: bool,
}

#[derive(Clone, Debug)]
pub struct TagVariantInfo {
    pub type_name: Name,
    pub field_count: usize,
    pub field_types: Vec<Type>,
}

#[derive(Clone, Debug)]
pub(super) struct TypeAnnRef {
    program: Arc<ArenaProgram>,
    id: TypeExprId,
}

impl TypeAnnRef {
    pub(super) fn new(program: Arc<ArenaProgram>, id: TypeExprId) -> Self {
        Self { program, id }
    }
}

#[derive(Clone, Debug)]
pub(super) enum TypeDefBody {
    Alias(TypeAnnRef),
    RecordSchema(Vec<SchemaField>),
    ModuleContract(Vec<ModuleContractEntry>),
    TagUnion(Vec<TagVariant>),
}

#[derive(Clone, Debug)]
pub(super) struct SchemaField {
    name: Name,
    ty: TypeAnnRef,
}

#[derive(Clone, Debug)]
pub(super) struct ModuleContractEntry {
    name: Name,
    optional: bool,
    kind: ModuleContractEntryKind,
}

#[derive(Clone, Debug)]
pub(super) enum ModuleContractEntryKind {
    Value(TypeAnnRef),
    Proc {
        params: Vec<ContractParam>,
        effects: Option<Vec<Effect>>,
        return_ty: TypeAnnRef,
    },
    Pure {
        params: Vec<ContractParam>,
        return_ty: TypeAnnRef,
    },
}

#[derive(Clone, Debug)]
pub(super) struct ContractParam {
    name: Name,
    ty: TypeAnnRef,
    defaulted: bool,
    rest: bool,
}

#[derive(Clone, Debug)]
pub(super) struct TagVariant {
    name: Name,
    fields: Vec<TypeAnnRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorFamilyInfo {
    pub variants: BTreeMap<Name, ErrorVariantInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorVariantInfo {
    pub fields: BTreeMap<Name, Type>,
    pub facets: Vec<Name>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuilderKind {
    ProcessCommand,
}

#[derive(Clone, Debug, Default)]
pub(super) struct UserModuleSig {
    values: BTreeMap<Name, Type>,
    procs: FxHashMap<Name, FunctionSig>,
    pures: FxHashMap<Name, FunctionSig>,
    streams: FxHashMap<Name, FunctionSig>,
    types: BTreeMap<Name, TypeDefBody>,
    error_families: BTreeMap<Name, ErrorFamilyInfo>,
}

pub struct Checker {
    scopes: Vec<FxHashMap<Name, Binding>>,
    procs: FxHashMap<Name, FunctionSig>,
    pures: FxHashMap<Name, FunctionSig>,
    streams: FxHashMap<Name, FunctionSig>,
    qualified_procs: FxHashMap<QualifiedName, FunctionSig>,
    qualified_pures: FxHashMap<QualifiedName, FunctionSig>,
    qualified_streams: FxHashMap<QualifiedName, FunctionSig>,
    type_defs: FxHashMap<Name, TypeDefBody>,
    type_namespaces: FxHashMap<Name, BTreeMap<Name, TypeDefBody>>,
    tag_variants: FxHashMap<Name, TagVariantInfo>,
    error_families: FxHashMap<Name, ErrorFamilyInfo>,
    error_facets: FxHashSet<Name>,
    resolving_types: Vec<Name>,
    user_modules: FxHashMap<String, UserModuleSig>,
    diagnostics: Vec<Diagnostic>,
    annotation_facts: Vec<AnnotationFact>,
    reveal_types: Vec<Diagnostic>,
    expr_types: BTreeMap<Span, Type>,
    options: CheckOptions,
    current_return: Option<Type>,
    current_yield: Option<Type>,
    in_pure: bool,
    current_effects: Option<Vec<Effect>>,
    last_status_available: bool,
    stream_item_types: Vec<Type>,
    loop_depth: usize,
    block_depth: usize,
    retry_attempt_depth: usize,
    module_depth: usize,
    in_signal_hook: bool,
    root_signal_hooks: FxHashMap<Name, Span>,
    current_exported: bool,
}

impl Checker {
    pub fn check_arena(program: &crate::syntax::arena::ArenaProgram, source: &str) -> CheckOutput {
        Self::check_arena_with_options(program, source, CheckOptions::default())
    }

    pub fn check_arena_with_options(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
        options: CheckOptions,
    ) -> CheckOutput {
        let mut checker = Self::new(options);
        checker.check_program_arena(program, source);
        let callable_effects = checker.callable_effects();
        CheckOutput {
            diagnostics: checker.diagnostics,
            annotation_facts: checker.annotation_facts,
            reveal_types: checker.reveal_types,
            expr_types: checker.expr_types,
            callable_effects,
        }
    }

    pub fn check_arena_interactive(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
    ) -> CheckOutput {
        Self::check_arena_interactive_with_commands(program, source, |_| false)
    }

    pub fn check_arena_interactive_with_commands(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
        interactive_commands: fn(&str) -> bool,
    ) -> CheckOutput {
        Self::check_arena_with_options(
            program,
            source,
            CheckOptions {
                interactive_commands: Some(interactive_commands),
                strict_dynamic: false,
                reveal_types: false,
                migration_diagnostics: false,
            },
        )
    }

    /// Check a multi-module program assembled from separately parsed arenas.
    ///
    /// `main` is the entry arena+source; each `(key, name, arena, source)` in
    /// `modules` is checked as a user module and the matching `use` statements
    /// in a cloned main arena have their `resolved` field set to `key`. This
    /// mirrors the module wiring used by the loader.
    pub fn check_arena_with_modules(
        main: (&crate::syntax::arena::ArenaProgram, &str),
        modules: &[(&str, &str, &crate::syntax::arena::ArenaProgram, &str)],
    ) -> CheckOutput {
        let mut checker = Self::new(CheckOptions::default());
        for (key, name, arena, source) in modules {
            let module_program = Arc::new((*arena).clone());
            let module = crate::syntax::arena::ArenaUserModule {
                key: (*key).to_string(),
                name: Name::intern(name),
                statements: arena.statements,
            };
            let sig = checker.check_user_module_arena(arena, module_program, source, &module);
            checker.user_modules.insert((*key).to_string(), sig);
        }

        let mut main_program = main.0.clone();
        if let Some((key, ..)) = modules.first() {
            let resolved = std::sync::Arc::<str>::from(*key);
            let use_ids = main_program
                .statement_ids()
                .filter_map(|stmt_id| {
                    let stmt = main_program.arena.stmt(stmt_id);
                    match stmt.kind {
                        crate::syntax::arena::ArenaStmtKind::Use(use_id) => Some(use_id),
                        _ => None,
                    }
                })
                .collect::<Vec<_>>();
            for use_id in use_ids {
                main_program.arena.use_stmts[use_id.index()].resolved = Some(resolved.clone());
            }
        }

        checker.check_program_arena(&main_program, main.1);
        let callable_effects = checker.callable_effects();
        CheckOutput {
            diagnostics: checker.diagnostics,
            annotation_facts: checker.annotation_facts,
            reveal_types: checker.reveal_types,
            expr_types: checker.expr_types,
            callable_effects,
        }
    }

    pub(crate) fn new(options: CheckOptions) -> Self {
        let mut checker = Self {
            scopes: vec![FxHashMap::default()],
            procs: FxHashMap::default(),
            pures: FxHashMap::default(),
            streams: FxHashMap::default(),
            qualified_procs: FxHashMap::default(),
            qualified_pures: FxHashMap::default(),
            qualified_streams: FxHashMap::default(),
            type_defs: FxHashMap::default(),
            type_namespaces: FxHashMap::default(),
            tag_variants: FxHashMap::default(),
            error_families: FxHashMap::default(),
            error_facets: FxHashSet::default(),
            resolving_types: Vec::new(),
            user_modules: FxHashMap::default(),
            diagnostics: Vec::new(),
            annotation_facts: Vec::new(),
            reveal_types: Vec::new(),
            expr_types: BTreeMap::new(),
            options,
            current_return: None,
            current_yield: None,
            in_pure: false,
            current_effects: None,
            last_status_available: false,
            stream_item_types: Vec::new(),
            loop_depth: 0,
            block_depth: 0,
            retry_attempt_depth: 0,
            module_depth: 0,
            in_signal_hook: false,
            root_signal_hooks: FxHashMap::default(),
            current_exported: false,
        };
        checker.register_builtin_process_error_family();
        checker.define_standard_values();
        checker
    }

    fn register_builtin_process_error_family(&mut self) {
        for family in xsh_registry::errors::builtin_error_families() {
            let fields = family
                .fields
                .iter()
                .map(|field| {
                    (
                        Name::intern(field.name),
                        crate::modules::signature::convert_type(&field.ty),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let mut variants = BTreeMap::new();
            for variant in family.variants {
                for facet in variant.facets {
                    self.error_facets.insert(Name::intern(facet));
                }
                variants.insert(
                    Name::intern(variant.name),
                    ErrorVariantInfo {
                        fields: fields.clone(),
                        facets: variant.facets.iter().map(Name::intern).collect(),
                    },
                );
            }
            let family_name = if family.name == "ProcessError" {
                Name::PROCESS_ERROR
            } else {
                Name::intern(family.name)
            };
            self.error_families
                .insert(family_name, ErrorFamilyInfo { variants });
        }
    }

    pub(crate) fn define_standard_values(&mut self) {
        self.define_builtin_value("args", Binding::new(Type::List(Box::new(Type::Str)), false));
        self.define_builtin_value("ARGV", Binding::new(Type::List(Box::new(Type::Str)), false));
    }

    pub(crate) fn check_program_arena(
        &mut self,
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
    ) {
        let type_program = Arc::new(program.clone());
        self.collect_user_modules_arena(program, type_program.clone(), source);
        self.collect_type_imports_arena(program, program.statement_ids());
        self.collect_definitions_arena(program, type_program, source, program.statement_ids());
        for stmt in program.statement_ids() {
            self.check_stmt_arena(program, source, stmt);
        }
    }

    fn callable_effects(&self) -> FxHashMap<String, Option<Vec<Effect>>> {
        let mut effects = FxHashMap::default();
        for (name, sig) in &self.procs {
            effects.insert(name.to_string(), sig.effects.clone());
        }
        for (name, sig) in &self.streams {
            effects.insert(name.to_string(), sig.effects.clone());
        }
        for (name, sig) in &self.qualified_procs {
            effects.insert(name.to_string(), sig.effects.clone());
        }
        for (name, sig) in &self.qualified_streams {
            effects.insert(name.to_string(), sig.effects.clone());
        }
        effects
    }

    pub(crate) fn effects_covers(caller: &[Effect], required: &Effect) -> bool {
        if caller.contains(required) {
            return true;
        }
        // io subsumes fs, net, process, env but not time or error
        caller.contains(&Effect::Io)
            && matches!(
                required,
                Effect::Fs | Effect::Net | Effect::Process | Effect::Env
            )
    }

    pub(crate) fn check_callee_effects(
        &mut self,
        caller_effs: &[Effect],
        callee_effects: &Option<Vec<Effect>>,
        callee_name: &str,
        span: Span,
    ) {
        match callee_effects {
            None => {
                self.error(
                    span,
                    &format!("proc `{callee_name}` is unrestricted — cannot call from a proc with declared effects"),
                    "check.effect-violation",
                );
            }
            Some(callee_effs) => {
                for eff in callee_effs {
                    if !Self::effects_covers(caller_effs, eff) {
                        self.error(
                            span,
                            &format!(
                                "effect `{}` required by `{callee_name}` is not in caller's declared effects",
                                eff.as_str()
                            ),
                            "check.effect-violation",
                        );
                    }
                }
            }
        }
    }

    fn lookup(&self, name: Name) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(&name))
    }

    fn define(&mut self, name: Name, binding: Binding, span: Span) {
        self.check_standard_module_shadow(name.as_str(), span);
        self.current_scope_mut().insert(name, binding);
    }

    fn define_builtin_value(&mut self, name: &str, binding: Binding) {
        self.current_scope_mut().insert(Name::intern(name), binding);
    }

    pub(crate) fn check_standard_module_shadow(&mut self, name: &str, span: Span) {
        if name == "args" {
            return;
        }
        if api_spec().is_standard_module(name) {
            let message = format!("name `{name}` shadows the standard module `{name}`");
            self.error(span, &message, "check.standard-module-shadow");
        }
    }

    pub(crate) fn reveal_type(&mut self, ty: &Type, span: Span) {
        let message = format!("revealed type: {ty}");
        self.reveal_types.push(
            Diagnostic::new(Severity::Note, message)
                .with_code("check.reveal-type")
                .with_label(Label::primary(span, "expression has this type")),
        );
    }

    pub(crate) fn check_builtin_args_shadow(&mut self, name: &str, span: Span) {
        if name == "args" {
            self.error(
                span,
                "name `args` shadows the built-in script-arguments binding",
                "check.standard-module-shadow",
            );
        }
    }

    fn current_scope(&self) -> &FxHashMap<Name, Binding> {
        self.scopes.last().expect("checker always has a scope")
    }

    fn current_scope_mut(&mut self) -> &mut FxHashMap<Name, Binding> {
        self.scopes.last_mut().expect("checker always has a scope")
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(FxHashMap::default());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(crate) fn error(&mut self, span: Span, message: &str, code: &str) {
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }

    pub(crate) fn warning(&mut self, span: Span, message: &str, code: &str) {
        self.diagnostics.push(
            Diagnostic::warning(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }
}
