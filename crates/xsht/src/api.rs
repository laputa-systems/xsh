use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use xsh::api::api_spec;
use xsh::api::{MethodReceiver, MethodReturn, ModuleFnSig};
use xsh::frontend::check::Type;
use xsh::frontend::check::record_schemas;
use xsh_registry::reference::language_references;
use xsh_registry::signature::{method_api_id, module_api_id, receiver_name, record_docs};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiFormat {
    Text,
    Jsonl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiDetails {
    Basic,
    Full,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiOptions {
    pub summary: bool,
    pub queries: Vec<String>,
    pub query_files: Vec<String>,
    pub read_stdin: bool,
    pub format: ApiFormat,
    pub strict: bool,
    pub details: Option<ApiDetails>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiOutput {
    pub status: u8,
    pub stdout: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiError {
    pub status: u8,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApiItem {
    id: String,
    kind: &'static str,
    summary: String,
    contract: String,
    example: Option<String>,
    effects: Vec<String>,
    tags: Vec<String>,
    signatures: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApiResponse {
    query: String,
    status: &'static str,
    matches: Vec<ApiItem>,
    details: ApiDetails,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApiSummary {
    standard_modules: usize,
    module_functions: usize,
    module_overloads: usize,
    method_receivers: usize,
    methods: usize,
    method_overloads: usize,
    standard_records: usize,
    language_reference_items: usize,
    total_queryable_items: usize,
    documented_items: usize,
    modules: Vec<ApiModuleTree>,
    method_receivers_tree: Vec<ApiMethodReceiverTree>,
    records: Vec<String>,
    language_groups: Vec<ApiLanguageGroup>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApiModuleTree {
    name: String,
    functions: Vec<ApiCallableTree>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApiMethodReceiverTree {
    name: String,
    methods: Vec<ApiCallableTree>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApiCallableTree {
    name: String,
    overloads: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApiLanguageGroup {
    name: String,
    items: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Selector {
    Module(String),
    Api(String, String),
    Method(String, String),
    MethodReceiver(String),
    Record(String),
    Language(String),
    Search(String),
}

pub fn query(options: &ApiOptions) -> Result<ApiOutput, ApiError> {
    if options.summary {
        return summary(options);
    }
    let raw_queries = collect_queries(options)?;
    if raw_queries.is_empty() {
        return Ok(intro(options));
    }

    let catalog = catalog();
    let mut responses = Vec::with_capacity(raw_queries.len());
    let mut missing = false;
    for (raw_query, from_input) in raw_queries {
        let selector = parse_selector(&raw_query).map_err(|message| ApiError {
            status: if from_input { 1 } else { 2 },
            message,
        })?;
        let matches = select(&catalog, &selector);
        if matches.is_empty() {
            missing = true;
        }
        let details = options
            .details
            .unwrap_or_else(|| default_details(&selector, &matches));
        responses.push(ApiResponse {
            query: raw_query,
            status: if matches.is_empty() {
                "missing"
            } else if matches.len() == 1 {
                "exact"
            } else {
                "matches"
            },
            matches,
            details,
        });
    }

    let stdout = match options.format {
        ApiFormat::Text => render_text(&responses),
        ApiFormat::Jsonl => render_jsonl(&responses),
    };
    Ok(ApiOutput {
        status: if options.strict && missing { 1 } else { 0 },
        stdout,
    })
}

fn summary(options: &ApiOptions) -> Result<ApiOutput, ApiError> {
    if !options.queries.is_empty() || !options.query_files.is_empty() || options.read_stdin {
        return Err(usage_error(
            "`xsht api summary` does not accept selectors, --query-file, or --stdin",
        ));
    }
    if options.strict || options.details.is_some() {
        return Err(usage_error(
            "`xsht api summary` accepts only the optional --format text|jsonl",
        ));
    }

    let catalog = catalog();
    let spec = api_spec();
    let mut modules = spec
        .module_entries()
        .map(|(name, module)| ApiModuleTree {
            name: name.to_string(),
            functions: module
                .functions
                .iter()
                .map(|function| ApiCallableTree {
                    name: function.name.to_string(),
                    overloads: function.overloads.len(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| left.name.cmp(&right.name));
    for module in &mut modules {
        module
            .functions
            .sort_by(|left, right| left.name.cmp(&right.name));
    }

    let mut method_receivers_tree = spec
        .method_entries()
        .map(|(receiver, methods)| ApiMethodReceiverTree {
            name: summary_receiver_name(receiver).to_string(),
            methods: methods
                .iter()
                .map(|method| ApiCallableTree {
                    name: method.name.to_string(),
                    overloads: method.overloads.len(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    method_receivers_tree.sort_by(|left, right| left.name.cmp(&right.name));
    for receiver in &mut method_receivers_tree {
        receiver
            .methods
            .sort_by(|left, right| left.name.cmp(&right.name));
    }

    let records = record_schemas()
        .keys()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    let mut language_groups = BTreeMap::<String, Vec<String>>::new();
    for reference in language_references() {
        let (group, item) = reference
            .id
            .split_once('.')
            .map_or(("other", reference.id.as_str()), |(group, item)| {
                (group, item)
            });
        language_groups
            .entry(group.to_string())
            .or_default()
            .push(item.to_string());
    }
    let language_groups = language_groups
        .into_iter()
        .map(|(name, mut items)| {
            items.sort();
            ApiLanguageGroup { name, items }
        })
        .collect::<Vec<_>>();

    let standard_modules = modules.len();
    let module_functions = modules.iter().map(|module| module.functions.len()).sum();
    let module_overloads = modules
        .iter()
        .flat_map(|module| module.functions.iter())
        .map(|function| function.overloads)
        .sum();
    let method_receivers = method_receivers_tree.len();
    let methods = method_receivers_tree
        .iter()
        .map(|receiver| receiver.methods.len())
        .sum();
    let method_overloads = method_receivers_tree
        .iter()
        .flat_map(|receiver| receiver.methods.iter())
        .map(|method| method.overloads)
        .sum();
    let standard_records = records.len();
    let language_reference_items = language_groups.iter().map(|group| group.items.len()).sum();
    let documented_items = catalog
        .iter()
        .filter(|item| !item.summary.trim().is_empty())
        .count();
    assert_eq!(
        documented_items,
        catalog.len(),
        "the canonical API catalog contains an undocumented public item"
    );
    let summary = ApiSummary {
        standard_modules,
        module_functions,
        module_overloads,
        method_receivers,
        methods,
        method_overloads,
        standard_records,
        language_reference_items,
        total_queryable_items: catalog.len(),
        documented_items,
        modules,
        method_receivers_tree,
        records,
        language_groups,
    };
    let stdout = match options.format {
        ApiFormat::Text => render_summary_text(&summary),
        ApiFormat::Jsonl => render_summary_jsonl(&summary),
    };
    Ok(ApiOutput { status: 0, stdout })
}

fn collect_queries(options: &ApiOptions) -> Result<Vec<(String, bool)>, ApiError> {
    let mut queries = options
        .queries
        .iter()
        .cloned()
        .map(|query| (query, false))
        .collect::<Vec<_>>();
    for path in &options.query_files {
        let text = fs::read_to_string(path).map_err(|error| ApiError {
            status: 1,
            message: format!("failed to read API query file '{path}': {error}"),
        })?;
        queries.extend(query_lines(&text).map(|query| (query, true)));
    }
    if options.read_stdin {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| ApiError {
                status: 1,
                message: format!("failed to read API queries from stdin: {error}"),
            })?;
        queries.extend(query_lines(&text).map(|query| (query, true)));
    }
    Ok(queries)
}

fn intro(options: &ApiOptions) -> ApiOutput {
    let stdout = match options.format {
        ApiFormat::Text => intro_text(),
        ApiFormat::Jsonl => intro_jsonl(),
    };
    ApiOutput { status: 0, stdout }
}

const INTRO_SCRIPT: &str = include_str!("../../../docs/snippets/api/hello.xsh");

fn intro_text() -> String {
    format!(
        "XSH API getting started\n\nWrite this as hello.xsh:\n\n{}\n\nBasic development loop:\n  xsht check hello.xsh\n  xsht fmt hello.xsh\n  xsht lint hello.xsh\n  xsh hello.xsh\n\nAsk for language rules, a module or receiver overview, or one exact API item:\n  xsht api language:core\n  xsht api module:fs\n  xsht api method:Str\n  xsht api api:fs.read_text\n  xsht api method:Path.read_text\n  xsht api record:FsEntry\n  xsht api search:rooted extraction\n\n`method:Str` lists every method on the Str receiver by purpose; append a member name (method:Str.lower) to read one exact item. Exact API items include purpose, contract, effects, signatures, tags, and a small example when one is useful. Use `xsht api summary` for the complete index and `--format jsonl` for machine-readable output.\n",
        INTRO_SCRIPT.trim_end(),
    )
}

fn intro_jsonl() -> String {
    let mut output = String::from("{\"schema_version\":1,\"kind\":\"guide\"");
    output.push(',');
    push_json_field(&mut output, "title", "XSH API getting started", true);
    push_json_field(&mut output, "script", INTRO_SCRIPT.trim_end(), true);
    push_json_array(
        &mut output,
        "loop",
        &[
            "xsht check hello.xsh".to_string(),
            "xsht fmt hello.xsh".to_string(),
            "xsht lint hello.xsh".to_string(),
            "xsh hello.xsh".to_string(),
        ],
        true,
    );
    push_json_array(
        &mut output,
        "queries",
        &[
            "language:core".to_string(),
            "module:fs".to_string(),
            "api:fs.read_text".to_string(),
            "method:Str".to_string(),
            "method:Path.read_text".to_string(),
            "record:FsEntry".to_string(),
            "search:rooted extraction".to_string(),
        ],
        false,
    );
    output.push_str("}\n");
    output
}

fn query_lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

fn parse_selector(raw: &str) -> Result<Selector, String> {
    let Some((kind, value)) = raw.split_once(':') else {
        return Err(format!("invalid API query '{raw}'; expected KIND:VALUE"));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(format!(
            "invalid API query '{raw}'; selector value is empty"
        ));
    }
    match kind {
        "module" => Ok(Selector::Module(value.to_string())),
        "api" => split_member_selector(raw, value)
            .map(|(module, function)| Selector::Api(module, function)),
        "method" => {
            if value.contains('.') {
                split_member_selector(raw, value)
                    .map(|(receiver, method)| Selector::Method(receiver, method))
            } else {
                Ok(Selector::MethodReceiver(value.to_string()))
            }
        }
        "record" => Ok(Selector::Record(value.to_string())),
        "language" => Ok(Selector::Language(value.to_string())),
        "search" => Ok(Selector::Search(value.to_string())),
        _ => Err(format!(
            "invalid API query '{raw}'; unknown selector kind '{kind}'"
        )),
    }
}

fn split_member_selector(raw: &str, value: &str) -> Result<(String, String), String> {
    let Some((left, right)) = value.split_once('.') else {
        return Err(format!("invalid API query '{raw}'; expected NAME.MEMBER"));
    };
    if left.is_empty() || right.is_empty() || right.contains('.') {
        return Err(format!("invalid API query '{raw}'; expected NAME.MEMBER"));
    }
    Ok((left.to_string(), right.to_string()))
}

fn select(catalog: &[ApiItem], selector: &Selector) -> Vec<ApiItem> {
    let mut matches = match selector {
        Selector::Module(module) => catalog
            .iter()
            .filter(|item| {
                (item.kind == "module" || item.kind == "module-function")
                    && (item.id == format!("module.{module}")
                        || item.id.starts_with(&format!("module.{module}.")))
            })
            .cloned()
            .collect(),
        Selector::Api(module, function) => catalog
            .iter()
            .filter(|item| item.id == module_api_id(module, function))
            .cloned()
            .collect(),
        Selector::Method(receiver, method) => catalog
            .iter()
            .filter(|item| {
                item.kind == "method" && item.id == format!("method.{receiver}.{method}")
            })
            .cloned()
            .collect(),
        Selector::MethodReceiver(receiver) => catalog
            .iter()
            .filter(|item| {
                item.kind == "method" && item.id.starts_with(&format!("method.{receiver}."))
            })
            .cloned()
            .collect(),
        Selector::Record(record) => catalog
            .iter()
            .filter(|item| item.id == format!("record.{record}"))
            .cloned()
            .collect(),
        Selector::Language(language) => catalog
            .iter()
            .filter(|item| {
                item.kind == "language"
                    && (item.id == format!("language.{language}")
                        || item.id.starts_with(&format!("language.{language}.")))
            })
            .cloned()
            .collect(),
        Selector::Search(terms) => search(catalog, terms),
    };
    matches.sort_by(|left, right| left.id.cmp(&right.id));
    matches
}

fn default_details(selector: &Selector, matches: &[ApiItem]) -> ApiDetails {
    match selector {
        // A `module:NAME.MEMBER` query resolves to a single module-function item;
        // render its contract as fully as `api:NAME.MEMBER` so the text formatter
        // shows the signature. A bare `module:NAME` overview keeps Basic output.
        Selector::Module(_) if matches.len() == 1 && matches[0].kind == "module-function" => {
            ApiDetails::Full
        }
        Selector::Module(_) | Selector::MethodReceiver(_) | Selector::Search(_) => {
            ApiDetails::Basic
        }
        Selector::Language(_) => ApiDetails::Full,
        Selector::Api(_, _) | Selector::Method(_, _) | Selector::Record(_) => ApiDetails::Full,
    }
}

fn search(catalog: &[ApiItem], terms: &str) -> Vec<ApiItem> {
    let terms = terms
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut matches = catalog
        .iter()
        .filter_map(|item| search_score(item, &terms).map(|score| (score, item.clone())))
        .collect::<Vec<_>>();
    matches.sort_by(|(left_score, left), (right_score, right)| {
        left_score
            .cmp(right_score)
            .then_with(|| left.id.cmp(&right.id))
    });
    matches.into_iter().map(|(_, item)| item).collect()
}

fn search_score(item: &ApiItem, terms: &[String]) -> Option<u8> {
    let id = item.id.to_ascii_lowercase();
    let summary = item.summary.to_ascii_lowercase();
    let contract = item.contract.to_ascii_lowercase();
    let tags = item
        .tags
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if !terms.iter().all(|term| {
        id.contains(term)
            || summary.contains(term)
            || contract.contains(term)
            || tags.iter().any(|tag| tag.contains(term))
    }) {
        return None;
    }
    if terms.contains(&id) {
        return Some(0);
    }
    if terms.iter().any(|term| tags.iter().any(|tag| tag == term)) {
        return Some(1);
    }
    if terms
        .iter()
        .any(|term| id.split('.').any(|part| part.starts_with(term)))
    {
        return Some(2);
    }
    Some(3)
}

fn catalog() -> Vec<ApiItem> {
    let spec = api_spec();
    let mut items = Vec::new();
    for (module_name, module) in spec.module_entries() {
        if let Some(docs) = spec.docs(&format!("module.{module_name}")) {
            items.push(item_from_docs(
                format!("module.{module_name}"),
                "module",
                docs,
                module
                    .functions
                    .iter()
                    .flat_map(|function| {
                        function.overloads.iter().map(move |overload| {
                            module_signature(module_name, function.name, overload)
                        })
                    })
                    .collect(),
                module_effects(module),
            ));
        }
        for function in &module.functions {
            let id = module_api_id(module_name, function.name);
            if let Some(docs) = spec.docs(&id) {
                items.push(item_from_docs(
                    id,
                    "module-function",
                    docs,
                    function
                        .overloads
                        .iter()
                        .map(|overload| module_signature(module_name, function.name, overload))
                        .collect(),
                    module_function_effects(&function.overloads),
                ));
            }
        }
    }
    for (receiver, methods) in spec.method_entries() {
        for method in methods {
            let id = method_api_id(receiver, method.name);
            if let Some(docs) = spec.docs(&id) {
                items.push(item_from_docs(
                    id,
                    "method",
                    docs,
                    method
                        .overloads
                        .iter()
                        .map(|overload| method_signature(receiver, method.name, overload))
                        .collect(),
                    method_effects(&method.overloads),
                ));
            }
        }
    }
    items.extend(record_items());
    items.extend(language_items());
    assert!(
        items.iter().all(|item| !item.summary.trim().is_empty()),
        "the canonical API catalog contains an undocumented public item"
    );
    items
}

fn item_from_docs(
    id: String,
    kind: &'static str,
    docs: &xsh::api::ApiDocs,
    signatures: Vec<String>,
    effects: Vec<String>,
) -> ApiItem {
    ApiItem {
        id,
        kind,
        summary: docs.summary.clone(),
        contract: docs.contract.clone(),
        example: docs.example.clone(),
        effects,
        tags: docs.tags.clone(),
        signatures,
    }
}

fn record_items() -> Vec<ApiItem> {
    record_schemas()
        .into_iter()
        .map(|(name, ty)| {
            let docs = record_docs(name);
            item_from_docs(
                format!("record.{name}"),
                "record",
                &docs,
                vec![format!("{name} {}", render_type(&ty))],
                vec!["none".to_string()],
            )
        })
        .collect()
}

fn language_items() -> Vec<ApiItem> {
    language_references()
        .into_iter()
        .map(|reference| {
            let signatures = if reference.signature.is_empty() {
                Vec::new()
            } else {
                vec![reference.signature.clone()]
            };
            let effects = if reference.effects.is_empty() {
                vec!["none".to_string()]
            } else {
                reference.effects.clone()
            };
            item_from_docs(
                format!("language.{}", reference.id),
                "language",
                &reference.docs,
                signatures,
                effects,
            )
        })
        .collect()
}

fn module_effects(signature: &xsh::api::ModuleSig) -> Vec<String> {
    let mut effects = BTreeSet::new();
    for function in &signature.functions {
        for effect in module_function_effects(&function.overloads) {
            effects.insert(effect);
        }
    }
    effects.into_iter().collect()
}

fn module_function_effects(overloads: &[ModuleFnSig]) -> Vec<String> {
    let mut effects = BTreeSet::new();
    for overload in overloads {
        if let Some(effect) = &overload.effect {
            effects.insert(effect.as_str().to_string());
        }
    }
    if effects.is_empty() {
        effects.insert("none".to_string());
    }
    effects.into_iter().collect()
}

fn method_effects(overloads: &[xsh::api::MethodSig]) -> Vec<String> {
    let mut effects = BTreeSet::new();
    for overload in overloads {
        if let Some(effect) = &overload.sig.effect {
            effects.insert(effect.as_str().to_string());
        }
    }
    if effects.is_empty() {
        effects.insert("none".to_string());
    }
    effects.into_iter().collect()
}

fn module_signature(module: &str, function: &str, signature: &ModuleFnSig) -> String {
    format!(
        "{module}.{function}({}) -> {}",
        render_params(&signature.params),
        render_type(&signature.return_ty)
    )
}

fn method_signature(
    receiver: MethodReceiver,
    method: &str,
    signature: &xsh::api::MethodSig,
) -> String {
    let return_type = match &signature.return_ty {
        MethodReturn::Type(ty) => render_type(ty),
        MethodReturn::Receiver => "Self".to_string(),
    };
    format!(
        "{}.{method}({}) -> {return_type}",
        receiver_name(receiver),
        render_params(&signature.sig.params)
    )
}

fn render_params(params: &[xsh::api::ParamSig]) -> String {
    params
        .iter()
        .map(|param| {
            let suffix = if param.defaulted { " = default" } else { "" };
            format!("{}: {}{suffix}", param.name, render_type(&param.ty))
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
        Type::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, field_type)| format!("{name}: {}", render_type(field_type)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Module(_) => "Module".to_string(),
        Type::Result(ok, error) => format!("Result[{}, {}]", render_type(ok), render_type(error)),
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

fn render_text(responses: &[ApiResponse]) -> String {
    let mut output = String::new();
    for (index, response) in responses.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str("query: ");
        output.push_str(&response.query);
        output.push('\n');
        output.push_str("status: ");
        output.push_str(response.status);
        output.push('\n');
        for item in &response.matches {
            output.push('\n');
            output.push_str("api: ");
            output.push_str(&item.id);
            output.push('\n');
            output.push_str("kind: ");
            output.push_str(item.kind);
            output.push('\n');
            output.push_str("purpose: ");
            output.push_str(&item.summary);
            output.push('\n');
            if response.details == ApiDetails::Full {
                write_full_text_item(&mut output, item);
            }
        }
    }
    output
}

fn render_summary_text(summary: &ApiSummary) -> String {
    let mut output = format!(
        "XSH API summary\n\
standard modules: {}\n\
module functions: {}\n\
module overloads: {}\n\
method receivers: {}\n\
methods: {}\n\
method overloads: {}\n\
standard records: {}\n\
language reference items: {}\n\
total queryable items: {}\n\
documented items: {}\n\
",
        summary.standard_modules,
        summary.module_functions,
        summary.module_overloads,
        summary.method_receivers,
        summary.methods,
        summary.method_overloads,
        summary.standard_records,
        summary.language_reference_items,
        summary.total_queryable_items,
        summary.documented_items,
    );
    append_callable_tree(
        &mut output,
        "modules",
        summary
            .modules
            .iter()
            .map(|module| (&module.name, &module.functions)),
    );
    append_callable_tree(
        &mut output,
        "methods",
        summary
            .method_receivers_tree
            .iter()
            .map(|receiver| (&receiver.name, &receiver.methods)),
    );
    append_leaf_tree(&mut output, "records", &summary.records);
    let language_groups = summary
        .language_groups
        .iter()
        .map(|group| (&group.name, &group.items))
        .collect::<Vec<_>>();
    append_group_tree(&mut output, "language", &language_groups);
    output
}

fn render_summary_jsonl(summary: &ApiSummary) -> String {
    let mut output = format!(
        "{{\"schema_version\":1,\"kind\":\"summary\",\"standard_modules\":{},\"module_functions\":{},\"module_overloads\":{},\"method_receivers\":{},\"methods\":{},\"method_overloads\":{},\"standard_records\":{},\"language_reference_items\":{},\"total_queryable_items\":{},\"documented_items\":{}",
        summary.standard_modules,
        summary.module_functions,
        summary.module_overloads,
        summary.method_receivers,
        summary.methods,
        summary.method_overloads,
        summary.standard_records,
        summary.language_reference_items,
        summary.total_queryable_items,
        summary.documented_items,
    );
    push_summary_modules_json(&mut output, &summary.modules);
    push_summary_methods_json(&mut output, &summary.method_receivers_tree);
    output.push(',');
    push_json_array(&mut output, "records", &summary.records, false);
    push_summary_language_json(&mut output, &summary.language_groups);
    output.push_str("}\n");
    output
}

fn append_callable_tree<'a>(
    output: &mut String,
    title: &str,
    groups: impl Iterator<Item = (&'a String, &'a Vec<ApiCallableTree>)>,
) {
    output.push('\n');
    output.push_str(title);
    output.push('\n');
    let groups = groups.collect::<Vec<_>>();
    for (group_index, (group, callables)) in groups.iter().enumerate() {
        let group_last = group_index + 1 == groups.len();
        output.push_str(if group_last {
            "└── "
        } else {
            "├── "
        });
        output.push_str(group);
        output.push_str(&format!(" ({} items)\n", callables.len()));
        for (callable_index, callable) in callables.iter().enumerate() {
            output.push_str(if group_last { "    " } else { "│   " });
            output.push_str(if callable_index + 1 == callables.len() {
                "└── "
            } else {
                "├── "
            });
            output.push_str(&callable.name);
            output.push_str(" (");
            output.push_str(&overload_label(callable.overloads));
            output.push_str(")\n");
        }
    }
}

fn append_leaf_tree(output: &mut String, title: &str, items: &[String]) {
    output.push('\n');
    output.push_str(title);
    output.push('\n');
    for (index, item) in items.iter().enumerate() {
        output.push_str(if index + 1 == items.len() {
            "└── "
        } else {
            "├── "
        });
        output.push_str(item);
        output.push('\n');
    }
}

fn append_group_tree(output: &mut String, title: &str, groups: &[(&String, &Vec<String>)]) {
    output.push('\n');
    output.push_str(title);
    output.push('\n');
    for (group_index, (group, items)) in groups.iter().enumerate() {
        let group_last = group_index + 1 == groups.len();
        output.push_str(if group_last {
            "└── "
        } else {
            "├── "
        });
        output.push_str(group);
        output.push_str(&format!(" ({} items)\n", items.len()));
        for (item_index, item) in items.iter().enumerate() {
            output.push_str(if group_last { "    " } else { "│   " });
            output.push_str(if item_index + 1 == items.len() {
                "└── "
            } else {
                "├── "
            });
            output.push_str(item);
            output.push('\n');
        }
    }
}

fn push_summary_modules_json(output: &mut String, modules: &[ApiModuleTree]) {
    output.push_str(",\"modules\":[");
    for (module_index, module) in modules.iter().enumerate() {
        if module_index > 0 {
            output.push(',');
        }
        output.push_str("{\"name\":");
        push_json_string(output, &module.name);
        output.push_str(",\"functions\":[");
        push_callable_tree_json(output, &module.functions);
        output.push_str("]}");
    }
    output.push(']');
}

fn push_summary_methods_json(output: &mut String, receivers: &[ApiMethodReceiverTree]) {
    output.push_str(",\"method_receivers\":[");
    for (receiver_index, receiver) in receivers.iter().enumerate() {
        if receiver_index > 0 {
            output.push(',');
        }
        output.push_str("{\"name\":");
        push_json_string(output, &receiver.name);
        output.push_str(",\"methods\":[");
        push_callable_tree_json(output, &receiver.methods);
        output.push_str("]}");
    }
    output.push(']');
}

fn push_callable_tree_json(output: &mut String, callables: &[ApiCallableTree]) {
    for (callable_index, callable) in callables.iter().enumerate() {
        if callable_index > 0 {
            output.push(',');
        }
        output.push_str("{\"name\":");
        push_json_string(output, &callable.name);
        output.push_str(&format!(",\"overloads\":{}}}", callable.overloads));
    }
}

fn push_summary_language_json(output: &mut String, groups: &[ApiLanguageGroup]) {
    output.push_str(",\"language_groups\":[");
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 {
            output.push(',');
        }
        output.push_str("{\"name\":");
        push_json_string(output, &group.name);
        output.push_str(",\"items\":[");
        for (item_index, item) in group.items.iter().enumerate() {
            if item_index > 0 {
                output.push(',');
            }
            push_json_string(output, item);
        }
        output.push_str("]}");
    }
    output.push(']');
}

fn summary_receiver_name(receiver: MethodReceiver) -> &'static str {
    match receiver {
        MethodReceiver::PathConstructor => "Path constructor",
        MethodReceiver::Path => "Path methods",
        _ => receiver_name(receiver),
    }
}

fn overload_label(overloads: usize) -> String {
    if overloads == 1 {
        "1 overload".to_string()
    } else {
        format!("{overloads} overloads")
    }
}

fn write_full_text_item(output: &mut String, item: &ApiItem) {
    if !item.contract.is_empty() {
        output.push_str("contract: ");
        output.push_str(&item.contract);
        output.push('\n');
    }
    output.push_str("effects: ");
    output.push_str(&item.effects.join(", "));
    output.push('\n');
    for signature in &item.signatures {
        output.push_str("signature: ");
        output.push_str(signature);
        output.push('\n');
    }
    if !item.tags.is_empty() {
        output.push_str("tags: ");
        output.push_str(&item.tags.join(", "));
        output.push('\n');
    }
    if let Some(example) = &item.example {
        output.push_str("example:\n");
        for line in example.lines() {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }
    }
}

fn render_jsonl(responses: &[ApiResponse]) -> String {
    let mut output = String::new();
    for response in responses {
        output.push_str("{\"schema_version\":1,\"query\":");
        push_json_string(&mut output, &response.query);
        output.push_str(",\"status\":");
        push_json_string(&mut output, response.status);
        output.push_str(",\"matches\":[");
        for (index, item) in response.matches.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            push_json_item(&mut output, item);
        }
        output.push_str("]}\n");
    }
    output
}

fn push_json_item(output: &mut String, item: &ApiItem) {
    output.push('{');
    push_json_field(output, "id", &item.id, true);
    push_json_field(output, "kind", item.kind, true);
    push_json_field(output, "summary", &item.summary, true);
    push_json_field(output, "contract", &item.contract, true);
    push_json_array(output, "effects", &item.effects, true);
    output.push_str("\"example\":");
    match &item.example {
        Some(example) => push_json_string(output, example),
        None => output.push_str("null"),
    }
    output.push(',');
    push_json_array(output, "tags", &item.tags, true);
    push_json_array(output, "signatures", &item.signatures, true);
    output.push('}');
}

fn push_json_field(output: &mut String, key: &str, value: &str, trailing: bool) {
    push_json_string(output, key);
    output.push(':');
    push_json_string(output, value);
    if trailing {
        output.push(',');
    }
}

fn push_json_array(output: &mut String, key: &str, values: &[String], trailing: bool) {
    push_json_string(output, key);
    output.push_str(":[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        push_json_string(output, value);
    }
    output.push(']');
    if trailing {
        output.push(',');
    }
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", character as u32))
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

fn usage_error(message: impl Into<String>) -> ApiError {
    ApiError {
        status: 2,
        message: message.into(),
    }
}
