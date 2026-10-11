test test_fs_path_frame_apply_executes_live_host_rows { |ctx|
  let root = test.temp_dir(ctx, name: "fs-path-frame-apply")?
  let nested = fp"{root}/nested"
  nested.mkdir(parents: true)
  let file = fp"{nested}/payload.txt"
  file.write("payload")
  file.write_atomic(b"bytes")

  assert file.read_text()? == "bytes"
  assert file.read_bytes()? == b"bytes"
  assert file.exists()?
  assert ! file.executable()?
  assert file.du()? >= 0
  assert file.metadata()?.size == 5
  assert file.resolve()?.name() == "payload.txt"

  let link = fp"{nested}/link"
  link.symlink(to: file)
  assert link.readlink()?.display() == file.display()
  assert link.resolve()?.display() == file.resolve()?.display()

  let files = fs.files(root, gitignore: false, stat: true, exts: ["txt"], hidden: false)?.collect()
  assert files.len() == 1
  let walked = fs.walk(root, gitignore: false, stat: true, hidden: false)?.collect()
  assert walked.len() >= 3
  let children = fs.children(nested, stat: true, ordered: true)?.collect()
  assert children.len() == 2

  let tarball = fp"{root}/payload.tar"
  archive.tar_create(tarball, root, [p"nested/payload.txt"])
  assert tarball.exists()?

  let scoped = fp"{root}/scoped"
  tempdir scratch at scoped {
    assert scratch.exists()?
    fp"{scratch}/item".write("scoped")
  }
  assert ! scoped.exists()?

  let removed = fp"{nested}/remove-me"
  removed.write("temporary")
  removed.remove(missing_ok: false)
  assert ! removed.exists()?
}

test test_fs_path_frame_apply_evaluates_source_operands_before_host_creation { |ctx|
  let root = test.temp_dir(ctx, name: "fs-path-frame-order")?
  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/member".write("payload")
  let target = fp"{root}/not-created.tar"
  let order_file = fp"{root}/operand-order"

  let failed = test.run_script(
    ctx,
    f"""var order: List[Str] = []
let invalid: Any = "not a boolean"
archive.tar_create(
  if true {{ order += ["path"]; p"{target}" }} else {{ p"{target}" }},
  if true {{ order += ["root"]; p"{source}" }} else {{ p"{source}" }},
  if true {{ order += ["entries"]; [p"member"] }} else {{ [p"member"] }},
  compression: if true {{ order += ["compression"]; "auto" }} else {{ "auto" }},
  overwrite: if true {{
    order += ["overwrite"]
    p"{order_file}".write(order.join(","))
    invalid.require(Bool)?
  }} else {{ false }},
)
""",
  )?

  assert failed.status == 3, failed.stderr
  assert "schema check failed" in failed.stderr
  assert order_file.read_text()? == "path,root,entries,compression,overwrite"
  assert ! target.exists()?
}

test test_path_write_evaluates_operands_before_host_failure { |ctx|
  let root = test.temp_dir(ctx, name: "path-write-frame-data")?
  let target = fp"{root}/missing/target"
  let order_file = fp"{root}/order"

  let failed = test.run_script(
    ctx,
    f"""let target = if true {{
  p"{order_file}".write("path,")
  p"{target}"
}} else {{ p"{target}" }}
target.write(if true {{
  let order = p"{order_file}".read_text()?
  p"{order_file}".write(f"{{order}}data")
  "payload"
}} else {{ "payload" }})
""",
  )?

  assert failed.status == 3, failed.stderr
  assert order_file.read_text()? == "path,data"
  assert ! target.exists()?
}

test test_fs_tempdir_frame_apply_releases_created_resources { |ctx|
  let root = test.temp_dir(ctx, name: "fs-tempdir-frame-cleanup")?
  let path_file = fp"{root}/scratch-path"
  let output = test.run_script(
    ctx,
    f"""let scratch = fs.tempdir()?
p"{path_file}".write(scratch.host_path()?.display())
""",
  )?

  assert output.status == 0, output.stderr
  let scratch = Path(path_file.read_text()?)
  assert ! scratch.exists()?
}
