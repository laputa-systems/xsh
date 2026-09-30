test test_dot_env_run [fs, process, error] { |ctx|
  let envfile = test.temp_file(ctx, name: ".env", contents: b"FOO=bar\nQUOTED=\"hello\"\n")?
  let output = run.text "xsh" "showcase/dot-env-run.xsh" -- $envfile true ?
  "loaded 2 var(s)" in output
}
