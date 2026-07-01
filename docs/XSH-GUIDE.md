# XSH Guide

This page is generated from `docs-src/CHAPTER-*.md.in`.

Markdown under `docs/` is the primary generated artifact for agents, code review, and repository navigation. Human readers should start with `docs-html/index.html`, which is generated from the same markdown and checked for drift by `xsht docs check`.

`docs/SPEC.md` is the normative language contract. `docs/SPEC-OS.md` details OS-facing runtime behavior such as signals, process groups, cancellation, and signal hooks. `docs/STDLIB.md` is the generated standard-library manual. `STDLIB-PROPOSALS.md` records standard-library design criteria, explicit non-goals, and open proposals. `docs/REFERENCE.md` is the generated non-stdlib language and tooling reference. The tutorial chapters are generated from `docs-src/` and include only cataloged examples from `examples/`.

## Reader Paths

- New to XSH: read chapters 1 through 8 in order.
- Shell user evaluating the value: read chapters 1, 2, 4, 5, 8, and 15.
- Building a maintainable tool: read chapters 3, 4, 10, 11, 12, and 13.
- Looking up exact behavior: use `docs/STDLIB.md`, `docs/REFERENCE.md`, and `docs/SPEC.md`.

## Chapters

- `docs/CHAPTER-01-why-xsh.md`: Chapter 1: Why XSH
- `docs/CHAPTER-02-foundations.md`: Chapter 2: Language Foundations
- `docs/CHAPTER-03-tooling.md`: Chapter 3: Tooling And Development Loop
- `docs/CHAPTER-04-processes.md`: Chapter 4: Processes And System State
- `docs/CHAPTER-05-files-install.md`: Chapter 5: Files And Install Workflows
- `docs/CHAPTER-06-text-bytes-hash.md`: Chapter 6: Text, Bytes, And Hashes
- `docs/CHAPTER-07-json-data.md`: Chapter 7: JSON And Data Boundaries
- `docs/CHAPTER-08-structured-streams.md`: Chapter 8: Structured Streams
- `docs/CHAPTER-09-archive-diff-patch.md`: Chapter 9: Archives, Diffs, And Patches
- `docs/CHAPTER-10-types-records-procs.md`: Chapter 10: Types, Records, And Procs
- `docs/CHAPTER-11-large-programs.md`: Chapter 11: Larger Programs
- `docs/CHAPTER-12-testing.md`: Chapter 12: Testing
- `docs/CHAPTER-13-tracing.md`: Chapter 13: Tracing And Tracebacks
- `docs/CHAPTER-14-idioms.md`: Chapter 14: Idioms
- `docs/CHAPTER-15-why-not-xsh.md`: Chapter 15: Why Not XSH
- `docs/STDLIB.md`: generated standard-library manual.
- `STDLIB-PROPOSALS.md`: standard-library design and open proposals.
- `docs/DOCS-STYLE.md`: tutorial and reference documentation style guide.
- `docs/REFERENCE.md`: generated non-stdlib language and tooling reference.
- `docs/AGENT-ROUTING.md`: coding-agent task routing and owner map.
- `docs/TEST-MAP.md`: focused verification commands by change type.
- `docs/GENERATED-DOCS.md`: generated documentation source map.
- `docs/CHANGE-RECIPES.md`: common implementation checklists.
- `docs/JSON.md`: guidance for JSON boundary patterns and dynamic JSON tools.
- `docs/STREAMS.md`: structured stream implementation notes and invariants.
- `docs/COVERAGE.md`: practical coverage plan and harness notes.
- `docs/IR.md`: lowered IR, symbol identity, registry invariant, and benchmark contract.
- `perf/README.md`: performance scenarios, profiling, PGO, and syscall tracing.
- `docs/THREADRIPPER.md`: remote amd64 Alpine host notes for native musl work.

## Maintenance

Edit `docs-src/`, `examples/catalog.json`, and the implementation metadata. Use the formatter-free docs gate in `docs/TEST-MAP.md`.
