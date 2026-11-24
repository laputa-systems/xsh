use super::{CliOutput, push_text, text_bytes};
use std::fs;
use xsh::diagnostic::DiagnosticRenderer;
use xsh::execution::evaluator::Evaluator;
use xsh::frontend::load::{entry_source_from_bytes, parse_load_entry_source_arena_only};
use xsh::frontend::source::{SourceMap, Span};
use xsh::trace::TracebackRenderer;

#[derive(Clone, Debug)]
pub struct ProfileOptions {
    pub script: String,
    pub args: Vec<String>,
    pub json: Option<String>,
    pub folded: Option<String>,
    pub compare: Option<(String, String, f64)>,
}

#[derive(deser::Serialize)]
struct Location { file: String, start: usize, end: usize }
#[derive(deser::Serialize)]
struct Site { kind: String, name: String, file: String, line: usize, definition_span: Option<Location>, call_site_span: Option<Location>, calls: u64, inclusive_ns: u64, exclusive_ns: u64, blocked_ns: u64, alloc_count: u64, alloc_bytes: u64 }
#[derive(deser::Serialize)]
struct Edge { caller: u32, callee: u32, calls: u64 }
#[derive(deser::Serialize)]
struct Opaque { kind: String, span: Option<Location>, wall_ns: u64, count: u64, overlapping: bool }
#[derive(deser::Serialize)]
struct Completeness { mode: &'static str, overhead_ns_per_hook: u64, dropped: u64, blocked_time_rule: &'static str, opaque_regions: Vec<Opaque> }
#[derive(deser::Serialize)]
struct Report { complete: bool, format: &'static str, completeness: Completeness, sites: Vec<Site>, edges: Vec<Edge>, stages: Vec<Stage>, critical_path: Vec<CriticalSite>, unobserved: Vec<&'static str> }
#[derive(deser::Serialize)]
struct Worker { items: u64, start_delay_ns: u64, setup_ns: u64, busy_ns: u64, wall_ns: u64, worker_alloc_peak_bytes: u64 }
#[derive(deser::Serialize)]
struct Stage { span: Option<Location>, file: String, line: usize, executions: u64, items: u64, workers: u64, worker: Vec<Worker>, critical_ns: u64, imbalance: f64, parent_wait_ns: u64, buffered_results: u64, buffered_bytes: u64, buffered_item_max_bytes: u64, input_bytes: u64 }
#[derive(deser::Serialize)]
struct CriticalSite { name: String, kind: String, file: String, line: usize, inclusive_ns: u64 }

fn location(sources: &SourceMap, span: Option<Span>) -> Option<Location> {
    let span = span?;
    Some(Location { file: sources.get(span.source_id)?.name().to_string(), start: span.start(), end: span.end() })
}

fn site_name(sources: &SourceMap, definition: Option<Span>, call_site: Option<Span>, kind: &str) -> String {
    let Some(span) = definition.or(call_site) else { return kind.to_string() };
    let text = sources.span_text(span).unwrap_or("");
    let mut words = text.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|word| !word.is_empty());
    let first = words.next().unwrap_or(kind);
    if matches!(kind, "proc" | "pure") {
        std::iter::once(first).chain(words).find(|word| !matches!(*word, "export" | "pure" | "proc" | "stream" | "optional")).unwrap_or(first).to_string()
    } else { first.to_string() }
}

pub fn profile_script(options: ProfileOptions) -> CliOutput {
    fn failure(message: String) -> CliOutput {
        CliOutput { status: 2, stdout: Vec::new(), stderr: text_bytes(message), trace_text: String::new(), syscall_summary: None }
    }
    if let Some((base, new, threshold)) = &options.compare { return compare_profiles(base, new, *threshold, options.json.as_deref()); }
    let bytes = match fs::read(&options.script) { Ok(bytes) => bytes, Err(error) => return failure(format!("xsht profile: failed to read '{}': {error}\n", options.script)) };
    let entry = entry_source_from_bytes(&options.script, bytes);
    let source_id = entry.source_id;
    if !entry.diagnostics.is_empty() { return failure(DiagnosticRenderer::new().render(&entry.diagnostics, &entry.sources)); }
    let (sources, parsed) = parse_load_entry_source_arena_only(&options.script, entry, Vec::new());
    if !parsed.diagnostics.is_empty() { return failure(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources)); }
    xsh::mem_track::enable_tracking();
    xsh::mem_track::begin_stage();
    let overhead = Evaluator::profile_calibration_ns();
    xsh::mem_track::end_stage();
    xsh::mem_track::begin_worker_collection();
    let output = Evaluator::new_with_sources(options.args, sources).with_profiling().eval(&parsed.arena, source_id);
    let _ = xsh::mem_track::end_worker_collection();
    let sources = &output.sources;
    let mut stderr = output.stderr;
    if !output.diagnostics.is_empty() { push_text(&mut stderr, &DiagnosticRenderer::new().render(&output.diagnostics, sources)); }
    let status = if let Some(traceback) = output.traceback { push_text(&mut stderr, &TracebackRenderer::new().render(&traceback, sources)); 3 } else { output.status };
    let Some(profile) = output.profile else { return CliOutput { status, stdout: output.stdout, stderr, trace_text: String::new(), syscall_summary: None } };
    let mut sites = Vec::new();
    let mut edges = Vec::new();
    for (index, site) in profile.sites.iter().enumerate() {
        let span = site.key.definition.or(site.key.call_site);
        let loc = span.and_then(|span| sources.location(span.source_id, span.start()));
        let kind = site.key.kind.name();
        let name = match site.key.kind {
            xsh_kind if xsh_kind.phase_name().is_some() => xsh_kind.phase_name().unwrap().to_string(),
            _ if site.key.function_name.is_some() => site.key.function_name.unwrap().as_str().as_str().to_string(),
            _ => site.key.module.as_ref().map(|module| {
                let path = std::path::Path::new(module.as_ref());
                std::env::current_dir().ok().and_then(|cwd| path.strip_prefix(cwd).ok().map(|path| path.to_string_lossy().into_owned())).unwrap_or_else(|| module.to_string())
            }).unwrap_or_else(|| site_name(sources, site.key.definition, site.key.call_site, kind)),
        };
        sites.push(Site { kind: kind.to_string(), name, file: loc.as_ref().map_or_else(String::new, |loc| loc.file.clone()), line: loc.map_or(0, |loc| loc.line), definition_span: location(sources, site.key.definition), call_site_span: location(sources, site.key.call_site), calls: site.calls, inclusive_ns: site.inclusive_ns, exclusive_ns: site.exclusive_ns, blocked_ns: site.blocked_ns, alloc_count: site.alloc_count, alloc_bytes: site.alloc_bytes });
        for (&callee, &calls) in &site.children { edges.push(Edge { caller: index as u32, callee, calls }); }
    }
    edges.sort_by_key(|edge| (edge.caller, edge.callee));
    let opaque_regions = profile.opaque.iter().map(|region| Opaque { kind: region.kind.to_string(), span: location(sources, region.span), wall_ns: region.wall_ns, count: region.count, overlapping: region.overlapping }).collect();
    let stages: Vec<_> = profile.stages.iter().map(|stage| {
        let loc = sources.location(stage.span.source_id, stage.span.start());
        let busy: u64 = stage.worker.iter().map(|worker| worker.busy_ns).sum();
        let max = stage.worker.iter().map(|worker| worker.busy_ns).max().unwrap_or(0);
        Stage { span: location(sources, Some(stage.span)), file: loc.as_ref().map_or_else(String::new, |loc| loc.file.clone()), line: loc.map_or(0, |loc| loc.line), executions: stage.executions, items: stage.items, workers: stage.workers, worker: stage.worker.iter().map(|worker| Worker { items: worker.items, start_delay_ns: worker.start_delay_ns, setup_ns: worker.setup_ns, busy_ns: worker.busy_ns, wall_ns: worker.wall_ns, worker_alloc_peak_bytes: worker.worker_alloc_peak_bytes }).collect(), critical_ns: stage.critical_ns, imbalance: if busy == 0 { 1.0 } else { max as f64 * stage.worker.len() as f64 / busy as f64 }, parent_wait_ns: stage.parent_wait_ns, buffered_results: stage.buffered_results, buffered_bytes: stage.buffered_bytes, buffered_item_max_bytes: stage.buffered_item_max_bytes, input_bytes: stage.input_bytes }
    }).collect();
    let mut critical_path = Vec::new();
    let mut parent = None;
    while let Some((index, path)) = profile.paths.iter().enumerate().filter(|(_, path)| path.parent == parent && !path.worker).max_by_key(|(_, path)| path.inclusive_ns) {
        let site = &sites[path.site as usize];
        critical_path.push(CriticalSite { name: site.name.clone(), kind: site.kind.clone(), file: site.file.clone(), line: site.line, inclusive_ns: path.inclusive_ns });
        if let Some(stage) = profile.stages.iter().find(|stage| profile.sites[path.site as usize].key.call_site == Some(stage.span)) {
            if let Some((worker_index, worker)) = stage.worker.iter().enumerate().max_by_key(|(_, worker)| worker.start_delay_ns.saturating_add(worker.wall_ns)) {
                critical_path.push(CriticalSite { name: format!("{} worker {}", site.name, worker_index), kind: "parallel-worker".into(), file: site.file.clone(), line: site.line, inclusive_ns: worker.wall_ns });
            }
            break;
        }
        parent = Some(index as u32);
    }
    let report = Report { complete: profile.dropped == 0, format: "xsht-profile/1", completeness: Completeness { mode: "attribution", overhead_ns_per_hook: overhead, dropped: profile.dropped, blocked_time_rule: "Evaluator host-call and parallel wait wall time; overlapping lifetimes are opaque.", opaque_regions }, sites, edges, stages, critical_path, unobserved: vec!["entry-script preparation (see xsht runtime-stats)", "fused par-map/reduce workers", "external command internals", "cross-thread frees debit the freeing thread, not the allocating site", "parent-side worker slot cloning", "per-worker critical path frames"] };
    if let Some(path) = options.json {
        let json = deser_json::to_string(&report).expect("profile report encodes");
        if let Err(error) = fs::write(&path, format!("{json}\n")) { push_text(&mut stderr, &format!("xsht profile: failed to write '{path}': {error}\n")); return CliOutput { status: 4, stdout: output.stdout, stderr, trace_text: String::new(), syscall_summary: None }; }
    }
    if let Some(path) = options.folded {
        let mut folded = String::new();
        for item in &profile.paths {
            if item.exclusive_ns == 0 { continue; }
            let mut names = vec![report.sites[item.site as usize].name.replace(';', "_")];
            let mut parent = item.parent;
            while let Some(index) = parent { let node = &profile.paths[index as usize]; names.push(report.sites[node.site as usize].name.replace(';', "_")); parent = node.parent; }
            names.reverse();
            folded.push_str(&format!("{} {}\n", names.join(";"), item.exclusive_ns));
        }
        if let Err(error) = fs::write(&path, folded) { push_text(&mut stderr, &format!("xsht profile: failed to write '{path}': {error}\n")); return CliOutput { status: 4, stdout: output.stdout, stderr, trace_text: String::new(), syscall_summary: None }; }
    }
    let mut stdout = output.stdout;
    let mut ranked: Vec<_> = report.sites.iter().collect();
    ranked.sort_by_key(|site| std::cmp::Reverse(site.exclusive_ns));
    push_text(&mut stdout, &format!("profile: complete: {}\n", report.complete));
    push_text(&mut stdout, "profile: exclusive ns  calls  site\n");
    for site in ranked.iter().take(20) { push_text(&mut stdout, &format!("{:>20} {:>6}  {} {}:{}\n", site.exclusive_ns, site.calls, site.name, site.file, site.line)); }
    push_text(&mut stdout, "profile: critical path\n");
    for site in &report.critical_path { push_text(&mut stdout, &format!("  {} ns {} {} {}:{}\n", site.inclusive_ns, site.kind, site.name, site.file, site.line)); }
    for stage in &report.stages { push_text(&mut stdout, &format!("profile: stage {}:{} imbalance {:.2}\n", stage.file, stage.line, stage.imbalance)); }
    CliOutput { status, stdout, stderr, trace_text: String::new(), syscall_summary: None }
}

#[derive(deser::Deserialize)]
struct CompareInput { format: String, completeness: CompareCompleteness, sites: Vec<CompareSite> }
#[derive(deser::Deserialize)]
struct CompareCompleteness { overhead_ns_per_hook: u64 }
#[derive(deser::Deserialize)]
struct CompareLocation { start: usize }
#[derive(deser::Deserialize)]
struct CompareSite { kind: String, name: String, file: String, definition_span: Option<CompareLocation>, calls: u64, inclusive_ns: u64, exclusive_ns: u64 }
#[derive(Default)]
struct Totals { calls: u64, inclusive_ns: u64, exclusive_ns: u64 }
#[derive(deser::Serialize)]
struct ComparisonRow { kind: String, name: String, file: String, definition_start: Option<usize>, status: String, exclusive_delta_ns: i128, inclusive_delta_ns: i128, calls_delta: i128, regressed: bool }
#[derive(deser::Serialize)]
struct Comparison { format: &'static str, threshold_pct: f64, sites: Vec<ComparisonRow> }

fn compare_profiles(base: &str, new: &str, threshold: f64, json: Option<&str>) -> CliOutput {
    fn read(path: &str) -> Result<CompareInput, String> {
        let text = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let report: CompareInput = deser_json::from_str(&text).map_err(|error| format!("{path}: {error}"))?;
        if report.format != "xsht-profile/1" { return Err(format!("{path}: unsupported profile format")); }
        Ok(report)
    }
    let result = (|| -> Result<(String, bool), String> {
        let base = read(base)?;
        let new = read(new)?;
        let overhead = base.completeness.overhead_ns_per_hook.max(new.completeness.overhead_ns_per_hook);
        let mut sites = std::collections::BTreeMap::new();
        for (side, report) in [(0, base), (1, new)] {
            for site in report.sites {
                let key = (site.kind, site.name, site.file, site.definition_span.map(|span| span.start));
                let pair: &mut [Option<Totals>; 2] = sites.entry(key).or_insert_with(|| [None, None]);
                let totals = pair[side].get_or_insert_with(Totals::default);
                totals.calls = totals.calls.saturating_add(site.calls);
                totals.inclusive_ns = totals.inclusive_ns.saturating_add(site.inclusive_ns);
                totals.exclusive_ns = totals.exclusive_ns.saturating_add(site.exclusive_ns);
            }
        }
        let mut rows = Vec::new();
        let mut text = String::from("profile comparison: exclusive ns  inclusive ns  calls  site\n");
        let mut any = false;
        for ((kind, name, file, definition_start), [before, after]) in sites {
            let status = match (&before, &after) { (None, _) => "added", (_, None) => "removed", _ => "matched" };
            let before = before.unwrap_or_default();
            let after = after.unwrap_or_default();
            let delta = i128::from(after.exclusive_ns) - i128::from(before.exclusive_ns);
            let noise = u128::from(overhead).saturating_mul(10).saturating_mul(u128::from(before.calls.max(after.calls)));
            let regressed = status == "matched" && delta > 0 && (delta as u128) > noise && (after.exclusive_ns as f64) > (before.exclusive_ns as f64) * (1.0 + threshold / 100.0);
            any |= regressed;
            let inclusive_delta_ns = i128::from(after.inclusive_ns) - i128::from(before.inclusive_ns);
            let calls_delta = i128::from(after.calls) - i128::from(before.calls);
            text.push_str(&format!("{delta:+} {inclusive_delta_ns:+} {calls_delta:+} {name} {file} {status}{}\n", if regressed { " REGRESSED" } else { "" }));
            rows.push(ComparisonRow { kind, name, file, definition_start, status: status.into(), exclusive_delta_ns: delta, inclusive_delta_ns, calls_delta, regressed });
        }
        if let Some(path) = json {
            let report = Comparison { format: "xsht-profile-comparison/1", threshold_pct: threshold, sites: rows };
            let data = deser_json::to_string(&report).map_err(|error| error.to_string())?;
            fs::write(path, format!("{data}\n")).map_err(|error| format!("{path}: {error}"))?;
        }
        Ok((text, any))
    })();
    match result {
        Ok((text, any)) => CliOutput { status: u8::from(any), stdout: text_bytes(text), stderr: Vec::new(), trace_text: String::new(), syscall_summary: None },
        Err(error) => CliOutput { status: 2, stdout: Vec::new(), stderr: text_bytes(format!("xsht profile: {error}\n")), trace_text: String::new(), syscall_summary: None },
    }
}
