pub use crate::api_docs::ApiDocs;
use crate::records::{
    archive_entry_type, diff_result_type, dns_host_type, dns_lookup_type, elf_info_type,
    env_entry_type, env_path_entry_type, env_raw_entry_type, fs_copy_tree_result_type, fs_entry_type,
    fs_filesystem_stats_type, fs_lock_type, fs_mount_type, fs_remove_manifest_result_type,
    fs_root_children_result_type, fs_root_filesystem_stats_type, fs_root_read_result_type,
    fs_root_readlink_result_type, fs_root_type, group_record_type, hash_check_line_type,
    linux_blkid_type, linux_block_device_type, linux_disk_usage_type, linux_file_attrs_type,
    linux_fsck_type, linux_interface_type, linux_loop_device_type, linux_meminfo_type,
    linux_modinfo_type, linux_module_type, linux_network_dump_type, linux_open_file_type,
    linux_partition_table_type, linux_rfkill_type, linux_route_type, linux_uevent_type,
    measured_command_type, mime_info_type, mime_parse_type, net_pool_type, net_response_type,
    patch_result_type, process_entry_type, process_port_type, process_stats_type,
    process_thread_type, process_wait_any_type, regex_match_type, signal_record_type,
    spawn_record_type, system_execution_units_type, system_memory_type, system_os_release_type,
    uname_record_type, unix_child_event_type, unix_id_type, unix_kill_all_result_type,
    unix_logged_process_group_type, unix_pid1_event_type, unix_pid1_shutdown_type,
    unix_spawned_child_type, unix_tty_attrs_type, user_record_type,
};
#[cfg(feature = "native-tests")]
use crate::records::{test_call_type, test_context_type, test_script_output_type};
pub use crate::runtime_op::RuntimeOp;
pub(in crate::signature) use crate::types::Type;
pub(in crate::signature) use std::collections::BTreeMap;
use std::sync::OnceLock;

mod builders;
mod arithmetic;
pub use arithmetic::{ArithmeticDomain, ArithmeticOperator, ArithmeticResult, ArithmeticSignature};
mod docs;
mod linux_storage;
mod methods;
mod modules;
mod streams;

pub(in crate::signature) use builders::command_callable;
pub use docs::{associated_module_functions, method_api_id, module_api_id, receiver_name};
pub(in crate::signature) use methods::value_methods;
pub(in crate::signature) use modules::build_api_spec;
pub use modules::record_docs;

#[derive(Clone, Debug)]
pub struct ApiSpec {
    pub modules: Vec<ModuleEntry>,
    pub methods: Vec<MethodReceiverSig>,
    docs: BTreeMap<String, ApiDocs>,
    registered_methods: Vec<RegisteredMethodLocation>,
    registered_method_ids: Vec<BTreeMap<&'static str, Vec<RegisteredMethodId>>>,
    method_family_members: [Vec<RegisteredMethodId>; 3],
}

#[derive(Clone, Copy, Debug)]
struct RegisteredMethodLocation {
    receiver: usize,
    method: usize,
    overload: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum MethodFamilyId {
    Length,
    Emptiness,
    PathWrite,
}

impl MethodFamilyId {
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Length),
            1 => Some(Self::Emptiness),
            2 => Some(Self::PathWrite),
            _ => None,
        }
    }

    pub const fn raw(self) -> u8 {
        self as u8
    }
}

fn method_family_for_signature(
    receiver: MethodReceiver,
    name: &str,
    signature: &MethodSig,
) -> Option<MethodFamilyId> {
    let params = &signature.sig.params;
    let result_ty = &signature.sig.return_ty;
    match (receiver, name, signature.sig.op) {
        (MethodReceiver::List, "len", RuntimeOp::ListLen)
        | (MethodReceiver::Map, "len", RuntimeOp::MapLen)
        | (MethodReceiver::Set, "len", RuntimeOp::SetLen)
        | (MethodReceiver::Bytes, "len", RuntimeOp::BytesLen)
            if params.is_empty() && *result_ty == Type::Int && signature.sig.pure =>
        {
            Some(MethodFamilyId::Length)
        }
        (MethodReceiver::List, "is_empty", RuntimeOp::ListIsEmpty)
        | (MethodReceiver::Map, "is_empty", RuntimeOp::MapIsEmpty)
        | (MethodReceiver::Set, "is_empty", RuntimeOp::SetIsEmpty)
        | (MethodReceiver::Bytes, "is_empty", RuntimeOp::BytesIsEmpty)
        | (MethodReceiver::Str, "is_empty", RuntimeOp::TextIsEmpty)
            if params.is_empty() && *result_ty == Type::Bool && signature.sig.pure =>
        {
            Some(MethodFamilyId::Emptiness)
        }
        (MethodReceiver::Path, "write", RuntimeOp::FsWrite)
            if !signature.sig.pure
                && *result_ty == result(Type::Unit)
                && (params.len() == 1 || params.len() == 2)
                && matches!(&params[0].ty, Type::Str | Type::Bytes)
                && (params.len() == 1
                    || params[1].ty == Type::Int
                        && params[1].name == "mode"
                        && !params[1].defaulted) =>
        {
            Some(MethodFamilyId::PathWrite)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct RegisteredMethodId(u32);

impl RegisteredMethodId {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl ApiSpec {
    pub fn new(modules: Vec<ModuleEntry>, methods: Vec<MethodReceiverSig>) -> Self {
        let docs = docs::build_api_docs(&modules, &methods);
        let mut registered_methods = Vec::new();
        let mut registered_method_ids = Vec::<BTreeMap<_, Vec<_>>>::new();
        let mut method_family_members = [Vec::new(), Vec::new(), Vec::new()];
        for (receiver_index, receiver) in methods.iter().enumerate() {
            let lookup_index = receiver.receiver as usize;
            if registered_method_ids.len() <= lookup_index { registered_method_ids.resize_with(lookup_index + 1, BTreeMap::new); }
            for (method_index, method) in receiver.methods.iter().enumerate() {
                for (overload, signature) in method.overloads.iter().enumerate() {
                    let id = RegisteredMethodId(u32::try_from(registered_methods.len()).expect("registered method count fits its identifier"));
                    registered_methods.push(RegisteredMethodLocation { receiver: receiver_index, method: method_index, overload });
                    registered_method_ids[lookup_index].entry(method.name).or_default().push(id);
                    if let Some(family) = method_family_for_signature(receiver.receiver, method.name, signature) {
                        method_family_members[family.raw() as usize].push(id);
                    }
                }
            }
        }
        let spec = Self {
            modules,
            methods,
            docs,
            registered_methods,
            registered_method_ids,
            method_family_members,
        };
        spec.validate_docs()
            .expect("standard API registry must have complete documentation");
        spec
    }

    pub fn docs(&self, id: &str) -> Option<&ApiDocs> {
        self.docs.get(id)
    }

    pub fn method_family_id(
        &self,
        receiver: MethodReceiver,
        name: &str,
        overload: usize,
    ) -> Option<MethodFamilyId> {
        let id = *self.registered_method_ids.get(receiver as usize)?.get(name)?.get(overload)?;
        let (_, _, _, signature) = self.registered_method(id)?;
        method_family_for_signature(receiver, name, signature)
    }

    pub fn registered_method_id(
        &self,
        receiver: MethodReceiver,
        name: &str,
        overload: usize,
    ) -> Option<RegisteredMethodId> {
        self.registered_method_ids.get(receiver as usize)?.get(name)?.get(overload).copied()
    }

    pub fn registered_method(
        &self,
        id: RegisteredMethodId,
    ) -> Option<(MethodReceiver, &'static str, usize, &MethodSig)> {
        let location = self.registered_methods.get(id.raw() as usize)?;
        let receiver = self.methods.get(location.receiver)?;
        let method = receiver.methods.get(location.method)?;
        let signature = method.overloads.get(location.overload)?;
        Some((receiver.receiver, method.name, location.overload, signature))
    }

    pub fn method_family_member_ids(&self, family: MethodFamilyId) -> &[RegisteredMethodId] {
        &self.method_family_members[family.raw() as usize]
    }

    /// Every embedded implementation module identity named by a binding.
    ///
    /// Used to check the embedded catalog against the registry in both
    /// directions: no binding may name an identity the catalog lacks, and no
    /// catalog entry may be unreachable from the registry.
    pub fn script_impls(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        let mut impls = Vec::new();
        for module in &self.modules {
            for function in &module.sig.functions {
                for overload in &function.overloads {
                    if let ImplBinding::Script(script) = overload.binding {
                        impls.push((module.name, function.name, script.module));
                    }
                }
            }
        }
        for receiver in &self.methods {
            for method in &receiver.methods {
                for overload in &method.overloads {
                    if let ImplBinding::Script(script) = overload.sig.binding {
                        impls.push((receiver_name(receiver.receiver), method.name, script.module));
                    }
                }
            }
        }
        impls
    }

    pub fn docs_entries(&self) -> impl Iterator<Item = (&str, &ApiDocs)> {
        self.docs.iter().map(|(id, docs)| (id.as_str(), docs))
    }

    pub fn validate_docs(&self) -> Result<(), String> {
        let mut expected = BTreeMap::<String, ()>::new();
        for module in &self.modules {
            expected.insert(format!("module.{}", module.name), ());
            for function in &module.sig.functions {
                expected.insert(module_api_id(module.name, function.name), ());
            }
        }
        for receiver in &self.methods {
            for method in &receiver.methods {
                expected.insert(method_api_id(receiver.receiver, method.name), ());
            }
        }

        for id in expected.keys() {
            let Some(docs) = self.docs(id) else {
                return Err(format!("missing API docs for '{id}'"));
            };
            if docs.summary.trim().is_empty() {
                return Err(format!("API docs for '{id}' have an empty summary"));
            }
            if docs.tags.iter().any(|tag| tag.trim().is_empty()) {
                return Err(format!("API docs for '{id}' have an empty tag"));
            }
            if docs
                .example
                .as_deref()
                .is_some_and(|example| example.trim().is_empty())
            {
                return Err(format!("API docs for '{id}' have an empty example"));
            }
        }

        for id in self.docs.keys() {
            if !expected.contains_key(id) {
                return Err(format!("API docs contain unknown item '{id}'"));
            }
        }

        Ok(())
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

#[derive(Clone, Debug)]
pub struct NamedModuleFns {
    pub name: &'static str,
    pub overloads: Vec<ModuleFnSig>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScriptImpl {
    /// Internal identity of the embedded implementation module.
    pub module: &'static str,
    /// Implementation function identity inside that module.
    pub function: &'static str,
}

/// Where a public callable entry or overload gets its behavior.
///
/// The binding is closed: an entry either keeps its native operation or is
/// served by one embedded implementation function. It carries implementation
/// routing only — the public signature stays the single declaration of record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImplBinding {
    /// The existing native `RuntimeOp` body.
    Native,
    /// An embedded XSH implementation function.
    Script(ScriptImpl),
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
    pub binding: ImplBinding,
    /// The call's value may be dropped in statement position once its
    /// failure is propagated: `f(...)?` is a statement even though the call
    /// returns a value. The entry returns something most callers do not
    /// need, and the failure is the part that must not be lost.
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

/// How a call writes the argument of one parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelRule {
    /// By position, or labeled with the parameter's name.
    Free,
    /// Only labeled with the parameter's name, which is chosen so that the
    /// call reads as a sentence and its operands cannot be swapped
    /// unnoticed: `src.copy(to: dest)`, `text.replace("a", with: "b")`. A
    /// call that passes the argument by position does not check.
    Required,
}

/// Preparation facts that refine a callable beyond structural type substitution.
/// Dynamic arguments keep the declared result when no checked fact is available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticRule {
    Standard,
    SchemaValidation,
    ConstantKeyProjection,
    CliDescriptor,
    CliCommands,
}

impl SemanticRule {
    fn for_operation(op: RuntimeOp) -> Self {
        match op {
            RuntimeOp::CliCommands => Self::CliCommands,
            RuntimeOp::RecordGet => Self::ConstantKeyProjection,
            RuntimeOp::CliParse | RuntimeOp::CliApplet | RuntimeOp::CliParseFull => {
                Self::CliDescriptor
            }
            _ => Self::Standard,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiArgCheck {
    Standard,
    JsonCompatible,
    HashVerifyFile,
    PathLikeSingle,
    ResultContext,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MethodReceiver {
    PathConstructor,
    Result,
    EnvPathList,
    Path,
    Int,
    Float,
    List,
    Map,
    Record,
    Stream,
    Str,
    Bytes,
    Status,
    Digest,
    Regex,
    ProcessHandle,
    NetJob,
    FsRoot,
    /// A `NonEmpty[T]` value: the operations the validation guarantees or
    /// survives. Every other method of the value is a `List` method.
    NonEmpty,
    /// A `RelPath` value: the operations that keep a path confined. Every
    /// other method of the value is a `Path` method.
    RelPath,
    Set,
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

/// Removed module spellings used only for checked migration diagnostics.
/// Receiver promotion must preserve source evaluation order and capability identity.
pub fn legacy_fs_root_method(function: &str) -> Option<&'static str> {
    match function {
        "close_root" => Some("close"),
        "root_path" => Some("host_path"),
        "root" => Some("open_root"),
        "root_read" => Some("read_bytes"),
        "root_read_text" => Some("read_text"),
        "root_read_result" => Some("read_result"),
        "root_filesystem_stats" => Some("filesystem_stats"),
        "root_children" => Some("children"),
        "root_write" => Some("write"),
        "root_write_atomic" => Some("write_atomic"),
        "root_metadata" => Some("metadata"),
        "root_stat" => Some("stat"),
        "root_exists" => Some("exists"),
        "root_mkdir" => Some("mkdir"),
        "root_remove" => Some("remove"),
        "root_readlink" => Some("readlink"),
        "root_readlink_result" => Some("readlink_result"),
        "root_symlink" => Some("symlink"),
        "root_chmod" => Some("chmod"),
        _ => None,
    }
}

/// The `fs` functions removed because the `Path` method of the same name is
/// the one spelling of the operation: `fs.NAME(path, rest...)` was
/// `path.NAME(rest...)`. Used only for the checked migration diagnostic.
/// `executable` is listed for its `Path` overload; `fs.executable(mode: Int)`
/// has no path receiver and is still a function.
pub const REMOVED_FS_PATH_FUNCTIONS: [&str; 11] = [
    "chmod",
    "copy",
    "executable",
    "exists",
    "metadata",
    "mkdir",
    "read_text",
    "remove",
    "rename",
    "write",
    "write_atomic",
];

pub fn api_spec() -> &'static ApiSpec {
    static SPEC: OnceLock<ApiSpec> = OnceLock::new();
    SPEC.get_or_init(build_api_spec)
}

pub(in crate::signature) fn btree_map<K: Into<String>, V>(
    entries: Vec<(K, V)>,
) -> BTreeMap<String, V> {
    entries
        .into_iter()
        .map(|(name, value)| (name.into(), value))
        .collect()
}

pub fn module_sig(entries: Vec<(&'static str, ModuleFnSig)>) -> ModuleSig {
    let mut functions = Vec::<NamedModuleFns>::new();
    for (name, sig) in entries {
        if let Some(entry) = functions.iter_mut().find(|entry| entry.name == name) {
            entry.overloads.push(sig);
        } else {
            functions.push(NamedModuleFns {
                name,
                overloads: vec![sig],
            });
        }
    }
    ModuleSig { functions }
}

pub fn sig(params: Vec<ParamSig>, return_ty: Type, pure: bool, op: RuntimeOp) -> ModuleFnSig {
    let command = command_callable(&params, &return_ty, pure);
    ModuleFnSig {
        params,
        return_ty,
        pure,
        command,
        arg_check: ApiArgCheck::Standard,
        semantic_rule: SemanticRule::for_operation(op),
        op,
        binding: ImplBinding::Native,
        discardable: false,
    }
}

/// Declare a public entry whose implementation is an embedded XSH function.
///
/// The parameters are the public contract; the embedded implementation
/// function mirrors them positionally and by name, so the call boundary can
/// bind caller arguments against either description interchangeably.
pub fn script_sig(
    params: Vec<ParamSig>,
    return_ty: Type,
    pure: bool,
    op: RuntimeOp,
    module: &'static str,
    function: &'static str,
) -> ModuleFnSig {
    ModuleFnSig {
        command: command_callable(&params, &return_ty, pure),
        params,
        return_ty,
        pure,
        arg_check: ApiArgCheck::Standard,
        semantic_rule: SemanticRule::for_operation(op),
        op,
        binding: ImplBinding::Script(ScriptImpl { module, function }),
        discardable: false,
    }
}

/// Declare a script-backed entry with a non-default argument check.
pub fn script_sig_with_arg_check(
    params: Vec<ParamSig>,
    return_ty: Type,
    pure: bool,
    op: RuntimeOp,
    arg_check: ApiArgCheck,
    module: &'static str,
    function: &'static str,
) -> ModuleFnSig {
    ModuleFnSig {
        command: command_callable(&params, &return_ty, pure),
        params,
        return_ty,
        pure,
        arg_check,
        semantic_rule: SemanticRule::for_operation(op),
        op,
        binding: ImplBinding::Script(ScriptImpl { module, function }),
        discardable: false,
    }
}

fn sig_with_arg_check(
    params: Vec<ParamSig>,
    return_ty: Type,
    pure: bool,
    op: RuntimeOp,
    arg_check: ApiArgCheck,
) -> ModuleFnSig {
    ModuleFnSig {
        command: command_callable(&params, &return_ty, pure),
        params,
        return_ty,
        pure,
        arg_check,
        semantic_rule: SemanticRule::for_operation(op),
        op,
        binding: ImplBinding::Native,
        discardable: false,
    }
}

pub fn param(name: &'static str, ty: Type) -> ParamSig {
    ParamSig {
        name,
        ty,
        defaulted: false,
        label: LabelRule::Free,
    }
}

pub fn default_param(name: &'static str, ty: Type) -> ParamSig {
    ParamSig {
        name,
        ty,
        defaulted: true,
        label: LabelRule::Free,
    }
}

/// A required parameter whose argument is written with its label.
pub fn labeled_param(name: &'static str, ty: Type) -> ParamSig {
    ParamSig {
        name,
        ty,
        defaulted: false,
        label: LabelRule::Required,
    }
}

/// What `Path.remove` does about a missing path when a call
/// leaves `missing_ok` out: nothing, because the caller wants the path gone
/// and it is. The registry records only that the parameter has a default, so
/// every runtime path reads the value here.
pub const REMOVE_MISSING_OK_DEFAULT: bool = true;

pub fn result(ok: Type) -> Type {
    Type::Result(Box::new(ok), Box::new(Type::Error))
}

#[cfg(test)]
mod tests {
    use super::{api_spec, result, RuntimeOp, Type};
    use crate::{records, reference};

    fn method_overloads<'a>(
        spec: &'a super::ApiSpec,
        receiver: super::MethodReceiver,
        name: &str,
    ) -> Option<&'a [super::MethodSig]> {
        spec.methods
            .iter()
            .find(|entry| entry.receiver == receiver)?
            .methods
            .iter()
            .find(|method| method.name == name)
            .map(|method| method.overloads.as_slice())
    }

    #[test]
    fn membership_aliases_are_absent_from_callable_and_documentation_inventory() {
        for (receiver, name) in [
            (super::MethodReceiver::Str, "contains"),
            (super::MethodReceiver::Bytes, "contains"),
            (super::MethodReceiver::List, "contains"),
            (super::MethodReceiver::Map, "has"),
            (super::MethodReceiver::Record, "has"),
        ] {
            assert!(
                !api_spec()
                    .methods
                    .iter()
                    .any(|entry| entry.receiver == receiver
                        && entry.methods.iter().any(|method| method.name == name))
            );
            assert!(
                api_spec()
                    .docs(&super::method_api_id(receiver, name))
                    .is_none()
            );
        }
        for (module, name) in [
            ("set", "has"),
            ("test", "contains"),
            ("test", "not_contains"),
        ] {
            assert!(!api_spec().modules.iter().any(|entry| {
                entry.name == module
                    && entry
                        .sig
                        .functions
                        .iter()
                        .any(|function| function.name == name)
            }));
            assert!(
                api_spec()
                    .docs(&super::module_api_id(module, name))
                    .is_none()
            );
        }
        assert!(
            crate::errors::builtin_error_families()
                .iter()
                .any(|family| family.name == "AssertionError")
        );
    }

    #[test]
    fn semantic_preparation_rules_follow_canonical_operations() {
        use super::{MethodReceiver, SemanticRule};
        let spec = api_spec();
        let cli = spec
            .modules
            .iter()
            .find(|entry| entry.name == "cli")
            .unwrap();
        for name in ["parse", "applet", "parse_full"] {
            let entry = cli
                .sig
                .functions
                .iter()
                .find(|entry| entry.name == name)
                .unwrap();
            assert!(
                entry
                    .overloads
                    .iter()
                    .all(|sig| sig.semantic_rule == SemanticRule::CliDescriptor)
            );
        }
        let commands = cli
            .sig
            .functions
            .iter()
            .find(|entry| entry.name == "commands")
            .unwrap();
        assert!(
            commands
                .overloads
                .iter()
                .all(|sig| sig.semantic_rule == SemanticRule::CliCommands)
        );
        let record = spec
            .methods
            .iter()
            .find(|entry| entry.receiver == MethodReceiver::Record)
            .unwrap();
        let get = record
            .methods
            .iter()
            .find(|entry| entry.name == "get")
            .unwrap();
        assert!(
            get.overloads
                .iter()
                .all(|sig| sig.sig.semantic_rule == SemanticRule::ConstantKeyProjection)
        );
        let list = spec
            .methods
            .iter()
            .find(|entry| entry.receiver == MethodReceiver::List)
            .unwrap();
        let get = list
            .methods
            .iter()
            .find(|entry| entry.name == "get")
            .unwrap();
        assert!(
            get.overloads
                .iter()
                .all(|sig| sig.sig.semantic_rule == SemanticRule::Standard)
        );
    }

    #[test]
    fn public_api_items_have_complete_registry_docs() {
        api_spec()
            .validate_docs()
            .expect("public API docs should be complete");
    }

    #[test]
    fn ed25519_verifier_is_pure_and_accepts_exact_byte_arguments() {
        let spec = api_spec();
        let module = spec
            .modules
            .iter()
            .find(|module| module.name == "crypto")
            .expect("crypto module");
        let function = module
            .sig
            .functions
            .iter()
            .find(|function| function.name == "ed25519_verify")
            .expect("Ed25519 verifier");
        assert_eq!(function.overloads.len(), 1);

        let signature = &function.overloads[0];
        assert_eq!(
            signature
                .params
                .iter()
                .map(|param| (param.name, param.ty.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("message", Type::Bytes),
                ("public_key", Type::Bytes),
                ("signature", Type::Bytes),
            ]
        );
        assert_eq!(signature.return_ty, result(Type::Bool));
        assert!(signature.params.iter().all(|param| !param.defaulted));
        assert!(signature.pure);
        assert_eq!(signature.op, RuntimeOp::CryptoEd25519Verify);
        let docs = spec
            .docs("module.crypto.ed25519_verify")
            .expect("Ed25519 verifier docs");
        assert!(docs.contract.contains("message bytes unchanged"));
        assert!(docs.contract.contains("32 bytes"));
        assert!(docs.contract.contains("64 bytes"));
    }

    #[test]
    fn record_and_language_registry_docs_are_complete() {
        for name in records::record_schemas().keys() {
            let docs = super::record_docs(name);
            assert!(!docs.summary.trim().is_empty(), "record.{name}");
            assert!(
                docs.tags.iter().all(|tag| !tag.trim().is_empty()),
                "record.{name}"
            );
        }

        let references = reference::language_references();
        for reference in &references {
            assert!(!reference.id.trim().is_empty());
            assert!(
                !reference.docs.summary.trim().is_empty(),
                "{}",
                reference.id
            );
            assert!(
                reference.docs.tags.iter().all(|tag| !tag.trim().is_empty()),
                "{}",
                reference.id
            );
        }
        let mut ids = references
            .iter()
            .map(|reference| &reference.id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            references.len(),
            "language reference IDs must be unique"
        );
    }
    #[test]
    fn method_families_are_closed_over_canonical_signatures() {
        let spec = api_spec();
        for (receiver, name, family) in [
            (super::MethodReceiver::List, "len", super::MethodFamilyId::Length),
            (super::MethodReceiver::Map, "len", super::MethodFamilyId::Length),
            (super::MethodReceiver::Set, "len", super::MethodFamilyId::Length),
            (super::MethodReceiver::Bytes, "len", super::MethodFamilyId::Length),
            (super::MethodReceiver::List, "is_empty", super::MethodFamilyId::Emptiness),
            (super::MethodReceiver::Map, "is_empty", super::MethodFamilyId::Emptiness),
            (super::MethodReceiver::Set, "is_empty", super::MethodFamilyId::Emptiness),
            (super::MethodReceiver::Bytes, "is_empty", super::MethodFamilyId::Emptiness),
            (super::MethodReceiver::Str, "is_empty", super::MethodFamilyId::Emptiness),
        ] {
            let overloads = method_overloads(spec, receiver, name).expect("registered family member");
            assert!(!overloads.is_empty());
            for index in 0..overloads.len() {
                assert_eq!(spec.method_family_id(receiver, name, index), Some(family));
            }
        }
        let writes = method_overloads(spec, super::MethodReceiver::Path, "write").unwrap();
        assert_eq!(writes.len(), 4);
        for index in 0..writes.len() {
            assert_eq!(spec.method_family_id(super::MethodReceiver::Path, "write", index), Some(super::MethodFamilyId::PathWrite));
        }
        assert!(method_overloads(spec, super::MethodReceiver::Str, "len").is_none());
        for receiver in [super::MethodReceiver::Str, super::MethodReceiver::Bytes] {
            assert!(method_overloads(spec, receiver, "byte_at").unwrap().iter().enumerate()
                .all(|(index, _)| spec.method_family_id(receiver, "byte_at", index).is_none()));
        }
        for receiver in &spec.methods {
            for method in &receiver.methods {
                for (overload, signature) in method.overloads.iter().enumerate() {
                    let id = spec.registered_method_id(receiver.receiver, method.name, overload).unwrap();
                    let (actual_receiver, actual_name, actual_overload, actual_signature) = spec.registered_method(id).unwrap();
                    assert_eq!(actual_receiver, receiver.receiver);
                    assert_eq!(actual_name, method.name);
                    assert_eq!(actual_overload, overload);
                    assert!(std::ptr::eq(actual_signature, signature));
                }
            }
        }
        for family in [
            super::MethodFamilyId::Length,
            super::MethodFamilyId::Emptiness,
            super::MethodFamilyId::PathWrite,
        ] {
            let members = spec.method_family_member_ids(family);
            assert!(!members.is_empty());
            for &id in members {
                let (receiver, name, overload, signature) = spec.registered_method(id).unwrap();
                assert_eq!(spec.method_family_id(receiver, name, overload), Some(family));
                assert!(std::ptr::eq(signature, &method_overloads(spec, receiver, name).unwrap()[overload]));
            }
        }
    }
}
