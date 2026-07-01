use miniserde::json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use xsh::modules::json::{
    parse_raw_json, pretty_raw_json, raw_json_array, raw_json_as_str, raw_json_get,
    raw_json_object, raw_json_string, raw_json_u64, raw_json_usize,
};
use xsh::modules::{MethodReceiver, api_spec};

#[derive(Clone, Debug, Default)]
pub struct CoverageCollector {
    api_hits: BTreeMap<String, CoverageHits>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CoverageHits {
    tests: u64,
    examples: u64,
}

impl CoverageCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest_jsonl(&mut self, scope: &str, jsonl: &str) -> Result<(), String> {
        for (index, line) in jsonl.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value = parse_raw_json(line)
                .map_err(|err| format!("failed to parse trace JSONL line {}: {err}", index + 1))?;
            let kind = raw_json_get(&value, "kind")
                .and_then(raw_json_as_str)
                .unwrap_or("");
            if (kind.ends_with(".call")
                || kind.ends_with(".start")
                || kind.ends_with(".enter")
                || kind == "cwd.enter")
                && let Some(api_id) = raw_json_get(&value, "api_id").and_then(raw_json_as_str)
            {
                self.api_hits
                    .entry(api_id.to_string())
                    .or_default()
                    .add(scope);
            }
        }
        Ok(())
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("coverage report\n");
        output.push_str("API coverage\n");
        self.render_api_totals(&mut output);
        output.push('\n');
        output.push_str("uncovered standard APIs\n");
        self.render_uncovered_apis(&mut output);
        output.push('\n');
        output.push_str("APIs covered by examples/tests\n");
        self.render_covered_apis(&mut output);
        output
    }

    pub fn write_json(&self, path: &str) -> Result<(), String> {
        let value = raw_json_object([
            ("api_hits".to_string(), self.api_hits_json()),
            (
                "standard_apis".to_string(),
                raw_json_array(standard_api_ids().into_iter().map(raw_json_string)),
            ),
            ("totals".to_string(), raw_json_array(self.api_totals_json())),
            (
                "uncovered".to_string(),
                raw_json_array(self.uncovered_apis().into_iter().map(raw_json_string)),
            ),
            (
                "covered".to_string(),
                raw_json_array(self.covered_apis_json()),
            ),
        ]);
        let text = pretty_raw_json(&value);
        if let Some(parent) = Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed to create coverage JSON directory '{}': {err}",
                    parent.display()
                )
            })?;
        }
        fs::write(path, format!("{text}\n"))
            .map_err(|err| format!("failed to write coverage JSON '{path}': {err}"))
    }

    fn render_api_totals(&self, output: &mut String) {
        for (group, covered, total) in self.api_totals() {
            output.push_str(&format!("{group}: {covered}/{total}\n"));
        }
    }

    fn api_totals(&self) -> Vec<(String, usize, usize)> {
        let standard = standard_api_ids();
        let mut totals = BTreeMap::<String, (usize, usize)>::new();
        for api_id in &standard {
            let group = api_id.split('.').next().unwrap_or("other").to_string();
            let entry = totals.entry(group).or_default();
            entry.1 += 1;
            if self.api_hits.contains_key(api_id) {
                entry.0 += 1;
            }
        }
        totals
            .into_iter()
            .map(|(group, (covered, total))| (group, covered, total))
            .collect()
    }

    fn api_hits_json(&self) -> JsonValue {
        raw_json_object(self.api_hits.iter().map(|(api_id, hits)| {
            (
                api_id.clone(),
                raw_json_object([
                    ("tests".to_string(), raw_json_u64(hits.tests)),
                    ("examples".to_string(), raw_json_u64(hits.examples)),
                ]),
            )
        }))
    }

    fn api_totals_json(&self) -> Vec<JsonValue> {
        self.api_totals()
            .into_iter()
            .map(|(group, covered, total)| {
                raw_json_object([
                    ("group".to_string(), raw_json_string(group)),
                    ("covered".to_string(), raw_json_usize(covered)),
                    ("total".to_string(), raw_json_usize(total)),
                ])
            })
            .collect()
    }

    fn render_uncovered_apis(&self, output: &mut String) {
        let mut count = 0usize;
        for api_id in self.uncovered_apis() {
            output.push_str(&format!("  {api_id}\n"));
            count += 1;
            if count >= 80 {
                output.push_str("  ...\n");
                break;
            }
        }
        if count == 0 {
            output.push_str("  none\n");
        }
    }

    fn uncovered_apis(&self) -> Vec<String> {
        standard_api_ids()
            .into_iter()
            .filter(|api_id| !self.api_hits.contains_key(api_id))
            .collect()
    }

    fn render_covered_apis(&self, output: &mut String) {
        let mut count = 0usize;
        for (api_id, total) in self.covered_apis() {
            output.push_str(&format!("  {api_id}: {total}\n"));
            count += 1;
        }
        if count == 0 {
            output.push_str("  none\n");
        }
    }

    fn covered_apis(&self) -> Vec<(&str, u64)> {
        self.api_hits
            .iter()
            .filter_map(|(api_id, hits)| {
                let total = hits.tests + hits.examples;
                (total > 0).then_some((api_id.as_str(), total))
            })
            .collect()
    }

    fn covered_apis_json(&self) -> Vec<JsonValue> {
        self.api_hits
            .iter()
            .filter_map(|(api_id, hits)| {
                let total = hits.tests + hits.examples;
                (total > 0).then(|| {
                    raw_json_object([
                        ("api_id".to_string(), raw_json_string(api_id)),
                        ("tests".to_string(), raw_json_u64(hits.tests)),
                        ("examples".to_string(), raw_json_u64(hits.examples)),
                        ("total".to_string(), raw_json_u64(total)),
                    ])
                })
            })
            .collect()
    }
}

impl CoverageHits {
    fn add(&mut self, scope: &str) {
        match scope {
            "examples" => self.examples += 1,
            _ => self.tests += 1,
        }
    }
}

#[allow(clippy::single_call_fn)]
fn standard_api_ids() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for (module_name, module) in api_spec().module_entries() {
        for function in &module.functions {
            let function_name = function.name;
            ids.insert(format!("module.{module_name}.{function_name}"));
        }
    }
    for (receiver, methods) in api_spec().method_entries() {
        for method in methods {
            let method_name = method.name;
            ids.insert(format!(
                "method.{}.{}",
                coverage_receiver_name(receiver),
                method_name
            ));
        }
    }
    for core in ["print", "eprint", "cd", "env"] {
        ids.insert(format!("core.{core}"));
    }
    ids.insert("run".to_string());
    ids.insert("run.pipeline".to_string());
    for stage in COVERAGE_STREAM_STAGES {
        ids.insert(format!("stream.{stage}"));
    }
    ids
}

#[allow(clippy::single_call_fn)]
fn coverage_receiver_name(receiver: MethodReceiver) -> &'static str {
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

const COVERAGE_STREAM_STAGES: &[&str] = &[
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
];
