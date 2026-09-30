test test_patch_apply [fs, error] { |ctx|
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
  applied.files == 1
  applied.hunks == 1
  "gamma" in fp"${root}/original.txt".read_text()?

  let escape_patch = """--- /dev/null
+++ ../escape.txt
@@ -0,0 +1 @@
+bad
"""

  test.error_kind(patch.apply(root, escape_patch), "patch-path")?
}
