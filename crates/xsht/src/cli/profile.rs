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
struct Report { format: &'static str, completeness: Completeness, sites: Vec<Site>, edges: Vec<Edge> }

fn location(sources: &SourceMap, span: Option<Span>) -> Option<Location> {
    let span = span?;
    Some(Location { file: sources.get(span.source_id)?.name().to_string(), start: span.start(), end: span.end() })
}

fn site_name(sources: &SourceMap, definition: Option<Span>, call_site: Option<Span>, kind: &str) -> String {
    let Some(span) = definition.or(call_site) else { return kind.to_string() };
    let text = sources.span_text(span).unwrap_or("");
    let mut words = text.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|word| !word.is_empty());
    let first = words.next().unwrap_or(kind);
    if matches!(first, "pure" | "proc" | "stream") { words.next().unwrap_or(first).to_string() } else { first.to_string() }
}

pub fn profile_script(options: ProfileOptions) -> CliOutput {
    fn failure(message: String) -> CliOutput {
        CliOutput { status: 2, stdout: Vec::new(), stderr: text_bytes(message), trace_text: String::new(), syscall_summary: None }
    }
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
    let output = Evaluator::new_with_sources(options.args, sources).with_profiling().eval(&parsed.arena, source_id);
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
        sites.push(Site { kind: kind.to_string(), name: site_name(sources, site.key.definition, site.key.call_site, kind), file: loc.as_ref().map_or_else(String::new, |loc| loc.file.clone()), line: loc.map_or(0, |loc| loc.line), definition_span: location(sources, site.key.definition), call_site_span: location(sources, site.key.call_site), calls: site.calls, inclusive_ns: site.inclusive_ns, exclusive_ns: site.exclusive_ns, blocked_ns: site.blocked_ns, alloc_count: site.alloc_count, alloc_bytes: site.alloc_bytes });
        for (&callee, &calls) in &site.children { edges.push(Edge { caller: index as u32, callee, calls }); }
    }
    edges.sort_by_key(|edge| (edge.caller, edge.callee));
    let opaque_regions = profile.opaque.iter().map(|region| Opaque { kind: region.kind.to_string(), span: location(sources, region.span), wall_ns: region.wall_ns, count: region.count, overlapping: region.overlapping }).collect();
    let report = Report { format: "xsht-profile/1", completeness: Completeness { mode: "attribution", overhead_ns_per_hook: overhead, dropped: profile.dropped, blocked_time_rule: "Evaluator host-call wall time only; overlapping external lifetimes are opaque.", opaque_regions }, sites, edges };
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
    push_text(&mut stdout, "profile: exclusive ns  calls  site\n");
    for site in ranked.iter().take(20) { push_text(&mut stdout, &format!("{:>20} {:>6}  {} {}:{}\n", site.exclusive_ns, site.calls, site.name, site.file, site.line)); }
    CliOutput { status, stdout, stderr, trace_text: String::new(), syscall_summary: None }
}
