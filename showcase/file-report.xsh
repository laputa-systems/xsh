#!/usr/bin/env -S xsh --
# File Report
# Report files of one extension with sizes and sha256 hashes.
# Usage: xsh showcase/file-report.xsh -- [--root DIR] [--ext EXT]
# Example: xsh showcase/file-report.xsh -- --root scripts --ext xsh
type FileEntry = {path: Str, size: Int, sha256: Str}

type Opts = {root: Path, ext: Str, verbose: Bool}

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
        default: "xsh",
      },
      verbose: {
        form: "--verbose",
        default: false,
      },
    },
  )?

  let root = opts.root.resolve()?

  if opts.verbose {
    print f"scanning ${root} for .${opts.ext} files"
  }

  # tee logs each file as it's hashed, before the pipeline knows the final order.
  # The final report is sorted largest-first — tee and sort are independent.
  let entries: List[FileEntry] = fs.files(root)
    |> where .path.ext() == opts.ext
    |> sort-by .path
    |> tee { |entry|
      if opts.verbose {
        print f"  hashing ${entry.path.relative_to(root)}"
      }
    }
    |> par-map { |entry|
      let rel = entry.path.relative_to(root)
      let sha = entry.path.read_bytes()?.sha256().hex()
      {path: rel.display(), size: entry.size, sha256: sha}
    }
    |> sort-by --desc .size

  if entries.len() == 0 {
    print f"no .${opts.ext} files found in ${root}"
    return
  }

  let total = entries
    |> map .size
    |> sum

  print f"${"file":<48} ${"bytes":>8}  sha256"
  print f"${"----":<48} ${"-----":>8}  ------"

  for e in entries {
    print f"${e.path:<48} ${e.size:>8}  ${e.sha256}"
  }

  print f"${entries.len()} files  ${total} bytes"
}
