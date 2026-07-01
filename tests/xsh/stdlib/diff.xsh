proc test_diff_unified(ctx: TestContext) [fs, error] {
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
  test.eq(d.files, 1)?
  test.eq(d.hunks, 1)?
  test.contains(d.text, "BETA")?
}
