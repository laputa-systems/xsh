test test_music_convert [fs, process, error] { |ctx|
  let root = test.temp_dir(ctx, name: "music")?
  fp"${root}/track.mp3".write("fake")?
  let out = test.temp_path(ctx, name: "music-out")
  let output = run.text "xsh" "showcase/music-convert.xsh" -- --out $out --root $root --dry-run ?
  test.contains(output, "track.mp3")?
  test.contains(output, "dry run")?
}
