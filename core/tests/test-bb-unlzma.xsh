use support.uu

# origin: busybox unlzma/unlzma (bad archive 1)
test test_bb_unlzma_unlzma_bad_archive_1_d8f3c81c { |ctx|
  let s = uu.scene(ctx)?
  let input = fp"{s.ctx.core_dir}/tests/data/busybox/unlzma/unlzma_issue_1.lzma".read_bytes()?
  let r = uu.invoke(s, "unlzma", [], stdin: input)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "unlzma: corrupted data\n")
}

# origin: busybox unlzma/unlzma (bad archive 2)
test test_bb_unlzma_unlzma_bad_archive_2_a3f8919f { |ctx|
  let s = uu.scene(ctx)?
  let input = fp"{s.ctx.core_dir}/tests/data/busybox/unlzma/unlzma_issue_2.lzma".read_bytes()?
  let r = uu.invoke(s, "unlzma", [], stdin: input)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "unlzma: corrupted data\n")
}

# origin: busybox unlzma/unlzma (bad archive 3)
test test_bb_unlzma_unlzma_bad_archive_3_b6219811 { |ctx|
  let s = uu.scene(ctx)?
  let input = fp"{s.ctx.core_dir}/tests/data/busybox/unlzma/unlzma_issue_3.lzma".read_bytes()?
  let r = uu.invoke(s, "unlzma", [], stdin: input)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "unlzma: corrupted data\n")
}

