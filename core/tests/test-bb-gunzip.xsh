use support.uu

proc packed(s: uu.Scene, name: Str) [fs, error] -> Result[Bytes] {
  fp"{s.ctx.core_dir}/tests/data/busybox/gunzip/{name}.gz".read_bytes()
}

proc prepare(s: uu.Scene) [fs, error] -> Result[Unit] {
  uu.write_bytes(s, "t1.gz", packed(s, "hello")?)?
  uu.write_bytes(s, "t2.gz", packed(s, "hello")?)?
  Ok()
}

# origin: busybox gunzip/gunzip-reads-from-standard-input
test test_bb_gunzip_gunzip_reads_from_standard_input_2cdfab61 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "gunzip", [], stdin: packed(s, "foo")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "foo\n")
}

# origin: busybox gunzip/gunzip: already exists
test test_bb_gunzip_gunzip_already_exists_c740415b { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  uu.touch(s, "t1")?
  let r = uu.invoke(s, "gunzip", ["t1.gz", "t2.gz"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "gunzip: can't open 't1': File exists\n")
  assert bytes.concat([uu.read(s, "t1")?, uu.read(s, "t2")?]) == b"HELLO\n"
}

# origin: busybox gunzip/gunzip: delete src
test test_bb_gunzip_gunzip_delete_src_9203335c { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  uu.succeeds(uu.invoke(s, "gunzip", ["t2.gz"])?)
  assert ! uu.exists(s, "t2.gz")?
}

# origin: busybox gunzip/gunzip: doesnt exist
test test_bb_gunzip_gunzip_doesnt_exist_88d062ab { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  let r = uu.invoke(s, "gunzip", ["z", "t1.gz"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "gunzip: z: No such file or directory\n")
  uu.file_is(s, "t1", "HELLO\n")
}

# origin: busybox gunzip/gunzip: stream unpack
test test_bb_gunzip_gunzip_stream_unpack_7394a9ba { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  let r = uu.invoke(s, "gunzip", [], stdin: uu.read(s, "t1.gz")?)?
  uu.succeeds(r)
  uu.stdout_is(r, "HELLO\n")
}

# origin: busybox gunzip/gunzip: unknown suffix
test test_bb_gunzip_gunzip_unknown_suffix_802096c9 { |ctx|
  let s = uu.scene(ctx)?
  prepare(s)?
  uu.touch(s, "t.zz")?
  let r = uu.invoke(s, "gunzip", ["t.zz", "t1.gz"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "gunzip: t.zz: unknown suffix - ignored\n")
  uu.file_is(s, "t1", "HELLO\n")
}
