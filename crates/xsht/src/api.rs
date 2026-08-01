use std::fs;
use std::io::Read;
use xsh::modules::api_spec;
use xsh::modules::signature::{MethodReceiver, MethodReturn, ModuleFnSig};
use xsh::sema::records::record_schemas;
use xsh::sema::types::Type;
use xsh_registry::records::record_docs;
use xsh_registry::reference::language_references;
use xsh_registry::signature::{method_api_id, module_api_id, receiver_name};

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
    tags: Vec<String>,
    signatures: Vec<String>,
    runtime_ops: Vec<String>,
    implementation: Vec<String>,
    tests: Vec<String>,
    showcase: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApiResponse {
    query: String,
    status: &'static str,
    matches: Vec<ApiItem>,
    details: ApiDetails,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Selector {
    Module(String),
    Api(String, String),
    Method(String, String),
    Record(String),
    Language(String),
    Search(String),
}

pub fn query(options: &ApiOptions) -> Result<ApiOutput, ApiError> {
    let raw_queries = collect_queries(options)?;
    if raw_queries.is_empty() {
        return Err(usage_error(
            "`xsht api` requires at least one QUERY, --query-file, or --stdin",
        ));
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
        let details = options.details.unwrap_or_else(|| match selector {
            Selector::Search(_) => ApiDetails::Basic,
            _ => ApiDetails::Full,
        });
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
        "method" => split_member_selector(raw, value)
            .map(|(receiver, method)| Selector::Method(receiver, method)),
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
            .filter(|item| item.kind == "module" && item.id == format!("module.{module}"))
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
    if terms.iter().any(|term| id == *term) {
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
                Vec::new(),
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
                    function
                        .overloads
                        .iter()
                        .map(|overload| format!("{:?}", overload.op))
                        .collect(),
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
                    method
                        .overloads
                        .iter()
                        .map(|overload| format!("{:?}", overload.sig.op))
                        .collect(),
                ));
            }
        }
    }
    items.extend(record_items());
    items.extend(language_items());
    items
}

fn item_from_docs(
    id: String,
    kind: &'static str,
    docs: &xsh::modules::signature::ApiDocs,
    signatures: Vec<String>,
    runtime_ops: Vec<String>,
) -> ApiItem {
    ApiItem {
        id,
        kind,
        summary: docs.summary.clone(),
        contract: docs.contract.clone(),
        tags: docs.tags.clone(),
        signatures,
        runtime_ops,
        implementation: docs.navigation.implementation.clone(),
        tests: docs.navigation.tests.clone(),
        showcase: docs.navigation.showcase.clone(),
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
                Vec::new(),
            )
        })
        .collect()
}

fn language_items() -> Vec<ApiItem> {
    language_references()
        .into_iter()
        .map(|reference| {
            item_from_docs(
                format!("language.{}", reference.id),
                "language",
                &reference.docs,
                Vec::new(),
                Vec::new(),
            )
        })
        .collect()
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
    signature: &xsh::modules::signature::MethodSig,
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

fn render_params(params: &[xsh::modules::signature::ParamSig]) -> String {
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
            output.push_str("summary: ");
            output.push_str(&item.summary);
            output.push('\n');
            if response.details == ApiDetails::Full {
                write_full_text_item(&mut output, item);
            }
        }
    }
    output
}

fn write_full_text_item(output: &mut String, item: &ApiItem) {
    if !item.contract.is_empty() {
        output.push_str("contract: ");
        output.push_str(&item.contract);
        output.push('\n');
    }
    for signature in &item.signatures {
        output.push_str("signature: ");
        output.push_str(signature);
        output.push('\n');
    }
    for operation in &item.runtime_ops {
        output.push_str("runtime-op: ");
        output.push_str(operation);
        output.push('\n');
    }
    if !item.tags.is_empty() {
        output.push_str("tags: ");
        output.push_str(&item.tags.join(", "));
        output.push('\n');
    }
    for path in &item.implementation {
        output.push_str("implementation: ");
        output.push_str(path);
        output.push('\n');
    }
    for path in &item.tests {
        output.push_str("tests: ");
        output.push_str(path);
        output.push('\n');
    }
    if let Some(showcase) = &item.showcase {
        output.push_str("showcase: ");
        output.push_str(showcase);
        output.push('\n');
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
    push_json_array(output, "tags", &item.tags, true);
    push_json_array(output, "signatures", &item.signatures, true);
    push_json_array(output, "runtime_ops", &item.runtime_ops, true);
    push_json_array(output, "implementation", &item.implementation, true);
    push_json_array(output, "tests", &item.tests, true);
    output.push_str("\"showcase\":");
    match &item.showcase {
        Some(showcase) => push_json_string(output, showcase),
        None => output.push_str("null"),
    }
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
