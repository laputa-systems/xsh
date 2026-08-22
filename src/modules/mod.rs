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
#[allow(clippy::single_call_fn)]
#[allow(dead_code)]
pub(crate) fn render_standard_module_contract() -> String {
    let modules = standard_modules();
    let mut output = String::new();
    let mut names = modules.keys().collect::<Vec<_>>();
    names.sort_unstable();

    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str("module ");
        output.push_str(name);
        output.push('\n');
        let module = &modules[*name];
        let mut functions = module.functions.iter().collect::<Vec<_>>();
        functions.sort_unstable_by(|left, right| left.name.cmp(right.name));
        for function in functions {
            for sig in &function.overloads {
                output.push_str("  ");
                output.push_str(function.name);
                output.push('(');
                for (index, param) in sig.params.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(param.name);
                    output.push_str(": ");
                    output.push_str(&render_type(&param.ty));
                    if param.defaulted {
                        output.push_str(" = default");
                    }
                }
                output.push_str(") -> ");
                output.push_str(&render_type(&sig.return_ty));
                output.push(' ');
                output.push_str(if sig.pure { "pure" } else { "effect" });
                output.push('\n');
            }
        }
    }
    output
}

#[cfg(test)]
#[allow(dead_code)]
fn render_type(ty: &crate::sema::types::Type) -> String {
    use crate::sema::types::Type;

    match ty {
        Type::Any => "Any".to_string(),
        Type::Unknown => "Unknown".to_string(),
        Type::Invalid => "<invalid>".to_string(),
        Type::Null => "Null".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::Duration => "Duration".to_string(),
        Type::Str => "Str".to_string(),
        Type::Bytes => "Bytes".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Regex => "Regex".to_string(),
        Type::Path => "Path".to_string(),
        Type::List(inner) => format!("List[{}]", render_type(inner)),
        Type::Map(inner) => format!("Map[{}]", render_type(inner)),
        Type::Stream(inner) => format!("Stream[{}]", render_type(inner)),
        Type::Record(fields) if fields.is_empty() => "Record".to_string(),
        Type::Record(fields) => {
            let fields = fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", render_type(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{fields}}}")
        }
        Type::Module(_) => "Module".to_string(),
        Type::Result(ok, err) => format!("Result[{}, {}]", render_type(ok), render_type(err)),
        Type::Status => "Status".to_string(),
        Type::EnvPathList => "EnvPathList".to_string(),
        Type::Error => "Error".to_string(),
        Type::ErrorFamily(name) => name.to_string(),
        Type::ErrorVariant { family, variant } => format!("{family}.{variant}"),
        Type::ErrorFacet(name) => name.to_string(),
        Type::ProcessError => "ProcessError".to_string(),
        Type::Pure => "Pure".to_string(),
        Type::Proc => "Proc".to_string(),
        Type::Command => "Command".to_string(),
        Type::ProcessHandle => "ProcessHandle".to_string(),
        Type::NetJob => "NetJob".to_string(),
        Type::Unit => "Unit".to_string(),
        Type::Tag(name) => name.to_string(),
        Type::Optional(inner) => format!("{}?", render_type(inner)),
    }
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
        assert!(modules.contains_key("shlex"));
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
        assert_eq!(
            only_overload(modules["fs"].function_overloads("read_text").unwrap()).return_ty,
            Type::Result(Box::new(Type::Str), Box::new(Type::Error))
        );
        assert!(
            modules["fs"]
                .function_overloads("write")
                .unwrap()
                .iter()
                .any(|sig| sig.params[1].ty == Type::Bytes)
        );
        assert!(
            modules["fs"]
                .function_overloads("write")
                .unwrap()
                .iter()
                .any(|sig| sig.params[1].ty == Type::Str)
        );
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
        assert!(modules["record"].function_overloads("get").is_none());
        assert!(modules["record"].function_overloads("has").is_none());
        assert!(modules["record"].function_overloads("keys").is_none());
        assert!(modules["record"].function_overloads("require").is_some());
        assert!(
            only_overload(modules["fs"].function_overloads("mkdir").unwrap())
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
