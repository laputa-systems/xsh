proc test_webp_dir_dry_run(ctx: TestContext) [fs, process, error] {
  let dir = test.temp_dir(ctx)?
  let _ = test.temp_file(ctx, name: "photo.jpg", contents: b"fake jpeg")?
  let root = dir.display()
  let output = run.text "xsh" "showcase/webp-dir.xsh" -- "--root="${root} ?
  test.contains(output, "would be converted")?
}

proc test_webp_dir_help() [fs, process, error] {
  let output = run.text "xsh" "showcase/webp-dir.xsh" -- --help ?
  test.contains(output, "quality")?
}
