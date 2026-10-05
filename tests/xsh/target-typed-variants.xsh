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
    return Err(.Failed(kind: "proof-kind", message: f"{entry.path} is {label(entry.kind)}"))
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
  Err(.Conflict(file:, owner: "base"))
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
    {XSH_MODULE_PATH: root},
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
    return Err(ProofError.Failed(kind: "proof", message: f"{entry.path} is a binary"))
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
  assert "Err(.Failed(kind: \"proof\"" in fixed, fixed
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

const variant_pattern_kinds = """##! Kinds.

## A file kind.
export enum Kind { File, Binary, Tree(Int) }

## Why ownership is unknown.
export error OwnerError {
    Unowned
    Conflict(file: Path, owner: Str)
}
"""

test test_target_typed_variant_patterns_match_like_their_qualified_spellings { |ctx|
  let root = test.temp_dir(ctx, name: "inferred-variant-patterns")?
  fp"{root}/kinds.xsh".write_atomic(variant_pattern_kinds)?
  let executed = test.run_script(
    ctx,
    r"""use kinds as k

enum Level { Info, Warn, Fault(Str) }

error FetchError {
    Usage
    Offline : Timeout
    Rejected(url: Str, status: Int)
}

pure fetch(step: Int) -> Result[Level, FetchError] {
  return Err(.Usage("bad flag")) when step == 0
  return Err(.Offline()) when step == 1
  return Err(.Rejected(url: "u", status: 503)) when step == 2
  return Ok(.Fault("disk")) when step == 3
  return Ok(.Warn) when step == 4
  Ok(.Info)
}

pure describe(step: Int) -> Str {
  match fetch(step) {
    Err(.Usage {message}) => f"usage: {message}"
    Err(.Offline) => "offline"
    Err(.Rejected {url, status}) => f"{url} {status}"
    Ok(.Fault(reason)) => f"fault {reason}"
    Ok(.Warn | .Info) => "calm"
    _ => "other"
  }
}

pure owner(kind: k.Kind) -> Result[k.Kind, k.OwnerError] {
  return Err(.Unowned("nobody")) when kind == k.File
  return Err(.Conflict(file: p"bin", owner: "base")) when kind == k.Binary
  Ok(kind)
}

pure owned(kind: k.Kind) -> Str {
  match owner(kind) {
    Ok(.Tree(depth)) => f"tree {depth}"
    Ok(k.Binary | .File) => "plain"
    Err(.Unowned {message}) => f"unowned: {message}"
    Err(.Conflict {owner, ..}) => f"conflict with {owner}"
    _ => "other"
  }
}

for step in range(6) {
  print describe(step)
}
print owned(k.File)
print owned(k.Binary)
print owned(k.Tree(3))

let level: Level = .Warn
let kind: k.Kind = .Binary
print (level is .Warn) (level is .Info) (kind is .Binary) (kind is .Tree(_))
if level is .Warn {
  print "warn body"
}
let failure: FetchError = .Rejected(url: "x", status: 1)
if failure is .Rejected {status: 1, ..} {
  print "rejected 1"
}
if let .Rejected {status, ..} = failure {
  print f"if-let rejected {status}"
}
if let Err(.Usage {message}) = fetch(0) {
  print f"if-let {message}"
}
let guarded = match level {
  Warn if failure
    .message != "" => "a guard still continues onto a .name line"
  _ => "other"
}
print $guarded
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """usage: bad flag
offline
u 503
fault disk
calm
calm
unowned: nobody
conflict with base
tree 3
true false true false
warn body
rejected 1
if-let rejected 1
if-let bad flag
a guard still continues onto a .name line
"""
}

test test_target_typed_variant_patterns_cannot_head_a_match_arm { |ctx|
  for source in [
    """enum Level { Info, Warn }
let level: Level = Warn
match level {
  .Info => print "info"
  _ => print "other"
}
""",
    """enum Level { Info, Warn, Fault(Str) }
let level: Level = Warn
let label = match level {
  Info => "info"
  .Fault(reason) => reason
  _ => "other"
}
""",
    """error E = Usage | Other(code: Int)
let failure: E = .Usage("x")
match failure {
  E.Usage => print "usage"
  .Other {code} => print "other"
}
""",
    """enum Level { Info, Warn }
let level: Level = Warn
let label = match level { .Info => "info", _ => "other" }
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, source
    assert "parse.inferred-variant-arm" in rejected.stderr, rejected.stderr
    assert "continues the line before it" in rejected.stderr, rejected.stderr
  }
}

test test_target_typed_variant_patterns_need_a_matched_type_that_names_them { |ctx|
  let prelude = """enum Level { Info, Warn, Fault(Str) }
error E = Usage | Other(code: Int)
proc broad() -> Result[Level] { Ok(Info) }
pure narrow() -> Result[Level, E] { Ok(Info) }
let text = "x"
"""
  for {tested, code, reason} in [
    {
      tested: "broad() is Err(.Usage)",
      code: "check.inferred-variant",
      reason: "names no single error family",
    },
    {
      tested: "narrow() is Ok(.Missing)",
      code: "check.inferred-variant",
      reason: "has no variant `Missing`",
    },
    {
      tested: "text is .Info",
      code: "check.inferred-variant",
      reason: "is not an enum or error family",
    },
    {
      tested: "narrow() is Ok(.Fault)",
      code: "check.pattern-arity",
      reason: "match them as `.Fault(...)`",
    },
    {
      tested: "narrow() is Ok(.Info {message: _})",
      code: "check.pattern-constructor",
      reason: "is an enum variant",
    },
    {
      tested: "narrow() is Err(.Other(_))",
      code: "check.pattern-constructor",
      reason: "is an error variant",
    },
    {
      tested: "narrow() is Err(.Other {missing: _})",
      code: "check.pattern-field",
      reason: "unknown error payload field",
    },
  ] {
    let rejected = test.run_script(ctx, f"{prelude}let found = {tested}\n")?
    assert ! rejected.success, tested
    assert code in rejected.stderr, rejected.stderr
    assert reason in rejected.stderr, rejected.stderr
  }
}

test test_prefer_inferred_variant_fix_reaches_patterns_but_not_arm_heads { |ctx|
  let root = test.temp_dir(ctx, name: "inferred-variant-pattern-lint")?
  fp"{root}/xsht-config.ini".write_atomic("[lint]\nprefer-inferred-variants = true\n")?
  fp"{root}/kinds.xsh".write_atomic(variant_pattern_kinds)?
  let source = r"""use kinds as k

error FetchError {
    Usage
    Rejected(url: Str, status: Int)
}

pure fetch(step: Int) -> Result[k.Kind, FetchError] {
  return Err(.Usage("bad flag")) when step == 0
  return Err(.Rejected(url: "u", status: 503)) when step == 1
  return Ok(.Tree(2)) when step == 2
  Ok(.Binary)
}

proc broad(step: Int) -> Result[k.Kind] {
  fetch(step)
}

pure describe(step: Int) -> Str {
  match fetch(step) {
    Err(FetchError.Usage {message}) => f"usage: {message}"
    Err(FetchError.Rejected {url, status}) => f"{url} {status}"
    Ok(k.Tree(depth)) => f"tree {depth}"
    Ok(k.Binary | k.File) => "plain"
    _ => "other"
  }
}

for step in range(4) {
  print describe(step)
  let kind = fetch(step) ?? .File
  let label = match kind {
    k.File | k.Binary => "leaf"
    k.Tree(_) => "tree"
    _ => "other"
  }
  print $label (kind is k.Binary) (fetch(step) is Err(FetchError.Usage))
  match broad(step) {
    Err(FetchError.Usage {message}) => print f"broad usage {message}"
    _ => print "broad other"
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
  assert "the matched value's type already selects this variant" in first.stderr, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-inferred-variant $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  # An arm head keeps its qualifier, while a later alternative needs none.
  # `Result[k.Kind]` carries the broad `Error`, which names no family.
  for kept in [
    "    Err(.Usage {message}) => f\"usage: {message}\"",
    "    Err(.Rejected {url, status}) => f\"{url} {status}\"",
    "    Ok(.Tree(depth)) => f\"tree {depth}\"",
    "    Ok(.Binary | .File) => \"plain\"",
    "    k.File | .Binary => \"leaf\"",
    "    k.Tree(_) => \"tree\"",
    "(kind is .Binary) (fetch(step) is Err(.Usage))",
    "    Err(FetchError.Usage {message}) => print f\"broad usage {message}\"",
  ] {
    assert kept in fixed, fixed
  }

  let after = test.run_script(ctx, fixed, [], module_env)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let second = run.capture --text "xsht" lint --only lint.prefer-inferred-variant $candidate ?
  assert second.status.exited_with(0), second.stderr
}

test test_target_typed_variant_patterns_with_unions_and_else_arms { |ctx|
  let prelude = """enum Level { Info, Warn }
error FetchError = Usage | Rejected(url: Str, status: Int)
pure pick(step: Int) -> Union[Str, FetchError] {
  return FetchError.Usage("bad") when step == 0
  "text"
}
"""
  # A union names no single family; its narrowed member does.
  let narrowed = test.run_script(
    ctx,
    prelude + """let value = pick(0)
if value is FetchError {
  print (value is .Usage) (value is .Rejected {status: 1, ..})
}
""",
  )?
  assert narrowed.success, narrowed.stderr
  assert narrowed.stdout == "true false\n"

  let union = test.run_script(ctx, prelude + "let found = pick(0) is .Usage\n")?
  assert ! union.success, union.stdout
  assert "check.inferred-variant" in union.stderr, union.stderr
  assert "test the member first" in union.stderr, union.stderr

  # After an `else` arm's body too, a `.Name` line is read as an arm head.
  let after_else = test.run_script(
    ctx,
    prelude + """let level: Level = Warn
let label = match level {
  Info => "info"
  else => "other"
  .Warn => "warn"
}
""",
  )?
  assert ! after_else.success, after_else.stdout
  assert "parse.inferred-variant-arm" in after_else.stderr, after_else.stderr
  assert "parse.match-else-arm" in after_else.stderr, after_else.stderr
}
