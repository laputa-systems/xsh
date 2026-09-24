proc test_git_digest_usage() [process, error] {
  let output = run.text "xsh" "showcase/git-digest.xsh" -- --help ?
  test.contains(output, "usage:")?
}

proc test_git_digest_quotes_non_utf8_paths_even_when_git_config_disables_quoting(ctx: TestContext) [fs, process, env, error] {
  if system.uname()?.sysname != "Linux" {
    test.skip("creating non-UTF-8 path components requires the pinned Linux filesystem")
    return
  }

  let repo = test.temp_dir(ctx, name: "git-digest-byte-path")?
  run git -C $repo init --quiet ?
  run git -C $repo config user.name Tester ?
  run git -C $repo config user.email "tester@example.invalid" ?
  run git -C $repo config core.quotePath false ?
  fp"${repo}/base.txt".write("base\n")?
  run git -C $repo add -A ?
  run git -C $repo commit --quiet -m base ?
  run git -C $repo branch base ?

  let raw_file = Path.parse_bytes(bytes.concat([bytes.from_text(repo.display()), b"/raw-\xff.txt"]))?
  raw_file.write("new\n")?
  run git -C $repo add -A ?
  run git -C $repo commit --quiet -m add ?

  let script = fp"${fs.cwd()?}/showcase/git-digest.xsh"
  let output_file = fp"${repo}/digest.out"
  let error_file = fp"${repo}/digest.err"
  let command = process.command_argv(
    "xsh",
    ["xsh", script, "--", "--base", "base"],
    cwd: repo,
    stdout: output_file,
    stderr: error_file,
  )
  let status = process.run(command)?
  test.ok(status.exited_with(0), error_file.read_text()?)?
  let output = output_file.read_text()?
  test.contains(output, "1 file(s) changed")?
  test.contains(output, "raw-\\377.txt")?
}
