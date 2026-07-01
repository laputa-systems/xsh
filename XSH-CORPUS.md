# XSH Corpus Design Notes

This note captures repeated patterns seen in the current `.xsh` corpus that may
signal language, standard library, or tooling deficiencies. Each section is
intended to be small enough for a fresh agent to pick up independently.

The corpus reviewed includes `examples/`, `showcase/`, `tools/`, `core/`,
`perf/`, and the XSH test fixtures. Generated and synthetic performance files
should be treated as weaker signal than applets, showcase scripts, and tools.

Self-contained applet boilerplate is intentionally not listed as a problem.
Core applets are expected to carry their own usage/error definitions rather
than depend on shared helpers.

## 1. CLI Parsing Escapes The Typed Parser

### Pattern

Newer scripts that use `cli.parse` are compact and declarative:

- `examples/typed-cli-options.xsh`
- `showcase/file-report.xsh`
- `showcase/rgrep.xsh`
- `showcase/wait-for.xsh`

However, many compatibility-oriented applets and a few showcase ports still
fall back to manual loops over `argv`:

- `core/rg.xsh`
- `core/cp.xsh`
- `core/mv.xsh`
- `core/sort.xsh`
- `core/fd.xsh`
- `core/head.xsh`
- `core/tail.xsh`
- `core/getty.xsh`
- `showcase/jq.xsh`
- `showcase/perf-collapse.xsh`

Common shapes:

```xsh
var index = 0

while index < argv.len() {
  let arg = argv[index]

  match arg {
    "-x" => flag = true
    "-n" => {
      index += 1
      value = argv[index]
    }
    _ => {
      if arg.starts_with("-n") and arg.count_chars() > 2 {
        value = arg.replace("-n", "")
      } else if arg.starts_with("--name=") {
        value = arg.replace("--name=", "")
      } else if arg.starts_with("-") {
        return Err(AppletError.Usage("unsupported option"))
      } else {
        operands = operands.push(arg)
      }
    }
  }

  index += 1
}
```

### Why It Matters

XSH's typed CLI parser is one of the cleanest parts of the corpus when the
command shape fits it. The repeated fallback to hand parsing suggests the
parser does not fully cover Unix applet compatibility. That pushes scripts back
toward stringly, index-heavy shell-style parsing exactly where XSH should feel
strong.

The main issue is not that applets need custom usage text. It is that ordinary
POSIX-compatible option forms require too much manual control flow.

### Candidate Improvements

Consider extending `cli.parse` or adding an applet-oriented parser mode that
supports:

- Short flag clusters such as `-abc`.
- Attached short-option values such as `-n2`, `-k2`, `-d,`, and `-Iinit`.
- Long-option assignment such as `--color=always`.
- `--` as an operands-only marker.
- Explicit operands-only mode after the first non-option, where appropriate.
- Flags that are accepted and ignored for compatibility.
- Mutually exclusive flags where later flags override earlier flags, such as
  `cp -n -f` or `sort -r`.
- Required option arguments with consistent usage errors.
- Command-like rest operands that must not be parsed as flags.

This could be a `cli.parse(..., compatibility: "posix")`, a separate
`cli.parse_argv`, or a lower-level option tokenizer that applets compose before
typing the result.

### Files To Read First

- `docs/STDLIB.md` for current `cli` API surface.
- `src/modules/signature/*` for the CLI module signature.
- `src/modules/*` and runtime module evaluation for parser implementation.
- `core/rg.xsh`, `core/cp.xsh`, `core/head.xsh`, `core/sort.xsh`,
  `core/fd.xsh` for representative compatibility needs.
- `examples/typed-cli-options.xsh` and `showcase/rgrep.xsh` for the desired
  declarative feel.

### Good Acceptance Test Shape

Add focused parser tests before migrating applets. The parser should be able to
represent cases like:

- `head -n2 file`
- `sort -nr -k2 -t, file`
- `fd -HI -e xsh -E target pattern root`
- `rg --color=always -efoo -g*.xsh root`
- `cp -n -f -t dest src1 src2`

Only after the API shape works should any applet be converted.

## 2. Collection Accumulation Beyond Comprehensions Is Verbose

### Pattern

Some corpus code still builds simple lists by declaring a mutable empty
collection and repeatedly reassigning it:

```xsh
var rows: List[Row] = []

for item in items {
  rows = rows.push(transform(item))
}
```

That shape should normally be a list comprehension:

```xsh
let rows = [transform(item) for item in items]
```

`lint.prefer-list-comp` already autofixes the narrow form where an empty list is
immediately followed by a `for` loop whose only statement pushes one value into
that same list. The remaining signal is the code this lint cannot safely rewrite:
accumulation with guards, multiple outputs, stateful conditions, or grouped/map
updates.

Map updates often use `get` with a default or a `match` on `get`:

```xsh
counts[key] = counts.get(key, 0) + 1
```

```xsh
match groups.get(key) {
  Ok(existing) => groups = groups.set(key, existing.push(row))
  Err(_) => groups[key] = [row]
}
```

Examples:

- `showcase/csv-query.xsh` builds rows from headers and fields.
- `showcase/px.xsh` accumulates matched rows, thread rows, ports by pid, and
  owner pid lists.
- `tools/xsh-ir-coverage.xsh` repeatedly accumulates names, reasons, scans, and
  report lines.
- `tools/cov-linux.xsh` builds LLVM argument lists.
- `core/ifup.xsh` builds packet byte chunks and DHCP option lists.

### Why It Matters

XSH pipelines and list comprehensions are pleasant for simple
filter/map/sort/group flows. They should absorb ordinary one-list transforms,
and the existing lint should keep expanding where the rewrite is mechanical and
obviously behavior-preserving.

The rougher case is incremental construction with side conditions, multiple
outputs, stateful deduplication, or grouped mutation. In those cases, local
mutation is clear but visually noisy: the business logic is interrupted by
repeated `var`, `.push`, `.extend`, `.get`, and `.set`.

This is not a request for broad abstraction. The likely gap is a small set of
ergonomic collection-building primitives that keep mutation local and explicit.

### Candidate Improvements

Investigate one or more of:

- Generalize `lint.prefer-list-comp` only for safe list rewrites, such as a
  single guarded push that maps directly to comprehension `if`.
- `ListBuilder[T]` and `MapBuilder[V]` APIs optimized for local accumulation
  where a comprehension or pipeline would be awkward.
- A concise `push` statement or in-place local append syntax for mutable lists
  only if builder or pipeline forms still leave common cases noisy.
- `map.update(key, default) { |value| ... }`.
- `map.push(key, value)` for `Map[List[T]]`.
- `count-by` or `freq-by` for common counting patterns.
- `index-by` for constructing `Map[T]` from records.
- `group-map` for grouping and transforming in one pass.

Keep any new API narrowly scoped. Existing pipelines, comprehensions, and
`lint.prefer-list-comp` already handle many collection transforms well.

### Files To Read First

- `docs/IDIOMS.md`, especially "Building maps".
- `docs/STDLIB.md` for collection helpers.
- `src/runtime/value.rs` and collection module implementations.
- `showcase/px.xsh`, especially the main matching and grouping flow.
- `tools/xsh-ir-coverage.xsh`, especially report scanning and rendering.
- `core/ifup.xsh` for low-level byte/list assembly.

### Good Acceptance Test Shape

Before changing syntax, prototype as library APIs and port one noisy but
contained call site. Good candidates:

- a simple list accumulation that should be eliminated by a generalized
  `lint.prefer-list-comp` autofix, if such a safe case remains in the corpus.
- `showcase/px.xsh` grouping threads by owner pid.
- `tools/cov-linux.xsh` `cov_args`.
- `examples/idiom-building-maps.xsh` as documentation for the final idiom.

The resulting code should be visibly shorter without hiding the control flow.

## 4. Small Structured Text Parsers Are Reimplemented

### Pattern

Many scripts parse lightweight formats with combinations of `split`, `fields`,
regex captures, and manual checks:

- `.env` files in `showcase/env-diff.xsh` and `showcase/dot-env-run.xsh`.
- Simple CSV in `showcase/csv-query.xsh`.
- `key=value` option and config lines in `core/ifup.xsh`, `core/env.xsh`, and
  other applets.
- Regex-based scan tools in `showcase/todo-scan.xsh`, `showcase/secret-scan.xsh`,
  and `showcase/parse-log.xsh`.
- Command-string splitting in `showcase/hyperfine.xsh`.

The current code is usually readable for simple cases, but it often has
intentional limitations:

```xsh
# Assumes simple CSV with no embedded commas in quoted fields.
```

or uses whitespace splitting where shell-like quoting would matter:

```xsh
let argv = text.fields()
```

### Why It Matters

XSH is supposed to be strong at crossing text and structured-data boundaries.
When common small formats are repeatedly implemented by hand, the language feels
less like a systems scripting environment and more like a collection of local
parsers.

This does not mean XSH should absorb every format. It does mean the standard
library should cover formats that appear naturally in system glue.

### Candidate Improvements

Consider targeted stdlib additions:

- `dotenv.read(path)` and `dotenv.parse(text)` returning typed key/value data.
- A minimal CSV reader/writer that handles quoted fields correctly.
- `str.split_once(sep)` for common key/value parsing.
- `str.cut_prefix(prefix)` and `str.cut_suffix(suffix)` returning a `Result` or
  optional pair.
- A documented `shlex`/argv parser recommendation for command strings.
- Regex helpers that avoid duplicate capture calls in pipelines, such as
  `where-let` style filtering or a `captures?` stage.

### Files To Read First

- `docs/STDLIB.md`.
- `docs/CHAPTER-06-text-bytes-hash.md`.
- `docs/CHAPTER-07-json-data.md`.
- Existing `shlex`, `ini`, `json`, and `regex` module implementations and
  tests.
- `showcase/csv-query.xsh`.
- `showcase/dot-env-run.xsh`.
- `showcase/env-diff.xsh`.
- `showcase/hyperfine.xsh`.
- `showcase/parse-log.xsh`.

### Good Acceptance Test Shape

Start with library tests and one showcase cleanup:

- `.env` parser handles blanks, comments, quoted values, and bare values.
- CSV parser handles quoted commas and escaped quotes.
- `split_once("=")` avoids accidental truncation or repeated split work.
- `showcase/env-diff.xsh` or `showcase/csv-query.xsh` becomes simpler while
  preserving its current behavior or intentionally documenting expanded
  behavior.

## 5. Parser/Interpreter-Sized Programs Become State-Heavy

### Pattern

Large showcase ports such as `showcase/jq.xsh` and `showcase/tokei.xsh` show
XSH can implement parser/interpreter-like work, but the code becomes noticeably
state-heavy:

- Byte-indexed scanners.
- Manual `while` loops.
- Explicit parser positions.
- Many temporary mutable booleans.
- Deep tag-union dispatch.
- Local performance comments explaining why higher-level list/string operations
  were avoided.

Examples:

- `showcase/jq.xsh` has a byte-indexed JSON input parser because char-list
  indexing was too expensive for large inputs.
- `showcase/tokei.xsh` implements language-specific comment/string scanners
  with many `while index < line_len` loops and scanner state booleans.
- `tools/xsh-ir-coverage.xsh` scans XSH source structure with manual delimiter
  tracking and near-duplicate pure/proc scanners.

### Why It Matters

This may not be a language-design flaw. `docs/CHAPTER-15-why-not-xsh.md`
explicitly says XSH is not trying to be a general application language. A jq
implementation, a tokei clone, or a source-code scanner is near or beyond that
boundary.

Still, these files are valuable stress tests. They reveal where XSH becomes
awkward when orchestration scripts grow into medium-sized tools with parsers.
The right response may be documentation that says "this is where you should
handoff to Rust/Python", but some small primitives could help without expanding
XSH's mission.

### Candidate Improvements

Investigate selectively:

- Byte/char scanner helper APIs that keep tight loops readable.
- A parser-combinator library only if it stays small and dependency-free.
- `for byte in bytes` / `for char in string` style iteration if performance is
  acceptable and semantics are clear.
- Efficient indexed sequence access documentation, including when `List.get`
  is inappropriate.
- Better source-scanning APIs for tools that inspect XSH itself, possibly
  exposing parser/AST data rather than requiring text heuristics.

### Files To Read First

- `docs/CHAPTER-15-why-not-xsh.md` to preserve the language boundary.
- `showcase/jq.xsh`, especially comments around byte-indexed parsing.
- `showcase/tokei.xsh`, especially slash-language scanning.
- `tools/xsh-ir-coverage.xsh`, especially pure/proc/script scanning.
- `docs/IR.md` if evaluating whether source scanning exists only for IR
  coverage tooling.

### Good Acceptance Test Shape

Do not start by redesigning syntax. First classify the pain:

- Which code is intentionally outside XSH's target domain?
- Which code is in-domain but missing small primitives?
- Which code exists only because internal parser/AST information is not exposed?

Good outcomes could be a doc update, a tiny scanner helper, or a replacement of
text heuristics with an existing internal API.

## Suggested Priority

1. CLI compatibility parsing.
2. Path/String boundary helpers.
3. Small structured text parsers.
4. Collection builder ergonomics.
5. Parser/interpreter stress-case triage.

The first two appear most central to XSH's stated purpose as systems glue. The
last one should be handled cautiously so XSH does not drift into becoming a
general application language.
