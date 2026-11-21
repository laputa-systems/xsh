const dated = rx"^(.+)-(\d{4})-(\d{2})-(\d{2})\.tar\.gz$"

type Backup = {name: Str, path: Path, date: Str}

cli main(dir: Path, keep: Int = 3, dry_run = false) {
  if keep < 1 {
    eprint "prune: --keep must be positive"
    exit 2
  }

  var report: Map[List[Str]] = {}
  for service in fs.children(dir)? |> where .kind == "dir" {
    let entries = fs.children(service.path)? |> map { |e| e }
    if entries |> any { .name == ".lock" } {
      report[service.name] = []
      continue
    }

    var backups: List[Backup] = []
    for entry in entries {
      let parts = dated.captures(entry.name)
      if parts.len() == 5 and parts[1] == service.name {
        backups += [Backup(name: entry.name, path: entry.path, date: f"{parts[2]}{parts[3]}{parts[4]}")]
      }
    }

    let old = backups |> sort-by(desc: true) .date |> drop(keep)
    if ! dry_run {
      for backup in old {
        if backup.path.is_dir()? {
          backup.path.remove_dir()
        } else {
          backup.path.remove()
        }
      }
    }

    report[service.name] = old |> map .name |> sort()
  }

  print (json.encode(report)?)
}
