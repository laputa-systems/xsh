proc entry_count(directory: Path) [fs, error] -> Result[Int] {
  let entries = fs.children(directory)? |> collect()
  entries.len()
}

pure failure_message(result: Result[Unit]) -> Str {
  match result {
    Ok(_) => ""
    Err(failure) => failure.message
  }
}

test test_tempdir_creates_the_directory_and_removes_it_on_exit { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-basic")?
  let target = fp"{root}/scratch"
  var seen = false
  tempdir scratch at target {
    seen = entry_count(scratch)? == 0
    fp"{scratch}/nested/file".parent.mkdir()
    fp"{scratch}/nested/file".write("x")
  }

  assert seen
  assert ! target.exists()?
}

test test_tempdir_clears_what_is_at_the_path { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-clear")?
  let target = fp"{root}/scratch"
  target.write("a file, not a directory")
  tempdir scratch at target {
    assert entry_count(scratch)? == 0
  }

  fp"{target}/stale/deep".mkdir()
  fp"{target}/stale/deep/file".write("stale")
  var entries = -1
  tempdir scratch at target {
    entries = entry_count(scratch)?
  }

  assert entries == 0
  assert ! target.exists()?
}

test test_tempdir_evaluates_the_path_once { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-once")?
  var calls = 0
  tempdir scratch at {
    calls += 1
    fp"{root}/scratch-{calls}"
  } {
    assert scratch.name == "scratch-1"
  }

  assert calls == 1
  assert ! fp"{root}/scratch-1".exists()?
}

proc fail_inside(target: Path) [fs, error] {
  tempdir scratch at target {
    fp"{scratch}/partial".write("x")
    return error.fail("body failed")
  }
}

proc return_inside(target: Path) [fs, error] -> Result[Str] {
  tempdir scratch at target {
    return "early" when scratch.exists()
  }

  "late"
}

test test_tempdir_removes_the_directory_however_the_body_leaves { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-leave")?
  let target = fp"{root}/scratch"
  let failed = try { fail_inside(target)? }
  assert "body failed" in failure_message(failed)
  assert ! target.exists()?

  assert return_inside(target)? == "early"
  assert ! target.exists()?

  var rounds = 0
  for _ in [1, 2, 3] {
    tempdir scratch at target {
      rounds += 1
      continue when rounds == 1
      break when scratch.exists()
    }
  }

  assert rounds == 2
  assert ! target.exists()?
}

test test_tempdir_removal_runs_after_the_body_defers { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-order")?
  let target = fp"{root}/scratch"
  var present_in_defer = false
  tempdir scratch at target {
    defer { present_in_defer = scratch.exists()? }
  }

  assert present_in_defer
  assert ! target.exists()?
}

proc staged(target: Path) [fs, error] -> Result[Str] {
  tempdir scratch at target {
    fp"{scratch}/stamp".write("staged")
    fp"{scratch}/stamp".read_text()?
  }
}

# As the tail of a function that returns `Result[T]` the scope is that
# `Result`: `Ok` of the body's tail, computed before the directory goes.
test test_tempdir_as_a_tail_produces_the_body_tail { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-tail")?
  let target = fp"{root}/scratch"
  assert staged(target)? == "staged"
  assert ! target.exists()?
}

test test_tempdir_nests { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-nest")?
  var inner_parent = ""
  tempdir outer at fp"{root}/a" {
    tempdir inner at fp"{outer}/b" {
      inner_parent = inner.parent.name
    }

    # The inner directory is gone before the outer body goes on.
    assert outer.name == "a"
    assert entry_count(outer)? == 0
  }

  assert inner_parent == "a"
  assert ! fp"{root}/a".exists()?
}

test test_tempdir_and_at_stay_ordinary_names { |ctx|
  let base = test.temp_dir(ctx, name: "tempdir-names")?
  var tempdir = 0
  tempdir at at fp"{base}/x" {
    tempdir += 1
    assert at.name == "x"
  }

  assert tempdir == 1
}

test test_tempdir_at_script_top_level { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-top")?
  let output = test.expect(
    ctx,
    f"""tempdir scratch at p"{root}/scratch" {{
  print \$scratch.name
  print \${{scratch.exists()?}}
}}
print \${{p"{root}/scratch".exists()?}}
""",
    status: 0,
  )?
  assert output.stdout == "scratch\ntrue\nfalse\n"
}

test test_tempdir_path_must_be_a_path { |ctx|
  let output = test.run_script(
    ctx,
    """let text = ["/tmp/never"][0]
tempdir scratch at text {
  print "never"
}
""",
  )?
  assert output.status != 0
  assert "check.type-mismatch" in output.stderr
  assert "expected Path, found Str" in output.stderr
  # One diagnostic, on the path the user wrote.
  assert output.stderr.split("check.type-mismatch").len() == 2, output.stderr
  assert ":2:20" in output.stderr
  assert output.stdout == ""
}

test test_tempdir_name_is_immutable_and_scoped_to_the_body { |ctx|
  let reassigned = test.run_script(
    ctx,
    """tempdir scratch at p"/tmp/never" {
  scratch = p"/tmp/other"
}
""",
  )?
  assert reassigned.status != 0
  assert ":2:" in reassigned.stderr

  let escaped = test.run_script(
    ctx,
    """tempdir scratch at p"/tmp/never" {
}
print \$scratch
""",
  )?
  assert escaped.status != 0
  assert ":3:" in escaped.stderr
}

# The path is a head expression like the source of a `for`: it may break
# lines inside brackets.
test test_tempdir_path_may_span_lines { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-lines")?
  var name = ""
  tempdir scratch at [
    fp"{root}/first",
    fp"{root}/second",
  ][1] {
    name = scratch.name
  }

  assert name == "second"
}

# The three words that begin the scope are on one line; anything else is a
# statement that begins with a name.
test test_tempdir_words_are_on_one_line { |ctx|
  let output = test.run_script(
    ctx,
    """tempdir scratch
  at p"/tmp/never" {
  print "never"
}
""",
  )?
  assert output.status != 0
  assert output.stdout == ""
}

test test_tempdir_needs_the_fs_effect { |ctx|
  let output = test.run_script(
    ctx,
    """proc stage(target: Path) [error] {
  tempdir scratch at target {
    print \$scratch
  }
}
""",
  )?
  assert output.status != 0
  assert "fs" in output.stderr
  assert ":2:" in output.stderr
}

test test_tempdir_creation_failure_stops_before_the_body { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-create")?
  let blocker = fp"{root}/file"
  blocker.write("not a directory")
  var ran = false
  let outcome = tempdir scratch at fp"{blocker}/below" {
    ran = scratch.exists()?
  }
  assert failure_message(outcome) != ""
  assert ! ran
  assert blocker.read_text()? == "not a directory"
}

proc stage_below(parent: Path) [fs, error] {
  tempdir scratch at fp"{parent}/below" {
    print $scratch
  }
  print "after"
}

# In statement position a failure to clear or create the directory leaves the
# function, as any failing statement does; a `?` says the same.
test test_tempdir_statement_propagates_a_creation_failure { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-statement")?
  let blocker = fp"{root}/file"
  blocker.write("not a directory")
  let bare = try { stage_below(blocker)? }
  assert failure_message(bare) != ""

  var ran = false
  let marked = try {
    tempdir scratch at fp"{blocker}/below" {
      ran = scratch.exists()?
    }?
    "after"
  }
  assert marked is Err(_)
  assert ! ran
}

# In value position the scope is `Result[T]` of the body's tail, computed
# before the directory is removed.
test test_tempdir_value_scope_returns_the_tail_as_a_result { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-value")?
  let target = fp"{root}/scratch"
  let count = tempdir scratch at target {
    fp"{scratch}/a".write("1")
    fp"{scratch}/b".write("2")
    entry_count(scratch)?
  }
  assert count == Ok(2)
  assert ! target.exists()?

  let name = tempdir scratch at target { scratch.name }?
  assert name == "scratch"
}

# A producer that yields from inside the scope keeps its directory between
# pulls, and loses it when it ends or its consumer stops early.
test test_tempdir_producer_keeps_its_directory_between_pulls { |ctx|
  let output = test.expect(
    ctx,
    r"""
stream staged_names(target: Path) [fs, error] -> Stream[Str] {
  tempdir scratch at target {
    fp"{scratch}/stamp".write("x")
    for name in ["a", "b"] {
      yield f"{name} {fp"{scratch}/stamp".exists()?}"
    }
  }
}
let root = fs.tempdir()?
defer root.close()?
let target = fp"{root.host_path()?}/scratch"
print ${(staged_names(target) |> collect()).join(",")} ${target.exists()?}
print ${staged_names(target) |> take(1) |> first()?} ${target.exists()?}
""",
    status: 0,
  )?
  assert output.stdout == "a true,b true false\na true false\n"
}

# A `Result` tail stays nested, and so does the scope as the tail of `try`:
# the outer `Result` is the directory's, the inner one the body's.
test test_tempdir_result_tail_stays_nested { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-nested")?
  let target = fp"{root}/scratch"
  let read = tempdir scratch at target {
    try { fp"{scratch}/missing".read_text()? }
  }
  assert read is Ok(Err(_))

  let captured = try {
    tempdir scratch at target { scratch.name }
  }
  assert captured == Ok(Ok("scratch"))
  assert ! target.exists()?
}

# The removal is deferred, so its failure on exit is not the scope's `Err`:
# when the body succeeded it leaves the function as the failure of a deferred
# action does, and when the body failed, the body's failure stays primary.
test test_tempdir_removal_failure_on_exit { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-exit")?
  let script = f"""proc stage(fail: Bool) [fs, error] -> Result[Unit] {{
  let parent = p"{root}/locked"
  fs.mkdir(parent)
  defer fs.chmod(parent, 0o755)
  tempdir scratch at fp"{{parent}}/scratch" {{
    fs.chmod(parent, 0o555)
    print "body"
    return error.fail("body failed") when fail
  }}
  print "never"
}}

for fail in [false, true] {{
  match try {{ stage(fail)? }} {{
    Ok(_) => print "ok"
    Err(failure) => print f"failed: {{failure.message}}"
  }}
}}
"""
  let output = test.expect(ctx, script, status: 0)?
  let lines = output.stdout.lines()
  assert lines.len() == 4, output.stdout
  assert lines[0] == "body"
  assert lines[1].starts_with("failed: "), output.stdout
  assert "body failed" not in lines[1]
  assert "locked" in lines[1], output.stdout
  assert lines[2] == "body"
  assert lines[3] == "failed: body failed", output.stdout
}

test test_tempdir_formats_and_lints_as_written { |ctx|
  let root = test.temp_dir(ctx, name: "tempdir-fmt")?
  let source = f"""proc stage(root: Path) [fs, error] -> Result[Str] {{
  let second = fp"{{root}}/second"
  fs.remove(second, missing_ok: true)?
  fs.mkdir(second)?
  defer fs.remove(second, missing_ok: true)?
  tempdir   first   at   fp"{{second}}/first"{{
    fp"{{first}}/stamp".write("one")?
  }}
  fp"{{second}}/stamp".write("two")?
  fp"{{second}}/stamp".read_text()?
}}

print stage(p"{root}")?
"""
  let before = test.expect(ctx, source, status: 0)?
  assert before.stdout == "two\n"
  let candidate = test.temp_file(ctx, name: "tempdir.xsh", contents: bytes.from_text(source))?

  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert """  tempdir first at fp"{second}/first" {
    fp"{first}/stamp".write("one")?
  }
""" in candidate.read_text()?

  # The lint reports the written statements, never a `tempdir` scope.
  let linted = run.capture --text --accept=[0, 1] "xsht" lint $candidate
  let report = linted.stdout + linted.stderr
  assert report.split("lint.prefer-tempdir").len() == 2, report
  let fixed = run.capture --text "xsht" lint --fix $candidate
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert """  tempdir second at fp"{root}/second" {
    tempdir first at fp"{second}/first" {
""" in rewritten, rewritten
  assert """    fp"{second}/stamp".read_text()?
  }
}
""" in rewritten, rewritten
  assert "defer" not in candidate.read_text()?

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == before.stdout
}

# The scope calls nothing a pattern over `fs` calls could match.
test test_tempdir_scope_is_no_fs_call_to_grep { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "tempdir-grep.xsh",
    contents: bytes.from_text("""tempdir scratch at p"/tmp/never" {
  print "in"
}
fs.mkdir(p"/tmp/other")
"""),
  )?
  let found = run.capture --text "xsht" grep "fs.mkdir(P)" $candidate
  assert found.stdout.split("fs.mkdir(").len() == 2, found.stdout
  assert ":4:" in found.stdout
}
