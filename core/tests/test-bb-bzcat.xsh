use support.uu

# origin: busybox bzcat/bzcat can handle compressed zero-length bzip2 files
test test_bb_bzcat_bzcat_can_handle_compressed_zero_length_bzip2_files_bb1e1981 { |ctx|
  let s = uu.scene(ctx)?
  let packed = fp"{s.ctx.core_dir}/tests/data/busybox/bzcat/empty.bz2".read_bytes()?
  uu.write_bytes(s, "input", packed)?
  let r = uu.invoke(s, "bzcat", ["input", "input"])?
  uu.succeeds(r)
  uu.stdout_is(r, "")
}

# origin: busybox bzcat/bzcat can print many files
test test_bb_bzcat_bzcat_can_print_many_files_5c2b7261 { |ctx|
  let s = uu.scene(ctx)?
  let packed = fp"{s.ctx.core_dir}/tests/data/busybox/bzcat/a.bz2".read_bytes()?
  uu.write_bytes(s, "input", packed)?
  let r = uu.invoke(s, "bzcat", ["input", "input"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a\na\n")
}

# origin: busybox bzcat/bzcat-does-not-remove-compressed-file
test test_bb_bzcat_bzcat_does_not_remove_compressed_file_42211728 { |ctx|
  let s = uu.scene(ctx)?
  let packed = fp"{s.ctx.core_dir}/tests/data/busybox/bzcat/foo.bz2".read_bytes()?
  uu.write_bytes(s, "foo.bz2", packed)?
  let r = uu.invoke(s, "bzcat", ["foo.bz2"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "foo.bz2")?
}

