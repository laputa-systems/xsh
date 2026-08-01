# Documentation Cleanup Plan

## Goal

Make `docs/` a compact contract and reference layer for agents and maintainers,
not a tutorial corpus. Keep the documentation that explains philosophy,
contracts, invariants, architecture, and non-obvious idioms. Prefer exact code
and test pointers over prose that restates obvious language behavior.

## Scope Boundary

Preserve all root-level Markdown documents unchanged. The only root-level
Markdown exception is `AGENTS.md`, which will gain the content-tier and
documentation-routing policy for coding agents. This cleanup does not rewrite,
delete, or reorganize other top-level Markdown files.

`DOCS-CLEANUP.md` is the planning artifact itself and may change as this plan
evolves; it is not part of the product-document cleanup.

## Deduplication Rule

Treat tutorial content as redundant by default. Preserve a section only when it
adds at least one of:

- a language or runtime contract not already stated authoritatively elsewhere;
- a non-obvious invariant, platform constraint, or design rationale;
- an idiomatic multi-module composition that is easier to understand as a
  complete program than as isolated tests.

Introductory explanations of syntax, values, control flow, standard-library
signatures, and straightforward API usage should be deleted rather than
rewritten. When a tutorial contains a unique contract, move the smallest useful
statement to the canonical document and name the implementation and test paths.

## Keep

- `docs/CHAPTER-01-why-xsh.md` as hand-maintained philosophical grounding.
- Rigorous canonical docs: `SPEC.md`, `SPEC-OS.md`, `ARCHITECTURE.md`,
  `STREAMS.md`, `JSON.md`, `STDLIB.md`, `REFERENCE.md`, `AGENT-ROUTING.md`,
  `TEST-MAP.md`, and `IDIOMS.md`.
- `examples/` only for substantial, idiomatic showcase programs.
- `tests/xsh/stdlib/*.xsh` and related native tests as the detailed executable
  coverage for language and standard-library behavior.

## Agent Routing and Quality Policy

Add a strict content-routing policy to `AGENTS.md` so an agent writing XSH can
choose the right home without reading the whole repository:

- Native tests belong in `tests/xsh/stdlib/*.xsh` or the nearest focused native
  test module when they verify syntax, API behavior, edge cases, errors,
  platform behavior, or regressions.
- Curated examples belong in `examples/*.xsh` only when they demonstrate a
  substantial, idiomatic multi-module program that is useful to read as a
  whole. They must not duplicate focused native-test coverage.
- Larger production-like programs belong in `showcase/`. Existing
  `showcase/` content is explicitly outside this cleanup scope.

The same policy should route prose to canonical owners: language contracts to
`docs/SPEC.md`, OS behavior to `docs/SPEC-OS.md`, streams to `docs/STREAMS.md`,
JSON to `docs/JSON.md`, API details to `docs/STDLIB.md` or `docs/REFERENCE.md`,
architecture to `docs/ARCHITECTURE.md`, testing to `docs/TEST-MAP.md`, idioms to
`docs/IDIOMS.md`, and task navigation to `docs/AGENT-ROUTING.md`.

Set the quality bar explicitly: do not add prose that restates obvious syntax or
API signatures; prefer exact symbols, module paths, and test names; document
non-obvious constraints and rationale; and update the canonical owner instead
of creating another cross-cutting guide.

## Roll Up Then Delete

- Chapters 2–3 → `SPEC.md`, `REFERENCE.md`, and `XSHT.md` only for contracts or
  tooling behavior that is not already covered.
- Chapters 4–6 → `SPEC-OS.md` and `STDLIB.md` only for non-obvious process,
  filesystem, text, bytes, or hashing behavior.
- Chapter 7 → `JSON.md` and `examples/json.xsh`.
- Chapter 8 → `STREAMS.md` and a larger idiomatic `examples/streams.xsh`.
- Chapter 9 → `STDLIB.md` and a focused archive/diff/patch showcase only if the
  composition is useful beyond native tests.
- Chapter 10 → `SPEC.md` only for type-system contracts not already covered by
  native tests.
- Chapter 11 → `ARCHITECTURE.md` and `IDIOMS.md` only for maintainability or
  composition guidance.
- Chapter 12 → `TEST-MAP.md` and native-test documentation.
- Chapter 13 → tracing sections in `SPEC.md` or `ARCHITECTURE.md` only for
  trace contracts and ownership.
- Chapter 14 → `IDIOMS.md`; keep or consolidate its strongest examples.
- Chapter 15 → delete after preserving any durable boundary statements in
  Chapter 1 or the architecture docs.
- Delete `docs/XSH-GUIDE.md` as a prose aggregator; routing docs become the
  agent entry point.

## Example and Test Boundary

Examples and tests serve different purposes:

- `tests/xsh/stdlib/*.xsh` owns exhaustive, focused, regression-oriented
  coverage of syntax, signatures, edge cases, errors, and platform behavior.
- `examples/*.xsh` owns a small curated set of larger programs that demonstrate
  idiomatic composition across modules. An example should teach how to shape a
  useful program, not merely prove that one function exists.
- Each retained category showcase should cover the meaningful surface of its
  category in one coherent flow. For example, `JSON.md` should link to a
  substantial `examples/json.xsh`, not a two-line serialization demo.
- Delete simplistic examples such as isolated hello/value/argument/API probes
  after moving any missing assertions into the nearest native test module.
- Consolidate overlapping examples instead of preserving one file per feature.
  Keep examples such as JSON, streams, processes, tracing, and typed CLI only
  when they demonstrate durable idiomatic style.
- Keep retained examples checked in and lintable. Continue using a lightweight
  manifest for their arguments, expected output, status, and trace policy, but
  remove chapter/include metadata that exists only for tutorial generation.
- Update `examples/README.md` to explain this boundary and link each retained
  showcase to its canonical documentation and native-test coverage.

## Code and Build Changes

1. Audit Chapters 2–15 and classify every section as canonical contract,
   non-obvious invariant, idiomatic composition, or redundant explanation.
2. Move only canonical facts and durable invariants into the existing rigorous
   docs; discard the rest rather than paraphrasing it.
3. Inventory all 58 current examples against `tests/xsh/stdlib/*.xsh`; migrate
   focused coverage into native tests, consolidate worthwhile showcases, and
   delete trivial or duplicate examples.
4. Redesign `examples/catalog.json` as a showcase manifest without chapter or
   include directives, or remove it if the retained example runner can discover
   and validate examples directly.
5. Delete `docs-src/` and all chapter-template, include-expansion, and tutorial
   rendering code from `xsht`.
6. Make `docs/CHAPTER-01-why-xsh.md` static and hand-maintained.
7. Keep `xsht docs build/check` only for code-derived `STDLIB.md` and
   `REFERENCE.md` outputs.
8. Add the three-tier content policy and canonical documentation routing to
   `AGENTS.md`.
9. Revisit `docs/DOCS-STYLE.md`, `docs/GENERATED-DOCS.md`,
   `docs/CHANGE-RECIPES.md`, and `examples/README.md`; remove tutorial-era
   instructions and make the remaining guidance concise and operational.
10. Perform a repository-wide link and reference cleanup across Markdown,
    `AGENTS.md`, Makefiles, tests, comments, and tooling after deleting chapters
    and `docs-src/`.
11. Remove tutorial-generation instructions, chapter tests, stale links, and
    chapter fields from the docs and tooling.
12. Update the native example/test gates so retained showcases remain formatted,
   linted, and executable independently of documentation generation.

## Acceptance Criteria

- Only Chapter 1 remains from the tutorial chapter set.
- No `docs-src`, chapter include directives, or tutorial renderer references
  remain.
- Canonical docs contain contracts and invariants, not duplicated introductions.
- Every retained `examples/*.xsh` is a substantial idiomatic showcase with a
  clear documentation link and executable validation.
- Focused language and standard-library behavior lives under
  `tests/xsh/stdlib/*.xsh` or the nearest native test module.
- `AGENTS.md` routes new code, examples, tests, and prose to the correct tier
  and canonical documentation owner.
- No stale chapter, `docs-src`, include-directive, or deleted-document links
  remain anywhere in the repository.
- `xsht docs build/check`, native tests, example linting, and retained showcase
  execution all pass.
