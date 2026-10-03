#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

type WebpOptions = {quality: Int, jobs: Int, apply: Bool, root: Path}

type WebpResult = {converted: Bool}

pure image_ext(ext: Str) -> Bool {
  ext == "jpg" or ext == "jpeg" or ext == "png"
}

proc main(...argv: List[Str]) [fs, process, error] {
  let opts: WebpOptions = cli.parse(
    argv,
    {
      quality: {
        kind: "Int",
        short: "q",
        default: 85,
      },
      jobs: {
        kind: "Int",
        short: "j",
        default: cpu.count(),
      },
      apply: {
        kind: "Bool",
        default: false,
      },
      root: {
        kind: "Path",
        positional: true,
        default: p".",
      },
    },
  )?.require()?

  let tmp = fs.tempdir()?
  defer tmp.close()?
  let tmp_dir = tmp.host_path()?

  let entries = fs.files(opts.root, gitignore: false)
    |> where .kind == "file"
    |> where image_ext(.ext.lower())
    |> sort-by .path
    |> collect()

  let results = entries
    |> par-map(jobs: opts.jobs) { |entry|
      var out: WebpResult = WebpResult(converted: false)
      let rel = entry.path.relative_to(opts.root)
      let safe = rel.display().replace("/", "_")
      let webp_name = safe.replace(f".${entry.ext}", ".webp")
      let tmp_out = fp"${tmp_dir}/${webp_name}"
      let dest = fp"${entry.path.parent()}/${entry.name.replace(f".${entry.ext}", ".webp")}"
      let quality = f"${opts.quality}"

      if opts.apply {
        match run.capture --text cwebp -quiet -q $quality $entry.path -o $tmp_out {
          Ok(captured) => {
            var ok = true

            if ! captured.status.ok {
              print captured.stderr.trim()
              ok = false
            }

            if ok {
              if let Ok(meta) = tmp_out.metadata() {
                if meta.size == 0 {
                  print f"cwebp: empty output for ${entry.path}"
                  ok = false
                }
              } else {
                print f"cwebp: cannot stat output for ${entry.path}"
                ok = false
              }
            }

            if ok {
              let trash_status = run.status trash $entry.path

              if trash_status.ok {
                tmp_out.rename(dest, overwrite: true)?
                print f"${entry.path} -> ${dest}"
                out = {converted: true}
              } else {
                print f"trash failed: ${entry.path}"
              }
            }
          }
          Err(e) => print f"cwebp spawn: ${e.message}"
        }
      } else {
        print f"would convert: ${entry.path} -> ${dest}"
        out = {converted: true}
      }

      out
    }

  var converted = 0
  var skipped = 0

  for r in results {
    if r.converted {
      converted += 1
    } else {
      skipped += 1
    }
  }

  let verb = if opts.apply { "converted" } else { "would be converted" }
  print f"${converted} ${verb}  ${skipped} skipped/errored"
}
