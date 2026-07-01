pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_music_convert(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "music")?
  fp"${root}/track.mp3".write("fake")?
  let out = test.temp_path(ctx, name: "music-out")
  let output = run.text xsh_bin() "showcase/music-convert.xsh" -- --out $out --root $root --dry-run ?
  test.contains(output, "track.mp3")?
  test.contains(output, "dry run")?
}
