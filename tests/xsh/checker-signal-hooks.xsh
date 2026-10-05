# What `xsht check` reported for one program.
type Checked = {status: Status, stderr: Str}

# Runs only the checker over `source`: a script that declares a hook would
# install it.
proc check(ctx: TestContext, source: Str) [fs, process, error] -> Result[Checked] {
  let file = test.temp_file(ctx, name: "checked.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  {status: checked.status, stderr: checked.stderr}
}

test test_checker_accepts_entry_signal_hook_with_prior_bindings { |ctx|
  let checked = check(
    ctx,
    r"""
let marker = Path("/tmp/xsh-signal")

on SIGINT [fs, error] {
  marker.write("interrupted\n")?
  exit 130
}
""",
  )?
  assert checked.status.exited_with(0), checked.stderr
}

test test_checker_rejects_invalid_signal_hooks { |ctx|
  for case in [
    {
      source: "on 15 [] {\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on SIGKILL [] {\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on SIGCHLD [] {\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on SIGPIPE [] {\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on TERM [] {\n}\non SIGTERM [] {\n}\n",
      code: "check.duplicate-signal-hook",
    },
    {
      source: "on SIGINT [] {\n  later.remove()?\n}\nlet later = Path(\"/tmp/x\")\n",
      code: "check.unresolved-name",
    },
    {
      source: "proc bad() {\n  on SIGINT [] {\n  }\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "if true {\n  on SIGINT [] {\n  }\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "for value in [1] {\n  on SIGINT [] {\n  }\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "export on SIGINT [] {\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on SIGINT [] {\n  return\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on SIGINT [] {\n  break\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on SIGINT [] {\n  continue\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on SIGINT [] {\n  1\n}\n",
      code: "check.signal-hook",
    },
    {
      source: "on SIGINT [] {\n  time.sleep(1ms)?\n}\n",
      code: "check.effect-violation",
    },
    {
      source: "on SIGINT [error] {\n  Ok(\"bad\")\n}\n",
      code: "check.signal-hook",
    },
  ] {
    let checked = check(ctx, case.source)?
    assert checked.status.exited_with(2), f"{case.source}: {checked.stderr}"
    assert f"[{case.code}]" in checked.stderr, f"expected {case.code} for {case.source}: {checked.stderr}"
  }
}

test test_checker_accepts_platform_signal_hook_names { |ctx|
  let checked = check(
    ctx,
    r"""
on HUP [] {
}

on SIGUSR1 [] {
}

on ALRM [] {
}

on XCPU [] {
}

on XFSZ [] {
}
""",
  )?
  assert checked.status.exited_with(0), checked.stderr
}

test test_checker_rejects_signal_hooks_in_imported_modules { |ctx|
  let root = test.temp_dir(ctx, name: "hooks")?
  fp"{root}/helper.xsh".write("on SIGINT [] {\n}\n")
  fp"{root}/main.xsh".write("use helper\n")
  let checked = run.capture --text "xsht" check fp"{root}/main.xsh"
  assert checked.status.exited_with(2), checked.stderr
  assert "[check.signal-hook-module]" in checked.stderr, checked.stderr
}
