test test_dedup { |ctx|
  let root = test.temp_dir(ctx, name: "dedup")?
  fp"${root}/a.txt".write("same content")?
  fp"${root}/b.txt".write("same content")?
  fp"${root}/c.txt".write("unique")?
  let output = run.text "xsh" "showcase/dedup.xsh" -- --root $root ?
  "1 groups" in output
  "1 redundant files" in output
}
