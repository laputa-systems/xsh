use crate::sema::types::{ModuleExportType, Type};
use crate::symbol::Name;
use crate::syntax::node::Effect;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use xsh_registry::signature as registry;

pub use registry::{
    ApiArgCheck, ApiDocs, ImplBinding, LabelRule, MethodReceiver, ScriptImpl, SemanticRule,
};
pub use xsh_registry::RuntimeOp;

#[derive(Clone, Debug)]
pub struct ApiSpec {
    modules: Vec<ModuleEntry>,
    module_index: FxHashMap<&'static str, usize>,
    methods: Vec<MethodReceiverSig>,
    docs: BTreeMap<String, ApiDocs>,
    /// Reverse map from a `RuntimeOp` to its `module.function` spelling, for
    /// `module.call`/`module.result` trace event names.
    op_names: FxHashMap<RuntimeOp, String>,
}

impl ApiSpec {
    /// The embedded implementation bound to a script-backed method spelling.
    ///
    /// Lowering sees a method by name only — the receiver's runtime kind is not
    /// known when the call is encoded — so a spelling that two receivers bind
    /// to different implementations cannot be routed unambiguously. The
    /// registry test below rejects that case rather than letting lowering pick
    /// one arbitrarily.
    pub fn script_method_impl(&self, method: &str) -> Option<ScriptImpl> {
        static BY_METHOD: OnceLock<BTreeMap<&'static str, ScriptImpl>> = OnceLock::new();
        BY_METHOD
            .get_or_init(|| {
                let mut by_method = BTreeMap::new();
                for receiver in &self.methods {
                    for named in &receiver.methods {
                        for overload in &named.overloads {
                            if let Some(script) = overload.sig.script_impl() {
                                by_method.insert(named.name, script);
                            }
                        }
                    }
                }
                by_method
            })
            .get(method)
            .copied()
    }

    /// Every embedded implementation binding, as `(public owner, entry, impl)`.
    ///
    /// `owner` is a module name for module functions and a receiver name for
    /// methods. `entry` is the public spelling the binding serves.
    pub fn script_impls(&self) -> Vec<(&'static str, &'static str, ScriptImpl)> {
        let mut impls = Vec::new();
        for module in &self.modules {
            for function in &module.sig.functions {
                for overload in &function.overloads {
                    if let Some(script) = overload.script_impl() {
                        impls.push((module.name, function.name, script));
                    }
                }
            }
        }
        for receiver in &self.methods {
            for method in &receiver.methods {
                for overload in &method.overloads {
                    if let Some(script) = overload.sig.script_impl() {
                        impls.push((
                            registry::receiver_name(receiver.receiver),
                            method.name,
                            script,
                        ));
                    }
                }
            }
        }
        impls
    }

    fn from_registry(spec: &registry::ApiSpec) -> Self {
        Self::new(
            spec.modules.iter().map(convert_module_entry).collect(),
            spec.methods
                .iter()
                .map(convert_method_receiver_sig)
                .collect(),
            spec.docs_entries()
                .map(|(id, docs)| (id.to_string(), docs.clone()))
                .collect(),
        )
    }

    fn new(
        modules: Vec<ModuleEntry>,
        methods: Vec<MethodReceiverSig>,
        docs: BTreeMap<String, ApiDocs>,
    ) -> Self {
        let module_index = modules
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.name, index))
            .collect();
        let mut op_names = FxHashMap::default();
        for entry in &modules {
            for function in &entry.sig.functions {
                for overload in &function.overloads {
                    op_names
                        .entry(overload.op)
                        .or_insert_with(|| format!("{}.{}", entry.name, function.name));
                }
            }
        }
        for receiver in &methods {
            if receiver.receiver == MethodReceiver::FsRoot {
                for method in &receiver.methods {
                    for overload in &method.overloads {
                        op_names
                            .entry(overload.sig.op)
                            .or_insert_with(|| format!("FsRoot.{}", method.name));
                    }
                }
            }
        }
        Self {
            modules,
            module_index,
            methods,
            docs,
            op_names,
        }
    }

    /// The public callable spelling for a native operation. Root receiver
    /// methods retain operation IDs while publishing their receiver API identity.
    pub fn op_trace_name(&self, op: RuntimeOp) -> Option<&str> {
        self.op_names.get(&op).map(String::as_str)
    }

    pub fn docs(&self, id: &str) -> Option<&ApiDocs> {
        self.docs.get(id)
    }

    pub fn docs_entries(&self) -> impl Iterator<Item = (&str, &ApiDocs)> {
        self.docs.iter().map(|(id, docs)| (id.as_str(), docs))
    }

    #[allow(dead_code)]
    pub fn module_entries(&self) -> impl Iterator<Item = (&'static str, &ModuleSig)> {
        self.modules.iter().map(|entry| (entry.name, &entry.sig))
    }

    pub fn module_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.modules.iter().map(|entry| entry.name)
    }

    pub fn is_standard_module(&self, name: &str) -> bool {
        self.module_index.contains_key(name)
    }

    #[allow(dead_code)]
    pub fn method_entries(&self) -> impl Iterator<Item = (MethodReceiver, &[NamedMethodSigs])> {
        self.methods
            .iter()
            .map(|entry| (entry.receiver, entry.methods.as_slice()))
    }

    pub fn module(&self, name: &str) -> Option<&ModuleSig> {
        self.module_index
            .get(name)
            .and_then(|index| self.modules.get(*index))
            .map(|entry| &entry.sig)
    }

    pub fn module_overloads(&self, module: &str, name: &str) -> Option<&[ModuleFnSig]> {
        self.module(module)
            .and_then(|module| module.function_overloads(name))
    }

    pub fn module_op(&self, module: &str, name: &str) -> Option<RuntimeOp> {
        self.module_overloads(module, name)
            .and_then(|overloads| overloads.first())
            .map(|sig| sig.op)
    }

    pub fn module_required_effects(&self, module: &str, name: &str) -> &'static [Effect] {
        self.module_overloads(module, name)
            .and_then(|overloads| overloads.first())
            .map_or(&[], |sig| sig.effects)
    }

    pub fn method_overloads(&self, receiver: MethodReceiver, name: &str) -> Option<&[MethodSig]> {
        self.methods
            .iter()
            .find(|entry| entry.receiver == receiver)
            .and_then(|entry| entry.methods.iter().find(|method| method.name == name))
            .map(|method| method.overloads.as_slice())
    }

    pub fn method_names(
        &self,
        receiver: MethodReceiver,
    ) -> impl Iterator<Item = &'static str> + '_ {
        self.methods
            .iter()
            .find(|entry| entry.receiver == receiver)
            .into_iter()
            .flat_map(|entry| entry.methods.iter().map(|method| method.name))
    }

    pub fn method_op(&self, receiver: MethodReceiver, name: &str) -> Option<RuntimeOp> {
        self.method_overloads(receiver, name)
            .and_then(|overloads| overloads.first())
            .map(|method| method.sig.op)
    }
}

#[derive(Clone, Debug)]
pub struct ModuleEntry {
    pub name: &'static str,
    pub sig: ModuleSig,
}

#[derive(Clone, Debug)]
pub struct ModuleSig {
    pub functions: Vec<NamedModuleFns>,
}

impl ModuleSig {
    pub fn function_overloads(&self, name: &str) -> Option<&[ModuleFnSig]> {
        self.functions
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.overloads.as_slice())
    }
}

#[derive(Clone, Debug)]
pub struct NamedModuleFns {
    pub name: &'static str,
    pub overloads: Vec<ModuleFnSig>,
}

#[derive(Clone, Debug)]
pub struct ModuleFnSig {
    pub params: Vec<ParamSig>,
    pub return_ty: Type,
    pub pure: bool,
    pub command: bool,
    pub arg_check: ApiArgCheck,
    pub semantic_rule: SemanticRule,
    pub op: RuntimeOp,
    /// Implementation routing adapted from the canonical signature. `Native`
    /// entries keep their `op` dispatch; `Script` entries resolve to the named
    /// embedded implementation function at preparation time.
    pub binding: ImplBinding,
    /// Host capabilities inferred while adapting the canonical module or method
    /// signature. The checker and `xsht api` consume the same value.
    pub effects: &'static [Effect],
    /// The call's value may be dropped in statement position once its
    /// failure is propagated. Copied from the canonical signature; the
    /// checker's ignored-value rule is its only consumer.
    pub discardable: bool,
}

impl ModuleFnSig {
    /// The embedded implementation this entry routes to, if any.
    pub fn script_impl(&self) -> Option<ScriptImpl> {
        match self.binding {
            ImplBinding::Native => None,
            ImplBinding::Script(script) => Some(script),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MethodSig {
    pub sig: ModuleFnSig,
    pub receiver_ty: Option<Type>,
}

#[derive(Clone, Debug)]
pub struct ParamSig {
    pub name: &'static str,
    pub ty: Type,
    pub defaulted: bool,
    pub label: LabelRule,
}

#[derive(Clone, Debug)]
pub struct MethodReceiverSig {
    pub receiver: MethodReceiver,
    pub methods: Vec<NamedMethodSigs>,
}

#[derive(Clone, Debug)]
pub struct NamedMethodSigs {
    pub name: &'static str,
    pub overloads: Vec<MethodSig>,
}

pub fn api_spec() -> &'static ApiSpec {
    static SPEC: OnceLock<ApiSpec> = OnceLock::new();
    SPEC.get_or_init(|| ApiSpec::from_registry(registry::api_spec()))
}

fn convert_module_entry(entry: &registry::ModuleEntry) -> ModuleEntry {
    ModuleEntry {
        name: entry.name,
        sig: convert_module_sig(entry.name, &entry.sig),
    }
}

fn convert_module_sig(module: &str, sig: &registry::ModuleSig) -> ModuleSig {
    ModuleSig {
        functions: sig
            .functions
            .iter()
            .map(|function| convert_named_module_fns(module, function))
            .collect(),
    }
}

fn convert_named_module_fns(module: &str, function: &registry::NamedModuleFns) -> NamedModuleFns {
    NamedModuleFns {
        name: function.name,
        overloads: function
            .overloads
            .iter()
            .map(|sig| convert_module_fn_sig(module, function.name, sig))
            .collect(),
    }
}

fn convert_module_fn_sig(module: &str, function: &str, sig: &registry::ModuleFnSig) -> ModuleFnSig {
    ModuleFnSig {
        params: sig.params.iter().map(convert_param_sig).collect(),
        return_ty: convert_type(&sig.return_ty),
        pure: sig.pure,
        command: sig.command,
        arg_check: sig.arg_check,
        semantic_rule: sig.semantic_rule,
        op: sig.op,
        binding: sig.binding,
        effects: Effect::from_module_call(module, function),
        discardable: sig.discardable,
    }
}

fn convert_param_sig(param: &registry::ParamSig) -> ParamSig {
    ParamSig {
        name: param.name,
        ty: convert_type(&param.ty),
        defaulted: param.defaulted,
        label: param.label,
    }
}

fn convert_method_receiver_sig(entry: &registry::MethodReceiverSig) -> MethodReceiverSig {
    MethodReceiverSig {
        receiver: entry.receiver,
        methods: entry
            .methods
            .iter()
            .map(|method| convert_named_method_sigs(entry.receiver, method))
            .collect(),
    }
}

fn convert_named_method_sigs(
    receiver: MethodReceiver,
    method: &registry::NamedMethodSigs,
) -> NamedMethodSigs {
    NamedMethodSigs {
        name: method.name,
        overloads: method
            .overloads
            .iter()
            .map(|sig| convert_method_sig(receiver, sig))
            .collect(),
    }
}

fn convert_method_sig(receiver: MethodReceiver, sig: &registry::MethodSig) -> MethodSig {
    MethodSig {
        sig: ModuleFnSig {
            params: sig.sig.params.iter().map(convert_param_sig).collect(),
            return_ty: convert_type(&sig.sig.return_ty),
            pure: sig.sig.pure,
            command: sig.sig.command,
            arg_check: sig.sig.arg_check,
            semantic_rule: sig.sig.semantic_rule,
            op: sig.sig.op,
            binding: sig.sig.binding,
            effects: method_required_effects(receiver, sig.sig.pure),
            discardable: sig.sig.discardable,
        },
        receiver_ty: sig.receiver_ty.as_ref().map(convert_type),
    }
}

pub(crate) fn convert_type(ty: &xsh_registry::types::Type) -> Type {
    match ty {
        xsh_registry::types::Type::BuiltinParameter(parameter) => {
            Type::BuiltinParameter(*parameter)
        }
        xsh_registry::types::Type::Any => Type::Any,
        xsh_registry::types::Type::Unknown => Type::Unknown,
        xsh_registry::types::Type::Invalid => Type::Invalid,
        xsh_registry::types::Type::Null => Type::Null,
        xsh_registry::types::Type::Bool => Type::Bool,
        xsh_registry::types::Type::Int => Type::Int,
        xsh_registry::types::Type::UInt => Type::UInt,
        xsh_registry::types::Type::Float => Type::Float,
        xsh_registry::types::Type::Duration => Type::Duration,
        xsh_registry::types::Type::Str => Type::Str,
        xsh_registry::types::Type::Bytes => Type::Bytes,
        xsh_registry::types::Type::Digest => Type::Digest,
        xsh_registry::types::Type::Regex => Type::Regex,
        xsh_registry::types::Type::Path => Type::Path,
        xsh_registry::types::Type::List(inner) => Type::List(Box::new(convert_type(inner))),
        xsh_registry::types::Type::Map(key, inner) => {
            Type::Map(Box::new(convert_type(key)), Box::new(convert_type(inner)))
        }
        xsh_registry::types::Type::Stream(inner) => Type::Stream(Box::new(convert_type(inner))),
        xsh_registry::types::Type::Record(fields) if fields.is_empty() => Type::ErasedRecord,
        xsh_registry::types::Type::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(name, ty)| (Name::intern(name), convert_type(ty)))
                .collect(),
        ),
        xsh_registry::types::Type::Module(exports) if exports.is_empty() => Type::DynamicModule,
        xsh_registry::types::Type::Module(exports) => {
            Type::Module(std::sync::Arc::new(crate::sema::types::ModuleType::open(
                exports
                    .iter()
                    .map(|(name, ty)| {
                        (
                            Name::intern(name),
                            ModuleExportType::Value {
                                ty: convert_type(ty),
                                optional: false,
                            },
                        )
                    })
                    .collect::<BTreeMap<_, _>>(),
            )))
        }
        xsh_registry::types::Type::Result(ok, err) => {
            Type::Result(Box::new(convert_type(ok)), Box::new(convert_type(err)))
        }
        xsh_registry::types::Type::Status => Type::Status,
        xsh_registry::types::Type::EnvPathList => Type::EnvPathList,
        xsh_registry::types::Type::Error => Type::Error,
        xsh_registry::types::Type::ProcessError => Type::ProcessError,
        xsh_registry::types::Type::ErrorFamily(name) => Type::ErrorFamily(Name::intern(name)),
        xsh_registry::types::Type::Pure => Type::Pure,
        xsh_registry::types::Type::Proc => Type::Proc,
        xsh_registry::types::Type::Command => Type::Command,
        xsh_registry::types::Type::ProcessHandle => Type::ProcessHandle,
        xsh_registry::types::Type::NetJob => Type::NetJob,
        xsh_registry::types::Type::FsRoot => Type::FsRoot,
        xsh_registry::types::Type::FsLock => Type::FsLock,
        xsh_registry::types::Type::Unit => Type::Unit,
        xsh_registry::types::Type::Optional(inner) => Type::Optional(Box::new(convert_type(inner))),
        xsh_registry::types::Type::Union(members) => {
            Type::Union(members.iter().map(convert_type).collect())
        }
        xsh_registry::types::Type::NonEmpty(item) => Type::non_empty(convert_type(item)),
        xsh_registry::types::Type::RelPath => Type::rel_path(),
        xsh_registry::types::Type::Set(item) => Type::Set(Box::new(convert_type(item))),
    }
}

/// The effect a method call requires of its caller.
///
/// A Path method that is not pure reaches the filesystem; that is the only
/// way a Path method is impure. Deriving the effect from the registry's
/// purity flag means a new filesystem method cannot be registered without it.
fn method_required_effects(receiver: MethodReceiver, pure: bool) -> &'static [Effect] {
    match receiver {
        MethodReceiver::Path if !pure => &[Effect::Fs],
        // The scoped `PATH` view is environment state: every method reads or
        // assigns it, and a view handed to another proc still does.
        MethodReceiver::EnvPathList => &[Effect::Env],
        MethodReceiver::ProcessHandle => &[Effect::Process],
        MethodReceiver::NetJob => &[Effect::Net],
        MethodReceiver::FsRoot => &[Effect::Fs],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{api_spec, convert_type};
    use xsh_registry::signature as registry;

    #[test]
    fn api_spec_adapter_exactly_mirrors_registry() {
        let main = api_spec();
        let registry = registry::api_spec();

        assert_eq!(
            main.docs_entries().collect::<Vec<_>>(),
            registry.docs_entries().collect::<Vec<_>>()
        );

        assert_eq!(main.modules.len(), registry.modules.len());
        for (main_module, registry_module) in main.modules.iter().zip(&registry.modules) {
            assert_eq!(main_module.name, registry_module.name);
            assert_eq!(
                main_module.sig.functions.len(),
                registry_module.sig.functions.len()
            );
            for (main_function, registry_function) in main_module
                .sig
                .functions
                .iter()
                .zip(&registry_module.sig.functions)
            {
                assert_eq!(main_function.name, registry_function.name);
                assert_eq!(
                    main_function.overloads.len(),
                    registry_function.overloads.len()
                );
                for (main_overload, registry_overload) in main_function
                    .overloads
                    .iter()
                    .zip(&registry_function.overloads)
                {
                    assert_module_overload_matches_registry(main_overload, registry_overload);
                    assert_eq!(
                        main_overload.effects,
                        crate::syntax::node::Effect::from_module_call(
                            main_module.name,
                            main_function.name,
                        )
                    );
                }
            }
        }

        assert_eq!(main.methods.len(), registry.methods.len());
        for (main_receiver, registry_receiver) in main.methods.iter().zip(&registry.methods) {
            assert_eq!(main_receiver.receiver, registry_receiver.receiver);
            assert_eq!(main_receiver.methods.len(), registry_receiver.methods.len());
            for (main_method, registry_method) in
                main_receiver.methods.iter().zip(&registry_receiver.methods)
            {
                assert_eq!(main_method.name, registry_method.name);
                assert_eq!(main_method.overloads.len(), registry_method.overloads.len());
                for (main_overload, registry_overload) in
                    main_method.overloads.iter().zip(&registry_method.overloads)
                {
                    assert_module_overload_matches_registry(
                        &main_overload.sig,
                        &registry_overload.sig,
                    );
                    assert_eq!(
                        main_overload.sig.effects,
                        super::method_required_effects(
                            main_receiver.receiver,
                            main_overload.sig.pure,
                        )
                    );
                    assert_eq!(
                        main_overload.receiver_ty,
                        registry_overload.receiver_ty.as_ref().map(convert_type)
                    );
                }
            }
        }
    }

    fn assert_module_overload_matches_registry(
        main: &super::ModuleFnSig,
        registry: &registry::ModuleFnSig,
    ) {
        assert_eq!(main.params.len(), registry.params.len());
        for (main_param, registry_param) in main.params.iter().zip(&registry.params) {
            assert_eq!(main_param.name, registry_param.name);
            assert_eq!(main_param.ty, convert_type(&registry_param.ty));
            assert_eq!(main_param.defaulted, registry_param.defaulted);
        }
        assert_eq!(main.return_ty, convert_type(&registry.return_ty));
        assert_eq!(main.pure, registry.pure);
        assert_eq!(main.command, registry.command);
        assert_eq!(main.arg_check, registry.arg_check);
        assert_eq!(main.op, registry.op);
        assert_eq!(main.binding, registry.binding);
    }

    // An effect the checker does not demand is not an effect: a proc that
    // declares every effect but the one a method needs must be rejected, and
    // the same call must check once the effect is declared. The probes come
    // from the registry, so a method added later is covered without a list.
    #[test]
    fn every_registry_method_effect_is_enforced_by_the_checker() {
        use crate::diagnostic::DiagnosticCode;
        use crate::sema::check::Checker;
        use crate::source::SourceId;
        use crate::syntax::node::Effect;
        use crate::syntax::parser::Parser;

        let check = |source: &str| {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(
                parsed.diagnostics.is_empty(),
                "{source}\n{:?}",
                parsed.diagnostics
            );
            Checker::check_arena(&parsed.arena, source).diagnostics
        };
        let mut undeclared = Vec::new();
        let mut unenforced = Vec::new();
        let mut probed = 0;
        for receiver in &api_spec().methods {
            for method in &receiver.methods {
                for overload in &method.overloads {
                    let label = format!("{:?}.{}", receiver.receiver, method.name);
                    if overload.sig.effects.is_empty() {
                        // An impure method with no effect could run anywhere
                        // a proc can, unseen by every effect clause.
                        if !overload.sig.pure {
                            undeclared.push(label);
                        }
                        continue;
                    }
                    let receiver_ty =
                        overload
                            .receiver_ty
                            .clone()
                            .unwrap_or(match receiver.receiver {
                                super::MethodReceiver::Path => super::Type::Path,
                                super::MethodReceiver::EnvPathList => super::Type::EnvPathList,
                                super::MethodReceiver::ProcessHandle => super::Type::ProcessHandle,
                                super::MethodReceiver::NetJob => super::Type::NetJob,
                                super::MethodReceiver::FsRoot => super::Type::FsRoot,
                                other => panic!("{label}: no probe receiver for {other:?}"),
                            });
                    let required = overload
                        .sig
                        .params
                        .iter()
                        .filter(|param| !param.defaulted)
                        .collect::<Vec<_>>();
                    let params = required
                        .iter()
                        .enumerate()
                        .map(|(index, param)| format!(", a{index}: {}", param.ty))
                        .collect::<String>();
                    let args = required
                        .iter()
                        .enumerate()
                        .map(|(index, param)| match param.label {
                            super::LabelRule::Free => format!("a{index}"),
                            super::LabelRule::Required => format!("{}: a{index}", param.name),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let probe = |effects: Vec<&str>| {
                        format!(
                            "proc probe(receiver: {receiver_ty}{params}) [{}] {{\n  let _ = receiver.{}({args})\n}}\n",
                            effects.join(", "),
                            method.name
                        )
                    };
                    let every = Effect::ALL.iter().map(Effect::as_str).collect::<Vec<_>>();
                    let allowed = check(&probe(every.clone()));
                    assert!(allowed.is_empty(), "{label}: {allowed:?}");
                    for effect in overload.sig.effects {
                        // Every effect that does not grant the required one;
                        // `io` grants several, so the rule is asked, not assumed.
                        let without = Effect::ALL
                            .iter()
                            .filter(|other| !Checker::effects_covers(&[(*other).clone()], effect))
                            .map(Effect::as_str)
                            .collect();
                        let denied = check(&probe(without));
                        let demanded = denied.iter().any(|diagnostic| {
                            diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)
                                && diagnostic
                                    .labels
                                    .iter()
                                    .filter_map(|label| label.message.as_deref())
                                    .chain([diagnostic.message.as_str()])
                                    .any(|text| text.contains(&format!("`{}` effect", effect.as_str())))
                        });
                        if !demanded {
                            unenforced.push(label.clone());
                        }
                        probed += 1;
                    }
                }
            }
        }
        assert!(probed > 40, "only {probed} effectful methods were probed");
        assert!(
            unenforced.is_empty(),
            "effects not enforced: {unenforced:?}"
        );
        assert!(
            undeclared.is_empty(),
            "impure methods without an effect: {undeclared:?}"
        );
    }

    // Required labels rely on standard argument-slot binding. Methods also
    // need one overload so their binding route enforces the selected labels.
    #[test]
    fn a_required_label_is_declared_only_where_the_checker_enforces_it() {
        use super::{ApiArgCheck, LabelRule, MethodReceiver, ModuleFnSig};
        use crate::diagnostic::DiagnosticCode;
        use crate::sema::check::Checker;
        use crate::source::SourceId;
        use crate::syntax::parser::Parser;

        let requires = |sig: &ModuleFnSig| {
            sig.params
                .iter()
                .any(|param| param.label == LabelRule::Required)
        };
        for (module, sig) in api_spec().module_entries() {
            for function in &sig.functions {
                for overload in &function.overloads {
                    assert!(
                        !requires(overload) || overload.arg_check == ApiArgCheck::Standard,
                        "{module}.{} requires a label without standard argument binding",
                        function.name
                    );
                }
            }
        }
        let mut required = 0;
        for (receiver, methods) in api_spec().method_entries() {
            for method in methods {
                if !method
                    .overloads
                    .iter()
                    .any(|overload| requires(&overload.sig))
                {
                    continue;
                }
                required += 1;
                assert!(
                    method.overloads.len() == 1
                        && method.overloads[0].sig.arg_check == ApiArgCheck::Standard
                        && receiver != MethodReceiver::PathConstructor,
                    "{receiver:?}.{} requires a label the checker does not enforce",
                    method.name
                );
            }
        }
        assert_eq!(required, 6);

        let check = |arguments: &str| {
            let source = format!(
                "proc probe() [process] {{ let _ = linux.set_capabilities({arguments}) }}"
            );
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            Checker::check_arena(&parsed.arena, &source).diagnostics
        };
        let positional = check("[], [], []");
        assert!(
            positional.iter().any(|diagnostic| {
                diagnostic.code == Some(DiagnosticCode::CheckNamedArg)
            }),
            "required labels accepted positional arguments: {positional:?}"
        );
        let labeled = check("effective: [], permitted: [], inheritable: []");
        assert!(labeled.is_empty(), "labeled arguments rejected: {labeled:?}");
    }
}
