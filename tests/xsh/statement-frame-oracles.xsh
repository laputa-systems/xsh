test test_print_statement_runs_on_forced_frames { |ctx|
  let output = test.run_xsh(ctx, r"""proc mark(value: Str) -> Str {
  print f"eval:{value}"
  return value
}
print ${mark("first")} ${mark("second")}
""")?
  assert output.success, output.stderr
  assert output.stdout == """eval:first
eval:second
first second
"""
}

test test_cd_statement_runs_on_forced_frames { |ctx|
  let root = test.temp_dir(ctx, name: "statement-frame-cd")?
  let not_directory = test.temp_file(ctx, name: "statement-frame-file", contents: b"")?
  let output = test.run_xsht_trace(
    ctx,
    r"""let root = Path(args[0])
let not_directory = Path(args[1])
let original = fs.cwd()?
proc leave(root: Path) [fs, env, error] {
  cd $root {
    defer { print ${fs.cwd()? == root} }
    return
  }
}
leave(root)
assert fs.cwd()? == original
proc fail_scope(root: Path) [fs, env, error] -> Result[Unit] {
  cd $root {
    defer { print ${fs.cwd()? == root} }
    fail "cd body failed"
  }
  return Ok()
}
let failure = try { fail_scope(root)? }
assert failure is Err(_)
assert fs.cwd()? == original
proc break_scope(root: Path) [env, fs, error] {
  while true {
    cd $root {
      defer { print ${fs.cwd()? == root} }
      break
    }
    print "unreached"
  }
}
break_scope(root)
assert fs.cwd()? == original
proc rejected_scope(target: Path) [env, fs, error] -> Result[Unit] {
  cd $target { print "unreached" }
  return Ok()
}
let rejected = try { rejected_scope(not_directory)? }
assert rejected is Err(_)
assert fs.cwd()? == original
stream scoped_directories(root: Path) [env, fs, error] -> Stream[Bool] {
  cd $root {
    defer { print ${fs.cwd()? == root} }
    yield fs.cwd()? == root
    yield false
  }
}
for inside in scoped_directories(root) {
  print $inside ${fs.cwd()? == original}
  break
}
assert fs.cwd()? == original
print "restored"
""",
    ["--trace", "--raw"],
    [root.display(), not_directory.display()],
  )?
  assert output.success, output.stderr
  assert output.stdout == """true
true
true
true true
true
restored
"""
  assert "kind=cwd.enter" in output.stderr, output.stderr
  assert "kind=cwd.exit" in output.stderr, output.stderr
  assert "kind=core.call" in output.stderr, output.stderr
  assert "kind=core.result" in output.stderr, output.stderr
}

test test_env_statement_runs_on_forced_frames { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""let original = env.get_or("XSH_STATEMENT_FRAME", "absent")?
proc leave() [env, error] {
  env XSH_STATEMENT_FRAME=inside {
    defer { print e"XSH_STATEMENT_FRAME"? }
    return
  }
}
leave()
assert env.get_or("XSH_STATEMENT_FRAME", "absent")? == original
proc fail_scope() [env, error] -> Result[Unit] {
  env XSH_STATEMENT_FRAME=failed {
    defer { print e"XSH_STATEMENT_FRAME"? }
    fail "env body failed"
  }
  return Ok()
}
let failure = try { fail_scope()? }
assert failure is Err(_)
assert env.get_or("XSH_STATEMENT_FRAME", "absent")? == original
proc mark(value: Str) -> Str {
  print f"eval:{value}"
  return value
}
env XSH_STATEMENT_A=${mark("first")} XSH_STATEMENT_B=${mark("second")} {
  print e"XSH_STATEMENT_A"? e"XSH_STATEMENT_B"?
}
let search_paths = [/one, /two]
env XSH_STATEMENT_PATHS=$search_paths {
  assert env.PathList.XSH_STATEMENT_PATHS? == search_paths
}
stream scoped_values() [env, error] -> Stream[Str] {
  env XSH_STATEMENT_FRAME=producer {
    defer { print e"XSH_STATEMENT_FRAME"? }
    yield e"XSH_STATEMENT_FRAME"?
    yield "unreached"
  }
}
env XSH_STATEMENT_FRAME=consumer {
  for value in scoped_values() {
    print $value e"XSH_STATEMENT_FRAME"?
    break
  }
  print e"XSH_STATEMENT_FRAME"?
}?
assert env.get_or("XSH_STATEMENT_FRAME", "absent")? == original
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """inside
failed
eval:first
eval:second
first second
producer consumer
producer
consumer
"""
}
