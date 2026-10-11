# A real lock from another evaluator cannot select a local lock slot. This
# preserves cleanup-failure coverage without constructing an opaque identity.
# Two workers and inputs avoid the serial path that uses the caller evaluator.
proc foreign_lock(lock_path: Path) [fs, error] -> Result[FsLock] {
  let locks = [lock_path, fp"{lock_path}.other"] |> par-map(jobs: 2) { |lock_path| fs.lock(lock_path)? }
  locks[0]
}

proc lock_available(lock_path: Path) [fs, error] -> Bool {
  match fs.lock(lock_path, nonblocking: true) {
    Ok(lock) => { fs.unlock(lock); true }
    Err(_) => false
  }
}

# Whether `lock` is still held: releasing a lock the scope already released
# reports that the handle is not active.
proc was_released(lock: FsLock) [fs, error] -> Bool {
  match fs.unlock(lock) {
    Ok(_) => false
    Err(failure) => "not active" in failure.message
  }
}

proc was_closed(root: FsRoot) [fs, error] -> Bool {
  match root.close() {
    Ok(_) => false
    Err(failure) => "not active" in failure.message
  }
}

test test_with_binds_the_resource_and_releases_it_when_the_body_ends { |ctx|
  let dir = test.temp_dir(ctx, name: "with-basic")?
  var scratch = dir
  with root = fs.tempdir()? {
    scratch = root.host_path()?
    root.write(p"note", "kept until the block ends\n")
    assert fp"{scratch}/note".exists()?
  }

  assert scratch != dir
  assert ! scratch.exists()?

  let lock = with held = fs.lock(fp"{dir}/lock")? {
    assert ! held.shared
    held
  }?
  assert was_released(lock)
}

test test_with_in_value_position_is_a_result_of_the_body_tail { |ctx|
  let dir = test.temp_dir(ctx, name: "with-value")?
  fp"{dir}/data".write("payload\n")
  let length = with root = fs.open_root(dir)? { root.read_text(p"data")?.byte_len() }?
  assert length == 8

  let result = with root = fs.open_root(dir)? { root.exists(p"data")? }
  assert result is Ok(true)

  # A `Result` tail stays nested.
  let nested = with root = fs.open_root(dir)? { root.read_text(p"absent") }
  assert nested is Ok(Err(_))
}

proc tail_of_a_result_function(dir: Path) [fs, error] -> Result[Int] {
  with root = fs.open_root(dir)? {
    root.read_text(p"data")?.byte_len()
  }
}

test test_with_as_the_tail_of_a_result_function_is_its_result { |ctx|
  let dir = test.temp_dir(ctx, name: "with-tail")?
  fp"{dir}/data".write("four")
  assert tail_of_a_result_function(dir)? == 4
}

test test_with_enters_left_to_right_and_releases_every_resource { |ctx|
  let dir = test.temp_dir(ctx, name: "with-several")?
  fp"{dir}/inner".mkdir()
  var outer_lock: FsLock? = null
  var inner_root = null
  # The second value is opened through the first, so the first is in scope.
  with
    held = fs.lock(fp"{dir}/lock")?,
    root = fs.open_root(dir)?,
    inner = root.open_root(p"inner")?
  {
    inner.write(p"file", "x")
    outer_lock = held
    inner_root = inner
  }

  assert fp"{dir}/inner/file".exists()?
  assert was_released(outer_lock.require(FsLock)?)
  if let root = inner_root {
    assert was_closed(root)
  } else {
    test.fail("the body did not run")
  }
}

proc opens_then_fails(dir: Path, seen: Path) [fs, error] {
  with
    held = fs.lock(fp"{dir}/lock")?,
    root = {
      seen.write("entered")
      fs.open_root(fp"{dir}/absent")?
    }
  {
    root.write(p"never", "the body does not run")
  }
}

test test_with_releases_what_was_opened_when_a_later_value_fails { |ctx|
  let dir = test.temp_dir(ctx, name: "with-open-failure")?
  let seen = fp"{dir}/seen.json"
  let outcome = try {
    opens_then_fails(dir, seen)
    0
  }
  assert outcome is Err(_)
  assert seen.read_text()? == "entered"
  assert lock_available(fp"{dir}/lock")
}

proc leaves_by_return(dir: Path, seen: Path) [fs, error] {
  with held = fs.lock(fp"{dir}/lock")? {
    seen.write("entered")
    return
  }
}

proc leaves_by_propagation(dir: Path, seen: Path) [fs, error] -> Result[Int] {
  with held = fs.lock(fp"{dir}/lock")? {
    seen.write("entered")
    let _ = fp"{dir}/absent".read_text()?
  }

  0
}

proc leaves_by_fail(dir: Path, seen: Path) [fs, error] {
  with held = fs.lock(fp"{dir}/lock")? {
    seen.write("entered")
    fail "the body gave up"
  }
}

test test_with_releases_on_return_propagation_and_fail { |ctx|
  let dir = test.temp_dir(ctx, name: "with-exits")?
  let seen = fp"{dir}/seen.json"
  leaves_by_return(dir, seen)
  assert seen.read_text()? == "entered"
  assert lock_available(fp"{dir}/lock")

  assert leaves_by_propagation(dir, seen) is Err(_)
  assert seen.read_text()? == "entered"
  assert lock_available(fp"{dir}/lock")

  match leaves_by_fail(dir, seen) {
    Ok(_) => test.fail("the body's failure was lost")
    Err(failure) => assert failure.message == "the body gave up"
  }

  assert seen.read_text()? == "entered"
  assert lock_available(fp"{dir}/lock")
}

test test_with_releases_on_break_and_continue { |ctx|
  let dir = test.temp_dir(ctx, name: "with-loop")?
  var locks = []
  for round in [1, 2, 3] {
    with held = fs.lock(fp"{dir}/lock")? {
      locks += [held]
      continue when round == 1
      break when round == 2
    }
  }

  assert locks.len() == 2
  for lock in locks {
    assert was_released(lock)
  }
}

test test_with_releases_on_exit_and_at_a_within_deadline { |ctx|
  let dir = test.temp_dir(ctx, name: "with-exit")?
  let seen = fp"{dir}/scratch-path"
  let exited = test.expect(
    ctx,
    """
    cli main(seen: Path) {
      with root = fs.tempdir()? {
        seen.write(root.host_path()?.display())
        exit 4
      }
    }
    """,
    status: 4,
    args: [seen],
  )?
  assert exited.stderr == "", exited.stderr
  assert ! fp"{seen.read_text()?}".exists()?

  let timed_out = test.expect(
    ctx,
    """
    cli main(seen: Path) {
      let outcome = within 100ms {
        with root = fs.tempdir()? {
          seen.write(root.host_path()?.display())
          time.sleep(30s)
        }
      }
      print f"timed out: {outcome is Err(is Timeout)}"
    }
    """,
    status: 0,
    stdout: ["timed out: true"],
    args: [seen],
  )?
  assert timed_out.stderr == "", timed_out.stderr
  assert ! fp"{seen.read_text()?}".exists()?
}

proc release_fails_in_statement_position(lock: FsLock) [fs, error] -> Result[Str] {
  with held = lock {
    assert ! held.shared
  }

  "the failed release did not propagate"
}

test test_with_release_failure_after_a_finished_body_is_the_scope_failure { |ctx|
  let dir = test.temp_dir(ctx, name: "with-foreign-release")?
  let foreign = foreign_lock(fp"{dir}/lock")?
  # In value position the failure is the scope's `Err`, and nothing propagates.
  let outcome = with held = foreign { held.path }
  match outcome {
    Ok(_) => test.fail("the failed release was lost")
    Err(failure) => assert failure.message == "lock handle is not active"
  }

  # In statement position it propagates, as a failed statement does.
  match release_fails_in_statement_position(foreign) {
    Ok(text) => test.fail(text)
    Err(failure) => assert failure.message == "lock handle is not active"
  }
}

test test_with_releases_the_rest_when_one_release_fails { |ctx|
  let dir = test.temp_dir(ctx, name: "with-release-failure")?
  var first: FsLock? = null
  var last: FsLock? = null
  let foreign = foreign_lock(fp"{dir}/foreign")?
  let outcome = with a = fs.lock(fp"{dir}/a")?, broken = foreign, c = fs.lock(fp"{dir}/c")? {
    first = a
    last = c
    broken.path
  }
  assert outcome is Err(_)
  assert was_released(first.require(FsLock)?)
  assert was_released(last.require(FsLock)?)
}

test test_with_body_failure_wins_over_a_release_failure { |ctx|
  let failed = test.expect(
    ctx,
    """
    proc foreign_lock(lock_path: Path) [fs, error] -> Result[FsLock] {
      let locks = [lock_path, fp"{lock_path}.other"] |> par-map(jobs: 2) { |lock_path| fs.lock(lock_path)? }
      locks[0]
    }

    proc body_fails(lock: FsLock) [fs, error] {
      with held = lock {
        fail "the body failed first"
      }
    }

    tempdir dir { body_fails(foreign_lock(fp"{dir}/lock")?) }
    """,
    status: 3,
    stderr: ["the body failed first", "cleanup error [fs-lock]", "lock handle is not active"],
  )?
  assert failed.stdout == "", failed.stdout
}

test test_with_accepts_a_resource_the_body_already_released { |ctx|
  let dir = test.temp_dir(ctx, name: "with-double-close")?
  let closed_early = with root = fs.open_root(dir)? {
    root.close()
    1
  }
  assert closed_early is Ok(1)

  let unlocked_early = with held = fs.lock(fp"{dir}/lock")? {
    fs.unlock(held)
    2
  }
  assert unlocked_early is Ok(2)
}

test test_with_underscore_holds_a_resource_without_a_name { |ctx|
  let dir = test.temp_dir(ctx, name: "with-underscore")?
  let outcome = with _ = fs.lock(fp"{dir}/lock")? { 3 }
  assert outcome is Ok(3)
}

test test_with_runs_the_body_defers_before_it_releases { |ctx|
  let dir = test.temp_dir(ctx, name: "with-defer-order")?
  var at_defer = false
  with root = fs.open_root(dir)? {
    defer {
      at_defer = root.exists(p".") ?? false
    }
  }

  assert at_defer
}

test test_with_binding_shadows_a_constant_and_a_function_of_its_name { |ctx|
  let shadowing = test.expect(
    ctx,
    """
    const root = p"/nonexistent/constant"

    proc held() -> Str {
      "a function"
    }

    proc probe(dir: Path) [fs, error] -> Result[Str] {
      with root = fs.open_root(dir)?, held = fs.lock(fp"{dir}/lock")? {
        root.write(p"file", "x")
        f"{root.exists(p"file")?} {held.shared}"
      }
    }

    tempdir dir {
      print probe(dir)?
      print f"{root} {held()}"
    }
    """,
    status: 0,
    stdout: ["true false", "/nonexistent/constant a function"],
  )?
  assert shadowing.stderr == "", shadowing.stderr
}

test test_with_without_else_rejects_a_value_that_is_not_a_resource { |ctx|
  let not_a_resource = test.expect(
    ctx,
    """
    with text = "plain" {
      print $text
    }
    """,
    status: 2,
    stderr: [
      "check.with-resource",
      "`Str` is not one; the resource types are `FsRoot` and `FsLock`",
      "with ... { ... } else { ... }",
    ],
  )?
  assert not_a_resource.stdout == "", not_a_resource.stdout

  let not_propagated = test.expect(
    ctx,
    """
    with root = fs.open_root(p".") {
      print root.exists(p".")?
    }
    """,
    status: 2,
    stderr: ["check.with-resource", "propagate the `Result` with `?`"],
  )?
  assert not_propagated.stdout == "", not_propagated.stdout

  let dynamic = test.expect(
    ctx,
    """
    proc manage(value: Any) [fs, error] {
      with held = value {
        print "unreachable"
      }
    }

    manage(1)
    """,
    status: 2,
    stderr: ["check.with-resource", "`Any` is not one"],
  )?
  assert dynamic.stdout == "", dynamic.stdout
}

test test_with_without_else_is_rejected_in_a_pure_function { |ctx|
  let pure_scope = test.expect(
    ctx,
    """
    pure peek(root: FsRoot) -> Int {
      with held = root {
        1
      }

      2
    }
    """,
    status: 2,
    stderr: ["check.pure-effect", "a `with` resource scope is not allowed in pure functions"],
  )?
  assert pure_scope.stdout == "", pure_scope.stdout
}

test test_with_else_still_groups_fallible_bindings { |ctx|
  let dir = test.temp_dir(ctx, name: "with-else")?
  fp"{dir}/a".write("1")
  var message = ""
  with a = fp"{dir}/a".read_text()?, b = fp"{dir}/b".read_text()? {
    message = a + b
  } else { |failure|
    message = "handled"
  }

  assert message == "handled"
}

test test_with_formats_on_one_line_or_one_binding_to_a_line { |ctx|
  let unformatted = """
    proc sizes(dir: Path) [fs, error] -> Result[Int] {
      with   root = fs.open_root(dir)?,lock=fs.lock(fp"{dir}/lock")?   {
        root.write(p"a", "b")
      }
      let n = with
        root = fs.open_root(dir)?, lock = fs.lock(fp"{dir}/lock")? { 1 }?
      n
    }
    """
  let file = test.temp_file(ctx, name: "format.xsh", contents: bytes.from_text(unformatted))?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.ok, formatted.stderr
  let expected = """
    proc sizes(dir: Path) [fs, error] -> Result[Int] {
      with root = fs.open_root(dir)?, lock = fs.lock(fp"{dir}/lock")? {
        root.write(p"a", "b")
      }
      let n = with
        root = fs.open_root(dir)?,
        lock = fs.lock(fp"{dir}/lock")?
      { 1 }?
      n
    }
    """
  assert file.read_text()? == f"{expected}\n"
  let again = run.capture --text "xsht" fmt --check $file
  assert again.status.ok, again.stdout
}
