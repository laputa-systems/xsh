use support.checksum
use support.uu

# origin: busybox md5sum/md5sum
test test_bb_md5sum_md5sum_chunk_000 { |ctx| checksum.check_chunk(ctx, "md5sum", 0) }
test test_bb_md5sum_md5sum_chunk_001 { |ctx| checksum.check_chunk(ctx, "md5sum", 100) }
test test_bb_md5sum_md5sum_chunk_002 { |ctx| checksum.check_chunk(ctx, "md5sum", 200) }
test test_bb_md5sum_md5sum_chunk_003 { |ctx| checksum.check_chunk(ctx, "md5sum", 300) }
test test_bb_md5sum_md5sum_chunk_004 { |ctx| checksum.check_chunk(ctx, "md5sum", 400) }
test test_bb_md5sum_md5sum_chunk_005 { |ctx| checksum.check_chunk(ctx, "md5sum", 500) }
test test_bb_md5sum_md5sum_chunk_006 { |ctx| checksum.check_chunk(ctx, "md5sum", 600) }
test test_bb_md5sum_md5sum_chunk_007 { |ctx| checksum.check_chunk(ctx, "md5sum", 700) }
test test_bb_md5sum_md5sum_chunk_008 { |ctx| checksum.check_chunk(ctx, "md5sum", 800) }
test test_bb_md5sum_md5sum_chunk_009 { |ctx| checksum.check_chunk(ctx, "md5sum", 900) }
test test_bb_md5sum_md5sum_original_aggregate { |ctx| checksum.check_original_aggregate(ctx, "md5sum", "efe30c482e0b687e0cca0612f42ca29b") }

# origin: busybox md5sum/md5sum -c EMPTY
test test_bb_md5sum_md5sum_c_EMPTY_9c7796bd { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "EMPTY")
  uu.fails_with_code(uu.invoke(s, "md5sum", ["-c", "EMPTY"])?, 1)
}

# origin: busybox md5sum/md5sum-verifies-non-binary-file
test test_bb_md5sum_md5sum_verifies_non_binary_file_394a9a7e { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")
  let listing = uu.invoke(s, "md5sum", ["foo"])?
  uu.succeeds(listing)
  uu.write_bytes(s, "bar", listing.stdout)
  uu.succeeds(uu.invoke(s, "md5sum", ["-c", "bar"])?)
}
