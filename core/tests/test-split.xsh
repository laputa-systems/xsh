test test_split_lines { |ctx|
  let root = test.temp_dir(ctx, name: "split")?
  let input = fp"${root}/input.txt"

  input.write("""a
b
c
""")?

  let prefix = fp"${root}/chunk-"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/split.xsh" -- -l 2 $input $prefix ?

  """a
b""" in fp"${root}/chunk-aa".read_text()?

  "c" in fp"${root}/chunk-ab".read_text()?
}


test test_split_bytes_clamps_final_chunk { |ctx|
  let root = test.temp_dir(ctx, name: "split-bytes")?
  let input = fp"${root}/input.bin"
  input.write(b"abcdefg")?

  let prefix = fp"${root}/chunk-"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/split.xsh" -- -b 3 $input $prefix ?

  fp"${root}/chunk-aa".read_bytes()? == b"abc"
  fp"${root}/chunk-ab".read_bytes()? == b"def"
  fp"${root}/chunk-ac".read_bytes()? == b"g"
}

test test_split_bytes_large_count_preserves_entire_input { |ctx|
  let root = test.temp_dir(ctx, name: "split-large-byte-count")?
  let input = fp"${root}/input.bin"
  input.write(b"abcdefg")?

  let prefix = fp"${root}/chunk-"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/split.xsh" -- -b 9223372036854775807 $input $prefix ?

  fp"${root}/chunk-aa".read_bytes()? == b"abcdefg"
  !fp"${root}/chunk-ab".exists()?
}
