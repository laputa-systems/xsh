let root = fp"${args[0]}"
let manifest = fs.files(root, gitignore: false)
  |> map { |entry|
    let data = entry.path.read_bytes()?
    {
      path: entry.path.strip_prefix(root)?.display(),
      sha256: data.sha256().hex(),
      size: data.len(),
    }
  }
  |> sort-by .path

let encoded = json.encode(manifest)?
print ${manifest |> count()} encoded.byte_len()
