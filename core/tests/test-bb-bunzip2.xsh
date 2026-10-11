use support.uu

proc packed(s: uu.Scene, name: Str) [fs, error] -> Result[Bytes] {
  fp"{s.ctx.core_dir}/tests/data/busybox/bunzip2/{name}.bz2".read_bytes()
}

proc prepare(s: uu.Scene) [fs, error] -> Result[Unit] {
  uu.write_bytes(s, "t1.bz2", packed(s, "hello")?)?
  uu.write_bytes(s, "t2.bz2", packed(s, "hello")?)?
  Ok()
}

# origin: busybox bunzip2/bunzip2-reads-from-standard-input
test test_bb_bunzip2_bunzip2_reads_from_standard_input_71a35b87 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "bunzip2", [], stdin: packed(s, "foo")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "foo\n")
}

# origin: busybox bunzip2/bunzip2: already exists
test test_bb_bunzip2_bunzip2_already_exists_d734c910 { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  uu.touch(s, "t1")?
  let r = uu.invoke(s, "bunzip2", ["t1.bz2", "t2.bz2"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "bunzip2: can't open 't1': File exists\n")
  assert bytes.concat([uu.read(s, "t1")?, uu.read(s, "t2")?]) == b"HELLO\n"
}

# origin: busybox bunzip2/bunzip2: delete src
test test_bb_bunzip2_bunzip2_delete_src_cb3b80fd { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  uu.succeeds(uu.invoke(s, "bunzip2", ["t2.bz2"])?)
  assert ! uu.exists(s, "t2.bz2")?
}

# origin: busybox bunzip2/bunzip2: doesnt exist
test test_bb_bunzip2_bunzip2_doesnt_exist_ab375dd9 { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  let r = uu.invoke(s, "bunzip2", ["z", "t1.bz2"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "bunzip2: z: No such file or directory\n")
  uu.file_is(s, "t1", "HELLO\n")
}

# origin: busybox bunzip2/bunzip2: stream unpack
test test_bb_bunzip2_bunzip2_stream_unpack_619a21f7 { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  let r = uu.invoke(s, "bunzip2", [], stdin: uu.read(s, "t1.bz2")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "HELLO\n")
}

# origin: busybox bunzip2/bunzip2: unknown suffix
test test_bb_bunzip2_bunzip2_unknown_suffix_078b255b { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  uu.touch(s, "t.zz")?
  let r = uu.invoke(s, "bunzip2", ["t.zz", "t1.bz2"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "bunzip2: t.zz: unknown suffix - ignored\n")
  uu.file_is(s, "t1", "HELLO\n")
}

# origin: busybox bunzip2/bunzip2-removes-compressed-file
test test_bb_bunzip2_bunzip2_removes_compressed_file_3d9d152d { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "foo.bz2", packed(s, "foo")?)?
  uu.succeeds(uu.invoke(s, "bunzip2", ["foo.bz2"])?)
  assert ! uu.exists(s, "foo.bz2")?
}

# origin: busybox bunzip2/bunzip2: test_bz2 file
test test_bb_bunzip2_bunzip2_test_bz2_file_a9bce3bd { |ctx|
  let s = uu.scene(ctx)?
  let input = packed(s, "test")?
  uu.succeeds(uu.invoke(s, "bunzip2", [], stdin: input)?)
  let unpacked = uu.invoke(s, "bunzip2", [], stdin: input)?
  uu.succeeds(unpacked)
  let checksum = uu.invoke(s, "md5sum", [], stdin: unpacked.stdout)?
  uu.succeeds(checksum)
  uu.stdout_is(checksum, "61bbeee4be9c6f110a71447f584fda7b  -\n")
}

# origin: busybox bunzip2/bunzip2: pbzip_4m_zeros file
test test_bb_bunzip2_bunzip2_pbzip_4m_zeros_file_90b5162f { |ctx|
  let s = uu.scene(ctx)?
  let input = packed(s, "zeros")?
  uu.succeeds(uu.invoke(s, "bunzip2", [], stdin: input)?)
  let unpacked = uu.invoke(s, "bunzip2", [], stdin: input)?
  uu.succeeds(unpacked)
  let checksum = uu.invoke(s, "md5sum", [], stdin: unpacked.stdout)?
  uu.succeeds(checksum)
  uu.stdout_is(checksum, "b5cfa9d6c8febd618f91ac2843d50a1c  -\n")
}

# origin: busybox bunzip2/bunzip2: bz2_issue_11.bz2 corrupted example
test test_bb_bunzip2_bunzip2_bz2_issue_11_bz2_corrupted_example_4ad0d7f2 { |ctx|
  let s = uu.scene(ctx)?
  let input = packed(s, "bz2_issue_11")?
  let r = uu.invoke(s, "bunzip2", [], stdin: input)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "bunzip2: bunzip error -5\n")
}

# origin: busybox bunzip2/bunzip2: bz2_issue_12.bz2 corrupted example
test test_bb_bunzip2_bunzip2_bz2_issue_12_bz2_corrupted_example_46248631 { |ctx|
  let s = uu.scene(ctx)?
  let input = packed(s, "bz2_issue_12")?
  let r = uu.invoke(s, "bunzip2", [], stdin: input)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "bunzip2: bunzip error -5\n")
}
