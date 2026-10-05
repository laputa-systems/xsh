proc names(directory: Path) [fs, error] -> Result[List[Str]] {
  fs.children(directory)? |> sort-by .name |> map .name |> collect()
}

pure failure_message(result: Result[Unit]) -> Str {
  match result {
    Ok(_) => ""
    Err(failure) => failure.message
  }
}

test test_atomically_replaces_the_destination_after_the_body { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-basic")?
  let image = fp"{root}/image.tar"
  image.write("old")
  var during = ""
  var partial_name = ""
  atomically replace image as partial {
    partial.write("new")
    # The destination is the old file until the body has finished.
    during = image.read_text()?
    partial_name = partial.name()
    assert partial.parent() == image.parent()
  }

  assert during == "old"
  # A hidden sibling named after the destination, with a part drawn anew.
  assert partial_name.starts_with(".image.tar.")
  assert partial_name.ends_with(".tmp")
  assert partial_name != ".image.tar..tmp"
  assert image.read_text()? == "new"
  assert names(root)? == ["image.tar"]
}

test test_atomically_creates_a_destination_that_is_absent { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-create")?
  let file = fp"{root}/file"
  let source = fp"{root}/source"
  source.write("produced")
  atomically replace file as partial {
    run cp $source $partial
  }

  assert file.read_text()? == "produced"

  # What the body produces may be a directory.
  let tree = fp"{root}/tree"
  atomically replace tree as staging {
    fp"{staging}/a/b".mkdir()
    fp"{staging}/a/b/leaf".write("leaf")
  }

  assert fp"{tree}/a/b/leaf".read_text()? == "leaf"
  assert names(root)? == ["file", "source", "tree"]
}

# A run that crashed leaves its temporary file behind. A later run draws a
# name of its own, so the leftover is neither published nor removed.
test test_atomically_never_publishes_what_an_earlier_run_left { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-stale")?
  let dest = fp"{root}/out"
  let crashed = test.run_script(
    ctx,
    f"""atomically replace p"{dest}" as partial {{
  partial.write("half written")
  process.kill(process.current_pid()?, signal: "KILL")
}}
""",
  )?
  assert crashed.status != 0
  let left = names(root)?
  assert left.len() == 1, crashed.stderr
  let leftover = fp"{root}/{left[0]}"
  assert leftover.read_text()? == "half written"
  assert ! dest.exists()?

  var fresh = fp"{root}/unset"
  atomically replace dest as partial {
    fresh = partial
    assert ! partial.exists()?
    partial.write("fresh")
  }

  assert fresh != leftover
  assert dest.read_text()? == "fresh"
  assert leftover.read_text()? == "half written"
  assert names(root)? == [leftover.name(), "out"]
}

# Two writers that are live at once, to two destinations in one directory
# and to one destination, each have a temporary file of their own.
test test_atomically_writers_never_share_a_temporary_file { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-writers")?
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  var live = []
  atomically replace first as outer {
    outer.write("first")
    atomically replace second as inner {
      inner.write("second")
      atomically replace first as again {
        again.write("first again")
        live = names(root)?
        assert outer != again
        assert outer.read_text()? == "first"
      }
    }

    # The inner writer to the same destination published first.
    assert first.read_text()? == "first again"
    assert second.read_text()? == "second"
  }

  assert live.len() == 3, live.join(" ")
  assert first.read_text()? == "first"
  assert names(root)? == ["first", "second"]
}

test test_fs_temp_sibling_names_an_unused_hidden_path_and_creates_nothing { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-sibling")?
  let dest = fp"{root}/image.tar"
  let one = fs.temp_sibling(dest)?
  let two = fs.temp_sibling(dest)?
  assert one != two
  for sibling in [one, two] {
    assert sibling.parent() == dest.parent()
    assert sibling.name().starts_with(".image.tar.")
    assert sibling.name().ends_with(".tmp")
    assert ! sibling.exists()?
  }

  assert names(root)? == []

  # The result is spelled like the path it stands beside, and a directory
  # that does not exist is not an error here.
  let relative = fs.temp_sibling(p"missing-directory/out")?
  assert relative.parent() == p"missing-directory"
  assert fs.temp_sibling(p"out")?.parent() == p"out".parent()
  assert fs.temp_sibling(/) is Err(_)
}

proc publish(dest: Path, how: Str) [fs, error] -> Result[Str] {
  atomically replace dest as partial {
    partial.write(how)
    return "returned" when how == "return"
    if how == "fail" {
      error.fail("body failed")
    }
  }

  "published"
}

test test_atomically_leaves_the_destination_alone_unless_the_body_finishes { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-leave")?
  let dest = fp"{root}/out"
  dest.write("old")

  let failed = try {
    let _ = publish(dest, "fail")?
  }
  assert "body failed" in failure_message(failed)
  assert dest.read_text()? == "old"
  assert names(root)? == ["out"]

  # An early return is not an error, and it does not publish either.
  assert publish(dest, "return")? == "returned"
  assert dest.read_text()? == "old"
  assert names(root)? == ["out"]

  var rounds = 0
  for _ in [1, 2, 3] {
    atomically replace dest as partial {
      rounds += 1
      partial.write(f"round {rounds}")
      continue when rounds == 1
      break when rounds == 2
    }
  }

  assert rounds == 2
  assert dest.read_text()? == "old"
  assert names(root)? == ["out"]

  assert publish(dest, "new")? == "published"
  assert dest.read_text()? == "new"
}

test test_atomically_discards_the_temporary_file_on_exit { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-exit")?
  let dest = fp"{root}/out"
  dest.write("old")
  test.expect(
    ctx,
    f"""atomically replace p"{dest}" as partial {{
  partial.write("new")
  exit 3
}}
""",
    status: 3,
  )?
  assert dest.read_text()? == "old"
  assert names(root)? == ["out"]
}

# A body that leaves on every path only makes and discards a temporary file.
test test_atomically_lint_reports_a_body_that_always_leaves { |ctx|
  let source = """proc publish(dest: Path, ready: Bool) [fs, error] {
  atomically replace dest as partial {
    partial.write("x")
    return unless ready
  }

  for _ in [1, 2] {
    atomically replace dest as partial {
      partial.write("x")
      if ready {
        break
      } else {
        continue
      }
    }
  }
}
"""
  let candidate = test.temp_file(ctx, name: "atomically-never.xsh", contents: bytes.from_text(source))?
  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.atomically-never-replaces $candidate
  let report = linted.stdout + linted.stderr
  assert report.split("lint.atomically-never-replaces").len() == 2, report
  assert ":8:5" in report, report
  assert "discards the temporary file and publishes nothing" in report, report
}

test test_atomically_evaluates_the_destination_once { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-once")?
  var calls = 0
  atomically replace {
    calls += 1
    fp"{root}/out-{calls}"
  } as partial {
    partial.write("x")
  }

  assert calls == 1
  assert names(root)? == ["out-1"]
}

test test_atomically_fails_when_the_body_produced_nothing { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-nothing")?
  let dest = fp"{root}/out"
  dest.write("old")
  let outcome = try {
    atomically replace dest as partial {
      assert ! partial.exists()?
    }
  }
  assert failure_message(outcome) != ""
  assert dest.read_text()? == "old"

  # The destination's directory is not created.
  var ran = false
  let missing = try {
    atomically replace fp"{root}/absent/out" as partial {
      ran = true
      partial.write("x")
    }
  }
  assert ran
  assert failure_message(missing) != ""
  assert names(root)? == ["out"]
}

test test_atomically_renames_after_the_body_defers_and_removes_last { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-order")?
  let dest = fp"{root}/out"
  var at_defer = ""
  atomically replace dest as partial {
    defer {
      # The body's defers run while the file is still at the temporary path.
      at_defer = f"{partial.exists()?} {dest.exists()?}"
    }
    partial.write("x")
  }

  assert at_defer == "true false"
  assert names(root)? == ["out"]
}

proc publish_as_tail(dest: Path, produce: Bool) [fs, error] {
  atomically replace dest as partial {
    if produce {
      partial.write("tail")
    }
  }
}

# The statement has no value: a failed rename propagates from wherever the
# statement stands, the tail of a function or of a `try` block included.
test test_atomically_as_a_tail_propagates_a_failed_rename { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-tail")?
  let dest = fp"{root}/out"
  assert publish_as_tail(dest, true) is Ok(_)
  assert dest.read_text()? == "tail"
  assert publish_as_tail(dest, false) is Err(_)
  assert dest.read_text()? == "tail"
  assert names(root)? == ["out"]
}

test test_atomically_nests { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-nest")?
  let outer = fp"{root}/outer"
  atomically replace outer as staging {
    staging.mkdir()
    atomically replace fp"{staging}/inner" as partial {
      partial.write("inner")
    }

    # The inner file is in place before the outer body goes on.
    assert names(staging)? == ["inner"]
  }

  assert fp"{outer}/inner".read_text()? == "inner"
  assert names(root)? == ["outer"]
}

test test_atomically_words_stay_ordinary_names { |ctx|
  let base = test.temp_dir(ctx, name: "atomically-names")?
  let atomically = fp"{base}/x"
  var replace = 0
  atomically replace atomically as as {
    replace += 1
    as.write("as")
  }

  assert replace == 1
  assert atomically.read_text()? == "as"
  assert "a-b".replace("-", with: "+") == "a+b"
}

test test_atomically_at_script_top_level { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-top")?
  let output = test.expect(
    ctx,
    f"""atomically replace p"{root}/out" as partial {{
  print \$partial.name()
  partial.write("top")
}}
print \${{p"{root}/out".read_text()?}}
""",
    status: 0,
  )?
  let lines = output.stdout.lines()
  assert lines.len() == 2, output.stdout
  assert lines[0].starts_with(".out.") and lines[0].ends_with(".tmp"), output.stdout
  assert lines[1] == "top"
}

test test_atomically_destination_must_be_a_path { |ctx|
  let output = test.run_script(
    ctx,
    """let text = ["/tmp/never"][0]
atomically replace text as partial {
  print "never"
}
""",
  )?
  assert output.status != 0
  assert "check.type-mismatch" in output.stderr
  assert "expected Path, found Str" in output.stderr
  # One diagnostic, on the destination the user wrote.
  assert output.stderr.split("check.type-mismatch").len() == 2, output.stderr
  assert ":2:20" in output.stderr
  assert output.stdout == ""
}

test test_atomically_name_is_immutable_and_scoped_to_the_body { |ctx|
  let reassigned = test.run_script(
    ctx,
    """atomically replace p"/tmp/never" as partial {
  partial = p"/tmp/other"
}
""",
  )?
  assert reassigned.status != 0
  assert ":2:" in reassigned.stderr

  let escaped = test.run_script(
    ctx,
    """atomically replace p"/tmp/never" as partial {
}
print \$partial
""",
  )?
  assert escaped.status != 0
  assert ":3:" in escaped.stderr
}

# The body is a statement: its last statement may fail, and a value there is
# ignored like any other.
test test_atomically_body_value_is_rejected_as_ignored { |ctx|
  let output = test.run_script(
    ctx,
    """atomically replace p"/tmp/never" as partial {
  partial.write("x")
  1
}
""",
  )?
  assert output.status != 0
  assert "check.ignored-result" in output.stderr
  assert ":3:3" in output.stderr
}

# The destination is a head expression like the source of a `for`: it may
# break lines inside brackets.
test test_atomically_destination_may_span_lines { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-lines")?
  atomically replace [
    fp"{root}/first",
    fp"{root}/second",
  ][1] as partial {
    partial.write("x")
  }

  assert names(root)? == ["second"]
}

# The two words that begin the statement are on one line, and `as NAME`
# follows the destination; anything else is not this statement.
test test_atomically_head_is_checked { |ctx|
  let split = test.run_script(
    ctx,
    """atomically
  replace p"/tmp/never" as partial {
  print "never"
}
""",
  )?
  assert split.status != 0
  assert split.stdout == ""

  let unnamed = test.run_script(
    ctx,
    """atomically replace p"/tmp/never" {
  print "never"
}
""",
  )?
  assert unnamed.status != 0
  assert "expected `as NAME` after the destination" in unnamed.stderr
  assert ":1:" in unnamed.stderr
  assert unnamed.stdout == ""
}

test test_atomically_needs_the_fs_effect { |ctx|
  let output = test.run_script(
    ctx,
    """proc publish(dest: Path) [error] {
  atomically replace dest as partial {
    print \$partial
  }
}
""",
  )?
  assert output.status != 0
  assert "fs" in output.stderr
  assert ":2:" in output.stderr
}

test test_atomically_formats_and_lints_as_written { |ctx|
  let root = test.temp_dir(ctx, name: "atomically-fmt")?
  let source = f"""proc publish(dest: Path, text: Str) [fs, error] {{
  let partial = fp"{{dest}}.tmp"
  partial.remove(missing_ok: true)?
  defer partial.remove(missing_ok: true)?

  partial.write(text)
  if partial.read_text()? == "" {{
    return error.fail("empty")
  }}

  partial.rename(to: dest, overwrite: true)?
}}

proc stamp(dest: Path) [fs, error] {{
  atomically   replace   dest   as   partial{{
    partial.write("stamp")
  }}
}}

let out = p"{root}/out"
publish(out, "one")
print out.read_text()?
stamp(out)
print out.read_text()?
print \${{fs.children(p"{root}")? |> count()}}
"""
  let before = test.expect(ctx, source, status: 0)?
  assert before.stdout == "one\nstamp\n1\n"
  let candidate = test.temp_file(ctx, name: "atomically.xsh", contents: bytes.from_text(source))?

  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert """  atomically replace dest as partial {
    partial.write("stamp")
  }
""" in candidate.read_text()?

  # The lint reports the written statements, never the expansion of an
  # `atomically replace`.
  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-atomically-replace $candidate
  let report = linted.stdout + linted.stderr
  assert report.split("lint.prefer-atomically-replace").len() == 2, report
  let fixed = run.capture --text "xsht" lint --only lint.prefer-atomically-replace --fix $candidate
  assert fixed.status.exited_with(0), fixed.stderr
  let rewritten = candidate.read_text()?
  assert """proc publish(dest: Path, text: Str) [fs, error] {
  atomically replace dest as partial {
    partial.write(text)
    if partial.read_text()? == "" {
      return error.fail("empty")
    }
  }
}
""" in rewritten, rewritten
  assert ".rename(" not in rewritten

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, rewritten, status: 0)?
  assert after.stdout == before.stdout
}

# A sequence the form does not mean exactly is reported with the difference
# and left as written.
test test_atomically_lint_explains_a_sequence_it_does_not_rewrite { |ctx|
  let source = """proc publish(source: Path, dest: Path) [fs, error] {
  let partial = fp"{dest}.tmp"
  partial.remove(missing_ok: true)
  source.copy(to: partial)
  partial.rename(to: dest)
}
"""
  let candidate = test.temp_file(ctx, name: "atomically-note.xsh", contents: bytes.from_text(source))?
  let linted = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-atomically-replace $candidate
  let report = linted.stdout + linted.stderr
  assert report.split("lint.prefer-atomically-replace").len() == 2, report
  assert "no automatic rewrite" in report, report
  assert "two adjacent statements" in report, report
  let fixed = run.capture --text --accept=[0, 1] "xsht" lint --only lint.prefer-atomically-replace --fix $candidate
  assert fixed.stdout + fixed.stderr != ""
  assert candidate.read_text()? == source
}

test test_atomically_expansion_is_invisible_to_grep { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "atomically-grep.xsh",
    contents: bytes.from_text("""atomically replace p"/tmp/never/out" as partial {
  print "in"
}
p"/tmp/never/a".rename(to: p"/tmp/never/b")
"""),
  )?
  let found = run.capture --text "xsht" grep "A.rename(to: B)" $candidate
  assert found.stdout.split(".rename(").len() == 2, found.stdout
  assert ":4:" in found.stdout
}
