pub use crate::api_docs::ApiDocs;
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
mod docs;
mod methods;
mod modules;
mod streams;

pub(in crate::signature) use builders::command_callable;
pub use docs::{method_api_id, module_api_id, receiver_name};
pub(in crate::signature) use methods::value_methods;
pub(in crate::signature) use modules::build_api_spec;
pub use modules::record_docs;

#[derive(Clone, Debug)]
pub struct ApiSpec {
    pub modules: Vec<ModuleEntry>,
    pub methods: Vec<MethodReceiverSig>,
    docs: BTreeMap<String, ApiDocs>,
}

impl ApiSpec {
    pub fn new(modules: Vec<ModuleEntry>, methods: Vec<MethodReceiverSig>) -> Self {
        let docs = docs::build_api_docs(&modules, &methods);
        let spec = Self {
            modules,
            methods,
            docs,
        };
        spec.validate_docs()
            .expect("standard API registry must have complete documentation");
        spec
    }

    pub fn docs(&self, id: &str) -> Option<&ApiDocs> {
        self.docs.get(id)
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

#[cfg(test)]
mod tests {
    use super::api_spec;
    use crate::{records, reference};

    #[test]
    fn public_api_items_have_complete_registry_docs() {
        api_spec()
            .validate_docs()
            .expect("public API docs should be complete");
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
}
