# A project whose config excludes its top-level `stdlib/` directory, with a
# directory of the same name deeper in the tree.
proc project(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "discovery")?
  fp"{root}/xsht-config.ini".write_atomic("exclude = stdlib/**/*.xsh\n  generated/*.xsh\n")
  fs.mkdir(fp"{root}/stdlib")
  fs.mkdir(fp"{root}/generated")
  fs.mkdir(fp"{root}/tests/stdlib")
  fs.mkdir(fp"{root}/tests/generated")
  for file in [
    "main.xsh",
    "stdlib/excluded.xsh",
    "generated/excluded.xsh",
    "tests/kept.xsh",
    "tests/stdlib/kept.xsh",
    "tests/generated/kept.xsh",
  ] {
    # Unformatted, so `fmt --check` names every file it discovers.
    fp"{root}/{file}".write_atomic("const  value = 1\n")
  }

  Ok(root)
}

# The number of files a tool reports in its closing timing line.
pure checked_files(stderr: Str) -> Str {
  let found = rx"xsht [a-z]+: ([0-9]+) files? in".captures(stderr)
  if found.len() == 2 { found[1] } else { stderr }
}

test test_excludes_are_relative_to_the_config_for_a_directory_argument { |ctx|
  let root = project(ctx)?
  # The config's patterns name paths below the config, whichever directory
  # the tool is pointed at.
  cd $root {
    let everything = run.capture --text "xsht" check ?
    assert checked_files(everything.stderr) == "4", everything.stderr
    let tests = run.capture --text "xsht" check tests ?
    assert checked_files(tests.stderr) == "3", tests.stderr
    let nested = run.capture --text "xsht" check tests/stdlib ?
    assert checked_files(nested.stderr) == "1", nested.stderr
    let excluded = run.capture --text "xsht" check stdlib generated ?
    # Nothing is left to check, so there is no report at all.
    assert excluded.status.exited_with(0) and excluded.stderr == "", excluded.stderr
    let linted = run.capture --text "xsht" lint tests ?
    assert checked_files(linted.stderr) == "3", linted.stderr
    let unformatted = run.capture --text "xsht" fmt --check tests ?
    assert unformatted.stdout.split("needs formatting").len() == 4, unformatted.stdout
    assert "tests/stdlib/kept.xsh: needs formatting" in unformatted.stdout, unformatted.stdout
    let all_unformatted = run.capture --text "xsht" fmt --check ?
    assert all_unformatted.stdout.split("needs formatting").len() == 5, all_unformatted.stdout
  }

  let absolute = run.capture --text "xsht" check fp"{root}/tests" ?
  assert checked_files(absolute.stderr) == "3", absolute.stderr
}
