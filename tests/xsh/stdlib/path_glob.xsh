test test_path_glob_matches_below_the_receiver { |ctx|
  let root = glob_fixture(ctx, "path-glob")?
  assert root.glob("*.txt")? == [fp"{root}/a.txt", fp"{root}/b.txt"]
  assert root.glob(".*.txt")? == [fp"{root}/.dot.txt"]
  assert root.glob("[ab].txt")? == [fp"{root}/a.txt", fp"{root}/b.txt"]
  assert root.glob("[!a].txt")? == [fp"{root}/b.txt"]
  assert root.glob("?.md")? == [fp"{root}/c.md"]
  assert root.glob("a.txt")? == [fp"{root}/a.txt"]
  assert root.glob("src/*")? == [fp"{root}/src/deep", fp"{root}/src/lib.txt"]
  assert root.glob("*/lib.txt")? == [fp"{root}/src/lib.txt"]
  assert root.glob("nope*")? == []

  # The pattern is ordinary text computed at run time.
  let extension = "md"
  assert root.glob(f"*.{extension}")? == [fp"{root}/c.md"]

  # A trailing separator on the receiver does not double up.
  assert fp"{root}/src/".glob("*.txt")? == [fp"{root}/src/lib.txt"]

  # A root that is missing or is a file matches nothing.
  assert fp"{root}/absent".glob("*")? == []
  assert fp"{root}/a.txt".glob("*")? == []
}

test test_path_rglob_matches_at_any_depth_in_byte_order { |ctx|
  let root = glob_fixture(ctx, "path-rglob")?
  let expected = [
    fp"{root}/.hidden/in.txt",
    fp"{root}/a.txt",
    fp"{root}/b.txt",
    fp"{root}/src/deep/leaf.txt",
    fp"{root}/src/lib.txt",
  ]
  assert root.rglob("*.txt")? == expected
  assert root.glob("**/*.txt")? == expected
  assert root.rglob("leaf.txt")? == [fp"{root}/src/deep/leaf.txt"]
  assert root.rglob("deep/*.txt")? == [fp"{root}/src/deep/leaf.txt"]
  assert root.rglob(".*.txt")? == [fp"{root}/.dot.txt"]

  # A recursive match does not enter a symbolic link to a directory; a
  # literal component does.
  fs.symlink(fp"{root}/src", fp"{root}/link")?
  assert root.rglob("lib.txt")? == [fp"{root}/src/lib.txt"]
  assert root.glob("link/*.txt")? == [fp"{root}/link/lib.txt"]
}

test test_path_glob_agrees_with_glob_literals { |ctx|
  let root = glob_fixture(ctx, "path-glob-literal")?
  cd $root {
    assert p"src".glob("*.txt")? == g"src/*.txt"
    assert p"src".glob("**/*.txt")? == g"src/**/*.txt"
    assert p"src".rglob("*.txt")? == g"src/**/*.txt"
    assert p"src".glob("*")? == g"src/*"
    assert p"src".glob("deep/leaf.txt")? == g"src/deep/leaf.txt"
  } ?

  # The receiver is a path, never a pattern.
  let starred = fp"{root}/[ab]"
  starred.mkdir()?
  fp"{starred}/in.txt".write("x")?
  assert starred.glob("*.txt")? == [fp"{starred}/in.txt"]
}

test test_path_glob_rejects_patterns_that_are_not_relative { |ctx|
  let root = glob_fixture(ctx, "path-glob-errors")?
  assert glob_failure(root.glob("")) == "glob pattern is empty"
  assert glob_failure(root.rglob("")) == "glob pattern is empty"
  assert glob_failure(root.glob("/etc/*")) == "glob pattern must be relative to the receiver"
  assert glob_failure(root.rglob("/etc/*")) == "glob pattern must be relative to the receiver"
  assert glob_failure(root.glob("a\0b")) == "glob patterns cannot contain NUL"
}

test test_path_glob_is_a_filesystem_effect { |ctx|
  let checked = test.run_script(
    ctx,
    r"""
pure sources(root: Path) -> Result[List[Path]] {
  root.glob("*.xsh")
}
""",
  )?
  assert ! checked.success
  assert "pure" in checked.stderr

  let typed = test.run_script(
    ctx,
    r"""
let found = p"src".glob(p"*.xsh")?
""",
  )?
  assert ! typed.success
  assert "check.type-mismatch" in typed.stderr
}

proc glob_fixture(ctx: TestContext, name: Str) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name:)?
  fp"{root}/src/deep".mkdir()?
  fp"{root}/.hidden".mkdir()?
  for file in ["b.txt", "a.txt", "c.md", ".dot.txt", "src/lib.txt", "src/deep/leaf.txt", ".hidden/in.txt"] {
    fp"{root}/{file}".write("x")?
  }

  root
}

pure glob_failure(found: Result[List[Path]]) -> Str {
  match found {
    Ok(_) => ""
    Err(error) => error.message
  }
}
