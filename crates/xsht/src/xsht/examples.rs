use crate::xsht::format::Formatter;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use xsh::source::SourceMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExampleCatalog {
    pub examples: Vec<ExampleCase>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExampleCase {
    pub path: String,
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

pub fn load_catalog(root: impl AsRef<Path>) -> Result<ExampleCatalog, String> {
    let path = root.as_ref().join("examples/catalog.json");
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
    parse_catalog(&text).map_err(|err| format!("failed to parse '{}': {err}", path.display()))
}

pub fn validate_catalog(root: impl AsRef<Path>, catalog: &ExampleCatalog) -> Result<(), String> {
    let root = root.as_ref();
    let mut errors = Vec::new();
    let mut catalog_paths = BTreeSet::new();
    for example in &catalog.examples {
        if example.path.trim().is_empty() {
            errors.push("showcase path must not be empty".to_string());
            continue;
        }
        if !example.path.starts_with("examples/") || !example.path.ends_with(".xsh") {
            errors.push(format!(
                "showcase path '{}' must name an examples/*.xsh file",
                example.path
            ));
        }
        if !catalog_paths.insert(example.path.clone()) {
            errors.push(format!("duplicate showcase path '{}'", example.path));
            continue;
        }

        let path = root.join(&example.path);
        if !path.is_file() {
            errors.push(format!(
                "cataloged showcase '{}' does not exist",
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
                "showcase '{}' does not parse for formatting",
                example.path
            ));
        } else if formatted.formatted != text {
            errors.push(format!("showcase '{}' needs formatting", example.path));
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

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

pub fn test_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

fn parse_catalog(text: &str) -> Result<ExampleCatalog, String> {
    let value = xsh::modules::json::parse_raw_json(text)?;
    let examples = json_array_field(&value, "examples")?
        .iter()
        .map(parse_case)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ExampleCatalog { examples })
}

fn parse_case(value: &miniserde::json::Value) -> Result<ExampleCase, String> {
    let expected_status = json_u64_field(value, "expected_status")?;
    let expected_status = i32::try_from(expected_status)
        .map_err(|_| "example expected_status does not fit i32".to_string())?;
    Ok(ExampleCase {
        path: json_string_field(value, "path")?.to_string(),
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
