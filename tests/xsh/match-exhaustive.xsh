# A statement `match` over an enum covers every variant or ends in `else =>`.
# The checker rejects the gap, so a variant added later is reported at every
# match that has not decided what to do with it.

enum Level { Info, Warn, Fault(Str) }

enum Mode: Str { Fast = "fast", Slow = "slow" }

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure level_label(level: Level) -> Str {
  var label = "unset"
  match level {
    Info => label = "info"
    Warn if label == "set" => label = "never"
    Fault(_) | .Warn => label = "loud"
  }

  label
}

pure mode_label(mode: Mode) -> Str {
  var label = "unset"
  match mode {
    Fast => label = "fast"
    Slow => label = "slow"
  }

  label
}

test test_statement_match_covering_every_variant_runs {
  assert level_label(Info) == "info"
  assert level_label(Warn) == "loud"
  assert level_label(Fault("disk")) == "loud"
  assert mode_label(Fast) == "fast"
  assert mode_label(Slow) == "slow"
}

test test_statement_match_missing_an_enum_variant_is_a_check_error { |ctx|
  let stderr = check_errors(
    ctx,
    """enum Level { Info, Warn, Fault(Str) }
enum Mode: Str { Fast = "fast", Slow = "slow" }
const level: Level = Warn
const mode: Mode = Slow
match level {
  Info => print "info"
}
match mode {
  Fast => print "fast"
}
""",
  )?
  assert "non-exhaustive match: missing variant(s) `Warn, Fault`" in stderr, stderr
  assert "non-exhaustive match: missing variant(s) `Slow`" in stderr, stderr
  assert "`else =>` as the deliberate catch-all" in stderr, stderr
  assert count(stderr, "err[check.non-exhaustive-match]") == 2, stderr
  assert count(stderr, "err[") == 2, stderr
  assert "warn[" not in stderr, stderr
  assert ":5:" in stderr, stderr
  assert ":8:" in stderr, stderr
}

# An arm covers a variant only when it matches the variant whatever its
# payload is, and only when it has no guard.
test test_refutable_payloads_and_guards_do_not_cover_a_variant { |ctx|
  let stderr = check_errors(
    ctx,
    """enum Level { Info, Warn, Fault(Str) }
const level: Level = Warn
const quiet = true
match level {
  Info => print "info"
  Warn if quiet => print "quiet"
  Fault("disk") => print "disk"
}
""",
  )?
  assert "non-exhaustive match: missing variant(s) `Warn, Fault`" in stderr, stderr
  assert count(stderr, "err[") == 1, stderr
}

test test_catch_all_spellings_complete_a_statement_match { |ctx|
  let output = test.expect(
    ctx,
    """enum Level { Info, Warn, Fault(Str) }
for level in [Info, Warn, Fault("disk")] {
  match level {
    Info => print "info"
    else => print "else"
  }
  match level {
    Fault("disk") => print "disk"
    other => print "binding"
  }
}
""",
    status: 0,
  )?
  assert output.stdout == "info\nbinding\nelse\nbinding\nelse\ndisk\n", output.stdout
}

const kinds_module = """##! Kinds.

## A file kind.
export enum Kind { File, Binary, Tree(Int) }
"""

# An enum imported under a namespace is as closed as a local one: both of its
# spellings cover a variant, and a missing variant is named.
test test_imported_enum_matches_are_exhaustive_through_either_spelling { |ctx|
  let root = test.temp_dir(ctx, name: "match-exhaustive-module")?
  fp"{root}/kinds.xsh".write_atomic(kinds_module)
  let module_env = {XSH_MODULE_PATH: root.display()}
  let covered = test.expect(
    ctx,
    """use kinds as k

pure label(kind: k.Kind) -> Str {
  let short = match kind {
    k.File => "file",
    k.Tree(_) | .Binary => "other",
  }
  match kind {
    k.File => return f"{short} file"
    k.Binary => return f"{short} binary"
    k.Tree(depth) => return f"{short} tree {depth}"
  }
}

print label(k.File)
print label(k.Binary)
print label(k.Tree(2))
""",
    status: 0,
    args: [],
    env: module_env,
  )?
  assert covered.stdout == "file file\nother binary\nother tree 2\n", covered.stdout

  let rejected = test.expect(
    ctx,
    """use kinds as k

const kind: k.Kind = k.Binary
match kind {
  k.File => print "file"
}
""",
    status: 2,
    args: [],
    env: module_env,
  )?
  assert rejected.stdout == ""
  assert "non-exhaustive match: missing variant(s) `Binary, Tree`" in rejected.stderr, rejected.stderr
  assert count(rejected.stderr, "err[") == 1, rejected.stderr
}

# The checker enumerates enums only. Every other subject keeps the run-time
# failure, which is also what stops a match the checker could not see through.
test test_unenumerated_subjects_still_fail_with_match_no_arm { |ctx|
  let output = test.run_script(
    ctx,
    """let code = 3
print "before"
match code {
  1 => print "one"
  2 => print "two"
}
print "after"
""",
  )?
  assert output.status != 0
  assert output.stdout == "before\n", output.stdout
  assert "match-no-arm" in output.stderr, output.stderr
}

error FetchError {
    Usage
    Offline : Timeout
    Rejected(url: Str, status: Int)
}

# A value of one declared error family is closed the same way. `is Facet`
# covers the variants that implement the facet.
pure failure_label(failure: FetchError) -> Str {
  let kind = match failure {
    FetchError.Usage => "usage",
    is Timeout => "timeout",
    FetchError.Rejected {status, ..} => f"rejected {status}",
  }
  var label = kind
  match failure {
    FetchError.Usage {message} => label = f"{kind}: {message}"
    FetchError.Offline => {}
    FetchError.Rejected {url, ..} => label = f"{kind} {url}"
  }

  label
}

test test_error_family_matches_covering_every_variant_run {
  assert failure_label(FetchError.Usage("bad flag")) == "usage: bad flag"
  assert failure_label(FetchError.Offline()) == "timeout"
  assert failure_label(FetchError.Rejected(url: "u", status: 503)) == "rejected 503 u"
}

test test_statement_match_missing_an_error_variant_is_a_check_error { |ctx|
  let stderr = check_errors(
    ctx,
    """error FetchError {
    Usage
    Offline : Timeout
    Rejected(url: Str, status: Int)
}

proc report(failure: FetchError) [io] {
  match failure {
    FetchError.Usage => print "usage"
    FetchError.Rejected {status: 503, ..} => print "busy"
  }
  match failure {
    FetchError.Usage => print "usage"
    is Timeout => print "timeout"
  }
  match failure {
    FetchError.Usage => print "usage"
    else => print "other"
  }
}

report(FetchError.Offline())
""",
  )?
  assert "non-exhaustive match: missing variant(s) `Offline, Rejected`" in stderr, stderr
  assert "non-exhaustive match: missing variant(s) `Rejected`" in stderr, stderr
  assert "not every variant of this error family is handled" in stderr, stderr
  assert count(stderr, "err[check.non-exhaustive-match]") == 2, stderr
  assert count(stderr, "err[") == 2, stderr
  assert ":8:" in stderr, stderr
  assert ":12:" in stderr, stderr
}

test test_value_match_over_an_error_family_names_its_missing_variants { |ctx|
  let stderr = check_errors(
    ctx,
    """error FetchError = Usage | Offline : Timeout | Rejected(url: Str, status: Int)

pure label(failure: FetchError) -> Str {
  match failure {
    FetchError.Usage => "usage"
    FetchError.Offline => "offline"
  }
}

print label(FetchError.Offline())
""",
  )?
  assert "value-producing match must be exhaustive: missing variant(s) `Rejected`" in stderr, stderr
  assert count(stderr, "err[") == 1, stderr
}

# `Error` and a `Result` name no single closed family, so a match over them
# is not enumerated and an unmatched error is the run-time failure.
test test_broad_errors_and_results_keep_the_run_time_failure { |ctx|
  let output = test.run_script(
    ctx,
    """error FetchError = Usage | Offline : Timeout

pure fetch(step: Int) -> Result[Int, FetchError] {
  return Err(.Offline()) when step == 1
  Ok(step)
}

for step in [0, 1] {
  match fetch(step) {
    Ok(value) => print \$value
    Err(.Usage) => print "usage"
  }
}
""",
  )?
  assert output.status != 0
  assert output.stdout == "0\n", output.stdout
  assert "match-no-arm" in output.stderr, output.stderr
}

# A family is closed for `match` only. A pattern condition whose variant or
# facet patterns happen to cover every variant is still an accepted
# condition; only a catch-all cannot fail.
test test_pattern_conditions_over_a_family_stay_refutable { |ctx|
  let accepted = test.expect(
    ctx,
    """error FetchError = Usage(message: Str) : NotFound | Gone(message: Str) : NotFound

let failure: FetchError = FetchError.Gone(message: "gone")
if let is NotFound = failure { print "facet" }
if let (FetchError.Usage {message} | FetchError.Gone {message}) as original = failure {
  print \$message \${original.message}
}
""",
    status: 0,
  )?
  assert accepted.stderr == "", accepted.stderr
  assert accepted.stdout == "facet\ngone gone\n", accepted.stdout

  let stderr = check_errors(
    ctx,
    """error FetchError = Usage(message: Str) | Gone(message: Str)

let failure: FetchError = FetchError.Gone(message: "gone")
if let whole = failure { print \${whole.message} }
""",
  )?
  assert count(stderr, "err[check.irrefutable-pattern-condition]") == 1, stderr
  assert count(stderr, "err[") == 1, stderr
}
