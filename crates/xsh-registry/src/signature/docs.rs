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
        curated: true,
        tags: vec![module.to_string()],
        navigation: module_navigation(module),
    }
}

fn module_function_docs(module: &str, function: &str) -> ApiDocs {
    let mut docs = module_docs(module);
    if let Some((summary, contract, test)) = curated_function_docs(module, function) {
        docs.summary = summary.to_string();
        docs.contract = contract.to_string();
        docs.curated = true;
        docs.navigation.tests = vec![test.to_string()];
    } else {
        docs.contract = String::new();
        docs.curated = false;
    }
    docs.tags.push(function.to_string());
    docs
}

fn curated_function_docs(
    module: &str,
    function: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    match (module, function) {
        ("json", "read") => Some((
            "Reads one JSON document from a path.",
            "Successful parsing returns Any; require a schema before trusting fields.",
            "tests/xsh/stdlib/json.xsh::test_json_read_write_lines_and_paths",
        )),
        ("json", "decode") => Some((
            "Parses one JSON document from UTF-8 text.",
            "Successful parsing returns Any; require a schema before trusting fields.",
            "tests/xsh/stdlib/json.xsh::test_json_decode_type_patterns_and_public_boundaries",
        )),
        ("json", "write") => Some((
            "Writes one JSON-compatible value to a path.",
            "Serialization is explicit at the persistence boundary; unsupported values return an error.",
            "tests/xsh/stdlib/json.xsh::test_json_read_write_lines_and_paths",
        )),
        ("json", "write_lines") => Some((
            "Writes one JSON document per output line.",
            "Use this only for line-oriented interchange; each input value must be JSON-compatible.",
            "tests/xsh/stdlib/json.xsh::test_json_read_write_lines_and_paths",
        )),
        ("archive", "tar_create") => Some((
            "Creates a tar archive from paths rooted at a source directory.",
            "Archive members are supplied as typed paths; compression is selected explicitly or from the destination.",
            "tests/xsh/stdlib/archive.xsh::test_archive_tar_cpio_and_compression",
        )),
        ("archive", "tar_extract" | "cpio_extract" | "zip_extract") => Some((
            "Extracts an archive into a destination root.",
            "Unsafe member paths are rejected; extraction never treats archive paths as unrestricted host paths.",
            "tests/xsh/stdlib/archive.xsh::test_archive_tar_cpio_and_compression",
        )),
        ("archive", "tar_list" | "cpio_list" | "zip_list") => Some((
            "Lists archive members as structured stream records.",
            "Consume the stream directly when possible; collect only when random access is needed.",
            "tests/xsh/stdlib/archive.xsh::test_archive_tar_cpio_and_compression",
        )),
        ("archive", "compress" | "decompress" | "decompress_bytes") => Some((
            "Converts archive or file bytes through an explicit compression boundary.",
            "Keep binary data as Bytes until an explicit UTF-8 conversion is required.",
            "tests/xsh/stdlib/archive.xsh::test_archive_tar_cpio_and_compression",
        )),
        ("patch", "apply") => Some((
            "Applies a unified patch below an explicit root.",
            "Patch paths stay rooted at the supplied directory and traversal escapes are rejected.",
            "tests/xsh/stdlib/patch.xsh::test_patch_apply",
        )),
        ("fs", "lock") => Some((
            "Acquires a filesystem lock and returns an explicit lock record.",
            "The returned lock must be released with fs.unlock; nonblocking acquisition reports contention as data.",
            "tests/xsh/stdlib/fs.xsh::test_filesystem_path_and_install_apis",
        )),
        ("fs", "tempdir" | "tempfile") => Some((
            "Creates a temporary resource under XSH ownership.",
            "Close or remove the returned resource with defer as soon as it is created.",
            "tests/xsh/stdlib/fs.xsh::test_filesystem_path_and_install_apis",
        )),
        ("fs", "open_root" | "root") => Some((
            "Creates or accesses a rooted filesystem capability.",
            "Use rooted operations when a workflow must not escape its destination tree.",
            "tests/xsh/stdlib/fs.xsh::test_fs_tree_metadata_install_and_locking",
        )),
        ("fs", "files" | "dirs" | "walk") => Some((
            "Produces lazy structured filesystem entries.",
            "Order and traversal behavior are explicit in the options; use stream terminals to choose materialization.",
            "tests/xsh/stdlib/streams.xsh::test_structured_streams_walk_filter_map_collect_and_count",
        )),
        ("fs", "install" | "install_as") => Some((
            "Installs a file with explicit destination and mode policy.",
            "Use this instead of separately creating parents, copying, chmodding, and touching a destination.",
            "tests/xsh/stdlib/fs.xsh::test_filesystem_path_and_install_apis",
        )),
        ("fs", "write_atomic") => Some((
            "Replaces a file through an atomic write path.",
            "Use when readers must not observe a partially written replacement.",
            "tests/xsh/stdlib/fs.xsh::test_fs_tree_metadata_install_and_locking",
        )),
        ("process", "command") => Some((
            "Builds a typed command plan without starting it.",
            "The plan captures argv, cwd, environment, and redirection before execution or spawn.",
            "tests/xsh/stdlib/process.xsh::test_process_command_redirections",
        )),
        ("process", "spawn") => Some((
            "Starts a typed command and returns an owned process handle record.",
            "The handle has lexical cleanup ownership until wait, cancel, detach, or transfer changes that lifecycle.",
            "tests/xsh/stdlib/process.xsh::test_process_wait_and_handle_contracts",
        )),
        ("process", "wait") => Some((
            "Waits for one or more owned process handles.",
            "Waiting consumes the handle lifecycle and returns status data instead of treating nonzero exits as runtime errors.",
            "tests/xsh/stdlib/process.xsh::test_process_wait_and_handle_contracts",
        )),
        ("cli", "parse") => Some((
            "Parses script arguments into a typed option record.",
            "The descriptor record is the command-line contract; validate defaults and repeated/positional fields there.",
            "tests/xsh/stdlib/args.xsh::test_cli_parse_advanced_descriptors",
        )),
        ("cli", "applet") => Some((
            "Parses BusyBox-style applet arguments and compact option forms.",
            "Use for shipped command scripts that need compatibility flags rather than inventing local argv parsing.",
            "tests/xsh/stdlib/args.xsh::test_cli_applet_parses_sort_cluster_and_attached_values",
        )),
        ("cli", "commands") => Some((
            "Dispatches a typed subcommand schema.",
            "Command and fallback descriptors own positional conversion and rest-argument behavior.",
            "tests/xsh/stdlib/args.xsh::test_cli_commands_accept_aliases_forms_and_options",
        )),
        ("module", "load") => Some((
            "Loads a documented XSH module and returns its export record.",
            "Runtime-loaded modules must have a ##! module doc and ## docs on every export before they are checked or lowered.",
            "tests/xsh/stdlib/module.xsh::test_module_load",
        )),
        ("hash", "sha256" | "sha512" | "sha1" | "md5") => Some((
            "Calculates a digest from bytes or a file path.",
            "Hash bytes at the content boundary; format the digest only for storage, display, or comparison.",
            "tests/xsh/stdlib/hash.xsh",
        )),
        ("net", "request") => Some((
            "Performs one structured HTTP request.",
            "Network, timeout, and response failures remain typed error data; do not collapse them into booleans.",
            "tests/xsh/stdlib/net.xsh",
        )),
        ("net", "request_many" | "download_many") => Some((
            "Executes a bounded batch of network operations with ordered results.",
            "Batch ordering is preserved even though transport work is concurrent.",
            "tests/xsh/stdlib/net.xsh",
        )),
        _ => None,
    }
}

fn curated_method_docs(
    receiver: &str,
    method: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    match (receiver, method) {
        ("Path", "resolve") => Some((
            "Resolves a path through the filesystem.",
            "Use resolve only when symlink-aware canonical host resolution is required; it may fail for missing paths.",
            "tests/xsh/stdlib/path.xsh::test_path_methods",
        )),
        ("Path", "read_text") => Some((
            "Reads a UTF-8 file into Str.",
            "Use read_bytes when invalid or opaque byte content is valid input.",
            "tests/xsh/stdlib/path.xsh::test_path_methods",
        )),
        ("Path", "write_atomic") => Some((
            "Atomically replaces a path with text or bytes.",
            "Use when readers must not observe a partial update.",
            "tests/xsh/stdlib/fs.xsh::test_fs_tree_metadata_install_and_locking",
        )),
        ("Path", "relative_to") => Some((
            "Computes a path relative to an explicit base.",
            "The result is an error when the path is not below the requested base.",
            "tests/xsh/stdlib/path.xsh::test_path_methods",
        )),
        ("Result", "context") => Some((
            "Adds a domain-specific error context before propagation.",
            "Use at a boundary where the caller needs a stable error kind and message, not at every call site.",
            "tests/xsh/run.xsh",
        )),
        ("Stream", "collect") => Some((
            "Materializes a stream into a list.",
            "Collect only at a random-access boundary; preserve streaming for large or unbounded sources.",
            "tests/xsh/stdlib/streams.xsh::test_structured_streams_walk_filter_map_collect_and_count",
        )),
        ("Bytes", "utf8") => Some((
            "Decodes bytes as UTF-8 text.",
            "Invalid byte sequences are an error; keep Bytes when text validity is not guaranteed.",
            "tests/xsh/stdlib/bytes.xsh",
        )),
        _ => None,
    }
}

fn method_docs(receiver: MethodReceiver, method: &str) -> ApiDocs {
    let receiver = receiver_name(receiver);
    let (summary, contract, curated, tests) = match curated_method_docs(receiver, method) {
        Some((summary, contract, test)) => (summary.to_string(), contract.to_string(), true, vec![test.to_string()]),
        None => (format!("`{method}` method for `{receiver}` values."), String::new(), false, vec!["tests/xsh/stdlib/methods.xsh".to_string()]),
    };
    ApiDocs {
        summary,
        contract,
        curated,
        tags: vec![receiver.to_ascii_lowercase(), method.to_string()],
        navigation: ApiNavigation {
            implementation: vec!["src/runtime/eval.rs".to_string()],
            tests,
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
