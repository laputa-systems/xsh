proc test_dot_env_run(ctx: TestContext) [fs, process, error] {
  let envfile = test.temp_file(ctx, name: ".env", contents: b"FOO=bar\nQUOTED=\"hello\"\n")?
  let output = run.text "xsh" "showcase/dot-env-run.xsh" -- $envfile true ?
  test.contains(output, "loaded 2 var(s)")?
}
