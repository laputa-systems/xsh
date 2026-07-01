let root = fp"${args[0]}"
let pkgroot = fp"${root}/pkgroot"

let manifest = fs.files(pkgroot, gitignore: false)
  |> map { |entry|
    let data = entry.path.read_bytes()?

    {
      path: entry.path.strip_prefix(pkgroot)?.display(),
      sha256: data.sha256().hex(),
      size: data.len(),
      executable: entry.mode % 512 == 493,
    }
  }
  |> sort-by .path

let manifest_json = json.encode(manifest)?

let total_size = manifest
  |> map .size
  |> sum

print ${manifest |> count()} $total_size manifest[0].path manifest[0].sha256 manifest_json.count_lines()
