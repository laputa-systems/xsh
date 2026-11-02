test module_command_syntax_is_rejected_with_the_expression_call { |ctx|
  for case in [
    {
      source: "let durable = p\"x.json\"\nfs.fsync $durable\n",
      rewrite: "fs.fsync(durable)?",
    },
    {
      source: "let durable = p\"x.json\"\nfs.fsync $durable ?\n",
      rewrite: "fs.fsync(durable)?",
    },
    {
      source: "let durable = p\"x.json\"\njson.write $durable --pretty ({value: 1})\n",
      rewrite: "json.write(durable, {value: 1}, pretty: true)?",
    },
    {
      source: "json.write out.json ({value: 1}) false\n",
      rewrite: "json.write(p\"out.json\", {value: 1}, false)?",
    },
    {
      source: "let pause = 0ms\ntime.sleep $pause\n",
      rewrite: "time.sleep(pause)?",
    },
    {
      source: "time.sleep 0ms\n",
      rewrite: "time.sleep(0ms)?",
    },
  ] {
    let result = test.run_script(ctx, case.source)?
    assert !result.success, case.source
    assert "err[check.module-command-syntax]" in result.stderr, result.stderr
    assert case.rewrite in result.stderr, result.stderr
  }
}

test module_command_with_words_that_have_no_exact_rewrite_names_the_parameters { |ctx|
  let result = test.run_script(ctx, r"time.sleep ${0}ms" + "\n")?
  assert !result.success
  assert "err[check.module-command-syntax]" in result.stderr, result.stderr
  assert "time.sleep(" in result.stderr, result.stderr
}

test module_command_syntax_fails_the_check_before_any_statement_runs { |ctx|
  let result = test.run_script(ctx, "print unreachable\nfs.fsync (false)\n")?
  assert !result.success
  assert result.stdout == "", result.stdout
  assert "err[check.module-command-syntax]" in result.stderr, result.stderr
}

test expression_call_of_a_command_style_api_runs_the_checked_operation { |ctx|
  let result = test.expect(
    ctx,
    r"""let scratch = fs.tempdir()?
defer scratch.close()
let durable = fp"{scratch.host_path()?}/payload.json"
json.write(durable, {value: 1}, pretty: true)?
assert "\n" in durable.read_text()?
fs.fsync(durable)?
time.sleep(0ms)?
print completed
""",
    status: 0,
  )?
  assert result.stdout == "completed\n", result.stdout
}
