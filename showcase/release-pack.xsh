#!/usr/bin/env -S xsh --
# Release Pack
# Stage files, write a manifest with hashes, and create a release tarball.
# Publish only to an absent output directory with an existing parent, after the archive is complete.
# Usage: xsh showcase/release-pack.xsh -- INPUT OUTPUT [--dry-run=false]
# Example: xsh showcase/release-pack.xsh -- dist target/release-pack --dry-run=false
type ManifestEntry = {path: Str, size: Int, sha256: Str}

type Opts = {input: Path, output: Path, dry_run: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      input: {
        form: "INPUT",
        default: p".",
      },
      output: {
        form: "OUTPUT",
        default: p"target/showcase-release",
      },
      dry_run: {
        form: "--dry-run",
        default: true,
      },
    },
  )?

  let absolute_source = opts.input.resolve()?

  if opts.dry_run {
    let preview = fs.files(absolute_source) |> sort-by .path
    print f"would stage ${preview.len()} files from ${absolute_source.display()}"
    print f"would write to ${opts.output.display()} (dry run)"
    return
  }

  let output_parent = opts.output.parent
  let parent = if output_parent.display() == "" { fs.cwd()? } else { output_parent.resolve()? }
  let output = fp"${parent}/${opts.output.name()}"
  if output.exists()? {
    print f"output already exists: ${output.display()}"
    abort(1)
  }

  match output.strip_prefix(absolute_source) {
    Ok(_) => {
      print "output must be outside the input tree"
      abort(1)
    }
    Err(_) => {}
  }

  let pending = fp"${parent}/.${opts.output.name()}.xsh-stage"
  pending.mkdir(parents: false)?
  defer pending.remove(missing_ok: true)?
  let stage = fp"${pending}/stage"
  let payload = fp"${stage}/payload"
  let copied = fs.copy_tree(absolute_source, payload)?
  let payload_root = payload.resolve()?

  let entries: List[ManifestEntry] = fs.files(payload_root)
    |> sort-by .path
    |> map { |entry|
      let rel = entry.path.strip_prefix(payload_root)?
      {path: rel.display(), size: entry.size, sha256: entry.path.read_bytes()?.sha256().hex()}
    }

  let manifest = fp"${stage}/MANIFEST.json"
  json.write(manifest, {source: absolute_source.display(), files: entries})?
  let staged_tarball = fp"${pending}/release.tar"
  archive.tar_create(staged_tarball, stage, [p"."], "auto")?
  let listed = archive.tar_list(staged_tarball)?.collect()
  let digest = staged_tarball.read_bytes()?.sha256().hex()
  pending.rename(output)?
  let tarball = fp"${output}/release.tar"
  print f"staged ${copied.files} files ${copied.dirs} dirs"
  print f"archive ${tarball} entries ${listed.len()} sha256 ${digest}"
}
