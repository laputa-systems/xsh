//! The `xsht` command line, declared once.
//!
//! Every command's options, usage lines, and help text come from `COMMANDS`.
//! `parse_command_args` is the only place option spellings are matched, so
//! `--help`, `xsht help COMMAND`, and the generated CLI reference cannot drift
//! from what the parser accepts. Commands interpret the parsed options
//! (validation, cross-option rules); they do not match option spellings.

/// How an option consumes its value.
pub(crate) enum OptionArg {
    Flag,
    /// `--name VALUE`; with `equals`, also `--name=VALUE`.
    Value {
        metavar: &'static str,
        /// What usage lines show instead of `metavar` (for example
        /// `text|jsonl`), when the accepted values fit there.
        usage: &'static str,
        equals: bool,
    },
    /// `--name` alone or `--name=VALUE`. The next argument is never consumed.
    OptionalEquals {
        metavar: &'static str,
        usage: &'static str,
    },
}

impl OptionArg {
    const fn value(metavar: &'static str) -> Self {
        Self::Value {
            metavar,
            usage: metavar,
            equals: false,
        }
    }

    const fn value_or_equals(metavar: &'static str) -> Self {
        Self::Value {
            metavar,
            usage: metavar,
            equals: true,
        }
    }

    const fn optional_equals(metavar: &'static str) -> Self {
        Self::OptionalEquals {
            metavar,
            usage: metavar,
        }
    }

    /// Shows `usage` in usage lines instead of the metavariable.
    const fn in_usage(self, usage: &'static str) -> Self {
        match self {
            Self::Flag => Self::Flag,
            Self::Value {
                metavar, equals, ..
            } => Self::Value {
                metavar,
                usage,
                equals,
            },
            Self::OptionalEquals { metavar, .. } => Self::OptionalEquals { metavar, usage },
        }
    }
}

/// One option of one command. An option shared by several commands is
/// declared in each, because its description and accepted values are the
/// command's own.
pub(crate) struct OptionSpec {
    /// Every accepted spelling, short ones first; exactly one starts with
    /// `--` and names the option everywhere else (`long`).
    pub(crate) names: &'static [&'static str],
    pub(crate) arg: OptionArg,
    /// Whether the option may appear more than once and every occurrence
    /// counts. Otherwise the last occurrence wins.
    pub(crate) repeatable: bool,
    pub(crate) description: &'static str,
}

impl OptionSpec {
    pub(crate) fn long(&self) -> &'static str {
        self.names
            .iter()
            .copied()
            .find(|name| name.starts_with("--"))
            .expect("every option declares a long spelling")
    }

    /// The option as the option list shows it, such as `-j, --jobs N`.
    pub(crate) fn syntax(&self) -> String {
        let names = self.names.join(", ");
        match &self.arg {
            OptionArg::Flag => names,
            OptionArg::Value { metavar, .. } => format!("{names} {metavar}"),
            OptionArg::OptionalEquals { metavar, .. } => format!("{names}[={metavar}]"),
        }
    }

    fn usage(&self) -> String {
        let long = self.long();
        match &self.arg {
            OptionArg::Flag => long.to_string(),
            OptionArg::Value { usage, .. } => format!("{long} {usage}"),
            OptionArg::OptionalEquals { usage, .. } => format!("{long}[={usage}]"),
        }
    }
}

/// An option that is rejected with a specific message instead of the generic
/// unknown-option error, and is deliberately absent from help.
pub(crate) struct RemovedOption {
    pub(crate) name: &'static str,
    pub(crate) message: &'static str,
}

/// A piece of a usage line.
#[derive(Clone, Copy)]
pub(crate) enum UsagePart {
    /// An optional option, bracketed, by long name.
    Opt(&'static str),
    /// A required option, unbracketed, by long name.
    Req(&'static str),
    /// The `[OPTIONS]` placeholder for a command whose options are listed below.
    Options,
    /// The command's positional arguments.
    Args,
    /// Continues the form on a new line.
    Break,
}

pub(crate) struct CommandSpec {
    pub(crate) name: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) quick_label: &'static str,
    /// Positional arguments as usage lines show them, such as `[PATH...]`.
    pub(crate) args: &'static str,
    /// One entry per usage form.
    pub(crate) usage: &'static [&'static [UsagePart]],
    pub(crate) options: &'static [OptionSpec],
    pub(crate) removed: &'static [RemovedOption],
    /// Options end at the first positional argument and everything after it
    /// belongs to the script the command runs.
    pub(crate) options_end_at_first_argument: bool,
    pub(crate) notes: &'static [&'static str],
    pub(crate) examples: &'static [&'static str],
}

impl CommandSpec {
    pub(crate) fn option(&self, long: &str) -> &OptionSpec {
        self.options
            .iter()
            .find(|option| option.long() == long)
            .unwrap_or_else(|| panic!("`xsht {}` declares no option {long}", self.name))
    }

    /// The lines of the usage section, one `xsht NAME ...` form after another;
    /// a form's later lines are indented to align under its first.
    pub(crate) fn usage_lines(&self) -> Vec<String> {
        let prefix = format!("xsht {} ", self.name);
        let indent = " ".repeat(prefix.len());
        let mut lines = Vec::new();
        for form in self.usage {
            let mut line = prefix.clone();
            let mut at_line_start = true;
            for part in *form {
                if matches!(part, UsagePart::Break) {
                    lines.push(std::mem::replace(&mut line, indent.clone()));
                    at_line_start = true;
                    continue;
                }
                if !at_line_start {
                    line.push(' ');
                }
                line.push_str(&self.render_part(*part));
                at_line_start = false;
            }
            lines.push(line);
        }
        lines
    }

    fn render_part(&self, part: UsagePart) -> String {
        match part {
            UsagePart::Opt(long) => format!("[{}]", self.option(long).usage()),
            UsagePart::Req(long) => self.option(long).usage(),
            UsagePart::Options => "[OPTIONS]".to_string(),
            UsagePart::Args => self.args.to_string(),
            UsagePart::Break => unreachable!("breaks are handled by the caller"),
        }
    }
}

pub(crate) static COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "check",
        summary: "Parse and type-check scripts",
        quick_label: "Validate source",
        args: "[PATH...]",
        usage: &[&[
            UsagePart::Opt("--summary"),
            UsagePart::Opt("--annotate"),
            UsagePart::Args,
        ]],
        options: &[
            OptionSpec {
                names: &["--summary"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Append diagnostic counts by code",
            },
            OptionSpec {
                names: &["--annotate"],
                arg: OptionArg::optional_equals("POLICY")
                    .in_usage("default|signatures|locals|all|CLASS,..."),
                repeatable: false,
                description: "Apply inferred annotations in place",
            },
        ],
        removed: &[RemovedOption {
            name: "--strict",
            message: "`xsht check --strict` was removed; dynamic boundaries are checked by default; remove `--strict`",
        }],
        options_end_at_first_argument: false,
        notes: &[],
        examples: &[],
    },
    CommandSpec {
        name: "fmt",
        summary: "Format scripts",
        quick_label: "Format source",
        args: "[FILE...]",
        usage: &[&[UsagePart::Opt("--check"), UsagePart::Args]],
        options: &[OptionSpec {
            names: &["--check"],
            arg: OptionArg::Flag,
            repeatable: false,
            description: "Check formatting without rewriting",
        }],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[],
        examples: &[],
    },
    CommandSpec {
        name: "lint",
        summary: "Run quality checks and optional fixes",
        quick_label: "Improve source",
        args: "[FILE...]",
        usage: &[
            &[
                UsagePart::Opt("--fix"),
                UsagePart::Opt("--runless"),
                UsagePart::Opt("--only"),
                UsagePart::Args,
            ],
            &[UsagePart::Req("--list"), UsagePart::Opt("--format")],
        ],
        options: &[
            OptionSpec {
                names: &["--fix"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Apply safe autofixes",
            },
            OptionSpec {
                names: &["--only"],
                arg: OptionArg::value_or_equals("RULE[,RULE...]"),
                repeatable: true,
                description: "Report and fix only the named lint codes",
            },
            OptionSpec {
                names: &["--runless"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Reject external commands unless configured",
            },
            OptionSpec {
                names: &["--list"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "List every selectable code with a summary",
            },
            OptionSpec {
                names: &["--format"],
                arg: OptionArg::value_or_equals("FORMAT").in_usage("text|jsonl"),
                repeatable: false,
                description: "text or jsonl, with --list",
            },
        ],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[],
        examples: &[],
    },
    CommandSpec {
        name: "ast",
        summary: "Print parser debug output",
        quick_label: "Inspect syntax",
        args: "SCRIPT",
        usage: &[&[UsagePart::Args]],
        options: &[],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[],
        examples: &[],
    },
    CommandSpec {
        name: "highlight",
        summary: "Print syntax highlighting runs as JSON Lines",
        quick_label: "Highlight source",
        args: "SCRIPT",
        usage: &[&[UsagePart::Args]],
        options: &[],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[
            "Each line is {\"kind\":KIND,\"text\":TEXT}; the texts concatenate to the file.",
            "KINDs: plain, comment, doc-comment, keyword, constant, type, function,",
            "  property, variable, string, path, regex, number, operator, punctuation,",
            "  interpolation.",
        ],
        examples: &[],
    },
    CommandSpec {
        name: "desugar",
        summary: "Print a script with every sugar form expanded",
        quick_label: "Expand sugar",
        args: "SCRIPT",
        usage: &[&[UsagePart::Args]],
        options: &[],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[
            "Each sugar statement, such as `repeat N times { ... }` or `return x when c`,",
            "  is replaced by the core statements that define it, with its comments.",
            "The output is formatted XSH on stdout. SCRIPT is not modified and not",
            "  checked: the output checks when SCRIPT does.",
        ],
        examples: &[],
    },
    CommandSpec {
        name: "grammar",
        summary: "Print the language grammar",
        quick_label: "Read the grammar",
        args: "",
        usage: &[&[UsagePart::Opt("--format")]],
        options: &[OptionSpec {
            names: &["--format"],
            arg: OptionArg::value_or_equals("FORMAT").in_usage("ebnf|json"),
            repeatable: false,
            description: "ebnf productions (the default) or the json reference that make docs renders",
        }],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[],
        examples: &[],
    },
    CommandSpec {
        name: "trace",
        summary: "Run a script with trace output",
        quick_label: "Run with tracing",
        args: "SCRIPT [ARGS...]",
        usage: &[&[
            UsagePart::Opt("--raw"),
            UsagePart::Opt("--trace-format"),
            UsagePart::Break,
            UsagePart::Opt("--trace-file"),
            UsagePart::Opt("--syscalls"),
            UsagePart::Opt("--trace-top-syscalls"),
            UsagePart::Break,
            UsagePart::Args,
        ]],
        options: &[
            OptionSpec {
                names: &["--raw"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Write per-event trace output",
            },
            OptionSpec {
                names: &["--trace-format"],
                arg: OptionArg::value("FORMAT").in_usage("text|jsonl|flamegraph"),
                repeatable: false,
                description: "text, jsonl, or flamegraph",
            },
            OptionSpec {
                names: &["--trace-file"],
                arg: OptionArg::value("PATH"),
                repeatable: false,
                description: "Write trace output to PATH",
            },
            OptionSpec {
                names: &["--syscalls"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Include native syscall totals",
            },
            OptionSpec {
                names: &["--trace-top-syscalls"],
                arg: OptionArg::value("N"),
                repeatable: false,
                description: "Show N syscall rows; default: 8",
            },
        ],
        removed: &[],
        options_end_at_first_argument: true,
        notes: &[],
        examples: &[],
    },
    CommandSpec {
        name: "api",
        summary: "Query language and standard-library metadata",
        quick_label: "Query the API",
        args: "[QUERY...]",
        usage: &[&[UsagePart::Options, UsagePart::Args]],
        options: &[
            OptionSpec {
                names: &["--format"],
                arg: OptionArg::value_or_equals("FORMAT"),
                repeatable: false,
                description: "text or jsonl",
            },
            OptionSpec {
                names: &["--strict"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Fail when a selector has no match",
            },
            OptionSpec {
                names: &["--details"],
                arg: OptionArg::value_or_equals("LEVEL"),
                repeatable: false,
                description: "basic or full",
            },
            OptionSpec {
                names: &["--query-file"],
                arg: OptionArg::value_or_equals("PATH"),
                repeatable: true,
                description: "Read selectors from a file",
            },
            OptionSpec {
                names: &["--stdin"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Read selectors from stdin",
            },
        ],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[
            "Queries:",
            "  summary | module:NAME | api:MODULE.FUNCTION",
            "  method:RECEIVER.METHOD | record:NAME | language:ID | search:TERMS",
        ],
        examples: &[],
    },
    CommandSpec {
        name: "test",
        summary: "Run discovered tests",
        quick_label: "Run tests",
        args: "[FILTER]",
        usage: &[&[UsagePart::Options, UsagePart::Args]],
        options: &[
            OptionSpec {
                names: &["--list"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "List matching tests",
            },
            OptionSpec {
                names: &["--exact"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Match FILTER exactly",
            },
            OptionSpec {
                names: &["--cov"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Print source coverage",
            },
            OptionSpec {
                names: &["--api"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Include API coverage, with --cov",
            },
            OptionSpec {
                names: &["-j", "--jobs"],
                arg: OptionArg::value("N"),
                repeatable: false,
                description: "Run N tests concurrently (default: half the CPUs)",
            },
            OptionSpec {
                names: &["--timeout"],
                arg: OptionArg::value("DURATION"),
                repeatable: false,
                description: "Fail a test that runs longer (default: 120s; 0 or none disables)",
            },
            OptionSpec {
                names: &["--nocapture"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Show test output",
            },
            OptionSpec {
                names: &["--fail-fast"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Stop after the first failure",
            },
            OptionSpec {
                names: &["--keep-temp"],
                arg: OptionArg::Flag,
                repeatable: false,
                description: "Preserve temporary directories",
            },
            OptionSpec {
                names: &["--cov-json"],
                arg: OptionArg::value("FILE"),
                repeatable: false,
                description: "Write coverage JSON",
            },
        ],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[],
        examples: &[],
    },
    CommandSpec {
        name: "grep",
        summary: "Search scripts with AST patterns",
        quick_label: "Search source",
        args: "PATTERN [FILE...]",
        usage: &[&[UsagePart::Args]],
        options: &[],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[
            "Uppercase identifiers are expression metavariables; ARGS.. matches zero or more arguments.",
        ],
        examples: &[
            "xsht grep 'X.len()' .",
            "xsht grep 'X.push(ITEM)' src/",
            "xsht grep 'M.set(K, V)' .",
            "xsht grep 'for NAME in ITER' .",
        ],
    },
    CommandSpec {
        name: "refactor",
        summary: "Rewrite scripts with AST patterns",
        quick_label: "Rewrite source",
        args: "PATTERN REPLACEMENT [FILE...]",
        usage: &[&[UsagePart::Opt("--dry-run"), UsagePart::Args]],
        options: &[OptionSpec {
            names: &["--dry-run"],
            arg: OptionArg::Flag,
            repeatable: false,
            description: "Show changes without modifying files",
        }],
        removed: &[],
        options_end_at_first_argument: false,
        notes: &[],
        examples: &[],
    },
];

pub(crate) fn find(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|command| command.name == name)
}

pub(crate) enum Parsed {
    /// `-h` or `--help` was reached before any argument error.
    Help,
    Args(ParsedArgs),
}

/// A command's arguments sorted into declared options and positionals.
pub(crate) struct ParsedArgs {
    command: &'static CommandSpec,
    occurrences: Vec<(&'static OptionSpec, Option<String>)>,
    pub(crate) positionals: Vec<String>,
}

impl ParsedArgs {
    /// Every occurrence of the option, in order, with its value if it took one.
    pub(crate) fn occurrences(&self, long: &str) -> Vec<Option<&str>> {
        let option = self.command.option(long);
        self.occurrences
            .iter()
            .filter(|(candidate, _)| std::ptr::eq(*candidate, option))
            .map(|(_, value)| value.as_deref())
            .collect()
    }

    pub(crate) fn flag(&self, long: &str) -> bool {
        debug_assert!(matches!(self.command.option(long).arg, OptionArg::Flag));
        !self.occurrences(long).is_empty()
    }

    /// The value of a non-repeatable option; the last occurrence wins.
    pub(crate) fn value(&self, long: &str) -> Option<&str> {
        debug_assert!(!self.command.option(long).repeatable);
        self.occurrences(long).last().copied().flatten()
    }

    /// The values of a repeatable option, in order.
    pub(crate) fn values(&self, long: &str) -> Vec<&str> {
        debug_assert!(self.command.option(long).repeatable);
        self.occurrences(long).into_iter().flatten().collect()
    }
}

/// Sorts `args` into the command's declared options and positionals.
///
/// Any other argument that starts with `-` is an unknown option. `-h` and
/// `--help` stop parsing with `Parsed::Help`, except after the script of a
/// command whose options end at its first argument.
pub(crate) fn parse_command_args(
    command: &'static CommandSpec,
    args: &[String],
) -> Result<Parsed, String> {
    let mut occurrences = Vec::new();
    let mut positionals = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if !arg.starts_with('-') {
            positionals.push(arg.clone());
            if command.options_end_at_first_argument {
                positionals.extend(rest.by_ref().cloned());
            }
            continue;
        }
        if arg == "-h" || arg == "--help" {
            return Ok(Parsed::Help);
        }
        if let Some(removed) = command.removed.iter().find(|removed| removed.name == arg) {
            return Err(removed.message.to_string());
        }
        let unknown = || format!("unknown `xsht {}` option '{arg}'", command.name);
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value)),
            _ => (arg.as_str(), None),
        };
        let option = command
            .options
            .iter()
            .find(|option| option.names.contains(&name))
            .ok_or_else(unknown)?;
        let value = match (&option.arg, inline) {
            (OptionArg::Flag, None) => None,
            (OptionArg::Flag, Some(_)) | (OptionArg::Value { equals: false, .. }, Some(_)) => {
                return Err(unknown());
            }
            (OptionArg::Value { .. }, Some(value))
            | (OptionArg::OptionalEquals { .. }, Some(value)) => Some(value.to_string()),
            (OptionArg::OptionalEquals { .. }, None) => None,
            (OptionArg::Value { metavar, .. }, None) => {
                Some(rest.next().cloned().ok_or_else(|| {
                    format!(
                        "`xsht {} {}` requires {metavar}",
                        command.name,
                        option.long()
                    )
                })?)
            }
        };
        occurrences.push((option, value));
    }
    Ok(Parsed::Args(ParsedArgs {
        command,
        occurrences,
        positionals,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_string()).collect()
    }

    fn parse(command: &str, arguments: &[&str]) -> Result<ParsedArgs, String> {
        match parse_command_args(find(command).expect("known command"), &args(arguments))? {
            Parsed::Args(parsed) => Ok(parsed),
            Parsed::Help => panic!("unexpected help"),
        }
    }

    #[test]
    fn every_declared_spelling_is_parsed_and_every_parsed_option_is_in_help() {
        for command in COMMANDS {
            let help = crate::xsht::help::command_help(command.name).expect("command help");
            for option in command.options {
                assert!(
                    help.contains(&option.syntax()),
                    "`xsht {}` help lacks {}:\n{help}",
                    command.name,
                    option.syntax()
                );
                for name in option.names {
                    let value = match option.arg {
                        OptionArg::Flag => &[][..],
                        _ => &["x"][..],
                    };
                    let mut arguments = vec![*name];
                    arguments.extend(value);
                    let parsed = parse(command.name, &arguments)
                        .unwrap_or_else(|error| panic!("`xsht {} {name}`: {error}", command.name));
                    assert_eq!(
                        parsed.occurrences(option.long()).len(),
                        1,
                        "`xsht {} {name}` is not recognized as {}",
                        command.name,
                        option.long()
                    );
                }
            }
            // Nothing else in help looks like an option-list row.
            let listed = help.lines().filter(|line| line.starts_with("  -")).count();
            assert_eq!(
                listed,
                command.options.len(),
                "`xsht {}` help",
                command.name
            );
        }
    }

    #[test]
    fn unknown_options_name_the_command_and_removed_options_keep_their_message() {
        assert_eq!(
            parse("fmt", &["--nope"]).err().as_deref(),
            Some("unknown `xsht fmt` option '--nope'")
        );
        assert!(
            parse("check", &["--strict"])
                .err()
                .is_some_and(|message| message.contains("was removed"))
        );
        // A flag takes no value, and a separate-value option has no `=` form.
        assert!(parse("fmt", &["--check=yes"]).is_err());
        assert!(parse("test", &["--jobs=2"]).is_err());
        assert!(parse("api", &["--format=jsonl"]).is_ok());
    }

    #[test]
    fn values_follow_the_declared_arity() {
        let lint = parse("lint", &["--only", "a", "--only=b", "x.xsh"]).expect("lint args");
        assert_eq!(lint.values("--only"), ["a", "b"]);
        assert_eq!(lint.positionals, ["x.xsh"]);

        let check = parse("check", &["--annotate", "--annotate=all", "p"]).expect("check args");
        assert_eq!(check.occurrences("--annotate"), [None, Some("all")]);
        assert_eq!(check.positionals, ["p"]);

        let test = parse("test", &["-j", "2", "--jobs", "3"]).expect("test args");
        assert_eq!(test.value("--jobs"), Some("3"));

        assert_eq!(
            parse("test", &["--timeout"]).err().as_deref(),
            Some("`xsht test --timeout` requires DURATION")
        );
    }

    #[test]
    fn trace_options_end_at_the_script() {
        let trace = parse("trace", &["--raw", "s.xsh", "--raw", "--help"]).expect("trace args");
        assert!(trace.flag("--raw"));
        assert_eq!(trace.positionals, ["s.xsh", "--raw", "--help"]);
        assert!(matches!(
            parse_command_args(find("trace").unwrap(), &args(&["--help"])),
            Ok(Parsed::Help)
        ));
    }

    #[test]
    fn usage_lines_are_generated_from_the_option_table() {
        let usage = |name: &str| find(name).expect("command").usage_lines();
        assert_eq!(
            usage("check"),
            [
                "xsht check [--summary] [--annotate[=default|signatures|locals|all|CLASS,...]] [PATH...]"
            ]
        );
        assert_eq!(
            usage("trace"),
            [
                "xsht trace [--raw] [--trace-format text|jsonl|flamegraph]",
                "           [--trace-file PATH] [--syscalls] [--trace-top-syscalls N]",
                "           SCRIPT [ARGS...]",
            ]
        );
        assert_eq!(
            usage("lint"),
            [
                "xsht lint [--fix] [--runless] [--only RULE[,RULE...]] [FILE...]",
                "xsht lint --list [--format text|jsonl]",
            ]
        );
    }

    /// The registry carries abbreviated command forms for `xsht api`; the
    /// options they name must exist.
    #[test]
    fn registry_command_forms_name_declared_options() {
        for form in xsh_registry::reference::CLI_FORMS {
            let mut words = form.split_whitespace();
            if words.next() != Some("xsht") {
                continue;
            }
            let command = find(words.next().expect("command word")).expect("declared command");
            for word in words {
                let word = word.trim_matches(['[', ']']);
                if let Some(option) = word.strip_prefix("--") {
                    let long = format!("--{}", option.split('=').next().unwrap_or(option));
                    command.option(&long);
                }
            }
        }
    }
}
