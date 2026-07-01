#!/usr/bin/env -S xsh --
# Batch Rename
# Preview or apply bulk file renames with normalization, prefixes, suffixes, and numbering.
# Usage: xsh showcase/batch-rename.xsh -- --root DIR [--normalize] [--dry-run=false]
# Example: xsh showcase/batch-rename.xsh -- --root photos --normalize --number
type Opts = {root: Path, ext: Str, normalize: Bool, prefix: Str, suffix: Str, number: Bool, dry_run: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      root: {form: "--root DIR", default: p"."},
      ext: {form: "--ext EXT", default: ""},
      normalize: {form: "--normalize", default: false},
      prefix: {form: "--prefix STR", default: ""},
      suffix: {form: "--suffix STR", default: ""},
      number: {form: "--number", default: false},
      dry_run: {form: "--dry-run", default: true},
    },
  )?

  let root = opts.root.resolve()?

  let files = if opts.ext == "" {
    fs.files(root) |> sort-by .path
  } else {
    fs.files(root)
      |> where .path.ext() == opts.ext
      |> sort-by .path
  }

  if files.len() == 0 {
    print f"no files found in ${root.display()}"
    return
  }

  var renamed = 0
  var skipped = 0

  for item in files |> enumerate() {
    let src = item.value.path
    let old_name = src.name()
    let file_ext = src.ext()

    # Derive stem by stripping the extension via with_ext
    let stem = src.with_ext("").name()
    var new_stem = stem

    if opts.normalize {
      new_stem = new_stem.translate(" ", "_")
    }

    if opts.prefix != "" {
      new_stem = f"${opts.prefix}${new_stem}"
    }

    if opts.suffix != "" {
      new_stem = f"${new_stem}${opts.suffix}"
    }

    if opts.number {
      let n = item.index + 1
      let seq = if n < 10 { f"00${n}" } else { if n < 100 { f"0${n}" } else { f"${n}" } }
      new_stem = f"${seq}_${new_stem}"
    }

    let new_name = if file_ext == "" { new_stem } else { f"${new_stem}.${file_ext}" }

    if new_name == old_name {
      skipped += 1
      continue
    }

    let dest = fp"${src.parent()}/${new_name}"
    let action = if opts.dry_run { "would rename" } else { "rename" }
    print f"${action}: ${old_name} → ${new_name}"

    if ! opts.dry_run {
      src.rename(dest)?
    }

    renamed += 1
  }

  print ""

  if opts.dry_run {
    print f"${renamed} files would be renamed  ${skipped} unchanged (dry run)"
  } else {
    print f"${renamed} files renamed  ${skipped} unchanged"
  }
}
