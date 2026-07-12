#!/usr/bin/env -S xsh --
# Music Convert
# Plan or run ffmpeg audio conversion to AAC, preserving relative paths.
# Usage: xsh showcase/music-convert.xsh -- --root DIR --out DIR [--dry-run=false]
# Example: xsh showcase/music-convert.xsh -- --root Music --out Converted
pure nearest_aac_kbps(kbps: Int) -> Int {
  # Map kbps to nearest aac_at tier using midpoint thresholds
  if kbps <= 20 {
    return 16
  }

  if kbps <= 28 {
    return 24
  }

  if kbps <= 36 {
    return 32
  }

  if kbps <= 44 {
    return 40
  }

  if kbps <= 52 {
    return 48
  }

  if kbps <= 60 {
    return 56
  }

  if kbps <= 72 {
    return 64
  }

  if kbps <= 88 {
    return 80
  }

  if kbps <= 104 {
    return 96
  }

  if kbps <= 120 {
    return 112
  }

  if kbps <= 144 {
    return 128
  }

  if kbps <= 176 {
    return 160
  }

  if kbps <= 208 {
    return 192
  }

  if kbps <= 240 {
    return 224
  }

  if kbps <= 288 {
    return 256
  }

  320
}

pure ext_default_kbps(ext: Str) -> Int {
  match ext {
    "flac" => 256
    "wav" => 256
    "aiff" => 256
    "alac" => 256
    "ogg" => 192
    "opus" => 128
    "wma" => 192
    _ => 192
  }
}

type ConvertResult = {source: Str, ext: Str, orig_kbps: Int, aac_kbps: Int, ok: Bool}

type Opts = {root: Path, out: Path, dry_run: Bool, verbose: Bool}

proc main(...argv: List[Str]) [fs, process, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      root: {
        form: "--root DIR",
        default: p".",
      },
      out: {
        form: "--out DIR",
        kind: "Path",
        required: true,
      },
      dry_run: {
        form: "--dry-run",
        default: true,
      },
      verbose: {
        form: "--verbose",
        default: false,
      },
    },
  )?

  let root = opts.root.resolve()?
  let out_dir = opts.out

  let audio_exts = [
    "mp3",
    "flac",
    "ogg",
    "wav",
    "aiff",
    "alac",
    "wma",
    "opus",
  ]

  let audio_ext_set = set.from(audio_exts)

  let files = fs.files(root)
    |> where set.has(audio_ext_set, .path.ext())
    |> sort-by .path

  if files.len() == 0 {
    print f"no audio files found in ${root.display()}"
    return
  }

  print f"found ${files.len()} audio files in ${root.display()}"
  var results: List[ConvertResult] = []

  for entry in files {
    let src = entry.path
    let rel = src.relative_to(root)
    let ext = src.ext()
    let orig_kbps = ext_default_kbps(ext)
    let aac_kbps = nearest_aac_kbps(orig_kbps)

    if opts.verbose {
      print f"  ${rel.display()}: ${ext} ${orig_kbps}kbps → ${aac_kbps}kbps"
    }

    if opts.dry_run {
      results = results.push({source: rel.display(), ext: ext, orig_kbps: orig_kbps, aac_kbps: aac_kbps, ok: true})
      continue
    }

    let dest = fp"${out_dir}/${rel}".with_ext("m4a")
    dest.parent().mkdir()?

    let cmd = process.command_argv(
      "ffmpeg",
      [
        "ffmpeg",
        "-i",
        src.display(),
        "-c:a",
        "aac_at",
        "-b:a",
        f"${aac_kbps}k",
        "-vn",
        "-y",
        dest.display(),
      ],
    )

    let status = process.run(cmd)?

    results = results.push(
      {source: rel.display(), ext: ext, orig_kbps: orig_kbps, aac_kbps: aac_kbps, ok: status.exited_with(0)},
    )
  }

  print ""
  print f"${"source":<50} ${"ext":<5} ${"orig":>6} ${"aac":>6}  status"
  print f"${"------":<50} ${"---":<5} ${"----":>6} ${"---":>6}  ------"

  for r in results {
    let st = if r.ok { "ok" } else { "FAIL" }
    print f"${r.source:<50} ${r.ext:<5} ${r.orig_kbps:>5}k ${r.aac_kbps:>5}k  ${st}"
  }

  let ok_n = results
    |> where .ok
    |> count()

  print ""

  if opts.dry_run {
    print f"${results.len()} files would be converted (dry run)"
  } else {
    print f"${ok_n} converted  ${results.len() - ok_n} failed"
  }
}
