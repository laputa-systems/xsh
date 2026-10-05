test test_path_kind_predicates_ask_what_the_path_itself_is { |ctx|
  let root = test.temp_dir(ctx, name: "path-kind")?
  let dir = fp"{root}/dir"
  let file = fp"{root}/file"
  dir.mkdir()
  file.write("text")

  assert dir.is_dir()?
  assert ! dir.is_file()?
  assert ! dir.is_symlink()?

  assert file.is_file()?
  assert ! file.is_dir()?
  assert ! file.is_symlink()?

  # A link is a link whatever it names: the last component is not followed.
  let to_dir = fp"{root}/to-dir"
  let to_file = fp"{root}/to-file"
  let dangling = fp"{root}/dangling"
  to_dir.symlink(to: dir)
  to_file.symlink(to: file)
  dangling.symlink(to: fp"{root}/absent")
  for link in [to_dir, to_file, dangling] {
    assert link.is_symlink()?
    assert ! link.is_dir()?
    assert ! link.is_file()?
  }

  # A link earlier in the path is followed, as for any lookup.
  assert fp"{to_dir}/..".is_dir()?

  # Any other kind of entry is none of the three.
  let device = /dev/null
  assert device.metadata()?.kind == "other"
  assert ! device.is_file()?
  assert ! device.is_dir()?
  assert ! device.is_symlink()?
}

test test_path_kind_predicates_agree_with_metadata_kind { |ctx|
  let root = test.temp_dir(ctx, name: "path-kind-metadata")?
  let dir = fp"{root}/dir"
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  dir.mkdir()
  file.write("text")
  link.symlink(to: file)
  for entry in [dir, file, link, /dev/null] {
    let kind = entry.metadata()?.kind
    assert entry.is_dir()? == (kind == "dir")
    assert entry.is_file()? == (kind == "file")
    assert entry.is_symlink()? == (kind == "symlink")
  }

  # A path that does not exist is the failure `metadata` reports, not false.
  let absent = fp"{root}/absent"
  test.error_kind(absent.metadata(), "fs-metadata")
  test.error_kind(absent.is_dir(), "fs-metadata")
  test.error_kind(absent.is_file(), "fs-metadata")
  test.error_kind(absent.is_symlink(), "fs-metadata")
  test.error_kind(fp"{file}/below".is_file(), "fs-metadata")
}

test test_path_kind_predicates_on_an_optional_path { |ctx|
  let root = test.temp_dir(ctx, name: "path-kind-optional")?
  let present: Path? = root
  let missing: Path? = null
  assert (present?.is_dir() ?? Ok(false))?
  assert ! (missing?.is_dir() ?? Ok(false))?
}

test test_prefer_path_kind_lint_rewrites_a_kind_comparison { |ctx|
  let root = test.temp_dir(ctx, name: "path-kind-lint")?
  fp"{root}/link".symlink(to: root)
  fp"{root}/file".write("text")
  let source = r"""proc classify(out: Path) [fs, error] -> Result[Str] {
  if out.metadata()?.kind == "symlink" {
    return Ok("link")
  }
  if fs.metadata(out)?.kind != "dir" {
    return Ok("not a directory")
  }
  Ok("directory")
}

print (classify(p"ROOT")?) (classify(p"ROOT/link")?) (classify(p"ROOT/file")?)
print (classify(p"ROOT/absent")?)
""".replace("ROOT", with: root.display())
  let candidate = test.temp_file(ctx, name: "path-kind.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.prefer-path-kind --fix $candidate ?
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  let expected = source.replace("out.metadata()?.kind == \"symlink\"", with: "out.is_symlink()?")
    .replace("fs.metadata(out)?.kind != \"dir\"", with: "! out.is_dir()?")
  assert fixed == expected
  assert fixed != source

  # The same answers, and the same failure for a path that does not exist.
  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  assert before.stdout == "directory link not a directory\n"
  assert after.stdout == before.stdout
  assert ! before.success
  assert after.status == before.status
  assert "fs-metadata" in before.stderr, before.stderr
  assert "fs-metadata" in after.stderr, after.stderr

  let repeated = run.capture --text "xsht" lint --only lint.prefer-path-kind $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.prefer-path-kind" not in repeated.stderr
}
