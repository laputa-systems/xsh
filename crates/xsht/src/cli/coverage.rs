use miniserde::json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use xsh::api::{MethodReceiver, api_spec};
use xsh::frontend::load::parse_script_with_module_roots;
use xsh::frontend::source::{SourceId, SourceMap, Span};
use xsh::frontend::syntax::arena::{
    ArenaProgram, ArenaStmtKind, BlockId, ExprId, FunctionDefId, StmtId,
};
use xsh::host::json::{
    parse_raw_json, pretty_raw_json, raw_json_array, raw_json_as_str, raw_json_as_u64,
    raw_json_get, raw_json_object, raw_json_string, raw_json_u64, raw_json_usize,
};

#[derive(Clone, Debug, Default)]
pub struct CoverageCollector {
    include_api: bool,
    exclude: Vec<String>,
    api_hits: BTreeMap<String, CoverageHits>,
    source_hits: BTreeMap<String, SourceCoverage>,
    source_metadata: BTreeMap<String, SourceMetadata>,
    observed_source_files: BTreeSet<String>,
    root: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CoverageHits {
    tests: u64,
}

#[derive(Clone, Debug, Default)]
struct SourceCoverage {
    covered_lines: BTreeSet<usize>,
    proc_hits: BTreeMap<usize, u64>,
    proc_name_hits: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Default)]
struct SourceMetadata {
    executable_lines: BTreeSet<usize>,
    proc_decls: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
struct SourceFileCoverage {
    file: String,
    executable_lines: usize,
    covered_lines: usize,
    total_procs: usize,
    covered_procs: usize,
}

impl CoverageCollector {
    pub fn new() -> Self {
        Self::with_api(false)
    }

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

    pub fn register_source_files(&mut self, files: &[PathBuf], module_roots: &[PathBuf]) {
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
            };
            let metadata = parse_source_metadata(&file, module_roots).unwrap_or_else(|| {
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

    pub fn ingest_jsonl(&mut self, jsonl: &str) -> Result<(), String> {
        for (index, line) in jsonl.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value = parse_raw_json(line)
                .map_err(|err| format!("failed to parse trace JSONL line {}: {err}", index + 1))?;
            let kind = raw_json_get(&value, "kind")
                .and_then(raw_json_as_str)
                .unwrap_or("");
            if kind == "source.file" {
                self.ingest_source_file(&value);
                continue;
            }
            if self.include_api
                && (kind.ends_with(".call")
                    || kind.ends_with(".start")
                    || kind.ends_with(".enter")
                    || kind == "cwd.enter")
                && let Some(api_id) = raw_json_get(&value, "api_id").and_then(raw_json_as_str)
            {
                self.api_hits
                    .entry(api_id.to_string())
                    .or_default()
                    .tests += 1;
            }
            if let Some(span) = raw_json_get(&value, "source_span") {
                self.ingest_source_span(
                    kind,
                    raw_json_get(&value, "name").and_then(raw_json_as_str),
                    span,
                    raw_json_get(&value, "definition_span").is_none(),
                );
            }
            if (kind == "proc.enter" || kind == "pure.enter")
                && let Some(span) = raw_json_get(&value, "definition_span")
            {
                self.ingest_proc_definition(
                    raw_json_get(&value, "name").and_then(raw_json_as_str),
                    span,
                );
            }
        }
        Ok(())
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("coverage report\n");
        output.push_str("Source coverage\n");
        self.render_source_totals(&mut output);
        output.push('\n');
        output.push_str("least covered source files\n");
        self.render_least_covered_source_files(&mut output);
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
                raw_json_array(source_files.iter().cloned().map(source_file_json)),
            ),
            (
                "source_scope".to_string(),
                raw_json_object([
                    (
                        "files".to_string(),
                        raw_json_usize(source_files.len()),
                    ),
                    (
                        "observed_files".to_string(),
                        raw_json_usize(self.observed_source_files.len()),
                    ),
                ]),
            ),
        ];
        if self.include_api {
            fields.extend([
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
        }
        let value = raw_json_object(fields);
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

    fn ingest_source_file(&mut self, value: &JsonValue) {
        let Some(file) = raw_json_get(value, "file").and_then(raw_json_as_str) else {
            return;
        };
        if self.include_source_file(file) {
            let display = self.display_source_file(file);
            self.observed_source_files.insert(display.clone());
            self.source_hits.entry(display).or_default();
        }
    }

    fn ingest_source_span(
        &mut self,
        kind: &str,
        name: Option<&str>,
        span: &JsonValue,
        count_proc: bool,
    ) {
        let Some(file) = raw_json_get(span, "file").and_then(raw_json_as_str) else {
            return;
        };
        if !self.include_source_file(file) {
            return;
        }
        let Some(start_line) = raw_json_get(span, "start_line")
            .and_then(raw_json_as_u64)
            .and_then(|line| usize::try_from(line).ok())
        else {
            return;
        };
        let end_line = raw_json_get(span, "end_line")
            .and_then(raw_json_as_u64)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or(start_line);
        let entry = self
            .source_hits
            .entry(self.display_source_file(file))
            .or_default();
        for line in start_line..=end_line.max(start_line) {
            entry.covered_lines.insert(line);
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
        let Some(file) = raw_json_get(span, "file").and_then(raw_json_as_str) else {
            return;
        };
        if !self.include_source_file(file) {
            return;
        }
        let Some(start_line) = raw_json_get(span, "start_line")
            .and_then(raw_json_as_u64)
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
        if !file.ends_with(".xsh") {
            return false;
        }
        let path = Path::new(file);
        if let Some(root) = &self.root
            && super::is_path_excluded(root, path, &self.exclude)
        {
            return false;
        }
        let Some(root) = &self.root else {
            return true;
        };
        let Ok(_rel) = path.strip_prefix(root) else {
            return false;
        };
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
            files.push(SourceFileCoverage {
                file: file.clone(),
                executable_lines: metadata.executable_lines.len(),
                covered_lines,
                total_procs: metadata.proc_decls.len(),
                covered_procs,
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
        output.push_str(&format!(
            "source lines: {covered_lines}/{executable_lines} ({})\n",
            percent_text(covered_lines, executable_lines)
        ));
        output.push_str(&format!(
            "proc entries: {covered_procs}/{total_procs} ({})\n",
            percent_text(covered_procs, total_procs)
        ));
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
                truncate_middle(&file.file, 32),
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
        raw_json_object(self.api_hits.iter().map(|(api_id, hits)| {
            (
                api_id.clone(),
                raw_json_object([
                    ("tests".to_string(), raw_json_u64(hits.tests)),
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
                let total = hits.tests;
                (total > 0).then_some((api_id.as_str(), total))
            })
            .collect()
    }

    fn covered_apis_json(&self) -> Vec<JsonValue> {
        self.api_hits
            .iter()
            .filter_map(|(api_id, hits)| {
                let total = hits.tests;
                (total > 0).then(|| {
                    raw_json_object([
                        ("api_id".to_string(), raw_json_string(api_id)),
                        ("tests".to_string(), raw_json_u64(hits.tests)),
                        ("total".to_string(), raw_json_u64(total)),
                    ])
                })
            })
            .collect()
    }
}

fn source_file_json(file: SourceFileCoverage) -> JsonValue {
    raw_json_object([
        ("file".to_string(), raw_json_string(file.file)),
        (
            "executable_lines".to_string(),
            raw_json_usize(file.executable_lines),
        ),
        (
            "covered_lines".to_string(),
            raw_json_usize(file.covered_lines),
        ),
        ("total_procs".to_string(), raw_json_usize(file.total_procs)),
        (
            "covered_procs".to_string(),
            raw_json_usize(file.covered_procs),
        ),
    ])
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
    let mut by_source = BTreeMap::<SourceId, SourceMetadata>::new();
    for file in sources.files() {
        by_source.insert(file.id(), SourceMetadata::default());
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
        ArenaStmtKind::ProcDef(def) | ArenaStmtKind::PureDef(def) => {
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
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
            ..
        } => {
            for binding in program.arena.with_bindings(bindings) {
                add_span(sources, program.arena.span(binding.span), by_source);
            }
            collect_block(program, sources, body, by_source);
            collect_block(program, sources, else_block, by_source);
        }
        ArenaStmtKind::Loop { block } => collect_block(program, sources, block, by_source),
        ArenaStmtKind::Guard { else_block, .. } => {
            add_span(sources, statement.span, by_source);
            collect_block(program, sources, else_block, by_source);
        }
        ArenaStmtKind::GuardedStmt {
            stmt: inner,
            condition,
            ..
        } => {
            add_expr(program, sources, condition, by_source);
            collect_statement(program, sources, inner, by_source);
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
        | ArenaStmtKind::Var { .. }
        | ArenaStmtKind::Assign { .. }
        | ArenaStmtKind::Return(_)
        | ArenaStmtKind::Yield(_)
        | ArenaStmtKind::Defer(_)
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
        assert!(rendered.contains("source lines: 0/3 (0.0%)"), "{rendered}");
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
            rendered.contains("proc entries: 1/1 (100.0%)"),
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
        collector.register_source_files(&[covered.clone(), missed.clone()], &[]);
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
        collector.register_source_files(&[kept.clone(), excluded.clone()], &[]);

        let files = collector.source_file_coverages();
        assert!(files.iter().any(|file| file.file == "kept.xsh"));
        assert!(!files.iter().any(|file| file.file == "evals/task.xsh"));
        let _ = fs::remove_dir_all(root);
    }
}
