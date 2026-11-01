test test_full_inference_fsroot_calls_keep_selected_native_contracts { |ctx|
  let output = test.run_xsh(
    ctx,
    p"tests/fixtures/runtime/generic-fsroot-calls.xsh".read_text()?,
  )?
  assert output.success, output.stderr
  assert output.stdout == "rooted calls preserved\n"
}

test test_full_inference_fsroot_calls_reject_invalid_receiver_and_effects { |ctx|
  for source in [
    "proc listed(marker, root: Str) [fs, error] { let _ = marker; root.children(p\".\")? }",
    "proc listed(marker, root: FsRoot) [] { let _ = marker; root.children(p\".\")? }",
    "pure listed(marker, root: FsRoot) { let _ = marker; root.children(p\".\") }",
    "proc listed(marker, root: FsRoot) [fs, error] { let _ = marker; root.children(p\".\", max_entries: true)? }",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "err[check." in output.stderr
  }
}

test test_known_fsroot_overloads_keep_later_argument_domains { |ctx|
  let source = r"""proc prepare(root, name: RelPath) [fs, error] -> Path {
  root.write(name, b"")
  root.host_path()?
}
proc store(root, name: RelPath, contents) [fs, error] {
  let _ = prepare(root, name)
  root.write(name, contents)
}
proc named(root, name: RelPath, contents) [fs, error] {
  let _ = prepare(root, name)
  root.write(data: contents, path: name)
}
proc caller(root: FsRoot, name: RelPath) [fs, error] {
  store(root, name, b"payload")
}
proc atomic(root: FsRoot, name: RelPath, contents) [fs, error] {
  root.write_atomic(name, contents)
}
proc persist(destination: Path, contents) [fs, error] {
  destination.write_atomic(contents)
}
let root = fs.tempdir()?
defer root.close()
caller(root, p"binary")?
store(root, p"text", "message")?
named(root, p"named-binary", b"named")?
named(root, p"named-text", "label")?
atomic(root, p"atomic-binary", b"atomic")?
atomic(root, p"atomic-text", "saved")?
persist(fp"{root.host_path()?}/path-binary", b"path")?
persist(fp"{root.host_path()?}/path-text", "written")?
assert root.read_bytes(p"binary")? == b"payload"
assert root.read_text(p"text")? == "message"
assert root.read_bytes(p"named-binary")? == b"named"
assert root.read_text(p"named-text")? == "label"
assert root.read_bytes(p"atomic-binary")? == b"atomic"
assert root.read_text(p"atomic-text")? == "saved"
assert root.read_bytes(p"path-binary")? == b"path"
assert root.read_text(p"path-text")? == "written"
"""
  let output = test.run_xsh(ctx, source)?
  assert output.success, output.stderr
  for invalid in [
    "let _ = store(root, p\"invalid\", 7)\n",
    "let raw: Any = b\"unchecked\"\nlet _ = store(root, p\"invalid\", raw)\n",
  ] {
    let rejected = test.run_xsh(ctx, source + invalid)?
    assert ! rejected.success, rejected.stdout
    assert "err[check." in rejected.stderr, rejected.stderr
  }
}
