#!/usr/bin/env -S xsh --
# Todo Scan
# Collect TODO, FIXME, HACK, XXX, and NOTE comments across a tree.
# Usage: xsh showcase/todo-scan.xsh -- [--root DIR] [--tag TAG]
# Example: xsh showcase/todo-scan.xsh -- --root src --tag FIXME
type Hit = {file: Str, line: Int, tag: Str, text: Str}

type Opts = {root: Path, ext: List[Str], tag: Str, verbose: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      root: {
        form: "--root DIR",
        default: p".",
      },
      ext: {
        form: "--ext EXT",
        repeated: true,
      },
      tag: {
        form: "--tag TAG",
        default: "",
      },
      verbose: {
        form: "--verbose",
        default: false,
      },
    },
  )?

  let root = opts.root.resolve()?

  let scan_exts = if opts.ext.len() > 0 {
    opts.ext
  } else {
    [
      "rs",
      "go",
      "py",
      "js",
      "ts",
      "c",
      "cpp",
      "h",
      "xsh",
      "sh",
      "md",
    ]
  }

  let scan_ext_set = set.from(scan_exts)
  let re = regex.compile("\\b(TODO|FIXME|HACK|XXX|NOTE)\\b[:\\s]*(.*)")?

  let files = fs.files(root)
    |> where set.has(scan_ext_set, .path.ext())
    |> sort-by .path

  if opts.verbose {
    print f"scanning ${files.len()} files in ${root.display()}"
  }

  let hits: List[Hit] = files
    |> par-map { |entry|
      var file_hits: List[Hit] = []

      match entry.path.read_text() {
        Ok(src) => {
          let rel = entry.path.relative_to(root).display()

          for item in src.lines() |> enumerate() {
            let caps = re.captures(item.value)
            continue when caps.len() == 0
            let tag = caps[1]
            continue when opts.tag != "" and tag != opts.tag
            let body = caps[2].trim()
            file_hits = file_hits.push({file: rel, line: item.index + 1, tag: tag, text: body})
          }
        }
        Err(_) => {}
      }

      file_hits
    }
    |> flat-map { |file_hits|
      file_hits
    }

  if hits.len() == 0 {
    let filter_note = if opts.tag != "" { f" for tag ${opts.tag}" } else { "" }
    print f"no findings${filter_note} in ${files.len()} files scanned"
    return
  }

  let groups = hits
    |> group-by .tag
    |> sort-by .key

  for grp in groups {
    print f"${grp.key} (${grp.items.len()})"

    for h in grp.items |> sort-by .file {
      print f"  ${h.file}:${h.line}: ${h.text}"
    }

    print ""
  }

  print f"${hits.len()} findings across ${files.len()} files"
}
