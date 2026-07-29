# XSH Tracing TODO

XSH source is deliberately tree-shaped: ordinary files, stable syntax, readable
blocks, and forms that remain searchable with text tools. Runtime execution is
graph-shaped: calls create dynamic frames, processes connect argv/env/cwd to
host state, pipelines move data across stages, parallel jobs fork work, and
errors propagate along paths that are not identical to the AST.

Tracing should make that runtime graph inspectable without turning XSH into a
graph-programming language. The source stays simple; tooling exposes graph
projections when users need to understand what happened.

## Greppable implementation handles

| Concern | Symbols | Owner and coverage |
|---|---|---|
| event and payload contract | `TraceEvent`, `TraceKind`, `TracePayload`, `TraceStatus` | `src/trace.rs`; trace fixtures under `tests/runtime/` |
| dynamic parentage | `TraceEvent::with_parent`, `Evaluator::trace_enter`, `Evaluator::trace_exit`, `parent_event_id` | `src/trace.rs`, `src/runtime/eval.rs`; `spawn_trace_json_correlates_handle_ids` and process trace tests |
| trace rendering | `TraceTextRenderer`, `TraceJsonlRenderer`, `TraceSummaryRenderer` | `src/trace.rs`; `tests/runtime/run.rs` and `tests/runtime/examples.rs` |
| normalization and graph views | `TraceNormalizer`, `TraceFlamegraphRenderer` | `src/trace.rs`; trace rendering and summary tests |

Use the exact Rust symbols for implementation work and the `xsht trace`
command for user-facing behavior. The event contract is owned by
`TraceEvent`/`TracePayload`, not by rendered text.

## Design Rules

- Keep new language syntax out of tracing unless the same capability cannot be
  expressed through runtime events or tooling.
- Treat source spans as anchors into the source tree, not as the execution model
  itself.
- Record structured runtime relationships. Avoid reconstructing facts from
  shell strings, rendered diagnostics, or presentation-only summaries.
- Prefer derived graph views over additional mandatory trace payload in v1.
  Promote new event fields only when parentage, spans, timing, and payload data
  cannot represent the relationship clearly.
- Preserve the distinction between status-as-data and propagated errors.
  Tracing should make both visible without changing their semantics.

## Runtime Graph Vocabulary

- Event nodes: trace events with stable ids, kinds, timing, source spans, names,
  API ids, and structured payloads.
- Source anchors: links from runtime nodes back to the AST span that introduced
  the boundary.
- Parent edges: dynamic containment, including script execution, proc/pure calls,
  core calls, module/method calls, process work, pipeline stages, stream stages,
  and scoped cwd/env regions.
- Process edges: executable target, argv items, cwd, environment overlay,
  pid/status, exec failure, cancellation, and pipeline segment membership.
- Dataflow edges: structured stream stage input/output, terminal collection,
  item failures, and parallel job item ownership.
- Ambient-state edges: cwd and environment scopes that change how nested process
  or file operations resolve.
- Resource edges: file paths, redirection targets, archive paths, JSON inputs,
  and other host resources named by runtime payloads.
- Failure edges: `Result` propagation, runtime errors, tracebacks, process
  nonzero exits that are asserted as errors, and cancellation paths.
- Host-observation edges: syscall attribution and timing data attached to the
  XSH operation that caused the host activity.

## Near-Term TODOs

- [ ] Document the current trace event stream as a partial runtime graph model:
  ids are nodes, `parent_event_id` gives dynamic containment, source spans anchor
  nodes to syntax, and payloads describe cross-runtime relationships.
- [ ] Audit every explicit boundary for enough structured payload data:
  `run`, process pipelines, redirections, cwd scopes, env overlays, module calls,
  method calls, stream stages, parallel jobs, result propagation, and runtime
  errors.
- [ ] Define which graph edges are explicit fields today and which are derived
  from parentage, source spans, event kinds, timing windows, and payload values.
- [ ] Add trace fixture coverage for parentage and payload continuity across
  nested proc calls, process pipelines, stream stages, parallel jobs, cwd/env
  scopes, redirections, and propagated errors.
- [ ] Keep text summaries as summaries. Do not let summary rendering become the
  source of truth for relationships that raw text and JSONL traces need to
  expose structurally.
- [ ] Clarify syscall attribution as host-observation data attached to runtime
  graph nodes, not as a separate unrelated report.
- [ ] Prototype a derived graph projection for tooling. Possible outputs include
  graph JSON, DOT, or a trace query report, but the trace event contract should
  stay the primary source until the projection proves useful.
- [ ] Add a trace-query vocabulary before adding broad UI. Useful first queries:
  "show process tree", "show operations under this source span", "show failed
  path", "show environment/cwd in effect for this run", and "show slowest
  runtime edges".
- [ ] Decide how much item-level stream tracing is acceptable by default.
  Preserve low-noise traces for common scripts, with opt-in detail for dataflow
  investigations.
- [ ] Keep coverage derived from trace events aligned with the same graph model:
  coverage is a projection over runtime operation nodes, not a separate event
  system.

## Open Design Questions

- Should a future graph projection be a new `--trace-format` value or a separate
  `xsht trace graph` view?
- Should trace events gain explicit edge records, or should graph views remain
  derived from event ids and payloads until a concrete use case needs first-class
  edges?
- How should long-running or high-volume streams expose representative dataflow
  without making trace output unusably large?
- Which resource payloads should be standardized first: files, directories,
  sockets, archives, JSON sources, or process descriptors?
