test test_dedup { |ctx|
  let root = test.temp_dir(ctx, name: "dedup")?
  fp"${root}/a.txt".write("same content")?
  fp"${root}/b.txt".write("same content")?
  fp"${root}/c.txt".write("unique")?
  let output = run.text "xsh" "showcase/dedup.xsh" -- --root $root ?
  assert "1 groups" in output
  assert "1 redundant files" in output
}
