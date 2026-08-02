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
    "xsht api [OPTIONS] [QUERY...]",
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
use crate::api_docs::ApiDocs;

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
        let doc = run_doc(row);
        references.push(language_reference(id, doc.summary, doc.contract, doc.tags));
    }
    for row in EFFECT_REFERENCES {
        let doc = effect_doc(row.name);
        references.push(language_reference(format!("effect.{}", row.name), doc.summary, doc.contract, doc.tags));
    }
    for stage in STREAM_STAGES {
        let doc = stream_doc(stage);
        references.push(language_reference(format!("stream.{}", stage.replace('.', "-")), doc.summary, doc.contract, doc.tags));
    }
    for event in TRACE_EVENTS {
        let doc = trace_doc(event);
        references.push(language_reference(format!("trace.{event}"), doc.summary, doc.contract, doc.tags));
    }
    for form in CLI_FORMS {
        let id = form
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join("-")
            .replace(['[', ']', '.', '/'], "-")
            .replace("--", "-");
        let doc = cli_doc(form);
        references.push(language_reference(format!("cli.{id}"), doc.summary, doc.contract, doc.tags));
    }
    for item in CORE_LANGUAGE_ITEMS {
        let doc = core_doc(item);
        references.push(language_reference(format!("core.{item}"), doc.summary, doc.contract, doc.tags));
    }
    references
}

struct ReferenceDoc {
    summary: String,
    contract: String,
    tags: Vec<String>,
}

fn reference_doc(summary: &str, contract: &str, tags: &[&str]) -> ReferenceDoc {
    ReferenceDoc {
        summary: summary.to_string(),
        contract: contract.to_string(),
        tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
    }
}

fn run_doc(row: &RunFormReference) -> ReferenceDoc {
    match (row.form, row.context) {
        ("run", Some("statement position")) => reference_doc("Runs a command as a statement and requires successful completion.", "The statement form returns Unit; a nonzero child exit or setup failure propagates as ProcessError.", &["run", "process", "status-assertion"]),
        ("run", Some("value position")) => reference_doc("Runs a command in value position and returns its Status.", "A nonzero child exit is inspectable status data in value position; setup and spawn failures still propagate.", &["run", "process", "status-data"]),
        ("run.status", None) => reference_doc("Runs a command and returns structured process status.", "Nonzero exits are status data; only setup, spawn, or other process failures propagate as errors.", &["run", "process", "status-data"]),
        ("run.text", None) => reference_doc("Runs a command and captures stdout as UTF-8 text.", "Nonzero exits return ProcessError at this capture boundary, and invalid UTF-8 is also an error.", &["run", "process", "capture", "utf8"]),
        ("run.bytes", None) => reference_doc("Runs a command and captures stdout as Bytes.", "The capture preserves arbitrary output bytes; nonzero exits and process setup failures remain errors.", &["run", "process", "capture", "bytes"]),
        ("run.capture --text", None) => reference_doc("Captures command status, stdout, and stderr as text.", "The record preserves a nonzero status as data while setup failures return ProcessError; text streams must be UTF-8.", &["run", "process", "capture", "utf8"]),
        ("run.capture --bytes", None) => reference_doc("Captures command status, stdout, and stderr as Bytes.", "The record preserves arbitrary output bytes and child status; setup failures remain ProcessError.", &["run", "process", "capture", "bytes"]),
        ("run.stream --text", None) => reference_doc("Streams command stdout as UTF-8 lines.", "Consumption is lazy, invalid UTF-8 is an error, and a nonzero child exit is reported through the stream/process boundary.", &["run", "process", "streaming", "utf8"]),
        ("run.stream --bytes", None) => reference_doc("Streams command stdout as byte chunks.", "Consumption remains lazy and preserves arbitrary bytes while process setup and exit failures stay explicit.", &["run", "process", "streaming", "bytes"]),
        _ => panic!("missing run-form documentation for {}", row.form),
    }
}

fn effect_doc(name: &str) -> ReferenceDoc {
    match name {
        "fs" => reference_doc("Declares filesystem and rooted host access.", "The effect covers fs, archive, diff, patch, user, group, and module loading operations; it does not imply process or time access.", &["effect", "filesystem", "rooted"]),
        "io" => reference_doc("Declares broad input/output access.", "io covers its own streams plus fs, net, process, and env effects; it still does not cover time unless declared.", &["effect", "io", "superset"]),
        "net" => reference_doc("Declares network and DNS access.", "The effect covers net and dns module operations but does not authorize filesystem or process work.", &["effect", "net", "dns"]),
        "process" => reference_doc("Declares process creation and lifecycle access.", "The effect covers run, spawn, wait, process handles, Unix process groups, Linux process boundaries, and applet process work.", &["effect", "process", "ownership"]),
        "env" => reference_doc("Declares environment and host identity access.", "The effect covers env operations, cd, and system identity queries; lexical env overlays still follow their own scope rules.", &["effect", "env", "host-state"]),
        "time" => reference_doc("Declares clock, sleep, and timing access.", "The effect covers time APIs and delayed retry work without granting unrelated filesystem or process effects.", &["effect", "time", "clock"]),
        "error" => reference_doc("Declares explicit error propagation with postfix ?.", "The error effect is required when ? can propagate outside a retry attempt block; it does not describe a host capability.", &["effect", "error", "propagation"]),
        _ => panic!("missing effect documentation for {name}"),
    }
}

fn stream_doc(stage: &str) -> ReferenceDoc {
    let (summary, contract, tags): (&'static str, &'static str, &'static [&'static str]) = match stage {
        "where" => ("Filters stream items with a predicate block.", "Items remain in source order and only values whose predicate succeeds continue.", &["stream", "filter"]),
        "map" => ("Transforms each stream item with a block.", "Mapping preserves source order unless a later stage explicitly changes ordering.", &["stream", "transform"]),
        "par-map" => ("Transforms stream items with bounded parallel workers.", "The worker bound is explicit, output order is deterministic for the ordered form, and cancellation still runs stream cleanup.", &["stream", "parallel", "bounded", "ordered"]),
        "each" => ("Runs a side-effecting block for each stream item.", "The original item remains available to downstream stages and block failures stop the stream explicitly.", &["stream", "effect", "transform"]),
        "batch" => ("Groups stream items into bounded lists.", "The final short batch is retained and the configured batch size must be positive.", &["stream", "batch", "bounded"]),
        "sort" => ("Sorts all stream items.", "Sorting materializes the input and therefore requires a finite source and a defined item ordering.", &["stream", "sorting", "materialization"]),
        "sort-by" => ("Sorts stream items by a projected key.", "The key projection controls ordering and the stage materializes the input before emitting results.", &["stream", "sorting", "projection"]),
        "take" => ("Keeps at most a requested number of stream items.", "Taking zero stops the source promptly and runs source defers before downstream completion.", &["stream", "limit", "cleanup"]),
        "drop" => ("Skips an initial number of stream items.", "Dropped items are consumed from the source and the remainder preserves source order.", &["stream", "skip"]),
        "first" => ("Returns the first stream item.", "The source is consumed only as far as needed and absence remains distinguishable from a stored null-like value.", &["stream", "terminal", "short-circuit"]),
        "last" => ("Returns the last stream item.", "The source must be exhausted to determine the last value, so this terminal is not streaming in memory use.", &["stream", "terminal", "materialization"]),
        "unique-by" => ("Suppresses duplicate stream keys.", "The first item for each key is retained in source order and key storage grows with distinct inputs.", &["stream", "deduplication", "projection"]),
        "enumerate" => ("Adds a zero-based index to each stream item.", "Indices follow consumed source order and are not recomputed after later filtering.", &["stream", "indexing"]),
        "zip" => ("Pairs items from two streams.", "The output stops at the shorter source and both sources retain explicit cleanup ownership.", &["stream", "zip", "ownership"]),
        "range" => ("Produces an integer range as a stream.", "Start, end, and step define the half-open traversal and a zero step is invalid.", &["stream", "producer", "range"]),
        "repeat" => ("Repeats a value as a stream.", "An unbounded repeat must be limited by a downstream terminal or the caller owns the resulting nontermination.", &["stream", "producer", "unbounded"]),
        "tee" => ("Copies stream items to an explicit side-effect sink.", "The sink observes consumed items and does not make a live stream replayable.", &["stream", "side-effect", "ownership"]),
        "sum" | "min" | "max" => ("Reduces a numeric stream to one aggregate value.", "The source is consumed to completion and the item type must satisfy the terminal's numeric contract.", &["stream", "terminal", "reduction"]),
        "group-by" => ("Groups stream items by a projected key.", "The terminal materializes groups and preserves each group's source order.", &["stream", "terminal", "grouping"]),
        "fold" | "reduce" => ("Reduces stream items with an explicit accumulator block.", "Accumulator updates follow source order for the serial form; parallel variants require an associative reduction contract.", &["stream", "reduction", "accumulator"]),
        "flat-map" => ("Replaces each item with and flattens a child stream.", "Child streams are consumed under the parent stream's cleanup scope and their order is preserved.", &["stream", "transform", "flattening"]),
        "any" | "all" => ("Short-circuits a predicate over stream items.", "The terminal stops as soon as the truth value is determined and runs source cleanup on early stop.", &["stream", "terminal", "short-circuit"]),
        "shuffle" => ("Randomizes the order of all stream items.", "The input is materialized before shuffling and the result is intentionally nondeterministic.", &["stream", "terminal", "random"]),
        "table.print" => ("Renders stream records as a terminal table.", "Rendering is a terminal presentation boundary and uses terminal width policy rather than raw byte length.", &["stream", "terminal", "tui", "display"]),
        "text.lines" => ("Adapts text into a lazy line stream.", "Line decoding is UTF-8 based and consumption remains tied to the source stream lifecycle.", &["stream", "text", "utf8", "adapter"]),
        "bytes.chunks" => ("Adapts bytes into fixed-size chunks.", "Chunking is byte-oriented and retains a final short chunk when input remains.", &["stream", "bytes", "adapter"]),
        "json.lines" => ("Adapts newline-delimited JSON into a stream of values.", "Each line is parsed independently and one malformed document stops the adapter with an error.", &["stream", "json", "adapter"]),
        "json.stream" => ("Adapts a JSON array or document stream into values.", "Parsing remains structured and dynamic; callers must validate records before treating fields as typed.", &["stream", "json", "dynamic"]),
        "count" => ("Counts all items in a stream.", "The source is consumed to completion and the count is status/data, not a retained stream handle.", &["stream", "terminal", "count"]),
        "collect" => ("Materializes all stream items into a list.", "Collecting consumes the source and transfers its values into owned list storage.", &["stream", "terminal", "materialization"]),
        _ => panic!("missing stream-stage documentation for {stage}"),
    };
    reference_doc(summary, contract, tags)
}

fn trace_doc(event: &str) -> ReferenceDoc {
    let (summary, contract) = match event {
        "script.enter" => ("Records entry into a script trace scope.", "The event starts the source-anchored script containment node used by later child events."),
        "script.exit" => ("Records exit from a script trace scope.", "The event closes the matching script node after child calls and cleanup have completed."),
        "proc.enter" => ("Records entry into a user procedure trace scope.", "The event carries dynamic containment for the procedure invocation rather than only its source declaration."),
        "proc.exit" => ("Records exit from a user procedure trace scope.", "The event closes the matching procedure node after return or propagated failure."),
        "pure.enter" => ("Records entry into a pure-function trace scope.", "The event preserves pure-call nesting while the checker still enforces its no-effect boundary."),
        "pure.exit" => ("Records exit from a pure-function trace scope.", "The event closes the matching pure call and preserves failure containment."),
        "core.call" => ("Records a call into a core language operation.", "The event identifies the operation boundary that produced the corresponding core.result event."),
        "core.result" => ("Records the result of a core language operation.", "The event retains success or failure relation to its core.call parent."),
        "module.call" => ("Records entry into a standard module operation.", "The event carries the module operation identity and host-effect containment for its call."),
        "module.result" => ("Records the result of a standard module operation.", "The event preserves the module call's returned value or structured failure relation."),
        "method.call" => ("Records entry into a value method operation.", "The event identifies the receiver and method boundary rather than flattening it into a generic evaluator node."),
        "method.result" => ("Records the result of a value method operation.", "The event closes the matching method call with its status or value relation."),
        "run.start" => ("Records the start of a process run boundary.", "The event anchors command, argv, redirection, and ambient process context before host execution."),
        "run.end" => ("Records completion of a process run boundary.", "The event preserves status-as-data separately from setup or propagated process failure."),
        "stream.stage.enter" => ("Records entry into a structured stream stage.", "The event identifies stage ordering and source containment for lazy stream execution."),
        "stream.stage.exit" => ("Records completion of a structured stream stage.", "The event closes stage containment after values, cancellation, or failure have propagated."),
        _ => panic!("missing trace-event documentation for {event}"),
    };
    reference_doc(summary, contract, &["trace", event])
}

fn cli_doc(form: &str) -> ReferenceDoc {
    let (summary, contract) = match form {
        "xsh SCRIPT [ARGS...]" => ("Runs an XSH script with explicit positional arguments.", "Script arguments remain argv values and the runner reports the script's final status without shell expansion."),
        "xsh -- SCRIPT ARGS..." => ("Runs an XSH script after an explicit option terminator.", "The double dash separates runner options from script argv and is preserved for shebang-style invocation compatibility."),
        "xshi" => ("Starts the interactive XSH-compatible session frontend.", "Normal startup requires a terminal; session state such as cwd and aliases belongs to the interactive process."),
        "xsht check [--strict] [--summary] [--annotate] [PATH...]" => ("Checks XSH sources and reports semantic diagnostics.", "check uses the shared parser/checker pipeline; strictness, summaries, and source annotation are explicit command options."),
        "xsht fmt [--check] [FILE...]" => ("Formats or checks XSH source files.", "fmt validates the checked program before writing and --check reports drift without rewriting files."),
        "xsht lint [--fix] [--runless] [FILE...]" => ("Reports XSH lint diagnostics and optional safe fixes.", "Lint fixes are CST-guarded source edits and require validation after application; runless changes effect policy only."),
        "xsht ast SCRIPT" => ("Prints the parsed XSH syntax tree for a script.", "The command is inspection-only and does not execute the script."),
        "xsht trace [--raw] [--trace-format text|jsonl|flamegraph] [--trace-file FILE] [--syscalls] [--trace-top-syscalls N] SCRIPT [ARGS...]" => ("Runs a script and renders its runtime trace.", "Trace output and syscall collection are explicit diagnostics; the traced script still receives its own argv and status contract."),
        "xsht test [--cov] [OPTIONS] [FILTER]" => ("Discovers and runs native XSH tests.", "Test filtering, coverage, parallelism, and retained temporary roots are harness policy rather than script runtime behavior."),
        "xsht api [OPTIONS] [QUERY...]" => ("Queries the standalone XSH language and API reference.", "With no selector it prints a getting-started guide; selectors can inspect language rules, modules, exact operations, records, or search terms, with text and JSONL output available."),
        _ => panic!("missing CLI-form documentation for {form}"),
    };
    reference_doc(summary, contract, &["cli", form])
}

fn core_doc(item: &str) -> ReferenceDoc {
    let (summary, contract) = match item {
        "source-files" => ("Defines source-file and module loading boundaries.", "Source files are parsed and checked as modules; imports retain their source boundaries for diagnostics and runtime loading."),
        "comments" => ("Defines XSH comments and documentation comments.", "Documentation comments participate in exported-module contracts while ordinary comments remain source trivia."),
        "statements" => ("Defines statement sequencing and result propagation.", "Statement position applies the language's success and error propagation rules rather than silently discarding Result values."),
        "bindings" => ("Defines typed bindings and assignment scope.", "Bindings have declared mutability and type boundaries; reassignment cannot create an invalid inferred state."),
        "procs" => ("Defines procedure declarations and calls.", "Procedure calls preserve lexical scope, declared effects, return types, and runtime trace containment."),
        "pure-functions" => ("Defines effect-free function declarations.", "Pure functions cannot cross host-effect boundaries and retain a distinct trace/evaluation contract."),
        "records" => ("Defines structural and named record values.", "Named records are checked at their boundary; dynamic record access must be narrowed before typed field use."),
        "results" => ("Defines Result values and error families.", "Expected host failures remain Result data until ? or another explicit boundary propagates them."),
        "postfix-question" => ("Defines postfix ? error propagation.", "? unwraps a Result or returns its error and requires the declared error effect outside retry attempt blocks."),
        "fallback" => ("Defines fallback expressions for recoverable values.", "Fallback applies only to the documented missing/failed shape and does not erase unrelated errors."),
        "run" => ("Defines process run forms and status boundaries.", "Statement, value, status, capture, and stream forms differ in whether a nonzero child exit is asserted, returned, or wrapped as Result data."),
        "captures" => ("Defines lexical captures for blocks and procedures.", "Captured bindings retain lexical ownership and cannot be used to smuggle ambient mutable state across a declared boundary."),
        "streams" => ("Defines lazy structured stream values.", "A stream owns source cleanup until a terminal, cancellation, or failure consumes that lifecycle."),
        "native-tests" => ("Defines native XSH test declarations and harness context.", "Native tests run through the same checked runtime and expose test-only host helpers only in the native-test feature."),
        "command-interpolation" => ("Defines explicit command and argv interpolation.", "Interpolated values remain typed argv boundaries; XSH does not perform implicit shell evaluation or word splitting."),
        "path-literals" => ("Defines typed path literals.", "A path literal is a Path value and crosses into text or host bytes only through an explicit conversion."),
        "glob-literals" => ("Defines filesystem glob literals.", "Glob expansion is an explicit filesystem operation with deterministic path values rather than shell word splitting."),
        "display-strings" => ("Defines display-string interpolation.", "Display strings are presentation text: they interpolate with `${expr}` and do not become command argv or filesystem paths implicitly. Ordinary expression string literals never interpolate, so `$name` inside `\"...\"` is literal text; `lint.dollar-in-expression-string` warns when it names an in-scope binding, and raw strings keep `$` literal."),
        _ => panic!("missing core-language documentation for {item}"),
    };
    reference_doc(summary, contract, &["language", item])
}

fn language_reference(
    id: String,
    summary: String,
    contract: String,
    tags: Vec<String>,
) -> LanguageReference {
    LanguageReference {
        id: id.clone(),
        docs: ApiDocs {
            summary,
            contract,
            example: crate::examples::source(&format!("language.{id}")),
            tags,
        },
    }
}
