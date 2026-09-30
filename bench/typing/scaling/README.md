The fixed seed is `877114`. `../scaling-manifest.json` freezes 56 regular
closures (seven families, four sizes, annotated and inferred variants) and
eight adversarial closures across seven families. Infinite aliases and
self-application equations have separate closures so the first rejection cannot
hide the second boundary. The generator and resource-limit file hashes are
part of the freeze. Generated sources are UTF-8 with LF line endings.

Generate one closure before its counter run:

```sh
python3 bench/typing/scaling.py generate --case module_diamonds-1000-inferred --output /tmp/xsh-scaling-new-case
XSH_MODULE_PATH=/tmp/xsh-scaling-new-case /absolute/candidate/xsht check /tmp/xsh-scaling-new-case/entry.xsh
```

Use a fresh output directory for each bundle. `sources.json` contains every
relative source path, canonical namespace, byte count and SHA-256. The manifest
freezes its exact hash and the full sorted path/length/source closure hash.
Do not reuse a previous bundle to claim dependency-solving improvements. The
`typing_scaling` namespace avoids standard-module names. Every diamond has
three actual source files, four import edges, and a leaf imported twice; the
entry adds one source. The loader's actual `SourceId`/`ModuleId` mapping and
solve counts must still be captured by instrumentation, with one solve per
canonical source. No across-edit cache result can replace those counts.

```sh
python3 bench/typing/scaling.py verify
python3 -m unittest discover -s bench/typing/scaling -p test_scaling.py
python3 bench/typing/scaling/check_grammar.py --xsht /absolute/baseline/xsht --output /tmp/xsh-scaling-grammar.json
```

`grammar-checks.json` records seven accepted three-unit annotated witnesses
checked with the frozen baseline binary. They establish the supported syntax,
module lookup and concrete operation relationships. They do not establish
full-size acceptance, future inference soundness, or scaling budgets. The
inferred parameter/requirement/module targets remain mandatory candidate
checks even where historical baseline rejection is expected. Small inferred
sources are deliberately not given credit for old compiler rejection.

Each manifest case defines its source unit, exact input bytes, lines and lexical
token count. Lexical tokens are a generator statistic, not parser AST nodes.
The normalized-output model counts necessary source-owned facts, records the
width of the common wide-record row and gives exact module edge/source
expectations. It avoids treating repeated aliases or expanded copies of a
shared type as useful output. Actual AST nodes, normalized type/row DAG nodes
and edges, solver work, family-specific work and unique retained typing,
constraint, reason and interface bytes remain explicit pending measurements.
Source-fact envelopes only prevent oversized input construction; they cannot
substitute for actual graph/resource counter checks.

All final `2k -> 4k` and `4k -> 8k` regular comparisons must pass the raw 2.6x
work and 2.5x retained type/constraint byte limits. Wall-time growth above 2.8x
requires investigation. Every applicable counter includes failed probes,
wakeups, effects, module solves and diagnostic work. A missing counter is
`not-run`; a regular guard hit is a failure. Adversarial cases require a local
rejection within the frozen work, depth, diagnostic-output, wall-time and RSS
guards. No generated source invokes host operations or reads ambient state.

Largest frozen regular closure: 6,503,344 source bytes. Largest source count:
24,001. The maximum regular row width is 8,000 labels. These stay below the
32 MiB source, 32,768 source and 20,000 label limits. Compiler graph/work
limits still require independent measured evidence. Full-size compiler,
candidate semantic, counter, timing and retained-memory runs have not run
as part of generator verification.
