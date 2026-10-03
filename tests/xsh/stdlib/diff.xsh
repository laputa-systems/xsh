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
  d.files == 1
  d.hunks == 1
  "BETA" in d.text
}
