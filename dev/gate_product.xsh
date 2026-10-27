proc main(...arguments: List[Str]) [fs, error, io] {
  guard arguments.len() == 1 else { exit 2 }
  let product_path = fp"{arguments[0]}"
  let before = fs.stat(product_path)?
  guard before.kind == "file" and product_path.executable()? else { exit 2 }
  let digest = hash.sha256(product_path)?.hex()
  let after = fs.stat(product_path)?
  guard before.kind == after.kind and before.dev == after.dev and before.ino == after.ino and
    before.mode == after.mode and before.size == after.size and before.mtime_ns == after.mtime_ns and before.ctime_ns == after.ctime_ns else { exit 2 }
  print json.encode({digest: digest, mode: before.mode.bit_and(0o7777)})?
}
