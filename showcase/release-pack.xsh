#!/usr/bin/env -S xsh --
# Release Pack
# Stage files, write a manifest with hashes, and create a release tarball.
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

  opts.output.remove(missing_ok: true)?
  opts.output.mkdir()?
  let stage = fp"${opts.output}/stage"
  let payload = fp"${stage}/payload"
  payload.mkdir()?
  let payload_root = payload.resolve()?
  let copied = fs.copy_tree(absolute_source, payload_root, parents: true, overwrite: true)?

  let entries: List[ManifestEntry] = fs.files(payload_root)
    |> sort-by .path
    |> map { |entry|
      let rel = entry.path.strip_prefix(payload_root)?
      {path: rel.display(), size: entry.size, sha256: entry.path.read_bytes()?.sha256().hex()}
    }

  let manifest = fp"${stage}/MANIFEST.json"
  json.write(manifest, {source: absolute_source.display(), files: entries})?
  let tarball = fp"${opts.output}/release.tar"
  archive.tar_create(tarball, stage, [p"."], "auto", true)?
  let listed = archive.tar_list(tarball)?.collect()
  let digest = tarball.read_bytes()?.sha256().hex()
  print f"staged ${copied.files} files ${copied.dirs} dirs"
  print f"archive ${tarball} entries ${listed.len()} sha256 ${digest}"
}
