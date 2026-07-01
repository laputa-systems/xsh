proc test_patch_apply(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "patch")?

  fp"${root}/original.txt".write("""alpha
beta
""")?

  let patch_text = """--- original.txt
+++ original.txt
@@ -1,2 +1,3 @@
 alpha
-beta
+BETA
+gamma
"""

  let applied = patch.apply(root, patch_text)?
  test.eq(applied.files, 1)?
  test.eq(applied.hunks, 1)?
  test.contains(fp"${root}/original.txt".read_text()?, "gamma")?

  let escape_patch = """--- /dev/null
+++ ../escape.txt
@@ -0,0 +1 @@
+bad
"""

  test.error_kind(patch.apply(root, escape_patch), "patch-path")?
}
