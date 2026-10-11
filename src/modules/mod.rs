pub(crate) mod archive;
pub(crate) mod bytes;
pub(crate) mod cli;
pub(crate) mod compression;
pub(crate) mod cpu;
pub(crate) mod diff;
pub(crate) mod dns;
pub(crate) mod elf;
pub mod fs;
pub(crate) mod group;
pub(crate) mod hash;
pub mod ini;
pub mod json;
pub(crate) mod linux;
pub(crate) mod mime;
pub(crate) mod net;
pub(crate) mod numeric;
pub(crate) mod locale;
pub(crate) mod patch;
pub(crate) mod process;
pub(crate) mod regex;
pub(crate) mod shlex;
pub mod signature;
pub(crate) mod system;
pub(crate) mod text;
pub(crate) mod time;
pub(crate) mod tui;
pub(crate) mod unix;
pub(crate) mod user;

pub use signature::{ApiArgCheck, MethodReceiver, MethodSig, ModuleFnSig, RuntimeOp, api_spec};
pub use xsh_registry::ManagedResource;

/// The resource kind that manages a value of the checked type `ty`. Only the
/// exact type of a kind is one: an unresolved or dynamic type is not.
pub(crate) fn managed_resource_for(ty: &crate::sema::types::Type) -> Option<ManagedResource> {
    ManagedResource::ALL
        .into_iter()
        .filter(|kind| kind.release_op().is_some())
        .find(|kind| signature::convert_type(&kind.resource_type()) == *ty)
}

#[cfg(test)]
use rustc_hash::FxHashMap;

#[cfg(test)]
pub(crate) fn standard_modules() -> FxHashMap<&'static str, signature::ModuleSig> {
    api_spec()
        .module_entries()
        .map(|(name, module)| (name, module.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{api_spec, standard_modules};
    use crate::modules::ModuleFnSig;
    use crate::sema::types::Type;

    #[test]
    fn module_signatures_keep_expected_boundaries() {
        let modules = standard_modules();
        let module_names = api_spec().module_names().collect::<Vec<_>>();

        assert_eq!(modules.len(), module_names.len());
        assert_eq!(api_spec().module_entries().count(), module_names.len());
        for name in module_names {
            assert!(api_spec().is_standard_module(name));
            assert!(modules.contains_key(name));
        }
        assert!(!api_spec().is_standard_module("pm"));
        assert!(!api_spec().is_standard_module("build"));
        assert!(!modules.contains_key("pm"));
        assert!(!modules.contains_key("build"));
        assert!(modules.contains_key("archive"));
        assert!(modules.contains_key("applet"));
        assert!(modules.contains_key("fs"));
        assert!(modules.contains_key("cli"));
        assert!(modules.contains_key("path"));
        assert!(modules.contains_key("env"));
        assert!(modules.contains_key("hash"));
        assert!(modules.contains_key("ini"));
        assert!(modules.contains_key("mime"));
        assert!(modules.contains_key("cpu"));
        assert!(modules.contains_key("dns"));
        assert!(modules.contains_key("elf"));
        assert!(modules.contains_key("net"));
        assert!(modules.contains_key("time"));
        assert!(modules.contains_key("unix"));
        assert!(modules.contains_key("system"));
        assert!(modules.contains_key("test"));
        assert!(modules.contains_key("set"));
        assert!(modules.contains_key("user"));
        assert!(modules.contains_key("group"));
        assert!(modules["fs"].function_overloads("du").is_none());
        assert!(modules["fs"].function_overloads("read").is_none());
        assert!(modules["fs"].function_overloads("read_bytes").is_none());
        assert!(modules["fs"].function_overloads("readlink").is_none());
        assert!(modules["fs"].function_overloads("remove_dir").is_none());
        assert!(modules["fs"].function_overloads("touch").is_none());
        assert!(modules["fs"].function_overloads("truncate").is_none());
        assert!(modules["fs"].function_overloads("unlink").is_none());
        assert!(modules["fs"].function_overloads("hardlink").is_none());
        // An operation on one path is a `Path` method only; the mode test
        // has no path to be called on and is still a function.
        for removed in xsh_registry::signature::REMOVED_FS_PATH_FUNCTIONS {
            let remaining = modules["fs"].function_overloads(removed);
            if removed == "executable" {
                assert_eq!(only_overload(remaining.unwrap()).params[0].ty, Type::Int);
            } else {
                assert!(remaining.is_none(), "{removed}");
            }
        }
        assert!(
            modules["hash"]
                .function_overloads("sha256")
                .unwrap()
                .iter()
                .any(|sig| sig.pure && sig.params[0].ty == Type::Bytes)
        );
        assert!(
            modules["hash"]
                .function_overloads("sha256")
                .unwrap()
                .iter()
                .any(|sig| !sig.pure && sig.params[0].ty == Type::Path)
        );
        assert!(modules["path"].function_overloads("display").is_none());
        assert!(modules["env"].function_overloads("get_path").is_none());
        assert!(modules["json"].function_overloads("lines").is_none());
        assert!(modules["json"].function_overloads("stream").is_none());
        assert!(!modules.contains_key("record"));
        assert!(
            only_overload(modules["fs"].function_overloads("copy_tree").unwrap())
                .params
                .iter()
                .any(|param| param.name == "parents" && param.defaulted)
        );
    }

    fn only_overload(overloads: &[ModuleFnSig]) -> &ModuleFnSig {
        assert_eq!(overloads.len(), 1);
        &overloads[0]
    }
}
