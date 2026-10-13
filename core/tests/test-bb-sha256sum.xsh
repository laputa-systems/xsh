use support.checksum
use support.uu

# origin: busybox sha256sum/sha256sum
test test_bb_sha256sum_sha256sum_chunk_000 { |ctx| checksum.check_chunk(ctx, "sha256sum", 0) }
test test_bb_sha256sum_sha256sum_chunk_001 { |ctx| checksum.check_chunk(ctx, "sha256sum", 100) }
test test_bb_sha256sum_sha256sum_chunk_002 { |ctx| checksum.check_chunk(ctx, "sha256sum", 200) }
test test_bb_sha256sum_sha256sum_chunk_003 { |ctx| checksum.check_chunk(ctx, "sha256sum", 300) }
test test_bb_sha256sum_sha256sum_chunk_004 { |ctx| checksum.check_chunk(ctx, "sha256sum", 400) }
test test_bb_sha256sum_sha256sum_chunk_005 { |ctx| checksum.check_chunk(ctx, "sha256sum", 500) }
test test_bb_sha256sum_sha256sum_chunk_006 { |ctx| checksum.check_chunk(ctx, "sha256sum", 600) }
test test_bb_sha256sum_sha256sum_chunk_007 { |ctx| checksum.check_chunk(ctx, "sha256sum", 700) }
test test_bb_sha256sum_sha256sum_chunk_008 { |ctx| checksum.check_chunk(ctx, "sha256sum", 800) }
test test_bb_sha256sum_sha256sum_chunk_009 { |ctx| checksum.check_chunk(ctx, "sha256sum", 900) }
test test_bb_sha256sum_sha256sum_original_aggregate { |ctx| checksum.check_original_aggregate(ctx, "sha256sum", "8e1d3ed57ebc130f0f72508446559eeae06451ae6d61b1e8ce46370cfb8963c3") }

# origin: busybox sha256sum/sha256sum -c EMPTY
test test_bb_sha256sum_sha256sum_c_EMPTY_4cb981a2 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "EMPTY")
  uu.fails_with_code(uu.invoke(s, "sha256sum", ["-c", "EMPTY"])?, 1)
}
