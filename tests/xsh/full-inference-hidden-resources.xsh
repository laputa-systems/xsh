test test_full_inference_scalar_extra_fields_survive_record_require { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type VisibleFile = {path: Path}
type CompleteFile = {path: Path, hidden: Int}
let raw: Any = {path: p".", hidden: 17}
let visible: VisibleFile = raw.require(VisibleFile)?
let erased: Any = visible
let complete: CompleteFile = erased.require(CompleteFile)?
assert complete.path == p"."
assert complete.hidden == 17
print $complete.hidden
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "17\n"
}

test test_full_inference_hidden_resource_survives_open_record_require_until_owner_exit { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type VisibleDirectory = {path: Path}
proc make_visible_directory() [fs, error] -> Result[VisibleDirectory] {
  let root = fs.tempdir()?
  let directory_path = root.host_path()?
  let raw: Any = {path: directory_path, hidden: root}
  let visible: VisibleDirectory = raw.require(VisibleDirectory)?
  Ok(visible)
}
proc inspect_visible_directory() [fs, error] -> Result[Path] {
  let visible = make_visible_directory()?
  assert visible.path.exists()?
  Ok(visible.path)
}
let directory_path = inspect_visible_directory()?
assert ! directory_path.exists()?
print "record resource lifetime preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "record resource lifetime preserved\n"
}

test test_full_inference_nominal_tag_payload_resource_survives_until_owner_exit { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""enum HiddenDirectory { Owns(FsRoot, Path) }
proc make_hidden_directory() [fs, error] -> Result[HiddenDirectory] {
  let root = fs.tempdir()?
  let directory_path = root.host_path()?
  Ok(Owns(root, directory_path))
}
proc inspect_hidden_directory() [fs, error] -> Result[Path] {
  let hidden = make_hidden_directory()?
  let directory_path = match hidden {
    Owns(root, directory_path) => {
      assert root.exists(p".")?
      assert directory_path.exists()?
      directory_path
    }
  }
  Ok(directory_path)
}
let directory_path = inspect_hidden_directory()?
assert ! directory_path.exists()?
print "tag resource lifetime preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "tag resource lifetime preserved\n"
}
