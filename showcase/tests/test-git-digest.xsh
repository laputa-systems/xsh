pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_git_digest_usage() [process, error] {
  let output = run.text xsh_bin() "showcase/git-digest.xsh" -- --help ?
  test.contains(output, "usage:")?
}
