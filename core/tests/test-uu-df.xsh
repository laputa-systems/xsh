##! Native ports of the uutils df integration tests.

use support.uu as uu

pure lines(r: uu.Ran) -> Result[List[Str], Error] {
  Ok(r.stdout.utf8()?.lines().collect())
}

# The type-filter check needs a working directory on a filesystem different
# from /dev; temp scenes may share its tmpfs filesystem type.
proc invoke_at_root(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let words = uu.argv(s, "df", [Path(word) for word in args])?
  let out = uu.at(s, "root-stdout")
  let err = uu.at(s, "root-stderr")
  let status = process.run(process.command_argv(s.ctx.xsh_bin, words, p"/", {}, b"", out, err))?
  Ok({util: "df", args: args, status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# A mount namespace keeps the temporary /proc mask out of the test runner.
proc masked_proc(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let wrapper = uu.at(s, "mask-proc")
  wrapper.write("#!/bin/sh\nmount -t tmpfs tmpfs /proc || exit 99\nexec \"$@\"\n", mode: 0o755)?
  let words = [p"unshare", p"-rm", wrapper].extend(uu.argv(s, "df", [Path(word) for word in args])?)
  let out = uu.at(s, "masked-stdout")
  let err = uu.at(s, "masked-stderr")
  let status = process.run(process.command_argv("unshare", words, s.root, {}, b"", out, err, timeout: 10s))?
  Ok({util: "df", args: args, status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: uutils test_df::test_invalid_arg
test test_uu_df_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--definitely-invalid"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_df::test_df_compatible_no_size_arg
test test_uu_df_df_compatible_no_size_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-a"])?
  uu.succeeds(r0)
}

# origin: uutils test_df::test_df_shortened_long_argument
test test_uu_df_df_shortened_long_argument { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--a"])?
  uu.succeeds(r0)
}

# origin: uutils test_df::test_df_compatible
test test_uu_df_df_compatible { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-ah"])?
  uu.succeeds(r0)
}

# origin: uutils test_df::test_df_compatible_type
test test_uu_df_df_compatible_type { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-aT"])?
  uu.succeeds(r0)
}

# origin: uutils test_df::test_df_compatible_si
test test_uu_df_df_compatible_si { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-aH"])?
  uu.succeeds(r0)
}

# origin: uutils test_df::test_df_compatible_sync
test test_uu_df_df_compatible_sync { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--sync"])?
  uu.succeeds(r0)
}

# origin: uutils test_df::test_df_arguments_override_themselves
test test_uu_df_df_arguments_override_themselves { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--help", "--help"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "df", ["-aa"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "df", ["--block-size=3000", "--block-size=1000"])?
  uu.succeeds(r2)
  let r3 = uu.invoke(s, "df", ["--total", "--total"])?
  uu.succeeds(r3)
  let r4 = uu.invoke(s, "df", ["-hh"])?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "df", ["-HH"])?
  uu.succeeds(r5)
  let r6 = uu.invoke(s, "df", ["-ii"])?
  uu.succeeds(r6)
  let r7 = uu.invoke(s, "df", ["-kk"])?
  uu.succeeds(r7)
  let r8 = uu.invoke(s, "df", ["-ll"])?
  uu.succeeds(r8)
  let r9 = uu.invoke(s, "df", ["--no-sync", "--no-sync"])?
  uu.succeeds(r9)
  let r10 = uu.invoke(s, "df", ["-PP"])?
  uu.succeeds(r10)
  let r11 = uu.invoke(s, "df", ["--sync", "--sync"])?
  uu.succeeds(r11)
  let r12 = uu.invoke(s, "df", ["-TT"])?
  uu.succeeds(r12)
}

# origin: uutils test_df::test_df_conflicts_overriding
test test_uu_df_df_conflicts_overriding { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-hH"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "df", ["-Hh"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "df", ["--no-sync", "--sync"])?
  uu.succeeds(r2)
  let r3 = uu.invoke(s, "df", ["--sync", "--no-sync"])?
  uu.succeeds(r3)
  let r4 = uu.invoke(s, "df", ["-k", "--block-size=3000"])?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "df", ["--block-size=3000", "-k"])?
  uu.succeeds(r5)
}

# origin: uutils test_df::test_df_output_arg
test test_uu_df_df_output_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--output=source", "-iPT"])?
  uu.fails(r0)
  let r1 = uu.invoke(s, "df", ["-iPT", "--output=source"])?
  uu.fails(r1)
  let r2 = uu.invoke(s, "df", ["--output=source", "--output=source"])?
  uu.fails(r2)
}

# origin: uutils test_df::test_total_option_with_single_dash
test test_uu_df_total_option_with_single_dash { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-total"])?
  uu.fails(r0)
}

# origin: uutils test_df::test_output_option_without_equals_sign
test test_uu_df_output_option_without_equals_sign { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--output", "."])?
  uu.succeeds(r0)
}

# origin: uutils test_df::test_exclude_type_option
test test_uu_df_exclude_type_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-x", "ext4", "-x", "ext3"])?
  uu.succeeds(r0)
}

# origin: uutils test_df::test_df_output
test test_uu_df_df_output { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["-H", "--total"])?
  uu.succeeds(r)
  assert (lines(r)?)[0].words() == ["Filesystem", "Size", "Used", "Avail", "Use%", "Mounted", "on"]
}

# origin: uutils test_df::test_df_output_overridden
test test_uu_df_df_output_overridden { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["-hH", "--total"])?
  uu.succeeds(r)
  assert (lines(r)?)[0].words() == ["Filesystem", "Size", "Used", "Avail", "Use%", "Mounted", "on"]
}

# origin: uutils test_df::test_default_headers
test test_uu_df_default_headers { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", [])?
  uu.succeeds(r)
  assert (lines(r)?)[0].words() == ["Filesystem", "1K-blocks", "Used", "Available", "Use%", "Mounted", "on"]
}

# origin: uutils test_df::test_precedence_of_human_readable_and_si_header_over_output_header
test test_uu_df_precedence_of_human_readable_and_si_header_over_output_header { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["-h", "--human-readable", "-H", "--si"] {
    let r = uu.invoke(s, "df", [arg, "--output=size"])?
    uu.succeeds(r)
    assert (lines(r)?)[0] == " Size"
  }
}

# origin: uutils test_df::test_used_header_starts_with_space
test test_uu_df_used_header_starts_with_space { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["-h", "--output=used"])?
  uu.succeeds(r)
  assert (lines(r)?)[0] == " Used"
}

# origin: uutils test_df::test_df_follows_symlinks
test test_uu_df_df_follows_symlinks { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["-h", "--output=source"])?
  uu.succeeds(r)
  let output = lines(r)?
  for i in range(1, output.len()) {
    # Mount sources can be labels rather than paths; failed lookups are not links.
    assert ! (Path(output[i]).is_symlink() ?? false)
  }
}

# origin: uutils test_df::test_df_trailing_zeros
test test_uu_df_df_trailing_zeros { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["-h", "--output=size,used", "--total"])?
  uu.succeeds(r)
  assert ! rx"[[:space:]][1-9][A-Z]".matches(r.stdout.utf8()?)
}

# origin: uutils test_df::test_df_rounding
test test_uu_df_df_rounding { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["-H", "--output=size,used", "--total"])?
  uu.succeeds(r)
  assert ! rx"[[:space:]][0-9]{3}\\.[0-9][A-Z]".matches(r.stdout.utf8()?)
}

# origin: uutils test_df::test_order_same
test test_uu_df_order_same { |ctx|
  let s = uu.scene(ctx)?
  let first = uu.invoke(s, "df", ["--output=source"])?
  uu.succeeds(first)
  let second = uu.invoke(s, "df", ["--output=source"])?
  uu.succeeds(second)
  assert first.stdout == second.stdout
}

# origin: uutils test_df::test_output_mp_repeat
test test_uu_df_output_mp_repeat { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["/", "/"])?
  uu.succeeds(r)
  let output = lines(r)?
  assert output.len() == 3
  assert output[1].split(" ")[0] == output[2].split(" ")[0]
}

# origin: uutils test_df::test_output_conflict_options
test test_uu_df_output_conflict_options { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-i", "-T", "-P"] {
    let r = uu.invoke(s, "df", ["--output=source", option])?
    uu.fails(r)
  }
}

# origin: uutils test_df::test_output_option
test test_uu_df_output_option { |ctx|
  let s = uu.scene(ctx)?
  let all = uu.invoke(s, "df", ["--output"])?
  uu.succeeds(all)
  let selected = uu.invoke(s, "df", ["--output=source,target"])?
  uu.succeeds(selected)
  let invalid = uu.invoke(s, "df", ["--output=invalid_option"])?
  uu.fails(invalid)
}

# origin: uutils test_df::test_type_option
test test_uu_df_type_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["--output=fstype"])?
  uu.succeeds(r)
  let kind = (lines(r)?)[1].trim()
  for args in [["-t", kind], ["-t", kind, "-t", "nonexisting"]] {
    let r = uu.invoke(s, "df", args)?
    uu.succeeds(r)
  }
  let missing = uu.invoke(s, "df", ["-t", "nonexisting"])?
  uu.fails(missing)
  uu.stderr_contains(missing, "no file systems processed")
}

# origin: uutils test_df::test_type_option_with_file
test test_uu_df_type_option_with_file { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke_at_root(s, ["--output=fstype", "."])?
  uu.succeeds(r)
  let kind = (lines(r)?)[1].trim()
  let selected = invoke_at_root(s, ["-t", kind, "."])?
  uu.succeeds(selected)
  for args in [["-t", "nonexisting", "."], ["-t", kind, "/dev"]] {
    let rejected = invoke_at_root(s, args)?
    uu.fails(rejected)
    uu.stderr_contains(rejected, "no file systems processed")
  }
  let types = invoke_at_root(s, ["--output=fstype"])?
  uu.succeeds(types)
  let output = lines(types)?
  let others = [output[i] for i in range(1, output.len()) if output[i].trim() != kind and output[i].trim() != ""]
  if ! others.is_empty() {
    let rejected = invoke_at_root(s, ["-t", others[0], "."])?
    uu.fails(rejected)
    uu.stderr_contains(rejected, "no file systems processed")
  }
}

# origin: uutils test_df::test_exclude_all_types
test test_uu_df_exclude_all_types { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["--output=fstype"])?
  uu.succeeds(r)
  let output = lines(r)?
  var kinds: List[Str] = []
  var args: List[Str] = []
  for i in range(1, output.len()) {
    let kind = output[i].trim()
    if kind not in kinds { kinds += [kind]; args += ["-x", kind] }
  }
  let excluded = uu.invoke(s, "df", args)?
  uu.fails(excluded)
  uu.stderr_contains(excluded, "no file systems processed")
}

# origin: uutils test_df::test_include_exclude_same_type
test test_uu_df_include_exclude_same_type { |ctx|
  let s = uu.scene(ctx)?
  let single = uu.invoke(s, "df", ["-t", "ext4", "-x", "ext4"])?
  uu.fails(single)
  uu.stderr_is(single, "df: file system type 'ext4' both selected and excluded\n")
  let multiple = uu.invoke(s, "df", ["-t", "ext4", "-x", "ext4", "-t", "ext3", "-x", "ext3"])?
  uu.fails(multiple)
  uu.stderr_is(multiple, "df: file system type 'ext3' both selected and excluded\ndf: file system type 'ext4' both selected and excluded\n")
}

# origin: uutils test_df::test_total
test test_uu_df_total { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["--output=size,used,avail", "--total"])?
  uu.succeeds(r)
  let output = lines(r)?
  let reported = output[output.len() - 1].words()
  var computed = [0, 0, 0]
  for i in range(1, output.len() - 1) {
    let columns = output[i].words()
    for column in range(3) { computed[column] += columns[column].parse_int()? }
  }
  for column in range(3) { assert computed[column] == reported[column].parse_int()? }
}

# origin: uutils test_df::test_total_label_in_correct_column
test test_uu_df_total_label_in_correct_column { |ctx|
  let s = uu.scene(ctx)?
  for pair in [{arg: "source", expected: ["total"]}, {arg: "target", expected: ["total"]}, {arg: "source,target", expected: ["total", "-"]}, {arg: "target,source", expected: ["-", "total"]}] {
    let r = uu.invoke(s, "df", [f"--output={pair.arg}", "--total", "."])?
    uu.succeeds(r)
    let output = lines(r)?
    assert output[output.len() - 1].words() == pair.expected
  }
}

# origin: uutils test_df::test_use_percentage
test test_uu_df_use_percentage { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["--total", "--output=used,avail,pcent", "--block-size=1"])?
  uu.succeeds(r)
  let output = lines(r)?
  for i in range(1, output.len()) {
    let fields = output[i].words()
    let used = fields[0].parse_int()?
    let avail = fields[1].parse_int()?
    let reported = fields[2].replace("%", with: "").parse_int()?
    assert reported == (100 * used + used + avail - 1) / (used + avail)
  }
}

# origin: uutils test_df::test_iuse_percentage
test test_uu_df_iuse_percentage { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["--total", "--output=itotal,iused,ipcent"])?
  uu.succeeds(r)
  let output = lines(r)?
  for i in range(1, output.len()) {
    let fields = output[i].words()
    let total = fields[0].parse_int()?
    let used = fields[1].parse_int()?
    if fields[2] == "-" {
      assert total == 0
      assert used == 0
    } else {
      let reported = fields[2].replace("%", with: "").parse_int()?
      assert reported == (100 * used + total - 1) / total
    }
  }
}

# origin: uutils test_df::test_default_block_size
test test_uu_df_default_block_size { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--output=size"])?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "1K-blocks"
  let r1 = uu.invoke(s, "df", ["--output=size"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].trim() == "512B-blocks"
}

# origin: uutils test_df::test_default_block_size_in_posix_portability_mode
test test_uu_df_default_block_size_in_posix_portability_mode { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-P"])?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].words()[1] == "1024-blocks"
  let r1 = uu.invoke(s, "df", ["-P"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].words()[1] == "512-blocks"
}

# origin: uutils test_df::test_block_size_1024
test test_uu_df_block_size_1024 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-B", "1024", "--output=size"])?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "1K-blocks"
  let r1 = uu.invoke(s, "df", ["-B", "2048", "--output=size"])?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].trim() == "2K-blocks"
  let r2 = uu.invoke(s, "df", ["-B", "4096", "--output=size"])?
  uu.succeeds(r2)
  assert (lines(r2)?)[0].trim() == "4K-blocks"
  let r3 = uu.invoke(s, "df", ["-B", "1048576", "--output=size"])?
  uu.succeeds(r3)
  assert (lines(r3)?)[0].trim() == "1M-blocks"
  let r4 = uu.invoke(s, "df", ["-B", "2097152", "--output=size"])?
  uu.succeeds(r4)
  assert (lines(r4)?)[0].trim() == "2M-blocks"
  let r5 = uu.invoke(s, "df", ["-B", "1073741824", "--output=size"])?
  uu.succeeds(r5)
  assert (lines(r5)?)[0].trim() == "1G-blocks"
  let r6 = uu.invoke(s, "df", ["-B", "36507222016", "--output=size"])?
  uu.succeeds(r6)
  assert (lines(r6)?)[0].trim() == "34G-blocks"
  let r7 = uu.invoke(s, "df", ["-B", "128000", "--output=size"])?
  uu.succeeds(r7)
  assert (lines(r7)?)[0].trim() == "128kB-blocks"
  let r8 = uu.invoke(s, "df", ["-B", "1024000", "--output=size"])?
  uu.succeeds(r8)
  assert (lines(r8)?)[0].trim() == "1.1MB-blocks"
  let r9 = uu.invoke(s, "df", ["-B", "1000000000000", "--output=size"])?
  uu.succeeds(r9)
  assert (lines(r9)?)[0].trim() == "1TB-blocks"
}

# origin: uutils test_df::test_block_size_1m_option_with_output
test test_uu_df_block_size_1m_option_with_output { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-m", "--output=size"])?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "1M-blocks"
  let r1 = uu.invoke(s, "df", ["--output=size", "-m"])?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].trim() == "1M-blocks"
}

# origin: uutils test_df::test_block_size_with_suffix
test test_uu_df_block_size_with_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-B", "K", "--output=size"])?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "1K-blocks"
  let r1 = uu.invoke(s, "df", ["-B", "M", "--output=size"])?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].trim() == "1M-blocks"
  let r2 = uu.invoke(s, "df", ["-B", "G", "--output=size"])?
  uu.succeeds(r2)
  assert (lines(r2)?)[0].trim() == "1G-blocks"
  let r3 = uu.invoke(s, "df", ["-B", "1K", "--output=size"])?
  uu.succeeds(r3)
  assert (lines(r3)?)[0].trim() == "1K-blocks"
  let r4 = uu.invoke(s, "df", ["-B", "1M", "--output=size"])?
  uu.succeeds(r4)
  assert (lines(r4)?)[0].trim() == "1M-blocks"
  let r5 = uu.invoke(s, "df", ["-B", "1G", "--output=size"])?
  uu.succeeds(r5)
  assert (lines(r5)?)[0].trim() == "1G-blocks"
  let r6 = uu.invoke(s, "df", ["-B", "1KiB", "--output=size"])?
  uu.succeeds(r6)
  assert (lines(r6)?)[0].trim() == "1K-blocks"
  let r7 = uu.invoke(s, "df", ["-B", "1MiB", "--output=size"])?
  uu.succeeds(r7)
  assert (lines(r7)?)[0].trim() == "1M-blocks"
  let r8 = uu.invoke(s, "df", ["-B", "1GiB", "--output=size"])?
  uu.succeeds(r8)
  assert (lines(r8)?)[0].trim() == "1G-blocks"
  let r9 = uu.invoke(s, "df", ["-B", "1KB", "--output=size"])?
  uu.succeeds(r9)
  assert (lines(r9)?)[0].trim() == "1kB-blocks"
  let r10 = uu.invoke(s, "df", ["-B", "1MB", "--output=size"])?
  uu.succeeds(r10)
  assert (lines(r10)?)[0].trim() == "1MB-blocks"
  let r11 = uu.invoke(s, "df", ["-B", "1GB", "--output=size"])?
  uu.succeeds(r11)
  assert (lines(r11)?)[0].trim() == "1GB-blocks"
}

# origin: uutils test_df::test_block_size_in_posix_portability_mode
test test_uu_df_block_size_in_posix_portability_mode { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-P", "-B", "1024"])?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].words()[1] == "1024-blocks"
  let r1 = uu.invoke(s, "df", ["-P", "-B", "1K"])?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].words()[1] == "1024-blocks"
  let r2 = uu.invoke(s, "df", ["-P", "-B", "1KB"])?
  uu.succeeds(r2)
  assert (lines(r2)?)[0].words()[1] == "1000-blocks"
  let r3 = uu.invoke(s, "df", ["-P", "-B", "1M"])?
  uu.succeeds(r3)
  assert (lines(r3)?)[0].words()[1] == "1048576-blocks"
  let r4 = uu.invoke(s, "df", ["-P", "-B", "1MB"])?
  uu.succeeds(r4)
  assert (lines(r4)?)[0].words()[1] == "1000000-blocks"
}

# origin: uutils test_df::test_block_size_from_env
test test_uu_df_block_size_from_env { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--output=size"], vars: {DF_BLOCK_SIZE: "111"})?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "111B-blocks"
  let r1 = uu.invoke(s, "df", ["--output=size"], vars: {BLOCK_SIZE: "222"})?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].trim() == "222B-blocks"
  let r2 = uu.invoke(s, "df", ["--output=size"], vars: {BLOCKSIZE: "333"})?
  uu.succeeds(r2)
  assert (lines(r2)?)[0].trim() == "333B-blocks"
}

# origin: uutils test_df::test_block_size_from_env_zero
test test_uu_df_block_size_from_env_zero { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--output=size"], vars: {DF_BLOCK_SIZE: "0"})?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "1K-blocks"
  let r1 = uu.invoke(s, "df", ["--output=size"], vars: {BLOCK_SIZE: "0"})?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].trim() == "1K-blocks"
  let r2 = uu.invoke(s, "df", ["--output=size"], vars: {BLOCKSIZE: "0"})?
  uu.succeeds(r2)
  assert (lines(r2)?)[0].trim() == "1K-blocks"
}

# origin: uutils test_df::test_block_size_from_env_precedences
test test_uu_df_block_size_from_env_precedences { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--output=size"], vars: {DF_BLOCK_SIZE: "111", BLOCK_SIZE: "222"})?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "111B-blocks"
  let r1 = uu.invoke(s, "df", ["--output=size"], vars: {DF_BLOCK_SIZE: "111", BLOCKSIZE: "333"})?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].trim() == "111B-blocks"
  let r2 = uu.invoke(s, "df", ["--output=size"], vars: {BLOCK_SIZE: "222", BLOCKSIZE: "333"})?
  uu.succeeds(r2)
  assert (lines(r2)?)[0].trim() == "222B-blocks"
}

# origin: uutils test_df::test_precedence_of_block_size_arg_over_env
test test_uu_df_precedence_of_block_size_arg_over_env { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-B", "999", "--output=size"], vars: {DF_BLOCK_SIZE: "111"})?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "999B-blocks"
}

# origin: uutils test_df::test_invalid_block_size_from_env
test test_uu_df_invalid_block_size_from_env { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--output=size"], vars: {DF_BLOCK_SIZE: "invalid"})?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].trim() == "1K-blocks"
  let r1 = uu.invoke(s, "df", ["--output=size"], vars: {DF_BLOCK_SIZE: "invalid", BLOCK_SIZE: "222"})?
  uu.succeeds(r1)
  assert (lines(r1)?)[0].trim() == "1K-blocks"
  let r2 = uu.invoke(s, "df", ["--output=size"], vars: {DF_BLOCK_SIZE: "0", BLOCK_SIZE: "222"})?
  uu.succeeds(r2)
  assert (lines(r2)?)[0].trim() == "1K-blocks"
}

# origin: uutils test_df::test_ignore_block_size_from_env_in_posix_portability_mode
test test_uu_df_ignore_block_size_from_env_in_posix_portability_mode { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-P"], vars: {DF_BLOCK_SIZE: "111", BLOCK_SIZE: "222", BLOCKSIZE: "333"})?
  uu.succeeds(r0)
  assert (lines(r0)?)[0].words()[1] == "1024-blocks"
}

# origin: uutils test_df::test_df_binary_block_size
test test_uu_df_df_binary_block_size { |ctx|
  let s = uu.scene(ctx)?
  for pair in [["0b1", "1"], ["0b10100", "20"], ["0b1000000000", "512"], ["0b10K", "2K"]] {
    let binary = uu.invoke(s, "df", ["-B", pair[0], "--output=size"])?
    let decimal = uu.invoke(s, "df", ["-B", pair[1], "--output=size"])?
    uu.succeeds(binary)
    uu.succeeds(decimal)
    assert (lines(binary)?)[0].trim() == (lines(decimal)?)[0].trim()
  }
}

# origin: uutils test_df::test_df_binary_env_block_size
test test_uu_df_df_binary_env_block_size { |ctx|
  let s = uu.scene(ctx)?
  for key in ["DF_BLOCK_SIZE", "BLOCK_SIZE"] {
    let binary = uu.invoke(s, "df", ["--output=size"], vars: if key == "DF_BLOCK_SIZE" { {DF_BLOCK_SIZE: "0b10000000000"} } else { {BLOCK_SIZE: "0b10000000000"} })?
    let decimal = uu.invoke(s, "df", ["--output=size"], vars: if key == "DF_BLOCK_SIZE" { {DF_BLOCK_SIZE: "1024"} } else { {BLOCK_SIZE: "1024"} })?
    uu.succeeds(binary)
    uu.succeeds(decimal)
    assert (lines(binary)?)[0].trim() == (lines(decimal)?)[0].trim()
  }
}

# origin: uutils test_df::test_too_large_block_size
test test_uu_df_too_large_block_size { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--block-size=1Y"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "--block-size argument '1Y' too large")
  let r1 = uu.invoke(s, "df", ["--block-size=1Z"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "--block-size argument '1Z' too large")
}

# origin: uutils test_df::test_invalid_block_size
test test_uu_df_invalid_block_size { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--block-size=x"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid --block-size argument 'x'")
  let r1 = uu.invoke(s, "df", ["--block-size=0"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid --block-size argument '0'")
  let r2 = uu.invoke(s, "df", ["--block-size=0K"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "invalid --block-size argument '0K'")
}

# origin: uutils test_df::test_invalid_block_size_suffix
test test_uu_df_invalid_block_size_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--block-size=1H"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid suffix in --block-size argument '1H'")
  let r1 = uu.invoke(s, "df", ["--block-size=1.2"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid suffix in --block-size argument '1.2'")
}

# origin: uutils test_df::test_df_invalid_binary_size
test test_uu_df_df_invalid_binary_size { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["--block-size=0b123"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid suffix in --block-size argument '0b123'")
}

# origin: uutils test_df::test_df_binary_edge_cases
test test_uu_df_df_binary_edge_cases { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "df", ["-B0b"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid -B argument '0b'")
  let r1 = uu.invoke(s, "df", ["-B0B"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid -B argument '0B'")
  let r2 = uu.invoke(s, "df", ["--block-size=0b1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "too large")
}

# origin: uutils test_df::test_output_selects_columns
test test_uu_df_output_selects_columns { |ctx|
  let s = uu.scene(ctx)?
  let source = uu.invoke(s, "df", ["--output=source"])?
  uu.succeeds(source)
  assert (lines(source)?)[0] == "Filesystem"
  let target = uu.invoke(s, "df", ["--output=source,target"])?
  uu.succeeds(target)
  assert (lines(target)?)[0].words() == ["Filesystem", "Mounted", "on"]
  let used = uu.invoke(s, "df", ["--output=source,target,used"])?
  uu.succeeds(used)
  assert (lines(used)?)[0].words() == ["Filesystem", "Mounted", "on", "Used"]
}

# origin: uutils test_df::test_output_multiple_occurrences
test test_uu_df_output_multiple_occurrences { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["--output=source", "--output=target"])?
  uu.succeeds(r)
  assert (lines(r)?)[0].words() == ["Filesystem", "Mounted", "on"]
}

# origin: uutils test_df::test_output_file_all_filesystems
test test_uu_df_output_file_all_filesystems { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["--output=file"])?
  uu.succeeds(r)
  let output = lines(r)?
  assert output[0] == "File"
  for i in range(1, output.len()) { assert output[i] == "-" }
}

# origin: uutils test_df::test_output_file_specific_files
test test_uu_df_output_file_specific_files { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b", "c"] { uu.touch(s, name)? }
  let r = uu.invoke(s, "df", ["--output=file", "a", "b", "c"])?
  uu.succeeds(r)
  assert lines(r)? == ["File", "a", "b", "c"]
}

# origin: uutils test_df::test_file_column_width_if_filename_contains_unicode_chars
test test_uu_df_file_column_width_if_filename_contains_unicode_chars { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "äöü.txt")?
  let r = uu.invoke(s, "df", ["--output=file,target", "äöü.txt"])?
  uu.succeeds(r)
  assert (lines(r)?)[0] == "File Mounted on"
}

# origin: uutils test_df::test_output_field_no_more_than_once
test test_uu_df_output_field_no_more_than_once { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["--output=target,source,target"])?
  uu.fails(r)
  uu.stderr_is(r, "df: option --output: field 'target' used more than once\nTry 'df --help' for more information.\n")
}

# origin: uutils test_df::test_nonexistent_file
test test_uu_df_nonexistent_file { |ctx|
  let s = uu.scene(ctx)?
  let missing = uu.invoke(s, "df", ["does-not-exist"])?
  uu.fails(missing)
  uu.stderr_only(missing, "df: does-not-exist: No such file or directory\n")
  let partial = uu.invoke(s, "df", ["--output=file", "does-not-exist", "."])?
  uu.fails(partial)
  uu.stderr_is(partial, "df: does-not-exist: No such file or directory\n")
  uu.stdout_is(partial, "File\n.\n")
}

# origin: uutils test_df::test_df_all_shows_binfmt_misc
test test_uu_df_df_all_shows_binfmt_misc { |ctx|
  let s = uu.scene(ctx)?
  let mounted = match p"/proc/self/mountinfo".read_text() {
    Ok(content) => "binfmt_misc" in content,
    Err(_) => false,
  }
  if mounted {
  let r = uu.invoke(s, "df", ["--all", "--output=fstype,target"])?
  uu.succeeds(r)
  assert "binfmt_misc" in r.stdout.utf8()?
  }
}

# origin: uutils test_df::test_df_hides_binfmt_misc_by_default
test test_uu_df_df_hides_binfmt_misc_by_default { |ctx|
  let s = uu.scene(ctx)?
  let mounted = match p"/proc/self/mountinfo".read_text() {
    Ok(content) => "binfmt_misc" in content,
    Err(_) => false,
  }
  if mounted {
  let r = uu.invoke(s, "df", ["--output=fstype,target"])?
  uu.succeeds(r)
  assert ! ("binfmt_misc" in r.stdout.utf8()?)
  }
}

# origin: uutils test_df::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_df_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["-B", "1fb"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "df: invalid suffix in -B argument '1fb'\n")
}

# origin: uutils test_df::test_df_masked_proc_fallback
test test_uu_df_df_masked_proc_fallback { |ctx|
  let s = uu.scene(ctx)?
  match process.which("unshare") { Err(_) => return, Ok(_) => {} }
  let probe = process.run(process.command_argv("unshare", ["unshare", "-rm", "true"], s.root, {}, b"", p"/dev/null", p"/dev/null", timeout: 10s))?
  if ! probe.exited_with(0) { return }
  let measured = masked_proc(s, ["."])?
  uu.succeeds(measured)
  uu.stderr_contains(measured, "cannot read table of mounted file systems")
  uu.stdout_contains(measured, "Filesystem")
  uu.fails(masked_proc(s, [])?)
  for args in [["-a", "."], ["-l", "."], ["-t", "ext4", "."], ["-x", "tmpfs", "."]] {
    uu.fails(masked_proc(s, args)?)
  }
  for args in [["-i", "."], ["-T", "."], ["--total", "."]] {
    uu.succeeds(masked_proc(s, args)?)
  }
}
