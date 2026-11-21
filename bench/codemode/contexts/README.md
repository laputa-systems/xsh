# Contexts

Each XSH condition puts one context file before the task. The task section, the task's
API facts, and the run instructions are identical across XSH conditions; only this file
varies. The TypeScript arm has no context file.

| File | Condition | Status |
|---|---|---|
| `baseline.md` | `xsh-baseline` | Current documentation, frozen before any scored call |
| `candidate.md` | `xsh-candidate` | The compact guide written before the first development run |
| `revised.md` | `xsh-revised` | The one evidence-driven revision |
| `selected.md` | `xsh-selected` | The guide selected on development evidence, frozen before held-out runs |

`bench/codemode/codemode context` rebuilds `baseline.md` from its rule and fails if the
committed file differs; it prints the SHA-256 and size of every context present.

## Baseline selection rule

`baseline.md` is built mechanically by `harness/contexts.py` from the lane base
`c68ac9594` and the frozen `xsht`:

1. The output of `xsht api` with no selector, the toolchain's own getting-started text,
   verbatim in a `text` block.
2. The output of `xsht api language:core`, the language rules, verbatim in a `text`
   block.
3. The tour, `docs/user-tour.md` at the lane base: its introduction and every `##`
   section that teaches a general language or scripting facility, in tour order and
   verbatim, with HTML comments removed and every heading outside code blocks demoted
   one level. Included: Running a Script; Values at a Glance; Commands and argv; Paths;
   Errors, Results, and `?`; Environment Variables; Records, Lists, and Maps; Streams and
   Pipelines; JSON Boundaries; Effects; Functions and Inference; Concurrency: Fan Out,
   Collect; Cleanup with `defer` (with its config-editing subsection); Timeouts and
   Retries; A Real Program; Project Layout; Where to Go Next. Excluded, because each
   teaches one domain module or a tool rather than the language: Host State Without
   Scraping Text, Signals and Cancellation, Archives, Rendering Config from a Template,
   Testing, Formatting and Linting.

The rule was fixed before any model output was seen. A task that needs a module the rule
excludes receives that module's exact entries through its task facts, as every XSH
condition does.

## Task facts

Every XSH condition receives, per task, the output of
`xsht api --strict SELECTOR...` for the task's `xsh_api` selectors, and for a shared
task the exported declarations of its generated `svc` module with the service's
behavior notes. The selectors name the standard-library functions, methods, records and
language items the task's XSH reference calls or relies on at its boundaries. The
TypeScript arm receives the Node.js built-in modules its reference imports
(`ts_modules`), and for a shared task the generated `svc.mts` declarations and the same
behavior notes.
