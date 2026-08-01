#![allow(clippy::single_call_fn, dead_code)]

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use xsh::modules::api_spec;
use xsh::modules::signature::{
    MethodReceiver, MethodReturn, ModuleFnSig, ModuleSig, NamedMethodSigs,
};
use xsh::sema::records::record_schemas;
use xsh::sema::types::Type;
use xsh_registry::reference::{EFFECT_REFERENCES, RUN_FORM_REFERENCES};

fn insertion_sort_by<T>(items: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 && compare(&items[j], &items[j - 1]) == Ordering::Less {
            items.swap(j, j - 1);
            j -= 1;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub contents: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocsReport {
    pub generated: Vec<GeneratedFile>,
}

pub fn build(root: impl AsRef<Path>) -> Result<DocsReport, String> {
    let root = root.as_ref();
    let report = generate(root)?;
    for generated in &report.generated {
        let path = root.join(&generated.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create '{}': {err}", parent.display()))?;
        }
        fs::write(&path, &generated.contents)
            .map_err(|err| format!("failed to write '{}': {err}", path.display()))?;
    }
    Ok(report)
}

pub fn check(root: impl AsRef<Path>) -> Result<DocsReport, String> {
    let root = root.as_ref();
    let report = generate(root)?;
    let mut errors = Vec::new();

    for generated in &report.generated {
        let path = root.join(&generated.path);
        match fs::read_to_string(&path) {
            Ok(existing) if existing == generated.contents => {}
            Ok(_) => errors.push(format!("{} is stale", generated.path.display())),
            Err(err) => errors.push(format!(
                "{} is missing or unreadable: {err}",
                generated.path.display()
            )),
        }
    }

    if errors.is_empty() {
        Ok(report)
    } else {
        Err(errors.join("\n"))
    }
}

pub fn reference_markdown() -> Result<String, String> {
    let mut output = String::new();
    output.push_str("# XSH Reference\n\n");
    output.push_str("Generated from the docs engine. Do not edit by hand.\n\n");
    output.push_str("This file covers non-stdlib reference data. See `docs/STDLIB.md` for standard modules, value methods, and standard record schemas.\n\n");
    output.push_str("## Machine Index\n\n");
    output.push_str("```xsh-reference\n");
    output.push_str("kind: reference\nversion: 1\n");
    output.push_str(
        "sections: stream-stages, run-forms, effects, cli-forms, trace-events, language\n",
    );
    output.push_str("```\n\n");
    output.push_str("## Stream Stages\n\n");
    output.push_str("```xsh-reference\n");
    for stage in stream_stages() {
        output.push_str("stage: ");
        output.push_str(stage);
        output.push('\n');
    }
    output.push_str("```\n\n");
    output.push_str("## Run Forms\n\n");
    output.push_str("| Form | Returns | Nonzero Exit | Setup/Spawn/Capture Failure |\n");
    output.push_str("|---|---|---|---|\n");
    for row in RUN_FORM_REFERENCES {
        output.push('|');
        output.push_str(&run_form_label(row));
        output.push('|');
        output.push_str(&code_span(row.returns));
        output.push('|');
        output.push_str(&code_words(row.nonzero_exit));
        output.push('|');
        output.push_str(&code_words(row.failure));
        output.push_str("|\n");
    }
    output.push('\n');
    output.push_str("```xsh-reference\n");
    for row in RUN_FORM_REFERENCES {
        output.push_str("run: ");
        output.push_str(&run_form_label(row));
        output.push_str(" -> ");
        output.push_str(&code_span(row.returns));
        output.push('\n');
    }
    output.push_str("```\n\n");
    output.push_str("## Effects\n\n");
    output.push_str("| Effect | Covers |\n");
    output.push_str("|---|---|\n");
    for row in EFFECT_REFERENCES {
        output.push('|');
        output.push_str(&code_span(row.name));
        output.push('|');
        output.push_str(&effect_covers_markdown(row.covers));
        output.push_str("|\n");
    }
    output.push('\n');
    output.push_str("```xsh-reference\n");
    for row in EFFECT_REFERENCES {
        output.push_str("effect: ");
        output.push_str(&code_span(row.name));
        output.push_str(" -> ");
        output.push_str(&effect_covers_markdown(row.covers));
        output.push('\n');
    }
    output.push_str("```\n\n");
    output.push_str("## CLI Forms\n\n");
    output.push_str("```xsh-reference\n");
    for form in cli_forms() {
        output.push_str("cli: ");
        output.push_str(form);
        output.push('\n');
    }
    output.push_str("```\n\n");
    output.push_str("## Trace Events\n\n");
    output.push_str("```xsh-reference\n");
    for event in trace_events() {
        output.push_str("trace: ");
        output.push_str(event);
        output.push('\n');
    }
    output.push_str("```\n\n");
    output.push_str("## Core Language\n\n");
    output.push_str("```xsh-reference\n");
    for item in core_language_items() {
        output.push_str("language: ");
        output.push_str(item);
        output.push('\n');
    }
    output.push_str("```\n");
    Ok(output)
}

pub fn stdlib_markdown() -> Result<String, String> {
    let mut output = String::new();
    output.push_str("# XSH Standard Library\n\n");
    output.push_str("Generated from `src/modules/signature.rs`, `src/sema/records.rs`, and the docs engine. Do not edit by hand.\n\n");
    output.push_str("This file is the generated standard-library manual for modules, value methods, and standard record schemas. See `STDLIB-PROPOSALS.md` for stdlib design criteria and open proposals. See `docs/REFERENCE.md` for non-stdlib language and tooling reference data.\n\n");

    output.push_str("## Module Index\n\n");
    let modules = sorted_modules();
    for (module_name, module) in &modules {
        output.push_str("- `");
        output.push_str(module_name);
        output.push_str("` - ");
        output.push_str(module_summary(module_name));
        output.push_str(" (");
        output.push_str(&module.functions.len().to_string());
        output.push_str(" function(s))\n");
    }
    output.push('\n');

    output.push_str("## Modules\n\n");
    for (module_name, module) in modules {
        output.push_str("### `");
        output.push_str(module_name);
        output.push_str("`\n\n");
        output.push_str(module_summary(module_name));
        output.push_str("\n\n");
        let mut functions = module.functions.iter().collect::<Vec<_>>();
        insertion_sort_by(&mut functions, |left, right| left.name.cmp(right.name));
        for function in functions {
            for (index, sig) in function.overloads.iter().enumerate() {
                output.push_str("- `");
                output.push_str(&module_signature(module_name, function.name, sig));
                output.push('`');
                output.push_str(" - ");
                output.push_str(if sig.pure { "pure" } else { "effect" });
                if sig.command {
                    output.push_str(", command");
                }
                output.push_str("; ");
                output.push_str(&return_doc(&sig.return_ty));
                output.push_str(" ID `");
                output.push_str(&module_api_id(module_name, function.name, index));
                output.push_str("`.\n");
                if !sig.params.is_empty() {
                    output.push_str("  Params: ");
                    output.push_str(&param_markdown(&sig.params));
                    output.push('\n');
                }
            }
        }
        output.push('\n');
    }

    output.push_str("## Method Index\n\n");
    let receivers = sorted_method_receivers();
    for (receiver, methods) in &receivers {
        output.push_str("- `");
        output.push_str(receiver_name(*receiver));
        output.push_str("` - ");
        output.push_str(&methods.len().to_string());
        output.push_str(" method(s)\n");
    }
    output.push('\n');

    output.push_str("## Methods\n\n");
    for (receiver, methods) in receivers {
        output.push_str("### `");
        output.push_str(receiver_name(receiver));
        output.push_str("` Methods\n\n");
        let mut methods = methods.iter().collect::<Vec<_>>();
        insertion_sort_by(&mut methods, |left, right| left.name.cmp(right.name));
        for method_entry in methods {
            for (index, method) in method_entry.overloads.iter().enumerate() {
                output.push_str("- `");
                output.push_str(&method_signature(receiver, method_entry.name, method));
                output.push_str("` - ");
                output.push_str(if method.sig.pure { "pure" } else { "effect" });
                output.push_str("; ");
                output.push_str(&method_return_doc(&method.return_ty));
                output.push_str(" ID `");
                output.push_str(&method_api_id(receiver, method_entry.name, index));
                output.push_str("`.\n");
                if !method.sig.params.is_empty() {
                    output.push_str("  Params: ");
                    output.push_str(&param_markdown(&method.sig.params));
                    output.push('\n');
                }
            }
        }
        output.push('\n');
    }

    output.push_str("## Records\n\n");
    for (name, ty) in record_schemas() {
        output.push_str("### `");
        output.push_str(name);
        output.push_str("`\n\n");
        let Type::Record(fields) = ty else {
            return Err(format!("record schema '{name}' is not a record"));
        };
        for (field, ty) in fields {
            output.push_str("- `");
            output.push_str(field.as_str().as_str());
            output.push_str(": ");
            output.push_str(&render_type(&ty));
            output.push_str("`\n");
        }
        output.push('\n');
    }
    if output.ends_with("\n\n") {
        output.pop();
    }

    Ok(output)
}

fn generate(_: &Path) -> Result<DocsReport, String> {
    let mut errors = Vec::new();
    validate_api_docs(&mut errors);
    validate_record_docs(&mut errors);

    let generated = vec![
        GeneratedFile {
            path: PathBuf::from("docs/STDLIB.md"),
            contents: stdlib_markdown()?,
        },
        GeneratedFile {
            path: PathBuf::from("docs/REFERENCE.md"),
            contents: reference_markdown()?,
        },
    ];

    if errors.is_empty() {
        Ok(DocsReport { generated })
    } else {
        Err(errors.join("\n"))
    }
}

fn validate_api_docs(errors: &mut Vec<String>) {
    for name in api_spec().module_names() {
        if api_spec().module(name).is_none() {
            errors.push(format!("standard module '{name}' is missing from API docs"));
        }
    }
    for (module_name, module) in api_spec().module_entries() {
        if module_summary(module_name).is_empty() {
            errors.push(format!("module '{module_name}' is missing a summary"));
        }
        for function in &module.functions {
            for (index, sig) in function.overloads.iter().enumerate() {
                let id = module_api_id(module_name, function.name, index);
                if function_summary(module_name, function.name).is_empty() {
                    errors.push(format!("API '{id}' is missing a summary"));
                }
                if return_doc(&sig.return_ty).is_empty() {
                    errors.push(format!("API '{id}' is missing return docs"));
                }
                for param in &sig.params {
                    if param_doc(param.name, param.defaulted).is_empty() {
                        errors.push(format!(
                            "API '{id}' parameter '{}' is missing docs",
                            param.name
                        ));
                    }
                }
            }
        }
    }
    for (receiver, methods) in api_spec().method_entries() {
        for method_entry in methods {
            for (index, method) in method_entry.overloads.iter().enumerate() {
                let id = method_api_id(receiver, method_entry.name, index);
                if method_summary(receiver, method_entry.name).is_empty() {
                    errors.push(format!("method '{id}' is missing a summary"));
                }
                if method_return_doc(&method.return_ty).is_empty() {
                    errors.push(format!("method '{id}' is missing return docs"));
                }
            }
        }
    }
}

fn validate_record_docs(errors: &mut Vec<String>) {
    for (name, ty) in record_schemas() {
        let Type::Record(fields) = ty else {
            errors.push(format!("record schema '{name}' is not a record"));
            continue;
        };
        if fields.is_empty() {
            errors.push(format!("record schema '{name}' has no fields"));
        }
        for field in fields.keys() {
            if field.as_str().as_str().trim().is_empty() {
                errors.push(format!("record schema '{name}' has an empty field name"));
            }
        }
    }
}

fn sorted_modules() -> Vec<(&'static str, &'static ModuleSig)> {
    let mut modules = api_spec().module_entries().collect::<Vec<_>>();
    insertion_sort_by(&mut modules, |left, right| left.0.cmp(right.0));
    modules
}

fn sorted_method_receivers() -> Vec<(MethodReceiver, &'static [NamedMethodSigs])> {
    let mut receivers = api_spec().method_entries().collect::<Vec<_>>();
    insertion_sort_by(&mut receivers, |(receiver, _), (right_key, _)| {
        receiver_name(*receiver).cmp(receiver_name(*right_key))
    });
    receivers
}

fn module_signature(module: &str, function: &str, sig: &ModuleFnSig) -> String {
    format!(
        "{module}.{function}({}) -> {}",
        render_params(&sig.params),
        render_type(&sig.return_ty)
    )
}

fn method_signature(
    receiver: MethodReceiver,
    method: &str,
    sig: &xsh::modules::signature::MethodSig,
) -> String {
    let return_ty = match &sig.return_ty {
        MethodReturn::Type(ty) => render_type(ty),
        MethodReturn::Receiver => "Self".to_string(),
    };
    format!(
        "{}.{method}({}) -> {return_ty}",
        receiver_name(receiver),
        render_params(&sig.sig.params)
    )
}

fn param_markdown(params: &[xsh::modules::signature::ParamSig]) -> String {
    params
        .iter()
        .map(|param| {
            let suffix = if param.defaulted { " = default" } else { "" };
            format!("`{}: {}{}`", param.name, render_type(&param.ty), suffix)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_params(params: &[xsh::modules::signature::ParamSig]) -> String {
    params
        .iter()
        .map(|param| {
            let suffix = if param.defaulted { " = default" } else { "" };
            format!("{}: {}{}", param.name, render_type(&param.ty), suffix)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_type(ty: &Type) -> String {
    match ty {
        Type::Any => "Any".to_string(),
        Type::Unknown => "Unknown".to_string(),
        Type::Invalid => "<invalid>".to_string(),
        Type::Null => "Null".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::Duration => "Duration".to_string(),
        Type::Str => "Str".to_string(),
        Type::Bytes => "Bytes".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Regex => "Regex".to_string(),
        Type::Path => "Path".to_string(),
        Type::List(inner) => format!("List[{}]", render_type(inner)),
        Type::Map(inner) => format!("Map[{}]", render_type(inner)),
        Type::Stream(inner) => format!("Stream[{}]", render_type(inner)),
        Type::Record(fields) if fields.is_empty() => "Record".to_string(),
        Type::Record(fields) => {
            let fields = fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", render_type(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{fields}}}")
        }
        Type::Module(_) => "Module".to_string(),
        Type::Result(ok, err) => format!("Result[{}, {}]", render_type(ok), render_type(err)),
        Type::Status => "Status".to_string(),
        Type::EnvPathList => "EnvPathList".to_string(),
        Type::Error => "Error".to_string(),
        Type::ErrorFamily(name) => name.to_string(),
        Type::ErrorVariant { family, variant } => format!("{family}.{variant}"),
        Type::ErrorFacet(name) => name.to_string(),
        Type::ProcessError => "ProcessError".to_string(),
        Type::Pure => "Pure".to_string(),
        Type::Proc => "Proc".to_string(),
        Type::Command => "Command".to_string(),
        Type::ProcessHandle => "ProcessHandle".to_string(),
        Type::Unit => "Unit".to_string(),
        Type::Tag(name) => name.to_string(),
        Type::Optional(inner) => format!("{}?", render_type(inner)),
    }
}

fn module_api_id(module: &str, function: &str, index: usize) -> String {
    format!("module.{module}.{function}.{index}")
}

fn method_api_id(receiver: MethodReceiver, method: &str, index: usize) -> String {
    format!("method.{}.{}.{}", receiver_name(receiver), method, index)
}

fn module_summary(module: &str) -> &'static str {
    match module {
        "applet" => "Internal primitives for shipped core applet scripts.",
        "archive" => "Archive creation, extraction, listing, compression, and decompression.",
        "bytes" => "Byte inspection, encoding, decoding, copying, and hashing helpers.",
        "cli" => "Script command-line parsing into typed option records.",
        "cpu" => "CPU capability queries.",
        "diff" => "Unified diff generation.",
        "dns" => "DNS lookup and name resolution helpers.",
        "elf" => "ELF file-format inspection and dynamic dependency metadata.",
        "env" => "Environment variable and PATH manipulation.",
        "fs" => {
            "Filesystem reads, writes, metadata, links, permissions, locking, and installation."
        }
        "group" => "Unix group lookup records.",
        "hash" => "Digest calculation and checksum verification.",
        "ini" => "INI decoding, encoding, and file helpers.",
        "io" => "Script stdin and stdout helpers.",
        "json" => "JSON encoding, decoding, files, and streams.",
        "linux" => "Linux-specific boot, mount, device, and shutdown operations.",
        "list" => "List collection helpers.",
        "map" => "Map collection helpers.",
        "set" => "String-key set helpers backed by Map[Bool].",
        "mime" => "MIME type lookup and media-type parsing helpers.",
        "module" => "User module loading helpers.",
        "net" => "HTTP request, transfer, and connection-pool helpers.",
        "patch" => "Rooted patch application.",
        "path" => "Path normalization and resolution.",
        "process" => "Process discovery, command construction, execution, spawning, and signals.",
        "record" => "Record inspection helpers.",
        "regex" => "Regex compilation, matching, captures, and replacement.",
        "shlex" => "POSIX-like shell word rendering helpers.",
        "system" => "Host system identity records.",
        "test" => "Native XSH test assertions, temp resources, and host-effect mocks.",
        "text" => "Text splitting, joining, replacement, counting, and character transforms.",
        "time" => "Clock, sleep, command measurement, and Jiff strtime formatting.",
        "tui" => "Terminal styling, control sequences, and width-aware text padding.",
        "unix" => "Unix process-group, PID 1, hostname, uptime, exec, and reaping helpers.",
        "user" => "Unix user lookup records.",
        "utils" => "Process-scoped utility helpers.",
        _ => "",
    }
}

fn function_summary(module: &str, function: &str) -> String {
    match (module, function) {
        ("net", "request_many") => return "Returns ordered request results.".to_string(),
        ("net", "download_many") => return "Returns ordered download results.".to_string(),
        _ => {}
    }
    format!(
        "{} `{}` operation.",
        module_summary(module).trim_end_matches('.'),
        function
    )
}

fn method_summary(receiver: MethodReceiver, method: &str) -> String {
    format!(
        "`{}` method for `{}` values.",
        method,
        receiver_name(receiver)
    )
}

fn param_doc(name: &str, defaulted: bool) -> String {
    if defaulted {
        format!("Optional `{name}` argument; omitted calls use the runtime default.")
    } else {
        format!("Required `{name}` argument.")
    }
}

fn return_doc(ty: &Type) -> String {
    match ty {
        Type::Result(ok, err) => format!(
            "Returns `{}` or `{}` failure data.",
            render_type(ok),
            render_type(err)
        ),
        other => format!("Returns `{}`.", render_type(other)),
    }
}

fn method_return_doc(return_ty: &MethodReturn) -> String {
    match return_ty {
        MethodReturn::Type(ty) => return_doc(ty),
        MethodReturn::Receiver => "Returns the receiver result type.".to_string(),
    }
}

fn receiver_name(receiver: MethodReceiver) -> &'static str {
    match receiver {
        MethodReceiver::PathConstructor => "PathConstructor",
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

fn stream_stages() -> &'static [&'static str] {
    &[
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
    ]
}

fn run_form_label(row: &xsh_registry::reference::RunFormReference) -> String {
    match row.context {
        Some(context) => format!("`{}` in {context}", row.form),
        None => code_span(row.form),
    }
}

fn effect_covers_markdown(covers: &[&str]) -> String {
    covers
        .iter()
        .map(|cover| effect_cover_markdown(cover))
        .collect::<Vec<_>>()
        .join(", ")
}

fn effect_cover_markdown(cover: &str) -> String {
    if let Some(inner) = cover.strip_prefix("superset of ") {
        return format!("superset of {}", code_span(inner));
    }
    if let Some(inner) = cover.strip_prefix("effectful ") {
        return format!("effectful {}", code_span(inner));
    }
    if let Some(rest) = cover.strip_prefix("? ") {
        return format!("{} {rest}", code_span("?"));
    }
    if cover == "delayed retry blocks" {
        return cover.to_string();
    }
    code_span(cover)
}

fn code_words(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            if matches!(word, "ProcessError" | "Err(ProcessError)" | "Ok(record)") {
                code_span(word)
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn code_span(value: &str) -> String {
    format!("`{value}`")
}

fn cli_forms() -> &'static [&'static str] {
    &[
        "xsh SCRIPT [ARGS...]",
        "xsh -- SCRIPT ARGS...",
        "xshi",
        "xsht check [--strict] [--summary] [--annotate] [PATH...]",
        "xsht fmt [--check] [FILE...]",
        "xsht lint [--fix] [--runless] [FILE...]",
        "xsht ast SCRIPT",
        "xsht trace [--raw] [--trace-format text|jsonl|flamegraph] [--trace-file FILE] [--syscalls] [--trace-top-syscalls N] SCRIPT [ARGS...]",
        "xsht test [--cov] [OPTIONS] [FILTER]",
        "xsht docs build",
        "xsht docs check",
    ]
}

fn trace_events() -> &'static [&'static str] {
    &[
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
    ]
}

fn core_language_items() -> &'static [&'static str] {
    &[
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
    ]
}

#[cfg(test)]
mod tests {
    use crate::xsht::docs::{
        api_spec, record_schemas, reference_markdown, stdlib_markdown, validate_api_docs,
        validate_record_docs,
    };

    #[test]
    fn generated_stdlib_has_all_standard_modules_and_records() {
        let stdlib = stdlib_markdown().expect("stdlib markdown");
        for module in api_spec().module_names() {
            assert!(stdlib.contains(&format!("### `{module}`")), "{module}");
        }
        for record in record_schemas().keys() {
            assert!(stdlib.contains(&format!("### `{record}`")), "{record}");
        }
    }

    #[test]
    fn generated_reference_excludes_stdlib_api_surface() {
        let reference = reference_markdown().expect("reference markdown");
        assert!(!reference.contains("module: fs"));
        assert!(!reference.contains("receiver: Str"));
        assert!(!reference.contains("record: FsEntry"));
        assert!(reference.contains("stage: map"));
        assert!(reference.contains("stage: collect"));
        assert!(reference.contains("## Run Forms"));
        assert!(reference.contains("run: `run.capture --text`"));
        assert!(reference.contains("## Effects"));
        assert!(reference.contains("effect: `process`"));
        assert!(reference.contains("cli: xsht docs build"));
        assert!(reference.contains("trace: module.call"));
    }

    #[test]
    fn api_docs_metadata_covers_current_registry() {
        let mut errors = Vec::new();
        validate_api_docs(&mut errors);
        assert_eq!(errors, Vec::<String>::new());
    }

    #[test]
    fn record_docs_are_tied_to_checker_visible_schemas() {
        let mut errors = Vec::new();
        validate_record_docs(&mut errors);
        assert_eq!(errors, Vec::<String>::new());
    }

}
