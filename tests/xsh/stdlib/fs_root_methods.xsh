test fs_root_methods_keep_child_independent_after_parent_close { |ctx|
  let root = fs.open_root(test.temp_dir(ctx, name: "root-methods")?)?
  root.mkdir(p"child")
  let child = root.open_root(p"child")?
    let erased: Any = root
  let validated = erased.require(FsRoot)?
  validated.close()
  child.write(p"data", "payload")
  assert child.read_text(p"data")? == "payload"
  assert child.read_bytes(p"data")? == b"payload"
  assert root.exists(p"child") is Err(_)
}

test fs_tempdir_in_creates_a_private_child_and_removes_it_on_close { |ctx|
  let parent = test.temp_dir(ctx, name: "fs-tempdir-in-parent")?
  let root = fs.tempdir_in(parent)?
  let root_path = root.host_path()?

  assert fs.stat(root_path)?.mode.bit_and(0o777) == 0o700
  root.write(p"chunk", "data")
  root.chmod(p"chunk", 0o600)
  assert fs.stat(fp"{root_path}/chunk")?.mode.bit_and(0o777) == 0o600
  assert (fs.children(parent)? |> count()) == 1

  root.close()
  assert (fs.children(parent)? |> count()) == 0
}

test fs_root_methods_preserve_bounded_observations_and_raw_names {
  let root = fs.tempdir()?
  let raw = (b"raw-name" as Path).require(RelPath)?
  root.write(raw, b"abc\0def")
  root.write_atomic(p"text", "atomic")
  root.write_atomic(p"binary", b"bytes")
  assert root.read_bytes(raw)? == b"abc\0def"
  let native_name = (b"raw-\xff" as Path).require(RelPath)?
  let native_observation = root.read_result(native_name)?
  assert native_observation.state in ["absent", "read_failure"]
  assert root.read_text(p"text")? == "atomic"
  let observed = root.read_result(raw, max_bytes: 3)?
  assert observed.state == "observed"
  assert observed.data == b"abc"
  assert observed.truncated
  let children = root.children(p".", max_entries: 1)?
  assert children.state == "truncated"
  assert children.children.len() == 1
  let all: List[RelPath] = root.children(p".")?.children
  assert raw in all
  assert root.filesystem_stats(p".")?.state == "observed"
  test.error_kind(root.read_result(raw, max_bytes: -1), "fs-root-read-result")
  test.error_kind(root.children(p".", max_entries: -1), "fs-root-children")
  assert root.read_result(p"missing")?.state == "absent"
}

test fs_root_methods_keep_mutation_defaults_and_symlink_confinement { |ctx|
  let root = fs.tempdir()?
  root.mkdir(p"nested/child", parents: true)
  root.mkdir(p"restricted", mode: 0o700)
  assert root.metadata(p"restricted")?.mode % 512 == 0o700
  root.write(p"nested/data", "data")
  root.chmod(p"nested/data", 0o600)
  assert root.metadata(p"nested/data")?.mode % 512 == 0o600
  root.symlink(target: p"data", path: p"nested/link")
  assert root.readlink(p"nested/link")? == p"data"
  assert root.readlink_result(p"nested/link")?.state == "observed"
  assert root.read_text(p"nested/link")? == "data"
  assert root.readlink_result(p"missing")?.state == "absent"
  let outside = test.temp_path(ctx, name: "root-outside")
  root.symlink(target: outside, path: p"escape")
  test.error_kind(root.read_text(p"escape"), "fs-root-read")
  root.remove(p"nested/link")
  assert ! root.exists(p"nested/link")?
  root.remove(p"nested/child", dir: true)
  assert root.host_path() is Ok(_)
}

test fs_root_methods_named_arguments_evaluate_in_source_order { |ctx|
  let output = test.expect(
    ctx,
    r"""
let root = fs.tempdir()?
defer root.close()?
proc next_path() [] -> RelPath {
  print "path"
  p"data"
}
proc data() [] -> Str {
  print "data"
  "payload"
}
root.write(data: data(), path: next_path())?
print (root.read_text(p"data")?)
""",
    status: 0,
  )?
  assert output.stdout == """data
path
payload
"""
}

test fs_root_methods_reject_structural_forgery_aliases_and_missing_effect { |ctx|
  for source in [
    """let forged: FsRoot = {id: 1}
""",
    """let forged: Any = {id: 1}
let _ = forged.require(FsRoot)?
""",
    """let root = fs.tempdir()?
fs.close_root(root)?
""",
    """proc read(root: FsRoot) [] -> Result[Bytes] { root.read_bytes(p"data") }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status != 0
  }
}

test fs_root_methods_reject_paths_independently { |ctx|
  let path_parameter_cases = [
    r"""proc read(root: FsRoot, file: Path) [fs, error] -> Result[Bytes] {
  root.read_bytes(file)
}
""",
    r"""proc install(source_root: FsRoot, source: Path, dest_root: FsRoot, destination: RelPath) [fs, error] {
  fs.root_install_file(source_root, source, dest_root, destination, 0o600)
}
""",
    r"""proc install(source_root: FsRoot, source: RelPath, dest_root: FsRoot, destination: Path) [fs, error] {
  fs.root_install_file(source_root, source, dest_root, destination, 0o600)
}
""",
  ]
  for index, source in path_parameter_cases {
    let rejected = test.expect(ctx, source, status: 2, stderr: ["[check.type-mismatch]"])?
    assert rejected.stderr.split("[check.type-mismatch]").len() - 1 == 1, f"path parameter case {index}"
  }

  let invalid_literal_cases = [
    r"""proc read(root: FsRoot) [fs, error] -> Result[Str] {
  root.read_text(/tmp)
}
""",
    r"""proc read(root: FsRoot) [fs, error] -> Result[Str] {
  root.read_text(../secret.txt)
}
""",
    r"""proc exists(root: FsRoot) [fs, error] -> Result[Bool] {
  root.exists(../escape)
}
""",
    r"""proc readlink(root: FsRoot) [fs, error] {
  root.readlink_result(../escape)
}
""",
    r"""proc readlink(root: FsRoot) [fs, error] {
  root.readlink_result(p"nested/../../escape")
}
""",
    r"""proc stat(root: FsRoot) [fs, error] {
  root.stat(../outside)
}
""",
    r"""proc symlink(root: FsRoot, target: Path) [fs, error] {
  root.symlink(target: target, path: ../escape)
}
""",
  ]
  for index, source in invalid_literal_cases {
    let rejected = test.expect(ctx, source, status: 2, stderr: ["[check.validated-literal]"])?
    assert rejected.stderr.split("[check.validated-literal]").len() - 1 == 1, f"literal case {index}"
  }

  let symlink_target = test.expect(
    ctx,
    r"""proc symlink(root: FsRoot, target: Path, link: RelPath) [fs, error] {
  root.symlink(target: target, path: link)
}
""",
    status: 0,
  )?
  assert symlink_target.stdout == ""
}

test fs_root_methods_accept_validated_paths {
  let source_root = fs.tempdir()?
  let dest_root = fs.tempdir()?
  let source: RelPath = "source"
  let dest: RelPath = "dest"
  source_root.write(source, "payload")
  fs.root_install_file(source_root, source, dest_root, dest, 0o600)
  assert dest_root.read_text(dest)? == "payload"
}

test fs_root_removed_spellings_remain_usable_as_user_module_functions { |ctx|
  let root = test.temp_dir(ctx, name: "root-user-module")?
  fp"{root}/helper.xsh".write_atomic("""
##! Caller-owned functions.
## Returns its argument unchanged.
export pure root_path(value: Str) -> Str { value }
## Returns its argument unchanged.
export pure close_root(value: Str) -> Str { value }
""")
  let output = test.expect(
    ctx,
    """
use helper
print helper.root_path("path")
print helper.close_root("closed")
""",
    status: 0,
    args: [],
    env: {XSH_MODULE_PATH: root},
  )?
  assert output.stdout == """path
closed
"""
}

test fs_root_methods_optional_receiver_keeps_arguments_lazy {
  let absent: FsRoot? = null
  let missing = absent?.read_bytes(optional_root_path())
  assert missing == null
  let present: FsRoot? = fs.tempdir()?
  defer present.require(FsRoot)?.close()
  assert present?.children(p".") != null
}

proc optional_root_path() [error] -> RelPath {
  error.fail("must stay lazy")
  p"data"
}

test fs_root_methods_trace_retains_native_host_operation_identity { |ctx|
  let output = test.run_xsht_trace(
    ctx,
    """
let root = fs.tempdir()?
defer root.close()?
root.write(p"data", "trace")?
print (root.read_text(p"data")?)
""",
    ["--raw", "--trace-format", "jsonl"],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """trace
"""
  assert "method.FsRoot.write" in output.stderr
  assert "FsRoot.read_text" in output.stderr
}
