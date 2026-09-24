#!/usr/bin/env -S xsh --
# Backup Rotate
# Keep the newest direct backup files by name and delete or preview older files.
# Active deletion is best effort; run it against a directory that is not changing.
# Usage: xsh showcase/backup-rotate.xsh -- --dir DIR --keep N [--dry-run=false]
# Example: xsh showcase/backup-rotate.xsh -- --dir backups --keep 7
type Opts = {dir: Path, keep: Int, pattern: Str, dry_run: Bool, verbose: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      dir: {
        form: "--dir DIR",
        default: p".",
      },
      keep: {
        form: "--keep N",
        kind: "UInt",
        default: 5,
        min: 1,
      },
      pattern: {
        form: "--pattern REGEX",
        default: "",
      },
      dry_run: {
        form: "--dry-run",
        default: true,
      },
      verbose: {
        form: "--verbose",
        default: false,
      },
    },
  )?

  let dir = opts.dir.resolve()?
  let name_re = if opts.pattern == "" { regex.compile(".")? } else { regex.compile(opts.pattern)? }

  # Sort descending by path so the lexicographically largest names (newest ISO dates) come first
  let all_files = fs.ls(dir)
    |> where .kind == "file" and name_re.matches(.path.name())
    |> sort-by --desc .path

  if all_files.len() == 0 {
    print f"no files found in ${dir.display()}"
    return
  }

  if opts.verbose {
    print f"${all_files.len()} files found, keeping ${opts.keep}"
  }

  var kept = 0
  var deleted = 0

  for item in all_files |> enumerate() {
    let entry = item.value
    let name = entry.path.name()

    if item.index < opts.keep {
      if opts.verbose {
        print f"keep: ${name}"
      }

      kept += 1
      continue
    }

    if opts.dry_run {
      print f"would delete: ${name}"
    } else {
      entry.path.remove()?
      print f"delete: ${name}"
    }

    deleted += 1
  }

  print ""

  if opts.dry_run {
    print f"kept ${kept}  would delete ${deleted} (dry run)"
  } else {
    print f"kept ${kept}  deleted ${deleted}"
  }
}
