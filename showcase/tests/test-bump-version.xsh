proc test_bump_version_usage() [process, error] {
  let output = run.text "xsh" "showcase/bump-version.xsh" -- --help ?
  test.contains(output, "usage:")?
  test.contains(output, "major | minor | patch")?
}
