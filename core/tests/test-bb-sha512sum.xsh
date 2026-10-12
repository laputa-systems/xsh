use support.checksum
use support.uu

# origin: busybox sha512sum/sha512sum
test test_bb_sha512sum_sha512sum_chunk_000 { |ctx| checksum.check_chunk(ctx, "sha512sum", 0) }
test test_bb_sha512sum_sha512sum_chunk_001 { |ctx| checksum.check_chunk(ctx, "sha512sum", 100) }
test test_bb_sha512sum_sha512sum_chunk_002 { |ctx| checksum.check_chunk(ctx, "sha512sum", 200) }
test test_bb_sha512sum_sha512sum_chunk_003 { |ctx| checksum.check_chunk(ctx, "sha512sum", 300) }
test test_bb_sha512sum_sha512sum_chunk_004 { |ctx| checksum.check_chunk(ctx, "sha512sum", 400) }
test test_bb_sha512sum_sha512sum_chunk_005 { |ctx| checksum.check_chunk(ctx, "sha512sum", 500) }
test test_bb_sha512sum_sha512sum_chunk_006 { |ctx| checksum.check_chunk(ctx, "sha512sum", 600) }
test test_bb_sha512sum_sha512sum_chunk_007 { |ctx| checksum.check_chunk(ctx, "sha512sum", 700) }
test test_bb_sha512sum_sha512sum_chunk_008 { |ctx| checksum.check_chunk(ctx, "sha512sum", 800) }
test test_bb_sha512sum_sha512sum_chunk_009 { |ctx| checksum.check_chunk(ctx, "sha512sum", 900) }
test test_bb_sha512sum_sha512sum_original_aggregate { |ctx| checksum.check_original_aggregate(ctx, "sha512sum", "fe413e0f177324d1353893ca0772ceba83fd41512ba63895a0eebb703ef9feac2fb4e92b2cb430b3bda41b46b0cb4ea8307190a5cc795157cfb680a9cd635d0f") }

# origin: busybox sha512sum/sha512sum -c EMPTY
test test_bb_sha512sum_sha512sum_c_EMPTY_820a2c1d { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "EMPTY")?
  uu.fails_with_code(uu.invoke(s, "sha512sum", ["-c", "EMPTY"])?, 1)
}
