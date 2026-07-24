proc test_git_digest_usage() [process, error] {
  let output = run.text "xsh" "showcase/git-digest.xsh" -- --help ?
  test.contains(output, "usage:")?
}
