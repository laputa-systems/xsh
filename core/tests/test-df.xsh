pure normalize_df_mount(line: Str) -> Str {
  let fields = line.words()
  return fields.join(" ") when (fields.get(0) ?? "") == "Filesystem"

  f"{fields.get(0) ?? ""} {fields.get(1) ?? ""} {fields.get(5) ?? ""}"
}

proc normalize_df_mounts(text: Str) [error] -> Str {
  let lines = [normalize_df_mount(line) for line in text.trim().lines().collect()]
  lines.join("\n")
}

type TerminalRun = {status: Int, stderr: Str}

proc run_df_on_terminal(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[TerminalRun] {
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let root = test.temp_dir(ctx, name: "df-terminal")?
  let stdout = fp"{root}/stdout"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/df.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", stdout, fp"{pty.name}")
  let status = process.run(plan)?
  let stderr = unix.read_fd(pty.master, 8192)?.utf8()?
  Ok({status: status.exit_code()?, stderr: stderr.replace("\r\n", with: "\n")})
}

test test_df { |ctx|
  let root = test.temp_dir(ctx, name: "df")?
  fp"{root}/payload.txt".write("abcdef")
  let resolved = root.resolve()?
  let stats = fs.filesystem_stats(resolved)?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -kP $root
  assert output.lines().collect()[0].words().join(" ") == "Filesystem 1024-blocks Used Available Capacity Mounted on"
  assert f" {stats.blocks_1k} " in output
  let fake_used = root.du()?
  assert ! (f"{resolved} {fake_used} {fake_used} 0 100% {resolved}" in output)
}

test test_df_matches_alpine_kp { |ctx|
  if env.bool("XSH_SKIP_LIVE_COREUTILS_COMPARISONS") {
    test.skip("live coreutils comparison disabled")
  }

  let alpine_release = /etc/alpine-release

  if ! alpine_release.exists() {
    test.skip("Alpine-only df comparison")
  }

  let root = test.temp_dir(ctx, name: "df-alpine")?
  fp"{root}/payload.txt".write("abcdef")
  let alpine = run.text df -kP $root
  let ours = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -kP $root
  assert normalize_df_mounts(ours) == normalize_df_mounts(alpine)
}

test test_df_k_and_portability_select_units_and_header { |ctx|
  let root = test.temp_dir(ctx, name: "df-units")?
  let stats = fs.statvfs(root)?
  let k = run.capture --text DF_BLOCK_SIZE=1 ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -kP $root
  assert k.status.exited_with(0), k.stderr
  let lines = k.stdout.lines().collect()
  assert lines[0].words().join(" ") == "Filesystem 1024-blocks Used Available Capacity Mounted on"
  let fields = lines[1].words()
  let total = stats.blocks * stats.fragment_size
  assert fields[1].parse_int()? == (total + 1023) / 1024
  let portable = run.capture --text DF_BLOCK_SIZE=1 ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -P $root
  assert portable.status.exited_with(0), portable.stderr
  assert portable.stdout.lines().collect()[0].words().join(" ") == "Filesystem 1024-blocks Used Available Capacity Mounted on"
  let ordinary = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- $root
  assert ordinary.status.exited_with(0), ordinary.stderr
  assert ordinary.stdout.lines().collect()[0].words().join(" ") == "Filesystem 1K-blocks Used Available Use% Mounted on"
  let block_human = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -B human-readable $root
  assert block_human.status.exited_with(0), block_human.stderr
  assert block_human.stdout.lines().collect()[0].words().join(" ") == "Filesystem Size Used Avail Use% Mounted on"
  let block_si = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -B si $root
  assert block_si.status.exited_with(0), block_si.stderr
}

test test_df_column_selection_total_and_error_continuation { |ctx|
  let root = test.temp_dir(ctx, name: "df-output")?
  let good = fp"{root}/good"
  good.write("file")
  let missing = fp"{root}/missing"
  let output = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- --output=file,target --total $missing $good
  assert output.status.exited_with(1)
  assert "No such file or directory" in output.stderr
  let lines = output.stdout.lines().collect()
  assert lines.len() == 3
  assert lines[0].words().join(" ") == "File Mounted on"
  assert lines[1].words()[0] == good.display()
  assert lines[2].words().join(" ") == "- total"
}

test test_df_explicit_decimal_block_label_and_invalid_suffix { |ctx|
  let root = test.temp_dir(ctx, name: "df-label")?
  let selected = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -B128000 --output=size $root
  assert selected.status.exited_with(0), selected.stderr
  assert selected.stdout.lines().collect()[0].trim() == "128kB-blocks"
  let invalid = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -B1fb $root
  assert invalid.status.exited_with(1)
  assert invalid.stderr == "df: invalid suffix in --block-size argument '1fb'\n"
}

pure df_percent(used: Int, available: Int) -> Str {
  let total = used + available
  return "-" when total <= 0 or used < 0

  f"{(used * 100 + total - 1) / total}%"
}

# Other processes can change the host filesystem between df and statvfs reads.
test test_df_reports_filesystem_capacity_counters { |ctx|
  let root = test.temp_dir(ctx, name: "df-capacity")?
  fp"{root}/payload".write("capacity")
  let output = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -B1 --output=size,used,avail,pcent $root
  assert output.status.exited_with(0), output.stderr
  let stats = fs.statvfs(root)?
  let expected_size = stats.blocks * stats.fragment_size
  let expected_used = (stats.blocks - stats.blocks_free) * stats.fragment_size
  let expected_available = stats.blocks_available * stats.fragment_size
  let lines = output.stdout.lines().collect()
  assert lines.len() == 2
  let fields = lines[1].words()
  assert fields[0] == f"{expected_size}"
  let used = fields[1].parse_int()?
  let available = fields[2].parse_int()?
  let used_delta = if used > expected_used { used - expected_used } else { expected_used - used }
  let available_delta = if available > expected_available { available - expected_available } else { expected_available - available }
  assert used_delta <= 16 * stats.fragment_size
  assert available_delta <= 16 * stats.fragment_size
  assert fields[3] == df_percent(used, available)
}

test test_df_block_size_errors_point_into_terminal_arguments { |ctx|
  let unknown = run_df_on_terminal(ctx, ["-B", "1fb"])?
  assert unknown.status == 1
  assert unknown.stderr == """df: invalid suffix in --block-size argument '1fb'
   ╭─[ df:1:8 ]
   │
 1 │ df -B 1fb
   │        ─┬
   │         ╰── not a known unit
   │
   │ Help: a size is a number and an optional unit: K, M, G and so on for 1024, KB, MB, GB for 1000
───╯
""", unknown.stderr

  let zero = run_df_on_terminal(ctx, ["--block-size=0"])?
  assert zero.status == 1
  assert "df: invalid --block-size argument '0'" in zero.stderr, zero.stderr
  assert "df:1:17" in zero.stderr, zero.stderr
  assert ! ("not a known unit" in zero.stderr), zero.stderr
}

# Hiding /proc behind an empty tmpfs needs a private mount namespace, so this
# runs df through unshare and skips where unprivileged namespaces are off.
test test_df_operand_is_measured_when_mount_table_is_unreadable { |ctx|
  let probe = run.status unshare -rm true
  if ! probe.exited_with(0) { test.skip("user namespaces are unavailable"); return }
  let root = test.temp_dir(ctx, name: "df-masked-proc")?
  let helper = fp"{root}/mask-proc"
  helper.write("#!/bin/sh\nmount -t tmpfs tmpfs /proc || exit 99\nexec \"$@\"\n", mode: 0o755)
  let plain = run.capture --text unshare -rm ${helper} ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- .
  assert plain.status.exited_with(0), plain.stderr
  assert "cannot read table of mounted file systems" in plain.stderr, plain.stderr
  assert plain.stdout.lines().collect()[0].words()[0] == "Filesystem", plain.stdout
  for args in [["-a", "."], ["-l", "."], ["-t", "ext4", "."], ["-x", "tmpfs", "."]] {
    let filtered = run.capture --text unshare -rm ${helper} ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- ${args}
    assert filtered.status.exited_with(1), f"df {args.join(" ")}: {filtered.stdout}"
    assert "cannot read table of mounted file systems" in filtered.stderr, filtered.stderr
  }
}

# GNU df accepts -v and ignores it; the output must match the run without it.
test test_df_verbose_option_has_no_effect { |ctx|
  let root = test.temp_dir(ctx, name: "df-verbose")?
  let good = fp"{root}/good"
  good.write("file")
  let plain = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- -k --output=file,target $good
  assert plain.status.exited_with(0), plain.stderr
  for args in [["-v", "-k"], ["-kv"], ["-vk"]] {
    let verbose = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/df.xsh" -- ${args} --output=file,target $good
    assert verbose.status.exited_with(0), f"df {args.join(" ")}: {verbose.stderr}"
    assert verbose.stdout == plain.stdout, f"df {args.join(" ")}: {verbose.stdout}"
    assert verbose.stderr == plain.stderr, f"df {args.join(" ")}: {verbose.stderr}"
  }
}
