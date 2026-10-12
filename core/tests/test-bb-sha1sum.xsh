use support.checksum
use support.uu

# origin: busybox sha1sum/sha1sum
test test_bb_sha1sum_sha1sum_chunk_000 { |ctx| checksum.check_chunk(ctx, "sha1sum", 0) }
test test_bb_sha1sum_sha1sum_chunk_001 { |ctx| checksum.check_chunk(ctx, "sha1sum", 100) }
test test_bb_sha1sum_sha1sum_chunk_002 { |ctx| checksum.check_chunk(ctx, "sha1sum", 200) }
test test_bb_sha1sum_sha1sum_chunk_003 { |ctx| checksum.check_chunk(ctx, "sha1sum", 300) }
test test_bb_sha1sum_sha1sum_chunk_004 { |ctx| checksum.check_chunk(ctx, "sha1sum", 400) }
test test_bb_sha1sum_sha1sum_chunk_005 { |ctx| checksum.check_chunk(ctx, "sha1sum", 500) }
test test_bb_sha1sum_sha1sum_chunk_006 { |ctx| checksum.check_chunk(ctx, "sha1sum", 600) }
test test_bb_sha1sum_sha1sum_chunk_007 { |ctx| checksum.check_chunk(ctx, "sha1sum", 700) }
test test_bb_sha1sum_sha1sum_chunk_008 { |ctx| checksum.check_chunk(ctx, "sha1sum", 800) }
test test_bb_sha1sum_sha1sum_chunk_009 { |ctx| checksum.check_chunk(ctx, "sha1sum", 900) }
test test_bb_sha1sum_sha1sum_original_aggregate { |ctx| checksum.check_original_aggregate(ctx, "sha1sum", "d41337e834377140ae7f98460d71d908598ef04f") }

# origin: busybox sha1sum/sha1sum -c EMPTY
test test_bb_sha1sum_sha1sum_c_EMPTY_31f096b5 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "EMPTY")?
  uu.fails_with_code(uu.invoke(s, "sha1sum", ["-c", "EMPTY"])?, 1)
}
