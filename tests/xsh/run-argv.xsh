test test_spliced_target_runs_the_vector {
  let argv = ["printf", "%s|%s|%s", "one"]
  let trailing = run.text @argv two three
  assert trailing == "one|two|three"
  let inline = run.text @(["printf", "%s", "inline"])
  assert inline == "inline"
  let more = ["four", "five"]
  let spliced = run.text @argv @more
  assert spliced == "one|four|five"
  let paths = [/bin/sh, p"-c", p"printf path"]
  let from_paths = run.text @paths
  assert from_paths == "path"
}

test test_spliced_target_in_every_run_form {
  let quiet = ["sh", "-c", "exit 3"]
  let status = run.status @quiet
  assert status.exited_with(3)
  let captured = run.capture --text @(["sh", "-c", "printf out; printf err >&2"])
  assert captured.stdout == "out" and captured.stderr == "err"
  let lines = run.stream --text @(["printf", "a\nb\n"]) |> collect()
  assert lines == ["a", "b"]
  let upper = run.text @(["printf", "piped"]) | run @(["tr", "a-z", "A-Z"])
  assert upper == "PIPED"
  let job = spawn run @(["true"]) ?
  let waited = wait job?
  assert waited.success
  let plan = process.command {
    env = {GREETING: "planned"}
    run @(["sh", "-c", "exit 0"])
  }
  assert process.run(plan)?.success
}

test test_spliced_target_matches_a_written_command {
  # The first element is the program; the rest are its arguments, in order.
  let argv = ["sh", "-c", "printf %s \"$0\"", "named"]
  let named = run.text @argv
  assert named == "named"
  let spliced = run.text @(["sh", "-c", "printf %s \"$0\""])
  let written = run.text sh -c "printf %s \"$0\""
  assert spliced == written
}

test test_empty_spliced_command_is_an_invalid_target {
  let argv: List[Str] = []
  let outcome = try {
    run @argv
  }
  match outcome {
    Ok(_) => assert false, "an empty command ran"
    Err(error) => {
      assert error is ProcessError.InvalidTarget
      assert error.message == "spliced command is empty: its first element names the program to run"
    }
  }

  let in_value_position = try {
    run.status @argv
  }
  assert in_value_position is Err(_)
}

test test_empty_list_literal_target_is_a_check_error { |ctx|
  let output = test.run_script(ctx, "run @([])\n")?
  assert ! output.success
  assert output.stderr.split("err[check.run-target]").len() == 2, output.stderr
  assert "err[check.argv-conversion]" not in output.stderr, output.stderr
}

test test_interpolated_target_is_still_one_item { |ctx|
  let output = test.run_script(ctx, "let argv = [\"printf\", \"x\"]\nrun $argv\n")?
  assert ! output.success
  assert "run target must produce one argv item" in output.stderr, output.stderr
}

test test_lint_rewrites_a_rebuilt_command_vector { |ctx|
  let source = "proc launch(argv: List[Str]) [process, error] -> Result[Status, ProcessError] {\n  let status = process.run(process.command_argv(argv[0], argv))?\n  Ok(status)\n}\n\nlet status = launch([\"sh\", \"-c\", \"exit 4\"])?\nprint f\"{status.exited_with(4)}\"\n"
  let file = test.temp_file(ctx, name: "launch.xsh", contents: bytes.from_text(source))?
  let before = test.expect(ctx, source, status: 0)?
  let reported = run.capture --text "xsht" lint --only lint.prefer-run-argv $file
  assert reported.stderr.split("warn[lint.prefer-run-argv]").len() == 2, reported.stderr
  let fixed = run.capture --text "xsht" lint --fix --only lint.prefer-run-argv $file
  assert "  let status = run.status @argv ?\n" in file.read_text()?, fixed.stderr
  let after = test.expect(ctx, file.read_text()?, status: 0)?
  assert after.stdout == before.stdout
}

pure make(spec, target, argv) {
  process.command_argv(target, argv, cwd: spec.cwd, env: spec.environment)
}

test test_inferred_command_argv_keeps_text_and_path_word_domains {
  let spec = {cwd: p".", environment: {}}
  let text: Any = make(spec, "not-launched", ["not-launched", "--flag"])
  let path_plan: Any = make(spec, p"/not-launched", [p"/not-launched", p"--flag"])
  assert text is Command
  assert path_plan is Command
}

test test_inferred_command_argv_rejects_nonword_domains { |ctx|
  for invocation in ["make(1, [\"word\"])", "make(true, [\"word\"])", "make(\"word\", [1])", "make(\"word\", [true])"] {
    let source = "pure make(target, argv) { process.command_argv(target, argv) }\nlet plan = " + invocation + "\n"
    let output = test.run_script(ctx, source)?
    assert ! output.success
    assert "err[check." in output.stderr, output.stderr
  }
}
