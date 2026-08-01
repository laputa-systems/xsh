use super::{ApiDocs, ApiNavigation, MethodReceiver, MethodReceiverSig, ModuleEntry};
use std::collections::BTreeMap;

pub fn module_api_id(module: &str, function: &str) -> String {
    format!("module.{module}.{function}")
}

pub fn method_api_id(receiver: MethodReceiver, method: &str) -> String {
    format!("method.{}.{}", receiver_name(receiver), method)
}

pub fn receiver_name(receiver: MethodReceiver) -> &'static str {
    match receiver {
        MethodReceiver::PathConstructor => "Path",
        MethodReceiver::Result => "Result",
        MethodReceiver::EnvPathList => "EnvPathList",
        MethodReceiver::Path => "Path",
        MethodReceiver::Int => "Int",
        MethodReceiver::Float => "Float",
        MethodReceiver::List => "List",
        MethodReceiver::Map => "Map",
        MethodReceiver::Record => "Record",
        MethodReceiver::Stream => "Stream",
        MethodReceiver::Str => "Str",
        MethodReceiver::Bytes => "Bytes",
        MethodReceiver::Status => "Status",
        MethodReceiver::Digest => "Digest",
        MethodReceiver::Regex => "Regex",
        MethodReceiver::ProcessHandle => "ProcessHandle",
    }
}

pub(super) fn build_api_docs(
    modules: &[ModuleEntry],
    methods: &[MethodReceiverSig],
) -> BTreeMap<String, ApiDocs> {
    let mut docs = BTreeMap::new();
    for module in modules {
        insert(
            &mut docs,
            format!("module.{}", module.name),
            module_docs(module.name),
        );
        for function in &module.sig.functions {
            insert(
                &mut docs,
                module_api_id(module.name, function.name),
                module_function_docs(module.name, function.name),
            );
        }
    }
    for receiver in methods {
        for method in &receiver.methods {
            insert(
                &mut docs,
                method_api_id(receiver.receiver, method.name),
                method_docs(receiver.receiver, method.name),
            );
        }
    }
    docs
}

fn insert(docs: &mut BTreeMap<String, ApiDocs>, id: String, value: ApiDocs) {
    assert!(
        docs.insert(id.clone(), value).is_none(),
        "duplicate API documentation for '{id}'"
    );
}

fn module_docs(module: &str) -> ApiDocs {
    let (summary, contract) = match module {
        "applet" => (
            "Internal primitives for shipped core applet scripts.",
            "This module is reserved for the maintained core applets rather than general scripts.",
        ),
        "archive" => (
            "Archive creation, extraction, listing, compression, and decompression.",
            "Extraction APIs preserve their rooted destination boundary and reject unsafe archive paths.",
        ),
        "bytes" => (
            "Byte inspection, encoding, decoding, copying, and hashing helpers.",
            "Keep binary data as Bytes until an explicit text or display boundary.",
        ),
        "cli" => (
            "Script command-line parsing into typed option records.",
            "Descriptors define the stable script-facing command-line contract.",
        ),
        "cpu" => ("CPU capability queries.", ""),
        "diff" => ("Unified diff generation.", ""),
        "dns" => (
            "DNS lookup and name resolution helpers.",
            "Network and lookup failures remain typed error data.",
        ),
        "elf" => (
            "ELF file-format inspection and dynamic dependency metadata.",
            "",
        ),
        "env" => (
            "Environment variable and PATH manipulation.",
            "Environment changes are explicit values or lexical overlays.",
        ),
        "fs" => (
            "Filesystem reads, writes, metadata, links, permissions, locking, and installation.",
            "Filesystem APIs use typed Path values and return structured records at metadata boundaries.",
        ),
        "group" => (
            "Unix group lookup records.",
            "Host lookup failures remain typed error data.",
        ),
        "hash" => (
            "Digest calculation and checksum verification.",
            "Hash file contents as bytes, not formatted text.",
        ),
        "ini" => ("INI decoding, encoding, and file helpers.", ""),
        "io" => ("Script stdin and stdout helpers.", ""),
        "json" => (
            "JSON encoding, decoding, files, and streams.",
            "Parsed JSON is dynamic; require a schema before trusting fields.",
        ),
        "linux" => (
            "Linux-specific boot, mount, device, and shutdown operations.",
            "This is a narrow privileged surface with platform and host-global constraints.",
        ),
        "list" => ("List collection helpers.", ""),
        "map" => ("Map collection helpers.", ""),
        "set" => ("String-key set helpers backed by Map[Bool].", ""),
        "mime" => ("MIME type lookup and media-type parsing helpers.", ""),
        "module" => (
            "User module loading helpers.",
            "Dynamic module values require an explicit contract before use.",
        ),
        "net" => (
            "HTTP request, transfer, and connection-pool helpers.",
            "Network failures remain typed error data.",
        ),
        "patch" => (
            "Rooted patch application.",
            "Patch paths are constrained to the supplied root.",
        ),
        "path" => (
            "Path normalization and resolution.",
            "Path conversion is explicit at string and filesystem boundaries.",
        ),
        "process" => (
            "Process discovery, command construction, execution, spawning, and signals.",
            "Process status data and runtime failures are distinct contracts.",
        ),
        "record" => ("Record inspection helpers.", ""),
        "regex" => (
            "Regex compilation, matching, captures, and replacement.",
            "",
        ),
        "shlex" => (
            "POSIX-like shell word rendering helpers.",
            "Rendering does not create implicit shell execution.",
        ),
        "system" => ("Host system identity records.", ""),
        "test" => (
            "Native XSH test assertions, temp resources, and host-effect mocks.",
            "Use native tests for focused XSH behavior coverage.",
        ),
        "text" => (
            "Text splitting, joining, replacement, counting, and character transforms.",
            "Text APIs operate on UTF-8 Str values.",
        ),
        "time" => (
            "Clock, sleep, command measurement, and Jiff strtime formatting.",
            "",
        ),
        "tui" => (
            "Terminal styling, control sequences, and width-aware text padding.",
            "",
        ),
        "unix" => (
            "Unix process-group, PID 1, hostname, uptime, exec, and reaping helpers.",
            "Unix host operations have platform, privilege, and process-lifecycle constraints.",
        ),
        "user" => (
            "Unix user lookup records.",
            "Host lookup failures remain typed error data.",
        ),
        "utils" => ("Process-scoped utility helpers.", ""),
        _ => panic!("missing module documentation for '{module}'"),
    };
    ApiDocs {
        summary: summary.to_string(),
        contract: contract.to_string(),
        tags: vec![module.to_string()],
        navigation: module_navigation(module),
    }
}

fn module_function_docs(module: &str, function: &str) -> ApiDocs {
    let mut docs = module_docs(module);
    docs.summary = match (module, function) {
        ("net", "request_many") => "Returns ordered request results.".to_string(),
        ("net", "download_many") => "Returns ordered download results.".to_string(),
        _ => format!(
            "{} `{function}` operation.",
            docs.summary.trim_end_matches('.')
        ),
    };
    docs.contract = match (module, function) {
        ("json", "read" | "decode" | "lines") => {
            "The returned value is dynamic; require a schema before field access.".to_string()
        }
        ("archive", "tar_extract" | "cpio_extract" | "zip_extract") => {
            "Extraction is rooted at the supplied destination and rejects unsafe paths.".to_string()
        }
        ("patch", "apply") => "Patch paths are constrained to the supplied root.".to_string(),
        ("process", "spawn" | "wait") => {
            "Process handles have explicit ownership and lifecycle semantics.".to_string()
        }
        _ => docs.contract,
    };
    docs.tags.push(function.to_string());
    docs
}

fn method_docs(receiver: MethodReceiver, method: &str) -> ApiDocs {
    let receiver = receiver_name(receiver);
    ApiDocs {
        summary: format!("`{method}` method for `{receiver}` values."),
        contract: String::new(),
        tags: vec![receiver.to_ascii_lowercase(), method.to_string()],
        navigation: ApiNavigation {
            implementation: vec!["src/runtime/eval.rs".to_string()],
            tests: vec!["tests/xsh/stdlib/methods.xsh".to_string()],
            showcase: None,
        },
    }
}

fn module_navigation(module: &str) -> ApiNavigation {
    let implementation = match module {
        "archive" => "src/modules/archive/mod.rs".to_string(),
        "applet" => "src/runtime/eval/modules.rs".to_string(),
        "list" | "map" | "record" | "set" => "src/runtime/eval/modules.rs".to_string(),
        "test" => "src/runtime/eval.rs".to_string(),
        "utils" => "src/modules/mod.rs".to_string(),
        _ => "src/runtime/eval/modules.rs".to_string(),
    };
    let tests = match module {
        "applet" => "tests/xsh/stdlib/auth.xsh".to_string(),
        "cli" => "tests/xsh/stdlib/args.xsh".to_string(),
        "list" | "map" | "record" | "set" => "tests/xsh/stdlib/methods.xsh".to_string(),
        "test" => "tests/xsh/stdlib/test.xsh".to_string(),
        "utils" => "tests/xsh/stdlib/utils.xsh".to_string(),
        _ => format!("tests/xsh/stdlib/{module}.xsh"),
    };
    let showcase = match module {
        "json" => Some("examples/json.xsh".to_string()),
        "process" => Some("examples/processes.xsh".to_string()),
        "archive" | "patch" => Some("examples/release-package.xsh".to_string()),
        _ => None,
    };
    ApiNavigation {
        implementation: vec![implementation],
        tests: vec![tests],
        showcase,
    }
}
