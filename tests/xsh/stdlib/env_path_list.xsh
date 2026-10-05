test test_env_overlay_joins_a_path_list_and_reads_it_back {
  let raw = Path.parse_bytes(b"/opt/bad\xffname/bin")?
  let dirs = [/opt/stage/usr/bin, raw, /bin]
  env ({XSH_PATH_LIST: dirs}) {
    assert env.PathList.XSH_PATH_LIST? == dirs
    let child = run.bytes printenv XSH_PATH_LIST ?
    assert child == b"/opt/stage/usr/bin:/opt/bad\xffname/bin:/bin\n"
  }

  # An empty entry is kept, and an empty list is the empty value.
  let none: List[Path] = []
  env ({XSH_PATH_LIST: [/a, p"", /b], XSH_PATH_LIST_EMPTY: none}) {
    assert e"XSH_PATH_LIST"? == "/a::/b"
    assert e"XSH_PATH_LIST_EMPTY"? == ""
  }

  # A spliced list extends the variable that is already set, and an unset one
  # adds no entry.
  env ({XSH_PATH_LIST: "/usr/bin:/bin"}) {
    env ({
      XSH_PATH_LIST: [/opt/bin, @env.PathList.XSH_PATH_LIST ?? []],
      XSH_PATH_LIST_NEW: [/opt/bin, @env.PathList.XSH_PATH_LIST_UNSET ?? []],
    }) {
      assert e"XSH_PATH_LIST"? == "/opt/bin:/usr/bin:/bin"
      assert e"XSH_PATH_LIST_NEW"? == "/opt/bin"
    }
  }
}

test test_every_environment_value_position_joins_a_path_list { |ctx|
  let root = test.temp_dir(ctx, name: "env-path-list")?
  let sh = process.which("sh")?
  let dirs = [/one, /two]
  let script = "printf '%s' \"$XSH_PATH_LIST\""

  env XSH_PATH_LIST=$dirs XSH_PATH_LIST_EXPR=$dirs {
    assert e"XSH_PATH_LIST"? == "/one:/two"
    assert e"XSH_PATH_LIST_EXPR"? == "/one:/two"
  }

  let by_word = run.text XSH_PATH_LIST=$dirs $sh -c $script ?
  assert by_word == "/one:/two"

  env XSH_PATH_LIST_SCOPE=1 {
    e"XSH_PATH_LIST" = dirs
    assert e"XSH_PATH_LIST"? == "/one:/two"
  }

  let by_argv = fp"{root}/argv.out"
  let argv_plan = process.command_argv(sh, [sh, "-c", script], env: {XSH_PATH_LIST: dirs}, stdout: by_argv)
  assert process.run(argv_plan)?.exited_with(0)
  assert by_argv.read_text()? == "/one:/two"

  let by_builder = fp"{root}/builder.out"
  let builder_plan = process.command {
    env = {XSH_PATH_LIST: dirs}
    stdout = by_builder
    run $sh -c $script
  }
  assert process.run(builder_plan)?.exited_with(0)
  assert by_builder.read_text()? == "/one:/two"

  let nested = test.run_xsh(ctx, "print (e\"XSH_PATH_LIST\"?)\n", env: {XSH_PATH_LIST: dirs})?
  assert nested.success, nested.stderr
  assert nested.stdout == "/one:/two\n"
}

test test_a_path_list_entry_with_the_separator_fails { |ctx|
  let split = [/one, /two:/three]
  let entered = env ({XSH_PATH_LIST: split}) { 1 }
  test.error_kind(entered, "env-value")
  env XSH_PATH_LIST_SCOPE=1 {
    assert (e"XSH_PATH_LIST" ?? "unset") == "unset"
  }

  for source in [
    r"""env XSH_PATH_LIST=$split {
  print "entered"
}
""",
    "e\"XSH_PATH_LIST\" = split\nprint \"entered\"\n",
    r"""run XSH_PATH_LIST=$split true
print "entered"
""",
    "let plan = process.command_argv(\"true\", [\"true\"], env: {XSH_PATH_LIST: split})\nprint \"entered\"\n",
  ] {
    let failed = test.run_script(ctx, "let split = [p\"/one\", p\"/two:/three\"]\n" + source)?
    assert failed.status == 3, failed.stderr
    assert "env-value" in failed.stderr, failed.stderr
    assert "entry 1 contains the separator" in failed.stderr, failed.stderr
    assert "entered" not in failed.stdout, source
  }
}

test test_a_list_of_anything_else_is_not_an_environment_value { |ctx|
  for case in [
    {source: "env ({NAMES: [\"a\", \"b\"]}) {\n  print \"entered\"\n}\n", code: "check.env-value"},
    {source: "env NAMES=([\"a\", \"b\"]) {\n  print \"entered\"\n}\n", code: "check.argv-conversion"},
    {source: "e\"NAMES\" = [1, 2]\n", code: "check.env-value"},
  ] {
    let rejected = test.run_script(ctx, case.source)?
    assert rejected.status == 2, case.source
    assert case.code in rejected.stderr, rejected.stderr
  }

  # A command plan's `env` record is checked when the plan is built.
  let plan = test.run_script(
    ctx,
    "let plan = process.command_argv(\"true\", [\"true\"], env: {NAMES: [\"a\", \"b\"]})\nprint \"built\"\n",
  )?
  assert plan.status == 3, plan.stderr
  assert "must be a List[Path]" in plan.stderr, plan.stderr
  assert "built" not in plan.stdout
}

test test_a_list_word_that_is_not_a_path_list_keeps_the_one_item_rule { |ctx|
  let one = ["only"]
  env XSH_PATH_LIST=$one {
    assert e"XSH_PATH_LIST"? == "only"
  }
  for names in ["[\"a\", \"b\"]", "[]"] {
    let failed = test.run_script(
      ctx,
      f"let names: List[Str] = {names}\nenv NAMES=\$names {{\n  print \"entered\"\n}}\n",
    )?
    assert failed.status == 3, failed.stderr
    assert "environment values must be one value" in failed.stderr, failed.stderr
  }
}

test test_prefer_env_path_list_lint_offers_the_list_without_applying_it { |ctx|
  let program = r"""proc show(root: Path) [process, env, error] {
  env ({XSH_PATH_LIST: VALUE}) {
    print (e"XSH_PATH_LIST"?)
  }
}

show(p"/stage")?
"""
  let source = program.replace("VALUE", "f\"{root}/usr/bin:/opt/bin:{e\"XSH_PATH_LIST\" ?? \"\"}\"")
  let candidate = test.temp_file(ctx, name: "env-path-list.xsh", contents: bytes.from_text(source))?
  let reported = run.capture --text "xsht" lint --only lint.prefer-env-path-list --fix $candidate ?
  assert "lint.prefer-env-path-list" in reported.stderr, reported.stderr
  let rewrite = r"""[fp"{root}/usr/bin", p"/opt/bin", @env.PathList.XSH_PATH_LIST ?? []]"""
  assert rewrite in reported.stderr, reported.stderr
  assert "no trailing empty entry" in reported.stderr, reported.stderr

  # The list is another value when the variable is unset, so `--fix` leaves
  # the file alone.
  assert candidate.read_text()? == source

  let listed = program.replace("VALUE", rewrite)
  assert listed != source
  let inherited = {XSH_PATH_LIST: "/usr/bin:/bin"}
  let before_set = test.run_script(ctx, source, [], inherited)?
  let after_set = test.run_script(ctx, listed, [], inherited)?
  assert after_set.success, after_set.stderr
  assert before_set.stdout == "/stage/usr/bin:/opt/bin:/usr/bin:/bin\n"
  assert after_set.stdout == before_set.stdout

  let before_unset = test.run_script(ctx, source)?
  let after_unset = test.run_script(ctx, listed)?
  assert before_unset.stdout == "/stage/usr/bin:/opt/bin:\n"
  assert after_unset.stdout == "/stage/usr/bin:/opt/bin\n"
}
