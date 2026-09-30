test fs_root_methods_keep_child_independent_after_parent_close [fs, error] { |ctx|
  let root = fs.open_root(test.temp_dir(ctx, name: "root-methods")?)?
  root.mkdir(p"child")?
  let child = root.open_root(p"child")?
  defer child.close()?
  let erased: Any = root
  let validated = erased.require(FsRoot)?
  validated.close()?
  child.write(p"data", "payload")?
  (child.read_text(p"data")?) == ("payload")
  (child.read_bytes(p"data")?) == (b"payload")
  (root.exists(p"child") is Err(_))
  (child.exists(p"../escape") is Err(_))
}

test fs_root_methods_preserve_bounded_observations_and_raw_names [fs, error] {
  let root = fs.tempdir()?
  defer root.close()?
  let raw = Path.parse_bytes(b"raw-name")?
  root.write(raw, b"abc\0def")?
  root.write_atomic(p"text", "atomic")?
  root.write_atomic(p"binary", b"bytes")?
  (root.read_bytes(raw)?) == (b"abc\0def")
  let native_name = Path.parse_bytes(b"raw-\xff")?
  let native_observation = root.read_result(native_name)?
  (native_observation.state in ["absent", "read_failure"])
  (root.read_text(p"text")?) == ("atomic")
  let observed = root.read_result(raw, max_bytes: 3)?
  (observed.state) == ("observed")
  (observed.data) == (b"abc")
  (observed.truncated)
  let children = root.children(p".", max_entries: 1)?
  (children.state) == ("truncated")
  (children.children.len()) == (1)
  let all = root.children(p".")?
  (raw in all.children)
  (root.filesystem_stats(p".")?.state) == ("observed")
  test.error_kind(root.read_result(raw, max_bytes: -1), "fs-root-read-result")?
  test.error_kind(root.children(p".", max_entries: -1), "fs-root-children")?
  (root.read_result(p"missing")?.state) == ("absent")
}

test fs_root_methods_keep_mutation_defaults_and_symlink_confinement [fs, error] { |ctx|
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"nested/child", parents: true)?
  root.mkdir(p"restricted", mode: 0o700)?
  (root.metadata(p"restricted")?.mode % 512) == (0o700)
  root.write(p"nested/data", "data")?
  root.chmod(p"nested/data", 0o600)?
  (root.metadata(p"nested/data")?.mode % 512) == (0o600)
  root.symlink(p"data", p"nested/link")?
  (root.readlink(p"nested/link")?) == (p"data")
  (root.readlink_result(p"nested/link")?.state) == ("observed")
  (root.read_text(p"nested/link")?) == ("data")
  (root.readlink_result(p"missing")?.state) == ("absent")
  let outside = test.temp_path(ctx, name: "root-outside")
  root.symlink(outside, p"escape")?
  test.error_kind(root.read_text(p"escape"), "fs-root-read")?
  root.remove(p"nested/link")?
  (! root.exists(p"nested/link")?)
  root.remove(p"nested/child", dir: true)?
  (root.host_path() is Ok(_))
}

test fs_root_methods_named_arguments_evaluate_in_source_order [error] { |ctx|
  let output = test.run_script(ctx, r"""
let root = fs.tempdir()?
defer root.close()?
proc next_path() [] -> Path {
  print "path"
  p"data"
}
proc data() [] -> Str {
  print "data"
  "payload"
}
root.write(data: data(), path: next_path())?
print (root.read_text(p"data")?)
""")?
  (output.status) == (0)
  (output.stdout) == ("data\npath\npayload\n")
}

test fs_root_methods_reject_structural_forgery_aliases_and_missing_effect [error] { |ctx|
  for source in [
    "let forged: FsRoot = {id: 1}\n",
    "let forged: Any = {id: 1}\nlet _ = forged.require(FsRoot)?\n",
    "let root = fs.tempdir()?\nfs.close_root(root)?\n",
    "proc read(root: FsRoot) [] -> Result[Bytes] { root.read_bytes(p\"data\") }\n",
  ] {
    let output = test.run_script(ctx, source)?
    (output.status != 0)
  }
}

test fs_root_removed_spellings_remain_usable_as_user_module_functions [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "root-user-module")?
  fp"${root}/helper.xsh".write_atomic("""
##! Caller-owned functions.
## Returns its argument unchanged.
export pure root_path(value: Str) -> Str { value }
## Returns its argument unchanged.
export pure close_root(value: Str) -> Str { value }
""")?
  let output = test.run_script(ctx, """
use helper
print helper.root_path("path")
print helper.close_root("closed")
""", [], {XSH_MODULE_PATH: root.display()})?
  assert output.success, output.stderr
  output.stdout == "path\nclosed\n"
}


test fs_root_methods_optional_receiver_keeps_arguments_lazy [fs, error] {
  let absent: FsRoot? = null
  let missing = absent?.read_bytes(optional_root_path())
  (missing) == (null)
  let present: FsRoot? = fs.tempdir()?
  defer present.require(FsRoot)?.close()?
  (present?.children(p".") != null)
}

proc optional_root_path() [error] -> Path {
  error.fail("must stay lazy")?
  p"data"
}


test fs_root_methods_trace_retains_native_host_operation_identity [error] { |ctx|
  let output = test.run_xsht_trace(ctx, """
let root = fs.tempdir()?
defer root.close()?
root.write(p"data", "trace")?
print (root.read_text(p"data")?)
""", ["--raw", "--trace-format", "jsonl"])?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("trace\n")
  ("method.FsRoot.write" in output.stderr)
  ("FsRoot.read_text" in output.stderr)
}
