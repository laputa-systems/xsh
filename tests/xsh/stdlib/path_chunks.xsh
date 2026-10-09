test test_path_chunks_preserves_bytes_with_bounded_reads { |ctx|
  let root = test.temp_dir(ctx, name: "path-chunks")?
  let input = fp"{root}/input.bin"
  let contents = b"ab\0\nxyz"
  input.write(contents)

  let chunks = input.chunks(3)?.collect()
  assert chunks.len() > 0
  for chunk in chunks {
    assert chunk.len() > 0
    assert chunk.len() <= 3
  }
  assert bytes.concat(chunks) == contents

  let empty = fp"{root}/empty"
  empty.write(b"")
  assert empty.chunks(3)?.collect() == []
}

test test_path_chunks_validates_limit_and_open_errors { |ctx|
  let root = test.temp_dir(ctx, name: "path-chunks-errors")?
  let input = fp"{root}/input.bin"
  input.write(b"x")

  assert input.chunks(0) is Err(_)
  assert input.chunks(-1) is Err(_)
  assert input.chunks(1048577) is Err(_)
  assert fp"{root}/missing".chunks(1) is Err(_)
}

test test_path_chunks_reads_unbounded_devices_lazily { |ctx|
  let zero = p"/dev/zero"
  if zero.exists()? {
    let chunks = zero.chunks(4)? |> take(3) |> collect()
    assert chunks == [b"\0\0\0\0", b"\0\0\0\0", b"\0\0\0\0"]

    let looped = test.run_script(
      ctx,
      r"""proc main() [fs, io, error] {
  for chunk in p"/dev/zero".chunks(4)? {
    print chunk.len()
    break
  }
}
""",
    )?
    assert looped.success, looped.stderr
    assert looped.stdout == "4\n"
  }
}
