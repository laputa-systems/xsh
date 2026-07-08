use crate::records::{
    archive_entry_type, diff_result_type, dns_host_type, dns_lookup_type, elf_info_type,
    env_entry_type, env_path_entry_type, fs_copy_tree_result_type, fs_entry_type,
    fs_filesystem_stats_type, fs_lock_type, fs_mount_type, fs_remove_manifest_result_type,
    fs_root_type, group_record_type, linux_blkid_type, linux_block_device_type,
    linux_disk_usage_type, linux_file_attrs_type, linux_fsck_type, linux_interface_type,
    linux_loop_device_type, linux_meminfo_type, linux_modinfo_type, linux_module_type,
    linux_open_file_type, linux_partition_table_type, linux_rfkill_type, linux_route_type,
    linux_uevent_type, measured_command_type, mime_info_type, mime_parse_type, net_pool_type,
    net_response_type, patch_result_type, process_entry_type, process_port_type,
    process_stats_type, process_thread_type, process_wait_any_type, regex_match_type,
    signal_record_type, spawn_record_type, system_memory_type, system_os_release_type,
    test_call_type, test_context_type, test_script_output_type, uname_record_type,
    unix_child_event_type, unix_id_type, unix_kill_all_result_type, unix_logged_process_group_type,
    unix_pid1_event_type, unix_pid1_shutdown_type, unix_spawned_child_type, unix_tty_attrs_type,
    user_record_type,
};
pub use crate::runtime_op::RuntimeOp;
pub(in crate::signature) use crate::types::Type;
pub(in crate::signature) use std::collections::BTreeMap;
use std::sync::OnceLock;

mod builders;
mod methods;
mod modules;
mod streams;

pub(in crate::signature) use builders::command_callable;
pub(in crate::signature) use methods::value_methods;
pub(in crate::signature) use modules::build_api_spec;

#[derive(Clone, Debug)]
pub struct ApiSpec {
    pub modules: Vec<ModuleEntry>,
    pub methods: Vec<MethodReceiverSig>,
}

impl ApiSpec {
    pub fn new(modules: Vec<ModuleEntry>, methods: Vec<MethodReceiverSig>) -> Self {
        Self { modules, methods }
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
        op,
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
        op,
    }
}

pub fn param(name: &'static str, ty: Type) -> ParamSig {
    ParamSig {
        name,
        ty,
        defaulted: false,
    }
}

pub fn default_param(name: &'static str, ty: Type) -> ParamSig {
    ParamSig {
        name,
        ty,
        defaulted: true,
    }
}

pub fn result(ok: Type) -> Type {
    Type::Result(Box::new(ok), Box::new(Type::Error))
}
