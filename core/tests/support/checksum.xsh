##! Checksums used by the sharded command-line integration tests.
##! Each case compares a bounded input range with committed digest rows so a
##! child-process failure stays local to a small set of inputs.
use uu

## Checks one hundred file lengths and the stdin padding boundaries against committed digest rows.
export proc check_chunk(ctx: TestContext, util: Str, start: Int) [fs, process, env, error] -> Result[Unit, Error] {
  assert start >= 0 and start % 100 == 0 and start + 100 <= 1000, f"invalid {util} checksum input range"
  let scene = uu.scene(ctx)?
  let expected = fp"{ctx.core_dir}/tests/data/checksum/{util}.txt".read_lines()?
  assert expected.len() == 1000, f"expected 1,000 {util} input digests"
  let block = bytes.from_text(["The quick brown fox jumps over the lazy dog\n" for _ in range(24)].join(""))
  var names = []
  var rows = []
  for length in range(start, start + 100) {
    let name = f"input-{length}"
    uu.write_bytes(scene, name, block[0..length])
    names += [name]
    assert expected[length].ends_with("  -"), f"invalid {util} digest row for length {length}"
    rows += [expected[length].replace("  -", with: f"  {name}") + "\n"]
  }
  let files = uu.invoke(scene, util, names)?
  uu.succeeds(files)
  uu.stdout_is(files, rows.join(""))
  uu.stderr_is(files, "")
  for length in [0, 1, 55, 56, 57, 63, 64, 65, 111, 112, 113, 127, 128, 129, 255, 256, 257, 511, 512, 513, 999] {
    continue when length < start or length >= start + 100
    let result = uu.invoke(scene, util, [], stdin: block[0..length])?
    uu.succeeds(result)
    assert result.stdout == bytes.from_text(expected[length] + "\n"), f"{util} input length {length} stdout bytes differ"
    uu.stderr_is(result, "")
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
