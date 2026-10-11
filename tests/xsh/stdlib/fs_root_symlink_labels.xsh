test test_fs_root_symlink_requires_each_path_label { |ctx|
  for arguments in [
    "p\"target\", p\"link\"",
    "p\"target\", path: p\"link\"",
    "target: p\"target\", p\"link\"",
  ] {
    let source = "let root = fs.tempdir()?\nroot.symlink(" + arguments + ")?\n"
    let rejected = test.expect(ctx, source, status: 2)?
    assert "check.named-arg" in rejected.stderr, rejected.stderr
    assert "passed by position" in rejected.stderr, rejected.stderr
  }
  let guarded = test.expect(ctx, "let root: FsRoot? = null\nlet result = root?.symlink(p\"target\", p\"link\")\n", status: 2)?
  assert "check.named-arg" in guarded.stderr, guarded.stderr
}

test test_fs_root_symlink_labels_keep_defaults_and_source_order { |ctx|
  let output = test.expect(ctx, r"""proc operand(label: Str, destination: Path) [] -> Path {
  print $label
  destination
}
let root = fs.tempdir()?
defer root.close()
root.symlink(path: operand("path", p"nested/link"), target: operand("target", p"data"))?
root.symlink(...{target: p"data", path: p"spread"})?
print (root.readlink(p"nested/link")?) (root.readlink(p"spread")?)
""", status: 0)?
  assert output.stdout == "path\ntarget\ndata data\n"
}

test test_required_argument_label_diagnostic_offers_a_concrete_edit { |ctx|
  let output = test.expect(ctx, "p\"source\".copy(p\"destination\")\n", status: 2)?
  assert "check.named-arg" in output.stderr, output.stderr
  assert "help: pass `to` by name -> to: " in output.stderr, output.stderr
}
