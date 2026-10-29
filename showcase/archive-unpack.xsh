#!/usr/bin/env -S xsh --
# Archive Unpack
# List, extract, compress, or decompress archives using XSH archive APIs.
# Mutating operations publish to an absent path with an existing parent after success.
# SIGINT and SIGTERM clean staging after the current blocking archive call returns.
# Usage: xsh showcase/archive-unpack.xsh -- ARCHIVE [--out DIR] [--dry-run=false]
# Example: xsh showcase/archive-unpack.xsh -- backup.tar.gz --out /tmp/out --dry-run=false
type Opts = {archive: List[Str], out: Path, list: Bool, compress: Str, decompress: Str, dry_run: Bool}

type StagedOutput = {published: Path, pending: Path}

on SIGINT [process, error] {
  exit 3
}

on SIGTERM [process, error] {
  exit 3
}

proc staged_output(dest: Path) [fs, process, error] -> Result[StagedOutput] {
  let output_parent = dest.parent
  let parent = if output_parent == "" { fs.cwd()? } else { output_parent.resolve()? }
  let published = fp"{parent}/{dest.name()}"
  if published.exists() {
    print f"destination already exists: {published}"
    exit 1
  }

  let pending = fp"{parent}/.{dest.name()}.xsh-stage"
  pending.mkdir(parents: false)
  {published, pending}
}

proc main(...argv: List[Str]) [fs, process, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      archive: {
        form: "...ARCHIVE",
        repeated: true,
        required_group: "input",
      },
      out: {
        form: "--out DIR",
        default: p".",
      },
      list: {
        form: "--list",
        default: false,
      },
      compress: {
        form: "--compress FILE",
        default: "",
        required_group: "input",
      },
      decompress: {
        form: "--decompress FILE",
        default: "",
        required_group: "input",
      },
      dry_run: {
        form: "--dry-run",
        default: true,
      },
    },
  )?

  if opts.compress != "" {
    let src = fp"{opts.compress}"
    let dest = fp"{src}.gz"

    if opts.dry_run {
      print f"would compress {src.name()} → {dest.name()} (dry run)"
      return
    }

    let output = staged_output(dest)?
    defer output.pending.remove()
    let staged_file = fp"{output.pending}/{dest.name()}"
    archive.compress(src, staged_file)
    staged_file.rename(to: output.published)
    print f"compressed {src.name()} → {dest.name()} ({dest.metadata()?.size} bytes)"
    return
  }

  if opts.decompress != "" {
    let src = fp"{opts.decompress}"
    let dest = if opts.out == p"." { src.with_ext("") } else { opts.out }

    if opts.dry_run {
      print f"would decompress {src.name()} → {dest.name()} (dry run)"
      return
    }

    let output = staged_output(dest)?
    defer output.pending.remove()
    let staged_file = fp"{output.pending}/{dest.name()}"
    archive.decompress(src, staged_file)
    staged_file.rename(to: output.published)
    print f"decompressed {src.name()} → {dest.name()}"
    return
  }

  let archive_arg = opts.archive.get(0) ?? ""
  let archive_path = fp"{archive_arg}"
  let name = archive_path.name()
  let is_zip = name.ends_with(".zip")
  let format: archive.Format = if is_zip { .Zip } else { .Tar }
  let entries = archive.list(archive_path, format)?.collect()

  for e in entries |> sort-by .path {
    let kind = if e.kind == .Directory { "dir" } else if e.kind == .Symlink { "symlink" } else { "file" }
    let size = e.size ?? 0
    print f"  {kind}  {e.path}  {size}b"
  }

  print f"{entries.len()} entries in {name}"

  if ! opts.list {
    if opts.dry_run {
      print f"would extract to {opts.out} (dry run)"
    } else {
      let output = staged_output(opts.out)?
      defer output.pending.remove()

      archive.extract(archive_path, output.pending, format)?

      output.pending.rename(to: output.published)
      print f"extracted to {opts.out}"
    }
  }
}
