##! Checksums used by the sharded command-line integration tests.
##! Each case compares a bounded input range with committed digest rows so a
##! child-process failure stays local to a small set of inputs.
use uu

## Checks one hundred consecutive input lengths against their expected digest rows.
export proc check_chunk(ctx: TestContext, util: Str, start: Int) [fs, process, env, error] -> Result[Unit, Error] {
  assert start >= 0 and start % 100 == 0 and start + 100 <= 1000, f"invalid {util} checksum input range"
  let scene = uu.scene(ctx)?
  let expected = fp"{ctx.core_dir}/tests/data/checksum/{util}.txt".read_lines()?
  assert expected.len() == 1000, f"expected 1,000 {util} input digests"
  let block = bytes.from_text(["The quick brown fox jumps over the lazy dog\n" for _ in range(24)].join(""))
  for length in range(start, start + 100) {
    let result = uu.invoke(scene, util, [], stdin: block[0..length])?
    uu.succeeds(result)
    assert result.stdout == bytes.from_text(expected[length] + "\n"), f"{util} input length {length} stdout bytes differ"
  }
  Ok()
}

## Checks the original whole-fixture output against its established golden value.
export proc check_original_aggregate(ctx: TestContext, util: Str, expected: Str) [fs, process, env, error] -> Result[Unit, Error] {
  let scene = uu.scene(ctx)?
  let rows = fp"{ctx.core_dir}/tests/data/checksum/{util}.txt".read_bytes()?
  let total = uu.invoke(scene, util, [], stdin: rows)?
  uu.succeeds(total)
  uu.stdout_is(total, f"{expected}  -\n")
  Ok()
}
