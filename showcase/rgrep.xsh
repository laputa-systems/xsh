#!/usr/bin/env -S xsh --
# Recursive Grep
# Search files by regex with extension filters and capped results.
# Usage: xsh showcase/rgrep.xsh -- --pattern REGEX [--root DIR] [--ext EXT]
# Example: xsh showcase/rgrep.xsh -- --pattern TODO --root src --ext rs
type SearchHit = {rel: Str, line: Int, text: Str}

type FileSearch = {rel: Str, hits: List[SearchHit]}

type Opts = {pattern: Str, root: Path, ext: List[Str], verbose: Bool, limit: Int}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      pattern: {
        form: "--pattern PATTERN",
        required: true,
      },
      root: {
        form: "--root DIR",
        default: p".",
      },
      ext: {
        form: "--ext EXT",
        repeated: true,
      },
      verbose: {
        form: "--verbose",
        default: false,
      },
      limit: {
        form: "--limit N",
        kind: "UInt",
        default: 50,
        min: 1,
      },
    },
  )?

  let re = regex.compile(opts.pattern)?
  let root = opts.root.resolve()?
  let exts = if opts.ext.len() == 0 { ["xsh", "txt", "md"] } else { opts.ext }
  let ext_set = set.from(exts)

  if opts.verbose {
    print f"pattern: ${opts.pattern}"
    print f"root: ${root.display()}"
    print f"extensions: ${exts.join(" ")}"
    print f"limit: ${opts.limit}"
  }

  let files = fs.files(root)
    |> where set.has(ext_set, .path.ext())
    |> sort-by .path

  let file_results: List[FileSearch] = files
    |> par-map { |entry|
      let rel = entry.path.relative_to(root).display()

      var hits = [
        {rel: rel, line: item.index + 1, text: item.value.trim()}
        for item in entry.path.lines()? |> enumerate()
        if re.matches(item.value)
      ]

      {rel: rel, hits: hits}
    }

  var total = 0

  for result in file_results {
    for hit in result.hits {
      print f"${hit.rel}:${hit.line}: ${hit.text}"
      total += 1

      if total >= opts.limit {
        print f"limit of ${opts.limit} reached"
        return
      }
    }

    if opts.verbose and result.hits.len() > 0 {
      print f"  (${result.hits.len()} matches in ${result.rel})"
    }
  }

  print f"${total} matches across ${files.len()} files scanned"
}
