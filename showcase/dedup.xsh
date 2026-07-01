#!/usr/bin/env -S xsh --
# Dedup
# Find duplicate files by sha256 and optionally delete redundant copies.
# Usage: xsh showcase/dedup.xsh -- --root DIR [--dry-run=false]
# Example: xsh showcase/dedup.xsh -- --root downloads
type FileInfo = {sha: Str, rel: Str, size: Int}

type Opts = {root: Path, dry_run: Bool, verbose: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      root: {form: "--root DIR", default: p"."},
      dry_run: {form: "--dry-run", default: true},
      verbose: {form: "--verbose", default: false},
    },
  )?

  let root = opts.root.resolve()?

  if opts.verbose {
    print f"hashing files in ${root.display()}"
  }

  # Hash every file and collect into typed records, then group by sha256
  let file_info: List[FileInfo] = fs.files(root)
    |> sort-by .path
    |> par-map { |entry|
      let sha = hash.sha256(entry.path)?.hex()
      let rel = entry.path.relative_to(root).display()
      {sha: sha, rel: rel, size: entry.size}
    }

  let dups = file_info
    |> group-by .sha
    |> where .items.len() > 1
    |> sort-by .key

  if dups.len() == 0 {
    print f"no duplicates found (${file_info.len()} files scanned)"
    return
  }

  var dup_count = 0
  var wasted_bytes = 0

  for grp in dups {
    let size = grp.items[0].size
    let cnt = grp.items.len()
    dup_count += cnt - 1
    wasted_bytes += size * (cnt - 1)
    print f"${grp.key}  ${size} bytes  ×${cnt}"

    for item in grp.items |> enumerate() {
      let marker = if item.index == 0 { "keep" } else { if opts.dry_run { "dup " } else { "DEL " } }
      print f"  [${marker}] ${item.value.rel}"

      if item.index > 0 and ! opts.dry_run {
        fp"${root}/${item.value.rel}".remove(missing_ok: true)?
      }
    }
  }

  print ""
  print f"${dups.len()} groups  ${dup_count} redundant files  ${wasted_bytes} wasted bytes"

  if opts.dry_run {
    print f"${dup_count} files would be deleted (dry run)"
  } else {
    print f"${dup_count} files deleted"
  }
}
