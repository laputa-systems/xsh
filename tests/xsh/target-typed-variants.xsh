test test_target_typed_variants_follow_expected_types { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum Kind { File, Binary, Symlink, Tree(Int) }
error ProofError = Missing(file: Path) | Failed(kind: Str, message: Str)
type Entry = {path: Path, kind: Kind}

pure label(kind: Kind) -> Str {
  match kind {
    File => "file"
    Binary => "binary"
    Symlink => "symlink"
    Tree(depth) => f"tree {depth}"
  }
}

pure classify(executable: Bool) -> Kind {
  if executable { .Binary } else { .File }
}

pure deepest() -> Kind {
  return .Tree(2)
}

pure require_binary(entry: Entry) -> Result[Entry, ProofError] {
  if entry.kind != .Binary {
    return Err(.Failed("proof-kind", f"{entry.path} is {label(entry.kind)}"))
  }
  entry
}

pure find(entries: List[Entry], file: Path) -> Result[Entry, ProofError] {
  for entry in entries {
    if entry.path == file {
      return Ok(entry)
    }
  }
  Err(.Missing(file:))
}

const entries: List[Entry] = [{path: p"usr/bin/xsh", kind: .Binary}, {path: p"usr/share", kind: .Tree(3)}]
let annotated: Kind = .Symlink
let optional: Kind? = .File
let kinds: List[Kind] = [.File, .Tree(1)]
let by_name: Map[Kind] = {sh: .Symlink}
let entry = Entry(path: p"usr/bin/sh", kind: .Symlink)
let picked: Kind = match entry.kind {
  Symlink => .File
  _ => .Binary
}

print label(annotated)
print label(.Tree(4))
print label(optional ?? .Binary)
print ([label(kind) for kind in kinds].join(","))
print label(by_name["sh"])
print label(entry.kind)
print label(picked)
print label(classify(true))
print label(deepest())
print label(entries[1].kind)
print (entry.kind == .Symlink)
print (.File == entry.kind)
print (entries[0].kind != .Binary)
print require_binary(entries[0])?.path
match require_binary(entry) {
  Ok(_) => print "unexpected"
  Err(ProofError.Failed {kind, message}) => print f"{kind}: {message}"
  Err(_) => print "other"
}
match find(entries, p"etc/missing") {
  Ok(_) => print "unexpected"
  Err(ProofError.Missing {file}) => print f"missing {file}"
  Err(_) => print "other"
}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """symlink
tree 4
file
file,tree 1
symlink
symlink
file
binary
tree 2
tree 3
true
false
false
usr/bin/xsh
proof-kind: usr/bin/sh is symlink
missing etc/missing
"""
}

test test_target_typed_variants_match_their_qualified_spellings { |ctx|
  let root = test.temp_dir(ctx, name: "target-typed-module")?
  fp"{root}/kinds.xsh".write_atomic("""##! File kinds.
## A packaged file's kind.
export enum Kind { File, Binary, Symlink }
## A packaged file's ownership failure.
export error OwnerError = Unowned(file: Path) | Conflict(file: Path, owner: Str)
## A packaged file.
export type Entry = {path: Path, kind: Kind}
""")?
  let executed = test.run_script(
    ctx,
    r"""use kinds as k
enum State: Str { Ready = "ready", Missing = "missing" }

pure owner(file: Path) -> Result[Str, k.OwnerError] {
  if file == p"etc/shadow" {
    return Err(.Unowned(file:))
  }
  Err(.Conflict(file, "base"))
}

pure describe(result: Result[Str, k.OwnerError]) -> Str {
  match result {
    Ok(name) => name
    Err(k.OwnerError.Unowned {file}) => f"unowned {file}"
    Err(k.OwnerError.Conflict {file, owner}) => f"{file} owned by {owner}"
    Err(_) => "other"
  }
}

let short: k.Kind = .Binary
let long: k.Kind = k.Binary
print (short == long)
let entry = k.Entry(path: p"bin/sh", kind: .Symlink)
print (entry.kind == k.Symlink)
print describe(owner(p"etc/shadow"))
print describe(owner(p"etc/hosts"))
let ready: State = .Ready
let pending: State = .Missing
print (json.encode({ready, pending})?)
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """true
true
unowned etc/shadow
etc/hosts owned by base
{"pending":"missing","ready":"ready"}
"""
}

test test_target_typed_variants_reject_missing_or_ambiguous_types { |ctx|
  for case in [
    {
      source: """enum Kind { File, Binary }
let kind = .Binary
""",
      message: "has no expected enum or error family type",
    },
    {
      source: """error ProofError = Failed(message: Str)
proc check() -> Result[Unit] {
  return Err(.Failed("missing"))
}
""",
      message: "names no single error family",
    },
    {
      source: """enum Kind { File, Binary }
let kind: Kind = .Binry
""",
      message: "did you mean `.Binary`?",
    },
    {
      source: """enum Kind { File, Binary }
let name: Str = .File
""",
      message: "is not an enum or error family",
    },
    {
      source: """enum Kind { File, Tree(Int) }
let kind: Kind = .Tree
""",
      message: "expects 1 argument(s)",
    },
    {
      source: """enum Kind { File, Tree(Int) }
let kind: Kind = .Tree("deep")
""",
      message: "expected Int",
    },
    {
      source: """enum Kind { File, Binary }
pure pick() {
  .File
}
""",
      message: "has no expected enum or error family type",
    },
  ] {
    let rejected = test.run_script(ctx, case.source)?
    assert ! rejected.success, case.source
    assert case.message in rejected.stderr, rejected.stderr
  }
}

test test_dot_names_inside_stream_stages_read_the_item { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum Kind { File, Binary }
type Entry = {name: Str, kind: Kind}
let entries: List[Entry] = [Entry(name: "xsh", kind: .Binary), Entry(name: "conf", kind: .File)]
let names = entries |> where { |entry| entry.kind == Binary } |> map .name
print names.join(",")
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == "xsh\n"

  let rejected = test.run_script(
    ctx,
    r"""enum Kind { File, Binary }
type Entry = {name: Str, kind: Kind}
let entries: List[Entry] = [Entry(name: "xsh", kind: .Binary)]
let names = entries |> where .kind == .Binary |> map .name
""",
  )?
  assert ! rejected.success
  assert "check.unknown-field" in rejected.stderr, rejected.stderr
}

test test_prefer_inferred_variant_fix_preserves_behavior_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "inferred-variant-lint")?
  fp"{root}/xsht-config.ini".write_atomic("[lint]\nprefer-inferred-variants = true\n")?
  fp"{root}/kinds.xsh".write_atomic("""##! Kinds.
## A file kind.
export enum Kind { File, Binary }
""")?
  let source = r"""use kinds
error ProofError = Failed(kind: Str, message: Str)
type Entry = {path: Path, kind: kinds.Kind}

proc check(entry: Entry) -> Result[Unit, ProofError] {
  if entry.kind == kinds.Binary {
    return Err(ProofError.Failed("proof", f"{entry.path} is a binary"))
  }
}

let entries: List[Entry] = [{path: p"bin/xsh", kind: kinds.Binary}, {path: p"etc/conf", kind: kinds.File}]
for entry in entries {
  match check(entry) {
    Ok(_) => print f"{entry.path} ok"
    Err(error) => print $error.message
  }
}
"""
  let module_env = {XSH_MODULE_PATH: root.display()}
  let before = test.run_script(ctx, source, [], module_env)?
  assert before.success, before.stderr
  let candidate = fp"{root}/main.xsh"
  candidate.write_atomic(source)?
  let first = run.capture --text "xsht" lint --only lint.prefer-inferred-variant $candidate ?
  assert first.status.exited_with(1), first.stderr
  assert "lint.prefer-inferred-variant" in first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-inferred-variant $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "Err(.Failed(\"proof\"" in fixed, fixed
  assert "kind: .Binary}" in fixed, fixed
  assert "kind: .File}" in fixed, fixed
  assert "entry.kind == .Binary" in fixed, fixed
  assert "kind: kinds.Kind}" in fixed, fixed
  let after = test.run_script(ctx, fixed, [], module_env)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let second = run.capture --text "xsht" lint --only lint.prefer-inferred-variant $candidate ?
  assert second.status.exited_with(0), second.stderr
}

test test_prefer_inferred_variant_skips_stage_items_and_unknown_targets { |ctx|
  let root = test.temp_dir(ctx, name: "inferred-variant-skip")?
  fp"{root}/xsht-config.ini".write_atomic("[lint]\nprefer-inferred-variants = true\n")?
  fp"{root}/kinds.xsh".write_atomic("""##! Kinds.
## A file kind.
export enum Kind { File, Binary }
## A kind failure.
export error KindError = Unexpected(message: Str)
""")?
  let candidate = fp"{root}/main.xsh"
  candidate.write_atomic(r"""use kinds
type Entry = {kind: kinds.Kind}

proc fail() -> Result[Unit] {
  return Err(kinds.KindError.Unexpected("generic"))
}

let inferred = kinds.Binary
let entries: List[Entry] = [Entry(kind: .Binary)]
let rebuilt = entries |> map { |entry| Entry(kind: kinds.File) }
print ${rebuilt.len()}
print ${[inferred].len()}
""")?
  let linted = run.capture --text "xsht" lint --only lint.prefer-inferred-variant $candidate ?
  assert linted.status.exited_with(0), linted.stderr
}
