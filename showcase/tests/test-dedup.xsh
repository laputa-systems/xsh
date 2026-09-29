test test_dedup [fs, process, error] { |ctx|
  let root = test.temp_dir(ctx, name: "dedup")?
  fp"${root}/a.txt".write("same content")?
  fp"${root}/b.txt".write("same content")?
  fp"${root}/c.txt".write("unique")?
  let output = run.text "xsh" "showcase/dedup.xsh" -- --root $root ?
  test.contains(output, "1 groups")?
  test.contains(output, "1 redundant files")?
}
