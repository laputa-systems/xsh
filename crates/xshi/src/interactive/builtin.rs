use super::alias::AliasMap;
use super::listing;
use super::path;
use std::path::{Path, PathBuf};

const FDU_HELP: &str =
    "Usage: {name} [--apparent] [-x PATH | --exclude PATH]... [--summary] [--read-only] [PATH]\nBrowse disk usage in a terminal.";

const ALIAS_HELP: &str =
    "Usage: {name} [NAME [COMMAND...]]\nDefine an alias, or list aliases when NAME is omitted.";
const CHANGE_DIRECTORY_HELP: &str =
    "Usage: {name} [DIRECTORY]\nChange the current directory. No argument goes to HOME; `-` returns to the previous directory.";
const COPY_SCROLLBACK_HELP: &str = "Usage: {name}\nCopy the terminal scrollback through OSC 52.";
const DENV_HELP: &str =
    "Usage: {name} <allow|deny|reload>\nTrust, revoke, or reload environment files for this directory.";
const EXIT_HELP: &str =
    "Usage: {name} [STATUS]\nExit the shell. Leaving a suspended foreground job produces a warning.";
const RESUME_JOB_HELP: &str = "Usage: {name}\nResume the suspended foreground job.";
const HISTORY_HELP: &str =
    "Usage: {name} [reset]\nShow saved history, or clear it with `reset`.";
const DUMP_LAYOUT_HELP: &str =
    "Usage: {name}\nWrite the current layout dump to the ish data directory.";
const LIST_HELP: &str =
    "Usage: {name} [PATH...]\nList visible files with sizes and relative ages.";
const LIST_HIDDEN_HELP: &str =
    "Usage: {name} [PATH...]\nList all files, including hidden ones, with sizes and relative ages.";
const CLEAR_SCREEN_HELP: &str = "Usage: {name}\nClear the screen.";
const LOCATE_HELP: &str =
    "Usage: {name} NAME\nLocate a command through aliases, builtins, functions, or PATH.";
const JUMP_HELP: &str =
    "Usage: {name} [QUERY]\nJump to the most frequently used directory matching QUERY. No query goes home.";
const ECHO_HELP: &str =
    "Usage: {name} [-n] [-e|-E] [ARG]...\nWrite arguments separated by spaces. `-n` omits the trailing newline; `-e` interprets escapes.";
const PRINTF_HELP: &str =
    "Usage: {name} FORMAT [ARG]...\nWrite formatted output from a C-style format string.";
const PWD_HELP: &str = "Usage: {name}\nPrint the current directory.";
const TRUE_HELP: &str = "Usage: {name}\nReturn a successful status.";
const FALSE_HELP: &str = "Usage: {name}\nReturn a failure status.";
const COLON_HELP: &str = "Usage: {name}\nReturn a successful status.";
const TEST_HELP: &str = "Usage: {name} EXPRESSION\nEvaluate a conditional expression.";
const BRACKET_HELP: &str =
    "Usage: {name} EXPRESSION ]\nEvaluate a conditional expression; the closing `]` is required.";
const KILL_HELP: &str =
    "Usage: {name} [-l] [-s SIGNAL | -SIGNAL] PID...\nSend a signal, or list signals with `-l`.";
const EXPORT_HELP: &str =
    "Usage: {name} [NAME[=VALUE]]...\nMark variables for the environment, or list exported variables with no arguments.";
const UNSET_HELP: &str = "Usage: {name} [-v|-f] NAME...\nRemove variables, or functions with `-f`.";
const SET_HELP: &str =
    "Usage: {name} [NAME [VALUE...]]\nSet and export a shell variable, or list the environment with no arguments.";
const COMMAND_HELP: &str =
    "Usage: {name} [-v|-V] NAME...\n       {name} [-p] COMMAND [ARG...]\nRun a command without function lookup, or describe how NAME resolves.";
const RMRF_HELP: &str =
    "Usage: {name} [--] PATH...\nPermanently remove each PATH, including directory trees. Missing paths are ignored.";

#[derive(Clone, Copy)]
enum Help {
    Text(&'static str),
    Copy,
    Move,
    Sync,
}

impl Help {
    fn render(self, name: &str) -> String {
        match self {
            Self::Text(text) => text.replace("{name}", name),
            Self::Copy => super::fs_command::help(super::fs_command::FileOperation::Copy),
            Self::Move => super::fs_command::help(super::fs_command::FileOperation::Move),
            Self::Sync => super::sync_command::help(),
        }
    }
}

struct Builtin {
    name: &'static str,
    help: Help,
}

const SHELL_BUILTINS: &[Builtin] = &[
    Builtin {
        name: "alias",
        help: Help::Text(ALIAS_HELP),
    },
    Builtin {
        name: "cd",
        help: Help::Text(CHANGE_DIRECTORY_HELP),
    },
    Builtin {
        name: "copy-scrollback",
        help: Help::Text(COPY_SCROLLBACK_HELP),
    },
    Builtin {
        name: "c",
        help: Help::Text(CLEAR_SCREEN_HELP),
    },
    Builtin {
        name: "cp",
        help: Help::Copy,
    },
    Builtin {
        name: "fdu",
        help: Help::Text(FDU_HELP),
    },
    Builtin {
        name: "denv",
        help: Help::Text(DENV_HELP),
    },
    Builtin {
        name: "exit",
        help: Help::Text(EXIT_HELP),
    },
    Builtin {
        name: "fg",
        help: Help::Text(RESUME_JOB_HELP),
    },
    Builtin {
        name: "history",
        help: Help::Text(HISTORY_HELP),
    },
    Builtin {
        name: "mv",
        help: Help::Move,
    },
    Builtin {
        name: "xshi-dump",
        help: Help::Text(DUMP_LAYOUT_HELP),
    },
    Builtin {
        name: "l",
        help: Help::Text(LIST_HELP),
    },
    Builtin {
        name: "ll",
        help: Help::Text(LIST_HIDDEN_HELP),
    },
    Builtin {
        name: "rmrf",
        help: Help::Text(RMRF_HELP),
    },
    Builtin {
        name: "rsync",
        help: Help::Sync,
    },
    Builtin {
        name: "type",
        help: Help::Text(LOCATE_HELP),
    },
    Builtin {
        name: "w",
        help: Help::Text(LOCATE_HELP),
    },
    Builtin {
        name: "which",
        help: Help::Text(LOCATE_HELP),
    },
    Builtin {
        name: "z",
        help: Help::Text(JUMP_HELP),
    },
];

const EPSH_BUILTINS: &[Builtin] = &[
    Builtin {
        name: "echo",
        help: Help::Text(ECHO_HELP),
    },
    Builtin {
        name: "printf",
        help: Help::Text(PRINTF_HELP),
    },
    Builtin {
        name: "pwd",
        help: Help::Text(PWD_HELP),
    },
    Builtin {
        name: "true",
        help: Help::Text(TRUE_HELP),
    },
    Builtin {
        name: "false",
        help: Help::Text(FALSE_HELP),
    },
    Builtin {
        name: ":",
        help: Help::Text(COLON_HELP),
    },
    Builtin {
        name: "test",
        help: Help::Text(TEST_HELP),
    },
    Builtin {
        name: "[",
        help: Help::Text(BRACKET_HELP),
    },
    Builtin {
        name: "kill",
        help: Help::Text(KILL_HELP),
    },
    Builtin {
        name: "export",
        help: Help::Text(EXPORT_HELP),
    },
    Builtin {
        name: "unset",
        help: Help::Text(UNSET_HELP),
    },
    Builtin {
        name: "set",
        help: Help::Text(SET_HELP),
    },
    Builtin {
        name: "command",
        help: Help::Text(COMMAND_HELP),
    },
];

/// Whether a command name is a builtin in either set.
pub(super) fn is_builtin(name: &str) -> bool {
    lookup(name).is_some()
}

fn lookup(name: &str) -> Option<&'static Builtin> {
    SHELL_BUILTINS
        .iter()
        .chain(EPSH_BUILTINS)
        .find(|builtin| builtin.name == name)
}

/// All builtin names, for completion.
pub(super) fn all_builtin_names() -> impl Iterator<Item = &'static str> {
    SHELL_BUILTINS
        .iter()
        .chain(EPSH_BUILTINS)
        .map(|builtin| builtin.name)
}

/// Help text for a builtin, with its own name substituted.
pub(super) fn help(name: &str) -> Option<String> {
    lookup(name).map(|builtin| builtin.help.render(name))
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
    if args.is_empty() {
        writeln!(stderr, "xshi: w: expected command name").ok();
        return 1;
    }

    let name = &args[0];

    let mut has_same_name_alias = false;
    if let Some(expansion) = aliases.get(name) {
        writeln!(stdout, "alias: {name} {}", expansion.join(" ")).ok();
        let same_name = expansion
            .first()
            .is_some_and(|first| strip_quotes_for_lookup(first) == name);
        if !same_name {
            return 0;
        }
        has_same_name_alias = true;
    }

    if is_builtin(name) {
        writeln!(stdout, "{name}: builtin").ok();
        if let Some(help) = help(name) {
            stdout.extend_from_slice(help.as_bytes());
            if !help.ends_with('\n') {
                writeln!(stdout).ok();
            }
        }
        return 0;
    }

    if is_refused_builtin(name) {
        if has_same_name_alias {
            return 0;
        }
        writeln!(stderr, "xshi: w: not found: {name}").ok();
        return 1;
    }

    if let Some(found) = path::scan_path(name, path_env) {
        writeln!(stdout, "{}", found.display()).ok();
        return 0;
    }

    if has_same_name_alias {
        return 0;
    }
    writeln!(stderr, "xshi: w: not found: {name}").ok();
    1
}

/// POSIX names the reference shell's engine reserves but the interactive
/// policy refuses, so PATH lookup never exposes them.
pub(super) fn is_refused_builtin(name: &str) -> bool {
    matches!(
        name,
        "return"
            | "break"
            | "continue"
            | "readonly"
            | "shift"
            | "eval"
            | "."
            | "source"
            | "read"
            | "local"
            | "exec"
            | "wait"
            | "trap"
            | "umask"
            | "getopts"
    )
}

pub(super) fn strip_quotes_for_lookup(word: &str) -> &str {
    if word.len() >= 2 {
        let bytes = word.as_bytes();
        if (bytes[0] == b'\'' && bytes[word.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[word.len() - 1] == b'"')
        {
            return &word[1..word.len() - 1];
        }
    }
    word
}

/// Runs the core `fdu` applet in this process, with the shell's arguments.
pub(super) fn run_fdu(args: &[String], stdout: &mut Vec<u8>, stderr: &mut Vec<u8>) -> i32 {
    use std::io::Write as _;
    let Some(script) = resolve_core_script("fdu") else {
        writeln!(stderr, "xshi: fdu: cannot find core/fdu.xsh").ok();
        return 1;
    };
    let options = xsh::execution::script::RunOptions {
        script: script.to_string_lossy().into_owned(),
        args: args.iter().map(std::ffi::OsString::from).collect(),
        coverage_trace_dir: None,
    };
    let output = xsh::execution::script::run_script_with_shared_stdio(options);
    stdout.extend_from_slice(&output.stdout);
    stderr.extend_from_slice(&output.stderr);
    i32::from(output.status)
}

fn resolve_core_script(name: &str) -> Option<PathBuf> {
    let file = format!("{name}.xsh");
    if let Some(directory) = std::env::var_os("XSH_CORE_DIR") {
        let candidate = Path::new(&directory).join(&file);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let executable = std::env::current_exe().ok()?;
    let installed = executable
        .parent()
        .and_then(Path::parent)
        .map(|prefix| prefix.join("lib/xsh/core").join(&file));
    installed
        .into_iter()
        .chain(
            executable
                .ancestors()
                .skip(1)
                .map(|base| base.join("core").join(&file)),
        )
        .find(|candidate| candidate.is_file())
}

/// Lists directories with the native listing. `show_hidden` selects `ll`
/// (hidden files) over `l` (visible only).
pub(super) fn list_directory(
    args: &[String],
    show_hidden: bool,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
) -> i32 {
    use std::io::Write as _;
    if args.is_empty() {
        return listing::list_dir_with_hidden(".", show_hidden, stdout, stderr);
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
        let listed = listing::list_dir_with_hidden(arg, show_hidden, stdout, stderr);
        if listed != 0 {
            status = listed;
        }
    }
    status
}
