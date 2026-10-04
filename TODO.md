# TODO

## Reduce `.display()` drudgery without lossy Path→Str conversion

`Path` holds native bytes, and `.display()` is its lossy conversion to UTF-8
text. Automatic Path→Str conversion is rejected because it would lose bytes
silently. A survey of this repo plus `../packages` found ~1,160 lines that call
`.display()`, and most of them do not need a lossy conversion at all:

| Share | Use | Possible fix |
|---|---|---|
| ~1/3 | inside interpolation (`f"..."`, `fp"..."`, command words, `print`) | done: `lint.redundant-path-display` and `lint.path-constructor` autofixes, applied to all three repos |
| ~80 | comparison with a literal, e.g. `p.display() == "/repo/x"` | let a string literal take the `Path` type when the other side of `==` or `!=` is a `Path`, so you write `p == "/repo/x"`. This is the same rule as typed bindings and parameters, and it is lossless. |
| dozens | OS byte sinks: `process.command_argv(exe.display(), ...)`, `bytes.from_text(p.display())` | accept `Path` in argv, env, and process APIs, which carry OS bytes anyway; add a lossless `p.bytes()` |
| ~30 | text queries: `.display().starts_with/ends_with/split/replace` | Path methods: component-wise `starts_with`/`ends_with` and `ext`/`stem`/`name` coverage, so path logic stays on paths |
| rest | real text boundaries (JSON, `Str` params, string building) | keep `.display()`: this is the one explicit point where bytes can be lost |

Prior art:
- Rust has the same `.display()`, but `AsRef<Path>`/`AsRef<OsStr>` APIs mean
  callers rarely convert.
- Python's `os.PathLike` makes nearly every API accept `Path`, while f-strings
  convert silently (lossy).
- Go and Nushell treat paths as strings and accept the lossiness.

The lesson is to make sinks accept `Path` rather than add sugar. A shorter
alias (`.text`, `str(p)`) would rename the chore without removing it.

Open decisions:
- which of the rows above to adopt;
- whether literal-to-Path conversion also applies to `in`, `match` patterns,
  and map keys.

Each adopted row changes the language contract and goes into `docs/SPEC.md`
first.
