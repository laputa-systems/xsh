##! Native ports of the uutils touch integration tests.

use support.uu as uu

# The read-only descriptor must remain fd 1 across exec; a writable capture
# would miss the contract for an existing file opened without write permission.
proc readonly_stdout_touch(s: uu.Scene, file: Str) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let wrapper = uu.at(s, ".readonly-stdout.xsh")
  wrapper.write(r"""proc main(root: Str, file: Str, ...words: List[Str]) [fs, process, env, error] {
  unix.redirect_fd(1, Path(file))?
  let cwd = Path(root)
  cd $cwd { unix.exec(process.command_argv(words[0], words))? }
}
""")?
  let argv = uu.argv(s, "touch", [p"-"])?
  let out = uu.at(s, ".wrapper-out")
  let err = uu.at(s, ".wrapper-err")
  let plan = process.command_argv(s.ctx.xsh_bin,
    [s.ctx.xsh_bin, wrapper, s.root, uu.at(s, file)].extend(argv), s.root,
    {LC_ALL: "C", TZ: "UTC"}, b"", out, err, timeout: 5s)
  let status = process.run(plan)?
  Ok({util: "touch", args: ["-"], status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: uutils test_touch::test_invalid_arg
test test_uu_touch_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "touch", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_touch::test_no_dereference_no_file
test test_uu_touch_no_dereference_no_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-h", "not-a-file"])?
  uu.fails(r)
  uu.stderr_contains(r, "setting times of 'not-a-file': No such file or directory")
  let r2 = uu.invoke(s, "touch", ["-h", "not-a-file-1", "not-a-file-2"])?
  uu.fails(r2)
  for file in ["not-a-file-1", "not-a-file-2"] { uu.stderr_contains(r2, f"setting times of '{file}': No such file or directory") }
}

# origin: uutils test_touch::test_obsolete_posix_format
test test_uu_touch_obsolete_posix_format { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["01010000", "11111111"], vars: {_POSIX2_VERSION: "199209", POSIXLY_CORRECT: "1"})?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "11111111")?
  assert ! uu.exists(s, "01010000")?
}

# origin: uutils test_touch::test_obsolete_posix_format_with_year
test test_uu_touch_obsolete_posix_format_with_year { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["0101000090", "11111111"], vars: {_POSIX2_VERSION: "199209", POSIXLY_CORRECT: "1"})?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "11111111")?
  assert ! uu.exists(s, "0101000090")?
}

# origin: uutils test_touch::test_touch_2_digit_years_2038
test test_uu_touch_touch_2_digit_years_2038 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "3801010000", "test_touch_set_two_digit_68_time"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "test_touch_set_two_digit_68_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_two_digit_68_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 2145916800000000000
  assert meta.mtime_ns == 2145916800000000000
}

# origin: uutils test_touch::test_touch_2_digit_years_68
test test_uu_touch_touch_2_digit_years_68 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "6801010000", "test_touch_set_two_digit_68_time"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "test_touch_set_two_digit_68_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_two_digit_68_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 3092601600000000000
  assert meta.mtime_ns == 3092601600000000000
}

# origin: uutils test_touch::test_touch_2_digit_years_69
test test_uu_touch_touch_2_digit_years_69 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "6901010000", "test_touch_set_two_digit_69_time"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "test_touch_set_two_digit_69_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_two_digit_69_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == -31536000000000000
  assert meta.mtime_ns == -31536000000000000
}

# origin: uutils test_touch::test_touch_changes_time_of_file_in_stdout
test test_uu_touch_touch_changes_time_of_file_in_stdout { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_touch_changes_time_of_file_in_stdout")?
  assert uu.file_exists(s, "test_touch_changes_time_of_file_in_stdout")?
  let before = fs.stat(uu.at(s, "test_touch_changes_time_of_file_in_stdout"))?.mtime_ns
  let r = uu.invoke(s, "touch", ["-"], stdout: uu.at(s, "test_touch_changes_time_of_file_in_stdout"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "test_touch_changes_time_of_file_in_stdout"))?.mtime_ns != before
}

# origin: uutils test_touch::test_touch_dash
test test_uu_touch_touch_dash { |ctx|
  let s = uu.scene(ctx)?
  let argv = uu.argv(s, "touch", [p"-h", p"-"])?
  let result = test.run_xsh(ctx, r"""let root = Path(args[0]); cd $root { unix.exec(process.command_argv(args[1], args[1..]))? }""", [], [word.display() for word in [s.root].extend(argv)], env: {LC_ALL: "C", TZ: "UTC"})?
  assert result.success, result.stderr
  assert result.stdout == "" and result.stderr == ""
}

# origin: uutils test_touch::test_touch_dash_updates_stdout_file
test test_uu_touch_touch_dash_updates_stdout_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "c")?
  fs.set_times(uu.at(s, "c"), atime_ns: 1000000000000000, mtime_ns: 1000000000000000)?
  let r = readonly_stdout_touch(s, "c")?
  uu.succeeds(r)
  uu.touch(s, ".clock")?
  fs.set_times(uu.at(s, ".clock"), mtime_now: true)?
  let age = fs.stat(uu.at(s, ".clock"))?.mtime_ns - fs.stat(uu.at(s, "c"))?.mtime_ns
  assert age >= 0
  assert age / 1000000000 < 60
}

# origin: uutils test_touch::test_touch_default
test test_uu_touch_touch_default { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["test_touch_default_file"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_default_file")?
}

# origin: uutils test_touch::test_touch_device_files
test test_uu_touch_touch_device_files { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["/dev/null", "/dev/zero", "/dev/full", "/dev/random"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_touch::test_touch_does_not_truncate_symlink_target
test test_uu_touch_touch_does_not_truncate_symlink_target { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "victim", "do not truncate me")?
  uu.at(s, "link").symlink(to: uu.at(s, "victim"))?
  let r = uu.invoke(s, "touch", ["link"])?
  uu.succeeds(r)
  uu.file_is(s, "victim", "do not truncate me")
}

# origin: uutils test_touch::test_touch_f_option
test test_uu_touch_touch_f_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-f", "test_f_option.txt"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "test_f_option.txt")?
  uu.remove(s, "test_f_option.txt")?
}

# origin: uutils test_touch::test_touch_fifo
test test_uu_touch_touch_fifo { |ctx|
  let s = uu.scene(ctx)?
  fs.mkfifo(uu.at(s, "fifo"), 0o600)?
  let r = uu.invoke(s, "touch", ["-d", "2020-01-01 00:00:00", "fifo"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "fifo"))?.kind == "fifo"
}

# origin: uutils test_touch::test_touch_invalid_date_format
test test_uu_touch_touch_invalid_date_format { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-m", "-t", "+1000000000000 years", "test_touch_invalid_date_format"])?
  uu.fails(r)
  uu.stderr_contains(r, "touch: invalid date format '+1000000000000 years'")
}

# origin: uutils test_touch::test_touch_invalid_timestamp_reports_original_input
test test_uu_touch_touch_invalid_timestamp_reports_original_input { |ctx|
  let s = uu.scene(ctx)?
  for stamp in ["2026-04-10", "26-04-10", "00000000"] {
    let r = uu.invoke(s, "touch", ["-t", stamp, "f"])?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, f"touch: invalid date format '{stamp}'\n")
  }
}

# origin: uutils test_touch::test_touch_leap_second
test test_uu_touch_touch_leap_second { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "197001010000.60", "test_touch_leap_sec"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_leap_sec")?
  let meta = fs.stat(uu.at(s, "test_touch_leap_sec"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns / 1000000000 == 60
  assert meta.mtime_ns / 1000000000 == 60
}

# origin: uutils test_touch::test_touch_mtime_dst_fails
test test_uu_touch_touch_mtime_dst_fails { |ctx|
  let s = uu.scene(ctx)?
  uu.fails(uu.invoke(s, "touch", ["-m", "-t", "202003080200", "test_touch_set_mtime_dst_fails"], vars: {TZ: "EST+5EDT,M3.2.0/2,M11.1.0/2"})?)
}

# origin: uutils test_touch::test_touch_mtime_dst_succeeds
test test_uu_touch_touch_mtime_dst_succeeds { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-m", "-t", "202103140300", "test_touch_set_mtime_dst_succeeds"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_mtime_dst_succeeds")?
  assert fs.stat(uu.at(s, "test_touch_set_mtime_dst_succeeds"))?.mtime_ns == 1615690800000000000
}

# origin: uutils test_touch::test_touch_no_args
test test_uu_touch_touch_no_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", [])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_is(r, "touch: missing file operand\nTry 'touch --help' for more information.\n")
}

# origin: uutils test_touch::test_touch_no_create_file_absent
test test_uu_touch_touch_no_create_file_absent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-c", "test_touch_no_create_file_absent"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! uu.exists(s, "test_touch_no_create_file_absent")?
}

# origin: uutils test_touch::test_touch_no_create_file_exists
test test_uu_touch_touch_no_create_file_exists { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_touch_no_create_file_exists")?
  assert uu.file_exists(s, "test_touch_no_create_file_exists")?
  let r = uu.invoke(s, "touch", ["-c", "test_touch_no_create_file_exists"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_no_create_file_exists")?
}

# origin: uutils test_touch::test_touch_no_dereference
test test_uu_touch_touch_no_dereference { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_touch_no_dereference_a")?
  fs.set_times(uu.at(s, "test_touch_no_dereference_a"), atime_ns: 1420070400000000000, mtime_ns: 1420070400000000000)?
  uu.at(s, "test_touch_no_dereference_b").symlink(to: uu.at(s, "test_touch_no_dereference_a"))?
  assert uu.file_exists(s, "test_touch_no_dereference_a")?
  assert uu.is_symlink(s, "test_touch_no_dereference_b")?
  let r = uu.invoke(s, "touch", ["-t", "201512312359", "-h", "test_touch_no_dereference_b"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let link = fs.stat(uu.at(s, "test_touch_no_dereference_b"), follow_symlinks: false)?
  assert link.atime_ns == link.mtime_ns
  assert link.atime_ns == 1451606340000000000
  assert link.mtime_ns == 1451606340000000000
  assert uu.file_exists(s, "test_touch_no_dereference_a")?
  let meta = fs.stat(uu.at(s, "test_touch_no_dereference_a"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 1420070400000000000
  assert meta.mtime_ns == 1420070400000000000
}

# origin: uutils test_touch::test_touch_no_dereference_dangling
test test_uu_touch_touch_no_dereference_dangling { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "nowhere", "dangling")?
  let r = uu.invoke(s, "touch", ["-h", "dangling"])?
  uu.succeeds(r)
}

# origin: uutils test_touch::test_touch_no_dereference_ref_dangling
test test_uu_touch_touch_no_dereference_ref_dangling { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, "nowhere", "dangling")?
  let r = uu.invoke(s, "touch", ["-h", "-r", "dangling", "file"])?
  uu.succeeds(r)
}

# origin: uutils test_touch::test_touch_no_such_file_error_msg
test test_uu_touch_touch_no_such_file_error_msg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["nonexistent/file"])?
  uu.fails(r)
  uu.stderr_only(r, "touch: cannot touch 'nonexistent/file': No such file or directory\n")
}

# origin: uutils test_touch::test_touch_non_utf8_paths
test test_uu_touch_touch_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let file = uu.at_bytes(s, b"test_\xff\xfe.txt")?
  let r = uu.invoke_paths(s, "touch", [Path.parse_bytes(b"test_\xff\xfe.txt")?])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(file) is Ok(_)
}

# origin: uutils test_touch::test_touch_permission_denied_error_msg
test test_uu_touch_touch_permission_denied_error_msg { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir_with_read_only_access")?
  uu.set_mode(s, "dir_with_read_only_access", 0o555)?
  let file = uu.at(s, "dir_with_read_only_access/file")
  let r = uu.invoke(s, "touch", [file.display()])?
  uu.fails(r)
  uu.stderr_only(r, f"touch: cannot touch '{file}': Permission denied\n")
}

# origin: uutils test_touch::test_touch_reference
test test_uu_touch_touch_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_touch_reference_a")?
  fs.set_times(uu.at(s, "test_touch_reference_a"), atime_ns: 1420070400000000000, mtime_ns: 1420070400000000000)?
  assert uu.file_exists(s, "test_touch_reference_a")?
  for option in ["-r", "--ref", "--reference"] {
    let r = uu.invoke(s, "touch", [option, "test_touch_reference_a", "test_touch_reference_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert uu.file_exists(s, "test_touch_reference_b")?
    let meta = fs.stat(uu.at(s, "test_touch_reference_b"))?
    assert meta.atime_ns == meta.mtime_ns
    assert meta.atime_ns == 1420070400000000000
    assert meta.mtime_ns == 1420070400000000000
    uu.remove(s, "test_touch_reference_b")?
  }
}

# origin: uutils test_touch::test_touch_reference_dangling
test test_uu_touch_touch_reference_dangling { |ctx|
  let s = uu.scene(ctx)?
  uu.at(s, "test_touch_reference_dangling").symlink(to: uu.at(s, "nonexistent_target"))?
  let r = uu.invoke(s, "touch", ["--reference", uu.at(s, "test_touch_reference_dangling").display(), "some_file"])?
  uu.fails(r)
  uu.stderr_contains(r, "touch: failed to get attributes of")
}

# origin: uutils test_touch::test_touch_reference_symlink_with_no_deref
test test_uu_touch_touch_reference_symlink_with_no_deref { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo.txt")?
  uu.symlink(s, "foo.txt", "bar.txt")?
  fs.set_times(uu.at(s, "bar.txt"), atime_ns: 123000000000, mtime_ns: 123000000000, follow_symlinks: false)?
  uu.touch(s, "baz.txt")?
  let r = uu.invoke(s, "touch", ["--reference", "bar.txt", "--no-dereference", "baz.txt"])?
  uu.succeeds(r)
  let meta = fs.stat(uu.at(s, "baz.txt"), follow_symlinks: false)?
  assert meta.atime_ns == 123000000000 and meta.mtime_ns == 123000000000
}

# origin: uutils test_touch::test_touch_set_both
test test_uu_touch_touch_set_both { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "201501011234", "-a", "-m", "test_touch_set_both"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_both")?
  let meta = fs.stat(uu.at(s, "test_touch_set_both"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 1420115640000000000
  assert meta.mtime_ns == 1420115640000000000
}

# origin: uutils test_touch::test_touch_set_both_date_and_reference
test test_uu_touch_touch_set_both_date_and_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_touch_reference")?
  fs.set_times(uu.at(s, "test_touch_reference"), atime_ns: 1420115640000000000, mtime_ns: 1420115640000000000)?
  assert uu.file_exists(s, "test_touch_reference")?
  let r = uu.invoke(s, "touch", ["-d", "Thu Jan 01 12:34:00 2015", "-r", "test_touch_reference", "test_touch_set_both_date_and_reference"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_both_date_and_reference")?
  let meta = fs.stat(uu.at(s, "test_touch_set_both_date_and_reference"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 1420115640000000000
  assert meta.mtime_ns == 1420115640000000000
}

# origin: uutils test_touch::test_touch_set_both_offset_date_and_reference
test test_uu_touch_touch_set_both_offset_date_and_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_touch_reference")?
  fs.set_times(uu.at(s, "test_touch_reference"), atime_ns: 1420115640000000000, mtime_ns: 1420115640000000000)?
  assert uu.file_exists(s, "test_touch_reference")?
  let r = uu.invoke(s, "touch", ["-d", "+5 days", "-r", "test_touch_reference", "test_touch_set_both_date_and_reference"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_both_date_and_reference")?
  let meta = fs.stat(uu.at(s, "test_touch_set_both_date_and_reference"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 1420547640000000000
  assert meta.mtime_ns == 1420547640000000000
}

# origin: uutils test_touch::test_touch_set_both_time_and_date
test test_uu_touch_touch_set_both_time_and_date { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "2015010112342", "-d", "Thu Jan 01 12:34:00 2015", "test_touch_set_both_time_and_date"])?
  uu.fails(r)
}

# origin: uutils test_touch::test_touch_set_both_time_and_reference
test test_uu_touch_touch_set_both_time_and_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_touch_reference")?
  fs.set_times(uu.at(s, "test_touch_reference"), atime_ns: 1420070400000000000, mtime_ns: 1420070400000000000)?
  assert uu.file_exists(s, "test_touch_reference")?
  let r = uu.invoke(s, "touch", ["-t", "2015010112342", "-r", "test_touch_reference", "test_touch_set_both_time_and_reference"])?
  uu.fails(r)
}

# origin: uutils test_touch::test_touch_set_cymdhm_time
test test_uu_touch_touch_set_cymdhm_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "201501011234", "test_touch_set_cymdhm_time"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let start = 1420070400000000000
  assert uu.file_exists(s, "test_touch_set_cymdhm_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_cymdhm_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns / 1000000000 - start / 1000000000 == 45240
  assert meta.mtime_ns / 1000000000 - start / 1000000000 == 45240
}

# origin: uutils test_touch::test_touch_set_cymdhms_time
test test_uu_touch_touch_set_cymdhms_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "201501011234.56", "test_touch_set_cymdhms_time"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let start = 1420070400000000000
  assert uu.file_exists(s, "test_touch_set_cymdhms_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_cymdhms_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns / 1000000000 - start / 1000000000 == 45296
  assert meta.mtime_ns / 1000000000 - start / 1000000000 == 45296
}

# origin: uutils test_touch::test_touch_set_date
test test_uu_touch_touch_set_date { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "Thu Jan 01 12:34:00 2015", "test_touch_set_date"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_date")?
  let meta = fs.stat(uu.at(s, "test_touch_set_date"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 1420115640000000000
  assert meta.mtime_ns == 1420115640000000000
}

# origin: uutils test_touch::test_touch_set_date2
test test_uu_touch_touch_set_date2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "2000-01-23", "test_touch_set_date"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_date")?
  let meta = fs.stat(uu.at(s, "test_touch_set_date"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 948585600000000000
  assert meta.mtime_ns == 948585600000000000
}

# origin: uutils test_touch::test_touch_set_date3
test test_uu_touch_touch_set_date3 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "@1623786360", "test_touch_set_date"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_date")?
  let meta = fs.stat(uu.at(s, "test_touch_set_date"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 1623786360000000000
  assert meta.mtime_ns == 1623786360000000000
}

# origin: uutils test_touch::test_touch_set_date4
test test_uu_touch_touch_set_date4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "1970-01-01 18:43:33", "test_touch_set_date"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_date")?
  let meta = fs.stat(uu.at(s, "test_touch_set_date"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 67413000000000
  assert meta.mtime_ns == 67413000000000
}

# origin: uutils test_touch::test_touch_set_date5
test test_uu_touch_touch_set_date5 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "1970-01-01 18:43:33.023456789", "test_touch_set_date"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_date")?
  let meta = fs.stat(uu.at(s, "test_touch_set_date"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 67413023456789
  assert meta.mtime_ns == 67413023456789
}

# origin: uutils test_touch::test_touch_set_date6
test test_uu_touch_touch_set_date6 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "2000-01-01 00:00", "test_touch_set_date"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_date")?
  let meta = fs.stat(uu.at(s, "test_touch_set_date"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 946684800000000000
  assert meta.mtime_ns == 946684800000000000
}

# origin: uutils test_touch::test_touch_set_date7
test test_uu_touch_touch_set_date7 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "2004-01-16 12:00 +0000", "test_touch_set_date"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_set_date")?
  let meta = fs.stat(uu.at(s, "test_touch_set_date"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns == 1074254400000000000
  assert meta.mtime_ns == 1074254400000000000
}

# origin: uutils test_touch::test_touch_set_date_relative_smoke
test test_uu_touch_touch_set_date_relative_smoke { |ctx|
  let s = uu.scene(ctx)?
  for date in ["-1 fortnight", "+1 fortnight", "-1 fortnights", "+1 fortnights", "fortnight", "fortnights", "-1 week", "+1 week", "-1 weeks", "+1 weeks", "week", "weeks", "-1 day", "+1 day", "-1 days", "+1 days", "day", "days", "-1 hour", "+1 hour", "-1 hours", "+1 hours", "hour", "hours", "-1 minute", "+1 minute", "-1 minutes", "+1 minutes", "minute", "minutes", "-1 min", "+1 min", "-1 mins", "+1 mins", "min", "mins", "-1 second", "+1 second", "-1 seconds", "+1 seconds", "second", "seconds", "-1 sec", "+1 sec", "-1 secs", "+1 secs", "sec", "secs", "yesterday", "tomorrow", "now", "2 seconds", "2 years 1 week", "2 days ago", "2 months 1 second", "a"] {
    uu.touch(s, "f")?
    let r = uu.invoke(s, "touch", ["-d", date, "f"])?
    uu.succeeds(r)
    uu.no_output(r)
    uu.remove(s, "f")?
  }
}

# origin: uutils test_touch::test_touch_set_date_without_leading_zeroes
test test_uu_touch_touch_set_date_without_leading_zeroes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "2026-6-27 13:12", "test_touch_set_date_without_leading_zeroes"], vars: {TZ: "UTC-8"})?
  uu.succeeds(r)
  uu.no_stderr(r)
  let meta = fs.stat(uu.at(s, "test_touch_set_date_without_leading_zeroes"))?
  assert meta.atime_ns == 1782537120000000000
  assert meta.mtime_ns == 1782537120000000000
}

# origin: uutils test_touch::test_touch_set_date_year_zero
test test_uu_touch_touch_set_date_year_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-d", "0000-01-01", "test_touch_year_zero"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_touch_year_zero")?
}

# origin: uutils test_touch::test_touch_set_mdhm_time
test test_uu_touch_touch_set_mdhm_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "01011234", "test_touch_set_mdhm_time"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let start = time.from_calendar(time.to_calendar(time.now() * 1000000, utc: true)?.year, 1, 1, utc: true)?
  assert uu.file_exists(s, "test_touch_set_mdhm_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_mdhm_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns / 1000000000 - start / 1000000000 == 45240
  assert meta.mtime_ns / 1000000000 - start / 1000000000 == 45240
}

# origin: uutils test_touch::test_touch_set_mdhms_time
test test_uu_touch_touch_set_mdhms_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "01011234.56", "test_touch_set_mdhms_time"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let start = time.from_calendar(time.to_calendar(time.now() * 1000000, utc: true)?.year, 1, 1, utc: true)?
  assert uu.file_exists(s, "test_touch_set_mdhms_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_mdhms_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns / 1000000000 - start / 1000000000 == 45296
  assert meta.mtime_ns / 1000000000 - start / 1000000000 == 45296
}

# origin: uutils test_touch::test_touch_set_only_atime
test test_uu_touch_touch_set_only_atime { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-a", "--time=access", "--time=atime", "--time=atim", "--time=a", "--time=use"] {
    let s = uu.scene(ctx)?
    let file = "test_touch_set_only_atime"
    let r = uu.invoke(s, "touch", ["-t", "201501011234", option, file])?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert uu.file_exists(s, file)?
    let meta = fs.stat(uu.at(s, file))?
    assert meta.atime_ns != meta.mtime_ns
    assert meta.atime_ns / 1000000000 - 1420070400 == 45240
    uu.remove(s, file)?
  }
}

# origin: uutils test_touch::test_touch_set_only_mtime
test test_uu_touch_touch_set_only_mtime { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-m", "--time=modify", "--time=mtime", "--time=m"] {
    let s = uu.scene(ctx)?
    let file = "test_touch_set_only_mtime"
    let r = uu.invoke(s, "touch", ["-t", "201501011234", option, file])?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert uu.file_exists(s, file)?
    let meta = fs.stat(uu.at(s, file))?
    assert meta.atime_ns != meta.mtime_ns
    assert meta.mtime_ns / 1000000000 - 1420070400 == 45240
    uu.remove(s, file)?
  }
}

# origin: uutils test_touch::test_touch_set_only_mtime_failed
test test_uu_touch_touch_set_only_mtime_failed { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "2015010112342", "-m", "test_touch_set_only_mtime"])?
  uu.fails(r)
}

# origin: uutils test_touch::test_touch_set_time_on_unreadable_unwritable_file
test test_uu_touch_touch_set_time_on_unreadable_unwritable_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "no_rights")?
  uu.set_mode(s, "no_rights", 0o000)?
  let r = uu.invoke(s, "touch", ["-d", "2000-01-03 00:00", "-c", "no_rights"])?
  uu.succeeds(r)
  uu.no_output(r)
  uu.set_mode(s, "no_rights", 0o644)?
  let meta = fs.stat(uu.at(s, "no_rights"))?
  assert meta.atime_ns == 946857600000000000
  assert meta.mtime_ns == 946857600000000000
}

# origin: uutils test_touch::test_touch_set_ymdhm_time
test test_uu_touch_touch_set_ymdhm_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "1501011234", "test_touch_set_ymdhm_time"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let start = 1420070400000000000
  assert uu.file_exists(s, "test_touch_set_ymdhm_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_ymdhm_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns / 1000000000 - start / 1000000000 == 45240
  assert meta.mtime_ns / 1000000000 - start / 1000000000 == 45240
}

# origin: uutils test_touch::test_touch_set_ymdhms_time
test test_uu_touch_touch_set_ymdhms_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["-t", "1501011234.56", "test_touch_set_ymdhms_time"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let start = 1420070400000000000
  assert uu.file_exists(s, "test_touch_set_ymdhms_time")?
  let meta = fs.stat(uu.at(s, "test_touch_set_ymdhms_time"))?
  assert meta.atime_ns == meta.mtime_ns
  assert meta.atime_ns / 1000000000 - start / 1000000000 == 45296
  assert meta.mtime_ns / 1000000000 - start / 1000000000 == 45296
}

# origin: uutils test_touch::test_touch_symlink_with_no_deref
test test_uu_touch_touch_symlink_with_no_deref { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo.txt")?
  let before = fs.stat(uu.at(s, "foo.txt"))?
  uu.symlink(s, "foo.txt", "bar.txt")?
  fs.set_times(uu.at(s, "bar.txt"), atime_ns: 123000000000, mtime_ns: 123000000000, follow_symlinks: false)?
  let r = uu.invoke(s, "touch", ["-a", "--no-dereference", "-d", "@456", "bar.txt"])?
  uu.succeeds(r)
  let link = fs.stat(uu.at(s, "bar.txt"), follow_symlinks: false)?
  assert link.atime_ns == 456000000000 and link.mtime_ns == 123000000000
  let after = fs.stat(uu.at(s, "foo.txt"))?
  assert before.atime_ns == after.atime_ns and before.mtime_ns == after.mtime_ns
}

# origin: uutils test_touch::test_touch_system_fails
test test_uu_touch_touch_system_fails { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["/"])?
  uu.fails(r)
  uu.stderr_contains(r, "setting times of '/'")
}

# origin: uutils test_touch::test_touch_through_dangling_symlink_creates_target
test test_uu_touch_touch_through_dangling_symlink_creates_target { |ctx|
  let s = uu.scene(ctx)?
  uu.at(s, "link").symlink(to: uu.at(s, "missing"))?
  let r = uu.invoke(s, "touch", ["link"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "missing")?
  uu.file_is(s, "missing", "")
}

# origin: uutils test_touch::test_touch_trailing_slash
test test_uu_touch_touch_trailing_slash { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "touch", ["no-file/"])?
  uu.fails(r)
  uu.stderr_only(r, "touch: setting times of 'no-file/': No such file or directory\n")
}

# origin: uutils test_touch::test_touch_trailing_slash_no_create
test test_uu_touch_touch_trailing_slash_no_create { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.fails_with_code(uu.invoke(s, "touch", ["-c", "file/"])?, 1)
  uu.succeeds(uu.invoke(s, "touch", ["-c", "no-file/"])?)
  assert ! uu.exists(s, "no-file")?
  uu.symlink(s, "nowhere", "dangling")?
  uu.succeeds(uu.invoke(s, "touch", ["-c", "dangling/"])?)
  assert ! uu.exists(s, "nowhere")?
  assert uu.is_symlink(s, "dangling")?
  uu.symlink(s, "loop", "loop")?
  uu.fails_with_code(uu.invoke(s, "touch", ["-c", "loop/"])?, 1)
  let loop_metadata = fs.stat(uu.at(s, "loop"), follow_symlinks: true)
  assert loop_metadata is Err(_)
  if let Err(failure) = loop_metadata { assert failure.errno == 40 }
  uu.touch(s, "file2")?
  uu.symlink(s, "file2", "link1")?
  uu.fails_with_code(uu.invoke(s, "touch", ["-c", "link1/"])?, 1)
  assert uu.file_exists(s, "file2")? and uu.is_symlink(s, "link1")?
  uu.mkdir(s, "dir")?
  uu.succeeds(uu.invoke(s, "touch", ["-c", "dir/"])?)
  uu.mkdir(s, "dir2")?
  uu.symlink(s, "dir2", "link2")?
  uu.succeeds(uu.invoke(s, "touch", ["-c", "link2/"])?)
}
