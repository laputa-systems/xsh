#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunFormReference {
    pub form: &'static str,
    pub context: Option<&'static str>,
    pub returns: &'static str,
    pub nonzero_exit: &'static str,
    pub failure: &'static str,
}

pub const RUN_FORM_REFERENCES: &[RunFormReference] = &[
    RunFormReference {
        form: "run",
        context: Some("statement position"),
        returns: "Unit",
        nonzero_exit: "propagates ProcessError",
        failure: "propagates ProcessError",
    },
    RunFormReference {
        form: "run",
        context: Some("value position"),
        returns: "Status",
        nonzero_exit: "status data",
        failure: "propagates ProcessError",
    },
    RunFormReference {
        form: "run.status",
        context: None,
        returns: "Status",
        nonzero_exit: "status data",
        failure: "propagates ProcessError",
    },
    RunFormReference {
        form: "run.text",
        context: None,
        returns: "Result[Str, ProcessError]",
        nonzero_exit: "Err(ProcessError)",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.bytes",
        context: None,
        returns: "Result[Bytes, ProcessError]",
        nonzero_exit: "Err(ProcessError)",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.capture --text",
        context: None,
        returns: "Result[{status, stdout: Str, stderr: Str}, ProcessError]",
        nonzero_exit: "Ok(record) with status",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.capture --bytes",
        context: None,
        returns: "Result[{status, stdout: Bytes, stderr: Bytes}, ProcessError]",
        nonzero_exit: "Ok(record) with status",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.stream --text",
        context: None,
        returns: "Result[Stream[Str], ProcessError]",
        nonzero_exit: "Err(ProcessError)",
        failure: "Err(ProcessError)",
    },
    RunFormReference {
        form: "run.stream --bytes",
        context: None,
        returns: "Result[Stream[Bytes], ProcessError]",
        nonzero_exit: "Err(ProcessError)",
        failure: "Err(ProcessError)",
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectReference {
    pub name: &'static str,
    pub covers: &'static [&'static str],
}

pub const EFFECT_REFERENCES: &[EffectReference] = &[
    EffectReference {
        name: "fs",
        covers: &[
            "fs.*",
            "archive.*",
            "diff.*",
            "patch.*",
            "user.*",
            "group.*",
            "module.*",
        ],
    },
    EffectReference {
        name: "io",
        covers: &[
            "io.*",
            "superset of fs",
            "superset of net",
            "superset of process",
            "superset of env",
        ],
    },
    EffectReference {
        name: "net",
        covers: &["net.*", "dns.*"],
    },
    EffectReference {
        name: "process",
        covers: &[
            "run",
            "spawn",
            "wait",
            "ProcessHandle.cancel",
            "effectful process.*",
            "unix.*",
            "linux.*",
            "applet.*",
        ],
    },
    EffectReference {
        name: "env",
        covers: &["env.*", "cd", "system.*"],
    },
    EffectReference {
        name: "time",
        covers: &["time.*", "delayed retry blocks"],
    },
    EffectReference {
        name: "error",
        covers: &["? propagation outside retry attempt blocks"],
    },
];

pub const STREAM_STAGES: &[&str] = &[
    "where",
    "map",
    "par-map",
    "each",
    "batch",
    "sort",
    "sort-by",
    "take",
    "drop",
    "first",
    "last",
    "unique-by",
    "enumerate",
    "zip",
    "range",
    "repeat",
    "tee",
    "sum",
    "min",
    "max",
    "group-by",
    "fold",
    "reduce",
    "flat-map",
    "any",
    "all",
    "shuffle",
    "table.print",
    "text.lines",
    "bytes.chunks",
    "json.lines",
    "json.stream",
    "count",
    "collect",
];

pub const TRACE_EVENTS: &[&str] = &[
    "script.enter",
    "script.exit",
    "proc.enter",
    "proc.exit",
    "pure.enter",
    "pure.exit",
    "core.call",
    "core.result",
    "module.call",
    "module.result",
    "method.call",
    "method.result",
    "run.start",
    "run.end",
    "stream.stage.enter",
    "stream.stage.exit",
];

pub const CLI_FORMS: &[&str] = &[
    "xsh SCRIPT [ARGS...]",
    "xsh -- SCRIPT ARGS...",
    "xshi",
    "xsht check [--strict] [--summary] [--annotate] [PATH...]",
    "xsht fmt [--check] [FILE...]",
    "xsht lint [--fix] [--runless] [FILE...]",
    "xsht ast SCRIPT",
    "xsht trace [--raw] [--trace-format text|jsonl|flamegraph] [--trace-file FILE] [--syscalls] [--trace-top-syscalls N] SCRIPT [ARGS...]",
    "xsht test [--cov] [OPTIONS] [FILTER]",
    "xsht api [OPTIONS] QUERY...",
];

pub const CORE_LANGUAGE_ITEMS: &[&str] = &[
    "source-files",
    "comments",
    "statements",
    "bindings",
    "procs",
    "pure-functions",
    "records",
    "results",
    "postfix-question",
    "fallback",
    "run",
    "captures",
    "streams",
    "native-tests",
    "command-interpolation",
    "path-literals",
    "glob-literals",
    "display-strings",
];
use crate::api_docs::{ApiDocs, ApiNavigation};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageReference {
    pub id: String,
    pub docs: ApiDocs,
}

pub fn language_references() -> Vec<LanguageReference> {
    let mut references = Vec::new();
    for row in RUN_FORM_REFERENCES {
        let id = if row.form == "run" {
            format!("run.{}", row.context.unwrap_or("default").replace(' ', "-"))
        } else {
            row.form.replace(' ', "-")
        };
        references.push(language_reference(
            id,
            format!("`{}` run form.", row.form),
            format!(
                "Returns {}; nonzero exits are {}; setup, spawn, and capture failures are {}.",
                row.returns, row.nonzero_exit, row.failure
            ),
            vec!["run".to_string(), row.form.to_string()],
            "docs/SPEC.md",
            "tests/xsh/run.xsh",
        ));
    }
    for row in EFFECT_REFERENCES {
        references.push(language_reference(
            format!("effect.{}", row.name),
            format!("`{}` effect.", row.name),
            format!("Covers {}.", row.covers.join(", ")),
            vec!["effect".to_string(), row.name.to_string()],
            "docs/SPEC.md",
            "tests/xsh/effects.xsh",
        ));
    }
    for stage in STREAM_STAGES {
        references.push(language_reference(
            format!("stream.{}", stage.replace('.', "-")),
            format!("`{stage}` structured stream stage."),
            String::new(),
            vec!["stream".to_string(), stage.to_string()],
            "docs/STREAMS.md",
            "tests/xsh/stdlib/streams.xsh",
        ));
    }
    for event in TRACE_EVENTS {
        references.push(language_reference(
            format!("trace.{event}"),
            format!("`{event}` trace event."),
            String::new(),
            vec!["trace".to_string(), event.to_string()],
            "docs/SPEC.md",
            "tests/xsh/run.xsh",
        ));
    }
    for form in CLI_FORMS {
        let id = form
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join("-")
            .replace(['[', ']', '.', '/'], "-")
            .replace("--", "-");
        references.push(language_reference(
            format!("cli.{id}"),
            format!("`{form}` command form."),
            String::new(),
            vec!["cli".to_string(), form.to_string()],
            "docs/XSHT.md",
            "tests/runtime/run.rs",
        ));
    }
    for item in CORE_LANGUAGE_ITEMS {
        references.push(language_reference(
            format!("core.{item}"),
            format!("Core language item `{item}`."),
            String::new(),
            vec!["language".to_string(), item.to_string()],
            "docs/SPEC.md",
            "tests/xsh/basic.xsh",
        ));
    }
    references
}

fn language_reference(
    id: String,
    summary: String,
    contract: String,
    tags: Vec<String>,
    implementation: &str,
    tests: &str,
) -> LanguageReference {
    LanguageReference {
        id,
        docs: ApiDocs {
            summary,
            contract,
            tags,
            navigation: ApiNavigation {
                implementation: vec![implementation.to_string()],
                tests: vec![tests.to_string()],
                showcase: None,
            },
        },
    }
}
