use support.uu as uu

# origin: gnu truncate/multiple-files.log
test test_gnu_truncate_multiple_files_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s0", "a", ".", "b"])?
  uu.fails_with_code(r, 1)
  assert uu.file_exists(s, "a")?
  assert uu.file_exists(s, "b")?
  uu.no_stdout(r)
  assert r.stderr.len() > 0
}

# origin: gnu truncate/truncate-dangling-symlink.log
test test_gnu_truncate_truncate_dangling_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "truncate-target", "t-symlink")?
  uu.succeeds(uu.invoke(s, "truncate", ["-s0", "t-symlink"])?)
  assert uu.file_exists(s, "truncate-target")?
}

# origin: gnu truncate/truncate-dir-fail.log
test test_gnu_truncate_truncate_dir_fail_log { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "truncate", ["-s+0", "."])?, 1)
}

# origin: gnu truncate/truncate-fail-diag.log
test test_gnu_truncate_truncate_fail_diag_log { |ctx|
  let s = uu.scene(ctx)?
  let missing = uu.invoke(s, "truncate", ["-s0", "no/such-dir"])?
  uu.fails(missing)
  assert bytes.concat([missing.stdout, missing.stderr]) == b"truncate: cannot open 'no/such-dir' for writing: No such file or directory\n"
  uu.fails(uu.invoke(s, "truncate", ["-s0", "no/"])?)
}

# origin: gnu truncate/truncate-fifo.log
test test_gnu_truncate_truncate_fifo_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  let target = uu.invoke(s, "truncate", ["-s0", "fifo"], timeout: 10s)?
  assert target.status != 124
  uu.touch(s, "file")?
  let reference = uu.invoke(s, "truncate", ["--reference=fifo", "file"], timeout: 10s)?
  assert reference.status != 124
}

# origin: gnu truncate/truncate-no-create-missing.log
test test_gnu_truncate_truncate_no_create_missing_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "truncate", ["-s0", "-c", "no-such-file"])?)
}

# origin: gnu truncate/truncate-overflow.log
test test_gnu_truncate_truncate_overflow_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "truncate", ["-s-1", "create-zero-len-file"])?)
  uu.write(s, "non-empty-file", "\n")?
  uu.fails_with_code(uu.invoke(s, "truncate", ["-s9223372036854775808", "file"])?, 1)
  uu.fails_with_code(uu.invoke(s, "truncate", ["-s+9223372036854775807", "non-empty-file"])?, 1)
  let stats = run.capture --text stat -f -c%s $s.root
  assert stats.status.exited_with(0)
  let block_size = stats.stdout.trim().parse_int()?
  let overflow = 9223372036854775807 / block_size + 1
  uu.fails_with_code(uu.invoke(s, "truncate", ["--io-blocks", f"--size={overflow}", "file"])?, 1)
}

# origin: gnu truncate/truncate-parameters.log
test test_gnu_truncate_truncate_parameters_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [
    ["--size=0"],
    ["file"],
    ["--size=0", "--reference=file", "file"],
    ["--io-blocks", "--reference=file", "file"],
    ["--size=invalid", "file"],
    ["--size=> -1", "file"],
  ] { uu.fails_with_code(uu.invoke(s, "truncate", args)?, 1) }
  uu.succeeds(uu.invoke(s, "truncate", ["--size= >1", "file"])?)
  uu.succeeds(uu.invoke(s, "truncate", ["--size= +1", "file"])?)
  assert uu.size(s, "file")? == 2
  uu.succeeds(uu.invoke(s, "truncate", ["--size= +1", "-r", "file", "file"])?)
  assert uu.size(s, "file")? == 3
  uu.succeeds(uu.invoke(s, "truncate", ["-r", "file", "file"])?)
  assert uu.size(s, "file")? == 3
  uu.succeeds(uu.invoke(s, "truncate", ["-r", "file", "file2"])?)
  assert uu.size(s, "file2")? == 3
  uu.succeeds(uu.invoke(s, "truncate", ["-s", "-1", "file"])?)
  assert uu.size(s, "file")? == 2
}

# origin: gnu truncate/truncate-relative.log
test test_gnu_truncate_truncate_relative_log { |ctx|
  let s = uu.scene(ctx)?
  for size in ["+>0", ">+0", "/0", "%0"] {
    uu.fails_with_code(uu.invoke(s, "truncate", [f"--size={size}", "file"])?, 1)
  }
}
