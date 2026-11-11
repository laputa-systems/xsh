use super::alias::AliasMap;
use super::listing;
use super::path;

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

struct Builtin {
    name: &'static str,
    help: &'static str,
}

const SHELL_BUILTINS: &[Builtin] = &[
    Builtin {
        name: "alias",
        help: ALIAS_HELP,
    },
    Builtin {
        name: "cd",
        help: CHANGE_DIRECTORY_HELP,
    },
    Builtin {
        name: "copy-scrollback",
        help: COPY_SCROLLBACK_HELP,
    },
    Builtin {
        name: "c",
        help: CLEAR_SCREEN_HELP,
    },
    Builtin {
        name: "denv",
        help: DENV_HELP,
    },
    Builtin {
        name: "exit",
        help: EXIT_HELP,
    },
    Builtin {
        name: "fg",
        help: RESUME_JOB_HELP,
    },
    Builtin {
        name: "history",
        help: HISTORY_HELP,
    },
    Builtin {
        name: "xshi-dump",
        help: DUMP_LAYOUT_HELP,
    },
    Builtin {
        name: "l",
        help: LIST_HELP,
    },
    Builtin {
        name: "ll",
        help: LIST_HIDDEN_HELP,
    },
    Builtin {
        name: "type",
        help: LOCATE_HELP,
    },
    Builtin {
        name: "w",
        help: LOCATE_HELP,
    },
    Builtin {
        name: "which",
        help: LOCATE_HELP,
    },
    Builtin {
        name: "z",
        help: JUMP_HELP,
    },
];

const EPSH_BUILTINS: &[Builtin] = &[
    Builtin {
        name: "echo",
        help: ECHO_HELP,
    },
    Builtin {
        name: "printf",
        help: PRINTF_HELP,
    },
    Builtin {
        name: "pwd",
        help: PWD_HELP,
    },
    Builtin {
        name: "true",
        help: TRUE_HELP,
    },
    Builtin {
        name: "false",
        help: FALSE_HELP,
    },
    Builtin {
        name: ":",
        help: COLON_HELP,
    },
    Builtin {
        name: "test",
        help: TEST_HELP,
    },
    Builtin {
        name: "[",
        help: BRACKET_HELP,
    },
    Builtin {
        name: "kill",
        help: KILL_HELP,
    },
    Builtin {
        name: "export",
        help: EXPORT_HELP,
    },
    Builtin {
        name: "unset",
        help: UNSET_HELP,
    },
    Builtin {
        name: "set",
        help: SET_HELP,
    },
    Builtin {
        name: "command",
        help: COMMAND_HELP,
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
    lookup(name).map(|builtin| builtin.help.replace("{name}", name))
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
            .is_some_and(|first| strip_quotes(first) == name);
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
fn is_refused_builtin(name: &str) -> bool {
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

fn strip_quotes(word: &str) -> &str {
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
