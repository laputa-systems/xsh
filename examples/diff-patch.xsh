let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let original = fp"${root}/config.old"
let modified = fp"${root}/config.new"

original.write("""name=demo
enabled=false
""")

modified.write("""name=demo
enabled=true
""")

let d = diff.unified(original, modified, context: 1)?
let apply_root = fp"${root}/apply"
fs.mkdir(apply_root)

fp"${apply_root}/config".write("""name=demo
enabled=false
""")

let patch_text = """--- config
+++ config
@@ -1,2 +1,2 @@
 name=demo
-enabled=false
+enabled=true
"""

let applied = patch.apply(apply_root, patch_text)?
let config = fp"${apply_root}/config".read_text()?.trim()
print $d.files $d.hunks $applied.files $applied.hunks ${"enabled=true" in config}
