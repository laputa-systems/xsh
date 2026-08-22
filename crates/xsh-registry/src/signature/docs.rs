use super::{ApiDocs, MethodReceiver, MethodReceiverSig, ModuleEntry};
use std::collections::BTreeMap;

pub fn module_api_id(module: &str, function: &str) -> String {
    format!("module.{module}.{function}")
}

pub fn method_api_id(receiver: MethodReceiver, method: &str) -> String {
    format!("method.{}.{}", receiver_name(receiver), method)
}

/// Returns module functions that construct or initialize values of a type.
/// These associations enrich API discovery only; runtime dispatch is unchanged.
pub fn associated_module_functions(
    receiver: MethodReceiver,
) -> &'static [(&'static str, &'static str)] {
    match receiver {
        MethodReceiver::Map => &[("map", "empty")],
        _ => &[],
    }
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
        MethodReceiver::NetJob => "NetJob",
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
        "error" => (
            "Expected validation failure construction.",
            "error.fail returns validation Result data; propagating it requires the enclosing error effect.",
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
            "Clock, sleep, command measurement, and duration display.",
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
        example: crate::examples::source(&format!("module.{module}")),
        tags: vec![module.to_string()],
    }
}

fn module_function_docs(module: &str, function: &str) -> ApiDocs {
    let doc = function_doc(module, function)
        .unwrap_or_else(|| panic!("missing function documentation for {module}.{function}"));
    ApiDocs {
        summary: doc.summary.to_string(),
        contract: doc.contract.to_string(),
        example: crate::examples::source(&module_api_id(module, function)),
        tags: api_tags(module, function, doc.tags),
    }
}

#[derive(Clone, Copy)]
struct DocRow {
    summary: &'static str,
    contract: &'static str,
    tags: &'static [&'static str],
}

fn api_tags(scope: &str, name: &str, tags: &[&str]) -> Vec<String> {
    let mut result = Vec::new();
    for tag in std::iter::once(scope)
        .chain(std::iter::once(name))
        .chain(tags.iter().copied())
    {
        if !result.iter().any(|existing| existing == tag) {
            result.push(tag.to_string());
        }
    }
    result
}

fn function_doc(module: &str, function: &str) -> Option<DocRow> {
    let docs: Option<(&'static str, &'static str, &'static [&'static str])> = match (
        module, function,
    ) {
        ("json", "read") => Some((
            "Reads one JSON document from a path.",
            "Successful parsing returns Any; require a schema before trusting fields.",
            &["json", "file", "dynamic"],
        )),
        ("json", "decode") => Some((
            "Parses one JSON document from UTF-8 text.",
            "Successful parsing returns Any; require a schema before trusting fields.",
            &["json", "utf8", "dynamic"],
        )),
        ("json", "write") => Some((
            "Writes one JSON-compatible value to a path.",
            "Serialization is explicit at the persistence boundary; unsupported values return an error.",
            &["json", "file"],
        )),
        ("json", "write_lines") => Some((
            "Writes one JSON document per output line.",
            "Use this only for line-oriented interchange; each input value must be JSON-compatible.",
            &["json", "lines"],
        )),
        ("archive", "tar_create") => Some((
            "Creates a tar archive from paths rooted at a source directory.",
            "Archive members are supplied as typed paths; compression is selected explicitly or from the destination.",
            &["archive", "tar", "rooted"],
        )),
        ("archive", "cpio_create") => Some((
            "Creates a cpio archive from paths below a source root.",
            "Members are interpreted relative to the supplied root and the destination overwrite policy is explicit.",
            &["archive", "cpio", "rooted"],
        )),
        ("archive", "tar_extract" | "cpio_extract" | "zip_extract") => Some((
            "Extracts an archive into a destination root.",
            "Unsafe member paths are rejected; extraction never treats archive paths as unrestricted host paths.",
            &["archive", "extraction", "rooted"],
        )),
        ("archive", "tar_list" | "cpio_list" | "zip_list") => Some((
            "Lists archive members as structured stream records.",
            "Consume the stream directly when possible; collect only when random access is needed.",
            &["archive", "listing", "streaming"],
        )),
        ("archive", "compress" | "decompress" | "decompress_bytes") => Some((
            "Converts archive or file bytes through an explicit compression boundary.",
            "Keep binary data as Bytes until an explicit UTF-8 conversion is required.",
            &["archive", "compression", "bytes"],
        )),
        ("patch", "apply") => Some((
            "Applies a unified patch below an explicit root.",
            "Patch paths stay rooted at the supplied directory and traversal escapes are rejected.",
            &["patch", "rooted"],
        )),
        ("path", "absolute") => Some((
            "Makes a path absolute using the evaluator's current working directory.",
            "The conversion is lexical and does not require the target to exist or resolve symlinks.",
            &["path", "absolute", "cwd"],
        )),
        ("fs", "lock") => Some((
            "Acquires a filesystem lock and returns an explicit lock record.",
            "The returned lock must be released with fs.unlock; nonblocking acquisition reports contention as data.",
            &["filesystem", "locking", "ownership"],
        )),
        ("fs", "tempdir" | "tempfile") => Some((
            "Creates a temporary resource under XSH ownership.",
            "Close or remove the returned resource with defer as soon as it is created.",
            &["filesystem", "temporary", "ownership"],
        )),
        ("fs", "open_root" | "root") => Some((
            "Creates or accesses a rooted filesystem capability.",
            "Use rooted operations when a workflow must not escape its destination tree.",
            &["filesystem", "rooted", "capability"],
        )),
        ("fs", "files" | "walk") => Some((
            "Produces lazy structured filesystem entries.",
            "Order and traversal behavior are explicit in the options; hidden: false by default omits dot-prefixed files and directories, while hidden: true includes them. Use stream terminals to choose materialization.",
            &["filesystem", "streaming", "walk"],
        )),
        ("fs", "dirs") => Some((
            "Produces lazy structured filesystem entries.",
            "Order and traversal behavior are explicit in the options; use stream terminals to choose materialization.",
            &["filesystem", "streaming", "walk"],
        )),
        ("fs", "install" | "install_as") => Some((
            "Installs a file with explicit destination and mode policy.",
            "Use this instead of separately creating parents, copying, chmodding, and touching a destination.",
            &["filesystem", "install", "permissions"],
        )),
        ("process", "command") => Some((
            "Builds a typed command plan without starting it.",
            "The plan captures argv, cwd, environment, and redirection before execution or spawn.",
            &["process", "argv", "plan"],
        )),
        ("process", "spawn") => Some((
            "Starts a typed command and returns an owned process handle record.",
            "The handle has lexical cleanup ownership until wait, cancel, detach, or transfer changes that lifecycle.",
            &["process", "ownership", "handle"],
        )),
        ("process", "wait") => Some((
            "Waits for one or more owned process handles.",
            "Waiting consumes the handle lifecycle and returns status data instead of treating nonzero exits as runtime errors.",
            &["process", "ownership", "status-data"],
        )),
        ("cli", "parse") => Some((
            "Parses script arguments into a typed option record.",
            "The descriptor record is the command-line contract; validate defaults and repeated/positional fields there.",
            &["cli", "typed", "argv"],
        )),
        ("cli", "applet") => Some((
            "Parses BusyBox-style applet arguments and compact option forms.",
            "Use for shipped command scripts that need compatibility flags rather than inventing local argv parsing.",
            &["cli", "applet", "argv"],
        )),
        ("cli", "commands") => Some((
            "Dispatches a typed subcommand schema.",
            "Command and fallback descriptors own positional conversion and rest-argument behavior.",
            &["cli", "subcommands", "typed"],
        )),
        ("module", "load") => Some((
            "Loads a documented XSH module and returns its export record.",
            "Runtime-loaded modules must have a ##! module doc and ## docs on every export before they are checked or lowered.",
            &["module", "dynamic", "documentation"],
        )),
        ("hash", "sha256" | "sha512" | "sha1" | "md5") => Some((
            "Calculates a digest from bytes or a file path.",
            "Hash bytes at the content boundary; format the digest only for storage, display, or comparison.",
            &["hash", "digest", "bytes"],
        )),
        ("net", "request") => Some((
            "Performs one structured HTTP request.",
            "Network, timeout, and response failures remain typed error data; do not collapse them into booleans.",
            &["net", "http", "status-data"],
        )),
        ("net", "start") => Some((
            "Starts an owned HTTP request and returns its NetJob handle.",
            "The handle is evaluator-owned and must be consumed with wait or cancel; transport work never executes XSH code.",
            &["net", "http", "ownership", "job"],
        )),
        ("net", "request_many" | "download_many") => Some((
            "Executes a bounded batch of network operations with ordered results.",
            "Batch ordering is preserved even though transport work is concurrent.",
            &["net", "batch", "ordered", "bounded"],
        )),
        ("applet", "hash_password") => Some((
            "Creates a password hash using a named crypt algorithm.",
            "Unsupported algorithms and invalid password policy inputs return an error rather than a partial hash.",
            &["auth", "password", "hash"],
        )),
        ("applet", "verify_password") => Some((
            "Checks a password against a stored crypt hash.",
            "Malformed or unsupported hashes fail closed and return false.",
            &["auth", "password", "verification"],
        )),
        ("applet", "current_euid") => Some((
            "Returns the effective user ID of the running process.",
            "The value describes the host process credentials, not the XSH source user record.",
            &["auth", "identity", "host-state"],
        )),
        ("applet", "current_exe") => Some((
            "Returns the executable path of the running applet process.",
            "The host may report an error when the executable path cannot be resolved.",
            &["applet", "path", "host-state"],
        )),
        ("applet", "login_session" | "su_session" | "sulogin_session") => Some((
            "Starts a login-style session for a selected Unix user.",
            "This crosses a privilege and process boundary; the returned status is data and environment preservation is explicit.",
            &["auth", "privileged", "process"],
        )),
        ("applet", "mdev") => Some((
            "Runs the maintained mdev device-management applet.",
            "Device setup is host-global and platform-specific; use it only from the shipped applet workflow.",
            &["applet", "device", "privileged"],
        )),
        ("bytes", "zero") => Some((
            "Allocates a zero-filled byte buffer.",
            "The requested length controls allocation size; the result remains binary Bytes.",
            &["bytes", "allocation"],
        )),
        ("bytes", "from_ints") => Some((
            "Builds Bytes from integer byte values.",
            "Every integer must fit the byte range; invalid values return an error.",
            &["bytes", "conversion", "validation"],
        )),
        ("bytes", "from_text") => Some((
            "Encodes UTF-8 text as Bytes.",
            "The conversion is lossless for Str and does not append a terminator.",
            &["bytes", "utf8", "conversion"],
        )),
        ("bytes", "concat") => Some((
            "Concatenates a list of byte buffers.",
            "Inputs are copied into one owned result in list order.",
            &["bytes", "copy", "collection"],
        )),
        ("bytes", "human") => Some((
            "Formats a byte count for human-readable display.",
            "The result is presentation text; it is not a parseable replacement for the original count.",
            &["bytes", "display"],
        )),
        ("bytes", "pack_le" | "pack_be") => Some((
            "Packs an integer into fixed-width little- or big-endian bytes.",
            "The requested width and signedness determine the representation; values that do not fit return an error.",
            &["bytes", "encoding", "endian"],
        )),
        ("bytes", "unpack_le" | "unpack_be") => Some((
            "Unpacks fixed-width little- or big-endian bytes into an integer.",
            "The input length must match the requested width and the selected signedness is explicit.",
            &["bytes", "decoding", "endian"],
        )),
        ("bytes", "read_at" | "write_at" | "zero_at") => Some((
            "Reads, writes, or clears a byte range at an explicit offset.",
            "Offsets and lengths are bounds-checked; an out-of-range operation returns an error without partial mutation.",
            &["bytes", "offset", "bounds"],
        )),
        ("bytes", "copy") => Some((
            "Copies a byte range between buffers.",
            "Source and destination ranges are checked before copying so an invalid request cannot partially write the destination.",
            &["bytes", "copy", "bounds"],
        )),
        ("bytes", "copy_file") => Some((
            "Copies bytes between files with explicit range options.",
            "Filesystem failures and range validation remain errors at the file boundary.",
            &["bytes", "filesystem", "copy"],
        )),
        ("cli", "parse_full") => Some((
            "Parses the complete script argument schema including help and usage policy.",
            "The full descriptor remains the source of truth for conversion, defaults, and help behavior.",
            &["cli", "typed", "usage"],
        )),
        ("cli", "tokens") => Some((
            "Tokenizes command-line flags without executing them.",
            "Token boundaries preserve attached values and option clusters for the later typed parser.",
            &["cli", "argv", "tokens"],
        )),
        ("cli", "usage") => Some((
            "Renders usage text from a command-line descriptor.",
            "Usage is derived from the descriptor rather than a second hand-maintained option list.",
            &["cli", "usage", "documentation"],
        )),
        ("cpu", "count") => Some((
            "Reports the host CPU count available to the process.",
            "The value is a host capability observation and may differ from the machine-wide physical core count.",
            &["cpu", "host-state"],
        )),
        ("diff", "unified") => Some((
            "Builds a unified diff between two text paths.",
            "The result is structured summary data plus diff text; missing or unreadable inputs return an error.",
            &["diff", "text", "filesystem"],
        )),
        ("dns", "lookup") => Some((
            "Looks up one DNS record type for a name.",
            "Resolver failures remain typed errors and unsupported record types are not silently downgraded.",
            &["dns", "net", "lookup"],
        )),
        ("dns", "resolve_host") => Some((
            "Resolves a host name into address records.",
            "The resolver may return multiple addresses; callers must preserve the returned order only when their policy requires it.",
            &["dns", "net", "host"],
        )),
        ("dns", "reverse") => Some((
            "Performs reverse DNS lookup for an address.",
            "A missing PTR record is an error result rather than an invented host name.",
            &["dns", "net", "reverse"],
        )),
        ("dns", "nameservers") => Some((
            "Reads the host resolver nameserver configuration.",
            "The result reflects current host configuration and does not establish a persistent resolver policy.",
            &["dns", "host-state", "configuration"],
        )),
        ("elf", "inspect") => Some((
            "Inspects ELF headers and dynamic dependency metadata.",
            "Malformed files return structured errors; the parser does not execute or load the inspected object.",
            &["elf", "binary", "inspection"],
        )),
        ("error", "fail") => Some((
            "Constructs an expected validation failure as Result data.",
            "error.fail(message) returns Result[Unit, Error] with kind validation; propagating it requires the enclosing error effect.",
            &["error", "validation", "failure", "result"],
        )),
        ("env", "get") => Some((
            "Reads one environment variable as text.",
            "Missing variables and invalid host bytes remain distinguishable results.",
            &["env", "lookup", "utf8"],
        )),
        ("env", "get_or") => Some((
            "Reads an environment variable with an explicit fallback.",
            "The fallback is used only for absence; invalid encoding is not hidden by it.",
            &["env", "lookup", "fallback"],
        )),
        ("env", "bool" | "int" | "list") => Some((
            "Reads and converts one environment variable to a typed value.",
            "Conversion errors are returned as data, so malformed configuration cannot become a silent default.",
            &["env", "configuration", "typed"],
        )),
        ("env", "path" | "path_list" | "path_entries") => Some((
            "Reads a path-valued environment variable into typed path data.",
            "Empty components and platform path separators are preserved according to the explicit path-list contract.",
            &["env", "path", "configuration"],
        )),
        ("fs", "cwd") => Some((
            "Returns the evaluator's current working directory.",
            "The value is XSH runtime state and may differ from the host process cwd after a scoped cd.",
            &["filesystem", "cwd", "state"],
        )),
        ("fs", "project_root") => Some((
            "Finds the project root from a starting path.",
            "Root discovery follows the repository markers implemented by the host helper and fails when no root is found.",
            &["filesystem", "root", "discovery"],
        )),
        ("fs", "user_root") => Some((
            "Returns the current user's filesystem root path.",
            "The result follows host user configuration and is not a substitute for a caller-supplied security root.",
            &["filesystem", "user", "path"],
        )),
        ("fs", "gitroot") => Some((
            "Finds the Git worktree root containing a path.",
            "Repository discovery is filesystem state; a path outside a Git worktree returns an error.",
            &["filesystem", "git", "root"],
        )),
        ("fs", "ls" | "children") => Some((
            "Lists immediate filesystem children as structured entries.",
            "The operation is shallow; use walk or files when recursive traversal is intended.",
            &["filesystem", "listing", "streaming"],
        )),
        ("fs", "metadata") => Some((
            "Reads filesystem metadata into an FsEntry record.",
            "Metadata reflects the host at read time and follows the path's symlink and permission behavior.",
            &["filesystem", "metadata", "record"],
        )),
        ("fs", "filesystem_stats") => Some((
            "Reads filesystem capacity statistics for a path.",
            "Capacity fields are host filesystem observations and may change between calls.",
            &["filesystem", "metadata", "capacity"],
        )),
        ("fs", "mounts" | "mount_for") => Some((
            "Reads mounted-filesystem records for the host.",
            "Mount information is a host-global snapshot and may be unavailable on unsupported platforms.",
            &["filesystem", "mount", "host-state"],
        )),
        ("fs", "read_text") => Some((
            "Reads a UTF-8 file into Str.",
            "Invalid byte sequences are an error; use the byte API when opaque content is valid input.",
            &["filesystem", "read", "utf8"],
        )),
        ("fs", "write") => Some((
            "Writes text or bytes to a path.",
            "The input type selects the boundary explicitly and the write replaces the destination according to host filesystem policy.",
            &["filesystem", "write"],
        )),
        ("fs", "write_atomic") => Some((
            "Replaces a file through an atomic write path.",
            "Use when readers must not observe a partially written replacement.",
            &["filesystem", "atomic", "write"],
        )),
        ("fs", "mkdir") => Some((
            "Creates a directory with an explicit parent policy.",
            "The parents option controls whether missing ancestors are created; existing non-directories remain errors.",
            &["filesystem", "directory", "creation"],
        )),
        ("fs", "remove") => Some((
            "Removes a file or empty directory with an explicit missing policy.",
            "Missing paths are errors unless missing_ok is enabled; recursive deletion is not implied.",
            &["filesystem", "remove", "destructive"],
        )),
        ("fs", "remove_manifest") => Some((
            "Removes files and empty parents listed by a manifest.",
            "Manifest paths are cleaned and constrained before removal, and the result reports what was removed.",
            &["filesystem", "remove", "manifest"],
        )),
        ("fs", "copy") => Some((
            "Copies one file to a destination path.",
            "Overwrite behavior is explicit; source and destination errors are returned without pretending a partial copy succeeded.",
            &["filesystem", "copy"],
        )),
        ("fs", "copy_tree") => Some((
            "Copies a directory tree and returns copy statistics.",
            "Symlink and overwrite policy are explicit; the returned record describes completed entries.",
            &["filesystem", "copy", "tree"],
        )),
        ("fs", "rename") => Some((
            "Renames a path with an explicit overwrite policy.",
            "The operation is a host rename boundary; it does not silently copy across filesystems.",
            &["filesystem", "rename"],
        )),
        ("fs", "symlink") => Some((
            "Creates a symbolic link with explicit target and link paths.",
            "The target is stored as link text and is not required to exist at creation time.",
            &["filesystem", "symlink"],
        )),
        ("fs", "root_read" | "root_read_text") => Some((
            "Reads bytes or UTF-8 text below a rooted filesystem capability.",
            "The relative path is checked against the root and traversal or absolute escapes are rejected.",
            &["filesystem", "rooted", "read"],
        )),
        ("fs", "root_write" | "root_write_atomic") => Some((
            "Writes bytes or text below a rooted filesystem capability.",
            "The relative path cannot escape the root; the atomic variant protects readers from partial replacement.",
            &["filesystem", "rooted", "write", "atomic"],
        )),
        ("fs", "root_metadata" | "root_exists") => Some((
            "Inspects a path below a rooted filesystem capability.",
            "The root boundary is enforced before the host lookup, including for symlink-sensitive paths.",
            &["filesystem", "rooted", "metadata"],
        )),
        (
            "fs",
            "root_mkdir" | "root_remove" | "root_readlink" | "root_chmod" | "root_symlink"
            | "root_install_file",
        ) => Some((
            "Mutates a path below a rooted filesystem capability.",
            "Relative paths are validated against the root before the mutation and cannot address an outside destination.",
            &["filesystem", "rooted", "mutation"],
        )),
        ("fs", "root_path") => Some((
            "Returns the host path represented by a rooted filesystem capability.",
            "Treat the returned path as a diagnostic boundary; rooted operations remain the capability-safe interface.",
            &["filesystem", "rooted", "path"],
        )),
        ("fs", "close_root") => Some((
            "Closes an owned rooted filesystem capability.",
            "After close, further operations on the root are invalid and ownership must not be reused.",
            &["filesystem", "rooted", "ownership"],
        )),
        (
            "fs",
            "executable" | "world_writable" | "sticky" | "setuid" | "setgid" | "owner_executable"
            | "group_executable" | "other_executable",
        ) => Some((
            "Inspects one permission bit on a filesystem path.",
            "The result is metadata observed at the time of the call and does not change the path.",
            &["filesystem", "permissions", "metadata"],
        )),
        ("fs", "exists") => Some((
            "Checks whether a filesystem path exists.",
            "A false result describes absence; permission and other lookup failures remain errors.",
            &["filesystem", "lookup", "status-data"],
        )),
        ("fs", "du") => Some((
            "Calculates disk usage for a filesystem path.",
            "The count follows host filesystem allocation semantics rather than only logical file length.",
            &["filesystem", "usage", "metadata"],
        )),
        ("fs", "chown" | "chgrp") => Some((
            "Changes filesystem ownership metadata.",
            "The operation is privilege-sensitive and accepts explicit numeric or named ownership inputs as defined by its signature.",
            &["filesystem", "ownership", "privileged"],
        )),
        ("fs", "mkfifo") => Some((
            "Creates a named FIFO at a path.",
            "Creation is host-global filesystem state and fails when the path already conflicts with the requested node.",
            &["filesystem", "fifo", "privileged"],
        )),
        ("fs", "fsync" | "sync") => Some((
            "Flushes file or filesystem state to the host.",
            "The operation is a durability boundary; success reports the host call, not a cross-device durability guarantee.",
            &["filesystem", "durability"],
        )),
        ("fs", "unlock") => Some((
            "Releases a filesystem lock record.",
            "Unlock consumes the lock ownership; do not use the record after release.",
            &["filesystem", "locking", "ownership"],
        )),
        ("fs", "chmod") => Some((
            "Changes permission bits on a filesystem path.",
            "The supplied mode is applied as an explicit host permission value and may require privilege.",
            &["filesystem", "permissions", "privileged"],
        )),
        ("group", "current") => Some((
            "Returns the current process group record.",
            "The record reflects host identity state at lookup time.",
            &["group", "identity", "host-state"],
        )),
        ("group", "lookup" | "by_gid") => Some((
            "Looks up a Unix group by name or numeric ID.",
            "Missing entries and NSS failures remain typed lookup results.",
            &["group", "identity", "lookup"],
        )),
        ("group", "add" | "remove") => Some((
            "Adds or removes a Unix group entry.",
            "The mutation is privilege-sensitive and changes host account state; callers must handle the returned status explicitly.",
            &["group", "identity", "privileged"],
        )),
        ("hash", "crc32" | "crc32c") => Some((
            "Calculates a CRC checksum for bytes.",
            "CRC is an integrity check, not a cryptographic authenticity guarantee.",
            &["hash", "checksum", "bytes"],
        )),
        ("hash", "parse_check_line") => Some((
            "Parses one checksum-file verification line.",
            "The parser preserves the filename and expected digest so verification can remain an explicit later step.",
            &["hash", "checksum", "parsing"],
        )),
        ("hash", "verify_file") => Some((
            "Verifies a file against a named digest.",
            "A digest mismatch is status data; unreadable files and unsupported algorithms remain errors.",
            &["hash", "verification", "filesystem"],
        )),
        ("ini", "decode") => Some((
            "Parses INI text into a structured map.",
            "Section and key spelling follow the INI implementation; malformed input returns an error.",
            &["ini", "parsing", "text"],
        )),
        ("ini", "encode") => Some((
            "Encodes a structured map as INI text.",
            "Only values representable by the INI format are emitted; unsupported structures return an error.",
            &["ini", "encoding", "text"],
        )),
        ("ini", "read") => Some((
            "Reads and parses an INI file.",
            "Filesystem and parse failures remain errors at the file boundary.",
            &["ini", "filesystem", "parsing"],
        )),
        ("ini", "write") => Some((
            "Encodes and writes an INI file.",
            "Serialization occurs at the explicit path boundary and does not silently discard unsupported values.",
            &["ini", "filesystem", "encoding"],
        )),
        ("io", "stdin_bytes") => Some((
            "Reads all standard input as Bytes.",
            "The operation preserves arbitrary bytes and consumes the evaluator's stdin source.",
            &["io", "stdin", "bytes"],
        )),
        ("io", "stdin_text") => Some((
            "Reads all standard input as UTF-8 text.",
            "Invalid UTF-8 is an error rather than replacement text.",
            &["io", "stdin", "utf8"],
        )),
        ("io", "stdin_line") => Some((
            "Reads one line from standard input.",
            "Line termination is consumed according to the stream boundary and end-of-input remains distinguishable.",
            &["io", "stdin", "lines"],
        )),
        ("io", "write_stdout") => Some((
            "Writes UTF-8 text to standard output.",
            "Output is explicit I/O and the value is not implicitly converted through shell word rules.",
            &["io", "stdout", "utf8"],
        )),
        ("io", "write_stdout_bytes") => Some((
            "Writes raw Bytes to standard output.",
            "The operation preserves byte values and does not append a text newline.",
            &["io", "stdout", "bytes"],
        )),
        ("json", "encode") => Some((
            "Serializes one JSON-compatible value to text.",
            "Unsupported dynamic values return an error instead of being stringified implicitly.",
            &["json", "encoding", "dynamic"],
        )),
        ("json", "encode_lines") => Some((
            "Serializes a list of values as newline-delimited JSON.",
            "Each line is an independent JSON document and every value must be JSON-compatible.",
            &["json", "encoding", "lines"],
        )),
        ("json", "get") => Some((
            "Reads a value at a JSON path.",
            "The result is dynamic data; callers must validate the returned shape before using it as a typed record.",
            &["json", "path", "dynamic"],
        )),
        ("json", "remove" | "set") => Some((
            "Removes or replaces a value at a JSON path.",
            "Path traversal and value compatibility are checked explicitly; the operation returns an updated document value.",
            &["json", "path", "dynamic"],
        )),
        ("linux", "interfaces" | "routes" | "uevent_stream") => Some((
            "Reads Linux network interface or route records.",
            "The result is a host-global snapshot and is only available on Linux.",
            &["linux", "network", "host-state"],
        )),
        (
            "linux",
            "link_up"
            | "link_down"
            | "set_ipv4_address"
            | "flush_ipv4_addresses"
            | "add_default_ipv4_route"
            | "del_default_ipv4_route",
        ) => Some((
            "Changes Linux network link or route configuration.",
            "The operation mutates host-global networking state and requires the corresponding platform privilege.",
            &["linux", "network", "privileged"],
        )),
        (
            "linux",
            "dhcp_socket" | "dhcp_send" | "dhcp_recv" | "dhcp_close" | "dhcp_send_release",
        ) => Some((
            "Performs one stage of the Linux DHCP socket lifecycle.",
            "The handle and packet flow are caller-owned; the operation is platform-specific and privilege-sensitive.",
            &["linux", "dhcp", "network", "privileged"],
        )),
        ("linux", "write_device" | "read_device" | "root_device" | "block_devices") => Some((
            "Reads or changes Linux block-device state.",
            "Device paths address host-global resources and may require privilege; no implicit device discovery is performed.",
            &["linux", "device", "privileged"],
        )),
        (
            "linux",
            "mount" | "mount_all" | "umount_all" | "swapon" | "swapon_all" | "swapoff"
            | "swapoff_all",
        ) => Some((
            "Changes Linux mount or swap state.",
            "The operation is host-global, privilege-sensitive, and can affect processes outside the current script.",
            &["linux", "mount", "privileged", "host-global"],
        )),
        (
            "linux",
            "meminfo" | "disk_usage" | "dmesg" | "is_mountpoint" | "file_attrs" | "file_version"
            | "open_files" | "partition_table" | "blkid" | "modinfo" | "rfkill_list" | "loop_list"
            | "modules",
        ) => Some((
            "Reads a Linux host-state record or inspection stream.",
            "The result is a point-in-time observation and is unavailable or restricted on hosts without the corresponding Linux interface.",
            &["linux", "inspection", "host-state"],
        )),
        ("linux", "sysctl_get" | "sysctl_load_dirs") => Some((
            "Reads Linux sysctl configuration values.",
            "Values are read from the host kernel interface and path or permission failures remain errors.",
            &["linux", "sysctl", "host-state"],
        )),
        ("linux", "sysctl_set" | "set_file_attrs" | "set_file_version") => Some((
            "Changes Linux kernel or filesystem attributes.",
            "The mutation is host-global or persistent filesystem state and requires explicit privilege.",
            &["linux", "mutation", "privileged"],
        )),
        ("linux", "kill_all") => Some((
            "Sends a signal to the selected Linux process set.",
            "The selection and signal are explicit; the returned record reports attempted results without hiding permission failures.",
            &["linux", "process", "signal", "privileged"],
        )),
        ("linux", "chroot" | "pivot_root" | "switch_root") => Some((
            "Changes the Linux process root or root-transition state.",
            "This is a privileged process-boundary operation with irreversible host effects for the calling process tree.",
            &["linux", "root", "privileged", "process"],
        )),
        ("linux", "mknod") => Some((
            "Creates a Linux device node.",
            "Device numbers and mode are explicit; creation changes host filesystem state and normally requires privilege.",
            &["linux", "device", "filesystem", "privileged"],
        )),
        ("linux", "insmod" | "rmmod" | "modprobe" | "depmod") => Some((
            "Loads, removes, or indexes Linux kernel modules.",
            "Kernel module changes are privileged host-global effects and may alter the running system immediately.",
            &["linux", "kernel", "privileged", "host-global"],
        )),
        ("linux", "hwclock" | "set_hwclock" | "set_system_clock") => Some((
            "Reads or changes Linux hardware and system clock state.",
            "Clock mutation is host-global and privilege-sensitive; time values are not inferred from local display text.",
            &["linux", "clock", "privileged", "host-global"],
        )),
        ("linux", "rfkill_block" | "rfkill_unblock") => Some((
            "Blocks or unblocks a Linux radio device.",
            "The operation changes host device policy and requires the platform interface and privilege available to the process.",
            &["linux", "rfkill", "privileged"],
        )),
        ("linux", "loop_attach" | "loop_detach") => Some((
            "Attaches or detaches a Linux loop device.",
            "Loop-device state is host-global; callers own the attached resource and must detach it deliberately.",
            &["linux", "loop", "ownership", "privileged"],
        )),
        ("linux", "mkswap") => Some((
            "Initializes a filesystem path as Linux swap space.",
            "The operation changes persistent block metadata and must be treated as a destructive privileged boundary.",
            &["linux", "swap", "destructive", "privileged"],
        )),
        ("linux", "write_partition_table") => Some((
            "Writes a Linux partition table description.",
            "Partition changes are destructive host storage effects; validate the complete table before calling this operation.",
            &["linux", "storage", "destructive", "privileged"],
        )),
        ("linux", "fsck") => Some((
            "Runs filesystem consistency checking through the Linux boundary.",
            "Repair policy and target device are explicit; status data must be inspected before treating the check as successful.",
            &["linux", "filesystem", "status-data", "privileged"],
        )),
        ("linux", "halt" | "poweroff" | "reboot") => Some((
            "Requests a Linux system power-state transition.",
            "This is an immediate host-global effect; callers must make the requested transition explicit and final.",
            &["linux", "shutdown", "privileged", "host-global"],
        )),
        ("map", "empty") => Some((
            "Creates an empty string-keyed Map with `map.empty()`; grow it with Map methods.",
            "The new map owns its entries and has no inherited process or module state. `{}` is an empty Record unless a Map type is expected.",
            &["map", "collection", "constructor"],
        )),
        ("mime", "lookup_ext") => Some((
            "Looks up a MIME type by file extension.",
            "The result is a database lookup and does not inspect file contents.",
            &["mime", "lookup", "extension"],
        )),
        ("mime", "lookup_path") => Some((
            "Looks up a MIME type from a path's extension.",
            "Only the path spelling is inspected; the file need not exist and its bytes are not read.",
            &["mime", "lookup", "path"],
        )),
        ("mime", "parse") => Some((
            "Parses a media type into structured fields.",
            "Malformed media-type syntax returns an error instead of a partially trusted record.",
            &["mime", "parsing", "text"],
        )),
        ("net", "download") => Some((
            "Downloads one HTTP response to a destination path.",
            "The destination write and response status are explicit failures; a network response is not implicitly text.",
            &["net", "http", "filesystem"],
        )),
        ("net", "upload") => Some((
            "Uploads a path or byte source in one structured HTTP request.",
            "Request, transport, and response failures remain typed error data and the source is read explicitly.",
            &["net", "http", "filesystem"],
        )),
        ("net", "pool") => Some((
            "Creates a named HTTP connection pool.",
            "Pool state is evaluator-owned and must be closed when the workflow ends; it is not a process-global singleton.",
            &["net", "http", "ownership", "pool"],
        )),
        ("net", "close_pool" | "close_all_pools") => Some((
            "Closes one or all evaluator-owned HTTP connection pools.",
            "Closing releases reusable connections and makes later requests use a new pool or explicit default state.",
            &["net", "http", "ownership", "cleanup"],
        )),
        ("process", "list" | "threads" | "stats" | "port" | "ports") => Some((
            "Reads structured process or listener information from the host.",
            "The result is a point-in-time observation; entries can disappear or change before a later operation uses them.",
            &["process", "host-state", "inspection"],
        )),
        ("process", "current_pid") => Some((
            "Returns the current process ID.",
            "The identifier belongs to the host process running the evaluator and is not stable across invocations.",
            &["process", "identity", "host-state"],
        )),
        ("process", "which") => Some((
            "Resolves an executable through the current PATH.",
            "Resolution reports absence as data and does not start or inspect the target process.",
            &["process", "path", "lookup"],
        )),
        ("process", "signal" | "kill") => Some((
            "Sends a selected signal to a process.",
            "Signal delivery is an explicit host effect; permission, liveness, and target errors are not converted to success.",
            &["process", "signal", "privileged"],
        )),
        ("process", "argv_words") => Some((
            "Splits a command string into an argv vector.",
            "The parser returns words without executing them and preserves the explicit quoting rules of the API.",
            &["process", "argv", "parsing"],
        )),
        ("process", "command_argv") => Some((
            "Builds a command plan from an executable and argv list.",
            "Arguments remain separate values; no shell expansion, word splitting, or implicit command execution occurs.",
            &["process", "argv", "plan"],
        )),
        ("process", "run") => Some((
            "Runs a typed command and returns its process status.",
            "A nonzero child status is status data at this boundary; setup and execution failures remain errors.",
            &["process", "status-data", "execution"],
        )),
        ("process", "wait_any") => Some((
            "Waits for one process from an owned handle set.",
            "Waiting consumes the selected handle lifecycle and returns status data for the child that completed.",
            &["process", "ownership", "status-data"],
        )),
        ("process", "wait_ready") => Some((
            "Waits for a process handle to become waitable.",
            "Readiness does not itself erase the handle or turn a nonzero child exit into a runtime error.",
            &["process", "ownership", "status-data"],
        )),
        ("record", "require") => Some((
            "Validates a dynamic value against a named record schema.",
            "Unknown fields and wrong field types are rejected before the value crosses into the requested record contract.",
            &["record", "schema", "dynamic"],
        )),
        ("regex", "compile") => Some((
            "Compiles a regular expression into a reusable Regex value.",
            "Invalid syntax is reported at compilation and is not deferred to a later match call.",
            &["regex", "parsing", "compiled"],
        )),
        ("set", "empty") => Some((
            "Creates an empty string-key set.",
            "The set is represented by a map-backed value and starts without inherited entries.",
            &["set", "collection"],
        )),
        ("set", "from") => Some((
            "Builds a set from a list of strings.",
            "Duplicate values collapse to one membership entry while input order does not become set ordering.",
            &["set", "collection", "deduplication"],
        )),
        ("set", "has") => Some((
            "Checks membership in a string-key set.",
            "The result is a pure membership value and does not mutate the set.",
            &["set", "collection", "lookup"],
        )),
        ("set", "add" | "remove") => Some((
            "Adds or removes one string membership entry.",
            "The operation returns the updated set value; it does not mutate an unrelated alias in place.",
            &["set", "collection", "mutation"],
        )),
        ("shlex", "quote") => Some((
            "Renders one value as a shell-safe word.",
            "The result is text for an external shell boundary; rendering never invokes a shell or performs expansion.",
            &["shlex", "quoting", "text"],
        )),
        ("shlex", "join") => Some((
            "Renders argv values as a shell-safe command string.",
            "Each input remains one word in the rendered representation and the result is not executed implicitly.",
            &["shlex", "quoting", "argv"],
        )),
        ("system", "hostname") => Some((
            "Reads the host name.",
            "The value is host state and may require environment or platform support; it is not a process argument.",
            &["system", "identity", "host-state"],
        )),
        ("system", "uname") => Some((
            "Reads the host kernel identity record.",
            "The returned fields describe the running host at query time.",
            &["system", "identity", "host-state"],
        )),
        ("system", "memory") => Some((
            "Reads host memory statistics.",
            "The record is a point-in-time observation and values can change immediately after the call.",
            &["system", "memory", "host-state"],
        )),
        ("system", "os_release") => Some((
            "Reads the host operating-system release record.",
            "Missing or malformed release metadata remains an error rather than an invented version.",
            &["system", "os", "host-state"],
        )),
        ("test", "ok" | "eq" | "ne" | "contains" | "not_contains" | "error_kind") => Some((
            "Asserts one native-test condition.",
            "A failed assertion marks the owning XSH test as failed and preserves the reported value or error context.",
            &["test", "assertion", "native-tests"],
        )),
        ("test", "fail") => Some((
            "Fails the current native XSH test with an explicit message.",
            "The failure is intentional test control flow and is reported by the native-test harness.",
            &["test", "assertion", "native-tests"],
        )),
        ("test", "skip") => Some((
            "Skips the current native XSH test.",
            "Skipped tests are reported separately from passes and failures by the harness.",
            &["test", "native-tests", "control-flow"],
        )),
        ("test", "temp_path" | "temp_dir" | "temp_file") => Some((
            "Creates a test-owned temporary path or resource.",
            "The returned resource belongs to the test scope and must be cleaned up or retained intentionally.",
            &["test", "temporary", "native-tests"],
        )),
        ("test", "mock") => Some((
            "Installs a scoped host-effect mock for a native XSH test.",
            "The mock is active only for its lexical test scope and must match the operation boundary declared by the test.",
            &["test", "mock", "native-tests"],
        )),
        ("test", "calls") => Some((
            "Reads recorded calls from a native-test mock.",
            "Call records describe the completed mock scope and are not a substitute for asserting the returned behavior.",
            &["test", "mock", "native-tests"],
        )),
        ("test", "run_script" | "run_xsh" | "run_xsht_trace") => Some((
            "Runs a nested XSH or tracing fixture from a native test.",
            "The nested process receives explicit arguments and its status/output remain test data for assertions.",
            &["test", "native-tests", "process"],
        )),
        ("time", "now") => Some((
            "Reads the current wall-clock time.",
            "The value is a host clock observation and is not a monotonic duration source.",
            &["time", "clock", "host-state"],
        )),
        ("time", "sleep") => Some((
            "Suspends the current XSH operation for a duration.",
            "Sleep is interruptible host work and consumes the declared time effect.",
            &["time", "sleep", "effect"],
        )),
        ("time", "millis" | "seconds") => Some((
            "Converts a duration to an integer time unit.",
            "Conversion follows the declared unit and does not query the wall clock.",
            &["time", "duration", "conversion"],
        )),
        ("time", "measure") => Some((
            "Measures a command or block and returns structured timing data.",
            "The measured operation still owns its normal status and error contract; timing is additional data.",
            &["time", "measurement", "process"],
        )),
        ("time", "duration_compact") => Some((
            "Formats a duration using compact fixed-width units.",
            "Negative inputs clamp to zero before display.",
            &["time", "duration", "display"],
        )),
        (
            "tui",
            "reset" | "bold" | "dim" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan"
            | "white" | "gray" | "clear" | "home" | "erase_line" | "hide_cursor" | "show_cursor",
        ) => Some((
            "Returns a terminal control sequence for the selected display operation.",
            "The value is output text; callers choose when to write it and the API does not mutate terminal state by itself.",
            &["tui", "terminal", "control-sequence"],
        )),
        ("tui", "left_pad" | "right_pad") => Some((
            "Pads text to a terminal display width.",
            "Width is measured for terminal cells rather than raw byte length, so wide characters affect the result.",
            &["tui", "terminal", "width"],
        )),
        ("tui", "read_secret") => Some((
            "Reads a secret from a terminal without echoing it.",
            "The operation requires an interactive or supplied terminal boundary and returns input as explicit text data.",
            &["tui", "terminal", "secret"],
        )),
        ("unix", "reap_child_events") => Some((
            "Reaps available Unix child events.",
            "Reaping consumes wait status from the host and must be coordinated with the process owner that spawned the child.",
            &["unix", "process", "reaping"],
        )),
        ("unix", "pid1_setup" | "wait_pid1_event" | "shutdown_process_groups") => Some((
            "Coordinates Unix PID 1 or process-group lifecycle state.",
            "The operation owns a process-lifecycle boundary and can affect descendants outside the immediate call.",
            &["unix", "process", "lifecycle", "privileged"],
        )),
        (
            "unix",
            "spawn_process_group"
            | "spawn_process_group_log"
            | "spawn_logged_process_group"
            | "spawn_with_tty",
        ) => Some((
            "Starts a Unix process group with explicit lifecycle and terminal options.",
            "The returned child ownership, logging destination, and tty boundary are explicit; cleanup must be deliberate.",
            &["unix", "process", "ownership", "tty"],
        )),
        ("unix", "notify_ready" | "notify_close") => Some((
            "Signals readiness or closes a Unix service notification channel.",
            "Notification is an external service boundary and success does not imply that the supervisor accepted the service state.",
            &["unix", "service", "notification"],
        )),
        ("unix", "kill_process_group" | "kill_all") => Some((
            "Sends a signal to a Unix process group or selected process set.",
            "Target selection and signal are explicit host effects; permission and liveness failures remain visible.",
            &["unix", "process", "signal", "privileged"],
        )),
        ("unix", "exec") => Some((
            "Replaces the current Unix process with a typed command.",
            "Successful execution does not return; setup and exec failures remain errors in the calling process.",
            &["unix", "process", "exec", "privileged"],
        )),
        ("unix", "set_hostname") => Some((
            "Changes the Unix host name.",
            "This is host-global privileged state and is not limited to the current XSH scope.",
            &["unix", "hostname", "privileged", "host-global"],
        )),
        ("unix", "id") => Some((
            "Returns Unix identity information for the selected process or user.",
            "The record reflects host account and process state at lookup time and does not grant privileges.",
            &["unix", "identity", "host-state"],
        )),
        ("unix", "uptime_seconds") => Some((
            "Reads Unix host uptime in seconds.",
            "The result is host clock state and is not the elapsed time of the current evaluator.",
            &["unix", "uptime", "host-state"],
        )),
        ("unix", "tty" | "tty_attrs" | "set_tty_attrs") => Some((
            "Reads or changes Unix terminal state.",
            "Terminal descriptors and attributes are host resources; mutation must preserve the caller's restoration policy.",
            &["unix", "tty", "terminal"],
        )),
        ("user", "current") => Some((
            "Returns the current process user record.",
            "The record reflects host account state at lookup time and does not grant the script that user's privileges.",
            &["user", "identity", "host-state"],
        )),
        ("user", "lookup" | "by_uid") => Some((
            "Looks up a Unix user by name or numeric ID.",
            "Missing entries and NSS failures remain typed lookup results.",
            &["user", "identity", "lookup"],
        )),
        ("user", "add" | "remove") => Some((
            "Adds or removes a Unix user entry.",
            "The mutation is privilege-sensitive and changes host account state; inspect the returned status explicitly.",
            &["user", "identity", "privileged"],
        )),
        ("utils", "cache") => Some((
            "Reads or writes a process-scoped utility cache entry.",
            "Cache state belongs to the current process and is not a durable configuration or cross-process store.",
            &["utils", "cache", "process-state"],
        )),
        _ => None,
    };
    docs.map(|(summary, contract, tags)| DocRow {
        summary,
        contract,
        tags,
    })
}

fn method_doc(receiver: &str, method: &str) -> Option<DocRow> {
    let docs: Option<(&'static str, &'static str, &'static [&'static str])> = match (
        receiver, method,
    ) {
        ("Path", "resolve") => Some((
            "Resolves a path through the filesystem.",
            "Use resolve only when symlink-aware canonical host resolution is required; it may fail for missing paths.",
            &["path", "filesystem", "resolution"],
        )),
        ("Path", "read_text") => Some((
            "Reads a UTF-8 file into Str.",
            "Use read_bytes when invalid or opaque byte content is valid input.",
            &["path", "filesystem", "utf8"],
        )),
        ("Path", "write_atomic") => Some((
            "Atomically replaces a path with text or bytes.",
            "Use when readers must not observe a partial update.",
            &["path", "filesystem", "atomic"],
        )),
        ("Path", "relative_to") => Some((
            "Computes a path relative to an explicit base.",
            "The result is an error when the path is not below the requested base.",
            &["path", "filesystem", "relative"],
        )),
        ("Result", "context") => Some((
            "Adds a domain-specific error context before propagation.",
            "Use at a boundary where the caller needs a stable error kind and message, not at every call site.",
            &["result", "error", "context"],
        )),
        ("Stream", "collect") => Some((
            "Materializes a stream into a list.",
            "Collect only at a random-access boundary; preserve streaming for large or unbounded sources.",
            &["stream", "materialization", "ownership"],
        )),
        ("Bytes", "utf8") => Some((
            "Decodes bytes as UTF-8 text.",
            "Invalid byte sequences are an error; keep Bytes when text validity is not guaranteed.",
            &["bytes", "utf8", "conversion"],
        )),
        ("Path", "display") => Some((
            "Formats a path for display.",
            "Display text is not a filesystem operation and must not be parsed back as an implicit path.",
            &["path", "display"],
        )),
        ("Path", "name") => Some((
            "Returns the final path component using native path semantics.",
            "The result is text derived from the path spelling and may be empty for a root-like path; use basename for POSIX basename semantics.",
            &["path", "component"],
        )),
        ("Path", "basename") => Some((
            "Returns the final component using POSIX basename semantics.",
            "Trailing slashes are ignored, and a root-only path returns `/`.",
            &["path", "component", "posix"],
        )),
        ("Path", "dirname") => Some((
            "Returns the directory component using POSIX dirname semantics.",
            "Trailing slashes are ignored and the result is a Path; this is lexical and does not inspect the filesystem.",
            &["path", "component", "posix"],
        )),
        ("Path", "ext") => Some((
            "Returns the path extension.",
            "Extension parsing follows native path component rules and returns an empty string when no extension is present; use ext_or to distinguish no extension from an empty trailing-dot extension.",
            &["path", "component"],
        )),
        ("Path", "ext_or") => Some((
            "Returns the path extension, or a fallback when there is no extension.",
            "Unlike ext(), this distinguishes a missing extension from an empty extension after a trailing dot; a leading dot alone is not an extension.",
            &["path", "component"],
        )),
        ("Path", "normalize") => Some((
            "Normalizes lexical path components.",
            "Normalization is lexical and does not resolve symlinks or require the path to exist.",
            &["path", "normalization"],
        )),
        ("Path", "parent") => Some((
            "Returns the lexical parent path.",
            "The operation does not query the filesystem or resolve symlinks.",
            &["path", "component"],
        )),
        ("Path", "strip_prefix") => Some((
            "Removes an explicit path prefix.",
            "The prefix must match the path boundary; unrelated paths return an error.",
            &["path", "relative"],
        )),
        ("Path", "with_ext") => Some((
            "Replaces a path extension.",
            "The operation changes spelling only and does not rename or touch the filesystem path.",
            &["path", "component"],
        )),
        ("Path", "exists" | "executable") => Some((
            "Checks a filesystem property for a path.",
            "The result describes the host at lookup time; permission and other lookup failures remain errors.",
            &["path", "filesystem", "metadata"],
        )),
        ("Path", "du") => Some((
            "Calculates disk usage for a path.",
            "The count follows host allocation semantics rather than only logical file length.",
            &["path", "filesystem", "usage"],
        )),
        ("Path", "metadata") => Some((
            "Reads a path's filesystem metadata record.",
            "Metadata is a host snapshot and follows the path's permissions and symlink behavior.",
            &["path", "filesystem", "metadata"],
        )),
        ("Path", "read_bytes") => Some((
            "Reads a file as Bytes.",
            "The operation preserves arbitrary file bytes and does not perform UTF-8 validation.",
            &["path", "filesystem", "bytes"],
        )),
        ("Path", "lines") => Some((
            "Streams UTF-8 file lines.",
            "Line production is lazy and invalid UTF-8 remains an error at the text boundary.",
            &["path", "filesystem", "streaming"],
        )),
        ("Path", "bytes_lines") => Some((
            "Streams file lines as Bytes.",
            "The byte stream preserves non-UTF-8 content and remains lazy until consumed.",
            &["path", "filesystem", "streaming", "bytes"],
        )),
        ("Path", "write") => Some((
            "Writes text or bytes to a path.",
            "The input type selects the boundary explicitly and the destination policy is owned by the filesystem call.",
            &["path", "filesystem", "write"],
        )),
        ("Path", "copy") => Some((
            "Copies a path to an explicit destination.",
            "Overwrite behavior is explicit and filesystem failures remain errors.",
            &["path", "filesystem", "copy"],
        )),
        ("Path", "rename") => Some((
            "Renames a path to an explicit destination.",
            "The operation is a host rename boundary and does not silently copy across filesystems.",
            &["path", "filesystem", "rename"],
        )),
        ("Path", "mkdir") => Some((
            "Creates a directory at a path.",
            "The parents option controls ancestor creation; existing non-directories remain errors.",
            &["path", "filesystem", "directory"],
        )),
        ("Path", "remove") => Some((
            "Removes a path with an explicit missing policy.",
            "Removal is not recursive unless the selected host operation says so, and missing_ok controls absence.",
            &["path", "filesystem", "remove"],
        )),
        ("Path", "remove_dir") => Some((
            "Removes an empty directory.",
            "Non-empty directories and missing paths remain errors.",
            &["path", "filesystem", "remove"],
        )),
        ("Path", "touch" | "touch_from") => Some((
            "Creates or updates a path timestamp.",
            "Creation and reference-path policy are explicit; timestamp mutation is a filesystem effect.",
            &["path", "filesystem", "metadata"],
        )),
        ("Path", "truncate") => Some((
            "Changes a file's length.",
            "The requested size is explicit and truncation can discard data, so failures and size policy remain visible.",
            &["path", "filesystem", "destructive"],
        )),
        ("Path", "chmod") => Some((
            "Changes permission bits on a path.",
            "The mode is an explicit host permission value and the operation may require privilege.",
            &["path", "filesystem", "permissions"],
        )),
        ("Path", "hardlink") => Some((
            "Creates a hard link to a path.",
            "The target must satisfy host filesystem link rules and remains a shared inode, not a copied file.",
            &["path", "filesystem", "link"],
        )),
        ("Path", "unlink") => Some((
            "Removes one directory entry.",
            "Unlink changes the directory entry while open handles may continue to own the inode.",
            &["path", "filesystem", "remove"],
        )),
        ("Path", "readlink") => Some((
            "Reads a symbolic link target.",
            "The returned path is link text; it is not canonicalized or required to exist.",
            &["path", "filesystem", "symlink"],
        )),
        ("Path", "parse_bytes") => Some((
            "Parses Bytes as a filesystem path.",
            "Invalid path encoding is an error and no implicit lossy text conversion is performed.",
            &["path", "bytes", "parsing"],
        )),
        ("EnvPathList", "prepend" | "append") => Some((
            "Adds a path to an environment path list.",
            "The mutation is scoped to the path-list value and preserves explicit component ordering.",
            &["env", "path", "mutation"],
        )),
        ("EnvPathList", "pop") => Some((
            "Removes one path from an environment path list.",
            "The returned path owns the removed component and an empty list reports an error.",
            &["env", "path", "ownership"],
        )),
        ("Int", "float") => Some((
            "Converts an integer to Float.",
            "The conversion follows the host numeric representation and does not parse display text.",
            &["numeric", "conversion"],
        )),
        ("Int", "bit_and" | "bit_or" | "clear_bits") => Some((
            "Combines a non-negative integer bitset with a mask.",
            "Both operands must be non-negative; bit_and keeps shared bits, bit_or sets mask bits, and clear_bits removes mask bits.",
            &["numeric", "bitset"],
        )),
        ("Float", "floor" | "ceil" | "round") => Some((
            "Rounds a floating-point value to an integer.",
            "Non-finite values and values outside the integer range return an error.",
            &["numeric", "rounding"],
        )),
        ("Float", "format") => Some((
            "Formats a floating-point value with an optional precision.",
            "The result is display text and is not a lossless numeric serialization contract.",
            &["numeric", "display"],
        )),
        ("Float", "sqrt" | "pow" | "exp" | "ln" | "log" | "sin" | "cos" | "tan" | "abs") => Some((
            "Computes a floating-point mathematical function.",
            "Domain errors follow the numeric result contract rather than being hidden as text or status values.",
            &["numeric", "math"],
        )),
        ("Record", "has") => Some((
            "Checks whether a record contains a field.",
            "Field presence is distinct from the field's value and does not require a typed field lookup.",
            &["record", "lookup"],
        )),
        ("Record", "get") => Some((
            "Reads a dynamic record field.",
            "Missing fields return an error result so callers cannot confuse absence with a null-like value.",
            &["record", "lookup", "dynamic"],
        )),
        ("Record", "keys") => Some((
            "Lists the field names in a record.",
            "The returned names describe the record value and are ordered by the record map contract.",
            &["record", "collection"],
        )),
        ("Map", "len") => Some((
            "Returns the number of entries in a map.",
            "The count is a pure snapshot of the map value.",
            &["map", "collection"],
        )),
        ("Map", "has") => Some((
            "Checks whether a map contains a key.",
            "Key presence is distinct from the stored value and does not mutate the map.",
            &["map", "lookup"],
        )),
        ("Map", "get") => Some((
            "Reads a map value with or without a fallback.",
            "The fallback overload distinguishes missing keys from stored values and never inserts the fallback.",
            &["map", "lookup", "fallback"],
        )),
        ("Map", "set") => Some((
            "Returns a map with one key replaced.",
            "The operation returns the updated value rather than mutating an unrelated alias.",
            &["map", "mutation"],
        )),
        ("Map", "push") => Some((
            "Appends a value to a map entry list.",
            "The entry is treated as a list and the returned map owns the updated list value.",
            &["map", "mutation", "collection"],
        )),
        ("Map", "remove") => Some((
            "Returns a map without one key.",
            "Removing a missing key leaves the map value unchanged.",
            &["map", "mutation"],
        )),
        ("Map", "keys" | "values") => Some((
            "Lists map keys or values.",
            "The result is a snapshot collection and does not retain a live map handle.",
            &["map", "collection"],
        )),
        ("List", "len") => Some((
            "Returns the number of list elements.",
            "The count is a pure snapshot of the list value.",
            &["list", "collection"],
        )),
        ("List", "contains") => Some((
            "Checks whether a list contains a value.",
            "Comparison follows XSH value equality and does not mutate the list.",
            &["list", "lookup"],
        )),
        ("List", "get") => Some((
            "Reads a list element with or without a fallback.",
            "The fallback overload distinguishes an out-of-range index from a stored value and does not resize the list.",
            &["list", "lookup", "fallback"],
        )),
        ("List", "push") => Some((
            "Returns a list with one value appended.",
            "The operation produces an updated list value rather than relying on hidden mutable collection state.",
            &["list", "mutation"],
        )),
        ("List", "extend") => Some((
            "Returns a list with another list appended.",
            "Elements are copied in input order and the source list remains independently owned.",
            &["list", "mutation", "collection"],
        )),
        ("List", "join") => Some((
            "Joins list values into text.",
            "Conversion is explicit at the text boundary and does not invoke shell word splitting.",
            &["list", "text"],
        )),
        ("Str", "trim") => Some((
            "Removes surrounding Unicode whitespace.",
            "Only the boundary is changed; interior characters remain in order.",
            &["text", "unicode"],
        )),
        ("Str", "starts_with" | "ends_with" | "contains") => Some((
            "Checks a text relationship.",
            "Matching operates on UTF-8 text values and returns a pure boolean.",
            &["text", "lookup"],
        )),
        ("Str", "lines" | "words" | "fields") => Some((
            "Splits text into a structured list.",
            "The selected delimiter and whitespace policy define the boundary; no shell parsing is implied.",
            &["text", "split", "collection"],
        )),
        ("Str", "split") => Some((
            "Splits text at an explicit separator.",
            "Separator handling is literal API behavior and does not invoke a regular expression or shell parser.",
            &["text", "split"],
        )),
        ("Str", "replace") => Some((
            "Replaces text occurrences.",
            "Replacement is literal according to the method contract and returns a new Str value.",
            &["text", "replace"],
        )),
        ("Str", "wrap") => Some((
            "Wraps text to a requested width.",
            "Width and line boundaries are explicit; the method returns presentation text rather than terminal output.",
            &["text", "layout"],
        )),
        ("Str", "translate") => Some((
            "Translates characters through an explicit mapping.",
            "Unmapped characters follow the method's deletion or preservation policy and are not silently re-encoded.",
            &["text", "transform"],
        )),
        ("Str", "lower" | "upper") => Some((
            "Changes text case.",
            "Case conversion follows Unicode text rules and returns a new Str value.",
            &["text", "unicode", "transform"],
        )),
        ("Str", "delete" | "squeeze" | "reverse") => Some((
            "Transforms text characters.",
            "The method returns a new value and applies its character policy without changing byte data in place.",
            &["text", "transform"],
        )),
        ("Str", "count_lines" | "count_words" | "count_chars" | "count_bytes" | "byte_len") => {
            Some((
                "Counts a text property.",
                "Character, byte, and line counts are distinct UTF-8 measurements; select the method that matches the boundary.",
                &["text", "count", "utf8"],
            ))
        }
        ("Str", "byte_at" | "byte_slice") => Some((
            "Reads a byte position or range from UTF-8 text.",
            "Indices are byte offsets and must land on valid UTF-8 boundaries where a Str result is required.",
            &["text", "utf8", "bounds"],
        )),
        ("Str", "find") => Some((
            "Finds a text substring position.",
            "The result uses the documented byte-offset convention and absence remains distinguishable.",
            &["text", "lookup", "offset"],
        )),
        ("Str", "parse_int") => Some((
            "Parses text as an integer.",
            "The accepted radix and syntax are explicit; malformed or out-of-range text returns an error.",
            &["text", "parsing", "numeric"],
        )),
        ("Str", "parse_int_decimal") => Some((
            "Parses text as a strict decimal integer.",
            "Only nonempty decimal digits without leading zeros (except `0`) are accepted; malformed or out-of-range text returns an error.",
            &["text", "parsing", "decimal", "numeric"],
        )),
        ("Str", "parse_uint") => Some((
            "Parses text as a non-negative decimal integer, trimming surrounding whitespace.",
            "Signs, radix prefixes, malformed text, and out-of-range values return an error.",
            &["text", "parsing", "decimal", "numeric", "unsigned"],
        )),
        ("Str", "parse_uint_positive") => Some((
            "Parses text as a positive decimal integer, trimming surrounding whitespace.",
            "Only decimal digits representing a value greater than zero are accepted; zero, signs, malformed, and out-of-range text return an error.",
            &["text", "parsing", "decimal", "numeric", "positive"],
        )),
        ("Str", "parse_float") => Some((
            "Parses text as a floating-point value.",
            "Malformed or non-finite input follows the numeric parser contract and returns an error.",
            &["text", "parsing", "numeric"],
        )),
        ("Str", "base64_decode" | "base32_decode") => Some((
            "Decodes text from a base encoding into Bytes.",
            "Invalid alphabet or padding returns an error rather than partially decoded bytes.",
            &["text", "encoding", "bytes"],
        )),
        ("Bytes", "len") => Some((
            "Returns the number of bytes.",
            "The count is a pure snapshot and is not a character count.",
            &["bytes", "count"],
        )),
        ("Bytes", "slice") => Some((
            "Returns a byte range.",
            "The range is bounds-checked and the result owns its copied bytes.",
            &["bytes", "bounds"],
        )),
        ("Bytes", "dump") => Some((
            "Formats bytes as a diagnostic dump.",
            "The result is display text and must not be used as an implicit binary round trip.",
            &["bytes", "display"],
        )),
        ("Bytes", "strings") => Some((
            "Extracts printable strings from bytes.",
            "The extraction is a diagnostic view and does not assert that all input bytes are UTF-8.",
            &["bytes", "inspection", "text"],
        )),
        ("Bytes", "base64" | "base32") => Some((
            "Encodes bytes using a base alphabet.",
            "The result is text for an explicit interchange boundary and preserves all input bytes.",
            &["bytes", "encoding", "text"],
        )),
        ("Bytes", "md5" | "sha1" | "sha256" | "sha512") => Some((
            "Calculates a digest for the byte value.",
            "The digest method consumes bytes directly; choose an algorithm appropriate to the integrity or compatibility boundary.",
            &["bytes", "hash", "digest"],
        )),
        ("Bytes", "chunks") => Some((
            "Splits bytes into fixed-size chunks.",
            "Chunk size and final remainder behavior are explicit; the result is a collection of owned byte values.",
            &["bytes", "chunks", "collection"],
        )),
        ("Bytes", "compare") => Some((
            "Compares two byte buffers.",
            "Comparison is bytewise and independent of UTF-8 decoding.",
            &["bytes", "comparison"],
        )),
        ("Bytes", "contains") => Some((
            "Checks whether one byte buffer contains another.",
            "Matching is bytewise and does not decode either value as text.",
            &["bytes", "lookup"],
        )),
        ("Bytes", "byte_at") => Some((
            "Reads one byte at an explicit offset.",
            "The offset is bounds-checked and the result is independent of UTF-8 decoding.",
            &["bytes", "lookup", "bounds"],
        )),
        ("Bytes", "count_lines") => Some((
            "Counts line separators in bytes.",
            "The operation is byte-oriented and does not require valid UTF-8.",
            &["bytes", "count", "lines"],
        )),
        ("Bytes", "lines") => Some((
            "Splits bytes into line-oriented chunks.",
            "The operation is byte-oriented and does not require valid UTF-8; each emitted chunk remains Bytes.",
            &["bytes", "lines", "streaming"],
        )),
        ("Bytes", "lower") => Some((
            "Lowercases ASCII-compatible bytes.",
            "Only the method's byte-level case policy applies; arbitrary binary data remains binary.",
            &["bytes", "transform"],
        )),
        ("Bytes", "trim") => Some((
            "Trims the byte boundary.",
            "Trimming follows the byte whitespace policy and does not decode arbitrary input as text.",
            &["bytes", "transform"],
        )),
        ("Bytes", "starts_with" | "ends_with") => Some((
            "Checks a byte-prefix or suffix relationship.",
            "Matching is bytewise and returns a pure boolean.",
            &["bytes", "lookup"],
        )),
        ("Status", "exited" | "signaled") => Some((
            "Checks how a process status completed.",
            "The predicate distinguishes normal exit from signal termination without discarding the original status.",
            &["process", "status-data"],
        )),
        ("Status", "exited_with") => Some((
            "Checks a process exit code.",
            "The comparison applies only to normal exits; signaled statuses remain distinct.",
            &["process", "status-data", "comparison"],
        )),
        ("Status", "exit_code" | "signal_number") => Some((
            "Reads one field from a process status.",
            "The field is optional by process outcome and callers must use the matching outcome predicate first.",
            &["process", "status-data"],
        )),
        ("Digest", "hex") => Some((
            "Formats a digest as hexadecimal text.",
            "Formatting is deterministic display/interchange output and does not recalculate the digest.",
            &["hash", "digest", "hex"],
        )),
        ("Digest", "base64") => Some((
            "Formats a digest as base64 text.",
            "Formatting preserves the digest bytes and does not add a verification step.",
            &["hash", "digest", "base64"],
        )),
        ("Regex", "matches") => Some((
            "Tests whether a regex matches text.",
            "The compiled expression is reused without changing the input text.",
            &["regex", "matching"],
        )),
        ("Regex", "find") => Some((
            "Finds regex matches in text.",
            "Match positions and text are returned as structured values; no replacement occurs.",
            &["regex", "matching", "search"],
        )),
        ("Regex", "captures") => Some((
            "Extracts regex capture groups.",
            "Capture absence remains distinguishable from an empty capture and the input is not mutated.",
            &["regex", "captures"],
        )),
        ("Regex", "replace") => Some((
            "Replaces regex matches in text.",
            "Replacement syntax follows the regex method contract and returns a new string.",
            &["regex", "replace"],
        )),
        ("ProcessHandle", "cancel") => Some((
            "Requests cancellation of an owned process handle.",
            "Cancellation changes handle lifecycle and process state; wait or detach remains the caller's responsibility.",
            &["process", "ownership", "cancellation"],
        )),
        ("NetJob", "wait") => Some((
            "Consumes an owned network job and returns its buffered response.",
            "A wait consumes every terminal result, including an error, so aliases cannot observe it again.",
            &["net", "http", "ownership", "wait"],
        )),
        ("NetJob", "cancel") => Some((
            "Cancels and consumes an owned network job.",
            "Cancel waits for a terminal transport state before releasing the job's evaluator-owned capacity.",
            &["net", "http", "ownership", "cancel"],
        )),
        _ => None,
    };
    docs.map(|(summary, contract, tags)| DocRow {
        summary,
        contract,
        tags,
    })
}

fn method_docs(receiver: MethodReceiver, method: &str) -> ApiDocs {
    let receiver = receiver_name(receiver);
    let doc = method_doc(receiver, method)
        .unwrap_or_else(|| panic!("missing method documentation for {receiver}.{method}"));
    ApiDocs {
        summary: doc.summary.to_string(),
        contract: doc.contract.to_string(),
        example: crate::examples::source(&format!("method.{receiver}.{method}")),
        tags: api_tags(&receiver.to_ascii_lowercase(), method, doc.tags),
    }
}
