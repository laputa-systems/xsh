##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_mknod.rs.

use support.uu as uu

# origin: uutils test_mknod::test_mknod_overflow_major_minor
test test_uu_mknod_mknod_overflow_major_minor { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["lg32", "c", "4294967296", "1"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "mknod: invalid major device number '4294967296'\n")
}

# origin: uutils test_mknod::test_mknod_invalid_arg
test test_uu_mknod_mknod_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["--foo"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "mknod: unrecognized option '--foo'\nTry 'mknod --help' for more information.\n")
}

# origin: uutils test_mknod::test_mknod_help
test test_uu_mknod_mknod_help { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["--help"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "Usage:")
}

# origin: uutils test_mknod::test_mknod_version
test test_uu_mknod_mknod_version { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["--version"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_str_starts_with(r, "mknod")
}

# origin: uutils test_mknod::test_mknod_fifo_default_writable
test test_uu_mknod_mknod_fifo_default_writable { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["test_file", "p"])?
  uu.succeeds(r)
  let metadata = fs.stat(uu.at(s, "test_file"))?
  assert metadata.kind == "fifo"
  assert metadata.mode.bit_and(0o222) != 0
}

# origin: uutils test_mknod::test_mknod_fifo_mnemonic_usage
test test_uu_mknod_mknod_fifo_mnemonic_usage { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["test_file", "pipe"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "test_file"))?.kind == "fifo"
}

# origin: uutils test_mknod::test_mknod_fifo_read_only
test test_uu_mknod_mknod_fifo_read_only { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["-m", "a=r", "test_file", "p"])?
  uu.succeeds(r)
  let metadata = fs.stat(uu.at(s, "test_file"))?
  assert metadata.kind == "fifo"
  assert metadata.mode.bit_and(0o222) == 0
}

# origin: uutils test_mknod::test_mknod_fifo_invalid_extra_operand
test test_uu_mknod_mknod_fifo_invalid_extra_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["test_file", "p", "1", "2"])?
  uu.fails(r)
  uu.stderr_contains(r, "Fifos do not have major and minor device numbers")
}

# origin: uutils test_mknod::test_mknod_character_device_requires_major_and_minor
test test_uu_mknod_mknod_character_device_requires_major_and_minor { |ctx|
  let s = uu.scene(ctx)?
  let missing = uu.invoke(s, "mknod", ["test_file", "c"])?
  uu.fails_with_code(missing, 1)
  uu.stderr_is(missing, "mknod: missing operand after 'c'\nSpecial files require major and minor device numbers.\nTry 'mknod --help' for more information.\n")
  let minor = uu.invoke(s, "mknod", ["test_file", "c", "1"])?
  uu.fails_with_code(minor, 1)
  uu.stderr_is(minor, "mknod: missing operand after '1'\nTry 'mknod --help' for more information.\n")
  let invalid_minor = uu.invoke(s, "mknod", ["test_file", "c", "1", "c"])?
  uu.fails(invalid_minor)
  uu.stderr_is(invalid_minor, "mknod: invalid minor device number 'c'\n")
  let invalid_major = uu.invoke(s, "mknod", ["test_file", "c", "c", "1"])?
  uu.fails(invalid_major)
  uu.stderr_is(invalid_major, "mknod: invalid major device number 'c'\n")
}

# origin: uutils test_mknod::test_mknod_invalid_mode
test test_uu_mknod_mknod_invalid_mode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["--mode", "rw", "test_file", "p"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "invalid mode")
}

# origin: uutils test_mknod::test_mknod_mode_comma_separated
test test_uu_mknod_mknod_mode_comma_separated { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["-m", "u=rwx,g=rx,o=", "test_file", "p"])?
  uu.succeeds(r)
  let metadata = fs.stat(uu.at(s, "test_file"))?
  assert metadata.kind == "fifo"
  assert metadata.mode.bit_and(0o777) == 0o750
}

# origin: uutils test_mknod::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_mknod_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mknod", ["-m", "u+rw?", "some_node", "p"])?
  uu.fails_with_code(r, 1)
  let stderr = r.stderr.utf8()?
  assert stderr.starts_with("mknod: ")
  assert ":1:" not in stderr
}
