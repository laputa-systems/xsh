proc xsh_bin() [env] -> Path {
  let bin = env.get("CARGO_BIN_EXE_xsh") ?? ""

  if bin != "" {
    return fp"${bin}"
  }

  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = env.get("XSH_CORE_DIR") ?? ""

  if dir != "" {
    return fp"${dir}/${name}"
  }

  return ../name
}

proc normalize_df(text: Str) [error] -> Str {
  let lines = [line.words().join(" ") for line in text.trim().lines().collect()]
  return lines.join("\n")
}

proc test_df(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "df")?
  fp"${root}/payload.txt".write("abcdef")?
  let resolved = root.resolve()?
  let stats = fs.filesystem_stats(resolved)?
  let output = run.text xsh_bin() core_script("df.xsh") -- -kP $root ?
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
  let ours = run.text xsh_bin() core_script("df.xsh") -- -kP $root ?
  test.eq(normalize_df(ours), normalize_df(alpine))?
}
