type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file`, applies the fixes of `rule` alone, and returns
# the text the file then holds.
proc fixed_by(file: Path, rule: Str, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let fixed = lint(file, ["--fix", "--only", rule])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1) or fixed.status.exited_with(2), fixed.stderr
  file.read_text()
}

# Requires `file` to check without a diagnostic.
proc assert_checks(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

test test_callable_alias_forwarder_fix_preserves_signature_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "callable-alias")?
  let file = fp"{root}/forwarder.xsh"
  let source = "pure render(value: Str, prefix: Str = \"label:\") -> Str { prefix + value }\nexport pure format(value: Str, prefix: Str = \"label:\") -> Str { render(value, prefix) }\nprint format(value: \"one\")\n"
  file.write(source)
  let reported = lint(file, ["--only", "lint.prefer-callable-alias"])?
  assert "warn[lint.prefer-callable-alias]" in reported.stderr, reported.stderr
  assert "-> export let format = render\n" in reported.stderr, reported.stderr

  let fixed = fixed_by(file, "lint.prefer-callable-alias", source)?
  assert fixed == source.replace(
    "export pure format(value: Str, prefix: Str = \"label:\") -> Str { render(value, prefix) }",
    with: "export let format = render",
  )
  assert_checks(file)
  let again = lint(file, [])?
  assert "lint.prefer-callable-alias" not in again.stderr, again.stderr
}

test test_callable_alias_forwarder_fix_retains_policy_comments_and_argument_order { |ctx|
  let root = test.temp_dir(ctx, name: "callable-alias-refused")?
  let file = fp"{root}/forwarder.xsh"
  for source in [
    "pure render(value: Str) -> Str { value }; pure format(value: Str) -> Str { # preserve context\n render(value) }\n",
    "pure render(left: Str, right: Str) -> Str { left + right }; pure format(left: Str, right: Str) -> Str { render(right, left) }\n",
    "pure render(value: Str) -> Str { value }; pure format(value: Str) -> Str { render(value.trim()) }\n",
    "proc render(value: Str) [error] -> Result[Str] { Ok(value) }; proc format(value: Str) [error] -> Result[Str] { render(value)? }\n",
  ] {
    assert fixed_by(file, "lint.prefer-callable-alias", source)? == source, source
    let reported = lint(file, ["--only", "lint.prefer-callable-alias"])?
    assert "help: " not in reported.stderr, reported.stderr
  }
}

test test_fs_root_receiver_fix_checks_an_isolated_file_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "root-receiver")?
  let file = fp"{root}/root-receiver.xsh"
  file.write("proc old(root: FsRoot) [fs, error] {\n  fs.root_mkdir(root, p\"nested\", parents: true)?\n}\n")
  let first = lint(file, ["--fix"])?
  assert first.status.exited_with(0), first.stderr
  let fixed = file.read_text()?
  # A later round removes the `?`: a statement-position `Result[Unit]`
  # already propagates its failure.
  assert "  root.mkdir(p\"nested\", parents: true)\n" in fixed, fixed
  let second = lint(file, ["--fix"])?
  assert second.status.exited_with(0) or second.status.exited_with(1), second.stderr
  assert "lint.fs-root-receiver" not in second.stderr, second.stderr
  assert file.read_text()? == fixed
}

test test_fs_root_receiver_fix_preserves_named_argument_text_and_refuses_reordered_receiver { |ctx|
  let root = test.temp_dir(ctx, name: "root-receiver-text")?
  let file = fp"{root}/root-receiver.xsh"
  for case in [
    {
      call: "fs.root_mkdir(root, p\"nested\", parents: true)",
      fixed: "root.mkdir(p\"nested\", parents: true)",
    },
    {
      call: "fs.root_write(data: \"text\", root: root, path: p\"data\")",
      fixed: "fs.root_write(data: \"text\", root: root, path: p\"data\")",
    },
    {
      call: "fs.root_write(\n    root,\n    p\"data\",\n    b\"#bytes\",\n  )",
      fixed: "root.write(\n    p\"data\",\n    b\"#bytes\",\n  )",
    },
    {
      call: "fs.root_mkdir(root, # retain this ownership comment\n    p\"nested\")",
      fixed: "fs.root_mkdir(root, # retain this ownership comment\n    p\"nested\")",
    },
  ] {
    let source = "proc old(root: FsRoot) [fs, error] {\n  " + case.call + "?\n}\n"
    file.write(source)
    let before = run.capture --text "xsht" check $file
    assert "err[check.unsupported-api]" in before.stderr, before.stderr
    let fixed = fixed_by(file, "lint.fs-root-receiver", source)?
    assert fixed == "proc old(root: FsRoot) [fs, error] {\n  " + case.fixed + "?\n}\n", fixed
    if case.fixed != case.call {
      assert_checks(file)
      let again = lint(file, [])?
      assert "lint.fs-root-receiver" not in again.stderr, again.stderr
    }
  }
}

test test_fs_root_receiver_refuses_user_record_methods_and_forged_capabilities { |ctx|
  let root = test.temp_dir(ctx, name: "root-receiver-refused")?
  let file = fp"{root}/root-receiver.xsh"
  for source in [
    "proc old(root: {id: Int}) [fs, error] { fs.root_read(root, p\"data\")? }\n",
    "let fs = {root_read: pure(root: Int, path: Path) -> Int { root }}\nlet _ = fs.root_read(1, p\"data\")\n",
  ] {
    assert fixed_by(file, "lint.fs-root-receiver", source)? == source, source
    let reported = lint(file, ["--only", "lint.fs-root-receiver"])?
    assert "lint.fs-root-receiver" not in reported.stderr, reported.stderr
  }
}

test test_stage_callable_wrapper_fix_rechecks_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "stage-callable")?
  let file = fp"{root}/wrapper.xsh"
  let source = "pure increment(value: Int) -> Int { value + 1 }\nlet values = [1, 2] |> map { |item| increment(item) }\nprint values.len()\n"
  file.write(source)
  assert_checks(file)
  let reported = lint(file, ["--only", "lint.stage-callable"])?
  assert "warn[lint.stage-callable]" in reported.stderr, reported.stderr

  let fixed = fixed_by(file, "lint.stage-callable", source)?
  assert fixed == source.replace("|> map { |item| increment(item) }", with: "|> map(increment)")
  assert_checks(file)
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
  let again = lint(file, [])?
  assert "lint.stage-callable" not in again.stderr, again.stderr
}

test test_stage_callable_wrapper_fix_requires_exact_item_stable_name_and_no_propagation { |ctx|
  let root = test.temp_dir(ctx, name: "stage-callable-refused")?
  let file = fp"{root}/wrapper.xsh"
  for source in [
    "pure f(value: Int, amount: Int = 1) -> Int { value + amount }\nlet _ = [1] |> map { |item| f(item, 2) }\n",
    "pure f(value: Int) -> Int { value }\nlet _ = [1] |> map { |item| f(item + 1) }\n",
    "pure f(value: Int) -> Int { value }\nlet _ = [1] |> map { |item| # preserve explanation\n f(item) }\n",
    "pure f(value: Int) -> Int { value }\nlet _ = [1] |> map { |item| let copy = item; f(copy) }\n",
    "pure f(value: Int) -> Result[Int] { Ok(value) }\nproc main() [error] { let _ = [1] |> map { |item| f(item)? } }\n",
    "pure f(value: Int) -> Int { value }\nproc apply(f: pure(value: Int) -> Int) [] { let _ = [1] |> map { |item| f(item) } }\n",
  ] {
    file.write(source)
    let reported = lint(file, ["--only", "lint.stage-callable"])?
    assert "err[" not in reported.stderr, f"{source}{reported.stderr}"
    assert "lint.stage-callable" not in reported.stderr, f"{source}{reported.stderr}"
    assert fixed_by(file, "lint.stage-callable", source)? == source, source
  }

  # An erased callable local that shadows the function cannot be called at
  # all, so the wrapper is a check error and is left as written.
  let erased = "pure f(value: Int) -> Int { value }\nproc apply(f: Pure) [] { let _ = [1] |> map { |item| f(item) } }\n"
  assert fixed_by(file, "lint.stage-callable", erased)? == erased
  let rejected = lint(file, ["--only", "lint.stage-callable"])?
  assert "err[check.call-target]" in rejected.stderr, rejected.stderr
  assert "lint.stage-callable" not in rejected.stderr, rejected.stderr
}
