use super::alias::AliasMap;
use super::listing;
use super::path;

/// Builtins provided by xshi's session layer, beyond the POSIX-style set.
const EXTENSION_BUILTINS: &[&str] = &[
    "fg",
    "bg",
    "z",
    "l",
    "c",
    "w",
    "which",
    "type",
    "copy-scrollback",
    "history",
    "alias",
    "denv",
    "xshi-dump",
];

/// The shell builtins a POSIX-style interactive shell resolves before PATH.
const SHELL_BUILTINS: &[&str] = &[
    ":", "true", "false", "echo", "printf", "cd", "pwd", "exit", "return", "break", "continue",
    "export", "readonly", "unset", "set", "shift", "eval", ".", "source", "test", "[", "read",
    "local", "exec", "command", "type", "wait", "trap", "kill", "umask", "getopts",
];

pub(super) fn is_extension_builtin(name: &str) -> bool {
    EXTENSION_BUILTINS.contains(&name)
}

/// Whether a command name is a builtin in either set.
pub(super) fn is_builtin(name: &str) -> bool {
    is_extension_builtin(name) || SHELL_BUILTINS.contains(&name)
}

/// All builtin names, for completion.
pub(super) fn all_builtin_names() -> impl Iterator<Item = &'static str> {
    EXTENSION_BUILTINS.iter().chain(SHELL_BUILTINS).copied()
}

/// Locates a command through aliases, builtins, or PATH. `path_env` is the
/// session's `$PATH`.
pub(super) fn locate_command(
    args: &[String],
    aliases: &AliasMap,
    path_env: &[u8],
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> i32 {
    use std::io::Write as _;
    let Some(name) = args.first() else {
        writeln!(stderr, "xshi: w: expected command name").ok();
        return 1;
    };

    if let Some(expansion) = aliases.get(name) {
        writeln!(stdout, "alias: {name} {}", expansion.join(" ")).ok();
        return 0;
    }

    if is_builtin(name) {
        writeln!(stdout, "builtin").ok();
        return 0;
    }

    if let Some(found) = path::scan_path(name, path_env) {
        writeln!(stdout, "{}", found.display()).ok();
        return 0;
    }

    writeln!(stderr, "xshi: w: not found: {name}").ok();
    1
}

/// Lists directories with the native `l` listing.
pub(super) fn list_directory(args: &[String], stdout: &mut Vec<u8>, stderr: &mut Vec<u8>) -> i32 {
    use std::io::Write as _;
    if args.is_empty() {
        return listing::list_dir(".", stdout, stderr);
    }
    let mut status = 0;
    let label = args.len() > 1;
    for (index, arg) in args.iter().enumerate() {
        if label {
            if index > 0 {
                writeln!(stdout).ok();
            }
            writeln!(stdout, "{arg}:").ok();
        }
        let listed = listing::list_dir(arg, stdout, stderr);
        if listed != 0 {
            status = listed;
        }
    }
    status
}
