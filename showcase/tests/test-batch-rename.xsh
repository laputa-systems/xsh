test test_batch_rename { |ctx|
  let root = test.temp_dir(ctx, name: "rename")?
  fp"{root}/hello world.txt".write("a")
  fp"{root}/foo bar.txt".write("b")
  let dry = run.text "xsh" "showcase/batch-rename.xsh" -- --root $root --normalize --dry-run ?
  assert "would rename" in dry
  assert "hello_world.txt" in dry
  let actual = run.text "xsh" "showcase/batch-rename.xsh" -- --root $root --normalize --dry-run=false ?
  assert "2 files renamed" in actual
  assert fp"{root}/hello_world.txt".exists()?
}
