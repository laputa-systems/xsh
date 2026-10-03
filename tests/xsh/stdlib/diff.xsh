test test_diff_unified { |ctx|
  let root = test.temp_dir(ctx, name: "diff")?
  let original = fp"${root}/original.txt"
  let modified = fp"${root}/modified.txt"

  original.write("""alpha
beta
""")?

  modified.write("""alpha
BETA
gamma
""")?

  let d = diff.unified(original, modified, context: 1)?
  assert d.files == 1
  assert d.hunks == 1
  assert "BETA" in d.text
}
