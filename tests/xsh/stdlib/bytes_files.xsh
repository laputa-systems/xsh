test test_bytes_regular_file_guards_reject_special_files_without_mutation { |ctx|
  let root = test.temp_dir(ctx)?
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, mode: 384)?
  let script = test.temp_file(ctx, name: "fifo-guard.xsh", contents: bytes.from_text(r"""
let fifo = Path(args[0])
assert bytes.read_at(fifo, 0, 1, regular: true) is Err(_)
assert bytes.write_at(fifo, 0, b"x", regular: true) is Err(_)
assert bytes.zero_at(fifo, 0, 1, regular: true) is Err(_)
assert bytes.resize(fifo, 1, regular: true) is Err(_)
"""))?
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--", fifo.display()], timeout: 2s))?
  assert status.ok
  assert fs.stat(fifo)?.kind == "fifo"
  assert bytes.read_at(/dev/null, 0, 1, regular: true) is Err(_)
  assert bytes.write_at(/dev/null, 0, b"x", regular: true) is Err(_)
  assert bytes.zero_at(/dev/null, 0, 1, regular: true) is Err(_)
  assert bytes.resize(/dev/null, 1, regular: true) is Err(_)
  assert fs.stat(/dev/null)?.kind == "char"
}

test test_bytes_resize_preserves_existing_data_and_zero_fills_extension { |ctx|
  let file = test.temp_file(ctx, contents: b"abcdef")?
  bytes.resize(file, 9, regular: true)?
  assert bytes.read_at(file, 0, 9, regular: true)? == b"abcdef\0\0\0"
  assert bytes.write_at(file, 2, b"XY", regular: true)? == 2
  assert bytes.zero_at(file, 4, 2, regular: true)? == 2
  assert bytes.read_at(file, 0, 9, regular: true)? == b"abXY\0\0\0\0\0"
  bytes.resize(file, 3, regular: true)?
  assert file.read_bytes()? == b"abX"
  bytes.resize(file, 0, regular: true)?
  assert file.read_bytes()? == b""
}

test test_bytes_resize_creation_is_explicit_and_exclusive { |ctx|
  let file = test.temp_path(ctx, name: "image")
  assert bytes.resize(file, 8, regular: true) is Err(_)
  assert ! file.exists()?
  test.error_kind(bytes.resize(file, 8, exclusive: true, regular: true), "bytes-resize")
  assert ! file.exists()?
  test.error_kind(bytes.resize(file, -1, create: true, regular: true), "bytes-resize")
  assert ! file.exists()?
  bytes.resize(file, 8, create: true, exclusive: true, regular: true)?
  assert file.read_bytes()? == b"\0\0\0\0\0\0\0\0"
  assert bytes.write_at(file, 0, b"original", regular: true)? == 8
  assert bytes.resize(file, 1, create: true, exclusive: true, regular: true) is Err(_)
  assert file.read_bytes()? == b"original"
}

test test_bytes_file_guards_follow_read_symlinks_and_refuse_mutating_symlinks { |ctx|
  let root = test.temp_dir(ctx)?
  let target = fp"{root}/target"
  target.write("original")
  let link = fp"{root}/link"
  link.symlink(to: target)
  assert bytes.read_at(link, 0, 8, regular: true)? == b"original"
  assert bytes.write_at(link, 0, b"changed", regular: true) is Err(_)
  assert bytes.zero_at(link, 0, 8, regular: true) is Err(_)
  assert bytes.resize(link, 1, regular: true) is Err(_)
  assert target.read_text()? == "original"
  let null_link = fp"{root}/null-link"
  null_link.symlink(to: /dev/null)
  assert bytes.read_at(null_link, 0, 1, regular: true) is Err(_)
}

test test_bytes_dash_is_a_literal_path { |ctx|
  let root = test.temp_dir(ctx)?
  cd root {
    bytes.resize(p"-", 4, create: true, exclusive: true, regular: true)?
    assert bytes.write_at(p"-", 0, b"file", regular: true)? == 4
    assert bytes.read_at(p"-", 0, 4, regular: true)? == b"file"
  }
  assert fp"{root}/-".read_bytes()? == b"file"
}
