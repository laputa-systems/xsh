# Syntax audit

Phase A of the [syntax consolidation campaign](CAMPAIGN-SYNTAX-CONSOLIDATION.md). It lists what the language spells more than one way, which words are special only in one position, and which rules exist only to separate two forms. Nothing here is approved. Phase B removes nothing until the owner marks the list below.

## Summary

- The language has 39 reserved keywords, 25 contextual words named by SPEC 2.3 (33 once the grammar is read), 176 grammar productions, 74 `lint.prefer-*` lints, and a 7,107-line SPEC.
- Of the 74 `prefer-*` lints, 5 name a real duplicate that can become a check error, 10 need an owner choice, and 59 are idioms, annotations, or advice that are not second spellings of a form. Most of the 74 do not collapse the grammar.
- The grammar-sized wins are elsewhere: command-style standard-API calls (1 site), set literals (86 sites, none in xsh source or Laputa), `repeat N times` (190 sites), builder blocks (36 sites, a lookahead in 10 productions), bare path literals (986 sites), and `with ... else` (37 sites, none outside tests and one snippet).
- 20 of the 74 `prefer-*` lints report no site in either repository, and `lint.prefer-in` guards a spelling that no longer exists (no membership method is left in the API).
- The two largest clusters are policy: postfix `when`/`unless` (5,803 sites) against `guard` (1,141) against single-action `if` (3,984), and the direction in which the current lints push (`as` over `parse_int()?`, `e"NAME"` over `env.get`, `.Variant` over `Type.Variant`) against campaign rule 3.
- Five contextual words cannot be reserved without breaking thousands of identifiers (`at`, `ctx`, `range`, `main`, `task`); the 35 stage names after `|>` are contextual by construction. Rule 4 needs an owner-approved exception list or the redesign of the forms that use them.
- The ranked list has 37 items: 13 remove, 15 owner decision, 9 keep.

## Method and limits

Revisions counted: xsh `7e4586ee9dfd23e7ee8d3105330722a1d43d4b65` (master, clean tree) and Laputa `75b8701e33c421699e24a12e2fd8e8c67bc74c72` (master, clean tree). The corpus is every tracked `.xsh` file at those revisions: 2,137 in xsh and 602 in Laputa.

Excluded: 60 files under `dev/consolidation/evidence/` (frozen copies of other corpus files and probe output) and 7 documentation snippets that are meant to fail to parse (`docs/snippets/spec/rejected/`). Counted: 2672 files, 600,760 lines. No file in either repository is generated or vendored XSH.

Every site count is four numbers, `S/T/D/L`, with the total first:

- `S` xsh source: `core/`, `stdlib/`, `examples/`, `showcase/`, `tools/`, `dev/` (461 files).
- `T` xsh tests, fixtures, and benchmarks: `tests/`, any `tests/` or `fixtures/` directory, `bench/`, `dev/performance/` (1331 files).
- `D` documentation snippets under `docs/snippets/` (278 files). They show every form on purpose and are weak evidence of demand.
- `L` Laputa (602 files).

`S+L` is the production evidence; `T` mostly exercises features the SPEC defines.

Method. Syntax counts come from a small tool that links the `xsh` crate and calls `Parser::parse_source_arena_only` on each file, then walks the syntax tree with the sugar view of what the user wrote (no expansion, so nothing counts twice). It counts node kinds, and reads spelling-level differences from the node or from the source text at the node (for example whether a string starts with `r"` or a path with `p"`), never by searching the file. Words that look like keywords inside strings, comments, or command words are therefore not counted. Counts of the old spelling of a `prefer-*` lint come from `xsht lint --only` with every `lint.*` code, run on copies of the two repositories whose `xsht-config.ini` kept the module path and dropped every `exclude` and `[lint]` switch, so each rule saw each counted file. One site is one diagnostic.

Limits.

- Lint counts exist only for files that check cleanly; `xsht lint` skips files with check errors. A rule that needs a checked fact (types, effects) reports nothing in a file the checker rejects.
- `lint.prefer-inferred-pure-return`, `lint.prefer-inferred-proc-return`, and `lint.default-param-type` probe function bodies and ran far slower than every other rule; they were not measured and are marked so.
- A lint counts sites that match its pattern, which can be fewer than the sites a removal would migrate (it declines unsafe cases), or more (it reports advice that has no rewrite).
- The survivor count is a syntax count of the form the lint recommends, not of the sites that replaced the old form. "Not syntax-countable" means the form is defined by types or effects.
- Identifier counts for contextual words cover variables, bindings (let, parameters, patterns), and callable names. A word after a `.` is a member and stays legal when reserved (SPEC 2.4), as do record and argument labels, so those are not counted.
- Module-name words (`env`, `error`, `test`, `cli`, `codec`, `module`) show large variable counts because `env.get(...)` and `test.expect(...)` are references to the standard modules.
- Counts of grammar lookaheads are regular-expression counts over the grammar reference text and are approximate.

## Starting metrics

| Metric | Value |
|---|---|
| Reserved keywords (SPEC 2.3 and grammar reference) | 39 |
| Contextual words named by SPEC 2.3 | 25 |
| Contextual words in the grammar but not in SPEC 2.3 | 8 (`exit`, `without`, `main`, `exact`, `module`, `optional`, `range`, `task`) |
| Contextual words, total | 33 |
| Other position-bound vocabularies | 35 stage names after `\|>`; 4 run-form words (`status`, `text`, `bytes`, `capture`) and 3 run options; 7 effect names; 8 builtin type constructors in type position |
| Grammar productions (`docs/reference/grammar.md`) | 176 in 7 sections; 15 of them are `condition*` copies of expression productions |
| Grammar lookaheads (approximate) | about 94 negative (`!`), 20 positive (`&`), 217 no-space glue marks (`~`) |
| Sugar forms with a defined expansion (`SugarForm`) | 9: `repeat`, `when`, `unless`, `guard`, `fail`, `atomically`, for-index, `guard ... else fail`, `wait until` |
| `lint.prefer-*` lints | 74 (of 145 `lint.*` codes; 10 more `check.*` codes carry fixes) |
| SPEC.md lines | 7,107 |
| Grammar reference lines | 613 |
| Corpus | 2,672 files, 600,760 lines |

Phase B records the same table beside the final values in DESIGN.md.

## Inventory

Class: **core** is needed and has no second spelling; **sugar** expands to core forms with the same order, effects, and failures; **duplicate** has a surviving spelling that already exists.

### Reserved keywords

| Keyword | Forms it begins or joins | Class | Sites S/T/D/L |
|---|---|---|---|
| `and` | `a and b` | core | 9578 (6822/1721/10/1025) |
| `assert` | `assert COND, MSG` | core | 39984 (9/37194/39/2742) |
| `break` | `break`, `break VALUE` | core | 807 (622/99/0/86) |
| `const` | `const NAME = EXPR`; module-level `let` is the other spelling of inert data (`lint.prefer-const`) | core | 2673 (1289/335/123/926) |
| `continue` | `continue` | core | 1790 (1477/86/5/222) |
| `defer` | `defer ACTION`, `defer { }` | core | 1626 (255/1265/4/102) |
| `else` | clause of `if`, `guard`, `with`, and the `else =>` match arm | core clause | 10682 (7518/1632/41/1491) |
| `enum` | `enum NAME { A, B }`, `enum NAME: Str { A = "a" }` | core | 193 (105/29/13/46) |
| `errdefer` | `errdefer ACTION`; runs only when the block leaves with an error | core (capability; see removal list) | 3 (0/1/1/1) |
| `export` | `export` before a declaration | core | 4940 (2358/261/9/2312) |
| `false` | Bool literal (counted with `true`) | core | 21185 (12852/5607/91/2635) |
| `for` | `for X in SRC { }`, comprehension clause | core | 8672 (3725/3582/41/1324) |
| `guard` | `guard COND else { }` (sugar), `guard let T = E else { }` (core) | sugar + core | 1626 (715/368/8/535) |
| `if` | `if` statement, `if` expression, `if let`, comprehension filter | core | 25993 (19994/2310/49/3640) |
| `in` | `for X in`, `x in xs`, `x not in xs` | core | 16529 (5688/8221/45/2575) |
| `let` | `let NAME = EXPR`, `let {a, b} = R`, `if let`, `while let`, `guard let` | core | 80479 (19130/49106/478/11765) |
| `loop` | `loop { }` statement and expression | core | 95 (70/12/0/13) |
| `match` | statement and expression | core | 2580 (1052/522/36/970) |
| `not` | only in `not in`; negation is `!` | duplicate of `!(a in b)` | 1688 (762/760/2/164) |
| `null` | literal | core | 7750 (5293/1851/21/585) |
| `or` | `a or b` | core | 6965 (5733/283/6/943) |
| `proc` | `proc NAME(...) { }`, callable type `proc(...)` | core | 6854 (2461/1793/108/2492) |
| `pure` | `pure NAME(...) { }`, callable type `pure(...)` | core | 6555 (4249/1147/77/1082) |
| `retry` | `retry [delays] { }`, `retry backoff A..B within L { }` | core | 30 (1/24/4/1) |
| `return` | `return`, `return VALUE` | core | 12313 (8448/1098/31/2736) |
| `run` | `run`, `run.status`, `run.text`, `run.bytes`, `run.capture`, `run.stream` (segments counted) | core | 2953 (64/2361/85/443) |
| `spawn` | `spawn run ...`, `spawn COMMAND` | core | 355 (10/301/8/36) |
| `stream` | `stream NAME(...) { }` producers (declarations counted) | core | 67 (2/55/6/4) |
| `true` | Bool literal (counted with `false`) | core | 0 |
| `try` | `try { }` (blocks), `try run ...` (run forms) | core | 194 (29/134/8/23) |
| `type` | `type NAME = ...` (record schema, alias, module contract, bounded Int) | core | 3120 (1876/713/63/468) |
| `unless` | postfix guard only | sugar | 437 (239/44/7/147) |
| `use` | `use A.B as C` | core | 2375 (710/615/4/1046) |
| `var` | `var NAME = EXPR` | core | 10984 (8154/1200/19/1611) |
| `wait` | `wait HANDLE` (core); `wait until COND within LIMIT` (sugar, counted below) | core + sugar | 185 (6/161/5/13) |
| `when` | postfix guard only | sugar | 5366 (4419/280/16/651) |
| `while` | `while COND { }`, `while let` | core | 1994 (1460/244/0/290) |
| `with` | `with N = V { }` resource scope (core); `with N = V { } else { }` handler form | core + owner decision | 114 (1/72/5/36) |
| `yield` | `yield VALUE`, `yield @LIST` | core | 2062 (1456/371/13/222) |

### Contextual words

The last column is the cost of reserving the word: uses as an ordinary identifier (variable, binding, callable name). Labels and members are legal for reserved words and are not counted.

| Word | Special only in | Class | Keyword-form sites S/T/D/L | Ordinary-identifier uses S/T/D/L (variable, binding, callable name) |
|---|---|---|---|---|
| `as` | between a value and a type (conversion); `use M as N`; `PATTERN as NAME`; end of `atomically replace DEST as NAME` | core (one word, four roles) | 2415 (511/1351/20/533) | 4 (0/4/0/0) |
| `at` | `tempdir NAME at PATH` | sugar | 51 (0/21/5/25) | 3563 (3447/88/0/28) |
| `atomically` | head of `atomically replace DEST as NAME { }` | sugar | 34 (0/19/2/13) | 3 (0/3/0/0) |
| `backoff` | after `retry` and after `wait until ... within L` | sugar clause | 11 (0/9/2/0) | 14 (10/2/2/0) |
| `because` | after the first operand of `fail` | sugar clause | 3 (0/2/1/0) | 0 |
| `cd` | core command `cd PATH { }`; call form `cd (PATH) { }` | core, two spellings | 168 (11/108/4/45) | 0 |
| `cli` | `cli main(...) { }` | core | 54 (2/8/9/35) | 469 (207/254/1/7) |
| `collect` | `collect { }` before a brace | core | 1222 (862/222/4/134) | 14 (12/2/0/0) |
| `ctx` | `ctx DESCRIPTION { }` | core | 28 (6/17/3/2) | 41628 (1413/38278/7/1930) |
| `env` | core command `env A=B { }`; call form `env (OVERLAY) { }` | core, two spellings | 320 (18/119/10/173) | 519 (205/253/10/51) |
| `eprint` | statement head | core | 382 (305/5/5/67) | 0 |
| `error` | `error NAME = ...` declarations | core | 283 (79/77/17/110) | 1405 (243/719/29/414) |
| `every` | clause of `wait until` | sugar clause | 21 (0/8/1/12) | 4 (0/4/0/0) |
| `exact` | `type N = exact module { }` | core | 6 (0/5/1/0) | 173 (16/154/1/2) |
| `exit` | `exit STATUS` statement | core | 1355 (1314/21/5/15) | 2 (2/0/0/0) |
| `fail` | `fail MESSAGE` statement | sugar | 516 (186/38/11/281) | 65 (48/17/0/0) |
| `is` | `value is PATTERN`, `is Type` patterns | core | 1183 (230/867/25/61) | 0 |
| `main` | `cli main` | core | 54 (2/8/9/35) | 750 (305/128/12/305) |
| `module` | `type N = module { }` contracts | core | 58 (10/29/3/16) | 223 (34/169/4/16) |
| `nominal` | `nominal type` | core | 5 (0/2/3/0) | 10 (0/10/0/0) |
| `on` | `on SIGNAL { }`; `retry ... on (PATTERN)` | core | 43 (22/16/5/0) | 16 (4/12/0/0) |
| `optional` | `export optional` module-contract entries | core | 26 (0/7/2/17) | 19 (10/9/0/0) |
| `print` | statement head | core | 1847 (723/320/283/521) | 3 (3/0/0/0) |
| `range` | `type N = Int range LOW..HIGH` | core (also a stage and a function) | 12 (1/7/3/1) | 1869 (1015/762/3/89) |
| `repeat` | `repeat N times { }` | sugar | 190 (107/81/1/1) | 45 (9/34/0/2) |
| `replace` | `atomically replace` | sugar clause | 34 (0/19/2/13) | 38 (23/5/0/10) |
| `task` | `task NAME { }` entry inside a builder block | core of builder blocks | 0 | 329 (11/8/4/306) |
| `tempdir` | `tempdir NAME { }` | core | 76 (3/31/10/32) | 24 (0/24/0/0) |
| `test` | `test NAME { }` | core | 16148 (0/15403/5/740) | 9748 (0/8556/3/1189) |
| `times` | `repeat N times` | sugar clause | 190 (107/81/1/1) | 20 (12/8/0/0) |
| `until` | directly after `wait` | sugar clause | 27 (0/13/2/12) | 2 (0/2/0/0) |
| `within` | `within DURATION { }`; clause of `wait until`, `retry backoff` | core + sugar clause | 68 (0/50/6/12) | 36 (30/6/0/0) |
| `without` | `without EFFECT { }` | core | 9 (0/6/3/0) | 33 (0/32/0/1) |

### Operators and punctuation

| Operator or punctuation | Meaning | Class | Sites S/T/D/L |
|---|---|---|---|
| `+ - * / %` | arithmetic (`+` also concatenates) | core | 16062 (10795/3774/46/1447) |
| unary `-` | negation | core | 1923 (1227/514/3/179) |
| `!` | Bool negation | core | 7201 (3840/2329/16/1016) |
| `== !=` | equality | core | 50764 (17999/27030/87/5648) |
| `< <= > >=` | ordering | core | 9814 (7521/1148/21/1124) |
| `a < b < c` | ordering chain; middle operand evaluates once | sugar for `a < b and b < c` | 27 (4/23/0/0) |
| `and`, `or` | logical | core | 16543 (12555/2004/16/1968) |
| `in` | membership | core | 7857 (1963/4639/4/1251) |
| `not in` | negated membership | sugar for `!(a in b)` | 1688 (762/760/2/164) |
| `??` | fallback for `Result`/optional | core | 5391 (3752/806/39/794) |
| `?` suffix | propagate a failure | core | 65226 (7898/46432/236/10660) |
| `?.` | null-safe field | core | 5624 (400/4589/21/614) |
| `?[ ]` | null-safe index or slice | core | 66 (0/58/4/4) |
| `.name`, `.name(...)` with no receiver | item of the innermost callback; also target-typed variant when capitalized | core + duplicate (see removal list) | 3404 (1346/1695/62/301) |
| `\|` and `&` on sets | union and intersection | core | 23 (0/18/5/0) |
| `as TYPE` | conversion that propagates its failure | core (see D08) | 1358 (225/1061/18/54) |
| `is PATTERN` | pattern test | core | 897 (157/679/9/52) |
| `\|>` | pipeline into a stage or a value call | core | 2578 (950/1325/39/264) |
| `.. ` | half-open slice (`a[1..3]`); bounded Int; rest pattern; backoff range | core | 1468 (957/410/8/93) |
| `...` | record spread, named-argument spread, rest parameter | core | 1906 (948/645/9/304) |
| `@` | splice a list into a list literal, call, or command | core | 219 (21/118/13/67) |
| `=>` | match arm | core | 9064 (5233/1302/99/2430) |
| `;` | statement separator (a newline does the same) | duplicate separator spelling | 2153 (1493/655/0/5) |
| `$name`, `${expr}` | interpolation in command words | core (see Commands) | 4035 (52/3154/25/804) |
| `$?` | status of the last run (grammar reference only; not in the SPEC) | duplicate of `run.status` | 1 (0/1/0/0) |
| `> >> < 2> 2>> >&` | redirection | core | 257 (6/206/10/35) |
| `E>`, `E>>` | legacy stderr redirection; the lexer still tokenizes them to report `parse.legacy-stderr-redirection` | already an error | 0 |

### Statements and declarations

| Form | Spellings | Class | Sites S/T/D/L |
|---|---|---|---|
| binding | `let`, `const`, `var`; `let {a, b} = r` destructuring; `guard let` | core | 80479 (19130/49106/478/11765) |
|   of which `let` with an annotation | `let x: T = ...` | core (optional annotation) | 2253 (925/987/60/281) |
|   of which record destructuring | `let {a, b, ..} = r` | core | 524 (60/427/14/23) |
| assignment | `=` | core | 12817 (9720/1206/16/1875) |
| compound assignment | `+= -= *= /= %=` | sugar for `x = x OP y` | 8900 (6904/698/16/1282) |
| `proc` declaration | `proc NAME(...) [effects] -> T { }` | core | 6854 (2461/1793/108/2492) |
|   of which with a written effect clause | `[fs, error]` | core (inferable) | 4941 (2111/1495/76/1259) |
|   of which with a written return type | `-> T` | core (inferable) | 4675 (1755/1306/59/1555) |
| `pure` declaration | `pure NAME(...) -> T { }` | core | 6555 (4249/1147/77/1082) |
| `stream` declaration | `stream NAME(...) { yield ... }` | core | 67 (2/55/6/4) |
| `test` declaration | `test NAME { \|ctx\| ... }` | core (test harness) | 16148 (0/15403/5/740) |
| `cli main` entry | `cli main [sub] (params) { }` | core | 54 (2/8/9/35) |
| signal hook | `on SIGNAL [effects] { }` | core | 37 (22/13/2/0) |
| `export` | `export proc\|pure\|stream\|let\|const\|type\|enum\|error` | core | 4940 (2358/261/9/2312) |
| `use` | `use a.b`, `use a.b as c` | core | 2375 (710/615/4/1046) |
| record schema | `type N = {f: T = default}` | core | 2978 (1842/644/41/451) |
| type alias | `type N = T` | core | 84 (24/40/19/1) |
| module contract | `type N = module { }`, `exact module { }` | core | 58 (10/29/3/16) |
| bounded Int | `type N = Int range LOW..HIGH`, `..=` | core | 12 (1/7/3/1) |
| `enum` | `enum N { A, B(T) }`, `enum N: Str { A = "a" }` | core | 193 (105/29/13/46) |
| legacy tag union | `type N = A \| B` | already an error (`parse.enum-migration`) | 0 |
| `error` declaration | `error N = A \| B` (pipe); `error N { A newline B }` (braces) | duplicate spelling | 283 (79/77/17/110) |
|   pipe spelling | `error N = A(f: T) \| B` | survivor candidate | 253 (68/66/12/107) |
|   brace spelling | `error N { A newline B }` | duplicate | 30 (11/11/5/3) |
| `if` statement | `if`, `else if`, `else` | core | 20780 (16030/1609/38/3103) |
|   `if let PATTERN = EXPR` | pattern condition | core (binds) | 2158 (1785/279/6/88) |
|   `if COND { exit-statement }` with no else | single `return`/`break`/`continue`/`exit`/`yield`/`fail` (`lint.prefer-guard`) | duplicate of `guard` and postfix guards | 3984 (2686/99/6/1193) |
| `if` expression | `if C { a } else { b }` | core | 5213 (3964/701/11/537) |
| `while`, `while let` |  | core | 1994 (1460/244/0/290) |
| `for X in SRC` |  | core | 8672 (3725/3582/41/1324) |
| `for INDEX, ITEM in SRC` | index binding | sugar for `for {index: i, value: v} in SRC \|> enumerate()` | 19 (1/11/1/6) |
| `loop` | statement and expression | core | 95 (70/12/0/13) |
| `repeat N times { }` |  | sugar for `for _ in range(N)` | 190 (107/81/1/1) |
| `match` statement and expression | `PATTERN => body`, `else => body`, `_ => body` | core | 2580 (1052/522/36/970) |
|   `else =>` arm | catch-all | survivor | 614 (456/104/9/45) |
|   `_ =>` or `_ if COND =>` arm | wildcard (counts guarded arms too) | duplicate when unguarded | 91 (15/28/1/47) |
| `guard COND else { }` |  | sugar for `if COND {} else { }` | 1141 (373/230/5/533) |
| `guard COND else fail MSG` |  | sugar on sugar | 3 (0/2/1/0) |
| `guard let T = E else { }` |  | core (binds into the enclosing block) | 485 (342/138/3/2) |
| postfix `when` | `return X when C`, also `continue break yield exit fail`, expression, assignment, `print` | sugar for `if C { STMT }` | 5366 (4419/280/16/651) |
| postfix `unless` | same statements | sugar for `if C {} else { STMT }` | 437 (239/44/7/147) |
| `fail MSG` |  | sugar for `return Err(error.failure(MSG))` | 516 (186/38/11/281) |
| `fail MSG because CAUSE` |  | sugar for `return Err(error.failure(MSG), cause: CAUSE)` | 3 (0/2/1/0) |
| `exit STATUS` |  | core | 1355 (1314/21/5/15) |
| `return` | bare, value, run form | core | 12313 (8448/1098/31/2736) |
|   `return Err(...)` | explicit error return | core | 3980 (2622/60/9/1289) |
| `break`, `continue` | `break VALUE` in `loop` expressions | core | 2597 (2099/185/5/308) |
| `yield`, `yield @LIST` |  | core | 2062 (1456/371/13/222) |
| `assert COND, MSG` |  | core | 39984 (9/37194/39/2742) |
| `defer`, `errdefer` | action or block | core | 1629 (255/1266/5/103) |
| `try { }` | capture failures as `Result` | core | 141 (27/95/4/15) |
| `try run ...` | capture one run form | core | 53 (2/39/4/8) |
| `retry [delays] { }` |  | core | 24 (1/19/3/1) |
| `retry backoff A..B within L { }` |  | sugar for a computed delay list | 6 (0/5/1/0) |
| `ctx DESCRIPTION { }` |  | core (adds an error context frame) | 28 (6/17/3/2) |
| `collect { yield ... }` |  | core | 1222 (862/222/4/134) |
| `wait HANDLE`, `wait [HANDLES]` |  | core | 185 (6/161/5/13) |
| `wait until COND within L [every I \| backoff A..B]` |  | sugar (seven-line expansion) | 27 (0/13/2/12) |
| `spawn run ...`, `spawn COMMAND` |  | core | 355 (10/301/8/36) |
| `cd PATH { }` | command spelling (statement only) | duplicate of `cd (PATH) { }` | 115 (4/65/2/44) |
| `cd (PATH) { }` | call spelling (also a value) | core | 53 (7/43/2/1) |
| `env A=B { }` | command spelling (statement only) | duplicate of `env (OVERLAY) { }` | 127 (13/65/6/43) |
| `env (OVERLAY) { }` | call spelling (also a value) | core | 193 (5/54/4/130) |
| `tempdir NAME { }` |  | core | 76 (3/31/10/32) |
| `tempdir NAME at PATH { }` |  | core extension | 51 (0/21/5/25) |
| `within DURATION { }` | deadline scope | core | 35 (0/32/3/0) |
| `without EFFECT, ... { }` | negative effect bound | core | 9 (0/6/3/0) |
| `with N = V { }` | resource scope | core | 77 (1/36/4/36) |
| `with N = V { } else { }` | grouped fallible bindings with one handler | distinct form; same head as the resource scope | 37 (0/36/1/0) |
| `atomically replace DEST as NAME { }` |  | sugar (seven-line expansion) | 34 (0/19/2/13) |
| builder block | `process.command { cwd = ...; run ... }` | duplicate of `process.command_argv(..., named args)` | 36 (1/28/7/0) |
| command statements | `print`, `eprint` (words and typed arguments) | core | 2229 (1028/325/288/588) |
| `run` statement | `run ...` with optional `?` | core | 575 (11/329/46/189) |
| command-style standard API | `json.write out.json $x --pretty` | duplicate of `json.write(...)` | 1 (0/0/1/0) |

### Expressions and literals

| Form | Spellings | Class | Sites S/T/D/L |
|---|---|---|---|
| string | `"..."` | core | 218958 (45717/145521/499/27221) |
| raw string | `r"..."`, `r"""..."""` | core | 2717 (7/2645/0/65) |
| triple-quoted string | `"""..."""` | core | 4334 (128/3797/10/399) |
| f-string | `f"{expr}"`, `f"""..."""` | core | 15026 (6959/4799/167/3101) |
| path literal | `p"..."` | core | 8650 (706/3913/105/3926) |
| bare path literal | `/abs`, `./rel`, `../up` in expression position | duplicate of `p"..."` | 986 (201/321/18/446) |
| interpolated path | `fp"{expr}/x"` | core | 14989 (1640/9057/73/4219) |
| glob | `g"*.xsh"` | core | 15 (0/5/0/10) |
| bytes | `b"..."` | core | 9463 (1289/8056/11/107) |
| regex literal | `rx"..."` | sugar for `regex.compile` with check-time validation | 699 (354/279/3/63) |
| environment string | `e"NAME"` read, `e"NAME" = v` write | duplicate of `env.get("NAME")` | 288 (20/61/12/195) |
| integer | decimal, `0o755` octal | core | 88610 (39239/41949/313/7109) |
| size literal | `64MiB`, `4KB` | sugar for a product of literals | 86 (0/59/6/21) |
| duration literal | `250ms`, `30s`, `2m`, `1h` | core | 2302 (39/1990/43/230) |
| float | `1.5e6` | core | 772 (491/275/4/2) |
| list literal | `[a, @b, c]` | core | 49721 (9857/33710/118/6036) |
| list comprehension | `[x for ... if ...]` | core | 2274 (742/1097/11/424) |
| record literal | `{a: 1, b}`, `{...r, a.b: 2}` | core | 22491 (8303/10162/101/3925) |
|   field `name: v` | labeled | core | 57683 (23991/23408/180/10104) |
|   field `"name": v` quoted label | quoted label | needed for non-identifier keys | 512 (224/248/3/37) |
|   field `name` (shorthand) | punned | sugar for `name: name` | 928 (120/88/10/710) |
|   field `a.b: v` (path) | functional update | core | 43 (2/40/1/0) |
|   field `[k]: v` (computed, makes a map) |  | core | 79 (1/69/4/5) |
| map comprehension | `{[k]: v for ...}` | core | 73 (5/22/4/42) |
| set literal | `{"a", "b"}` | duplicate of `set.from([...])` | 82 (0/73/9/0) |
| set comprehension | `{x for ...}` | duplicate of `set.from([x for ...])` | 4 (0/3/1/0) |
| call, positional argument | `f(x)` | core | 307065 (73725/192058/554/40728) |
| call, named argument | `f(name: x)` | core | 27485 (5388/16953/102/5042) |
| call, punned argument | `f(name:)` | sugar for `f(name: name)` | 1075 (568/278/21/208) |
| call, named spread | `f(...record)` | core | 49 (7/39/2/1) |
| call, splice | `f(@list)` | core | 58 (4/6/2/46) |
| index | `xs[i]`, `xs[-1]` | core | 10788 (5409/4221/34/1124) |
|   negative index | `xs[-1]` | core | 233 (147/57/2/27) |
| slice | `xs[a..b]`, `xs[a..]`, `xs[..b]` | core | 701 (510/143/2/46) |
| callback item | `.`, `.field` inside a callback block | core | 3404 (1346/1695/62/301) |
| callback parameter | `{ \|x\| ... }` | core | 14946 (307/13977/33/629) |
| target-typed variant | `.Name`, `.Name(args)`, `pat .Name` | duplicate of `Type.Name` | 1620 (682/786/34/118) |
| pattern condition | `if let`, `while let` | core | 2433 (1987/338/8/100) |
| pattern test | `x is P` | core | 897 (157/679/9/52) |
| conversion | `x as T`, `.require(T)` | core | 3464 (1043/1920/37/464) |
| value pipeline | `x \|> f(_, y)` | core | 180 (70/82/1/27) |
| structured pipeline | `xs \|> where ... \|> map ...` | core | 2398 (880/1243/38/237) |

### Command words and run forms

| Command-word or run form | Sites S/T/D/L | Note |
|---|---|---|
| `run` words: bare text | 5998 (136/5340/154/368) | core |
| `run` words: quoted | 2338 (18/1428/10/882) | core |
| `run` words: `$name` | 2981 (49/2122/24/786) | core |
| `run` words: `${expr}` | 1054 (3/1032/1/18) | overlaps the typed forms below |
| `run` words: mixed bare and interpolated | 41 (0/37/2/2) | core |
| `run` typed arguments: `fp"..."`, `f"..."`, `p"..."` | 1099 (6/934/2/157) | overlap `${expr}` words |
| `run` typed arguments: calls, fields, other | 31 (0/7/0/24) | typed `(expr)` arguments |
| `run` splices `@name`, `@(expr)` | 161 (17/112/11/21) | core |
| `print` words and typed arguments | 2160 (752/362/328/718) | f-strings are 838 of these |
| `print` with `$name` | 261 (52/60/75/74) | overlaps `f"{name}"` |
| `print` with `${expr}` | 133 (11/69/42/11) | overlaps `f"{expr}"` |
| `print` with a typed f-string | 838 (518/93/104/123) | dominant spelling |
| `run` plain / `run.status` | 742 (17/435/59/231) | core |
| `run.text`, `run.bytes` | 931 (26/721/23/161) | core |
| `run.capture --text\|--bytes` | 1256 (21/1184/3/48) | core |
| `run.stream --text\|--bytes` | 24 (0/21/0/3) | core |
| byte pipeline `run a \| run b` | 43 (0/38/4/1) | core |
| run options `--timeout= --cpumax= --accept=` | 112 (5/93/10/4) | core |
| redirections `> >> < 2> 2>> >&` | 257 (6/206/10/35) | core |

### Stream stages

A `|>` stage that begins with one of these names is a stream stage; any other stage is an expression. The List type has no `map`, `filter`, or `sort` method, so stages are the only spelling of these operations.

| Stage | Argument form | Sites S/T/D/L |
|---|---|---|
| `where` | block or inline | 579 (175/316/15/73) |
| `map` | block or inline | 233 (70/140/9/14) |
| `par-map` | block or inline | 101 (14/75/3/9) |
| `each` | block or inline | 13 (0/13/0/0) |
| `batch` | block | 24 (0/24/0/0) |
| `sort` | none | 180 (56/85/1/38) |
| `sort-by` | block or inline | 536 (405/73/11/47) |
| `take` | none | 123 (67/39/4/13) |
| `drop` | none | 151 (103/24/0/24) |
| `first` | none | 65 (0/64/1/0) |
| `last` | none | 5 (0/5/0/0) |
| `unique-by` | block or inline | 21 (7/13/1/0) |
| `enumerate` | none | 37 (34/3/0/0) |
| `zip` | none | 9 (0/9/0/0) |
| `range` | none | 15 (0/15/0/0) |
| `repeat` | none | 28 (7/21/0/0) |
| `tee` | block or inline | 12 (1/11/0/0) |
| `sum` | none | 22 (10/11/1/0) |
| `min` | none | 13 (2/11/0/0) |
| `max` | none | 11 (3/8/0/0) |
| `group-by` | block or inline | 27 (10/15/1/1) |
| `fold` | block | 29 (1/25/3/0) |
| `reduce` | block | 7 (0/5/2/0) |
| `flat-map` | block or inline | 22 (4/17/1/0) |
| `any` | block or inline | 276 (21/229/0/26) |
| `all` | block or inline | 18 (1/14/0/3) |
| `shuffle` | none | 7 (0/7/0/0) |
| `table.print` | none | 6 (1/5/0/0) |
| `text.lines` | none | 6 (0/6/0/0) |
| `bytes.chunks` | none | 4 (0/4/0/0) |
| `json.lines` | none | 8 (1/7/0/0) |
| `json.stream` | none | 4 (0/4/0/0) |
| `count` | block | 118 (12/86/7/13) |
| `collect` | block | 256 (63/175/1/17) |
| `reduce-by` | block | 54 (4/50/0/0) |

## The 74 `prefer-*` lints

Kinds. **syntax**: both spellings are grammar forms or literals. **api**: both are library entry points. **shape**: the old form is built from general constructs, so making it an error would add a rule instead of removing one. **annotation**: explicit against inferred. **advice**: the two are not equivalent.

Actions. **remove** promotes the old spelling to a check error whose diagnostic shows the rewrite, and deletes the lint. **keep** leaves a lint. **owner decision** is in the ranked list below.

"Fix when proved" is `FixPolicy::ProducerProof`: `--fix` rewrites a site only when checked facts show the rewrite preserves behavior, and declines otherwise. Where this table says mechanical it means this and nothing stronger.

| Lint (`lint.prefer-` +) | Kind | Old spelling | Proposed survivor | Default | Fix | Old sites S/T/D/L (from lint) | Survivor sites S/T/D/L (from syntax) | Action | Why |
|---|---|---|---|---|---|---|---|---|---|
| `atomically-replace` | shape | manual `fs.temp_sibling` or temp path, `defer remove`, `rename(to: DEST, overwrite: true)` | `atomically replace DEST as NAME { }` | on | fix when proved | 1 (0/0/0/1) | 34 (0/19/2/13) | keep | the manual form is five or more ordinary lines; the sugar earns its place by the one-extra-line budget. Its contextual words are D03 |
| `bare-field-label` | syntax | `{"name": v}` with an identifier-shaped label | `{name: v}` | on | fix when proved | 36 (3/0/0/33) | 57171 (23767/23160/177/10067) | remove | C12: promote to a check error; quoted labels stay for non-identifier keys |
| `block-string` | shape | constant `"a\n" + "b\n"` concatenation | `"""block"""` | on | fix when proved | 2 (0/2/0/0) | 4334 (128/3797/10/399) | keep | the old shape is an ordinary `+` of literals; an error would single out one use of an operator |
| `callable-alias` | shape | a function that only forwards its arguments | immutable alias `let f = g` | on | fix when proved | 11 (2/2/0/7) | not syntax-countable | keep | equivalence needs the checker (same signature, defaults, effects); not a syntax pair |
| `comparison-chain` | shape | `a < b and b < c` | `a < b < c` | on | fix when proved | 3 (0/3/0/0) | 27 (4/23/0/0) | owner decision | D10: the chain itself is sugar (27 sites) |
| `const` | syntax | module-level `let` of inert data | `const` | on | fix when proved | 328 (0/78/1/249) | 2673 (1289/335/123/926) | keep | the two differ: `const` is prepared at check time under restrictions; the lint recommends, it does not equate |
| `context-scope-value` | shape | scope assigns a placeholder declared outside | scope yields the value | on | fix when proved | 0 | 71 (5/56/6/4) | keep | idiom; the old shape is general code |
| `defer-block` | shape | single-use cleanup helper | `defer { }` | on | fix when proved | 0 | 1626 (255/1265/4/102) | keep | idiom |
| `empty-map-literal` | syntax | `map.empty()` | `{}` in a map-typed context | on | fix when proved | 17 (0/1/0/16) | 63 (15/7/2/39) | keep | `{}` is only a map where the context says so; `map.empty()` stays for uninferred contexts |
| `env-string` | syntax | `env.get("NAME")` | `e"NAME"` | on, off in this repo config | fix when proved | 93 (48/33/1/11) | 288 (20/61/12/195) | owner decision | D08: direction conflicts with rule 3 (method over syntax); off in this repository's config |
| `fail` | advice | private message-only error family | `fail MESSAGE` | on | fix when proved | 57 (4/6/1/46) | 516 (186/38/11/281) | keep | not equivalent: changes nominal identity |
| `file-lines` | api | `read_text()?.lines()` in a loop | `path.lines()?` | on | no fix | 6 (0/4/0/2) | 591 (165/325/4/97) | keep | no fix; the two differ in memory behavior |
| `fs-files` | api | `fs.walk()` filtered to files | `fs.files()` | on | fix when proved | 1 (0/1/0/0) | 89 (23/48/10/8) | keep | `fs.walk` stays for other kinds |
| `generic-record-constructor` | shape | constructor through a concrete alias | constructor by the generic name | on | fix when proved | 0 | not syntax-countable | keep | checker-dependent |
| `guard` | syntax | single-action `if COND { return }` | `guard !COND else { return }` (or a postfix guard) | on | fix when proved | 469 (173/38/0/258) | 1141 (373/230/5/533) | owner decision | D01/D02: three spellings of an early exit |
| `implicit-message` | syntax | variant declared with an explicit `message: Str` payload | variant with no payload; message passed positionally | on | fix when proved | 49 (0/30/7/12) | not syntax-countable | owner decision | C13 candidate: depends on the count below; the implicit message is a built-in special case (SPEC 4.10) |
| `in` | api | a membership method call (no such method exists any more: `contains` is missing from `Str`, `List`, `Map`, and `Set`) | `x in xs` | on | fix when proved | 0 | 9545 (2725/5399/6/1415) | remove | the old spelling is gone from the API and has no site; delete the dead lint. `in` itself is core |
| `inferred-private-effects` | annotation | private proc with an effect clause equal to the inferred one | omit the clause | on, off in this repo config | fix when proved | 2184 (997/925/42/220) | 1913 (350/298/32/1233) | keep | an annotation is not a second spelling; an error would break on every inference change. Off in this repository's config |
| `inferred-pure-return` | annotation | private pure function return type equal to the inferred one | omit the type | opt-in | fix when proved | not measured | 177 (3/165/9/0) | keep | as above; opt-in |
| `inferred-variant` | syntax | `Kind.Symlink` | `.Symlink` (target-typed) | on | fix when proved | 667 (26/235/13/393) | 1620 (682/786/34/118) | owner decision | D08: two spellings of a variant; the leading dot also collides with the item `.name` |
| `item-shorthand` | syntax | `{ \|hit\| hit.url }` | `{ .url }` | on, off in this repo config | fix when proved | 146 (66/64/7/9) | 3404 (1346/1695/62/301) | keep | both are needed: nested callbacks need names. Off in this repository's config |
| `known-field-access` | api | `row.get("type")?` | `row.type` | on | fix when proved | 0 | not syntax-countable | keep | checker-dependent (field guaranteed present) |
| `list-comp` | shape | loop that only builds a list | comprehension | on | fix when proved | 40 (3/11/0/26) | 2274 (742/1097/11/424) | keep | idiom |
| `list-compound-assignment` | shape | `xs = xs + [x]` | `xs += [x]` | on | fix when proved | 33 (9/24/0/0) | 8424 (6521/664/16/1223) | keep | `+=` is the sugar form that earns its place (8,900 sites) |
| `list-element-assignment` | shape | rebuild the list to change one element | `xs[i] = v` | on | fix when proved | 0 | not syntax-countable | keep | idiom; needs index validity |
| `list-pattern` | shape | length check then indexing | `if let [a, b] = xs` | on | fix when proved | 3 (0/2/0/1) | 104 (20/69/3/12) | keep | idiom |
| `list-splicing` | shape | `a + b + c` | `[@a, @b, @c]` | on | fix when proved | 13 (0/6/0/7) | not syntax-countable | keep | idiom |
| `map-comp` | shape | loop that only builds a map | map comprehension | on | fix when proved | 3 (2/0/0/1) | 73 (5/22/4/42) | keep | idiom |
| `map-entry-iteration` | shape | iterate keys and look each value up | iterate entries | on | fix when proved | 0 | not syntax-countable | keep | idiom |
| `map-literal` | shape | `map.empty()` then inserts | one literal | on | fix when proved | 0 | not syntax-countable | keep | idiom |
| `match-else` | syntax | `_ =>` as a whole arm | `else =>` | on | fix when proved | 90 (15/28/0/47) | 614 (456/104/9/45) | remove | C04: the SPEC already says the catch-all has one spelling |
| `method` | api | `module.func(receiver, ...)` | `receiver.func(...)` | on | fix when proved | 0 | not syntax-countable | remove | C14: precedent `check.removed-fs-function`; rule 3 |
| `named-argument-pun` | syntax | `f(name: name)` | `f(name:)` | on | fix when proved | 368 (95/33/0/240) | 1075 (568/278/21/208) | owner decision | D09: saves characters, not lines |
| `named-argument-spread` | syntax | `f(a: r.a, b: r.b)` | `f(...r)` | on | fix when proved | 5 (0/3/0/2) | 49 (7/39/2/1) | keep | forwards fields without naming them; the field list needs types |
| `nested-record-update` | syntax | nested `{...r, a: {...r.a, b: 1}}` | `{...r, a.b: 1}` | on | fix when proved | 3 (1/0/0/2) | 43 (2/40/1/0) | keep | the old shape is general record spread |
| `optional-binding` | shape | exiting null test then rebinding | `guard let` | on | fix when proved | 1 (1/0/0/0) | 485 (342/138/3/2) | keep | idiom |
| `optional-postfix` | shape | explicit null branch | `?.` with `??` | on | fix when proved | 0 | 5624 (400/4589/21/614) | keep | idiom |
| `positional-constructor` | syntax | `Entry(path: p, kind: k)` | `Entry(p, k)` | on | fix when proved | 168 (1/3/10/154) | not syntax-countable | owner decision | D09: two argument spellings for one constructor; the lint applies only where no two fields can hold the same value |
| `propagation` | shape | `match r { Err(e) => return Err(e) ... }` | `r?` | on | fix when proved | 1 (1/0/0/0) | 65226 (7898/46432/236/10660) | keep | idiom |
| `read-lines` | api | `read_text()?.lines()` | `Path.read_lines()?` | on | fix when proved | 13 (0/7/0/6) | 100 (7/70/4/19) | keep | idiom |
| `record-constructor` | syntax | record literal of a schema type | `Schema(field: v)` | on | fix when proved | 87 (14/41/0/32) | not syntax-countable | owner decision | D09: two spellings for building a schema record; type-directed |
| `record-destructuring` | shape | adjacent `let a = r.a` lines | `let {a, b} = r` | on | fix when proved | 14 (6/5/0/3) | 524 (60/427/14/23) | keep | idiom |
| `regex-literal` | syntax | `regex.compile("...")` of a literal | `rx"..."` | on | fix when proved | 0 | 699 (354/279/3/63) | keep | capability: the literal is validated at check time |
| `repeat` | syntax | `for _ in range(N)` | `repeat N times` | on | fix when proved | 21 (2/6/0/13) | 190 (107/81/1/1) | remove | C03: rule 2 and rule 4; the survivor is the old spelling |
| `scalar-iteration` | shape | iterate `text.split("")` or byte indexes | iterate the string or bytes | on | fix when proved | 8 (8/0/0/0) | not syntax-countable | keep | idiom |
| `run-argv` | api | `process.command_argv(argv[0], argv)` | `run @argv ?` | on | fix when proved | 0 | not syntax-countable | keep | idiom |
| `signature-cli` | api | literal CLI schema parsed with `cli.parse` | `cli main(...)` signature | on | fix when proved | 0 | 54 (2/8/9/35) | keep | the schema API serves programs that build schemas at run time |
| `size-literal` | syntax | `64 * 1024 * 1024` | `64MiB` | on | fix when proved | 13 (0/4/0/9) | 86 (0/59/6/21) | keep | the old shape is general arithmetic |
| `slice` | api | `s.slice(offset, count)` | `s[a..b]` | on | fix when proved | 1 (0/0/0/1) | 1466 (957/408/8/93) | keep | the lint applies only where bounds are equivalent; the rewrite repeats the offset operand |
| `stream-producer` | advice | proc that builds a list item by item | `stream` producer with `yield` | on | no fix | 0 | 67 (2/55/6/4) | keep | not equivalent (laziness) |
| `string-concat` | shape | `join("")` of literal pieces | `+` | on | fix when proved | 0 | not syntax-countable | keep | idiom |
| `tempdir-scope` | shape | `fs.tempdir()?` plus `defer root.close()?` | `tempdir NAME { }` | on, off in this repo config | fix when proved | 0 | 76 (3/31/10/32) | keep | the handle form is needed when the `FsRoot` capability is passed on. Off in this repository's config |
| `tempdir` | shape | `NAME.remove()`, `NAME.mkdir()`, `defer NAME.remove()` | `tempdir NAME at PATH { }` | on | fix when proved | 0 | 51 (0/21/5/25) | owner decision | D04: the survivor needs the unreservable word `at` |
| `try-capture` | shape | single-use closed helper | `try { }` | on | fix when proved | 0 | 141 (27/95/4/15) | keep | idiom |
| `value-pipeline` | shape | nested calls or a single-use temporary | `x \|> f(_)` | on | fix when proved | 43 (2/8/0/33) | 180 (70/82/1/27) | keep | idiom |
| `within` | advice | every `run` in a block carries the same `--timeout` | `within DURATION { }` | on | no fix | 0 | 35 (0/32/3/0) | keep | note only: the two differ (per-process timeout against one deadline) |
| `write-lines` | api | join then append a newline | `Path.write_lines` | on | fix when proved | 2 (0/2/0/0) | 16 (0/12/1/3) | keep | idiom |
| `yield-delegation` | shape | `for x in xs { yield x }` | `yield @xs` | on | fix when proved | 1 (0/1/0/0) | 137 (98/25/5/9) | keep | the loop is general code |
| `inferred-proc-return` | annotation | private proc return type equal to the inferred one | omit the type | opt-in | fix when proved | not measured | 2179 (706/487/49/937) | keep | as above; opt-in |
| `env-path-list` | advice | formatting a `:`-separated string | `[fp"{dir}", @env.PathList.NAME ?? []]` | on | fix when proved | 19 (0/0/0/19) | not syntax-countable | keep | not applied by `--fix`: the values differ in three cases (SPEC 10.4) |
| `write-mode` | api | `write` followed by `chmod` | `write(..., mode: M)` | on | fix when proved | 9 (0/2/0/7) | not syntax-countable | keep | idiom |
| `for-index` | shape | counter loop that only walks a list | `for index, item in list` | on | fix when proved | 0 | 19 (1/11/1/6) | keep | the old shape is general code; the sugar form itself is C08 |
| `path-kind` | api | `metadata()?.kind == File` | `is_file()`, `is_dir()`, `is_symlink()` | on | fix when proved | 2 (1/0/0/1) | 375 (11/303/3/58) | keep | idiom |
| `typed-callable` | advice | `Proc`/`Pure` parameter type | named callable type | on | no fix | 1 (0/1/0/0) | not syntax-countable | keep | no fix; advice |
| `test-expect` | api | assertions on each field of `test.run_script` | `test.expect` | on | fix when proved | 42 (0/42/0/0) | 1705 (0/1704/1/0) | keep | idiom |
| `is-empty` | api | `xs.len() == 0` | `xs.is_empty()` | on | fix when proved | 47 (18/18/0/11) | 2598 (1863/429/5/301) | keep | the old shape is general code |
| `negative-index` | shape | `xs[xs.len() - 1]` | `xs[-1]` | on | fix when proved | 10 (8/2/0/0) | 233 (147/57/2/27) | keep | the old shape is general arithmetic |
| `non-empty-argv` | advice | spliced `List[T]` command vector | `NonEmpty[T]` | on | no fix | 50 (4/39/5/2) | not syntax-countable | keep | advice about a type |
| `as-conversion` | api | `text.parse_int()?` | `text as Int` | on | fix when proved | 120 (24/65/0/31) | 1358 (225/1061/18/54) | owner decision | D08: direction conflicts with rule 3 |
| `text-pattern` | advice | split read by position; prefix test with a slice | text pattern | opt-in | no fix | 413 (309/53/2/49) | 28 (0/22/6/0) | keep | opt-in advice |
| `argument-label` | api | `fs.symlink(target, link)` | `link.symlink(to: target)` | on | fix when proved | 3 (0/1/0/2) | 858 (38/729/2/89) | keep | idiom |
| `set` | advice | `Map[K, Bool]` whose values are only `true` | `Set[K]` | on (advice opt-in) | fix when proved | 134 (38/4/0/92) | not syntax-countable | keep | advice about a type; opt-in for the general case |
| `collect` | shape | declared-empty list then only appended | `collect { }` | on | fix when proved | 100 (32/27/0/41) | 1222 (862/222/4/134) | keep | idiom |
| `wait-until` | advice | polling loop with `time.sleep` and a count | `wait until COND within LIMIT` | on | fix when proved | 45 (2/5/0/38) | 27 (0/13/2/12) | keep | note only: the failures differ |

Totals: 5 remove, 59 keep, 10 owner decision (74).

### Lints that already carry a removed spelling

These show the pattern Phase B follows. Each pairs a parse or check error with a lint that carries the fix.

| Lint | Removed spelling | Error |
|---|---|---|
| `lint.enum-declaration` | `type N = A \| B` | `parse.enum-migration` |
| `lint.env-scope` | expression environment assignments in `env` | parser finding under its migration code |
| `lint.block-header` | error-handler parameters outside the block | parse error |
| `lint.stream-options` | stream stage flags | parse error |
| `lint.removed-record-require` | `record.require` | check error |
| `lint.compatibility-vocabulary` | removed vocabulary such as `run.builtin` | check error |
| `lint.positional-error-arguments` | positional error arguments after a named one | check error |
| `check.removed-abort`, `check.removed-fs-function`, `check.removed-set-function` | `abort(STATUS)`, removed `fs` and `set` functions | check error with a fix |
| (none) | `E>`, `E>>` redirection | `parse.legacy-stderr-redirection`; the lexer still tokenizes both |

### Other lints that carry a second spelling

Counts are the lint's own old-spelling sites. All stay lints unless a ranked item says otherwise.

| Lint | Old spelling | Sites S/T/D/L |
|---|---|---|
| `redundant-tail-return` | final `return X` instead of a tail value | 228 (97/120/0/11) |
| `redundant-tail-return-binding` | `let x = e; return x` at the tail | 1 (0/1/0/0) |
| `redundant-ok-tail` | `return Ok(...)` at a tail | 18 (4/14/0/0) |
| `redundant-ok-return` | `return Ok()` in a `Result[Unit]` function | 21 (11/3/0/7) |
| `redundant-bare-return` | final bare `return` | 5 (1/1/0/3) |
| `redundant-propagation` | `?` on a statement-position `Result[Unit]` | 2678 (203/635/0/1840) |
| `redundant-scope-propagation` | `?` on a statement-position scope | 24 (0/1/0/23) |
| `boolean-guard` | leading failure branch on a Bool as `if` | 240 (39/133/0/68) |
| `boolean-pattern-test` | `match` yielding `true`/`false` per arm | 11 (0/5/0/6) |
| `pattern-conditional` | two-arm `match` with a complementary pattern | 251 (20/87/0/144) |
| `identical-match-arms` | adjacent arms with one body | 12 (0/0/0/12) |
| `command-value` | `(name)` command argument instead of `$name` | 0 |
| `redundant-command-fmt` | single-value command f-string | 16 (0/16/0/0) |
| `redundant-command-interpolation` | single interpolation in command arguments | 42 (0/42/0/0) |
| `redundant-fmt-wrapper` | `${}` or `()` around an f-string | 0 |
| `redundant-string-interpolation` | interpolation of one value | 2 (0/2/0/0) |
| `redundant-path-interpolation` | single-value path interpolation | 4 (0/1/0/3) |
| `redundant-path-display` | `.display()` in an interpolated path | 88 (8/8/0/72) |
| `path-constructor` | `Path(...)` instead of `p"..."` | 36 (7/11/0/18) |
| `lexical-block` | value block for an unconditional scope | 1 (0/1/0/0) |
| `core-assert` | assertion call instead of `assert` | 333 (0/8/0/325) |
| `redundant-use-alias` | `use a.b as b` | 188 (3/7/0/178) |
| `redundant-main-call` | explicit `main(@args)` | 0 |
| `redundant-newline-triple-string` | single-newline triple string | 0 |
| `duration-arithmetic` | `(n * 1s)` for a bounded numeric conversion | 1 (1/0/0/0) |
| `lookup-absence` | comparing a lookup with `-1` | 0 |
| `missing-f-prefix` | plain string whose `{name}` names a binding | 0 |
| `needless-annotation` | redundant binding annotation | 240 (67/99/5/69) |
| `default-param-type` | parameter type that its default establishes | not measured |

## Disambiguation and special-case rules

Rules in the SPEC and the grammar reference that exist to separate two forms. "Structural" means the collision is between forms the language keeps; removal of one side would need an item in the ranked list.

| # | Rule | Where | Collision it resolves | Disposition |
|---|---|---|---|---|
| S1 | `wait (until)`: a handle named `until` must be parenthesized | SPEC 8.6, 11.7; `primary: "wait" !"until" operand` | contextual `until` after `wait` against an ordinary identifier | Gone if `until` is reserved (2 identifier uses) or if `wait until` goes (D03) |
| S2 | The `..` of a backoff is two dots with nothing between them | SPEC 8.6; `backoff_range = head_duration "." ~"." head_duration` | `..` is not a token; two glued dots are told apart from `.name` item expressions and line-leading `.` | Structural: shared by slices, bounded Int, rest patterns; the backoff use goes with D03 |
| S3 | A wait or retry limit and each interval is a duration literal or a name with `.field`s; any other expression must be bound first | SPEC 8.6, 8.8; `head_duration` | an arbitrary expression after `within`/`every`/`backoff` cannot be told from the following `{`, clause word, or `..` | Goes with whichever of `wait until`, `retry backoff`, `within` is removed (D03) |
| S4 | `repeat` starts a statement only when `times` follows the count on the same line directly before `{`; neither word is reserved | SPEC 8.6; `repeat_statement` | contextual `repeat`/`times` against identifiers and commands named `repeat` | Removed by C03 |
| S5 | `fail` and `exit` start a statement only when an operand follows on the same line, in the position where a command would be read; `fail = 1`, `fail(1)`, `fail as Int` stay ordinary | SPEC 8.6, 8.9; `fail_statement`, `exit_statement`, lookahead `!name_expression`, exclusions in `named_command` | contextual statement words against bare-name commands, assignments, calls, and conversions | Gone if `fail` and `exit` are reserved (cost 65 and 2 identifier uses) or if C01 removes bare-name commands |
| S6 | `because` is a word only after the first operand of `fail` | SPEC 8.6; `fail_statement` | contextual `because` against an identifier in the message expression | Removed by C06 |
| S7 | `atomically replace` head: no word reserved; the destination ends at the first `as` outside brackets directly before a name and `{`; `atomically replace target as Path as tmp {` converts `target` | SPEC 6.11, 8.7; `atomically_statement`, exclusion in `named_command` | three contextual words and a second role for `as` against commands, identifiers, and the conversion operator | Gone if `atomically` and `replace` are reserved (3 and 38 identifier uses) or the sugar goes (D03) |
| S8 | `tempdir` starts a scope only when a name and then `{` or `at` follow on the same line | SPEC 10.4; `tempdir_scope`, exclusions in `named_command` and `guarded_expression_statement` | contextual `tempdir`/`at` against identifiers and commands | Gone if D04 drops the `at` form and `tempdir` is reserved (24 identifier uses) |
| S9 | `within` starts a scope only when a duration literal or a name with `.field`s follows and `{` follows on the same line | SPEC 10.4; `within_scope`, exclusions in `guarded_expression_statement` | contextual `within` against identifiers (36 uses) and the `wait until`/`retry` clause word | D03 |
| S10 | `collect` begins the expression only when `{` follows on the same line; a value named `collect` that a block follows is written `(collect)` | SPEC 6.9; `primary: !("collect" "{")` | contextual `collect` against `rows.collect()` and `\|> collect()` | Gone if `collect` is reserved (14 identifier uses; member and stage uses stay legal) |
| S11 | `ctx` begins the expression only before a head expression and a block; `ctx` stays an ordinary name elsewhere | SPEC 8.8; `primary: "ctx" ...` | contextual `ctx` against 41628 identifier uses (every test parameter) | D05 |
| S12 | `as` has four roles decided by position: conversion, `use` rename, pattern alias, end of `atomically replace` | SPEC 6.11; `conversion_tail`, `use_statement`, `alias_pattern`, `atomically_statement` | one contextual word, four forms; after `is`, an `as` belongs to the pattern | Shrinks to three roles if the `atomically` sugar goes; `as` itself is reservable (4 identifier uses) |
| S13 | `with` begins two forms that a following `else` tells apart; in value position `else` after a resource scope is a parse error | SPEC 8.6, 10.4; `with_statement`, `resource_scope` | one head, two meanings, decided by a clause that follows the whole block | Removed by D06 if the handler form goes |
| S14 | `guard let` against `guard COND` | SPEC 8.6; `!"let"` in `guard_statement`, `guard_condition` | binding form against condition form after the same word | Structural: both forms are needed unless D02 removes `guard COND` |
| S15 | After `=>`, a `{` is a block unless it opens a record whose first field is written out (`name:`, `"key":`, `[key]:`, `...spread`); `=> ({x})` needs parentheses | SPEC 6.1; `arm_statement`, `arm_value` | block against record literal after a match arrow | Structural for blocks and records; shares its cause with S16 |
| S16 | A bare `{` is a block, a record, a map, or a set, decided by how the first entry begins; `{ (name) }` is a block of one name; `{}` is always a record or map; `{a, b}` is a record unless a `Set[T]` is expected; a one-element set needs a trailing comma; mixing entry kinds is `parse.brace-literal-mixed` | SPEC 6.4, 6.9; `set_lead`, `set_literal`, `set_comprehension`, `record_literal`, `map_comprehension`, `block` | four meanings for one brace pair; `set_lead` is a 46-alternative lookahead | Block/record/map remain; the set share goes with C02 |
| S17 | Before a block, a condition, `for` source, `match` subject, `with` value, or `ctx` message is read with the `condition_` rules: a brace after a qualified pattern test is the test payload only when it looks like `{name: P}` or `{..}` | grammar reference, "Expressions before a block"; `condition_*` productions (15 of them) | record-pattern payload against the block that follows | Structural; the 14 `condition_*` productions duplicate the expression productions to carry this one rule |
| S18 | A brace after `module.function(...)` before a block is the body, not a builder callback | grammar reference; `!builder_callback_before_condition_body` in 10 productions | builder block against the body of `if`, `while`, `for`, `match`, `with`, `ctx`, `tempdir` | Removed by D07 if builder blocks go |
| S19 | A statement beginning with a name or `.name` chain followed by a word is a command, except where the word continues an expression (operator, `is`, `as`); `cleanup when done` passes two words; write `cleanup() when done` | SPEC 8.6, 6.11, 10.1; `named_command`, `expression_statement`, `guarded_expression_statement`, `&name_expression` | command style against expression statements | Removed by C01 (the only commands are `print`, `eprint`, `cd`, `env`, `run`) |
| S20 | Postfix `when`/`unless`: in `print`/`eprint` a bare `when` or `unless` always begins the guard; a command other than `print` reads them as words; a run payload must be grouped; an assignment from a run form is guarded after its `?`; no other statement takes one | SPEC 8.6; `postfix_guard`, `guarded_value`, `print_statement`, `guarded_expression_statement` | guard word against command argument word | Removed by D01 if postfix guards go |
| S21 | A line continues only when it begins with a listed token; `-`, `/`, `is`, `./`, `../` start new statements; `else` is not a continuation (`parse.detached-else`) but `with ... else` allows line breaks before `else` | SPEC 2.5; grammar "Line continuation"; `block NEWLINE* "else"` in `with_statement` | negation/absolute path against subtraction/division across a line break; bare paths against operators | The `/` and `./` exceptions go with C10; `-` stays |
| S22 | A `\` continuation exists for commands only; elsewhere `parse.line-continuation` | SPEC 2.5 | command words against expression continuation | Structural |
| S23 | Inside a `match` statement arm or a `with` binding a `,` ends the statement, so a bare command word there cannot contain `,` | grammar reference, "Conditions outside the productions" | arm separator against comma in a command word | The `with` half goes with D06 |
| S24 | A value-pipeline stage that is a call uses exactly one `_` placeholder as a whole argument or none (`parse.pipeline-hole`) | grammar reference | placeholder against ordinary argument | Structural |
| S25 | A `\|>` stage that begins with one of 35 stage names is a stream stage; any other stage is an expression | grammar reference, "Stream stages"; `value_stage` negative lookahead | a stage name shadows any function of that name after `\|>` | D12 |
| S26 | Ordering never mixes with equality, membership, or a pattern test without grouping (`parse.mixed-comparison`); `and` with `or` and `??` with either need grouping (`check.mixed-logical`); an `if`/`match` or a pipeline held by an operator needs grouping (`check.ambiguous-grouping`) | SPEC 6.1; `ordering`, `equality` | precedence a reader cannot see at a glance | Structural; `parse.mixed-comparison` only exists because chains do (D10) |
| S27 | `.name` at the start of a line continues the previous line, so a callback block can begin with `.name` only as its first statement | SPEC 6.9; grammar "Line continuation"; `item_expression` | item expression against a statement beginning with `.` | Structural |
| S28 | `.Name` is a target-typed variant, `.name` an item field; a leading-dot arm head is `parse.inferred-variant-arm`; `fail .Name()` outside a declared family is `check.inferred-variant` | SPEC 5.5, 8.6; `item_expression`, `arm_head = !("." IDENT) pattern` | item field against target-typed variant, decided by capitalization and context | D08 (leading-dot variants) |
| S29 | A bare path literal starts with `/`, `./`, or `../` and contains no whitespace or delimiter | SPEC 2.6; `bare_path`, `PATH_PART`, `item_expression !~"/"` | path literal against division, a leading `.name`, and `..` | Removed by C10 |
| S30 | `?` glue: `x?.f` is null-safe access, `(x?).f` propagates then accesses; `expression_item` takes a trailing `?` only before a newline, `;`, `,`, `)`, `]`, `}` | SPEC 6.1; `postfix`, `expression_item` | propagation against null-safe access and the ternary-like trailing position | Structural |
| S31 | Command words: adjacent parts form one argument; `name.field` is a word, `$name.field` a value; `(name)` is redundant (`lint.command-value`); words are never split or globbed | SPEC 10.2; `word`, `dollar_name`, `command_argument` | three spellings for interpolating a value into an argument: `$name`/`${expr}`, `(expr)`, `f"..."` | D08-adjacent; no removal proposed |
| S32 | Run options `--timeout=`, `--cpumax=`, `--accept=` may appear in any order, each once | grammar reference; `run_options` (15 permutation alternatives) | option words against command words | Structural; the 15 alternatives are one production |
| S33 | Run forms are `run`, `run.status`, `run.text`, `run.bytes`, `run.capture --text\|--bytes`, `run.stream --text\|--bytes`; only the first segment of a pipeline may name a form other than plain | SPEC 11.1, 11.6; `run_member`, `run_segment` | one form per pipeline head | Structural |
| S34 | Keywords are legal as field labels and member names but never become variables through shorthand or punning | SPEC 2.4; `LABEL`, `MEMBER` | reserved words against record fields (`{type: "file"}`) | Structural |
| S35 | `error` may be bound anywhere and `error.fail(...)` names the module even where `error` is bound; `args` is predeclared; standard module names are reserved in binding and alias positions | SPEC 2.4 | module names against payload names | Structural |
| S36 | `codec.decode_as(...)` and `codec.read_as(...)` take a type as their third argument; `.require`, `.require_exact`, `.require_stripped` take an optional type | grammar reference; `codec_schema_call`, `codec_schema_argument`, `member_access`, `call_argument_chain` | a type inside call parentheses against an expression argument | Structural; three productions exist only for this |
| S37 | The grammar accepts `cli NAME(...)`; the checker rejects every name but `main` (`check.cli-entry`) | grammar reference; `cli_entry` | a rule that only produces a better diagnostic | Removed by C11 |
| S38 | The statement-start exclusion list (`let const var assert if ... test NAME, on NAME, cli IDENT (, error NAME =, nominal type, process.command {`) appears in both `expression_statement` and `guarded_expression_statement` | grammar reference | contextual statement heads against expression statements | Shrinks with every contextual word that is reserved (D03) or removed |
| S39 | `$?` is accepted by the grammar and is not described in the SPEC | grammar reference; `primary: "$?"` | undocumented duplicate of `run.status` | Removed by C07 |
| S40 | f-strings: `{{` and `}}` are braces, `{ {a: 1}.a }` needs a space, `${` is an error, `$name` is an error when `name` is bound; env strings allow only an identifier and no escapes | SPEC 2.6 | interpolation against literal braces and dollars | Structural |
| S41 | Size and duration literals take their unit directly after the digits and no fraction; `1KBps` is not a unit | SPEC 2.6 | unit suffix against identifier | Structural |
| S42 | Proc names and module path segments may contain `-`; a hyphenated final module segment needs an alias | SPEC 2.4; `NAME` | hyphenated name against subtraction | Structural |

## Ranked removal list

Order: removals that delete the most rules per migrated site first, then the owner decisions by the rules they would delete, then the forms that stay. Ids: `C` removal, `D` owner decision, `K` keep. Site counts are of the spelling that would change.

| Rank | Id | Item | Decision | Sites to migrate S/T/D/L | Removes | Mechanical | Reasoning |
|---|---|---|---|---|---|---|---|
| 1 | C01 | Command-style calls of standard APIs (`json.write out.json $x --pretty`) | remove | 1 (0/0/1/0) | the third alternative of `named_command`; rule S19 and parts of S5, S20, S38; the `&name_expression` lookahead; SPEC 10.1 paragraph | by hand: 1 documentation site; words become typed arguments | None in S, T, or L. The campaign line "a statement that begins with a name followed by a word is a command" exists only for this form and for the four core commands. |
| 2 | C02 | Set literals and set comprehensions (`{"a", "b"}`, `{x for x in xs}`) | remove | 86 (0/76/10/0) | `set_literal`, `set_comprehension`, `set_expression`, `set_lead` (a 46-alternative lookahead); rule S16's set half; `parse.brace-literal-mixed`; the "bare names are a record unless a `Set[T]` is expected" typing rule; SPEC 6.4 and 6.5 set text | yes: `set.from([...])`, `set.from([x for ...])`; `set.from` must be admitted in `const` initializers (checker rule, not syntax) | No use in S or L. `set.from` and `set.empty` exist. The binary `\|` and `&` on sets stay (23 sites). |
| 3 | C03 | `repeat N times { }` | remove | 190 (107/81/1/1) | `repeat_statement`; `SugarForm::Repeat`; contextual words `repeat` and `times`; rule S4; `lint.prefer-repeat`; SPEC paragraph | yes: `for _ in range(N) { }`, the expansion the SPEC already defines; no extra line | The survivor exists (46 `for _ in` loops, 31 over `range(...)`). The sugar saves four characters, so it fails rule 2. |
| 4 | C04 | `_ =>` as a whole unguarded arm | remove | 90 (15/28/0/47) | one SPEC sentence; `lint.prefer-match-else` (becomes the diagnostic); `else =>` is already the documented single spelling | yes: `else =>`; `_ if COND =>` stays | The count shown is the lint's where available, otherwise all `_` arms including guarded ones. `else =>` has 614 sites. |
| 5 | C05 | `guard COND else fail MSG` | remove | 3 (0/2/1/0) | `guard_fail_statement`; `SugarForm::GuardFail`; the rule that only `fail` may follow `else` without braces; SPEC paragraph | yes: `guard COND else { fail MSG }`, one line | Sugar on sugar with 3 sites. |
| 6 | C06 | `fail MSG because CAUSE` | remove | 3 (0/2/1/0) | contextual word `because`; rule S6; the cause operand of `SugarForm::Fail`; SPEC paragraph | yes: `return Err(error.failure(MSG), cause: CAUSE)` or `return Err(.Variant(...), cause: CAUSE)` | 3 sites. `fail MSG` itself stays (516 sites). |
| 7 | C07 | `$?` (status of the last run) | remove | 1 (0/1/0/0) | a `primary` alternative and a lexer token; it is in the grammar reference and not in the SPEC | by hand: 1 site, a parser test string | `run.status` exists. |
| 8 | C08 | `cli NAME(...)` accepted by the grammar and rejected by `check.cli-entry` | remove | 0 | one `cli_entry` alternative and rule S37 | nothing to migrate | The checker already rejects every name but `main`. |
| 9 | C09 | `error NAME { A newline B }` (brace spelling) | remove | 30 (11/11/5/3) | one alternative of `error_declaration`; the `"error" NAME "{"` exclusions in both statement-start lists; SPEC sentence | yes: `error NAME = A \| B`, which allows line breaks after each `\|` | Pipe spelling has 253 sites against 30. |
| 10 | C10 | Bare path literals (`/usr/bin/cc`, `./out`, `../x` in expression position) | remove | 986 (201/321/18/446) | `bare_path`, `PATH_PART`, the `!~"/"` lookahead of `item_expression`; rules S21 (path half) and S29; SPEC 2.6 sentence | yes: wrap the token as `p"..."` | `p"..."` has 8650 sites against 986. The largest remove item; 446 of the sites are in Laputa. Command words such as `./configure` are words, not literals, and are untouched. |
| 11 | C11 | Quoted labels that are identifier-shaped (`{"type": v}`) | remove | 36 (3/0/0/33) | `lint.prefer-bare-field-label` becomes the diagnostic; no grammar change | yes | Quoted labels stay for keys that are not identifiers. |
| 12 | C12 | Module-function form of a receiver method (`lint.prefer-method`) | remove | 0 | `lint.prefer-method` becomes the diagnostic; the module-function entries become removed functions, as `check.removed-fs-function` and `check.removed-set-function` did | yes where the receiver is the first argument | An API removal, not syntax. Included because rule 3 prefers the method. |
| 13 | C13 | `for INDEX, ITEM in SRC` | remove | 19 (1/11/1/6) | the `( IDENT "," )?` alternative of `for_statement`; `SugarForm::ForIndex`; `parse.for-index`; SPEC paragraph (`lint.prefer-for-index` loses its target; it must be rewritten to suggest `\|> enumerate()` or deleted) | yes: `for {index: i, value: v} in SRC \|> enumerate()`, the SPEC's own expansion | 19 sites against 31 `enumerate` loops. Weakest remove item: the survivor reads worse. Move it to owner decision if that matters. |
| 14 | D01 | Postfix `when` and `unless` | owner decision | 5803 (4658/324/23/798) | reserved words `when` and `unless`; `postfix_guard` (used in 7 productions); rule S20 (five special cases); part of S5, S19, S38; SPEC 8.6 paragraphs; `lint.prefer-guard` retargets | yes: `if C { STMT }` and `if C {} else { STMT }`, one line each | Largest cluster in the corpus: 5803 sites, 4658 in S. DESIGN.md names `return cached when cached != null` and `continue unless ...` as the model sugar, so removal reverses a stated principle. Rule 2 says the core form costs no extra line. |
| 15 | D02 | `guard COND else { }` | owner decision | 1141 (373/230/5/533) | `guard_statement`'s condition half; rule S14; `check.guard-fallthrough`; `lint.prefer-guard`, `lint.boolean-guard` | yes: `if !COND { }` or `if COND {} else { }` | 1141 sites. Decide D01 and D02 together: three spellings of an early exit are `if COND { return }` (3984 sites), a postfix guard, and `guard`. |
| 16 | D03 | Contextual words of the sugar clauses: reserve or drop the form | owner decision | per word, in the reasoning | rules S1, S2, S3, S5, S7, S9, S10, S12 (one role), S38 | reserving a word renames the identifiers shown in the contextual-word table; dropping a form follows its expansion | Per word, identifier uses that reserving would break (variable, binding, callable name): `until` 2, `every` 4, `backoff` 14, `atomically` 3, `replace` 38, `because` 0, `within` 36, `collect` 14, `without` 33, `exit` 2, `fail` 65, `repeat` 45, `times` 20, `as` 4, `on` 16, `nominal` 10, `optional` 19, `exact` 173, `tempdir` 24. Words that cannot be reserved: `at` 3563, `ctx` 41628, `range` 1869, `main` 750, `task` 329. Module-name words (`env`, `error`, `test`, `cli`, `codec`, `module`) are already reserved in binding positions. |
| 17 | D04 | `tempdir NAME at PATH { }` | owner decision | 51 (0/21/5/25) | the only form that needs `at`; rule S8's second half; `lint.prefer-tempdir` | by hand: the three-line manual shape (`remove`, `mkdir`, `defer remove`) exceeds the one-extra-line budget | 0 sites in S; 25 in Laputa. `at` has 3563 identifier uses and cannot be reserved. Either keep the one exception or drop the clause. |
| 18 | D05 | `ctx DESCRIPTION { }` | owner decision | 28 (6/17/3/2) | contextual `ctx`; rule S11 | not mechanical: the nearest existing spelling is `(try { ... }).context(DESCRIPTION)?`, which captures only `?` failures | `ctx` has 41628 identifier uses (test parameters), so it cannot be reserved. 28 sites. |
| 19 | D06 | `with N = V { } else { \|e\| ... }` (handler form of `with`) | owner decision | 37 (0/36/1/0) | rule S13; the line-break asymmetry of `else` in S21; part of S23; `with_statement`; SPEC paragraph | not mechanical: the handler sees only initializer failures, not body failures; a rewrite needs a `try` capture and a `match` | No use in S or L; 36 of 37 sites are tests of the feature. The resource scope (77 sites) stays. |
| 20 | D07 | Builder blocks (`process.command { cwd = ...; run ... }`) | owner decision | 36 (1/28/7/0) | four productions (`builder_call`, `builder_block`, `builder_compound_entry`, `builder_entry`); rule S18, which is a lookahead in 10 other productions; SPEC 11.9; `check.builder-check` | semi-mechanical: `process.command_argv(target, argv, named args)` takes every field; command words become list items; `run` against `run.status` needs a check | 36 sites (1 in S, none in L). `process.command_argv` has 1112 call sites. Builder entries other than fields and statements (`task NAME`, nested entries) have 0 uses. |
| 21 | D08 | Direction conflicts: rule 3 (method over syntax) against the lints that push toward syntax | owner decision | 3266 (927/1908/64/367) | depends on the choice: `as` conversions (1,358) against `parse_int()`, `parse_float()` (all exist); `e"NAME"` (288) against `env.get` and `env.get_or` (389 calls); leading-dot variants (1,563 expressions, 57 patterns) against `Type.Variant` | yes in each direction | Current lints say `as` over `parse_int()?`, `e"NAME"` over `env.get`, `.Name` over `Type.Name`. Rule 3 says the reverse for the first two. The rule 2 test (core costs more than a line) does not decide any of them. One answer applies to all three. `in` is not in this group: no membership method exists, so `lint.prefer-in` has nothing to prefer over. |
| 22 | D09 | Argument, field, and constructor spellings | owner decision | 2003 (688/366/31/918) | punned arguments (1,075) and shorthand fields (928); positional against named constructors; record literal against schema constructor; explicit against implicit `message: Str` payloads; item `.` (3,404) against `\|x\|` | yes for each | These save characters and no lines, so rule 2 removes them. Taste: owner call. `lint.prefer-named-argument-pun`, `-positional-constructor`, `-record-constructor`, `-implicit-message`, `-item-shorthand` are the lints in question. |
| 23 | D10 | Ordering chains `a < b < c` | owner decision | 27 (4/23/0/0) | `ComparisonChain`; `parse.mixed-comparison` and the `ordering` productions' chain half; SPEC paragraph | yes when the middle operand is a name or literal; otherwise one `let` line | 27 sites, 4 in S, none in Laputa. |
| 24 | D11 | `not in` | owner decision | 1688 (762/760/2/164) | reserved word `not` (its only use); `"not" "in"` in six productions and in the continuation-token list | yes: `!(a in b)`; no extra line | 1,688 sites (762 in S). `!` is the only other negation. Rule 2 removes it; 1,688 rewrites against one reserved word. |
| 25 | D12 | The 35 stage names after `\|>` are contextual | owner decision | 2398 (880/1243/38/237) | nothing: rule 4 cannot be met without new syntax (`map`, `first`, `count`, `min`, `range`, `sort` cannot be reserved), and rule 6 forbids new syntax | n/a | The List type has eight methods and no `map`/`filter`/`sort`, so stages are the only spelling. Recommend recording it as the one documented exception. |
| 26 | D13 | `cd PATH { }` and `env A=B { }` against `cd (PATH) { }` and `env (OVERLAY) { }` | owner decision | 242 (17/130/8/87) | the `cd` and `env` alternatives of `named_command`; `env_assignment`; core-command special names (SPEC 10.1); `lint.env-scope` already migrates one variant | semi-mechanical: command words become typed expressions (`p"..."`, a record for the overlay) | 242 command-style sites against 246 call-style sites. The call spelling is the only one valid in value position. The command spelling is the shell-like one. |
| 27 | D14 | Redundant tail `return`, `Ok(...)` wrapping, and `?` on calls that already propagate | owner decision | 2975 (316/775/0/1884) | `lint.redundant-tail-return`, `-tail-return-binding`, `-ok-tail`, `-ok-return`, `-bare-return`, `-propagation`, `-scope-propagation` would become errors | yes (each lint carries a fix) | Counted from the lints: tail `return` 228, `Ok(...)` wrapping 39, redundant `?` 2702 (1,840 of them in Laputa). A function ending in `return X` and one ending in `X` are two spellings; so are `f()` and `f()?` for a statement that propagates either way. Choose whether the redundant forms become errors. |
| 28 | D15 | Phase B mechanics: migration lints and who runs rewrites | owner decision | 0 | nothing | n/a | Seven lints already carry the fix for a removed spelling (`lint.block-header`, `-compatibility-vocabulary`, `-enum-declaration`, `-env-scope`, `-positional-error-arguments`, `-removed-record-require`, `-stream-options`) next to a parse or check error (`parse.enum-migration`, `parse.legacy-stderr-redirection`, `check.removed-abort`, ...). The campaign says to delete each `prefer-*` lint; the precedent keeps a migration lint. AGENTS.md forbids autofixers, so C10 (986 sites) and D01 (5,803) need the owner to run a one-off rewrite. |
| 29 | K01 | Compound assignment `+= -= *= /= %=` | keep | 8900 (6904/698/16/1282) | nothing | n/a | 8,900 sites; no extra line would be saved by removing them, but 8,900 rewrites would be forced. |
| 30 | K02 | `errdefer` | keep | 3 (0/1/1/1) | nothing | n/a | 3 sites, but no spelling expresses cleanup that runs only on error. Capability. |
| 31 | K03 | `wait until COND within L [every I \| backoff A..B]` | keep | 27 (0/13/2/12) | nothing (words are D03) | n/a | The expansion is seven lines (SPEC 8.6); the one-extra-line budget keeps the sugar. 12 sites in Laputa. |
| 32 | K04 | `atomically replace DEST as NAME { }` | keep | 34 (0/19/2/13) | nothing (words are D03) | n/a | Expansion is seven lines. 13 sites in Laputa. |
| 33 | K05 | `retry backoff A..B within L { }` | keep | 6 (0/5/1/0) | nothing (words are D03) | n/a | The computed delays and the limit cut-off cannot be written as a delay list without arithmetic. 6 sites. |
| 34 | K06 | `rx"..."`, `g"..."`, size literals `64MiB` | keep | 800 (354/343/9/94) | nothing | n/a | `rx` is validated at check time and `regex.compile` is not; `g` expands against the working directory with an effect; a size literal replaces a product of literals (86 sites), and the old shape is general arithmetic. |
| 35 | K07 | `guard let`, `if let`, `while let`, and the `is` test | keep | 3549 (2284/1105/18/142) | nothing | n/a | `guard let` binds into the enclosing block; `if let` binds in the branch; `is` binds nothing. Distinct capabilities. |
| 36 | K08 | `;` as a statement separator | keep | 2153 (1493/655/0/5) | nothing | n/a | 1,493 sites in S, mostly inline blocks `{ a; b }`; removing it forces line breaks. |
| 37 | K09 | `print`/`eprint` command words | keep | 2229 (1028/325/288/588) | nothing | n/a | `print` accepts `$name`, `${expr}`, quoted words, and `f"..."`; f-strings are 838 of its 2,160 arguments. Several spellings overlap, but the shell-like form is the language's identity (DESIGN.md Purpose). |

Totals: 13 remove, 15 owner decision, 9 keep (37).

Applied rules: one spelling per concept; sugar kept only when the core form costs more than one extra line or hides the intent (rule 2); a method over syntax (rule 3); no contextual words (rule 4); no collision-only rules (rule 5); no new syntax, so every survivor named above already exists (rule 6); same order, effects, and failures (rule 7).

## Phase B status

Items land in the order below. C10 and C13 are deferred by the owner; no D or K item is touched.

| Item | Status | Error code | Notes |
|---|---|---|---|
| C08 | done | `parse.cli-entry-name` | `cli NAME(...)` for any other name than `main` is a parse error; the checker branch is gone |
| C07 | done | `parse.last-status` | the audit counted one site; the expression was also an IR node with tag `ExprLastStatus`, a checker rule, and four native tests, all removed. The evaluator still tracks the last status for the interactive shell |
| C05 | done | `parse.guard-else-fail` | `SugarForm::GuardFail`, its formatter, flow, and highlight branches, and the `guard_fail_statement` production are gone. The generated-sentence suite then reached two latent grammar and parser disagreements about a run form's `?` followed by `( )` or `[`; the productions now reject them as the parser does |
| C06 | done | `parse.fail-because` | `lint.prefer-fail` no longer respells a returned `Err` that carries `cause:`; the cause operand of `SugarForm::Fail` and the `because` highlight rule are gone |

## Owner decisions

Each item marked "owner decision" above, with the choice needed.

- **D01**: Keep postfix `when`/`unless`, or remove them in favor of `if`? DESIGN.md names them as model sugar.
- **D02**: Keep `guard COND else { }`, or remove it in favor of `if !COND { }`? Decide with D01: which of `if`, `guard`, and postfix guards is the one early-exit spelling.
- **D03**: For each sugar clause word (`until`, `every`, `backoff`, `within`, `atomically`, `replace`, `exit`, `fail`, `collect`, `without`, `as`, `on`, `nominal`, `optional`, `exact`, `tempdir`): reserve it and rename the identifiers shown, or drop its form? The reserve costs are in the ranked list.
- **D04**: Keep the `at` clause of `tempdir` as the single contextual-word exception, or drop it and accept the three-line manual shape?
- **D05**: Keep `ctx` blocks (a contextual word with tens of thousands of identifier uses) or replace them with a method form? The nearest method form does not capture the same failures.
- **D06**: Remove the handler form of `with` (no use outside tests)? A rewrite is not mechanical.
- **D07**: Remove builder blocks in favor of `process.command_argv` with named arguments?
- **D08**: When a lint pushes toward syntax that replaced a method or a qualified name (`as`, `e"NAME"`, `.Variant`), does rule 3 override the lint? One answer for all three.
- **D09**: Do the character-only shorthands (punned arguments, shorthand fields, positional constructors, record literals of schema type, implicit `message` payloads, item `.`) stay?
- **D10**: Keep ordering chains (27 sites)?
- **D11**: Keep `not in` (1,688 sites) or remove it, which frees the reserved word `not`?
- **D12**: Record the stage names after `|>` as the one documented exception to rule 4?
- **D13**: Keep both `cd PATH { }` and `cd (PATH) { }` (and the same for `env`), or keep only the call spelling?
- **D14**: Should explicit tail `return`, `Ok(...)` wrapping, and `?` on calls that already propagate become errors?
- **D15**: Do migration lints stay after a removal, as `lint.enum-declaration` did? Who runs the large rewrites (C10, D01, D11), given the autofixer ban?

## Reproduction

The counting tool links the `xsh` crate at the revision above, as `dev/performance/corpus/inventory.rs` does. It walks `Parser::parse_source_arena_only` output with `AstArena::for_each_stmt_child(..., SugarView::Source, ...)`. The lint counts come from the `xsht` release build of the same revision. Neither tool is committed; the numbers above are the record.
