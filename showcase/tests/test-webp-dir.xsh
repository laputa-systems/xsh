test test_webp_dir_dry_run { |ctx|
  let dir = test.temp_dir(ctx)?
  let _ = test.temp_file(ctx, name: "photo.jpg", contents: b"fake jpeg")?
  let root = dir.display()
  let output = run.text "xsh" "showcase/webp-dir.xsh" -- "--root="${root} ?
  "would be converted" in output
}

test test_webp_dir_help {
  let output = run.text "xsh" "showcase/webp-dir.xsh" -- --help ?
  "quality" in output
}
