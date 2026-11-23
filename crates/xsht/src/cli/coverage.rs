use xsh::host::json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use xsh::api::{MethodReceiver, api_spec};
use xsh::frontend::load::parse_script_with_module_roots;
use xsh::frontend::source::{SourceId, SourceMap, Span};
use xsh::frontend::syntax::arena::{
    ArenaExprKind, ArenaPatternKind, ArenaProgram, ArenaStmtKind, ArenaSugar, ArenaSugarOperand, BlockId, ExprId, FunctionDefId,
    PatternId, StmtId,
};
use xsh::frontend::syntax::node::BinaryOp;


#[derive(Clone, Debug, Default)]
pub struct CoverageCollector {
    include_api: bool,
    exclude: Vec<String>,
    api_hits: BTreeMap<String, CoverageHits>,
    source_hits: BTreeMap<String, SourceCoverage>,
    source_metadata: BTreeMap<String, SourceMetadata>,
    observed_source_files: BTreeSet<String>,
    root: Option<PathBuf>,
    test_status: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct CoverageHits {
    tests: BTreeSet<String>,
    events: u64,
}

#[derive(Clone, Debug, Default)]
struct SourceCoverage {
    covered_lines: BTreeSet<usize>,
    line_tests: BTreeMap<usize, BTreeSet<String>>,
    proc_hits: BTreeMap<usize, u64>,
    proc_name_hits: BTreeMap<String, u64>,
    branches: BTreeMap<(usize, usize, usize, String), BTreeMap<String, u64>>,
}

#[derive(Clone, Debug, Default)]
struct SourceMetadata {
    executable_lines: BTreeSet<usize>,
    proc_decls: BTreeMap<String, usize>,
    lines_basis: bool,
    branches: BTreeMap<(usize, usize, usize, String), BTreeMap<String, u64>>,
    match_aliases: BTreeMap<(usize, usize), BTreeMap<String, String>>,
    if_condition_lines: BTreeMap<(usize, usize), Vec<usize>>,
}

#[derive(Clone, Debug)]
struct SourceFileCoverage {
    file: String,
    executable_lines: usize,
    covered_lines: usize,
    total_procs: usize,
    covered_procs: usize,
    basis: String,
    category: String,
    executable_set: BTreeSet<usize>,
    line_tests: BTreeMap<usize, BTreeSet<String>>,
    branches: BTreeMap<(usize, usize, usize, String), BTreeMap<String, u64>>,
}

impl CoverageCollector {
    // These constructors are for callers that do not need API or exclusion configuration.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_api(false)
    }

    // This constructor lets callers opt into API coverage without exclusions.
    #[allow(dead_code)]
    pub fn with_api(include_api: bool) -> Self {
        Self::with_api_and_excludes(include_api, &[])
    }

    pub fn with_api_and_excludes(include_api: bool, exclude: &[String]) -> Self {
        Self {
            include_api,
            exclude: exclude.to_vec(),
            root: std::env::current_dir().ok(),
            ..Self::default()
        }
    }

    pub fn register_source_files(&mut self, files: &[PathBuf]) {
        for file in files {
            let file = absolute_path(file);
            let file_text = match fs::read_to_string(&file) {
                Ok(text) => text,
                Err(_) => continue,
            };
            if !self.include_source_file(&file.to_string_lossy()) {
                continue;
            }

            let fallback = SourceMetadata {
                executable_lines: executable_line_numbers(&file_text),
                proc_decls: proc_decl_lines(&file_text),
                lines_basis: true,
                branches: BTreeMap::new(),
                match_aliases: BTreeMap::new(),
                if_condition_lines: BTreeMap::new(),
            };
            // Each file loads with its own project module roots, as it
            // does when it runs. Metadata is best effort: a file that does
            // not load keeps the line-based fallback.
            let module_roots = xsh::frontend::load::project_module_roots(&file).unwrap_or_default();
            let metadata = parse_source_metadata(&file, &module_roots).unwrap_or_else(|| {
                BTreeMap::from([(file.to_string_lossy().into_owned(), fallback)])
            });
            for (source_file, metadata) in metadata {
                if self.include_source_file(&source_file) {
                    let display = self.display_source_file(&source_file);
                    self.source_metadata
                        .entry(display.clone())
                        .or_insert(metadata);
                    self.source_hits.entry(display).or_default();
                }
            }
        }
    }

    pub fn register_test(&mut self, id: &str) {
        self.test_status.insert(id.to_string(), "missing".to_string());
    }

    pub fn ingest_test(&mut self, id: &str, trace: Option<&str>) -> Result<(), String> {
        let Some(trace) = trace else {
            self.register_test(id);
            return Ok(());
        };
        if trace.is_empty() {
            self.register_test(id);
            return Ok(());
        }
        self.test_status.insert(id.to_string(), "incomplete".to_string());
        if !trace.ends_with('\n') {
            return Err("truncated coverage JSONL".to_string());
        }
        self.ingest_test_jsonl(id, trace)?;
        self.test_status.insert(id.to_string(), "complete".to_string());
        Ok(())
    }

    #[cfg(test)]
    fn ingest_jsonl(&mut self, jsonl: &str) -> Result<(), String> {
        self.ingest_test_jsonl("test", jsonl)
    }

    fn ingest_test_jsonl(&mut self, id: &str, jsonl: &str) -> Result<(), String> {
        let has_statements = jsonl.lines().any(|line| {
            xsh::host::json::parse(line).ok().is_some_and(|value| {
                value.get("kind").and_then(|value| value.as_str()) == Some("coverage.statement")
            })
        });
        for (index, line) in jsonl.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value = xsh::host::json::parse(line)
                .map_err(|err| format!("failed to parse trace JSONL line {}: {err}", index + 1))?;
            let kind = value.get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if kind == "source.file" {
                self.ingest_source_file(&value);
                continue;
            }
            if kind == "coverage.branch" {
                self.ingest_branch(id, &value);
                continue;
            }
            if kind == "coverage.statement" && value.get("count").and_then(|value| value.as_u64()) == Some(0) {
                continue;
            }
            if self.include_api
                && (kind.ends_with(".call")
                    || kind.ends_with(".start")
                    || kind.ends_with(".enter")
                    || kind == "cwd.enter")
                && let Some(api_id) = value.get("api_id").and_then(|value| value.as_str())
            {
                let hits = self.api_hits.entry(api_id.to_string()).or_default();
                hits.tests.insert(id.to_string());
                hits.events += 1;
            }
            if let Some(span) = value.get("source_span") {
                if !has_statements || kind == "coverage.statement" {
                    self.ingest_source_span(
                        id,
                        kind,
                        value.get("name").and_then(|value| value.as_str()),
                        span,
                        value.get("definition_span").is_none(),
                    );
                } else if (kind == "proc.enter" || kind == "pure.enter") && value.get("definition_span").is_none() {
                    self.ingest_proc_definition(value.get("name").and_then(|value| value.as_str()), span);
                }
            }
            if (kind == "proc.enter" || kind == "pure.enter")
                && let Some(span) = value.get("definition_span")
            {
                self.ingest_proc_definition(
                    value.get("name").and_then(|value| value.as_str()),
                    span,
                );
            }
        }
        Ok(())
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("coverage report\n");
        for status in ["complete", "incomplete", "missing"] {
            output.push_str(&format!("coverage traces {status}: {}\n", self.test_status.values().filter(|value| value.as_str() == status).count()));
        }
        output.push_str("Source coverage\n");
        self.render_source_totals(&mut output);
        output.push('\n');
        output.push_str("least covered source files\n");
        self.render_least_covered_source_files(&mut output);
        output.push_str("\nunavailable source files\n");
        let unavailable: Vec<_> = self.source_hits.keys().filter(|file| self.read_source_text(file).is_none()).collect();
        if unavailable.is_empty() { output.push_str("  none\n"); }
        for file in unavailable { output.push_str(&format!("  {file} (unreadable)\n")); }
        if self.include_api {
            output.push('\n');
            output.push_str("API coverage\n");
            self.render_api_totals(&mut output);
            output.push('\n');
            output.push_str("uncovered standard APIs\n");
            self.render_uncovered_apis(&mut output);
            output.push('\n');
            output.push_str("APIs covered by tests\n");
            self.render_covered_apis(&mut output);
        }
        output
    }

    pub fn write_json(&self, path: &str) -> Result<(), String> {
        let source_files = self.source_file_coverages();
        let mut fields = vec![
            (
                "source_coverage".to_string(),
                (source_files.iter().cloned().map(source_file_json)).collect::<JsonValue>(),
            ),
            (
                "source_scope".to_string(),
                ([
                    ("files".to_string(), JsonValue::from(source_files.len())),
                    (
                        "observed_files".to_string(),
                        JsonValue::from(self.observed_source_files.len()),
                    ),
                ]).into_iter().collect::<std::collections::BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>(),
            ),
            ("test_collection".to_string(), self.test_status.iter().map(|(id, status)| (id.clone(), JsonValue::from(status.as_str()))).collect::<BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()),
            ("source_totals_by_basis".to_string(), ["ast", "lines"].into_iter().map(|basis| {
                let files: Vec<_> = source_files.iter().filter(|file| file.basis == basis).collect();
                let counts = [
                    ("covered_lines".to_string(), JsonValue::from(files.iter().map(|file| file.covered_lines).sum::<usize>())),
                    ("executable_lines".to_string(), JsonValue::from(files.iter().map(|file| file.executable_lines).sum::<usize>())),
                    ("covered_procs".to_string(), JsonValue::from(files.iter().map(|file| file.covered_procs).sum::<usize>())),
                    ("total_procs".to_string(), JsonValue::from(files.iter().map(|file| file.total_procs).sum::<usize>())),
                    ("fused_branch_points".to_string(), JsonValue::from(files.iter().flat_map(|file| file.branches.values()).filter(|arms| arms.get("fused").is_some_and(|count| *count > 0)).count())),
                ];
                (basis.to_string(), counts.into_iter().collect::<BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>())
            }).collect::<BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()),
            ("unavailable_sources".to_string(), self.source_hits.iter().filter(|(file, _)| self.read_source_text(file).is_none()).map(|(file, hits)| {
                [("file".to_string(), JsonValue::from(file.as_str())), ("category".to_string(), JsonValue::from("unreadable")), ("line_tests".to_string(), line_tests_json(&hits.line_tests))].into_iter().collect::<BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()
            }).collect::<JsonValue>()),
        ];
        if self.include_api {
            fields.extend([
                ("api_hits".to_string(), self.api_hits_json()),
                (
                    "standard_apis".to_string(),
                    (standard_api_ids().into_iter().map(JsonValue::from)).collect::<JsonValue>(),
                ),
                ("totals".to_string(), (self.api_totals_json()).into_iter().collect::<JsonValue>()),
                (
                    "uncovered".to_string(),
                    (self.uncovered_apis().into_iter().map(JsonValue::from)).collect::<JsonValue>(),
                ),
                (
                    "covered".to_string(),
                    (self.covered_apis_json()).into_iter().collect::<JsonValue>(),
                ),
            ]);
        }
        let value = (fields).into_iter().collect::<std::collections::BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>();
        let text = xsh::host::json::encode_pretty(&value).expect("host JSON document must be encodable");
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

    fn ingest_source_file(&mut self, value: &JsonValue) {
        let Some(file) = (value).get("file").and_then(|value| value.as_str()) else {
            return;
        };
        if self.include_source_file(file) {
            let display = self.display_source_file(file);
            self.observed_source_files.insert(display.clone());
            self.source_hits.entry(display.clone()).or_default();
            if !self.source_metadata.contains_key(&display) {
                let path = absolute_path(Path::new(file));
                let roots = xsh::frontend::load::project_module_roots(&path).unwrap_or_default();
                if let Some(metadata) = parse_source_metadata(&path, &roots) {
                    for (source, data) in metadata {
                        if self.include_source_file(&source) {
                            self.source_metadata.entry(self.display_source_file(&source)).or_insert(data);
                        }
                    }
                }
            }
        }
    }

    fn ingest_branch(&mut self, id: &str, value: &JsonValue) {
        let Some(span) = value.get("source_span") else { return; };
        let Some(file) = span.get("file").and_then(|value| value.as_str()) else { return; };
        if !self.include_source_file(file) { return; }
        let Some(start) = span.get("start_offset").and_then(|value| value.as_u64()).and_then(|value| usize::try_from(value).ok()) else { return; };
        let Some(end) = span.get("end_offset").and_then(|value| value.as_u64()).and_then(|value| usize::try_from(value).ok()) else { return; };
        let Some(line) = span.get("start_line").and_then(|value| value.as_u64()).and_then(|value| usize::try_from(value).ok()) else { return; };
        let Some((kind, arm)) = value.get("outcome").and_then(|value| value.as_str()).and_then(|value| value.split_once(':')) else { return; };
        let Some(count) = value.get("count").and_then(|value| value.as_u64()) else { return; };
        let display = self.display_source_file(file);
        if kind == "fused" && arm == "span" && count > 0 {
            if let Some(metadata) = self.source_metadata.get(&display) {
                let end_line = span.get("end_line").and_then(|value| value.as_u64()).and_then(|value| usize::try_from(value).ok()).unwrap_or(line);
                let hits = self.source_hits.entry(display).or_default();
                for covered in metadata.executable_lines.range(line..=end_line) {
                    hits.covered_lines.insert(*covered);
                    hits.line_tests.entry(*covered).or_default().insert(id.to_string());
                }
                for (point, _) in metadata.branches.range((start, 0, 0, String::new())..) {
                    if point.0 > end { break; }
                    if point.1 <= end {
                        *hits.branches.entry(point.clone()).or_default().entry("fused".to_string()).or_default() += count;
                    }
                }
            }
            return;
        }
        if kind == "if" && count > 0
            && let Some(lines) = self.source_metadata.get(&display).and_then(|metadata| metadata.if_condition_lines.get(&(start, end)))
        {
            let evaluated = if arm == "else" { lines.len() } else {
                arm.strip_prefix("arm:").and_then(|value| value.parse::<usize>().ok()).map_or(0, |index| index.saturating_add(1))
            };
            let entry = self.source_hits.entry(display.clone()).or_default();
            for line in lines.iter().take(evaluated) {
                entry.covered_lines.insert(*line);
                entry.line_tests.entry(*line).or_default().insert(id.to_string());
            }
        }
        *self.source_hits.entry(display).or_default().branches
            .entry((start, end, line, kind.to_string())).or_default().entry(arm.to_string()).or_default() += count;
    }

    fn ingest_source_span(
        &mut self,
        id: &str,
        kind: &str,
        name: Option<&str>,
        span: &JsonValue,
        count_proc: bool,
    ) {
        let Some(file) = (span).get("file").and_then(|value| value.as_str()) else {
            return;
        };
        if !self.include_source_file(file) {
            return;
        }
        let Some(start_line) = (span).get("start_line")
            .and_then(|value| value.as_u64())
            .and_then(|line| usize::try_from(line).ok())
        else {
            return;
        };
        let end_line = (span).get("end_line")
            .and_then(|value| value.as_u64())
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or(start_line);
        let end_line = if kind == "coverage.statement" { start_line } else { end_line };
        let entry = self
            .source_hits
            .entry(self.display_source_file(file))
            .or_default();
        for line in start_line..=end_line.max(start_line) {
            entry.covered_lines.insert(line);
            entry.line_tests.entry(line).or_default().insert(id.to_string());
        }
        if count_proc && (kind == "proc.enter" || kind == "pure.enter") {
            *entry.proc_hits.entry(start_line).or_default() += 1;
            if let Some(name) = name {
                *entry.proc_name_hits.entry(name.to_string()).or_default() += 1;
                if let Some(tail) = name.rsplit('.').next()
                    && tail != name
                {
                    *entry.proc_name_hits.entry(tail.to_string()).or_default() += 1;
                }
            }
        }
    }

    fn ingest_proc_definition(&mut self, name: Option<&str>, span: &JsonValue) {
        let Some(file) = (span).get("file").and_then(|value| value.as_str()) else {
            return;
        };
        if !self.include_source_file(file) {
            return;
        }
        let Some(start_line) = (span).get("start_line")
            .and_then(|value| value.as_u64())
            .and_then(|line| usize::try_from(line).ok())
        else {
            return;
        };
        let entry = self
            .source_hits
            .entry(self.display_source_file(file))
            .or_default();
        *entry.proc_hits.entry(start_line).or_default() += 1;
        if let Some(name) = name {
            *entry.proc_name_hits.entry(name.to_string()).or_default() += 1;
            if let Some(tail) = name.rsplit('.').next()
                && tail != name
            {
                *entry.proc_name_hits.entry(tail.to_string()).or_default() += 1;
            }
        }
    }

    fn include_source_file(&self, file: &str) -> bool {
        if file.starts_with('<') && file.ends_with('>') { return false; }
        if !file.ends_with(".xsh") {
            match fs::read_to_string(file) {
                Ok(text) if !text.lines().next().is_some_and(|line| {
                    line.starts_with("#!") && line.split_whitespace().any(|word| word.rsplit('/').next() == Some("xsh"))
                }) => return false,
                _ => {}
            }
        }
        let path = Path::new(file);
        if let Some(root) = &self.root
            && super::is_path_excluded(root, path, &self.exclude)
        {
            return false;
        }
        true
    }

    fn display_source_file(&self, file: &str) -> String {
        let path = Path::new(file);
        if let Some(root) = &self.root
            && let Ok(rel) = path.strip_prefix(root)
        {
            return rel.to_string_lossy().to_string();
        }
        file.to_string()
    }

    fn source_file_coverages(&self) -> Vec<SourceFileCoverage> {
        let mut files = Vec::new();
        let source_files = self
            .source_hits
            .keys()
            .chain(self.source_metadata.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        for file in source_files {
            let hits = self.source_hits.get(&file).cloned().unwrap_or_default();
            let Some(text) = self.read_source_text(&file) else {
                continue;
            };
            let fallback = SourceMetadata {
                executable_lines: executable_line_numbers(&text),
                proc_decls: proc_decl_lines(&text),
                lines_basis: true,
                branches: BTreeMap::new(),
                match_aliases: BTreeMap::new(),
                if_condition_lines: BTreeMap::new(),
            };
            let metadata = self.source_metadata.get(&file).unwrap_or(&fallback);
            let covered_lines = hits
                .covered_lines
                .iter()
                .filter(|line| metadata.executable_lines.contains(line))
                .count();
            let covered_procs = metadata
                .proc_decls
                .iter()
                .filter(|(name, line)| {
                    hits.proc_hits.contains_key(line) || hits.proc_name_hits.contains_key(*name)
                })
                .count();
            let mut branches = metadata.branches.clone();
            for (point, arms) in hits.branches {
                let aliases = metadata.match_aliases.get(&(point.0, point.1));
                let target = branches.entry(point).or_default();
                for (arm, count) in arms {
                    let arm = aliases.and_then(|aliases| aliases.get(&arm)).cloned().unwrap_or(arm);
                    *target.entry(arm).or_default() += count;
                }
            }
            files.push(SourceFileCoverage {
                file: file.clone(),
                executable_lines: metadata.executable_lines.len(),
                covered_lines,
                total_procs: metadata.proc_decls.len(),
                covered_procs,
                basis: if metadata.lines_basis { "lines" } else { "ast" }.to_string(),
                category: if Path::new(&file).is_absolute() { "outside_root" } else { "root" }.to_string(),
                executable_set: metadata.executable_lines.clone(),
                line_tests: hits.line_tests,
                branches,
            });
        }
        files.sort_by(|a, b| a.file.cmp(&b.file));
        files
    }

    fn read_source_text(&self, file: &str) -> Option<String> {
        let path = Path::new(file);
        if path.is_absolute() {
            return fs::read_to_string(path).ok();
        }
        self.root
            .as_ref()
            .and_then(|root| fs::read_to_string(root.join(path)).ok())
    }

    fn render_source_totals(&self, output: &mut String) {
        let files = self.source_file_coverages();
        let executable_lines: usize = files.iter().map(|file| file.executable_lines).sum();
        let covered_lines: usize = files.iter().map(|file| file.covered_lines).sum();
        let total_procs: usize = files.iter().map(|file| file.total_procs).sum();
        let covered_procs: usize = files.iter().map(|file| file.covered_procs).sum();
        output.push_str(&format!(
            "scope: {} source files ({} observed)\n",
            files.len(),
            self.observed_source_files.len()
        ));
        for basis in ["ast", "lines"] {
            let selected: Vec<_> = files.iter().filter(|file| file.basis == basis).collect();
            let total: usize = selected.iter().map(|file| file.executable_lines).sum();
            let covered: usize = selected.iter().map(|file| file.covered_lines).sum();
            output.push_str(&format!("source lines ({basis}): {covered}/{total} ({})\n", percent_text(covered, total)));
            let total_procs: usize = selected.iter().map(|file| file.total_procs).sum();
            let covered_procs: usize = selected.iter().map(|file| file.covered_procs).sum();
            output.push_str(&format!("proc entries ({basis}): {covered_procs}/{total_procs} ({})\n", percent_text(covered_procs, total_procs)));
        }
        if files.iter().all(|file| file.basis == "lines") || files.iter().all(|file| file.basis == "ast") {
            output.push_str(&format!("source lines: {covered_lines}/{executable_lines} ({})\n", percent_text(covered_lines, executable_lines)));
        }
        if files.iter().all(|file| file.basis == "lines") || files.iter().all(|file| file.basis == "ast") {
            output.push_str(&format!("proc entries: {covered_procs}/{total_procs} ({})\n", percent_text(covered_procs, total_procs)));
        }
    }

    fn render_least_covered_source_files(&self, output: &mut String) {
        let mut files = self.source_file_coverages();
        files.sort_by(|a, b| {
            coverage_basis_points(a.covered_lines, a.executable_lines)
                .cmp(&coverage_basis_points(b.covered_lines, b.executable_lines))
                .then_with(|| b.executable_lines.cmp(&a.executable_lines))
                .then_with(|| a.file.cmp(&b.file))
        });
        let mut count = 0usize;
        for file in files {
            if file.executable_lines == 0 {
                continue;
            }
            output.push_str(&format!(
                "  {:<32} lines {:>6} procs {:>6}\n",
                truncate_middle(&format!("{}{}", file.file, if file.category == "outside_root" { " (outside_root)" } else { "" }), 32),
                percent_text(file.covered_lines, file.executable_lines),
                percent_text(file.covered_procs, file.total_procs),
            ));
            count += 1;
            if count >= 20 {
                break;
            }
        }
        if count == 0 {
            output.push_str("  none\n");
        }
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
        (self.api_hits.iter().map(|(api_id, hits)| {
            (
                api_id.clone(),
                ([("tests".to_string(), JsonValue::from(hits.tests.len())), ("events".to_string(), JsonValue::from(hits.events))]).into_iter().collect::<std::collections::BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>(),
            )
        })).collect::<std::collections::BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()
    }

    fn api_totals_json(&self) -> Vec<JsonValue> {
        self.api_totals()
            .into_iter()
            .map(|(group, covered, total)| {
                ([
                    ("group".to_string(), JsonValue::from(group)),
                    ("covered".to_string(), JsonValue::from(covered)),
                    ("total".to_string(), JsonValue::from(total)),
                ]).into_iter().collect::<std::collections::BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()
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
                let total = hits.tests.len() as u64;
                (total > 0).then_some((api_id.as_str(), total))
            })
            .collect()
    }

    fn covered_apis_json(&self) -> Vec<JsonValue> {
        self.api_hits
            .iter()
            .filter_map(|(api_id, hits)| {
                let total = hits.tests.len() as u64;
                (total > 0).then(|| {
                    ([
                        ("api_id".to_string(), JsonValue::from(api_id)),
                        ("tests".to_string(), JsonValue::from(hits.tests.len())),
                        ("total".to_string(), JsonValue::from(total)),
                    ]).into_iter().collect::<std::collections::BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()
                })
            })
            .collect()
    }
}

fn source_file_json(file: SourceFileCoverage) -> JsonValue {
    let branch_total = file.branches.values().map(|arms| arms.keys().filter(|arm| arm.as_str() != "fused").count()).sum::<usize>();
    let branch_covered = file.branches.values().flat_map(|arms| arms.iter()).filter(|(arm, count)| arm.as_str() != "fused" && **count > 0).count();
    let fused_branch_points = file.branches.values().filter(|arms| arms.get("fused").is_some_and(|count| *count > 0)).count();
    ([
        ("file".to_string(), JsonValue::from(file.file)),
        ("denominator_basis".to_string(), JsonValue::from(file.basis)),
        ("category".to_string(), JsonValue::from(file.category)),
        ("executable_line_numbers".to_string(), file.executable_set.iter().copied().map(JsonValue::from).collect::<JsonValue>()),
        ("line_tests".to_string(), line_tests_json(&file.line_tests)),
        ("branches".to_string(), file.branches.into_iter().map(|((start, end, line, kind), arms)| {
            [("start_offset".to_string(), JsonValue::from(start)), ("end_offset".to_string(), JsonValue::from(end)),
             ("line".to_string(), JsonValue::from(line)), ("kind".to_string(), JsonValue::from(kind)),
             ("arms".to_string(), arms.into_iter().map(|(arm, count)| (arm, JsonValue::from(count))).collect::<BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>())]
                .into_iter().collect::<BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()
        }).collect::<JsonValue>()),
        ("fused_branch_points".to_string(), JsonValue::from(fused_branch_points)),
        ("total_branch_arms".to_string(), JsonValue::from(branch_total)),
        ("covered_branch_arms".to_string(), JsonValue::from(branch_covered)),
        (
            "executable_lines".to_string(),
            JsonValue::from(file.executable_lines),
        ),
        (
            "covered_lines".to_string(),
            JsonValue::from(file.covered_lines),
        ),
        ("total_procs".to_string(), JsonValue::from(file.total_procs)),
        (
            "covered_procs".to_string(),
            JsonValue::from(file.covered_procs),
        ),
    ]).into_iter().collect::<std::collections::BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()
}

fn line_tests_json(lines: &BTreeMap<usize, BTreeSet<String>>) -> JsonValue {
    lines.iter().map(|(line, tests)| (line.to_string(), tests.iter().map(|id| JsonValue::from(id.as_str())).collect::<JsonValue>())).collect::<BTreeMap<String, JsonValue>>().into_iter().collect::<JsonValue>()
}

fn absolute_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

fn parse_source_metadata(
    path: &Path,
    module_roots: &[PathBuf],
) -> Option<BTreeMap<String, SourceMetadata>> {
    let path_text = path.to_string_lossy().into_owned();
    let (sources, parsed) = parse_script_with_module_roots(&path_text, module_roots).ok()?;
    if !parsed.diagnostics.is_empty() { return None; }
    let mut by_source = BTreeMap::<SourceId, SourceMetadata>::new();
    for file in sources.files() {
        by_source.insert(file.id(), SourceMetadata::default());
    }

    let rows = parsed.arena.arena.source_rows();
    for file in sources.files() {
        for statement in rows.stmt_ids(file.id()).filter(|id| !parsed.arena.arena.stmt_is_synthetic(*id)) {
            let node = parsed.arena.arena.stmt(statement);
            match node.kind {
                ArenaStmtKind::If { branches, .. } => {
                    let mut arms = (0..parsed.arena.arena.if_branches(branches).len()).map(|index| format!("arm:{index}")).collect::<Vec<_>>();
                    arms.push("else".to_string());
                    register_branch(&sources, &mut by_source, node.span, "if", arms);
                    let lines = parsed.arena.arena.if_branches(branches).iter().filter_map(|branch| {
                        let span = parsed.arena.arena.expr(branch.condition).span;
                        sources.location(span.source_id, span.start()).map(|location| location.line)
                    }).collect();
                    if let Some(source) = by_source.get_mut(&node.span.source_id) {
                        source.if_condition_lines.insert((node.span.start(), node.span.end()), lines);
                    }
                }
                ArenaStmtKind::Match { arms, .. } => {
                    register_branch(&sources, &mut by_source, node.span, "match", (0..parsed.arena.arena.match_arms(arms).len()).map(|index| format!("arm:{index}")));
                    register_match_aliases(&parsed.arena, &mut by_source, node.span, parsed.arena.arena.match_arms(arms).iter().map(|arm| arm.pattern));
                }
                ArenaStmtKind::While { condition, .. } => {
                    register_branch(&sources, &mut by_source, parsed.arena.arena.expr(condition).span, "loop", ["false".to_string(), "true".to_string()]);
                }
                ArenaStmtKind::For { .. } => {
                    register_branch(&sources, &mut by_source, node.span, "loop", ["false".to_string(), "true".to_string()]);
                }
                _ => {}
            }
        }
        for expression in rows.expr_ids(file.id()).filter(|id| !parsed.arena.arena.expr_is_synthetic(*id)) {
            let node = parsed.arena.arena.expr(expression);
            match node.kind {
                ArenaExprKind::Binary { op: BinaryOp::And | BinaryOp::Or, left, right } => {
                    for operand in [left, right] {
                        register_branch(&sources, &mut by_source, parsed.arena.arena.expr(operand).span, "bool", ["false".to_string(), "true".to_string()]);
                    }
                }
                ArenaExprKind::If { branches, .. } => {
                    let mut arms = (0..parsed.arena.arena.if_expr_branches(branches).len()).map(|index| format!("arm:{index}")).collect::<Vec<_>>();
                    arms.push("else".to_string());
                    register_branch(&sources, &mut by_source, node.span, "if", arms);
                    let lines = parsed.arena.arena.if_expr_branches(branches).iter().filter_map(|branch| {
                        let span = parsed.arena.arena.expr(branch.condition).span;
                        sources.location(span.source_id, span.start()).map(|location| location.line)
                    }).collect();
                    if let Some(source) = by_source.get_mut(&node.span.source_id) {
                        source.if_condition_lines.insert((node.span.start(), node.span.end()), lines);
                    }
                }
                ArenaExprKind::Match { arms, .. } => {
                    register_branch(&sources, &mut by_source, node.span, "match", (0..parsed.arena.arena.match_expr_arms(arms).len()).map(|index| format!("arm:{index}")));
                    register_match_aliases(&parsed.arena, &mut by_source, node.span, parsed.arena.arena.match_expr_arms(arms).iter().map(|arm| arm.pattern));
                }
                ArenaExprKind::Try(_) => {
                    register_branch(&sources, &mut by_source, node.span, "try", ["continue".to_string(), "exit".to_string()]);
                }
                _ => {}
            }
        }
    }

    collect_statements(
        &parsed.arena,
        &sources,
        parsed.arena.statement_ids(),
        &mut by_source,
    );
    for module in &parsed.arena.modules {
        collect_statements(
            &parsed.arena,
            &sources,
            parsed.arena.module_statements(module),
            &mut by_source,
        );
    }

    Some(
        sources
            .files()
            .iter()
            .filter_map(|file| {
                by_source
                    .remove(&file.id())
                    .map(|metadata| (file.name().to_string(), metadata))
            })
            .collect(),
    )
}

fn register_match_aliases(program: &ArenaProgram, by_source: &mut BTreeMap<SourceId, SourceMetadata>, span: Span, patterns: impl IntoIterator<Item = PatternId>) {
    let Some(source) = by_source.get_mut(&span.source_id) else { return; };
    let aliases = source.match_aliases.entry((span.start(), span.end())).or_default();
    for (index, pattern) in patterns.into_iter().enumerate() {
        for key in match_pattern_aliases(program, pattern) {
            aliases.entry(key).or_insert_with(|| format!("arm:{index}"));
        }
    }
}

fn match_pattern_aliases(program: &ArenaProgram, pattern: PatternId) -> Vec<String> {
    match program.arena.pattern(pattern).kind {
        ArenaPatternKind::Wildcard => vec!["fallback".to_string()],
        ArenaPatternKind::Literal(expr) => match program.arena.expr(expr).kind {
            ArenaExprKind::Str(value) => vec![format!("key:{}", program.arena.string_literal(value))],
            _ => Vec::new(),
        },
        ArenaPatternKind::Constructor { name, arg: None } => vec![format!("key:{}", name.as_str().trim_start_matches('.'))],
        ArenaPatternKind::Alternation(items) => program.arena.pattern_ids(items).flat_map(|pattern| match_pattern_aliases(program, pattern)).collect(),
        _ => Vec::new(),
    }
}

fn register_branch(
    sources: &SourceMap,
    by_source: &mut BTreeMap<SourceId, SourceMetadata>,
    span: Span,
    kind: &str,
    arms: impl IntoIterator<Item = String>,
) {
    let Some(line) = sources.location(span.source_id, span.start()).map(|location| location.line) else { return; };
    let Some(source) = by_source.get_mut(&span.source_id) else { return; };
    let entry = source.branches.entry((span.start(), span.end(), line, kind.to_string())).or_default();
    for arm in arms { entry.entry(arm).or_default(); }
}

fn collect_statements(
    program: &ArenaProgram,
    sources: &SourceMap,
    statements: impl IntoIterator<Item = StmtId>,
    by_source: &mut BTreeMap<SourceId, SourceMetadata>,
) {
    for statement in statements {
        collect_statement(program, sources, statement, by_source);
    }
}

fn collect_statement(
    program: &ArenaProgram,
    sources: &SourceMap,
    statement: StmtId,
    by_source: &mut BTreeMap<SourceId, SourceMetadata>,
) {
    let statement = program.arena.stmt(statement);
    match statement.kind {
        ArenaStmtKind::Use(_) | ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_) => {}
        ArenaStmtKind::Export(inner) => collect_statement(program, sources, inner, by_source),
        ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) | ArenaStmtKind::PureDef(def) => {
            add_proc_definition(program, sources, statement.span, def, by_source);
        }
        ArenaStmtKind::StreamDef(def) => {
            collect_block(
                program,
                sources,
                program.arena.function_def(def).body,
                by_source,
            );
        }
        ArenaStmtKind::SignalHook(hook) => {
            collect_block(
                program,
                sources,
                program.arena.signal_hook(hook).body,
                by_source,
            );
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            for branch in program.arena.if_branches(branches) {
                add_expr(program, sources, branch.condition, by_source);
                collect_block(program, sources, branch.block, by_source);
            }
            if let Some(block) = else_block {
                collect_block(program, sources, block, by_source);
            }
        }
        ArenaStmtKind::While { condition, block }
        | ArenaStmtKind::For {
            iter: condition,
            block,
            ..
        } => {
            add_expr(program, sources, condition, by_source);
            collect_block(program, sources, block, by_source);
        }
        ArenaStmtKind::Loop { block } => collect_block(program, sources, block, by_source),
        // A `guard` is counted as one statement, as a `guard let` is.
        ArenaStmtKind::Sugar { form, operands, .. }
            if let ArenaSugar::Guard { else_block, .. } = program.arena.sugar(form, operands) =>
        {
            add_span(sources, statement.span, by_source);
            collect_block(program, sources, else_block, by_source);
        }
        ArenaStmtKind::Sugar { operands, .. } => {
            for operand in program.arena.sugar_operands(operands) {
                match *operand {
                    ArenaSugarOperand::Expr(expr) => add_expr(program, sources, expr, by_source),
                    ArenaSugarOperand::Block(block) => {
                        collect_block(program, sources, block, by_source);
                    }
                    ArenaSugarOperand::Stmt(inner) => {
                        collect_statement(program, sources, inner, by_source);
                    }
                    ArenaSugarOperand::BindingTarget(_)
                    | ArenaSugarOperand::TypeExpr(_)
                    | ArenaSugarOperand::Name(_) => {}
                }
            }
        }
        ArenaStmtKind::Guard { else_block, .. } => {
            add_span(sources, statement.span, by_source);
            collect_block(program, sources, else_block, by_source);
        }
        ArenaStmtKind::Assert { condition, message } => {
            add_expr(program, sources, condition, by_source);
            if let Some(message) = message {
                add_expr(program, sources, message, by_source);
            }
        }
        ArenaStmtKind::Match { value, arms } => {
            add_expr(program, sources, value, by_source);
            for arm in program.arena.match_arms(arms) {
                if let Some(guard) = arm.guard {
                    add_expr(program, sources, guard, by_source);
                }
                collect_block(program, sources, arm.block, by_source);
            }
        }
        ArenaStmtKind::Let { .. }
        | ArenaStmtKind::Const { .. }
        | ArenaStmtKind::Var { .. }
        | ArenaStmtKind::Assign { .. }
        | ArenaStmtKind::Return(_)
        | ArenaStmtKind::YieldDelegate(_)
        | ArenaStmtKind::Exit(_)
        | ArenaStmtKind::Yield(_)
        | ArenaStmtKind::Defer(..)
        | ArenaStmtKind::Break { .. }
        | ArenaStmtKind::Continue
        | ArenaStmtKind::Command(_)
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Expr(_) => add_span(sources, statement.span, by_source),
    }
}

fn collect_block(
    program: &ArenaProgram,
    sources: &SourceMap,
    block: BlockId,
    by_source: &mut BTreeMap<SourceId, SourceMetadata>,
) {
    let statements = program
        .arena
        .stmt_ids(program.arena.block(block).statements);
    collect_statements(program, sources, statements, by_source);
}

fn add_proc_definition(
    program: &ArenaProgram,
    sources: &SourceMap,
    span: Span,
    def: FunctionDefId,
    by_source: &mut BTreeMap<SourceId, SourceMetadata>,
) {
    if let Some(metadata) = by_source.get_mut(&span.source_id) {
        metadata.proc_decls.insert(
            program.arena.function_def(def).name.to_string(),
            sources
                .location(span.source_id, span.start())
                .map_or(0, |location| location.line),
        );
    }
    collect_block(
        program,
        sources,
        program.arena.function_def(def).body,
        by_source,
    );
}

fn add_expr(
    program: &ArenaProgram,
    sources: &SourceMap,
    expr: ExprId,
    by_source: &mut BTreeMap<SourceId, SourceMetadata>,
) {
    add_span(sources, program.arena.expr(expr).span, by_source);
}

fn add_span(sources: &SourceMap, span: Span, by_source: &mut BTreeMap<SourceId, SourceMetadata>) {
    let Some(start) = sources.location(span.source_id, span.start()) else {
        return;
    };
    let Some(end) = sources.location(span.source_id, span.end()) else {
        return;
    };
    if let Some(metadata) = by_source.get_mut(&span.source_id) {
        metadata
            .executable_lines
            .extend(start.line..=end.line.max(start.line));
    }
}

fn executable_line_numbers(text: &str) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        lines.insert(index + 1);
    }
    lines
}

fn proc_decl_lines(text: &str) -> BTreeMap<String, usize> {
    let mut lines = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let Some(name) = proc_decl_name(line) else {
            continue;
        };
        lines.insert(name.to_string(), index + 1);
    }
    lines
}

fn proc_decl_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("export proc ")
        .or_else(|| trimmed.strip_prefix("export pure "))
        .or_else(|| trimmed.strip_prefix("proc "))
        .or_else(|| trimmed.strip_prefix("pure "))?;
    let end = rest
        .find(|ch: char| ch == '(' || ch.is_whitespace())
        .unwrap_or(rest.len());
    if end == 0 { None } else { Some(&rest[..end]) }
}

fn coverage_basis_points(covered: usize, total: usize) -> usize {
    if total == 0 {
        return 10_000;
    }
    covered.saturating_mul(10_000) / total
}

fn percent_text(covered: usize, total: usize) -> String {
    if total == 0 {
        return "n/a".to_string();
    }
    let tenths = covered.saturating_mul(1000) / total;
    format!("{}.{:01}%", tenths / 10, tenths % 10)
}

fn truncate_middle(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return chars.into_iter().take(max_chars).collect();
    }
    let left = (max_chars - 1) / 2;
    let right = max_chars - left - 1;
    let prefix: String = chars.iter().take(left).collect();
    let suffix: String = chars
        .iter()
        .skip(chars.len().saturating_sub(right))
        .collect();
    format!("{prefix}...{suffix}")
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
        MethodReceiver::NetJob => "NetJob",
        MethodReceiver::FsRoot => "FsRoot",
        MethodReceiver::NonEmpty => "NonEmpty",
        MethodReceiver::Set => "Set",
        MethodReceiver::RelPath => "RelPath",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_coverage_reports_line_and_proc_percentages() {
        let root = std::env::temp_dir().join(format!("xsh-source-coverage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create root");
        let script = root.join("pm.xsh");
        fs::write(
            &script,
            "proc covered() {\n  print \"hit\"\n}\n\nproc missed() {\n  print \"miss\"\n}\n",
        )
        .expect("write script");
        let mut collector = CoverageCollector {
            root: Some(root.clone()),
            ..CoverageCollector::default()
        };
        collector
            .ingest_jsonl(&format!(
                "{{\"kind\":\"proc.enter\",\"name\":\"covered\",\"source_span\":{{\"file\":\"{}\",\"start_line\":1,\"end_line\":1}}}}\n{{\"kind\":\"core.call\",\"api_id\":\"core.print\",\"source_span\":{{\"file\":\"{}\",\"start_line\":2,\"end_line\":2}}}}\n",
                script.display(),
                script.display(),
            ))
            .expect("ingest");

        let rendered = collector.render();
        assert!(
            rendered.contains("scope: 1 source files (0 observed)"),
            "{rendered}"
        );
        assert!(rendered.contains("source lines: 2/6 (33.3%)"), "{rendered}");
        assert!(rendered.contains("proc entries: 1/2 (50.0%)"), "{rendered}");
        assert!(rendered.contains("pm.xsh"), "{rendered}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_file_events_count_loaded_unhit_files() {
        let root =
            std::env::temp_dir().join(format!("xsh-source-coverage-loaded-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create root");
        let script = root.join("pm/local.xsh");
        fs::create_dir_all(script.parent().expect("script parent")).expect("create parent");
        fs::write(&script, "proc missed() {\n  print \"miss\"\n}\n").expect("write script");
        let mut collector = CoverageCollector {
            root: Some(root.clone()),
            ..CoverageCollector::default()
        };
        collector
            .ingest_jsonl(&format!(
                "{{\"kind\":\"source.file\",\"file\":\"{}\",\"line_count\":3}}\n",
                script.display(),
            ))
            .expect("ingest");

        let rendered = collector.render();
        assert!(
            rendered.contains("scope: 1 source files (1 observed)"),
            "{rendered}"
        );
        assert!(rendered.contains("source lines: 0/2 (0.0%)"), "{rendered}");
        assert!(rendered.contains("proc entries: 0/1 (0.0%)"), "{rendered}");
        assert!(rendered.contains("pm/local.xsh"), "{rendered}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn proc_entries_use_definition_span_when_call_site_is_a_test() {
        let root = std::env::temp_dir().join(format!(
            "xsh-source-coverage-definition-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tests")).expect("create test root");
        let source = root.join("pm.xsh");
        let caller = root.join("tests/caller.xsh");
        fs::write(&source, "proc covered() {\n  print \"hit\"\n}\n").expect("write source");
        fs::write(&caller, "covered()\n").expect("write caller");
        let mut collector = CoverageCollector {
            root: Some(root.clone()),
            ..CoverageCollector::default()
        };
        collector
            .ingest_jsonl(&format!(
                "{{\"kind\":\"source.file\",\"file\":\"{}\",\"line_count\":3}}\n{{\"kind\":\"proc.enter\",\"name\":\"covered\",\"source_span\":{{\"file\":\"{}\",\"start_line\":1,\"end_line\":1}},\"definition_span\":{{\"file\":\"{}\",\"start_line\":1,\"end_line\":1}}}}\n",
                source.display(),
                caller.display(),
                source.display(),
            ))
            .expect("ingest");

        let rendered = collector.render();
        assert!(
            rendered.contains("proc entries (ast): 1/1 (100.0%)"),
            "{rendered}"
        );
        assert!(rendered.contains("pm.xsh"), "{rendered}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registered_sources_use_ast_lines_and_include_unobserved_files() {
        let root =
            std::env::temp_dir().join(format!("xsh-source-coverage-ast-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create root");
        let covered = root.join("covered.xsh");
        let missed = root.join("missed.xsh");
        fs::write(
            &covered,
            "type Config = {\n  value: Int,\n}\n\nproc covered() {\n  print \"hit\"\n}\n",
        )
        .expect("write covered source");
        fs::write(&missed, "proc missed() {\n  print \"miss\"\n}\n").expect("write missed source");

        let mut collector = CoverageCollector {
            root: Some(root.clone()),
            ..CoverageCollector::default()
        };
        collector.register_source_files(&[covered.clone(), missed.clone()]);
        collector
            .ingest_jsonl(&format!(
                "{{\"kind\":\"source.file\",\"file\":\"{}\",\"line_count\":7}}\n{{\"kind\":\"core.call\",\"source_span\":{{\"file\":\"{}\",\"start_line\":6,\"end_line\":6}}}}\n",
                covered.display(),
                covered.display(),
            ))
            .expect("ingest");

        let files = collector.source_file_coverages();
        let covered_file = files
            .iter()
            .find(|file| file.file == "covered.xsh")
            .unwrap();
        let missed_file = files.iter().find(|file| file.file == "missed.xsh").unwrap();
        assert!(covered_file.executable_lines < 6);
        assert_eq!(covered_file.covered_lines, 1);
        assert!(missed_file.executable_lines < 4);
        assert_eq!(missed_file.covered_lines, 0);
        assert!(
            collector
                .render()
                .contains("scope: 2 source files (1 observed)")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn coverage_excludes_source_families_without_affecting_source_loading() {
        let root = std::env::temp_dir().join(format!(
            "xsh-source-coverage-exclude-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("evals")).expect("create evals");
        let kept = root.join("kept.xsh");
        let excluded = root.join("evals/task.xsh");
        fs::write(&kept, "proc kept() {\n  print \"kept\"\n}\n").expect("write kept");
        fs::write(&excluded, "proc excluded() {\n  print \"excluded\"\n}\n")
            .expect("write excluded");

        let excludes = vec!["evals/**/*.xsh".to_string()];
        let mut collector = CoverageCollector {
            root: Some(root.clone()),
            exclude: excludes,
            ..CoverageCollector::default()
        };
        collector.register_source_files(&[kept.clone(), excluded.clone()]);

        let files = collector.source_file_coverages();
        assert!(files.iter().any(|file| file.file == "kept.xsh"));
        assert!(!files.iter().any(|file| file.file == "evals/task.xsh"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn coverage_distinct_tests_matrix_and_collection_status() {
        let mut collector = CoverageCollector { include_api: true, root: None, ..CoverageCollector::default() };
        let trace = "{\"kind\":\"core.call\",\"api_id\":\"core.print\",\"source_span\":{\"file\":\"/missing.xsh\",\"start_line\":3,\"end_line\":3}}\n";
        collector.ingest_test("one", Some(&format!("{trace}{trace}"))).unwrap();
        collector.ingest_test("two", Some(trace)).unwrap();
        collector.ingest_test("missing", None).unwrap();
        assert!(collector.ingest_test("truncated", Some("{\"kind\":")).is_err());
        assert_eq!(collector.api_hits["core.print"].tests.len(), 2);
        assert_eq!(collector.api_hits["core.print"].events, 3);
        assert_eq!(collector.source_hits["/missing.xsh"].line_tests[&3], BTreeSet::from(["one".to_string(), "two".to_string()]));
        assert_eq!(collector.test_status["missing"], "missing");
        assert_eq!(collector.test_status["truncated"], "incomplete");
        assert_eq!(collector.test_status["one"], "complete");
    }

    #[test]
    fn coverage_shebang_and_fallback_basis() {
        let root = std::env::temp_dir().join(format!("xsh-cov-shebang-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let script = root.join("script");
        let broken = root.join("broken.xsh");
        fs::write(&script, "#!/usr/bin/env -S xsh --\nprint \"ok\"\n").unwrap();
        fs::write(&broken, "let =\n").unwrap();
        let mut collector = CoverageCollector { root: Some(root.clone()), ..CoverageCollector::default() };
        collector.register_source_files(&[script, broken]);
        let files = collector.source_file_coverages();
        assert_eq!(files.iter().find(|f| f.file == "script").unwrap().basis, "ast");
        assert_eq!(files.iter().find(|f| f.file == "broken.xsh").unwrap().basis, "lines");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn coverage_reports_outside_and_deleted_sources() {
        let root = std::env::temp_dir().join(format!("xsh-cov-outside-{}", std::process::id()));
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        let external = root.join("external.xsh");
        let deleted = project.join("gone.xsh");
        fs::write(&external, "print \"external\"\n").unwrap();
        fs::write(&deleted, "print \"gone\"\n").unwrap();
        let mut collector = CoverageCollector { root: Some(project.clone()), ..CoverageCollector::default() };
        collector.register_source_files(&[external.clone(), deleted.clone()]);
        let trace = format!("{{\"kind\":\"core.call\",\"source_span\":{{\"file\":\"{}\",\"start_line\":1}}}}\n", deleted.display());
        collector.ingest_test("case", Some(&trace)).unwrap();
        fs::remove_file(deleted).unwrap();
        let output = project.join("report.json");
        collector.write_json(output.to_str().unwrap()).unwrap();
        let report = xsh::host::json::parse(&fs::read_to_string(output).unwrap()).unwrap();
        let sources = report.get("source_coverage").unwrap().as_seq().unwrap();
        assert!(sources.iter().any(|v| v.get("category").unwrap().as_str() == Some("outside_root")));
        let unavailable = report.get("unavailable_sources").unwrap().as_seq().unwrap();
        assert_eq!(unavailable.len(), 1);
        assert_eq!(unavailable[0].get("file").unwrap().as_str(), Some("gone.xsh"));
        assert_eq!(unavailable[0].get("line_tests").unwrap().get("1").unwrap().as_seq().unwrap()[0].as_str(), Some("case"));
        let _ = fs::remove_dir_all(root);
    }

}
