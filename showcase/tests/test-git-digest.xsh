test test_git_digest_usage {
  let output = run.text "xsh" "showcase/git-digest.xsh" -- --help ?
  assert "usage:" in output
}

test test_git_digest_counts_integer_statistics_and_binary_placeholders { |ctx|
  let repo = test.temp_dir(ctx, name: "git-digest-counts")?
  run git -C $repo init --quiet ?
  run git -C $repo config user.name Tester ?
  run git -C $repo config user.email "tester@example.invalid" ?
  fp"{repo}/text.txt".write("""base
""")?
  run git -C $repo add -A ?
  run git -C $repo -c core.hooksPath=/dev/null commit --quiet -m base ?
  run git -C $repo branch base ?
  fp"{repo}/text.txt".write("""base
extra
""")?
  fp"{repo}/binary.dat".write(b"\0binary")?
  run git -C $repo add -A ?
  run git -C $repo -c core.hooksPath=/dev/null commit --quiet -m changed ?
  let script = fp"{fs.cwd()?}/showcase/git-digest.xsh"
  let output_file = fp"{repo}/digest.out"
  let error_file = fp"{repo}/digest.err"
  let command = process.command_argv(
    "xsh",
    ["xsh", script, "--", "--base", "base"],
    cwd: repo,
    stdout: output_file,
    stderr: error_file,
  )
  let status = process.run(command)?
  let succeeded = status.exited_with(0)
  let failure_message = error_file.read_text()?
  assert succeeded, failure_message
  let output = output_file.read_text()?
  assert "2 file(s) changed  +1 -0" in output
  assert "binary.dat" in output
}

test test_git_digest_quotes_non_utf8_paths_even_when_git_config_disables_quoting { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("creating non-UTF-8 path components requires the pinned Linux filesystem")
    return
  }

  let repo = test.temp_dir(ctx, name: "git-digest-byte-path")?
  run git -C $repo init --quiet ?
  run git -C $repo config user.name Tester ?
  run git -C $repo config user.email "tester@example.invalid" ?
  run git -C $repo config core.quotePath false ?
  fp"{repo}/base.txt".write("""base
""")?
  run git -C $repo add -A ?
  run git -C $repo commit --quiet -m base ?
  run git -C $repo branch base ?

  let raw_file = Path.parse_bytes(bytes.concat([repo.bytes(), b"/raw-\xff.txt"]))?
  raw_file.write("""new
""")?
  run git -C $repo add -A ?
  run git -C $repo commit --quiet -m add ?

  let script = fp"{fs.cwd()?}/showcase/git-digest.xsh"
  let output_file = fp"{repo}/digest.out"
  let error_file = fp"{repo}/digest.err"
  let command = process.command_argv(
    "xsh",
    ["xsh", script, "--", "--base", "base"],
    cwd: repo,
    stdout: output_file,
    stderr: error_file,
  )
  let status = process.run(command)?
  let succeeded = status.exited_with(0)
  let failure_message = error_file.read_text()?
  assert succeeded, failure_message
  let output = output_file.read_text()?
  assert "1 file(s) changed" in output
  assert "raw-\\377.txt" in output
}
