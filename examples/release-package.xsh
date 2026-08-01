let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let source = fp"${root}/source"
let bin = fp"${source}/usr/bin"
bin.mkdir()
let tool = fp"${bin}/tool"
let config = fp"${source}/config"

tool.write("""demo
""")
tool.chmod(0o755)
config.write("""name=demo
enabled=false
""")

let tarball = fp"${root}/demo.tar.gz"
archive.tar_create(tarball, source, [p"."])
let entries = archive.tar_list(tarball)?.collect()
let destination = fp"${root}/destination"
archive.tar_extract(tarball, destination)

let patch_text = """--- config
+++ config
@@ -1,2 +1,2 @@
 name=demo
-enabled=false
+enabled=true
"""

let patch_report = patch.apply(destination, patch_text)?
let extracted_tool = fp"${destination}/usr/bin/tool"
let extracted_config = fp"${destination}/config".read_text()?.trim()
let compressed = fp"${root}/tool.gz"
archive.compress(extracted_tool, compressed, format: "gzip")
let decoded = archive.decompress_bytes(compressed)?.utf8()?.trim()
let tool_text = extracted_tool.read_text()?.trim()
let executable = extracted_tool.metadata()?.mode % 512 == 493

print "archive" entries.len() $tool_text $executable
print "patch" $patch_report.files $patch_report.hunks ${"enabled=true" in extracted_config}
print "compressed" $decoded
