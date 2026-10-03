test test_music_convert { |ctx|
  let root = test.temp_dir(ctx, name: "music")?
  fp"${root}/track.mp3".write("fake")?
  let out = test.temp_path(ctx, name: "music-out")
  let output = run.text "xsh" "showcase/music-convert.xsh" -- --out $out --root $root --dry-run ?
  assert "track.mp3" in output
  assert "dry run" in output
}
