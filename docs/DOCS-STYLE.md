# XSH Docs Style Guide

This guide keeps tutorial chapters human-friendly while the spec and reference
manuals stay precise.

## Code References

When documentation explains implementation ownership, pair the domain concept
with an exact symbol, path, and test: “compact executable commit is owned by
`FullBuilder::build_compact` in `src/runtime/eval/lower.rs`, covered by the
indexed verifier tests.” Use the same spelling as the definition so `rg -w`
finds the definition, callers, and coverage in one search.

User-facing tutorial text should name the XSH symbol being taught, such as
`test.run_script` or `Path.read_text()`. Do not replace stable XSH API names
with implementation names, and do not add Rust internals to examples unless
the implementation boundary is part of the lesson.

## Tutorial Chapter Shape

Every tutorial chapter should have:

- a clear opening goal in the first few paragraphs;
- one primary useful script or workflow;
- short explanations that build toward that script;
- expected output or a clear output note when host values vary;
- at least one comparison to bash or common CLI tools when it clarifies XSH's
  value;
- one or two "do not use this when..." notes where the feature has real
  tradeoffs;
- a concrete closing summary that names what the reader can now do.

Prefer one complete example over many disconnected fragments. Small snippets
are fine when they explain a local point, but the chapter should still feel like
it is moving toward a finished task.

## Voice

Write like a pragmatic mentor:

- conversational but not chatty;
- concrete before abstract;
- direct about tradeoffs;
- focused on real systems scripting work;
- careful with claims about safety, performance, and portability.

Avoid reference-manual prose in the tutorial. The tutorial should teach
judgment. `docs/STDLIB.md`, `docs/REFERENCE.md`, and `docs/SPEC.md` can list
complete surfaces.

## Bash And CLI Comparisons

Comparisons should show why XSH exists without dunking on existing tools.
Mention bash, `awk`, `sed`, `jq`, `xargs`, `tar`, or other tools when the reader
can immediately see the tradeoff:

- shell text pipelines are excellent at quick terminal work;
- XSH is better when the work becomes policy, needs typed boundaries, or must be
  reviewed and tested;
- external tools are still correct when they are the specialized worker and XSH
  is orchestrating them.

Do not turn the guide into a bash migration manual. Use comparisons at
important boundaries: argv, quoting, cwd/env scope, status handling, JSON,
streams, archives, and typed records.

## Example Naming

Cataloged example IDs and filenames should describe the task, not the internal
feature being exercised. Prefer names like `command-probe`, `package-records`,
`typed-cli-options`, and `stream-surface` over generic names like `types` or
`foundations`.

When renaming an example, update:

- the file under `examples/`;
- `examples/catalog.json`;
- the `{{include:...}}` directive in `docs-src/`;
- `examples/README.md`;
- any tests that refer to the example ID.

## Generated Docs

Edit `docs-src/`, cataloged examples, and implementation metadata first. Then
run:

```sh
cargo run -p xsht --features docs-html -- docs build
cargo run -p xsht --features docs-html -- docs check
cargo test -p xsht --features docs-html docs
cargo test --test runtime example_
```

Use `make docs` when you need the full repository docs gate.
