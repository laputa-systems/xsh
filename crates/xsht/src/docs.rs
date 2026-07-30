#![allow(clippy::single_call_fn, dead_code)]

use crate::xsht::format::Formatter;
#[cfg(feature = "docs-html")]
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use xsh::diagnostic::DiagnosticRenderer;
use xsh::modules::api_spec;
use xsh::modules::signature::{
    MethodReceiver, MethodReturn, ModuleFnSig, ModuleSig, NamedMethodSigs,
};
use xsh::runtime::eval::Evaluator;
use xsh::sema::check::Checker;
use xsh::sema::records::record_schemas;
use xsh::sema::types::Type;
use xsh::source::SourceMap;
#[cfg(feature = "docs-html")]
use xsh::symbol::Name;
#[cfg(feature = "docs-html")]
use xsh::syntax::lexer::Lexer;
use xsh::syntax::parser::Parser as XshParser;
#[cfg(feature = "docs-html")]
use xsh::syntax::token::{Keyword, TokenTag};
use xsh::trace::{TraceNormalizer, TraceSummaryRenderer, TraceTextRenderer};
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
pub struct ExampleCatalog {
    pub examples: Vec<ExampleCase>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExampleCase {
    pub include_id: String,
    pub path: String,
    pub chapter: String,
    pub args: Vec<String>,
    pub expected_status: i32,
    pub stdout: OutputPolicy,
    pub stderr: OutputPolicy,
    pub trace: bool,
    /// True for examples that exercise the `net` feature (HTTP/DNS); test
    /// runners skip these when xsh is built without `net`.
    pub requires_net: bool,
    /// True for examples that should be skipped by test runners (e.g.
    /// platform-dependent or flaky examples).
    pub skip: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputPolicy {
    Exact(String),
    Contains(String),
    Empty,
    Any,
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

#[cfg(feature = "docs-html")]
struct ReferenceJsonData {
    modules: Vec<ModuleJsonData>,
    methods: Vec<MethodGroupJsonData>,
    records: Vec<RecordJsonData>,
    stream_stages: Vec<String>,
    run_forms: Vec<String>,
    effects: Vec<String>,
    cli_forms: Vec<String>,
    trace_events: Vec<String>,
    language_items: Vec<String>,
}

#[cfg(feature = "docs-html")]
struct ModuleJsonData {
    name: String,
    summary: String,
    functions: Vec<FunctionJsonData>,
}

#[cfg(feature = "docs-html")]
struct FunctionJsonData {
    id: String,
    name: String,
    overload_index: usize,
    params: Vec<ParamJsonData>,
    return_type: String,
    is_pure: bool,
    command: bool,
    summary: String,
    returns: String,
}

#[cfg(feature = "docs-html")]
struct ParamJsonData {
    name: String,
    ty: String,
    defaulted: bool,
    doc: String,
}

#[cfg(feature = "docs-html")]
struct MethodGroupJsonData {
    receiver: String,
    methods: Vec<MethodJsonData>,
}

#[cfg(feature = "docs-html")]
struct MethodJsonData {
    id: String,
    name: String,
    overload_index: usize,
    params: Vec<ParamJsonData>,
    return_type: String,
    is_pure: bool,
    summary: String,
    returns: String,
}

#[cfg(feature = "docs-html")]
struct RecordJsonData {
    name: String,
    fields: Vec<FieldJsonData>,
}

#[cfg(feature = "docs-html")]
struct FieldJsonData {
    name: String,
    ty: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ChapterDoc {
    template: PathBuf,
    output_path: PathBuf,
    output_name: String,
    title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IncludeDirective {
    kind: String,
    id: String,
}

#[cfg(feature = "docs-html")]
#[derive(Clone, Debug, Eq, PartialEq)]
struct MarkdownPage {
    markdown_path: PathBuf,
    html_path: PathBuf,
    title: String,
    contents: String,
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

pub fn load_example_catalog(root: impl AsRef<Path>) -> Result<ExampleCatalog, String> {
    let path = root.as_ref().join("examples/catalog.json");
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
    parse_example_catalog(&text)
        .map_err(|err| format!("failed to parse '{}': {err}", path.display()))
}

fn parse_example_catalog(text: &str) -> Result<ExampleCatalog, String> {
    let value = xsh::modules::json::parse_raw_json(text)?;
    let examples = json_array_field(&value, "examples")?
        .iter()
        .map(parse_example_case)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ExampleCatalog { examples })
}

fn parse_example_case(value: &miniserde::json::Value) -> Result<ExampleCase, String> {
    let expected_status = json_u64_field(value, "expected_status")?;
    let expected_status = i32::try_from(expected_status)
        .map_err(|_| "example expected_status does not fit i32".to_string())?;
    Ok(ExampleCase {
        include_id: json_string_field(value, "include_id")?.to_string(),
        path: json_string_field(value, "path")?.to_string(),
        chapter: json_string_field(value, "chapter")?.to_string(),
        args: json_optional_string_list_field(value, "args")?.unwrap_or_default(),
        expected_status,
        stdout: parse_output_policy(json_field(value, "stdout")?)?,
        stderr: parse_output_policy(json_field(value, "stderr")?)?,
        trace: json_optional_bool_field(value, "trace")?.unwrap_or(false),
        requires_net: json_optional_bool_field(value, "requires_net")?.unwrap_or(false),
        skip: json_optional_bool_field(value, "skip")?.unwrap_or(false),
    })
}

fn parse_output_policy(value: &miniserde::json::Value) -> Result<OutputPolicy, String> {
    match json_string_field(value, "kind")? {
        "exact" => Ok(OutputPolicy::Exact(
            json_string_field(value, "value")?.to_string(),
        )),
        "contains" => Ok(OutputPolicy::Contains(
            json_string_field(value, "value")?.to_string(),
        )),
        "empty" => Ok(OutputPolicy::Empty),
        "any" => Ok(OutputPolicy::Any),
        kind => Err(format!("unknown output policy kind '{kind}'")),
    }
}

fn json_field<'a>(
    value: &'a miniserde::json::Value,
    key: &str,
) -> Result<&'a miniserde::json::Value, String> {
    xsh::modules::json::raw_json_get(value, key).ok_or_else(|| format!("missing field '{key}'"))
}

fn json_string_field<'a>(value: &'a miniserde::json::Value, key: &str) -> Result<&'a str, String> {
    let value = json_field(value, key)?;
    xsh::modules::json::raw_json_as_str(value)
        .ok_or_else(|| format!("field '{key}' must be a string"))
}

fn json_u64_field(value: &miniserde::json::Value, key: &str) -> Result<u64, String> {
    let value = json_field(value, key)?;
    xsh::modules::json::raw_json_as_u64(value)
        .ok_or_else(|| format!("field '{key}' must be a non-negative integer"))
}

fn json_array_field<'a>(
    value: &'a miniserde::json::Value,
    key: &str,
) -> Result<&'a miniserde::json::Array, String> {
    match json_field(value, key)? {
        miniserde::json::Value::Array(items) => Ok(items),
        _ => Err(format!("field '{key}' must be an array")),
    }
}

fn json_optional_bool_field(
    value: &miniserde::json::Value,
    key: &str,
) -> Result<Option<bool>, String> {
    let Some(value) = xsh::modules::json::raw_json_get(value, key) else {
        return Ok(None);
    };
    xsh::modules::json::raw_json_as_bool(value)
        .map(Some)
        .ok_or_else(|| format!("field '{key}' must be a boolean"))
}

fn json_optional_string_list_field(
    value: &miniserde::json::Value,
    key: &str,
) -> Result<Option<Vec<String>>, String> {
    let Some(miniserde::json::Value::Array(items)) = xsh::modules::json::raw_json_get(value, key)
    else {
        return Ok(None);
    };
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let Some(value) = xsh::modules::json::raw_json_as_str(item) else {
            return Err(format!("field '{key}' must contain only strings"));
        };
        values.push(value.to_string());
    }
    Ok(Some(values))
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

fn generate(root: &Path) -> Result<DocsReport, String> {
    let examples = load_example_catalog(root)?;
    let docs_src = root.join("docs-src");
    let mut errors = Vec::new();

    validate_examples(root, &examples, &mut errors);
    validate_api_docs(&mut errors);
    validate_record_docs(&mut errors);

    let include_ids = include_ids(&examples, &mut errors);
    let mut generated = Vec::new();
    #[cfg(feature = "docs-html")]
    let mut markdown_pages = Vec::new();
    let chapters = chapter_docs(&docs_src, &mut errors);
    validate_chapter_usage(&chapters, &examples, &mut errors);
    for chapter in &chapters {
        match render_template(root, &chapter.template, &examples, &include_ids) {
            Ok(contents) => {
                #[cfg(feature = "docs-html")]
                markdown_pages.push(MarkdownPage {
                    markdown_path: chapter.output_path.clone(),
                    html_path: html_path_for_markdown(&chapter.output_path)?,
                    title: chapter.title.clone(),
                    contents: contents.clone(),
                });
                generated.push(GeneratedFile {
                    path: chapter.output_path.clone(),
                    contents,
                });
            }
            Err(err) => errors.push(err),
        }
    }

    let guide = guide_markdown(&chapters);
    #[cfg(feature = "docs-html")]
    markdown_pages.push(MarkdownPage {
        markdown_path: PathBuf::from("docs/XSH-GUIDE.md"),
        html_path: PathBuf::from("docs-html/XSH-GUIDE.html"),
        title: "XSH Guide".to_string(),
        contents: guide.clone(),
    });
    generated.push(GeneratedFile {
        path: PathBuf::from("docs/XSH-GUIDE.md"),
        contents: guide,
    });
    let stdlib = stdlib_markdown()?;
    #[cfg(feature = "docs-html")]
    markdown_pages.push(MarkdownPage {
        markdown_path: PathBuf::from("docs/STDLIB.md"),
        html_path: PathBuf::from("docs-html/STDLIB.html"),
        title: "XSH Standard Library".to_string(),
        contents: stdlib.clone(),
    });
    generated.push(GeneratedFile {
        path: PathBuf::from("docs/STDLIB.md"),
        contents: stdlib,
    });
    let reference = reference_markdown()?;
    #[cfg(feature = "docs-html")]
    markdown_pages.push(MarkdownPage {
        markdown_path: PathBuf::from("docs/REFERENCE.md"),
        html_path: PathBuf::from("docs-html/REFERENCE.html"),
        title: "XSH Reference".to_string(),
        contents: reference.clone(),
    });
    generated.push(GeneratedFile {
        path: PathBuf::from("docs/REFERENCE.md"),
        contents: reference,
    });
    #[cfg(feature = "docs-html")]
    match fs::read_to_string(root.join("docs/IDIOMS.md")) {
        Ok(contents) => markdown_pages.push(MarkdownPage {
            markdown_path: PathBuf::from("docs/IDIOMS.md"),
            html_path: PathBuf::from("docs-html/IDIOMS.html"),
            title: "XSH Idioms".to_string(),
            contents,
        }),
        Err(err) => errors.push(format!("failed to read docs/IDIOMS.md: {err}")),
    }
    #[cfg(feature = "docs-html")]
    match reference_json_data() {
        Ok(json) => generated.push(GeneratedFile {
            path: PathBuf::from("docs-html/reference/data.json"),
            contents: json,
        }),
        Err(err) => errors.push(err),
    }

    #[cfg(feature = "docs-html")]
    match stdlib_html_files() {
        Ok(files) => generated.extend(files),
        Err(err) => errors.push(err),
    }

    #[cfg(feature = "docs-html")]
    generated.extend(html_files(&markdown_pages));

    if errors.is_empty() {
        insertion_sort_by(&mut generated, |left, right| left.path.cmp(&right.path));
        Ok(DocsReport { generated })
    } else {
        Err(errors.join("\n"))
    }
}

fn chapter_docs(docs_src: &Path, errors: &mut Vec<String>) -> Vec<ChapterDoc> {
    let mut chapters = Vec::new();
    let entries = match fs::read_dir(docs_src) {
        Ok(entries) => entries,
        Err(err) => {
            errors.push(format!("failed to read '{}': {err}", docs_src.display()));
            return chapters;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("CHAPTER-") && name.ends_with(".md.in"))
                {
                    match chapter_doc(path) {
                        Ok(chapter) => chapters.push(chapter),
                        Err(err) => errors.push(err),
                    }
                }
            }
            Err(err) => errors.push(format!("failed to read docs-src entry: {err}")),
        }
    }
    insertion_sort_by(&mut chapters, |left, right| {
        left.output_path.cmp(&right.output_path)
    });
    chapters
}

fn chapter_doc(template: PathBuf) -> Result<ChapterDoc, String> {
    let file_name = template
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid template path '{}'", template.display()))?;
    let output_name = file_name.trim_end_matches(".in").to_string();
    let text = fs::read_to_string(&template)
        .map_err(|err| format!("failed to read '{}': {err}", template.display()))?;
    let title = text
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .ok_or_else(|| format!("chapter template '{}' is missing an H1", template.display()))?
        .to_string();
    Ok(ChapterDoc {
        template,
        output_path: PathBuf::from("docs").join(&output_name),
        output_name,
        title,
    })
}

fn render_template(
    root: &Path,
    template: &Path,
    examples: &ExampleCatalog,
    include_ids: &BTreeSet<String>,
) -> Result<String, String> {
    let text = fs::read_to_string(template)
        .map_err(|err| format!("failed to read '{}': {err}", template.display()))?;
    let mut output = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(format!(
                "unclosed include directive in '{}'",
                template.display()
            ));
        };
        let directive = after[..end].trim();
        output.push_str(&render_directive(
            root,
            template,
            directive,
            examples,
            include_ids,
        )?);
        rest = &after[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

fn validate_chapter_usage(
    chapters: &[ChapterDoc],
    examples: &ExampleCatalog,
    errors: &mut Vec<String>,
) {
    let known_chapters = chapters
        .iter()
        .map(|chapter| chapter.output_name.as_str())
        .collect::<BTreeSet<_>>();
    let mut example_chapters = BTreeMap::new();
    for example in &examples.examples {
        example_chapters.insert(example.include_id.as_str(), example.chapter.as_str());
    }
    let mut used_examples = BTreeSet::new();

    for chapter in chapters {
        let directives = match include_directives(&chapter.template) {
            Ok(directives) => directives,
            Err(err) => {
                errors.push(err);
                continue;
            }
        };
        for directive in directives {
            if directive.kind.as_str() == "include" {
                used_examples.insert(directive.id.clone());
                match example_chapters.get(directive.id.as_str()) {
                    Some(expected) if *expected == chapter.output_name => {}
                    Some(expected) => errors.push(format!(
                        "include '{}' appears in '{}' but catalog says '{}'",
                        directive.id, chapter.output_name, expected
                    )),
                    None => {}
                }
            }
        }
    }

    for example in &examples.examples {
        if !known_chapters.contains(example.chapter.as_str()) {
            errors.push(format!(
                "example '{}' points at unknown chapter '{}'",
                example.include_id, example.chapter
            ));
        }
        if !used_examples.contains(&example.include_id) {
            errors.push(format!(
                "example '{}' is cataloged but not included in docs-src",
                example.include_id
            ));
        }
    }
}

fn include_directives(template: &Path) -> Result<Vec<IncludeDirective>, String> {
    let text = fs::read_to_string(template)
        .map_err(|err| format!("failed to read '{}': {err}", template.display()))?;
    let mut directives = Vec::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(format!(
                "unclosed include directive in '{}'",
                template.display()
            ));
        };
        let directive = after[..end].trim();
        let Some((kind, id)) = directive.split_once(':') else {
            return Err(format!(
                "invalid include directive '{{{{{directive}}}}}' in '{}'",
                template.display()
            ));
        };
        directives.push(IncludeDirective {
            kind: kind.trim().to_string(),
            id: id.trim().to_string(),
        });
        rest = &after[end + 2..];
    }
    Ok(directives)
}

fn guide_markdown(chapters: &[ChapterDoc]) -> String {
    let mut output = String::new();
    output.push_str("# XSH Guide\n\n");
    output.push_str("This page is generated from `docs-src/CHAPTER-*.md.in`.\n\n");
    output.push_str("Markdown under `docs/` is the primary generated artifact for agents, code review, and repository navigation. Human readers should start with `docs-html/index.html`, which is generated from the same markdown and checked for drift by `xsht docs check`.\n\n");
    output.push_str("`docs/SPEC.md` is the normative language contract. `docs/SPEC-OS.md` details OS-facing runtime behavior such as signals, process groups, cancellation, and signal hooks. `docs/STDLIB.md` is the generated standard-library manual. `STDLIB-PROPOSALS.md` records standard-library design criteria, explicit non-goals, and open proposals. `docs/REFERENCE.md` is the generated non-stdlib language and tooling reference. The tutorial chapters are generated from `docs-src/` and include only cataloged examples from `examples/`.\n\n");
    output.push_str("## Reader Paths\n\n");
    output.push_str("- New to XSH: read chapters 1 through 8 in order.\n");
    output.push_str("- Shell user evaluating the value: read chapters 1, 2, 4, 5, 8, and 15.\n");
    output.push_str("- Building a maintainable tool: read chapters 3, 4, 10, 11, 12, and 13.\n");
    output.push_str("- Looking up exact behavior: use `docs/STDLIB.md`, `docs/REFERENCE.md`, and `docs/SPEC.md`.\n\n");
    output.push_str("## Chapters\n\n");
    for chapter in chapters {
        output.push_str("- `");
        output.push_str(&chapter.output_path.to_string_lossy());
        output.push_str("`: ");
        output.push_str(&chapter.title);
        output.push('\n');
    }
    output.push_str("- `docs/STDLIB.md`: generated standard-library manual.\n");
    output.push_str("- `STDLIB-PROPOSALS.md`: standard-library design and open proposals.\n");
    output.push_str("- `docs/DOCS-STYLE.md`: tutorial and reference documentation style guide.\n");
    output
        .push_str("- `docs/REFERENCE.md`: generated non-stdlib language and tooling reference.\n");
    output.push_str("- `docs/AGENT-ROUTING.md`: coding-agent task routing and owner map.\n");
    output.push_str("- `docs/TEST-MAP.md`: focused verification commands by change type.\n");
    output.push_str("- `docs/GENERATED-DOCS.md`: generated documentation source map.\n");
    output.push_str("- `docs/CHANGE-RECIPES.md`: common implementation checklists.\n");
    output.push_str(
        "- `docs/JSON.md`: guidance for JSON boundary patterns and dynamic JSON tools.\n",
    );
    output
        .push_str("- `docs/STREAMS.md`: structured stream implementation notes and invariants.\n");
    output.push_str("- `docs/COVERAGE.md`: practical coverage plan and harness notes.\n");
    output.push_str(
        "- `docs/FRONTEND.md`: compact frontend, indexed runtime, symbol identity, ownership, and verification contract.\n",
    );
    output.push_str(
        "- `../FRONTEND-FOLLOWUPS.md`: evidence-based frontend performance and memory follow-ups.\n",
    );
    output.push_str(
        "- `docs/BENCHMARKING.md`: user-facing benchmarks, PGO, baselines, and syscall diagnostics.\n",
    );
    output.push('\n');
    output.push_str("## Maintenance\n\n");
    output.push_str(
        "Edit `docs-src/`, `examples/catalog.json`, and the implementation metadata. Use the formatter-free docs gate in `docs/TEST-MAP.md`.\n",
    );
    output
}

#[cfg(feature = "docs-html")]
fn html_path_for_markdown(markdown_path: &Path) -> Result<PathBuf, String> {
    let file_name = markdown_path
        .file_name()
        .ok_or_else(|| format!("invalid markdown output path '{}'", markdown_path.display()))?;
    Ok(PathBuf::from("docs-html")
        .join(file_name)
        .with_extension("html"))
}

#[cfg(feature = "docs-html")]
fn reference_json_data() -> Result<String, String> {
    let spec = api_spec();
    let mut module_entries = spec.module_entries().collect::<Vec<_>>();
    insertion_sort_by(&mut module_entries, |a, b| a.0.cmp(b.0));

    let mut modules_data: Vec<ModuleJsonData> = Vec::new();
    for (module_name, module) in module_entries {
        let mut fn_entries = module.functions.iter().collect::<Vec<_>>();
        insertion_sort_by(&mut fn_entries, |a, b| a.name.cmp(b.name));

        let mut functions_data: Vec<FunctionJsonData> = Vec::new();
        for function in fn_entries {
            for (index, sig) in function.overloads.iter().enumerate() {
                let params = sig
                    .params
                    .iter()
                    .map(|p| ParamJsonData {
                        name: p.name.to_string(),
                        ty: render_type(&p.ty),
                        defaulted: p.defaulted,
                        doc: param_doc(p.name, p.defaulted),
                    })
                    .collect();
                functions_data.push(FunctionJsonData {
                    id: module_api_id(module_name, function.name, index),
                    name: function.name.to_string(),
                    overload_index: index,
                    params,
                    return_type: render_type(&sig.return_ty),
                    is_pure: sig.pure,
                    command: sig.command,
                    summary: function_summary(module_name, function.name),
                    returns: return_doc(&sig.return_ty),
                });
            }
        }
        modules_data.push(ModuleJsonData {
            name: module_name.to_string(),
            summary: module_summary(module_name).to_string(),
            functions: functions_data,
        });
    }

    let mut receiver_entries = spec.method_entries().collect::<Vec<_>>();
    insertion_sort_by(&mut receiver_entries, |(r, _), (right_key, _)| {
        receiver_name(*r).cmp(receiver_name(*right_key))
    });
    let mut methods_data: Vec<MethodGroupJsonData> = Vec::new();
    for (receiver, methods) in receiver_entries {
        let mut method_entries = methods.iter().collect::<Vec<_>>();
        insertion_sort_by(&mut method_entries, |a, b| a.name.cmp(b.name));
        let mut methods_list: Vec<MethodJsonData> = Vec::new();
        for entry in method_entries {
            for (index, method) in entry.overloads.iter().enumerate() {
                let params = method
                    .sig
                    .params
                    .iter()
                    .map(|p| ParamJsonData {
                        name: p.name.to_string(),
                        ty: render_type(&p.ty),
                        defaulted: p.defaulted,
                        doc: param_doc(p.name, p.defaulted),
                    })
                    .collect();
                let return_type = match &method.return_ty {
                    MethodReturn::Type(ty) => render_type(ty),
                    MethodReturn::Receiver => "Self".to_string(),
                };
                methods_list.push(MethodJsonData {
                    id: method_api_id(receiver, entry.name, index),
                    name: entry.name.to_string(),
                    overload_index: index,
                    params,
                    return_type,
                    is_pure: method.sig.pure,
                    summary: method_summary(receiver, entry.name),
                    returns: method_return_doc(&method.return_ty),
                });
            }
        }
        methods_data.push(MethodGroupJsonData {
            receiver: receiver_name(receiver).to_string(),
            methods: methods_list,
        });
    }

    let records_data: Vec<RecordJsonData> = record_schemas()
        .into_iter()
        .map(|(name, ty)| {
            let fields = if let Type::Record(fields) = ty {
                fields
                    .into_iter()
                    .map(|(fname, fty)| FieldJsonData {
                        name: fname.to_string(),
                        ty: render_type(&fty),
                    })
                    .collect()
            } else {
                Vec::new()
            };
            RecordJsonData {
                name: name.to_string(),
                fields,
            }
        })
        .collect();

    let data = ReferenceJsonData {
        modules: modules_data,
        methods: methods_data,
        records: records_data,
        stream_stages: stream_stages().iter().map(|s| s.to_string()).collect(),
        run_forms: RUN_FORM_REFERENCES
            .iter()
            .map(|row| format!("{} -> {}", run_form_label(row), code_span(row.returns)))
            .collect(),
        effects: EFFECT_REFERENCES
            .iter()
            .map(|row| {
                format!(
                    "{} -> {}",
                    code_span(row.name),
                    effect_covers_markdown(row.covers)
                )
            })
            .collect(),
        cli_forms: cli_forms().iter().map(|s| s.to_string()).collect(),
        trace_events: trace_events().iter().map(|s| s.to_string()).collect(),
        language_items: core_language_items()
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };

    Ok(xsh::modules::json::pretty_raw_json(&reference_json_value(
        data,
    )))
}

#[cfg(feature = "docs-html")]
fn reference_json_value(data: ReferenceJsonData) -> miniserde::json::Value {
    xsh::modules::json::raw_json_object([
        (
            "modules".to_string(),
            xsh::modules::json::raw_json_array(data.modules.into_iter().map(module_json_value)),
        ),
        (
            "methods".to_string(),
            xsh::modules::json::raw_json_array(
                data.methods.into_iter().map(method_group_json_value),
            ),
        ),
        (
            "records".to_string(),
            xsh::modules::json::raw_json_array(data.records.into_iter().map(record_json_value)),
        ),
        (
            "stream_stages".to_string(),
            string_array_json_value(data.stream_stages),
        ),
        (
            "run_forms".to_string(),
            string_array_json_value(data.run_forms),
        ),
        ("effects".to_string(), string_array_json_value(data.effects)),
        (
            "cli_forms".to_string(),
            string_array_json_value(data.cli_forms),
        ),
        (
            "trace_events".to_string(),
            string_array_json_value(data.trace_events),
        ),
        (
            "language_items".to_string(),
            string_array_json_value(data.language_items),
        ),
    ])
}

#[cfg(feature = "docs-html")]
fn module_json_value(data: ModuleJsonData) -> miniserde::json::Value {
    xsh::modules::json::raw_json_object([
        (
            "name".to_string(),
            xsh::modules::json::raw_json_string(data.name),
        ),
        (
            "summary".to_string(),
            xsh::modules::json::raw_json_string(data.summary),
        ),
        (
            "functions".to_string(),
            xsh::modules::json::raw_json_array(data.functions.into_iter().map(function_json_value)),
        ),
    ])
}

#[cfg(feature = "docs-html")]
fn function_json_value(data: FunctionJsonData) -> miniserde::json::Value {
    xsh::modules::json::raw_json_object([
        (
            "id".to_string(),
            xsh::modules::json::raw_json_string(data.id),
        ),
        (
            "name".to_string(),
            xsh::modules::json::raw_json_string(data.name),
        ),
        (
            "overload_index".to_string(),
            xsh::modules::json::raw_json_usize(data.overload_index),
        ),
        (
            "params".to_string(),
            xsh::modules::json::raw_json_array(data.params.into_iter().map(param_json_value)),
        ),
        (
            "return_type".to_string(),
            xsh::modules::json::raw_json_string(data.return_type),
        ),
        (
            "pure".to_string(),
            xsh::modules::json::raw_json_bool(data.is_pure),
        ),
        (
            "command".to_string(),
            xsh::modules::json::raw_json_bool(data.command),
        ),
        (
            "summary".to_string(),
            xsh::modules::json::raw_json_string(data.summary),
        ),
        (
            "returns".to_string(),
            xsh::modules::json::raw_json_string(data.returns),
        ),
    ])
}

#[cfg(feature = "docs-html")]
fn param_json_value(data: ParamJsonData) -> miniserde::json::Value {
    xsh::modules::json::raw_json_object([
        (
            "name".to_string(),
            xsh::modules::json::raw_json_string(data.name),
        ),
        (
            "type".to_string(),
            xsh::modules::json::raw_json_string(data.ty),
        ),
        (
            "defaulted".to_string(),
            xsh::modules::json::raw_json_bool(data.defaulted),
        ),
        (
            "doc".to_string(),
            xsh::modules::json::raw_json_string(data.doc),
        ),
    ])
}

#[cfg(feature = "docs-html")]
fn method_group_json_value(data: MethodGroupJsonData) -> miniserde::json::Value {
    xsh::modules::json::raw_json_object([
        (
            "receiver".to_string(),
            xsh::modules::json::raw_json_string(data.receiver),
        ),
        (
            "methods".to_string(),
            xsh::modules::json::raw_json_array(data.methods.into_iter().map(method_json_value)),
        ),
    ])
}

#[cfg(feature = "docs-html")]
fn method_json_value(data: MethodJsonData) -> miniserde::json::Value {
    xsh::modules::json::raw_json_object([
        (
            "id".to_string(),
            xsh::modules::json::raw_json_string(data.id),
        ),
        (
            "name".to_string(),
            xsh::modules::json::raw_json_string(data.name),
        ),
        (
            "overload_index".to_string(),
            xsh::modules::json::raw_json_usize(data.overload_index),
        ),
        (
            "params".to_string(),
            xsh::modules::json::raw_json_array(data.params.into_iter().map(param_json_value)),
        ),
        (
            "return_type".to_string(),
            xsh::modules::json::raw_json_string(data.return_type),
        ),
        (
            "pure".to_string(),
            xsh::modules::json::raw_json_bool(data.is_pure),
        ),
        (
            "summary".to_string(),
            xsh::modules::json::raw_json_string(data.summary),
        ),
        (
            "returns".to_string(),
            xsh::modules::json::raw_json_string(data.returns),
        ),
    ])
}

#[cfg(feature = "docs-html")]
fn record_json_value(data: RecordJsonData) -> miniserde::json::Value {
    xsh::modules::json::raw_json_object([
        (
            "name".to_string(),
            xsh::modules::json::raw_json_string(data.name),
        ),
        (
            "fields".to_string(),
            xsh::modules::json::raw_json_array(data.fields.into_iter().map(field_json_value)),
        ),
    ])
}

#[cfg(feature = "docs-html")]
fn field_json_value(data: FieldJsonData) -> miniserde::json::Value {
    xsh::modules::json::raw_json_object([
        (
            "name".to_string(),
            xsh::modules::json::raw_json_string(data.name),
        ),
        (
            "type".to_string(),
            xsh::modules::json::raw_json_string(data.ty),
        ),
    ])
}

#[cfg(feature = "docs-html")]
fn string_array_json_value(items: Vec<String>) -> miniserde::json::Value {
    xsh::modules::json::raw_json_array(items.into_iter().map(xsh::modules::json::raw_json_string))
}

#[cfg(feature = "docs-html")]
fn html_files(pages: &[MarkdownPage]) -> Vec<GeneratedFile> {
    let mut generated = Vec::new();
    generated.push(GeneratedFile {
        path: PathBuf::from("docs-html/index.html"),
        contents: docs_index_html(pages),
    });
    generated.push(GeneratedFile {
        path: PathBuf::from("docs-html/style.css"),
        contents: docs_stylesheet(),
    });
    for (index, page) in pages.iter().enumerate() {
        generated.push(GeneratedFile {
            path: page.html_path.clone(),
            contents: docs_page_html(page, pages, index),
        });
    }
    generated
}

#[cfg(feature = "docs-html")]
fn stdlib_html_files() -> Result<Vec<GeneratedFile>, String> {
    let mut files = Vec::new();
    files.push(GeneratedFile {
        path: PathBuf::from("docs-html/stdlib/index.html"),
        contents: stdlib_index_html(),
    });

    for (module_name, module) in sorted_modules() {
        files.push(GeneratedFile {
            path: PathBuf::from(format!("docs-html/stdlib/module/{module_name}.html")),
            contents: stdlib_module_html(module_name, module),
        });
    }

    for (receiver, methods) in sorted_method_receivers() {
        let name = receiver_name(receiver);
        files.push(GeneratedFile {
            path: PathBuf::from(format!("docs-html/stdlib/methods/{name}.html")),
            contents: stdlib_methods_html(receiver, methods),
        });
    }

    for (name, ty) in record_schemas() {
        let Type::Record(fields) = ty else {
            return Err(format!("record schema '{name}' is not a record"));
        };
        files.push(GeneratedFile {
            path: PathBuf::from(format!("docs-html/stdlib/record/{name}.html")),
            contents: stdlib_record_html(name, fields),
        });
    }

    Ok(files)
}

#[cfg(feature = "docs-html")]
fn stdlib_index_html() -> String {
    let module_cards = sorted_modules()
        .into_iter()
        .map(|(name, module)| {
            format!(
                "<a class=\"stdlib-card\" href=\"module/{name}.html\"><h3>{}</h3><p>{}</p><span>{} function(s)</span></a>",
                escaped_html_string(name),
                escaped_html_string(module_summary(name)),
                module.functions.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let method_cards = sorted_method_receivers()
        .into_iter()
        .map(|(receiver, methods)| {
            let name = receiver_name(receiver);
            format!(
                "<a class=\"stdlib-card\" href=\"methods/{name}.html\"><h3>{}</h3><p>Value methods for `{}` values.</p><span>{} method(s)</span></a>",
                escaped_html_string(name),
                escaped_html_string(name),
                methods.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let record_cards = record_schemas()
        .into_iter()
        .map(|(name, ty)| {
            let count = match ty {
                Type::Record(fields) => fields.len(),
                _ => 0,
            };
            format!(
                "<a class=\"stdlib-card\" href=\"record/{name}.html\"><h3>{}</h3><p>Standard record schema.</p><span>{count} field(s)</span></a>",
                escaped_html_string(name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    stdlib_page_html(
        "XSH Standard Library",
        "../",
        "docs/STDLIB.md",
        &format!(
            "<h1>XSH Standard Library</h1>
<p class=\"stdlib-lede\">Generated standard-library manual for modules, value methods, and standard record schemas.</p>
<h2>Modules</h2>
<div class=\"stdlib-grid\">
{module_cards}
</div>
<h2>Methods</h2>
<div class=\"stdlib-grid\">
{method_cards}
</div>
<h2>Records</h2>
<div class=\"stdlib-grid\">
{record_cards}
</div>"
        ),
    )
}

#[cfg(feature = "docs-html")]
fn stdlib_module_html(module_name: &str, module: &ModuleSig) -> String {
    let mut functions = module.functions.iter().collect::<Vec<_>>();
    insertion_sort_by(&mut functions, |left, right| left.name.cmp(right.name));
    let entries = functions
        .into_iter()
        .flat_map(|function| {
            function
                .overloads
                .iter()
                .enumerate()
                .map(move |(index, sig)| {
                    stdlib_module_entry_html(module_name, function.name, index, sig)
                })
        })
        .collect::<Vec<_>>()
        .join("\n");
    stdlib_page_html(
        module_name,
        "../../",
        &format!("docs/STDLIB.md#{module_name}"),
        &format!(
            "<p><a href=\"../index.html\">Standard Library</a> / Modules</p>
<h1>{}</h1>
<p class=\"stdlib-lede\">{}</p>
{entries}",
            escaped_html_string(module_name),
            escaped_html_string(module_summary(module_name))
        ),
    )
}

#[cfg(feature = "docs-html")]
fn stdlib_module_entry_html(
    module_name: &str,
    function: &str,
    index: usize,
    sig: &ModuleFnSig,
) -> String {
    let params = stdlib_param_rows_html(&sig.params);
    let badges = stdlib_badges(sig.pure, sig.command);
    format!(
        "<section class=\"stdlib-entry\" id=\"{}\">
<header><code>{}</code><div>{badges}</div></header>
<p>{}</p>
{params}
<p class=\"stdlib-return\">{}</p>
</section>",
        escaped_html_string(&module_api_id(module_name, function, index)),
        escaped_html_string(&module_signature(module_name, function, sig)),
        escaped_html_string(&function_summary(module_name, function)),
        escaped_html_string(&return_doc(&sig.return_ty))
    )
}

#[cfg(feature = "docs-html")]
fn stdlib_methods_html(receiver: MethodReceiver, methods: &[NamedMethodSigs]) -> String {
    let receiver_name = receiver_name(receiver);
    let mut methods = methods.iter().collect::<Vec<_>>();
    insertion_sort_by(&mut methods, |left, right| left.name.cmp(right.name));
    let entries = methods
        .into_iter()
        .flat_map(|entry| {
            entry
                .overloads
                .iter()
                .enumerate()
                .map(move |(index, method)| {
                    stdlib_method_entry_html(receiver, entry.name, index, method)
                })
        })
        .collect::<Vec<_>>()
        .join("\n");
    stdlib_page_html(
        receiver_name,
        "../../",
        &format!("docs/STDLIB.md#{receiver_name}-methods"),
        &format!(
            "<p><a href=\"../index.html\">Standard Library</a> / Methods</p>
<h1>{} Methods</h1>
{entries}",
            escaped_html_string(receiver_name)
        ),
    )
}

#[cfg(feature = "docs-html")]
fn stdlib_method_entry_html(
    receiver: MethodReceiver,
    method_name: &str,
    index: usize,
    method: &xsh::modules::signature::MethodSig,
) -> String {
    let params = stdlib_param_rows_html(&method.sig.params);
    let badges = stdlib_badges(method.sig.pure, false);
    format!(
        "<section class=\"stdlib-entry\" id=\"{}\">
<header><code>{}</code><div>{badges}</div></header>
<p>{}</p>
{params}
<p class=\"stdlib-return\">{}</p>
</section>",
        escaped_html_string(&method_api_id(receiver, method_name, index)),
        escaped_html_string(&method_signature(receiver, method_name, method)),
        escaped_html_string(&method_summary(receiver, method_name)),
        escaped_html_string(&method_return_doc(&method.return_ty))
    )
}

#[cfg(feature = "docs-html")]
fn stdlib_record_html(name: &str, fields: BTreeMap<Name, Type>) -> String {
    let rows = fields
        .into_iter()
        .map(|(field, ty)| {
            format!(
                "<tr><td><code>{}</code></td><td><code>{}</code></td></tr>",
                escaped_html_string(field.as_str().as_str()),
                escaped_html_string(&render_type(&ty))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    stdlib_page_html(
        name,
        "../../",
        &format!("docs/STDLIB.md#{name}"),
        &format!(
            "<p><a href=\"../index.html\">Standard Library</a> / Records</p>
<h1>{}</h1>
<table class=\"stdlib-table\"><thead><tr><th>Field</th><th>Type</th></tr></thead><tbody>
{rows}
</tbody></table>",
            escaped_html_string(name)
        ),
    )
}

#[cfg(feature = "docs-html")]
fn stdlib_param_rows_html(params: &[xsh::modules::signature::ParamSig]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let rows = params
        .iter()
        .map(|param| {
            let default = if param.defaulted { "optional" } else { "required" };
            format!(
                "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{default}</td><td>{}</td></tr>",
                escaped_html_string(param.name),
                escaped_html_string(&render_type(&param.ty)),
                escaped_html_string(&param_doc(param.name, param.defaulted))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<table class=\"stdlib-table\"><thead><tr><th>Parameter</th><th>Type</th><th>Kind</th><th>Description</th></tr></thead><tbody>
{rows}
</tbody></table>"
    )
}

#[cfg(feature = "docs-html")]
fn stdlib_badges(pure: bool, command: bool) -> String {
    let mut badges = String::new();
    badges.push_str(if pure {
        "<span class=\"stdlib-badge pure\">pure</span>"
    } else {
        "<span class=\"stdlib-badge effect\">effect</span>"
    });
    if command {
        badges.push_str("<span class=\"stdlib-badge command\">command</span>");
    }
    badges
}

#[cfg(feature = "docs-html")]
fn stdlib_page_html(title: &str, prefix: &str, source: &str, content: &str) -> String {
    let title = escaped_html_string(title);
    let source = escaped_html_string(source);
    format!(
        "<!doctype html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">
<title>{title}</title>
<link rel=\"stylesheet\" href=\"{prefix}style.css\">
</head>
<body>
<header class=\"site-header\"><a href=\"{prefix}index.html\">XSH Docs</a><span>{source}</span></header>
<main class=\"markdown-body stdlib-body\">
{content}
</main>
</body>
</html>
"
    )
}

#[cfg(feature = "docs-html")]
fn docs_index_html(pages: &[MarkdownPage]) -> String {
    let mut links = String::new();
    for page in pages {
        if page.title == "XSH Standard Library" {
            links.push_str(
                "<li><a href=\"stdlib/index.html\">XSH Standard Library</a><span>docs-html/stdlib/</span></li>\n",
            );
            continue;
        }
        links.push_str("<li><a href=\"");
        push_escaped_attr(&mut links, html_file_name(&page.html_path));
        links.push_str("\">");
        push_escaped_html(&mut links, &page.title);
        links.push_str("</a><span>");
        push_escaped_html(&mut links, &page.markdown_path.to_string_lossy());
        links.push_str("</span></li>\n");
    }

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n<title>XSH Docs</title>\n<link rel=\"stylesheet\" href=\"style.css\">\n</head>\n<body>\n<main class=\"docs-index\">\n<h1>XSH Docs</h1>\n<p>Human-readable HTML generated from the checked-in markdown artifacts under <code>docs/</code>.</p>\n<ol class=\"docs-list\">\n{links}</ol>\n</main>\n</body>\n</html>\n"
    )
}

#[cfg(feature = "docs-html")]
fn docs_page_html(page: &MarkdownPage, pages: &[MarkdownPage], index: usize) -> String {
    let mut title = String::new();
    push_escaped_html(&mut title, &page.title);
    let toc = heading_toc(&page.contents);
    let body = add_heading_ids(markdown_to_html(&page.contents), &toc);
    let chapter_nav = page_chapter_nav(pages, index);
    let page_toc = page_toc_html(&toc);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n<title>{title}</title>\n<link rel=\"stylesheet\" href=\"style.css\">\n</head>\n<body>\n<header class=\"site-header\"><a href=\"index.html\">XSH Docs</a><span>{}</span></header>\n<div class=\"docs-shell\">\n<aside class=\"page-toc\" aria-label=\"Page contents\">\n{page_toc}</aside>\n<main class=\"markdown-body\">\n{chapter_nav}{body}{chapter_nav}</main>\n</div>\n</body>\n</html>\n",
        escaped_html_string(&page.markdown_path.to_string_lossy())
    )
}

#[cfg(feature = "docs-html")]
#[derive(Clone)]
struct TocEntry {
    title: String,
    slug: String,
}

#[cfg(feature = "docs-html")]
fn heading_toc(markdown: &str) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let Some(title) = line.strip_prefix("## ") else {
            continue;
        };
        let title = title.trim().trim_end_matches('#').trim().to_string();
        if title.is_empty() {
            continue;
        }
        entries.push(TocEntry {
            slug: heading_slug(&title),
            title,
        });
    }
    entries
}

#[cfg(feature = "docs-html")]
fn heading_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    if previous_dash {
        slug.pop();
    }
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

#[cfg(feature = "docs-html")]
fn page_toc_html(toc: &[TocEntry]) -> String {
    if toc.is_empty() {
        return String::new();
    }
    let mut output = String::from("<h2>On This Page</h2>\n<ol>\n");
    for entry in toc {
        output.push_str("<li><a href=\"#");
        push_escaped_attr(&mut output, &entry.slug);
        output.push_str("\">");
        push_escaped_html(&mut output, &plain_heading_title(&entry.title));
        output.push_str("</a></li>\n");
    }
    output.push_str("</ol>\n");
    output
}

#[cfg(feature = "docs-html")]
fn add_heading_ids(mut html: String, toc: &[TocEntry]) -> String {
    for entry in toc {
        let needle = format!("<h2>{}</h2>", rendered_heading_inner(&entry.title));
        let replacement = format!(
            "<h2 id=\"{}\">{}</h2>",
            escaped_attr_string(&entry.slug),
            rendered_heading_inner(&entry.title)
        );
        html = html.replacen(&needle, &replacement, 1);
    }
    html
}

#[cfg(feature = "docs-html")]
fn rendered_heading_inner(title: &str) -> String {
    let mut output = String::new();
    let mut in_code = false;
    for part in title.split('`') {
        if in_code {
            output.push_str("<code>");
            push_escaped_html(&mut output, part);
            output.push_str("</code>");
        } else {
            push_escaped_html(&mut output, part);
        }
        in_code = !in_code;
    }
    output
}

#[cfg(feature = "docs-html")]
fn plain_heading_title(title: &str) -> String {
    title.replace('`', "")
}

#[cfg(feature = "docs-html")]
fn page_chapter_nav(pages: &[MarkdownPage], index: usize) -> String {
    if !is_chapter_page(&pages[index]) {
        return String::new();
    }
    let prev = previous_markdown_page(pages, index);
    let next = next_markdown_page(pages, index);
    if prev.is_none() && next.is_none() {
        return String::new();
    }
    let mut output =
        String::from("<nav class=\"chapter-nav\" aria-label=\"Chapter navigation\">\n");
    if let Some(page) = prev {
        output.push_str("<a rel=\"prev\" href=\"");
        push_escaped_attr(&mut output, html_file_name(&page.html_path));
        output.push_str("\"><span>Previous</span>");
        push_escaped_html(&mut output, &page.title);
        output.push_str("</a>\n");
    } else {
        output.push_str("<span></span>\n");
    }
    if let Some(page) = next {
        output.push_str("<a rel=\"next\" href=\"");
        push_escaped_attr(&mut output, html_file_name(&page.html_path));
        output.push_str("\"><span>Next</span>");
        push_escaped_html(&mut output, &page.title);
        output.push_str("</a>\n");
    }
    output.push_str("</nav>\n");
    output
}

#[cfg(feature = "docs-html")]
fn previous_markdown_page(pages: &[MarkdownPage], index: usize) -> Option<&MarkdownPage> {
    pages[..index]
        .iter()
        .rev()
        .find(|page| is_chapter_page(page))
}

#[cfg(feature = "docs-html")]
fn next_markdown_page(pages: &[MarkdownPage], index: usize) -> Option<&MarkdownPage> {
    pages[index + 1..].iter().find(|page| is_chapter_page(page))
}

#[cfg(feature = "docs-html")]
fn is_chapter_page(page: &MarkdownPage) -> bool {
    page.markdown_path
        .to_string_lossy()
        .starts_with("docs/CHAPTER-")
}

#[cfg(feature = "docs-html")]
fn html_file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("index.html")
}

#[cfg(feature = "docs-html")]
fn markdown_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

#[cfg(feature = "docs-html")]
fn markdown_to_html(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, markdown_options());
    let mut output = String::new();
    let mut passthrough = Vec::new();
    let mut iter = parser.into_iter();

    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.as_ref() == "xsh" => {
                html::push_html(&mut output, passthrough.drain(..));
                let mut source = String::new();
                for event in iter.by_ref() {
                    match event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(text)
                        | Event::Code(text)
                        | Event::Html(text)
                        | Event::InlineHtml(text) => source.push_str(&text),
                        Event::SoftBreak | Event::HardBreak => source.push('\n'),
                        _ => {}
                    }
                }
                output.push_str(&highlighted_code_block(&source));
            }
            other => passthrough.push(other),
        }
    }

    html::push_html(&mut output, passthrough.into_iter());
    output
}

#[cfg(feature = "docs-html")]
fn highlighted_code_block(source: &str) -> String {
    let mut output = String::from("<pre><code class=\"language-xsh\">");
    output.push_str(&highlight_xsh(source));
    output.push_str("</code></pre>\n");
    output
}

#[cfg(feature = "docs-html")]
fn highlight_xsh(source: &str) -> String {
    let source_id = xsh::source::SourceId::new(0);
    let lexed = Lexer::new(source_id, source).lex_compact();
    let diagnostics = diagnostic_ranges(&lexed.diagnostics, source.len());
    let mut output = String::new();
    let mut cursor = 0usize;

    for index in 0..lexed.token_table.len() {
        let Some(tag) = lexed.token_table.tag_at(index) else {
            continue;
        };
        if tag == TokenTag::Eof {
            continue;
        }
        let Some(span) = lexed.token_table.span_at(index, source_id, source) else {
            continue;
        };
        push_highlight_gap(source, cursor, span.start(), &diagnostics, &mut output);
        let text = &source[span.start()..span.end()];
        if let Some(class) = token_css_class(
            tag,
            lexed.token_table.keyword_at(index),
            lexed.token_table.name_at(index),
        ) {
            output.push_str("<span class=\"");
            output.push_str(class);
            output.push_str("\">");
            push_escaped_html(&mut output, text);
            output.push_str("</span>");
        } else {
            push_escaped_html(&mut output, text);
        }
        cursor = span.end();
    }
    push_highlight_gap(source, cursor, source.len(), &diagnostics, &mut output);
    output
}

#[cfg(feature = "docs-html")]
fn diagnostic_ranges(
    diagnostics: &[xsh::diagnostic::Diagnostic],
    source_len: usize,
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for diagnostic in diagnostics {
        if let Some(span) = diagnostic.span {
            push_diagnostic_range(&mut ranges, span.start(), span.end(), source_len);
        }
        for label in &diagnostic.labels {
            push_diagnostic_range(
                &mut ranges,
                label.span.start(),
                label.span.end(),
                source_len,
            );
        }
    }
    insertion_sort_by(&mut ranges, |left, right| left.cmp(right));
    ranges
}

#[cfg(feature = "docs-html")]
fn push_diagnostic_range(
    ranges: &mut Vec<(usize, usize)>,
    start: usize,
    end: usize,
    source_len: usize,
) {
    let start = start.min(source_len);
    let mut end = end.min(source_len);
    if start == end && end < source_len {
        end += 1;
    }
    if start < end {
        ranges.push((start, end));
    }
}

#[cfg(feature = "docs-html")]
fn push_highlight_gap(
    source: &str,
    start: usize,
    end: usize,
    diagnostics: &[(usize, usize)],
    output: &mut String,
) {
    if start >= end {
        return;
    }
    let mut cursor = start;
    for (diag_start, diag_end) in diagnostics {
        if *diag_end <= cursor {
            continue;
        }
        if *diag_start >= end {
            break;
        }
        if cursor < *diag_start {
            push_escaped_html(output, &source[cursor..(*diag_start).min(end)]);
        }
        let marked_start = cursor.max(*diag_start);
        let marked_end = end.min(*diag_end);
        if marked_start < marked_end {
            output.push_str("<span class=\"tok-diagnostic\">");
            push_escaped_html(output, &source[marked_start..marked_end]);
            output.push_str("</span>");
        }
        cursor = marked_end;
    }
    if cursor < end {
        push_escaped_html(output, &source[cursor..end]);
    }
}

#[cfg(feature = "docs-html")]
fn token_css_class(
    tag: TokenTag,
    keyword: Option<Keyword>,
    name: Option<Name>,
) -> Option<&'static str> {
    match tag {
        TokenTag::Keyword
            if matches!(
                keyword,
                Some(Keyword::False | Keyword::Null | Keyword::True)
            ) =>
        {
            Some("tok-literal")
        }
        TokenTag::Keyword => Some("tok-keyword"),
        TokenTag::Ident
            if name.is_some_and(|name| name.as_str().starts_with(|c: char| c.is_uppercase())) =>
        {
            Some("tok-type")
        }
        TokenTag::Ident | TokenTag::ProcIdent | TokenTag::DollarIdent => Some("tok-ident"),
        TokenTag::String
        | TokenTag::PathString
        | TokenTag::GlobString
        | TokenTag::FmtString
        | TokenTag::PathFmtString => Some("tok-string"),
        TokenTag::Int
        | TokenTag::Float
        | TokenTag::Duration
        | TokenTag::Bytes
        | TokenTag::LastStatus => Some("tok-literal"),
        TokenTag::Comment => Some("tok-comment"),
        TokenTag::Equals
        | TokenTag::EqEq
        | TokenTag::Bang
        | TokenTag::BangEq
        | TokenTag::Lt
        | TokenTag::Le
        | TokenTag::Gt
        | TokenTag::Ge
        | TokenTag::Plus
        | TokenTag::Minus
        | TokenTag::Star
        | TokenTag::Slash
        | TokenTag::Percent
        | TokenTag::Pipe
        | TokenTag::PipeGt
        | TokenTag::Amp
        | TokenTag::GtGt
        | TokenTag::ErrorGt
        | TokenTag::ErrorGtGt
        | TokenTag::Question
        | TokenTag::QuestionQuestion
        | TokenTag::Arrow
        | TokenTag::FatArrow => Some("tok-operator"),
        TokenTag::LParen
        | TokenTag::RParen
        | TokenTag::LBrace
        | TokenTag::RBrace
        | TokenTag::LBracket
        | TokenTag::RBracket
        | TokenTag::Comma
        | TokenTag::Colon
        | TokenTag::Semicolon
        | TokenTag::Dot
        | TokenTag::At
        | TokenTag::DollarLBrace => Some("tok-punctuation"),
        TokenTag::Newline | TokenTag::Eof => None,
    }
}

#[cfg(feature = "docs-html")]
fn escaped_html_string(value: &str) -> String {
    let mut output = String::new();
    push_escaped_html(&mut output, value);
    output
}

#[cfg(feature = "docs-html")]
fn push_escaped_html(output: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(ch),
        }
    }
}

#[cfg(feature = "docs-html")]
fn push_escaped_attr(output: &mut String, value: &str) {
    push_escaped_html(output, value);
}

#[cfg(feature = "docs-html")]
fn escaped_attr_string(value: &str) -> String {
    let mut output = String::new();
    push_escaped_attr(&mut output, value);
    output
}

#[cfg(feature = "docs-html")]
fn docs_stylesheet() -> String {
    "\
:root {
  color-scheme: light;
  --bg: #ffffff;
  --panel: #f6f8fa;
  --text: #24292f;
  --muted: #57606a;
  --border: #d0d7de;
  --link: #0969da;
  --code: #f6f8fa;
  --keyword: #cf222e;
  --ident: #0550ae;
  --string: #116329;
  --literal: #c0107a;
  --type: #0e7069;
  --comment: #6e7781;
  --operator: #8250df;
  --punctuation: #57606a;
  --diagnostic: #b42318;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Helvetica, Arial, sans-serif;
}

a {
  color: var(--link);
  text-decoration: none;
}

a:hover {
  text-decoration: underline;
}

.site-header {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--border);
  padding: 12px 24px;
  color: var(--muted);
  font-size: 14px;
}

.site-header a {
  font-weight: 600;
}

.docs-index,
.markdown-body {
  max-width: 980px;
  margin: 0 auto;
  padding: 40px;
  line-height: 1.6;
}

.docs-shell {
  display: grid;
  grid-template-columns: minmax(180px, 240px) minmax(0, 980px);
  gap: 24px;
  max-width: 1280px;
  margin: 0 auto;
  padding: 0 24px;
}

.docs-shell .markdown-body {
  margin: 0;
  min-width: 0;
  padding-left: 0;
  padding-right: 0;
}

.page-toc {
  align-self: start;
  color: var(--muted);
  font-size: 14px;
  line-height: 1.4;
  max-height: calc(100vh - 72px);
  overflow: auto;
  padding-top: 40px;
  position: sticky;
  top: 0;
}

.page-toc h2 {
  color: var(--text);
  font-size: 13px;
  letter-spacing: .04em;
  margin: 0 0 8px;
  text-transform: uppercase;
}

.page-toc ol {
  list-style: none;
  margin: 0;
  padding: 0;
}

.page-toc li {
  margin: 8px 0;
}

.chapter-nav {
  border-bottom: 1px solid var(--border);
  border-top: 1px solid var(--border);
  display: grid;
  gap: 16px;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  margin: 0 0 28px;
  padding: 12px 0;
}

.chapter-nav:last-child {
  margin: 28px 0 0;
}

.chapter-nav a {
  display: block;
  min-width: 0;
}

.chapter-nav a[rel=\"next\"] {
  text-align: right;
}

.chapter-nav span {
  color: var(--muted);
  display: block;
  font-size: 12px;
  text-transform: uppercase;
}

.docs-list {
  padding-left: 24px;
}

.docs-list li {
  margin: 10px 0;
}

.docs-list span {
  display: block;
  color: var(--muted);
  font-size: 14px;
}

.markdown-body h1,
.markdown-body h2 {
  border-bottom: 1px solid var(--border);
  padding-bottom: .3em;
}

.markdown-body h1 {
  line-height: 1.15;
}

.markdown-body code,
.markdown-body pre {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: .95em;
}

.markdown-body pre {
  overflow: auto;
  background: var(--code);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 16px;
}

.markdown-body code {
  background: rgba(175, 184, 193, .2);
  border-radius: 6px;
  padding: .2em .4em;
}

.markdown-body pre code {
  background: transparent;
  padding: 0;
}

.markdown-body blockquote {
  margin: 1em 0;
  padding: 0 1em;
  color: var(--muted);
  border-left: .25em solid var(--border);
}

.markdown-body table {
  border-collapse: collapse;
}

.markdown-body th,
.markdown-body td {
  border: 1px solid var(--border);
  padding: 6px 13px;
}

.markdown-body tr:nth-child(2n) {
  background: var(--panel);
}

.stdlib-body {
  max-width: 1120px;
}

.stdlib-lede {
  color: var(--muted);
  font-size: 16px;
}

.stdlib-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  gap: 14px;
  margin: 18px 0 28px;
}

.stdlib-card {
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text);
  display: block;
  padding: 14px 16px;
}

.stdlib-card:hover {
  background: var(--panel);
  text-decoration: none;
}

.stdlib-card h3 {
  margin: 0 0 6px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 15px;
}

.stdlib-card p {
  color: var(--muted);
  margin: 0 0 8px;
}

.stdlib-card span,
.stdlib-return {
  color: var(--muted);
  font-size: 13px;
}

.stdlib-entry {
  border: 1px solid var(--border);
  border-radius: 8px;
  margin: 16px 0;
  overflow: hidden;
}

.stdlib-entry header {
  align-items: flex-start;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  display: flex;
  gap: 12px;
  justify-content: space-between;
  padding: 12px 14px;
}

.stdlib-entry header code {
  background: transparent;
  padding: 0;
}

.stdlib-entry p,
.stdlib-entry table {
  margin: 12px 14px;
}

.stdlib-badge {
  border-radius: 4px;
  display: inline-block;
  font-size: 11px;
  font-weight: 700;
  margin-left: 4px;
  padding: 2px 6px;
  text-transform: uppercase;
}

.stdlib-badge.pure {
  background: #dafbe1;
  color: #116329;
}

@media (max-width: 900px) {
  .docs-shell {
    display: block;
    padding: 0;
  }

  .page-toc {
    border-bottom: 1px solid var(--border);
    max-height: none;
    padding: 20px 40px 16px;
    position: static;
  }

  .docs-shell .markdown-body {
    padding-left: 40px;
    padding-right: 40px;
  }
}

.stdlib-badge.effect {
  background: #ffebe9;
  color: #cf222e;
}

.stdlib-badge.command {
  background: #fbefff;
  color: #8250df;
}

.stdlib-table {
  width: calc(100% - 28px);
}

.tok-keyword {
  color: var(--keyword);
  font-weight: 600;
}

.tok-ident {
  color: var(--ident);
}

.tok-string {
  color: var(--string);
}

.tok-literal {
  color: var(--literal);
}

.tok-type {
  color: var(--type);
}

.tok-comment {
  color: var(--comment);
  font-style: italic;
}

.tok-operator {
  color: var(--operator);
}

.tok-punctuation {
  color: var(--punctuation);
}

.tok-diagnostic {
  color: var(--diagnostic);
  text-decoration: underline;
  text-decoration-style: wavy;
}

@media (max-width: 767px) {
  .site-header {
    align-items: flex-start;
    flex-direction: column;
    gap: 4px;
    padding: 12px 20px;
  }

  .docs-index,
  .markdown-body {
    padding: 24px 20px;
  }
}
"
    .to_string()
}

fn render_directive(
    root: &Path,
    template: &Path,
    directive: &str,
    examples: &ExampleCatalog,
    include_ids: &BTreeSet<String>,
) -> Result<String, String> {
    let Some((kind, id)) = directive.split_once(':') else {
        return Err(format!(
            "invalid include directive '{{{{{directive}}}}}' in '{}'",
            template.display()
        ));
    };
    let id = id.trim();
    if !include_ids.contains(id) {
        return Err(format!(
            "unresolved docs include '{id}' in '{}'",
            template.display()
        ));
    }
    match kind.trim() {
        "include" => {
            let example = examples
                .examples
                .iter()
                .find(|example| example.include_id == id)
                .ok_or_else(|| {
                    format!("include '{id}' is not runnable in '{}'", template.display())
                })?;
            let source = fs::read_to_string(root.join(&example.path))
                .map_err(|err| format!("failed to read '{}': {err}", example.path))?;
            Ok(format!("```xsh\n{source}```"))
        }
        "trace-summary" | "trace-raw" => {
            let example = examples
                .examples
                .iter()
                .find(|example| example.include_id == id)
                .ok_or_else(|| {
                    format!("include '{id}' is not runnable in '{}'", template.display())
                })?;
            let raw = kind.trim() == "trace-raw";
            let rendered = render_example_trace(root, example, raw)?;
            let fence = "text";
            Ok(format!("```{fence}\n{rendered}```"))
        }
        other => Err(format!(
            "unknown include directive '{other}' in '{}'",
            template.display()
        )),
    }
}

fn render_example_trace(root: &Path, example: &ExampleCase, raw: bool) -> Result<String, String> {
    let source = fs::read_to_string(root.join(&example.path))
        .map_err(|err| format!("failed to read '{}': {err}", example.path))?;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(&example.path, source.clone());
    let parsed = XshParser::parse_source_arena_only(source_id, &source);
    if !parsed.diagnostics.is_empty() {
        return Err(format!(
            "failed to parse trace include '{}':\n{}",
            example.include_id,
            DiagnosticRenderer::new().render(&parsed.diagnostics, &sources)
        ));
    }
    let checked = Checker::check_arena(&parsed.arena, &source);
    if !checked.diagnostics.is_empty() {
        return Err(format!(
            "failed to check trace include '{}':\n{}",
            example.include_id,
            DiagnosticRenderer::new().render(&checked.diagnostics, &sources)
        ));
    }

    let output = Evaluator::new_with_sources(example.args.clone(), sources)
        .with_tracing()
        .eval(&parsed.arena, source_id);
    if output.status != example.expected_status as u8 {
        return Err(format!(
            "trace include '{}' exited with {}, expected {}",
            example.include_id, output.status, example.expected_status
        ));
    }
    let events = TraceNormalizer::new().normalize_events(&output.trace_events);
    if raw {
        Ok(normalize_trace_preview_paths(
            TraceTextRenderer::new().render_events(&events, &output.sources),
        ))
    } else {
        Ok(normalize_trace_preview_paths(
            TraceSummaryRenderer::new().render_events_with_width(&events, &output.sources, 96),
        ))
    }
}

fn normalize_trace_preview_paths(rendered: String) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return rendered;
    };
    rendered.replace(&cwd.to_string_lossy().into_owned(), "<cwd>")
}

fn include_ids(examples: &ExampleCatalog, errors: &mut Vec<String>) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for id in examples.examples.iter().map(|example| &example.include_id) {
        if id.trim().is_empty() {
            errors.push("catalog include IDs must not be empty".to_string());
        } else if !ids.insert(id.clone()) {
            errors.push(format!("duplicate include ID '{id}'"));
        }
    }
    ids
}

fn validate_examples(root: &Path, catalog: &ExampleCatalog, errors: &mut Vec<String>) {
    let mut catalog_paths = BTreeSet::new();
    let mut include_ids = BTreeSet::new();
    for example in &catalog.examples {
        if !include_ids.insert(&example.include_id) {
            errors.push(format!(
                "duplicate runnable include ID '{}'",
                example.include_id
            ));
        }
        if example.path.trim().is_empty() || example.chapter.trim().is_empty() {
            errors.push(format!(
                "example '{}' has incomplete metadata",
                example.include_id
            ));
        }
        catalog_paths.insert(example.path.clone());
        let path = root.join(&example.path);
        if !path.is_file() {
            errors.push(format!(
                "cataloged example '{}' does not exist",
                example.path
            ));
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                errors.push(format!("failed to read '{}': {err}", example.path));
                continue;
            }
        };
        let mut sources = SourceMap::new();
        let source_id = sources.add_file(&example.path, text.clone());
        let formatted = Formatter::new().format_source(source_id, &text);
        if !formatted.diagnostics.is_empty() {
            errors.push(format!(
                "example '{}' does not parse for formatting",
                example.path
            ));
        } else if formatted.formatted != text {
            errors.push(format!("example '{}' needs formatting", example.path));
        }
    }

    let mut discovered = BTreeSet::new();
    match fs::read_dir(root.join("examples")) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if path.extension().is_some_and(|extension| extension == "xsh") {
                            discovered.insert(
                                path.strip_prefix(root)
                                    .unwrap_or(&path)
                                    .to_string_lossy()
                                    .into_owned(),
                            );
                        }
                    }
                    Err(err) => errors.push(format!("failed to read examples entry: {err}")),
                }
            }
        }
        Err(err) => errors.push(format!("failed to read examples directory: {err}")),
    }
    if discovered != catalog_paths {
        errors.push(format!(
            "examples/catalog.json does not match examples/*.xsh: catalog={catalog_paths:?} discovered={discovered:?}"
        ));
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

    #[cfg(feature = "docs-html")]
    #[test]
    fn reference_json_data_generates_and_writes() {
        let json = crate::xsht::docs::reference_json_data().expect("reference JSON");
        assert!(json.contains("\"archive\""));
        assert!(json.contains("\"FsEntry\""));
        assert!(json.contains("\"Str\""));
        assert!(json.contains("\"run_forms\""));
        assert!(json.contains("\"effects\""));
        // Write to disk so the XSH generator can use it during development.
        let path = std::path::Path::new("docs-html/reference/data.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, &json).expect("write data.json");
    }
}
