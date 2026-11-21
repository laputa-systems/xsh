const dated = rx"^(.+)-(\d{4})-(\d{2})-(\d{2})\.tar\.gz$"

cli main(dir: Path, keep: Int = 3, dry_run = false) {
  if keep < 1 or ! (dir.is_dir() ?? false) {
    eprint "prune: usage error"
    exit 2
  }

  var report: Map[List[Str]] = {}
  for service in fs.children(dir)? |> where .kind == "dir" {
    let entries = fs.children(service.path)? |> map { |e| e }
    if entries |> any { .name == ".lock" } {
      report[service.name] = []
      continue
    }

    let names = entries |> where { |e| e.kind == "file" and dated.captures(e.name).len() == 5 } |> map .name |> sort()
    let old = names |> take(names.len() - keep)
    if ! dry_run {
      for name in old {
        fp"{service.path}/{name}".remove()
      }
    }

    report[service.name] = old
  }

  print (json.encode(report)?)
}
