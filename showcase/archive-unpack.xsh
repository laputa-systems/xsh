#!/usr/bin/env -S xsh --
# Archive Unpack
# List, extract, compress, or decompress archives using XSH archive APIs.
# Usage: xsh showcase/archive-unpack.xsh -- ARCHIVE [--out DIR] [--dry-run=false]
# Example: xsh showcase/archive-unpack.xsh -- backup.tar.gz --out /tmp/out --dry-run=false
type Opts = {archive: List[Str], out: Path, list: Bool, compress: Str, decompress: Str, dry_run: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      archive: {form: "...ARCHIVE", repeated: true, required_group: "input"},
      out: {form: "--out DIR", default: p"."},
      list: {form: "--list", default: false},
      compress: {form: "--compress FILE", default: "", required_group: "input"},
      decompress: {form: "--decompress FILE", default: "", required_group: "input"},
      dry_run: {form: "--dry-run", default: true},
    },
  )?

  if opts.compress != "" {
    let src = fp"${opts.compress}"
    let dest = fp"${src}.gz"

    if opts.dry_run {
      print f"would compress ${src.name()} → ${dest.name()} (dry run)"
      return
    }

    archive.compress(src, dest)?
    print f"compressed ${src.name()} → ${dest.name()} (${dest.metadata()?.size} bytes)"
    return
  }

  if opts.decompress != "" {
    let src = fp"${opts.decompress}"
    let dest = if opts.out == p"." { src.with_ext("") } else { opts.out }

    if opts.dry_run {
      print f"would decompress ${src.name()} → ${dest.name()} (dry run)"
      return
    }

    archive.decompress(src, dest)?
    print f"decompressed ${src.name()} → ${dest.name()}"
    return
  }

  let archive_arg = opts.archive.get(0, "")
  let archive_path = fp"${archive_arg}"
  let name = archive_path.name()
  let is_zip = name.ends_with(".zip")
  let entries = if is_zip { archive.zip_list(archive_path)? } else { archive.tar_list(archive_path)? }

  for e in entries |> sort-by .path {
    print f"  ${e.kind}  ${e.path.display()}  ${e.size}b"
  }

  print f"${entries.len()} entries in ${name}"

  if ! opts.list {
    if opts.dry_run {
      print f"would extract to ${opts.out.display()} (dry run)"
    } else {
      opts.out.mkdir()?

      if is_zip {
        archive.zip_extract(archive_path, opts.out)?
      } else {
        archive.tar_extract(archive_path, opts.out)?
      }

      print f"extracted to ${opts.out.display()}"
    }
  }
}
