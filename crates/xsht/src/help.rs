use std::fmt::Write as _;

struct HelpOption {
    syntax: &'static str,
    description: &'static str,
}

struct CommandHelp {
    name: &'static str,
    summary: &'static str,
    quick_label: &'static str,
    quick_usage: &'static str,
    usage: &'static [&'static str],
    options: &'static [HelpOption],
    notes: &'static [&'static str],
    examples: &'static [&'static str],
}

static COMMANDS: &[CommandHelp] = &[
    CommandHelp {
        name: "check",
        summary: "Parse and type-check scripts",
        quick_label: "Validate source",
        quick_usage: "check [PATH...]",
        usage: &[
            "xsht check [--strict] [--summary] [--annotate[=default|signatures|locals|all|CLASS,...]] [PATH...]",
        ],
        options: &[
            HelpOption {
                syntax: "--strict",
                description: "Enable strict dynamic-data diagnostics",
            },
            HelpOption {
                syntax: "--summary",
                description: "Append diagnostic counts by code",
            },
            HelpOption {
                syntax: "--annotate[=POLICY]",
                description: "Apply inferred annotations in place",
            },
        ],
        notes: &[],
        examples: &[],
    },
    CommandHelp {
        name: "fmt",
        summary: "Format scripts",
        quick_label: "Format source",
        quick_usage: "fmt [FILE...]",
        usage: &["xsht fmt [--check] [FILE...]"],
        options: &[HelpOption {
            syntax: "--check",
            description: "Check formatting without rewriting",
        }],
        notes: &[],
        examples: &[],
    },
    CommandHelp {
        name: "lint",
        summary: "Run quality checks and optional fixes",
        quick_label: "Improve source",
        quick_usage: "lint [FILE...]",
        usage: &["xsht lint [--fix] [--runless] [FILE...]"],
        options: &[
            HelpOption {
                syntax: "--fix",
                description: "Apply safe autofixes",
            },
            HelpOption {
                syntax: "--runless",
                description: "Reject external commands unless configured",
            },
        ],
        notes: &[],
        examples: &[],
    },
    CommandHelp {
        name: "ast",
        summary: "Print parser debug output",
        quick_label: "Inspect syntax",
        quick_usage: "ast SCRIPT",
        usage: &["xsht ast SCRIPT"],
        options: &[],
        notes: &[],
        examples: &[],
    },
    CommandHelp {
        name: "trace",
        summary: "Run a script with trace output",
        quick_label: "Run with tracing",
        quick_usage: "trace SCRIPT [ARGS...]",
        usage: &[
            "xsht trace [--raw] [--trace-format text|jsonl|flamegraph]",
            "           [--trace-file PATH] [--syscalls] [--trace-top-syscalls N]",
            "           SCRIPT [ARGS...]",
        ],
        options: &[
            HelpOption {
                syntax: "--raw",
                description: "Write per-event trace output",
            },
            HelpOption {
                syntax: "--trace-format FORMAT",
                description: "text, jsonl, or flamegraph",
            },
            HelpOption {
                syntax: "--trace-file PATH",
                description: "Write trace output to PATH",
            },
            HelpOption {
                syntax: "--syscalls",
                description: "Include native syscall totals",
            },
            HelpOption {
                syntax: "--trace-top-syscalls N",
                description: "Show N syscall rows; default: 8",
            },
        ],
        notes: &[],
        examples: &[],
    },
    CommandHelp {
        name: "api",
        summary: "Query language and standard-library metadata",
        quick_label: "Query the API",
        quick_usage: "api [QUERY...]",
        usage: &["xsht api [OPTIONS] [QUERY...]"],
        options: &[
            HelpOption {
                syntax: "--format FORMAT",
                description: "text or jsonl",
            },
            HelpOption {
                syntax: "--strict",
                description: "Fail when a selector has no match",
            },
            HelpOption {
                syntax: "--details LEVEL",
                description: "basic or full",
            },
            HelpOption {
                syntax: "--query-file PATH",
                description: "Read selectors from a file",
            },
            HelpOption {
                syntax: "--stdin",
                description: "Read selectors from stdin",
            },
        ],
        notes: &[
            "Queries:",
            "  summary | module:NAME | api:MODULE.FUNCTION",
            "  method:RECEIVER.METHOD | record:NAME | language:ID | search:TERMS",
        ],
        examples: &[],
    },
    CommandHelp {
        name: "test",
        summary: "Run native and cataloged tests",
        quick_label: "Run tests",
        quick_usage: "test [FILTER]",
        usage: &["xsht test [OPTIONS] [FILTER]"],
        options: &[
            HelpOption {
                syntax: "--examples",
                description: "Run cataloged examples",
            },
            HelpOption {
                syntax: "--all",
                description: "Run native and cataloged tests",
            },
            HelpOption {
                syntax: "--list",
                description: "List matching tests",
            },
            HelpOption {
                syntax: "--exact",
                description: "Match FILTER exactly",
            },
            HelpOption {
                syntax: "--cov",
                description: "Print source coverage",
            },
            HelpOption {
                syntax: "--api",
                description: "Include API coverage",
            },
            HelpOption {
                syntax: "-j, --jobs N",
                description: "Run tests concurrently",
            },
            HelpOption {
                syntax: "--nocapture",
                description: "Show test output",
            },
            HelpOption {
                syntax: "--fail-fast",
                description: "Stop after the first failure",
            },
            HelpOption {
                syntax: "--keep-temp",
                description: "Preserve temporary directories",
            },
            HelpOption {
                syntax: "--cov-json FILE",
                description: "Write API coverage JSON",
            },
        ],
        notes: &[],
        examples: &[],
    },
    CommandHelp {
        name: "grep",
        summary: "Search scripts with AST patterns",
        quick_label: "Search source",
        quick_usage: "grep PATTERN [FILE...]",
        usage: &["xsht grep PATTERN [FILE...]"],
        options: &[],
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
    CommandHelp {
        name: "refactor",
        summary: "Rewrite scripts with AST patterns",
        quick_label: "Rewrite source",
        quick_usage: "refactor PATTERN REPLACEMENT [FILE...]",
        usage: &["xsht refactor PATTERN REPLACEMENT [FILE...]"],
        options: &[HelpOption {
            syntax: "--dry-run",
            description: "Show changes without modifying files",
        }],
        notes: &[],
        examples: &[],
    },
];

pub(crate) fn root_help() -> String {
    let mut help = String::new();
    writeln!(help, "xsht {}", env!("CARGO_PKG_VERSION")).expect("write help heading");
    help.push_str(
        r#"
Usage:
  xsht <COMMAND> [OPTIONS]
  xsht -h | --help
  xsht <COMMAND> -h | --help
  xsht help [COMMAND]

Start here:
"#,
    );

    for command in COMMANDS {
        writeln!(
            help,
            "  {:<18} xsht {}",
            command.quick_label, command.quick_usage
        )
        .expect("write help quick start");
    }

    help.push_str("\nCommand reference:\n\n");
    for (index, command) in COMMANDS.iter().enumerate() {
        render_command(&mut help, command, false);
        if index + 1 < COMMANDS.len() {
            help.push('\n');
        }
    }

    help.push_str(
        r#"
Common workflows:

  xsht check .
  xsht fmt --check .
  xsht lint --fix .
  xsht test --cov
"#,
    );
    help
}

pub(crate) fn command_help(name: &str) -> Option<String> {
    let command = COMMANDS.iter().find(|command| command.name == name)?;
    let mut help = String::new();
    render_command(&mut help, command, true);
    Some(help)
}

fn render_command(help: &mut String, command: &CommandHelp, standalone: bool) {
    if standalone {
        writeln!(
            help,
            "xsht {} — {}\n\nUsage:",
            command.name, command.summary
        )
        .expect("write help command heading");
    } else {
        writeln!(help, "{} — {}", command.name, command.summary)
            .expect("write help command heading");
    }

    for usage in command.usage {
        writeln!(help, "  {usage}").expect("write help usage");
    }

    if !command.notes.is_empty() {
        help.push('\n');
        for note in command.notes {
            writeln!(help, "  {note}").expect("write help note");
        }
    }

    if !command.options.is_empty() {
        help.push('\n');
        render_options(help, command.options);
    }

    if !command.examples.is_empty() {
        help.push_str("\n  Examples:\n");
        for example in command.examples {
            writeln!(help, "    {example}").expect("write help example");
        }
    }
}

fn render_options(help: &mut String, options: &[HelpOption]) {
    let width = options
        .iter()
        .map(|option| option.syntax.len())
        .max()
        .unwrap_or(0);
    for option in options {
        writeln!(
            help,
            "  {:width$}  {}",
            option.syntax,
            option.description,
            width = width
        )
        .expect("write help option");
    }
}
