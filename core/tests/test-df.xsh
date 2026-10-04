pure normalize_df_mount(line: Str) -> Str {
  let fields = line.words()
  return fields.join(" ") when (fields.get(0) ?? "") == "Filesystem"

  f"{fields.get(0) ?? ""} {fields.get(1) ?? ""} {fields.get(5) ?? ""}"
}

proc normalize_df_mounts(text: Str) [error] -> Str {
  let lines = [normalize_df_mount(line) for line in text.trim().lines().collect()]
  lines.join("\n")
}

test test_df { |ctx|
  let root = test.temp_dir(ctx, name: "df")?
  fp"{root}/payload.txt".write("abcdef")?
  let resolved = root.resolve()?
  let stats = fs.filesystem_stats(resolved)?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -kP $root ?
  assert "Filesystem 1024-blocks Used Available Capacity Mounted on" in output
  assert f" {stats.blocks_1k} " in output
  let fake_used = root.du()?
  assert ! (f"{resolved} {fake_used} {fake_used} 0 100% {resolved}" in output)
}

test test_df_matches_alpine_kp { |ctx|
  if env.bool("XSH_SKIP_LIVE_COREUTILS_COMPARISONS")? {
    test.skip("live coreutils comparison disabled")
  }

  let alpine_release = /etc/alpine-release

  if ! alpine_release.exists()? {
    test.skip("Alpine-only df comparison")
  }

  let root = test.temp_dir(ctx, name: "df-alpine")?
  fp"{root}/payload.txt".write("abcdef")?
  let alpine = run.text df -kP $root ?
  let ours = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -kP $root ?
  assert normalize_df_mounts(ours) == normalize_df_mounts(alpine)
}
