# Design: opaque `Any`

Status: approved by the owner 2026-10-11 (drafted 2026-10-10). Part of workstream 8 of
`../CAMPAIGN.md`.

## Contract

An `Any` value is opaque. Without validation it may only:

- flow where `Any` is expected: an `Any` binding, parameter, return, record
  field, or `List[Any]` or `Map[Any]` element, and an API parameter declared
  `Any`;
- be compared with `==` or `!=`, tested with `in` or `not in` against a
  `List`, or used as a `group-by` or `unique-by` key;
- be validated by `.require(T)` or by a type pattern (`value is T`,
  `name is T` in a `match`);
- be read or rebuilt by path: `json.get(value, path)`,
  `json.get(value, path, fallback)`, `json.set`, `json.remove`.

The first three are SPEC 5.3 as it stands. The fourth is already true,
because those functions take an `Any` parameter; the SPEC now names it as the
way to work with data that has no schema.

Navigation is removed. Each of these on a receiver of type `Any` becomes
`check.dynamic-boundary`:

| Form | Rewrite |
|---|---|
| `v.name`, `v?.name`, `v[key]`, `v?[key]`, and chains of them | `json.get(v, ["name", key])?`, or `json.get(v, path, fallback)` where the old code tolerated absence |
| `v[a..b]` | `v.require(List[Any])?[a..b]` |
| `for x in v`, a comprehension over `v`, `v` as a pipeline source | the same over `v.require(List[Any])?`, or `Map[Any]` |
| `v.method(args)` | `v.require(T)?.method(args)` with the receiver type the method needs |
| `v?` | `v.require(T)?` |
| `{...v}` | `{...v.require(T)?}` |
| `wait v` | `wait v.require(ProcessHandle)?` |

No new syntax, keyword, method, or diagnostic code is added. The escape hatch
for code without a schema is `json.get`, which is greppable and local. This
supersedes the note in decision D4 of the campaign that the escape hatch
would be the one new spelling: there is none.

Unchanged:

- `.require(T)` and type patterns, including that extra fields are kept and
  defaults are never filled. Record and JSON exactness stays deferred.
- The erased `Record` type and a value from `module.load`. A field read from
  one still has type `Any`; what changes is that the result is opaque. A
  module contract (SPEC 4.9) remains the typed way into a loaded module.
- `Pure.call` and `Proc.call` still return `Any` and `Result[Any, Error]`.
- An API parameter declared `Any`, such as the words of
  `process.command_argv`, still accepts an `Any` argument. A parameter
  declared with a concrete type that today accepts `Any` by a special case in
  the checker stops accepting it.

A method call on an `Any` receiver was the only source of a call with
unknown effects inside checked code other than an opaque callable. Removing
it removes that source, and closes the `TODO.md` entry that an `Any` receiver
takes no named arguments.

## What a failure reports

Navigation failed at run time with `missing-field` or `type-error` as an
uncaught failure. The rewrites fail with a `Result` whose kind is
`json-path` (from `json.get`) or `schema` (from `.require`), propagated by
`?`. The rewrite therefore changes which failure a wrong shape reports, and
it needs an enclosing function or test that can propagate.

For that reason there is no autofix beyond the one `check.dynamic-boundary`
already offers, which appends `.require(T)?` where the context names `T`. An
autofix must not change behavior (`docs/DESIGN.md`).

## Migration

The checker change and the corpus migration land together, with no
intermediate lint and no temporary option.

1. One lane implements the rejection, its tests, and the SPEC and tour text,
   and is not merged.
2. The integrator builds that lane's `xsht` and runs `xsht check` over this
   repository and Laputa. Every `check.dynamic-boundary` it reports that the
   current `xsht` does not is a site. The list is committed as
   `dev/consolidation/any-sites.txt`.
3. Routine lanes rewrite the sites by the table above, at most 40 sites per
   lane, one directory per lane. Every rewrite is valid under the current
   checker, so each lane is verified with the current binaries and its
   directory's tests, and merges on its own.
4. When the lane's `xsht check` reports no site, the checker lane merges.

A routine lane rewrites a site only when the table gives one answer. It
stops and reports a site where the receiver type of a method call or the
element type of an iteration is not evident from the test data or the
surrounding code; the integrator re-issues those as an `xsh-lane` item.

Three classes of existing test change, and only these:

- A test that navigates an `Any` as a means to an assertion is rewritten by
  the table. Its assertion does not change.
- A test whose subject is the navigation itself (it asserts the value or the
  run-time failure of `.name`, an index, a method call, or iteration on an
  `Any`) is replaced by a rejected-program test for the same form. The
  integrator lists each such test in the handoff log.
- A test that asserts the kind `missing-field` or `type-error` for a wrong
  shape reached through navigation is rewritten to the new route and asserts
  `json-path` or `schema`.

If the routine lanes park more than one site in twenty, the checker lane is
parked instead of merged. The rewritten files stay, because they are valid
either way.

## Tests

- `docs/snippets/spec/rejected/`: one snippet per row of the table, each
  with its `# error: check.dynamic-boundary` line.
- `tests/xsh/checker-records.xsh` or a new `tests/xsh/checker-any.xsh`:
  accepted programs for each permitted use, including `json.get` with and
  without a fallback on a record, a `Str`-keyed map, and a list.
- The existing fix test for `check.dynamic-boundary` still passes unchanged.

## Relies on

Checked by the integrator at the start commit; if one is false this design
is parked whole.

- SPEC 5.3 lists navigation as permitted on `Any`, and every other use as
  `check.dynamic-boundary`.
- `json.get` is registered with a first parameter of type `Any`, in a
  two-parameter form returning `Result[Any]` and a three-parameter form
  returning `Any`, and its implementation (`json_path_get`) reads records,
  `Str`-keyed maps, and lists.
- The checker has no static record of which expressions navigate an `Any`
  other than its own acceptance of them, so the site list can only come from
  a build of the checker lane.
- `xsht check` on this repository and on Laputa reports nothing before the
  change.

## Out of scope

Exactness of records and JSON; a schema API; any change to `.require`;
opacity of the erased `Record` type.
