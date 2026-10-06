type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, tool: Str, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "compress-run")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/{tool}.xsh".display(), "--"].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_compress_binary_streams { |ctx|
  let payload = b"hello\0\xff\xfe\n"
  for family in [{encode: "gzip", decode: "gunzip"}, {encode: "bzip2", decode: "bunzip2"}, {encode: "xz", decode: "unxz"}, {encode: "lzma", decode: "unlzma"}, {encode: "zstd", decode: "unzstd"}] {
    let packed = invoke(ctx, family.encode, ["-c"], payload)?
    assert packed.status == 0, packed.stderr
    let plain = invoke(ctx, family.decode, ["-c"], packed.stdout)?
    assert plain.status == 0, plain.stderr
    assert plain.stdout == payload
    let checked = invoke(ctx, family.encode, ["-t"], packed.stdout)?
    assert checked.status == 0, checked.stderr
    assert checked.stdout == b""
    let bad = invoke(ctx, family.encode, ["-t"], b"not compressed")?
    assert bad.status != 0
  }
}

test test_compress_concatenated_streams { |ctx|
  for family in [{encode: "gzip", decode: "zcat"}, {encode: "bzip2", decode: "bzcat"}, {encode: "xz", decode: "xzcat"}, {encode: "zstd", decode: "zstdcat"}] {
    let first = invoke(ctx, family.encode, ["-c"], b"first\0")?
    let second = invoke(ctx, family.encode, ["-c"], b"second\xff")?
    let plain = invoke(ctx, family.decode, [], bytes.concat([first.stdout, second.stdout]))?
    assert plain.status == 0, plain.stderr
    assert plain.stdout == b"first\0second\xff"
  }
}

test test_compress_file_replacement_and_keep { |ctx|
  for family in [{encode: "gzip", decode: "gunzip", suffix: ".gz"}, {encode: "bzip2", decode: "bunzip2", suffix: ".bz2"}, {encode: "xz", decode: "unxz", suffix: ".xz"}, {encode: "lzma", decode: "unlzma", suffix: ".lzma"}, {encode: "zstd", decode: "unzstd", suffix: ".zst"}] {
    let source = test.temp_file(ctx, name: f"replace-{family.encode}", contents: b"original\0payload")?
    let packed = fp"{source}{family.suffix}"
    let removal = if family.encode == "zstd" { ["--rm"] } else { [] }
    assert invoke(ctx, family.encode, removal.extend([source.display()]))?.status == 0
    assert ! source.exists()?
    assert packed.exists()?
    assert invoke(ctx, family.decode, ["-k", packed.display()])?.status == 0
    assert source.read_bytes()? == b"original\0payload"
    assert packed.exists()?
    assert invoke(ctx, family.decode, [packed.display()])?.status != 0
    assert packed.exists()?
    assert invoke(ctx, family.decode, removal.extend(["-f", packed.display()]))?.status == 0
    assert ! packed.exists()?
  }
}

test test_compress_corrupt_input_preserves_files { |ctx|
  let root = test.temp_dir(ctx, name: "corrupt-compress")?
  let source = fp"{root}/corrupt.gz"
  source.write("bad gzip")
  let plain = fp"{source.display().byte_slice(0, length: source.display().byte_len() - 3)}"
  plain.write("existing")
  let result = invoke(ctx, "gunzip", ["-f", source.display()])?
  assert result.status == 1
  assert source.read_bytes()? == b"bad gzip"
  assert plain.read_text()? == "existing"
}

test test_gzip_original_name_and_time { |ctx|
  let root = test.temp_dir(ctx, name: "gzip-name")?
  let original = fp"{root}/original"
  original.write("payload")
  fs.set_times(original, mtime_ns: 123000000000)
  assert invoke(ctx, "gzip", [original.display()])?.status == 0
  let encoded = fp"{root}/original.gz"
  let renamed = fp"{root}/renamed.gz"
  encoded.rename(to: renamed)
  fs.set_times(renamed, mtime_ns: 456000000000)
  assert invoke(ctx, "gunzip", ["-N", renamed.display()])?.status == 0
  assert original.read_text()? == "payload"
  assert fs.stat(original)?.mtime_ns == 123000000000
  assert ! fp"{root}/renamed".exists()?
}

test test_compress_levels_and_multiple_operands { |ctx|
  for tool in ["gzip", "bzip2", "xz", "lzma", "zstd"] {
    let low = invoke(ctx, tool, ["-1c"], b"levels")?
    assert low.status == 0, low.stderr
    assert invoke(ctx, tool, ["-dc"], low.stdout)?.stdout == b"levels"
    let high = invoke(ctx, tool, ["-9c"], b"levels")?
    assert high.status == 0, high.stderr
    assert invoke(ctx, tool, ["-dc"], high.stdout)?.stdout == b"levels"
  }
  let good = test.temp_file(ctx, name: "multiple", contents: b"good")?
  let missing = test.temp_path(ctx, name: "missing-compress")
  let result = invoke(ctx, "gzip", ["-c", missing.display(), good.display()])?
  assert result.status == 1
  assert invoke(ctx, "gunzip", ["-c"], result.stdout)?.stdout == b"good"
}

test test_compress_force_stdout_pass_through { |ctx|
  for tool in ["gzip", "bzip2", "xz", "zstd"] {
    let result = invoke(ctx, tool, ["-dcf"], b"plain\0\xff")?
    assert result.status == 0, result.stderr
    assert result.stdout == b"plain\0\xff"
    assert invoke(ctx, tool, ["-dc"], b"plain\0\xff")?.status != 0
    assert invoke(ctx, tool, ["-tf"], b"plain\0\xff")?.status != 0
  }
}

test test_compress_file_modes_and_timestamps { |ctx|
  for family in [{encode: "gzip", suffix: ".gz"}, {encode: "bzip2", suffix: ".bz2"}, {encode: "xz", suffix: ".xz"}, {encode: "lzma", suffix: ".lzma"}, {encode: "zstd", suffix: ".zst"}] {
    let source = test.temp_file(ctx, name: f"metadata-{family.encode}", contents: b"mode")?
    source.chmod(0o640)
    fs.set_times(source, mtime_ns: 123456789000)
    assert invoke(ctx, family.encode, [source.display()])?.status == 0
    let packed = fp"{source}{family.suffix}"
    let info = fs.stat(packed)?
    assert info.mode % 0o1000 == 0o640
    assert info.mtime_ns == 123456789000
  }
}

test test_zstd_levels_and_corrupt_frames { |ctx|
  for args in [["-1c"], ["-19c"], ["--ultra", "-22c"], ["--fast=3", "-c"]] {
    let packed = invoke(ctx, "zstd", args, b"zstd levels\0\xff")?
    assert packed.status == 0, packed.stderr
    let decoded = invoke(ctx, "unzstd", ["-c"], packed.stdout)?
    assert decoded.status == 0, decoded.stderr
    assert decoded.stdout == b"zstd levels\0\xff"
    assert invoke(ctx, "zstd", ["-t"], packed.stdout)?.status == 0
    # Own the shortened fixture before passing it through the child-command
    # helper; byte views currently lose their argument slot on that path.
    # Remove the copy once child-command binding preserves byte views.
    let truncated = bytes.concat([packed.stdout[0..packed.stdout.len() - 1]])
    assert invoke(ctx, "zstd", ["-t"], truncated)?.status == 1
  }
  assert invoke(ctx, "zstd", ["-20c"], b"x")?.status == 1
  assert invoke(ctx, "zstd", ["--fast=0", "-c"], b"x")?.status == 1
}

test test_zstd_keeps_files_by_default_and_skips_frames { |ctx|
  let root = test.temp_dir(ctx, name: "zstd-keep")?
  let source = fp"{root}/payload"
  source.write(b"zstd\0\xff")
  assert invoke(ctx, "zstd", [source.display()])?.status == 0
  assert source.read_bytes()? == b"zstd\0\xff"
  let packed = fp"{root}/payload.zst"
  let frame = packed.read_bytes()?
  let skipped = bytes.concat([b"\x50\x2a\x4d\x18\x04\0\0\0skip", frame])
  let decoded = invoke(ctx, "zstdcat", [], skipped)?
  assert decoded.status == 0, decoded.stderr
  assert decoded.stdout == b"zstd\0\xff"
  let damaged = bytes.concat([frame[0..frame.len() - 1], bytes.from_ints([((frame.byte_at(frame.len() - 1) ?? 0) + 1) % 256])?])
  assert invoke(ctx, "zstd", ["-t"], damaged)?.status == 1
}

test test_compression_native_paths_and_validation { |ctx|
  let root = test.temp_dir(ctx, name: "compression-api")?
  let source = fp"{root}/-"
  let packed = fp"{root}/packed.gz"
  let decoded = fp"{root}/decoded"
  source.write(b"literal dash\0\xff")
  compression.transform(source, packed, "gzip", metadata: false)
  assert source.read_bytes()? == b"literal dash\0\xff"
  compression.transform(packed, decoded, "gzip", decode: true)
  assert decoded.read_bytes()? == b"literal dash\0\xff"
  compression.transform(packed, null, "gzip", decode: true, test: true)
  test.error_kind(compression.transform(packed, decoded, "gzip", decode: true, test: true), "compression-transform")
  test.error_kind(compression.transform(source, null, "gzip", pass_through: true), "compression-transform")
  test.error_kind(compression.transform(source, packed, "bzip2", level: 0), "compression-transform")
}
