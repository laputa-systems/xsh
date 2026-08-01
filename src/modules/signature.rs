use crate::sema::types::{ModuleExportType, Type};
use crate::symbol::Name;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use xsh_registry::signature as registry;

pub use registry::{ApiArgCheck, ApiDocs, ApiNavigation, MethodReceiver};
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
        Self {
            modules,
            module_index,
            methods,
            docs,
            op_names,
        }
    }

    /// The `module.function` spelling for a `RuntimeOp`, if it originates from a
    /// standard module function (used as the `module.call` trace name).
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

    pub fn method_overloads(&self, receiver: MethodReceiver, name: &str) -> Option<&[MethodSig]> {
        self.methods
            .iter()
            .find(|entry| entry.receiver == receiver)
            .and_then(|entry| entry.methods.iter().find(|method| method.name == name))
            .map(|method| method.overloads.as_slice())
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
    pub op: RuntimeOp,
}

#[derive(Clone, Debug)]
pub struct MethodSig {
    pub sig: ModuleFnSig,
    pub return_ty: MethodReturn,
}

impl MethodSig {
    pub fn concrete_return_ty(&self, receiver_ty: &Type) -> Type {
        match &self.return_ty {
            MethodReturn::Type(ty) => ty.clone(),
            MethodReturn::Receiver => receiver_ty.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum MethodReturn {
    Type(Type),
    Receiver,
}

#[derive(Clone, Debug)]
pub struct ParamSig {
    pub name: &'static str,
    pub ty: Type,
    pub defaulted: bool,
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
        sig: convert_module_sig(&entry.sig),
    }
}

fn convert_module_sig(sig: &registry::ModuleSig) -> ModuleSig {
    ModuleSig {
        functions: sig.functions.iter().map(convert_named_module_fns).collect(),
    }
}

fn convert_named_module_fns(function: &registry::NamedModuleFns) -> NamedModuleFns {
    NamedModuleFns {
        name: function.name,
        overloads: function
            .overloads
            .iter()
            .map(convert_module_fn_sig)
            .collect(),
    }
}

fn convert_module_fn_sig(sig: &registry::ModuleFnSig) -> ModuleFnSig {
    ModuleFnSig {
        params: sig.params.iter().map(convert_param_sig).collect(),
        return_ty: convert_type(&sig.return_ty),
        pure: sig.pure,
        command: sig.command,
        arg_check: sig.arg_check,
        op: sig.op,
    }
}

fn convert_param_sig(param: &registry::ParamSig) -> ParamSig {
    ParamSig {
        name: param.name,
        ty: convert_type(&param.ty),
        defaulted: param.defaulted,
    }
}

fn convert_method_receiver_sig(entry: &registry::MethodReceiverSig) -> MethodReceiverSig {
    MethodReceiverSig {
        receiver: entry.receiver,
        methods: entry
            .methods
            .iter()
            .map(convert_named_method_sigs)
            .collect(),
    }
}

fn convert_named_method_sigs(method: &registry::NamedMethodSigs) -> NamedMethodSigs {
    NamedMethodSigs {
        name: method.name,
        overloads: method.overloads.iter().map(convert_method_sig).collect(),
    }
}

fn convert_method_sig(sig: &registry::MethodSig) -> MethodSig {
    MethodSig {
        sig: convert_module_fn_sig(&sig.sig),
        return_ty: match &sig.return_ty {
            registry::MethodReturn::Type(ty) => MethodReturn::Type(convert_type(ty)),
            registry::MethodReturn::Receiver => MethodReturn::Receiver,
        },
    }
}

pub(crate) fn convert_type(ty: &xsh_registry::types::Type) -> Type {
    match ty {
        xsh_registry::types::Type::Any => Type::Any,
        xsh_registry::types::Type::Unknown => Type::Unknown,
        xsh_registry::types::Type::Invalid => Type::Invalid,
        xsh_registry::types::Type::Null => Type::Null,
        xsh_registry::types::Type::Bool => Type::Bool,
        xsh_registry::types::Type::Int => Type::Int,
        xsh_registry::types::Type::Float => Type::Float,
        xsh_registry::types::Type::Duration => Type::Duration,
        xsh_registry::types::Type::Str => Type::Str,
        xsh_registry::types::Type::Bytes => Type::Bytes,
        xsh_registry::types::Type::Digest => Type::Digest,
        xsh_registry::types::Type::Regex => Type::Regex,
        xsh_registry::types::Type::Path => Type::Path,
        xsh_registry::types::Type::List(inner) => Type::List(Box::new(convert_type(inner))),
        xsh_registry::types::Type::Map(inner) => Type::Map(Box::new(convert_type(inner))),
        xsh_registry::types::Type::Stream(inner) => Type::Stream(Box::new(convert_type(inner))),
        xsh_registry::types::Type::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(name, ty)| (Name::intern(name), convert_type(ty)))
                .collect(),
        ),
        xsh_registry::types::Type::Module(exports) => Type::Module(
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
                .collect(),
        ),
        xsh_registry::types::Type::Result(ok, err) => {
            Type::Result(Box::new(convert_type(ok)), Box::new(convert_type(err)))
        }
        xsh_registry::types::Type::Status => Type::Status,
        xsh_registry::types::Type::EnvPathList => Type::EnvPathList,
        xsh_registry::types::Type::Error => Type::Error,
        xsh_registry::types::Type::ProcessError => Type::ProcessError,
        xsh_registry::types::Type::Pure => Type::Pure,
        xsh_registry::types::Type::Proc => Type::Proc,
        xsh_registry::types::Type::Command => Type::Command,
        xsh_registry::types::Type::ProcessHandle => Type::ProcessHandle,
        xsh_registry::types::Type::Unit => Type::Unit,
        xsh_registry::types::Type::Optional(inner) => Type::Optional(Box::new(convert_type(inner))),
    }
}

#[cfg(test)]
mod tests {
    use super::{MethodReturn, api_spec, convert_type};
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
                    match (&main_overload.return_ty, &registry_overload.return_ty) {
                        (
                            MethodReturn::Type(main_ty),
                            registry::MethodReturn::Type(registry_ty),
                        ) => assert_eq!(main_ty, &convert_type(registry_ty)),
                        (MethodReturn::Receiver, registry::MethodReturn::Receiver) => {}
                        _ => panic!("method return adapter drifted for {}", main_method.name),
                    }
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
    }
}
