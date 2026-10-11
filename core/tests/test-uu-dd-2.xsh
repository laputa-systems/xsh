##! Native ports of the uutils dd integration tests.
use support.uu as uu

proc fixture(s: uu.Scene, name: Str) [fs, error] -> Result[Bytes, Error] {
  Ok(fp"{s.ctx.core_dir}/tests/data/uutils/dd/{name}".read_bytes()?)
}

proc ascii_block(size: Int) [error] -> Result[Bytes, Error] {
  bytes.from_ints([i % 128 for i in range(size)])
}

# FIFO writers keep one open descriptor so each delayed write remains a short record.
proc fifo_copy(s: uu.Scene, args: List[Str], data: Bytes, width: Int, pause: Duration) [fs, process, env, time, error] -> Result[uu.Ran, Error] {
  uu.mkfifo(s, "fifo")?
  let plan = uu.command(s, "dd", args, timeout: 15s)?
  let child = spawn plan?
  let fd = unix.open_fd(uu.at(s, "fifo"), write: true, nonblock: false)?
  var offset = 0
  while offset < data.len() {
    let end = if offset + width < data.len() { offset + width } else { data.len() }
    let part = data[offset..end]
    assert unix.write_fd(fd, part)? == part.len()
    offset = end
    time.sleep(pause)?
  }
  unix.close_fd(fd)?
  let status = process.wait_any([child])?.status
  Ok({util: "dd", args: args, status: status.exit_code()?, stdout: uu.read(s, ".uu-stdout")?, stderr: uu.read(s, ".uu-stderr")?})
}

# origin: uutils test_dd::test_lower_block
test test_uu_dd_lower_block { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=lcase,block", "cbs=8"], fixture(s, "dd-block8-lowercase.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "dd-block8-lowercase.spec")?)
}

# origin: uutils test_dd::test_no_dropped_writes
test test_uu_dd_no_dropped_writes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["if=/dev/urandom", "bs=16384", "count=1000"], timeout: 15s)?
  uu.succeeds(r)
  assert r.stdout.len() == 16384000
  uu.stderr_contains(r, "16384000 bytes")
}

# origin: uutils test_dd::test_noatime_does_not_update_infile_atime
test test_uu_dd_noatime_does_not_update_infile_atime { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "this-ifile-exists-noatime.txt", "this-ifile-exists-noatime.txt")?
  let before = fs.stat(uu.at(s, "this-ifile-exists-noatime.txt"))?.atime_ns
  let r = uu.invoke(s, "dd", ["status=none", "iflag=noatime", "if=this-ifile-exists-noatime.txt"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "this-ifile-exists-noatime.txt"))?.atime_ns == before
}

# origin: uutils test_dd::test_noatime_does_not_update_ofile_atime
test test_uu_dd_noatime_does_not_update_ofile_atime { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "this-ofile-exists-noatime.txt", "this-ofile-exists-noatime.txt")?
  let before = fs.stat(uu.at(s, "this-ofile-exists-noatime.txt"))?.atime_ns
  let r = uu.invoke(s, "dd", ["status=none", "oflag=noatime", "of=this-ofile-exists-noatime.txt"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "this-ofile-exists-noatime.txt"))?.atime_ns == before
}

# origin: uutils test_dd::test_nocache_eof
test test_uu_dd_nocache_eof { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "in.f", bytes.zero(1234567)?)?
  let r = uu.invoke(s, "dd", ["if=in.f", "of=out.f", "bs=1M", "oflag=nocache,sync", "status=noxfer"])?
  uu.succeeds(r)
  assert uu.read(s, "out.f")?.len() == 1234567
}

# origin: uutils test_dd::test_nocache_file
test test_uu_dd_nocache_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", ["a" for _ in range(1048576)].join(""))?
  let r = uu.invoke(s, "dd", ["if=f", "of=/dev/null", "iflag=nocache", "status=noxfer"])?
  uu.succeeds(r)
  uu.stderr_only(r, "2048+0 records in\n2048+0 records out\n")
}

# origin: uutils test_dd::test_nocache_stdin_error
test test_uu_dd_nocache_stdin_error { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["iflag=nocache", "count=0", "status=noxfer"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "dd: failed to discard cache for: 'standard input': Illegal seek\n0+0 records in\n0+0 records out\n")
}

# origin: uutils test_dd::test_nocreat_causes_failure_when_outfile_not_present
test test_uu_dd_nocreat_causes_failure_when_outfile_not_present { |ctx|
  let s = uu.scene(ctx)?
  assert ! uu.exists(s, "this-file-does-not-exist.txt")?
  let r = uu.invoke(s, "dd", ["conv=nocreat", "of=this-file-does-not-exist.txt"])?
  uu.fails(r)
  uu.stderr_only(r, "dd: failed to open 'this-file-does-not-exist.txt': No such file or directory\n")
  assert ! uu.exists(s, "this-file-does-not-exist.txt")?
}

# origin: uutils test_dd::test_notrunc_does_not_truncate
test test_uu_dd_notrunc_does_not_truncate { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "this-file-exists-notrunc.txt", ascii_block(256)?)?
  uu.fixture(s, "dd", "null.txt", "null.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "conv=notrunc", "of=this-file-exists-notrunc.txt", "if=null.txt"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.size(s, "this-file-exists-notrunc.txt")? == 256
}

# origin: uutils test_dd::test_null_fullblock
test test_uu_dd_null_fullblock { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "null.txt", "null.txt")?
  let r = uu.invoke(s, "dd", ["if=null.txt", "status=none", "iflag=fullblock"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_dd::test_null_stats
test test_uu_dd_null_stats { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "null.txt", "null.txt")?
  let r = uu.invoke(s, "dd", ["if=null.txt"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stderr_contains(r, "0+0 records in\n0+0 records out\n0 bytes copied, ")
  uu.stderr_contains(r, "0.0 kB/s")
  assert rx"\d(\.\d+)?(e-\d\d)? s, ".matches(r.stderr.utf8()?)
}

# origin: uutils test_dd::test_oflag_direct_partial_block
test test_uu_dd_oflag_direct_partial_block { |ctx|
  let s = uu.scene(ctx)?
  let data = bytes.from_ints([66 for _ in range(25087)])?
  uu.write_bytes(s, "test_direct_input.iso", data)?
  let r = uu.invoke(s, "dd", [f"if={uu.at(s, "test_direct_input.iso")}", f"of={uu.at(s, "test_direct_output.img")}", "oflag=direct", "bs=8192", "status=none"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.exists(s, "test_direct_output.img")?
  assert uu.size(s, "test_direct_output.img")? == 25087
  assert uu.read(s, "test_direct_output.img")? == data
  uu.remove(s, "test_direct_input.iso")?
  uu.remove(s, "test_direct_output.img")?
}

# origin: uutils test_dd::test_out_of_memory
test test_uu_dd_out_of_memory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bs=1PB"], b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "memory")
}

# origin: uutils test_dd::test_out_of_memory_skip
test test_uu_dd_out_of_memory_skip { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bs=1PB"], b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "memory")
}

# origin: uutils test_dd::test_outfile_dev_null
test test_uu_dd_outfile_dev_null { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["of=/dev/null"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.no_stdout(r)
}

# origin: uutils test_dd::test_partial_records_out
test test_uu_dd_partial_records_out { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bs=2", "status=noxfer"], b"abc", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is(r, "abc")
  uu.stderr_is(r, "1+1 records in\n1+1 records out\n")
}

# origin: uutils test_dd::test_random_73k_test_bs_prime_ibs_gt_obs_sync
test test_uu_dd_random_73k_test_bs_prime_ibs_gt_obs_sync { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "random-5828891cb1230748e146f34223bbd3b5.test", "random-5828891cb1230748e146f34223bbd3b5.test")?
  let r = uu.invoke(s, "dd", ["conv=sync", "ibs=1031", "obs=521", "if=random-5828891cb1230748e146f34223bbd3b5.test"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "gnudd-conv-sync-ibs-1031-obs-521-random.spec")?)
}

# origin: uutils test_dd::test_random_73k_test_bs_prime_obs_gt_ibs_sync
test test_uu_dd_random_73k_test_bs_prime_obs_gt_ibs_sync { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "random-5828891cb1230748e146f34223bbd3b5.test", "random-5828891cb1230748e146f34223bbd3b5.test")?
  let r = uu.invoke(s, "dd", ["conv=sync", "ibs=521", "obs=1031", "if=random-5828891cb1230748e146f34223bbd3b5.test"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "gnudd-conv-sync-ibs-521-obs-1031-random.spec")?)
}

# origin: uutils test_dd::test_random_73k_test_count_bytes
test test_uu_dd_random_73k_test_count_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "random-5828891cb1230748e146f34223bbd3b5.test", "random-5828891cb1230748e146f34223bbd3b5.test")?
  let r = uu.invoke(s, "dd", ["ibs=521", "obs=1031", "count=32x1024", "iflag=count_bytes", "if=random-5828891cb1230748e146f34223bbd3b5.test"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "gnudd-random-first-32k.spec")?)
}

# origin: uutils test_dd::test_random_73k_test_count_reads
test test_uu_dd_random_73k_test_count_reads { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "random-5828891cb1230748e146f34223bbd3b5.test", "random-5828891cb1230748e146f34223bbd3b5.test")?
  let r = uu.invoke(s, "dd", ["bs=1024", "count=32", "if=random-5828891cb1230748e146f34223bbd3b5.test"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "gnudd-random-first-32k.spec")?)
}

# origin: uutils test_dd::test_random_73k_test_lazy_fullblock
test test_uu_dd_random_73k_test_lazy_fullblock { |ctx|
  let s = uu.scene(ctx)?
  let data = fixture(s, "random-5828891cb1230748e146f34223bbd3b5.test")?
  let r = fifo_copy(s, ["ibs=521", "obs=1031", "iflag=fullblock", "if=fifo", "status=noxfer"], data, 260, 10ms)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, data)
  uu.stderr_is(r, "142+1 records in\n72+1 records out\n")
}

# origin: uutils test_dd::test_random_73k_test_not_a_multiple_obs_gt_ibs
test test_uu_dd_random_73k_test_not_a_multiple_obs_gt_ibs { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "random-5828891cb1230748e146f34223bbd3b5.test", "random-5828891cb1230748e146f34223bbd3b5.test")?
  let r = uu.invoke(s, "dd", ["ibs=521", "obs=1031", "if=random-5828891cb1230748e146f34223bbd3b5.test"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "random-5828891cb1230748e146f34223bbd3b5.test")?)
}

# origin: uutils test_dd::test_random_73k_test_obs_lt_not_a_multiple_ibs
test test_uu_dd_random_73k_test_obs_lt_not_a_multiple_ibs { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "random-5828891cb1230748e146f34223bbd3b5.test", "random-5828891cb1230748e146f34223bbd3b5.test")?
  let r = uu.invoke(s, "dd", ["ibs=1031", "obs=521", "if=random-5828891cb1230748e146f34223bbd3b5.test"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "random-5828891cb1230748e146f34223bbd3b5.test")?)
}

# origin: uutils test_dd::test_reading_partial_blocks_from_fifo
test test_uu_dd_reading_partial_blocks_from_fifo { |ctx|
  let s = uu.scene(ctx)?
  let r = fifo_copy(s, ["ibs=3", "obs=3", "if=fifo"], b"abcd", 2, 100ms)?
  uu.stdout_is_bytes(r, b"abcd")
  assert r.stderr.utf8()?.starts_with("0+2 records in\n1+1 records out\n4 bytes copied")
}

# origin: uutils test_dd::test_reading_partial_blocks_from_fifo_gathered_into_larger_obs
test test_uu_dd_reading_partial_blocks_from_fifo_gathered_into_larger_obs { |ctx|
  let s = uu.scene(ctx)?
  let r = fifo_copy(s, ["ibs=3", "obs=6", "if=fifo"], b"abcd", 2, 100ms)?
  uu.stdout_is_bytes(r, b"abcd")
  assert r.stderr.utf8()?.starts_with("0+2 records in\n0+1 records out\n4 bytes copied")
}

# origin: uutils test_dd::test_reading_partial_blocks_from_fifo_unbuffered
test test_uu_dd_reading_partial_blocks_from_fifo_unbuffered { |ctx|
  let s = uu.scene(ctx)?
  let r = fifo_copy(s, ["bs=3", "ibs=1", "obs=1", "if=fifo"], b"abcd", 2, 100ms)?
  uu.stdout_is_bytes(r, b"abcd")
  assert r.stderr.utf8()?.starts_with("0+2 records in\n0+2 records out\n4 bytes copied")
}

# origin: uutils test_dd::test_seek_blocks_times_obs_overflow_does_not_wrap
test test_uu_dd_seek_blocks_times_obs_overflow_does_not_wrap { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "out.f", "0123456789abcdef")?
  let r = uu.invoke(s, "dd", ["if=/dev/null", "of=out.f", "seek=17592186044416", "obs=1048576", "conv=notrunc"])?
  uu.fails(r)
  uu.stderr_contains(r, "Value too large for defined data type")
  uu.file_is(s, "out.f", "0123456789abcdef")
}

# origin: uutils test_dd::test_seek_bytes
test test_uu_dd_seek_bytes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["seek=8", "oflag=seek_bytes"], b"abcdefghijklm\n", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is(r, "\0\0\0\0\0\0\0\0abcdefghijklm\n")
}

# origin: uutils test_dd::test_seek_do_not_overwrite
test test_uu_dd_seek_do_not_overwrite { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "outfile", "abc")?
  let r = uu.invoke(s, "dd", ["bs=1", "skip=1", "seek=1", "count=1", "status=noxfer", "of=outfile"], b"123")?
  uu.succeeds(r)
  uu.stderr_is(r, "1+0 records in\n1+0 records out\n")
  uu.no_stdout(r)
  uu.file_is(s, "outfile", "a2")
}

# origin: uutils test_dd::test_seek_output_fifo
test test_uu_dd_seek_output_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  let plan = uu.command(s, "dd", ["count=0", "seek=1", "of=fifo", "status=noxfer"], timeout: 15s)?
  let child = spawn plan?
  uu.at(s, "fifo").write(bytes.zero(512)?)?
  assert process.wait_any([child])?.status.exit_code()? == 0
  assert uu.read(s, ".uu-stdout")? == b""
  assert uu.read(s, ".uu-stderr")? == b"0+0 records in\n0+0 records out\n"
}

# origin: uutils test_dd::test_seek_past_dev
test test_uu_dd_seek_past_dev { |ctx|
  let s = uu.scene(ctx)?
  if ! p"/dev/sda1".exists()? { test.skip("no /dev/sda1 device found") }
  if applet.current_euid() != 0 { test.skip("requires root user") }
  let r = uu.invoke(s, "dd", ["bs=1", "seek=10000000000000000", "count=0", "status=noxfer"], stdout: p"/dev/sda1")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "dd: 'standard output': cannot seek: Invalid argument")
  uu.stderr_contains(r, "0+0 records in")
  uu.stderr_contains(r, "0+0 records out")
}

# origin: uutils test_dd::test_self_transfer
test test_uu_dd_self_transfer { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "self-transfer-256k.txt", "self-transfer-256k.txt")?
  assert uu.file_exists(s, "self-transfer-256k.txt")?
  assert uu.size(s, "self-transfer-256k.txt")? == 262144
  let r = uu.invoke(s, "dd", ["status=none", "conv=notrunc", "if=self-transfer-256k.txt", "of=self-transfer-256k.txt"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "self-transfer-256k.txt")?
  assert uu.size(s, "self-transfer-256k.txt")? == 262144
}

# origin: uutils test_dd::test_skip_beyond_file
test test_uu_dd_skip_beyond_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bs=1", "skip=5", "count=0", "status=noxfer"], b"abcd", timeout: 15s)?
  uu.succeeds(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "'standard input': cannot skip to specified offset\n0+0 records in\n0+0 records out\n")
}

# origin: uutils test_dd::test_skip_beyond_file_seekable_stdin
test test_uu_dd_skip_beyond_file_seekable_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "abcd")?
  for args in [["bs=1", "skip=5"], ["bs=3", "skip=2"]] {
    let r = uu.invoke_from_path(s, "dd", args.extend(["count=0", "status=noxfer"]), uu.at(s, "in"))?
    uu.succeeds(r)
    uu.no_stdout(r)
    uu.stderr_contains(r, "'standard input': cannot skip to specified offset\n0+0 records in\n0+0 records out\n")
  }
}

# origin: uutils test_dd::test_skip_blocks_times_ibs_overflow_does_not_wrap
test test_uu_dd_skip_blocks_times_ibs_overflow_does_not_wrap { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in.f", "0123456789abcdef")?
  let r = uu.invoke(s, "dd", ["if=in.f", "skip=17592186044416", "ibs=1048576", "count=1"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Value too large for defined data type")
}

# origin: uutils test_dd::test_skip_input_fifo
test test_uu_dd_skip_input_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  let plan = uu.command(s, "dd", ["count=0", "skip=1", "if=fifo", "status=noxfer"], timeout: 15s)?
  let child = spawn plan?
  uu.at(s, "fifo").write(bytes.zero(512)?)?
  assert (wait child?).exited_with(0)
  assert uu.read(s, ".uu-stdout")? == b""
  assert uu.read(s, ".uu-stderr")? == b"0+0 records in\n0+0 records out\n"
}

# origin: uutils test_dd::test_skip_past_dev
test test_uu_dd_skip_past_dev { |ctx|
  let s = uu.scene(ctx)?
  if ! p"/dev/sda1".exists()? { test.skip("no /dev/sda1 device found") }
  if applet.current_euid() != 0 { test.skip("requires root user") }
  let r = uu.invoke_from_path(s, "dd", ["bs=1", "skip=10000000000000000", "count=0", "status=noxfer"], p"/dev/sda1")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "dd: 'standard input': cannot skip: Invalid argument")
  uu.stderr_contains(r, "0+0 records in")
  uu.stderr_contains(r, "0+0 records out")
}

# origin: uutils test_dd::test_skip_zero
test test_uu_dd_skip_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["skip=0", "status=noxfer"], b"", timeout: 15s)?
  uu.succeeds(r)
  uu.no_stdout(r)
  uu.stderr_is(r, "0+0 records in\n0+0 records out\n")
}

# origin: uutils test_dd::test_sparse
test test_uu_dd_sparse { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "infile")?
  uu.truncate(s, "infile", 1048576)?
  let r = uu.invoke(s, "dd", ["bs=32K", "if=infile", "of=outfile", "conv=sparse"])?
  uu.succeeds(r)
  assert uu.size(s, "infile")? == uu.size(s, "outfile")?
}

# origin: uutils test_dd::test_stdin_stdout
test test_uu_dd_stdin_stdout { |ctx|
  let s = uu.scene(ctx)?
  let input = ascii_block(521)?
  let r = uu.invoke(s, "dd", ["status=none"], input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, input)
  uu.no_stderr(r)
}

# origin: uutils test_dd::test_stdin_stdout_count
test test_uu_dd_stdin_stdout_count { |ctx|
  let s = uu.scene(ctx)?
  let input = ascii_block(521)?
  let r = uu.invoke(s, "dd", ["status=none", "count=2", "ibs=128"], input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, input[..256])
  uu.no_stderr(r)
}

# origin: uutils test_dd::test_stdin_stdout_count_bytes
test test_uu_dd_stdin_stdout_count_bytes { |ctx|
  let s = uu.scene(ctx)?
  let input = ascii_block(521)?
  let r = uu.invoke(s, "dd", ["status=none", "count=256", "iflag=count_bytes"], input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, input[..256])
  uu.no_stderr(r)
}

# origin: uutils test_dd::test_stdin_stdout_count_w_multiplier
test test_uu_dd_stdin_stdout_count_w_multiplier { |ctx|
  let s = uu.scene(ctx)?
  let input = ascii_block(5120)?
  let r = uu.invoke(s, "dd", ["status=none", "count=2KiB", "iflag=count_bytes"], input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, input[..2048])
  uu.no_stderr(r)
}

# origin: uutils test_dd::test_stdin_stdout_skip
test test_uu_dd_stdin_stdout_skip { |ctx|
  let s = uu.scene(ctx)?
  let input = ascii_block(521)?
  let r = uu.invoke(s, "dd", ["status=none", "skip=2", "ibs=128"], input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, input[256..])
  uu.no_stderr(r)
}

# origin: uutils test_dd::test_stdin_stdout_skip_bytes
test test_uu_dd_stdin_stdout_skip_bytes { |ctx|
  let s = uu.scene(ctx)?
  let input = ascii_block(521)?
  let r = uu.invoke(s, "dd", ["status=none", "skip=256", "ibs=128", "iflag=skip_bytes"], input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, input[256..])
  uu.no_stderr(r)
}

# origin: uutils test_dd::test_stdin_stdout_skip_w_multiplier
test test_uu_dd_stdin_stdout_skip_w_multiplier { |ctx|
  let s = uu.scene(ctx)?
  let input = ascii_block(10240)?
  let r = uu.invoke(s, "dd", ["status=none", "skip=5K", "iflag=skip_bytes"], input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, input[5120..])
}

# origin: uutils test_dd::test_swab_256_test
test test_uu_dd_swab_256_test { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=swab"], fixture(s, "seq-byte-values.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "seq-byte-values-swapped.test")?)
}

# origin: uutils test_dd::test_swab_257_test
test test_uu_dd_swab_257_test { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=swab"], fixture(s, "seq-byte-values-odd.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "seq-byte-values-odd.spec")?)
}

# origin: uutils test_dd::test_sync_delayed_reader
test test_uu_dd_sync_delayed_reader { |ctx|
  let s = uu.scene(ctx)?
  let data = bytes.from_ints([15 for _ in range(64)])?
  let expected = bytes.from_ints([if i % 16 < 8 { 15 } else { 0 } for i in range(128)])?
  let r = fifo_copy(s, ["ibs=16", "obs=32", "conv=sync", "if=fifo", "status=noxfer"], data, 8, 100ms)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
  uu.stderr_is(r, "0+8 records in\n4+0 records out\n")
}

# origin: uutils test_dd::test_to_file_with_ibs_obs
test test_uu_dd_to_file_with_ibs_obs { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "zero-256k.txt", "zero-256k.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "if=zero-256k.txt", "of=TESTFILE-zero-256k.tmp", "ibs=222", "obs=111"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read(s, "TESTFILE-zero-256k.tmp")? == fixture(s, "zero-256k.txt")?
}

# origin: uutils test_dd::test_to_stdout_with_ibs_obs
test test_uu_dd_to_stdout_with_ibs_obs { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "y-nl-1k.txt", "y-nl-1k.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "if=y-nl-1k.txt", "ibs=521", "obs=1031"])?
  uu.succeeds(r)
  uu.stdout_only(r, ["y\n" for _ in range(512)].join(""))
}

# origin: uutils test_dd::test_truncated_record
test test_uu_dd_truncated_record { |ctx|
  let s = uu.scene(ctx)?
  for item in [{input: b"ab", output: b"a", stats: "1 truncated record\n"}, {input: b"ab\ncd\n", output: b"ac", stats: "2 truncated records\n"}] {
    let r = uu.invoke(s, "dd", ["cbs=1", "conv=block", "status=noxfer"], item.input)?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, item.output)
    uu.stderr_is(r, "0+1 records in\n0+1 records out\n" + item.stats)
  }
}

# origin: uutils test_dd::test_ucase_ascii_to_lcase_ascii
test test_uu_dd_ucase_ascii_to_lcase_ascii { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=lcase"], fixture(s, "ucase-ascii.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "lcase-ascii.test")?)
}

# origin: uutils test_dd::test_ucase_lcase
test test_uu_dd_ucase_lcase { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=ucase,lcase"], b"", timeout: 15s)?
  uu.fails(r)
  uu.stderr_contains(r, "lcase")
  uu.stderr_contains(r, "ucase")
}

# origin: uutils test_dd::test_unblock_lower
test test_uu_dd_unblock_lower { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=unblock,lcase", "cbs=8"], fixture(s, "dd-unblock8-lowercase.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "dd-unblock8-lowercase.spec")?)
}

# origin: uutils test_dd::test_unblock_multi_16
test test_uu_dd_unblock_multi_16 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=unblock", "cbs=16"], fixture(s, "dd-unblock-cbs16.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "dd-unblock-cbs16.spec")?)
}

# origin: uutils test_dd::test_unblock_multi_16_as_8
test test_uu_dd_unblock_multi_16_as_8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=unblock", "cbs=8"], fixture(s, "dd-unblock-cbs16.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "dd-unblock-cbs8.spec")?)
}

# origin: uutils test_dd::test_unicode_filenames
test test_uu_dd_unicode_filenames { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "😎💚🦊.txt", "😎💚🦊.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "if=😎💚🦊.txt", "of=TESTFILE-😎💚🦊.tmp"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read(s, "TESTFILE-😎💚🦊.tmp")? == fixture(s, "😎💚🦊.txt")?
}

# origin: uutils test_dd::test_wrong_number_err_msg
test test_uu_dd_wrong_number_err_msg { |ctx|
  let s = uu.scene(ctx)?
  for number in ["kBb", "1kBb555"] {
    let r = uu.invoke(s, "dd", [f"count={number}"])?
    uu.fails(r)
    uu.stderr_contains(r, f"dd: invalid number: '{number}'\n")
  }
}

# origin: uutils test_dd::test_x_multiplier
test test_uu_dd_x_multiplier { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bs=2x3", "count=1"], b"abcdefghi", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is(r, "abcdef")
}

# origin: uutils test_dd::test_ys_to_stdout
test test_uu_dd_ys_to_stdout { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "y-nl-1k.txt", "y-nl-1k.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "if=y-nl-1k.txt"])?
  uu.succeeds(r)
  uu.stdout_only(r, ["y\n" for _ in range(512)].join(""))
}

# origin: uutils test_dd::test_zero_multiplier_warning
test test_uu_dd_zero_multiplier_warning { |ctx|
  let s = uu.scene(ctx)?
  let warning = "dd: warning: '0x' is a zero multiplier; use '00x' if that is intended\n"
  for arg in ["count", "seek", "skip"] {
    for number in ["0", "00x1"] {
      let r = uu.invoke(s, "dd", [f"{arg}={number}", "status=none"])?
      uu.succeeds(r)
      uu.no_output(r)
    }
    for number in ["0x1", "1x0x1"] {
      let r = uu.invoke(s, "dd", [f"{arg}={number}", "status=none"])?
      uu.succeeds(r)
      uu.no_stdout(r)
      uu.stderr_contains(r, "warning: '0x' is a zero multiplier; use '00x' if that is intended")
    }
    for number in ["0x0x1", "0x0x0"] {
      let r = uu.invoke(s, "dd", [f"{arg}={number}", "status=none"])?
      uu.succeeds(r)
      uu.no_stdout(r)
      uu.stderr_is(r, warning)
    }
  }
}

# origin: uutils test_dd::test_zeros_4k_conv_sync_ibs_gt_obs
test test_uu_dd_zeros_4k_conv_sync_ibs_gt_obs { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=sync", "ibs=1031", "obs=521"], fixture(s, "zeros-620f0b67a91f7f74151bc5be745b7110.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "gnudd-conv-sync-ibs-1031-obs-521-zeros.spec")?)
}

# origin: uutils test_dd::test_zeros_4k_conv_sync_obs_gt_ibs
test test_uu_dd_zeros_4k_conv_sync_obs_gt_ibs { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=sync", "ibs=521", "obs=1031"], fixture(s, "zeros-620f0b67a91f7f74151bc5be745b7110.test")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "gnudd-conv-sync-ibs-521-obs-1031-zeros.spec")?)
}

# origin: uutils test_dd::test_zeros_to_file
test test_uu_dd_zeros_to_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "zero-256k.txt", "zero-256k.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "if=zero-256k.txt", "of=TESTFILE-zero-256k.tmp"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read(s, "TESTFILE-zero-256k.tmp")? == fixture(s, "zero-256k.txt")?
}

# origin: uutils test_dd::test_zeros_to_stdout
test test_uu_dd_zeros_to_stdout { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "zero-256k.txt", "zero-256k.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "if=zero-256k.txt"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.zero(262144)?)
}

# origin: uutils test_dd::version
test test_uu_dd_version { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["--version"], b"", timeout: 15s)?
  uu.succeeds(r)
}

# origin: uutils test_dd::test_skip_overflow
test test_uu_dd_skip_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bs=1", "skip=9223372036854775808", "count=0"])?
  uu.fails(r)
  uu.stderr_contains(r, "dd: invalid number: '9223372036854775808': Value too large for defined data type")
}
