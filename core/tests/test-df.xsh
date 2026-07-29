pure normalize_df_mount(line: Str) -> Str {
  let fields = line.words()
  if fields.get(0, "") == "Filesystem" {
    return fields.join(" ")
  }
  return f"${fields.get(0, "")} ${fields.get(1, "")} ${fields.get(5, "")}"
}

proc normalize_df_mounts(text: Str) [error] -> Str {
  let lines = [normalize_df_mount(line) for line in text.trim().lines().collect()]
  return lines.join("\n")
}

proc test_df(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "df")?
  fp"${root}/payload.txt".write("abcdef")?
  let resolved = root.resolve()?
  let stats = fs.filesystem_stats(resolved)?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/df.xsh" -- -kP $root ?
  test.contains(output, "Filesystem 1024-blocks Used Available Capacity Mounted on")?
  test.contains(output, f" ${stats.blocks_1k} ")?
  let fake_used = root.du()?
  test.ok(! (f"${resolved} ${fake_used} ${fake_used} 0 100% ${resolved}" in output))?
}

proc test_df_matches_alpine_kp(ctx: TestContext) [fs, process, env, error] {
  if env.bool("XSH_SKIP_LIVE_COREUTILS_COMPARISONS")? {
    test.skip("live coreutils comparison disabled")
  }

  let alpine_release = /etc/alpine-release

  if ! alpine_release.exists()? {
    test.skip("Alpine-only df comparison")
  }

  let root = test.temp_dir(ctx, name: "df-alpine")?
  fp"${root}/payload.txt".write("abcdef")?
  let alpine = run.text df -kP $root ?
  let ours = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/df.xsh" -- -kP $root ?
  test.eq(normalize_df_mounts(ours), normalize_df_mounts(alpine))?
}
