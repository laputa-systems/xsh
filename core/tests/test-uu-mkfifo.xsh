##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_mkfifo.rs.

use support.uu as uu

# origin: uutils test_mkfifo::test_invalid_arg
test test_uu_mkfifo_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "mkfifo", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_mkfifo::test_create_fifo_missing_operand
test test_uu_mkfifo_create_fifo_missing_operand { |ctx|
  let s = uu.scene(ctx)?
  uu.fails(uu.invoke(s, "mkfifo", [])?)
}

# origin: uutils test_mkfifo::test_create_one_fifo
test test_uu_mkfifo_create_one_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkfifo", ["abc"])?)
}

# origin: uutils test_mkfifo::test_create_one_fifo_with_invalid_mode
test test_uu_mkfifo_create_one_fifo_with_invalid_mode { |ctx|
  for mode in ["invalid", "0999"] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "mkfifo", ["abcd", "-m", mode])?
    uu.fails(r)
    uu.stderr_contains(r, "invalid mode")
  }
}

# origin: uutils test_mkfifo::test_create_one_fifo_with_non_file_permission_mode
test test_uu_mkfifo_create_one_fifo_with_non_file_permission_mode { |ctx|
  let s = uu.scene(ctx)?
  let special = uu.invoke(s, "mkfifo", ["abcd", "-m", "1777"])?
  uu.fails(special)
  uu.stderr_is(special, "mkfifo: mode must specify only file permission bits\n")
  let invalid = uu.invoke(s, "mkfifo", ["abcd", "-m", "1999"])?
  uu.fails(invalid)
  uu.stderr_contains(invalid, "invalid mode")
}

# origin: uutils test_mkfifo::test_create_multiple_fifos
test test_uu_mkfifo_create_multiple_fifos { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkfifo", ["abcde", "def", "sed", "dum"])?)
}

# origin: uutils test_mkfifo::test_create_one_fifo_with_mode
test test_uu_mkfifo_create_one_fifo_with_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkfifo", ["abcde", "-m600"])?)
}

# origin: uutils test_mkfifo::test_create_one_fifo_already_exists
test test_uu_mkfifo_create_one_fifo_already_exists { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkfifo", ["abcdef", "abcdef"])?
  uu.fails(r)
  uu.stderr_contains(r, "mkfifo: cannot create fifo 'abcdef': File exists")
}

# origin: uutils test_mkfifo::test_create_fifo_permission_denied
test test_uu_mkfifo_create_fifo_permission_denied { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "owner_no_exec_dir")?
  uu.set_mode(s, "owner_no_exec_dir", 0o644)?
  let r = uu.invoke(s, "mkfifo", ["owner_no_exec_dir/mkfifo_err", "-m", "666"])?
  uu.fails(r)
  uu.stderr_is(r, "mkfifo: cannot create fifo 'owner_no_exec_dir/mkfifo_err': Permission denied\n")
}

# origin: uutils test_mkfifo::test_mkfifo_permission_unchanged_when_failed
test test_uu_mkfifo_mkfifo_permission_unchanged_when_failed { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_file", "content")?
  uu.set_mode(s, "test_file", 0o600)?
  let r = uu.invoke(s, "mkfifo", ["test_file", "-m", "666"])?
  uu.fails(r)
  uu.stderr_is(r, "mkfifo: cannot create fifo 'test_file': File exists\n")
  assert fs.stat(uu.at(s, "test_file"))?.kind == "file"
  assert uu.mode(s, "test_file")? == 0o600
}
