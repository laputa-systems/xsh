let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
fs.write(fp"${root}/a.txt", b"abc")?
fs.write(fp"${root}/b.txt", b"")?

let manifest = fs.children(root)
  |> where .kind == "file"
  |> map { |entry|
    let data = entry.path.read_bytes()?
    {path: entry.path.strip_prefix(root)?.display(), sha256: data.sha256().hex(), size: entry.size}
  }
  |> sort-by .path

let manifest_json = json.encode(manifest)?
hash.verify_file(fp"${root}/a.txt", sha256: b"abc".sha256().hex())
let parsed = hash.parse_check_line(f"${manifest[0].sha256}  ${manifest[0].path}")?
print ${manifest |> count()} manifest[0].path manifest[0].sha256
print $parsed.path $parsed.binary ${"\"b.txt\"" in manifest_json}
