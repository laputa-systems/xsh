const dated = rx"^(.+)-(\d{4})-(\d{2})-(\d{2})\.tar\.gz$"

type Backup = {name: Str, path: Path, date: Str}

proc doomed(service: Path, name: Str, keep: Int) -> Result[List[Backup]] {
  let entries = fs.children(service)? |> map { |e| e }
  if entries |> any { .name == ".lock" } {
    return []
  }

  var backups: List[Backup] = []
  for entry in entries {
    let parts = dated.captures(entry.name)
    if entry.kind == "file" and parts.len() == 5 and parts[1] == name {
      backups += [Backup(name: entry.name, path: entry.path, date: f"{parts[2]}{parts[3]}{parts[4]}")]
    }
  }

  backups |> sort-by(desc: true) .date |> drop(keep)
}

cli main(dir: Path, keep: Int = 3, dry_run = false) {
  if keep < 1 {
    eprint f"prune: --keep must be a positive integer, got {keep}"
    exit 2
  }

  if ! (dir.is_dir() ?? false) {
    eprint f"prune: {dir} is not a directory"
    exit 2
  }

  var report: Map[List[Str]] = {}
  for service in fs.children(dir)? |> where .kind == "dir" {
    let old = doomed(service.path, service.name, keep)?
    if ! dry_run {
      for backup in old {
        backup.path.remove()
      }
    }

    report[service.name] = old |> map .name |> sort()
  }

  print (json.encode(report)?)
}
