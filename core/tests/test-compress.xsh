type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs one applet script with `dir` as its working directory; the captured
# streams live elsewhere so listing `dir` shows only what the applet made.
proc invoke_in(ctx: TestContext, tool: Str, args: List[Str], dir: Path, input = b"") [fs, process, error] -> Result[Ran] {
  let capture = test.temp_dir(ctx, name: "compress-capture")?
  let out = fp"{capture}/out"
  let err = fp"{capture}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/{tool}.xsh".display(), "--"].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, dir, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc invoke(ctx: TestContext, tool: Str, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  invoke_in(ctx, tool, args, test.temp_dir(ctx, name: "compress-run")?, input)
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

type Family = {tool: Str, decoder: Str, cat: Str, suffix: Str, text: Str, empty: Str, bin: Str, lines: Str, big: Str}
type Variant = {tool: Str, what: Str, data: Str}
type Member = {tool: Str, data: Str}

# Reference payloads. `lines` is a few KiB of repetitive text and `big` is
# 250000 bytes of one repeating line, which the block-based codecs split
# across several blocks.
pure plain_text() -> Bytes { b"hello compression\n" }

pure plain_bin() -> Bytes { b"hello\0\xff\xfe\n" }

pure plain_lines() -> Bytes {
  bytes.concat(collect { for n in range(1, 201) { yield bytes.from_text(f"line {n}: the quick brown fox\n") } })
}

pure plain_big() -> Bytes {
  let chunk = bytes.concat(collect { for _ in range(400) { yield b"0123456789abcdefghijklmn\n" } })
  bytes.concat(collect { for _ in range(25) { yield chunk } })
}

# The fixtures below were written by gzip 1.15, bzip2 1.0.8, xz 5.8.4 (also for
# lzma) and zstd 1.5.7 from the payloads above: gzip -n (-9 for lines, --fast
# for big), bzip2 (-9 for lines, -1 for big, so big spans several blocks),
# xz -9e for lines and --block-size=100000 for big, lzma -9 for lines, and
# zstd -19. They are the real tools' bytes, not XSH output, so decoding them is
# a one-way differential check that needs no tool at test time.
pure families() -> List[Family] {
  [
    {tool: "gzip", decoder: "gunzip", cat: "zcat", suffix: ".gz",
     text: "H4sIAAAAAAAAA8tIzcnJV0jOzy0oSi0uzszP4wIAnss+shIAAAA=",
     empty: "H4sIAAAAAAAAAwMAAAAAAAAAAAA=",
     bin: "H4sIAAAAAAAAA8tIzcnJZ/j/jwsAIYGJZQkAAAA=",
     lines: "H4sIAAAAAAACA4XYy00kQRRE0T1WlAkZ+U/MGQQCgUCgGTHmIwzgsI7dUXfVrffy9Hp/5fb6+3h/vf97unu+/ny8fb5eD2//b16+t4qtYevYBraJbWHb2A62FI2SiWgimwgn0ol4Ip8IKBKqEqr87UioSqhKqEqoSqhKqEqoSqhJqEmo8e8loSahJqEmoSahJqEmoS6hLqEuoc4nkIS6hLqEuoS6hLqEhoSGhIaEhoQGH9ISGhIaEhoSGhKaEpoSmhKaEpoSmnyPSWhKaEpoSmhJaEloSWhJaEloSWjxVS+hJaEloS2hLaEtoS2hLaEtoS2hzRqS0JbQkdCR0JHQkdCR0JHQkdCR0GEwuhiZjIXNWBiNhdVYmI2F3VgYjoXlWJiOhVa/5DWtHNgubCe2G9uR7cp2ZrOzw9BO9bcIrdjaYWyHtR3mdtjbYXCHxR0md9jcaf5woxWzO+zuMLzD8g7TO2zvML7D+g7zO91fubRigYcJHjZ4GOFhhYcZHnZ4GOJhiWf4JEArxnhY42GOhz0eBnlY5GGSh00eRnmm7ye0YpeHYR6WeZjmYZuHcR7WeZjnYZ9n+dhEKyZ62OhhpIeVHmZ62OlhqIelHqZ6ti9ztGKth7ke9noY7GGxh8keNnsY7WG15/iM6TvmD1ZffSsDsAQXAAA=",
     big: "H4sIAAAAAAAEA+3ZuRWAIBAFwNxq8EItxxMVtP/QQpyYjAd7/Al103Z9HMZpXtZtP9J53bk8bxUcuBKPwTdQGdRE3UCDNBoYisyJJmS7gXXJomhFlhrISyRFwjOxocBUhiw95wbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMSEmxISYEBNiQkyICTEhJsSEmBATYkJMiAkxISbEhJgQE2JCTIgJMfmHmHxYcztVkNADAA=="},
    {tool: "bzip2", decoder: "bunzip2", cat: "bzcat", suffix: ".bz2",
     text: "QlpoOTFBWSZTWYwpdkUAAALRgAAQQAAKZ9gAIAAxANABADQFW07HVgJLzeA8XckU4UJCMKXZFA==",
     empty: "QlpoORdyRThQkAAAAAA=",
     bin: "QlpoOTFBWSZTWdoIgm8AAAHBAMAQAkSAAaAAIgNohDAgYh1J4u5IpwoSG0EQTeA=",
     lines: "QlpoOTFBWSZTWbyeazsAB21ZgAAQQAB/8BtttsBAAjzgOGBQADQAAGTwJ79UqAAAGP/VUkH/qn+qqgAAaBSpUB+1SBoAaepwIuYi+BF9CLqEXMRbzUZp1cvDx8vP055ve973vfNOWqa1TWqa1TWueeDnng554OeeDnngupVVa2223r2AAAuq6qq2tttu/UAAC6q8VVtrbbd+gAAF1VeVVttbbd+YAAF1VXnVttrbd+QAAF1VV6Vtttbd+AAALqqq9bbbbW76AAAuqqq222212AADh11eJmZmZmIiIiIgANABCgBqVAZFgMBcFwYCwGRUBqUAIaQ0oAalQGRYDAXBcGAsBkVAzvzNZmZmZm8chF8iLzEXyIusRdBF/CL8EX2IvoRcCLBFwIugiwRdBFoRcCLgIv8XckU4UJC8nms7",
     big: "QlpoMTFBWSZTWcmcA6YAD59JAAAQf+A//zAA+AKAAaBkyCgAGgZMgTVVBpoaDINNNgUG4FBjwqpBxqpBhVSDlVSDGqkGAKDmCg6AoOoKDIFB2BQdwUHgFB5BQZgoNAUHoFB7BQfAUH0FBqCg/AoNgUH8xQVkmU1kMeUmLAD58SQAAEH/gP/8wAPgCgAGgZMgoABoGTIE1VQZAZGgZNgUG4FBvBQcAUHEFBgCgxBQcgUGXSqkHWqkHaqkHeqkGVVIMAUGYKDwCg8goNAUHoFB7BQfAUH0FB+BQagoNgUH8xQVkmU1mCXl0HADLPSQAAEH/gP/8wANAKAAaBkyBNVRAGmgDTCgAGgZMjWUgwVIMVSDcqQb1SDgqQcVSDkqQc1SDoqQdVSDJUg7KkGWapB3lIPEpB5lIPUpB7lIPkpB9lIP0pBpKQf5SDWUg2F3JFOFCQvNzAig"},
    {tool: "xz", decoder: "unxz", cat: "xzcat", suffix: ".xz",
     text: "/Td6WFoAAATm1rRGBMAWEiEBFgAAAAAAAAAAAKdRd54BABFoZWxsbyBjb21wcmVzc2lvbgoAAABsh2AL6pd1XgABMhISkE8+H7bzfQEAAAAABFla",
     empty: "/Td6WFoAAATm1rRGAAAAABzfRCEftvN9AQAAAAAEWVo=",
     bin: "/Td6WFoAAATm1rRGBMANCSEBFgAAAAAAAAAAAF9PM+QBAAhoZWxsbwD//goAAAAAukNT5PEy2c0AASkJZJIcHR+2830BAAAAAARZWg==",
     lines: "/Td6WFoAAATm1rRGBMCaAoQuIQEcAAAAAAAAAM2Yh8DgFwMBEl0ANhpKHwigLARuDWy4pe1jnI582072nkt4GFZc9x8wdtbZxm6/0Fg3tn3B+sJ0bUu9SJJV8lRv9sKJBLw0Er9ezAZYkS+OLCRBKsjf44BT9jJblP/zrA8mS+YbWCARIh+bZkiPTZiWHUQCEITKijG+8gP+0A4GyBKG3rySmILV+mqAO1uQF5rF/4EggbhflGggTk8HTRUuLBK78mVDmgaUqGo1XMMwpMlOZP/Z4S2nSJhH7n+g+WUJsF2OHY4mIaDFQXmg7uFWVKA1USNcBjhYyBY+dprmRLW5a1A31pktSfft8bwytBBKa/UwFlpgEx8+DVteTiglB/n8x0GpW2NlfGk1/6DxQWihV7XuWO6y8MO4AAAAAFZCFtLUkPMSAAG2AoQuAABTPe4lscRn+wIAAAAABFla",
     big: "/Td6WFoAAATm1rRGA8B2oI0GIQEWAAAAjR0dneGGnwBuXQAYDEKSame8DtEzLOTMQinAm5xVexExtFq809bAxmrm6Y6WT8ADAOWBBA60lf4EgUUq7wC/zkrwc7CprGp5eJLOmdQFVx7Lhi1BVSOc1HbnnuCVzupNPqq4VpwFKUCGXD2/wn9r6gMdJT4xm64AAAAAAA1OT+8pTVwwA8B2oI0GIQEWAAAAjR0dneGGnwBuXQAYDEKSame8DtEzLOTMQinAm5xVexExtFq809bAxmrm6Y6WT8ADAOWBBA60lf4EgUUq7wC/zkrwc7CprGp5eJLOmdQFVx7Lhi1BVSOc1HbnnuCVzupNPqq4VpwFKUCGXD2/wn9r6gMdJT4xm64AAAAAAA1OT+8pTVwwA8Bq0IYDIQEWAAAAz4x+X+DDTwBiXQAYDEKSame8DtEzLOTMQinAm5xVexExtFq809bAxmrm6Y6WT8ADAOWBBA60lf4EgUUq7wC/zkrwc7CprGp5eJLOmdQFVx7Lhi1BVSOc1HbnnuCVzupNPqq4VpwFKUCEAMrxAAAAABOQQaHzJlToAAOOAaCNBo4BoI0GggHQhgMAAAC8JhtECfRi5gUAAAAABFla"},
    {tool: "lzma", decoder: "unlzma", cat: "lzcat", suffix: ".lzma",
     text: "XQAAgAD//////////wA0GUnujekSFAmXrhSOHJCt2FjI+RB///JhQAA=",
     empty: "XQAAgAD//////////wCD//v//8AAAAA=",
     bin: "XQAAgAD//////////wA0GUnujdfFuNlicDv///woYAA=",
     lines: "XQAAAAT//////////wA2GkofCKAsBG4NbLil7WOcjnzbTvaeS3gYVlz3HzB21tnGbr/QWDe2fcH6wnRtS71IklXyVG/2wokEvDQSv17MBliRL44sJEEqyN/jgFP2MluU//OsDyZL5htYIBEiH5tmSI9NmJYdRAIQhMqKMb7yA/7QDgbIEobevJKYgtX6aoA7W5AXmsX/gSCBuF+UaCBOTwdNFS4sErvyZUOaBpSoajVcwzCkyU5k/9nhLadImEfuf6D5ZQmwXY4djiYhoMVBeaDu4VZUoDVRI1wGOFjIFj52muZEtblrUDfWmS1J9+3xvDK0EEpr9TAWWmATHz4NW15OKCUH+fzHQalbY2V8aTX/oPFBaKFXte5Y7rL+nHq+//yRoW4=",
     big: "XQAAgAD//////////wAYDEKSame8DtEzLOTMQinAm5xVexExtFq809bAxmrm6Y6WT8ADAOWBBA60lf4EgUUq7wC/zkrwc7CprGp5eJLOmdQFVx7Lhi1BVSOc1HbnnuCVzupNPqq4VpwFKUCGXD2/wn9r6gMdJUQ4idX3qiaZ8lXHXjgPQye5jtpm4U67ZruVpf//9KUAAA=="},
    {tool: "zstd", decoder: "unzstd", cat: "zstdcat", suffix: ".zst",
     text: "KLUv/SQSkQAAaGVsbG8gY29tcHJlc3Npb24KpPA27Q==",
     empty: "KLUv/SQAAQAAmenYUQ==",
     bin: "KLUv/SQJSQAAaGVsbG8A//4KiiNKOQ==",
     lines: "KLUv/WQEFoUIABZVMRaATTrAiOM4jsME1UVSSilTSg/hxlHpMgApACUALsucFc1dlp8VzV0WnxXNXZaeFc1dFp4VzV1AZFFIFJBFQ5AgBocHIyEYHEmACUFxSATE4WmWZOEVK1WoTHlxaWEphWdFc5flzormLoudFc1dljormrssdFY0D2NTQzPz8XQ4Ze72On3++z1ffjaXyePbruniq7VKnb5uy5YejcW9s6ujm/v5erxyZ2ZlZGM3W41W7Mqqimrq5WqxSh0ZFRENnUwlUqELgMioEdD19n8D4KV2AxEkBP8rBB0/u1sCIIAgAAIBEAiAQAAEAiAQcHi4wn2DABmDDwFnBz4Ejh34CDhzMMBrY2EsqIXwwK4Cp/w0jQ==",
     big: "KLUv/aSQ0AMADAEAyDAxMjM0NTY3ODlhYmNkZWZnaGlqa2xtbgoBAJH/cz7HRQAAAAEAjdCcTyAB4nWB"},
  ]
}

# Archives written by the reference tools with options that change the
# container layout; each must decode to the lines payload.
pure variants() -> List[Variant] {
  [
    {tool: "gzip", what: "gzip -1 (fixed and stored blocks)", data: "H4sIAAAAAAAEA4XWy20VQQAAwTtRvBC2Zz+zQzggIywsWyAQhI8IwMW5b3Xql+fXp0cfHz+/Pj2+/3r+/O3x6cfb79fHl7c/H17+tYG2ox1oJ9qFNtFutIXWpiiZRJNsEk7SSTzJJwEloSGhIaEhoSGhIaEhoSGhIaEhoSGhXUK7hHYJ7RLaJbRLaJfQLqFdQruEDgkdEjokdEjokNAhoUNCh4QOCR0SOiV0SuiU0CmhU0KnhE4JnRI6JXRK6JLQJaFLQpeELgldErokdEnoktAloSmhKaEpoSmhKaEpoSmhKaEpoSmhW0K3hG4J3RK6JXRL6JbQLaFbQreEloSWhJaEloSWhJaEloSWhJaEloTaRNQmozYhtUmpTUxtcmoTVJuk2kTVRqv/7DWtPNg+bC+2H9uT7cv2ZvOz42jH046rHV87zna87bjb8bfjcMfjjssdnztOd7zuuN3xu+N4x/OO6x3fO853vO+43/G/44DHA48LHh88Tni88Ljh8cPjiMcTjyseXzzOeLzxuOPxx+OQxyOPSx6fPE55vPK45fHL45jHM49rHt88znm887jn8c/joMdDj4seHz1Oerz0uOnx0+Oox1OPqx5fPc56vPW46/HX47DHY4/LHp89Tnu89rjt8dvHe9/+F30rA7AEFwAA"},
    {tool: "gzip", what: "gzip with stored name and time", data: "H4sICOmaymoAA2xpbmVzAIXWy20VQQBFwT1RvBDm9Hx6mnBARlhYtkAgCB8RgIv13R31p16eX58efXz8/Pr0+P7r+fO3x6cfb79fH1/e/nx4+bcNbDu2A9uJ7cI2sd3YFrY2jSqT0qQ2KU6qk/KkPilQKjRUaPDsqNBQoaFCQ4WGCg0VGio0VGhXoV2Fdl4vFdpVaFehXYV2FdpVaFehQ4UOFTpU6OALpEKHCh0qdKjQoUKHCp0qdKrQqUKnCp18pFXoVKFThU4VOlXoUqFLhS4VulToUqGL/5gKXSp0qdClQlOFpgpNFZoqNFVoqtDkV69CU4WmCt0qdKvQrUK3Ct0qdKvQrUI3NaRCtwotFVoqtFRoqdBSoaVCS4WWCi2C0WIkGTeacSMaN6pxIxs3unEjHDfKcSMdN7b6D6/ZysC2sE1sG9vItrLNbDo7QjtKO1I7WjtiO2o7cjt6O4I7ijuSO5o7ojuqO7I7ujvCO8o70jvaO+I76jvyO/o7AjwKPBI8GjwiPCo8Mjw6PEI8SjxSPFo8YjxqPHI8ejyCPIo8kjyaPKI8qjyyPLo8wjzKPNI82jziPOo88jz6PAI9Cj0SPRo9Ij0qPTI9Oj1CPUo9Uj1aPWI9aj1yPXo9gj2KPZI9mj2iPao9sj26fbzn9r99KwOwBBcAAA=="},
    {tool: "xz", what: "xz --check=crc32", data: "/Td6WFoAAAFpIt42BMCaAoQuIQEWAAAAAAAAAAWER1HgFwMBEl0ANhpKHwigLARuDWy4pe1jnI582072nkt4GFZc9x8wdtbZxm6/0Fg3tn3B+sJ0bUu9SJJV8lRv9sKJBLw0Er9ezAZYkS+OLCRBKsjf44BT9jJblP/zrA8mS+YbWCARIh+bZkiPTZiWHUQCEITKijG+8gP+0A4GyBKG3rySmILV+mqAO1uQF5rF/4EggbhflGggTk8HTRUuLBK78mVDmgaUqGo1XMMwpMlOZP/Z4S2nSJhH7n+g+WUJsF2OHY4mIaDFQXmg7uFWVKA1USNcBjhYyBY+dprmRLW5a1A31pktSfft8bwytBBKa/UwFlpgEx8+DVteTiglB/n8x0GpW2NlfGk1/6DxQWihV7XuWO6y8MO4AAAAAH0rA7AAAbIChC4AAEV/f74+MA2LAgAAAAABWVo="},
    {tool: "xz", what: "xz --check=sha256", data: "/Td6WFoAAArh+wyhBMCaAoQuIQEWAAAAAAAAAAWER1HgFwMBEl0ANhpKHwigLARuDWy4pe1jnI582072nkt4GFZc9x8wdtbZxm6/0Fg3tn3B+sJ0bUu9SJJV8lRv9sKJBLw0Er9ezAZYkS+OLCRBKsjf44BT9jJblP/zrA8mS+YbWCARIh+bZkiPTZiWHUQCEITKijG+8gP+0A4GyBKG3rySmILV+mqAO1uQF5rF/4EggbhflGggTk8HTRUuLBK78mVDmgaUqGo1XMMwpMlOZP/Z4S2nSJhH7n+g+WUJsF2OHY4mIaDFQXmg7uFWVKA1USNcBjhYyBY+dprmRLW5a1A31pktSfft8bwytBBKa/UwFlpgEx8+DVteTiglB/n8x0GpW2NlfGk1/6DxQWihV7XuWO6y8MO4AAAAALYq30mN8OdynOMQpE3sbmHtRd2qrwBFDWs3Edyc+NHjAAHOAoQuAAD/sp/CtunfHAIAAAAAClla"},
    {tool: "xz", what: "xz --check=none", data: "/Td6WFoAAAD/EtlBBMCaAoQuIQEWAAAAAAAAAAWER1HgFwMBEl0ANhpKHwigLARuDWy4pe1jnI582072nkt4GFZc9x8wdtbZxm6/0Fg3tn3B+sJ0bUu9SJJV8lRv9sKJBLw0Er9ezAZYkS+OLCRBKsjf44BT9jJblP/zrA8mS+YbWCARIh+bZkiPTZiWHUQCEITKijG+8gP+0A4GyBKG3rySmILV+mqAO1uQF5rF/4EggbhflGggTk8HTRUuLBK78mVDmgaUqGo1XMMwpMlOZP/Z4S2nSJhH7n+g+WUJsF2OHY4mIaDFQXmg7uFWVKA1USNcBjhYyBY+dprmRLW5a1A31pktSfft8bwytBBKa/UwFlpgEx8+DVteTiglB/n8x0GpW2NlfGk1/6DxQWihV7XuWO6y8MO4AAAAAAABrgKELgAApb1ryqgACvwCAAAAAABZWg=="},
    {tool: "xz", what: "xz --lzma2=preset=0", data: "/Td6WFoAAATm1rRGA8CBAoQuIQEMAAAALPtmeeAXAwD5XQA2GkofCKAsBG4NbLil7WOcjnzbTvaeS3gYVlz3HzB21tnGbr/QWDe2fcH6wnRtS71IklXyVG/2wokEvDQSv17MBliRL44sJEEqv25rdT53OIM55vEueg8WvHJEgUmm03GHynkj5Tn7kmaExbsiBhUr8Q8mT3Jc+nudGRn8ZFpr1l4c6nZGxj8pP7wEZFIJlw9+6sMjlvuc1kYgVR/SeeyJA2pKt8fliDP4tjVpUG6Dvc4VmgBPEFXesZX3AaOReBVgrRv2eUpo/WMOT+qkwqQBVo/tBHl2HGgXJxgRTxSbthTeBlv9LbcIUmmvz5Ab8hp8/BX5M4JhpAAAAAAAVkIW0tSQ8xIAAZkChC4AALCKFNOxxGf7AgAAAAAEWVo="},
    {tool: "zstd", what: "zstd --no-check", data: "KLUv/WAEFt0HAFJOIxmAKWkDwNjHrBGh7IaBeYUiu3dK6dI3jhUM0ru7u7s7MzMzM7OqqqqqKiIiIiKimZmZmZn////ftm3bbtu2bUmSJCkiIiIiwru7u7s7MzMzM7OqqqqqKiIiIiKimZmZmZn////ftm3bbtu2bUmSJCk8K5q7AA0JgoFgSCQIDIOBwwLCKAoURgBHgTgcBIDIqCH49r8DwKNyNREkBP8nBBKejwEbW6ultVpbq9W1Wl2r1bZaravB7oVsseUfJR+lHqU8yjvKOkpzlHCUb5S6NhIIAAAABAAQACAAQADglJAkZISIkBACQj6IB+kgFQGMBcPCshJIPNCq"},
    {tool: "zstd", what: "zstd --fast=5", data: "KLUv/WQEFu0jAERCbGluZSAxOiB0aGUgcXVpY2sgYnJvd24gZm94CmxpbmUgMjogdGhlIHF1aWNrIGJyb3duIGZveApsaW5lIDM6IHRoZSBxdWljayBicm93biBmb3gKbGluZSA0NTogdGg2OiB0aDc6IHRoODogdGg5OiB0aDEwMTEyOiB0aDM6IHRoNDogdGg1OiB0aDY6IHRoNzogdGg4OiB0aDk6IHRoMjA6IHQxOiB0aDI6IHRoMzogdGg0OiB0aDU6IHRoNjogdGg3OiB0aDg6IHRoOTogdGgzMDogdDE6IHRoMjogdGgzOiB0aDQ6IHRoNTogdGg2OiB0aDc6IHRoODogdGg5OiB0aDQwOiB0MTogdGgyOiB0aDM6IHRoNDogdGg1OiB0aDY6IHRoNzogdGg4OiB0aDk6IHRoNTA6IHQxOiB0aDI6IHRoMzogdGg0OiB0aDU6IHRoNjogdGg3OiB0aDg6IHRoOTogdGg2MDogdDE6IHRoMjogdGgzOiB0aDQ6IHRoNTogdGg2OiB0aDc6IHRoODogdGg5OiB0aDcwOiB0MTogdGgyOiB0aDM6IHRoNDogdGg1OiB0aDY6IHRoNzogdGg4OiB0aDk6IHRoODA6IHQxOiB0aDI6IHRoMzogdGg0OiB0aDU6IHRoNjogdGg3OiB0aDg6IHRoOTogdGg5MDogdDE6IHRoMjogdGgzOiB0aDQ6IHRoNTogdGg2OiB0aDc6IHRoODogdGg5OiB0aDEwMDEwMTI6IHRoMzogdGg0OiB0aDU6IHRoNjogdGg3OiB0aDg6IHRoOTogdGgxMDogdDE6IHRoMjogdGgzOiB0aDQ6IHRoNTogdGg2OiB0aDc6IHRoODogdGg5OiB0aDIwOiB0MTogdGgyOiB0aDM6IHRoNDogdGg1OiB0aDY6IHRoNzogdGg4OiB0aDk6IHRoMzA6IHQxOiB0aDI6IHRoMzogdGg0OiB0aDU6IHRoNjogdGg3OiB0aDg6IHRoOTogdGg0MDogdDE6IHRoMjogdGgzOiB0aDQ6IHRoNTogdGg2OiB0aDc6IHRoODogdGg5OiB0aDUwOiB0MTogdGgyOiB0aDM6IHRoNDogdGg1OiB0aDY6IHRoNzogdGg4OiB0aDk6IHRoNjA6IHQxOiB0aDI6IHRoMzogdGg0OiB0aDU6IHRoNjogdGg3OiB0aDg6IHRoOTogdGg3MDogdDE6IHRoMjogdGgzOiB0aDQ6IHRoNTogdGg2OiB0aDc6IHRoODogdGg5OiB0aDgwOiB0MTogdGgyOiB0aDM6IHRoNDogdGg1OiB0aDY6IHRoNzogdGg4OiB0aDk6IHRoOTA6IHQxOiB0aDI6IHRoMzogdGg0OiB0aDU6IHRoNjogdGg3OiB0aDg6IHRoOTogdGgyMDA6IIDFqBEkCAF+/8/QG2kOEfyfEGRonDpYM97QGBqMoWEaGsbQYQyNxtCwDA1j2LDoiIFU67e+NCdDBhJjZWggY0yGDSTGydBAxfC3A0QFyP+yFgNKu/5dBaf8NI0="},
  ]
}

# Two members written by the reference tools and concatenated: text then lines.
pure members() -> List[Member] {
  [
    {tool: "gzip", data: "H4sIAAAAAAAAA8tIzcnJV0jOzy0oSi0uzszP4wIAnss+shIAAAAfiwgAAAAAAAIDhdjLTSRBFETRPVaUCRn5T8wZBAKBQKAZMeYjDOCwjt1Rd9Wt9/L0en/l9vr7eH+9/3u6e77+fLx9vl4Pb/9vXr63iq1h69gGtoltYdvYDrYUjZKJaCKbCCfSiXginwgoEqoSqvztSKhKqEqoSqhKqEqoSqhKqEmoSajx7yWhJqEmoSahJqEmoSahLqEuoS6hzieQhLqEuoS6hLqEuoSGhIaEhoSGhAYf0hIaEhoSGhIaEpoSmhKaEpoSmhKafI9JaEpoSmhKaEloSWhJaEloSWhJaPFVL6EloSWhLaEtoS2hLaEtoS2hLaHNGpLQltCR0JHQkdCR0JHQkdCR0JHQYTC6GJmMhc1YGI2F1ViYjYXdWBiOheVYmI6FVr/kNa0c2C5sJ7Yb25HtynZms7PD0E71twit2NphbIe1HeZ22NthcIfFHSZ32Nxp/nCjFbM77O4wvMPyDtM7bO8wvsP6DvM73V+5tGKBhwkeNngY4WGFhxkedngY4mGJZ/gkQCvGeFjjYY6HPR4GeVjkYZKHTR5GeabvJ7Ril4dhHpZ5mOZhm4dxHtZ5mOdhn2f52EQrJnrY6GGkh5UeZnrY6WGoh6Uepnq2L3O0Yq2HuR72ehjsYbGHyR42exjtYbXn+IzpO+YPVl99KwOwBBcAAA=="},
    {tool: "bzip2", data: "QlpoOTFBWSZTWYwpdkUAAALRgAAQQAAKZ9gAIAAxANABADQFW07HVgJLzeA8XckU4UJCMKXZFEJaaDkxQVkmU1m8nms7AAdtWYAAEEAAf/AbbbbAQAI84DhgUAA0AABk8Ce/VKgAABj/1VJB/6p/qqoAAGgUqVAftUgaAGnqcCLmIvgRfQi6hFzEW81GadXLw8fLz9Oeb3ve973zTlqmtU1qmtU1rnng554OeeDnng554LqVVWtttt69gAALquqqtrbbbv1AAAuqvFVba223foAABdVXlVbbW23fmAABdVV51bba23fkAABdVVelbbbW3fgAAC6qqvW2221u+gAALqqqtttttdgAA4ddXiZmZmZiIiIiIADQAQoAalQGRYDAXBcGAsBkVAalACGkNKAGpUBkWAwFwXBgLAZFQM78zWZmZmZvHIRfIi8xF8iLrEXQRfwi/BF9iL6EXAiwRcCLoIsEXQRaEXAi4CL/F3JFOFCQvJ5rOw=="},
    {tool: "xz", data: "/Td6WFoAAATm1rRGBMAWEiEBFgAAAAAAAAAAAKdRd54BABFoZWxsbyBjb21wcmVzc2lvbgoAAABsh2AL6pd1XgABMhISkE8+H7bzfQEAAAAABFla/Td6WFoAAATm1rRGBMCaAoQuIQEcAAAAAAAAAM2Yh8DgFwMBEl0ANhpKHwigLARuDWy4pe1jnI582072nkt4GFZc9x8wdtbZxm6/0Fg3tn3B+sJ0bUu9SJJV8lRv9sKJBLw0Er9ezAZYkS+OLCRBKsjf44BT9jJblP/zrA8mS+YbWCARIh+bZkiPTZiWHUQCEITKijG+8gP+0A4GyBKG3rySmILV+mqAO1uQF5rF/4EggbhflGggTk8HTRUuLBK78mVDmgaUqGo1XMMwpMlOZP/Z4S2nSJhH7n+g+WUJsF2OHY4mIaDFQXmg7uFWVKA1USNcBjhYyBY+dprmRLW5a1A31pktSfft8bwytBBKa/UwFlpgEx8+DVteTiglB/n8x0GpW2NlfGk1/6DxQWihV7XuWO6y8MO4AAAAAFZCFtLUkPMSAAG2AoQuAABTPe4lscRn+wIAAAAABFla"},
    {tool: "zstd", data: "KLUv/SQSkQAAaGVsbG8gY29tcHJlc3Npb24KpPA27Si1L/1kBBaFCAAWVTEWgE06wIjjOI7DBNVFUkopU0oP4cZR6TIAKQAlAC7LnBXNXZafFc1dFp8VzV2WnhXNXRaeFc1dQGRRSBSQRUOQIAaHByMhGBxJgAlBcUgExOFplmThFStVqEx5cWlhKYVnRXOX5c6K5i6LnRXNXZY6K5q7LHRWNA9jU0Mz8/F0OGXu9jp9/vs9X342l8nj267p4qu1Sp2+bsuWHo3FvbOro5v7+Xq8cmdmZWRjN1uNVuzKqopq6uVqsUodGRURDZ1MJVKhC4DIqBHQ9fZ/A+CldgMRJAT/KwQdP7tbAiCAIAACARAIgEAABAIgEHB4uMJ9gwAZgw8BZwc+BI4d+Ag4czDAa2NhLKiF8MCuAqf8NI0="},
  ]
}

proc names_in(dir: Path) [fs, error] -> Result[List[Str]] {
  Ok(collect { for entry in fs.children(dir, ordered: true)? { yield entry.name } })
}

# Own the shortened bytes before they cross the child-command boundary: byte
# views currently lose their argument slot on that path.
pure drop_tail(data: Bytes, count: Int) -> Bytes {
  bytes.concat([data[0..data.len() - count]])
}

# Replace the byte in the middle of an archive, which lands inside the coded
# payload of every fixture and so must be caught by a checksum or the codec.
proc flip_middle(data: Bytes) [error] -> Result[Bytes] {
  let at = data.len() / 2
  let flipped = bytes.from_ints([((data.byte_at(at) ?? 0) + 1) % 256])?
  Ok(bytes.concat([data[0..at], flipped, data[at + 1..data.len()]]))
}

test test_reference_archives_decode_through_every_entry_point { |ctx|
  for family in families() {
    let payloads = [
      {name: "text", packed: family.text, plain: plain_text()},
      {name: "empty", packed: family.empty, plain: b""},
      {name: "bin", packed: family.bin, plain: plain_bin()},
      {name: "lines", packed: family.lines, plain: plain_lines()},
      {name: "big", packed: family.big, plain: plain_big()},
    ]
    for payload in payloads {
      let packed = payload.packed.base64_decode()?
      let label = f"{family.tool} {payload.name}"
      let via_cat = invoke(ctx, family.cat, [], packed)?
      assert via_cat.status == 0, f"{label}: {via_cat.stderr}"
      assert via_cat.stdout == payload.plain, f"{label}: {family.cat} output"
      let via_tool = invoke(ctx, family.tool, ["-dc"], packed)?
      assert via_tool.status == 0, f"{label}: {via_tool.stderr}"
      assert via_tool.stdout == payload.plain, f"{label}: {family.tool} -dc output"
      let via_decoder = invoke(ctx, family.decoder, ["-c"], packed)?
      assert via_decoder.status == 0, f"{label}: {via_decoder.stderr}"
      assert via_decoder.stdout == payload.plain, f"{label}: {family.decoder} -c output"
      let checked = invoke(ctx, family.tool, ["-t"], packed)?
      assert checked.status == 0, f"{label}: {checked.stderr}"
      assert checked.stdout == b"" and checked.stderr == "", label
    }
  }
}

test test_reference_archives_decode_from_file_operands { |ctx|
  for family in families() {
    let root = test.temp_dir(ctx, name: f"reference-{family.tool}")?
    let packed = fp"{root}/lines{family.suffix}"
    packed.write(family.lines.base64_decode()?)
    let printed = invoke(ctx, family.cat, [packed.display()])?
    assert printed.status == 0, printed.stderr
    assert printed.stdout == plain_lines()
    assert packed.exists()?, f"{family.cat} must keep its operand"
    assert names_in(root)? == [f"lines{family.suffix}"]
    assert invoke(ctx, family.tool, ["-t", packed.display()])?.status == 0
    assert invoke(ctx, family.decoder, [packed.display()])?.status == 0
    assert fp"{root}/lines".read_bytes()? == plain_lines()
    # zstd keeps its input unless asked to remove it; the others replace it.
    assert packed.exists()? == (family.tool == "zstd"), f"{family.decoder} operand handling"
  }
}

test test_reference_archive_variants_decode { |ctx|
  for variant in variants() {
    let packed = variant.data.base64_decode()?
    let plain = invoke(ctx, variant.tool, ["-dc"], packed)?
    assert plain.status == 0, f"{variant.what}: {plain.stderr}"
    assert plain.stdout == plain_lines(), variant.what
    assert invoke(ctx, variant.tool, ["-t"], packed)?.status == 0, variant.what
  }
}

test test_reference_member_sequences_decode_in_order { |ctx|
  let expected = bytes.concat([plain_text(), plain_lines()])
  for member in members() {
    let packed = member.data.base64_decode()?
    let plain = invoke(ctx, member.tool, ["-dc"], packed)?
    assert plain.status == 0, f"{member.tool}: {plain.stderr}"
    assert plain.stdout == expected, member.tool
    assert invoke(ctx, member.tool, ["-t"], packed)?.status == 0
  }
}

# The container magic each family must begin with, whatever the level.
pure magic(tool: Str) -> Bytes {
  match tool {
    "gzip" => b"\x1f\x8b\x08"
    "bzip2" => b"BZh"
    "xz" => b"\xfd7zXZ\0"
    "lzma" => b"\x5d\0\0"
    else => b"\x28\xb5\x2f\xfd"
  }
}

test test_compress_output_round_trips_at_every_level { |ctx|
  for family in families() {
    for level in range(1, 10) {
      let packed = invoke(ctx, family.tool, [f"-{level}c"], plain_lines())?
      assert packed.status == 0, f"{family.tool} -{level}: {packed.stderr}"
      assert packed.stdout[0..magic(family.tool).len()] == magic(family.tool), f"{family.tool} -{level} magic"
      assert packed.stdout.len() < plain_lines().len() / 4, f"{family.tool} -{level} did not compress"
      let again = invoke(ctx, family.tool, [f"-{level}c"], plain_lines())?
      assert again.stdout == packed.stdout, f"{family.tool} -{level} output must be deterministic"
      let plain = invoke(ctx, family.decoder, ["-c"], packed.stdout)?
      assert plain.status == 0, f"{family.tool} -{level}: {plain.stderr}"
      assert plain.stdout == plain_lines(), f"{family.tool} -{level} round trip"
    }
  }
}

test test_compress_round_trips_empty_binary_and_multiblock_input { |ctx|
  for family in families() {
    for payload in [{name: "empty", plain: b""}, {name: "bin", plain: plain_bin()}, {name: "big", plain: plain_big()}] {
      let packed = invoke(ctx, family.tool, ["-c"], payload.plain)?
      assert packed.status == 0, f"{family.tool} {payload.name}: {packed.stderr}"
      assert ! packed.stdout.is_empty(), f"{family.tool} {payload.name} must still write a container"
      assert packed.stdout[0..magic(family.tool).len()] == magic(family.tool)
      let plain = invoke(ctx, family.cat, [], packed.stdout)?
      assert plain.status == 0, f"{family.tool} {payload.name}: {plain.stderr}"
      assert plain.stdout == payload.plain, f"{family.tool} {payload.name} round trip"
    }
  }
}

test test_gzip_and_bzip2_level_is_recorded_in_the_header { |ctx|
  # gzip's extra-flags byte is 2 for best compression and 4 for fastest; the
  # bzip2 block-size digit follows the magic.
  let best = invoke(ctx, "gzip", ["-9c"], plain_lines())?.stdout
  let fast = invoke(ctx, "gzip", ["-1c"], plain_lines())?.stdout
  assert best.byte_at(8) == 2 and fast.byte_at(8) == 4
  for digit in range(1, 10) {
    let packed = invoke(ctx, "bzip2", [f"-{digit}c"], plain_lines())?.stdout
    assert packed[0..4] == bytes.concat([b"BZh", bytes.from_text(f"{digit}")])
  }
  let default = invoke(ctx, "bzip2", ["-c"], plain_lines())?.stdout
  assert default[0..4] == b"BZh9"
}

test test_bzip2_accepts_the_small_memory_flag { |ctx|
  let packed = invoke(ctx, "bzip2", ["-s", "-c"], plain_lines())?
  assert packed.status == 0, packed.stderr
  assert packed.stdout[0..4] == b"BZh9"
  let plain = invoke(ctx, "bzip2", ["-sdc"], packed.stdout)?
  assert plain.status == 0, plain.stderr
  assert plain.stdout == plain_lines()
}

test test_compress_long_options_match_short_options { |ctx|
  for family in families() {
    let short = invoke(ctx, family.tool, ["-c"], plain_lines())?
    let long = invoke(ctx, family.tool, ["--stdout"], plain_lines())?
    assert long.status == 0, long.stderr
    assert long.stdout == short.stdout, f"{family.tool} --stdout"
    let plain = invoke(ctx, family.tool, ["--decompress", "--stdout"], long.stdout)?
    assert plain.stdout == plain_lines(), f"{family.tool} --decompress"
    assert invoke(ctx, family.tool, ["--test"], long.stdout)?.status == 0, f"{family.tool} --test"
    assert invoke(ctx, family.tool, ["--test", "--quiet"], b"not an archive")?.status != 0
  }
}

test test_compress_standard_input_and_dash_operand { |ctx|
  for family in families() {
    let implicit = invoke(ctx, family.tool, ["-c"], plain_text())?
    let dash = invoke(ctx, family.tool, ["-c", "-"], plain_text())?
    assert dash.status == 0, dash.stderr
    assert dash.stdout == implicit.stdout, f"{family.tool}: - names standard input"
    assert invoke(ctx, family.decoder, ["-c", "-"], implicit.stdout)?.stdout == plain_text()
  }
}

test test_compress_operands_after_double_dash_are_files { |ctx|
  let root = test.temp_dir(ctx, name: "compress-dash")?
  let source = fp"{root}/-z"
  source.write(b"dash operand")
  let result = invoke_in(ctx, "gzip", ["--", "-z"], root)?
  assert result.status == 0, result.stderr
  assert ! source.exists()?
  let packed_file = fp"{root}/-z.gz"
  assert packed_file.exists()?
  assert invoke_in(ctx, "gunzip", ["--", "-z.gz"], root)?.status == 0
  assert source.read_bytes()? == b"dash operand"
}

test test_compress_stdout_mode_concatenates_operands_in_order { |ctx|
  for family in families() {
    let root = test.temp_dir(ctx, name: f"compress-many-{family.tool}")?
    let first = fp"{root}/first"
    let second = fp"{root}/second"
    first.write(plain_text())
    second.write(plain_bin())
    let packed = invoke(ctx, family.tool, ["-c", first.display(), second.display()])?
    assert packed.status == 0, packed.stderr
    assert first.exists()? and second.exists()?, "-c must not remove the sources"
    assert names_in(root)? == ["first", "second"]
    if family.tool != "lzma" {
      let plain = invoke(ctx, family.cat, [], packed.stdout)?
      assert plain.stdout == bytes.concat([plain_text(), plain_bin()]), f"{family.tool} member order"
    }
    let both = invoke(ctx, family.tool, ["-dc", first.display()].extend([second.display()]))?
    assert both.status != 0, "plain files are not archives"
  }
}

test test_compress_refuses_to_replace_an_existing_archive_without_force { |ctx|
  for family in families() {
    let root = test.temp_dir(ctx, name: f"compress-clobber-{family.tool}")?
    let source = fp"{root}/data"
    let packed_file = fp"{root}/data{family.suffix}"
    source.write(plain_lines())
    packed_file.write(b"precious")
    let refused = invoke(ctx, family.tool, ["-k", source.display()])?
    assert refused.status != 0, f"{family.tool} must not overwrite"
    assert packed_file.read_bytes()? == b"precious"
    assert source.read_bytes()? == plain_lines()
    let forced = invoke(ctx, family.tool, ["-kf", source.display()])?
    assert forced.status == 0, forced.stderr
    assert source.read_bytes()? == plain_lines()
    let plain = invoke(ctx, family.decoder, ["-c", packed_file.display()])?
    assert plain.stdout == plain_lines(), f"{family.tool} forced archive"
  }
}

test test_compress_failed_decode_keeps_input_and_leaves_no_output { |ctx|
  for family in families() {
    let good = family.lines.base64_decode()?
    var damaged = [
      {what: "truncated", data: drop_tail(good, 5)},
      {what: "not an archive", data: b"plain text, not compressed\n"},
    ]
    if family.tool != "lzma" {
      # The raw lzma container has no checksum, so only the others notice a
      # flipped byte.
      damaged += [{what: "flipped byte", data: flip_middle(good)?}]
    }
    for case in damaged {
      let root = test.temp_dir(ctx, name: f"compress-damaged-{family.tool}")?
      let source = fp"{root}/damaged{family.suffix}"
      source.write(case.data)
      let label = f"{family.tool} {case.what}"
      let decoded = invoke(ctx, family.decoder, [source.display()])?
      assert decoded.status != 0, label
      assert decoded.stderr != "", f"{label} must explain the failure"
      assert source.read_bytes()? == case.data, f"{label} must keep its input"
      assert names_in(root)? == [f"damaged{family.suffix}"], f"{label} must not leave output behind"
      let checked = invoke(ctx, family.tool, ["-t", source.display()])?
      assert checked.status != 0, f"{label} -t"
      assert checked.stdout == b""
      let streamed = invoke(ctx, family.tool, ["-dc"], case.data)?
      assert streamed.status != 0, f"{label} from standard input"
    }
  }
}

test test_compress_test_mode_reports_every_operand { |ctx|
  for family in families() {
    let root = test.temp_dir(ctx, name: f"compress-test-{family.tool}")?
    let good = fp"{root}/good{family.suffix}"
    let bad = fp"{root}/bad{family.suffix}"
    good.write(family.text.base64_decode()?)
    bad.write(drop_tail(family.text.base64_decode()?, 3))
    let result = invoke(ctx, family.tool, ["-tv", good.display(), bad.display(), good.display()])?
    assert result.status != 0, family.tool
    assert result.stdout == b""
    assert f"good{family.suffix}" in result.stderr, result.stderr
    assert good.exists()? and bad.exists()?
    let only_good = invoke(ctx, family.tool, ["-tv", good.display()])?
    assert only_good.status == 0, only_good.stderr
    assert only_good.stdout == b""
    assert f"good{family.suffix}" in only_good.stderr, "-v must name the checked file"
  }
}

test test_compress_missing_operand_does_not_stop_later_operands { |ctx|
  for family in families() {
    let root = test.temp_dir(ctx, name: f"compress-missing-{family.tool}")?
    let present = fp"{root}/present{family.suffix}"
    present.write(family.text.base64_decode()?)
    let missing = fp"{root}/missing{family.suffix}"
    let result = invoke(ctx, family.tool, ["-dc", missing.display(), present.display()])?
    assert result.status != 0, family.tool
    assert result.stdout == plain_text(), f"{family.tool} must still decode the later operand"
    assert "missing" in result.stderr, result.stderr
    let alone = invoke(ctx, family.cat, [missing.display()])?
    assert alone.status != 0 and alone.stdout == b""
  }
}

test test_compress_alias_force_passes_plain_input_through { |ctx|
  for family in families() {
    # A raw lzma stream has no magic, so telling it from plain data needs the
    # header; that is only possible for file operands (the next test).
    continue when family.tool == "lzma"
    let plain = b"not compressed\0\xff\n"
    let passed = invoke(ctx, family.cat, ["-f"], plain)?
    assert passed.status == 0, f"{family.cat} -f: {passed.stderr}"
    assert passed.stdout == plain, family.cat
    let refused = invoke(ctx, family.cat, [], plain)?
    assert refused.status != 0, f"{family.cat} without -f"
    assert refused.stdout == b"", f"{family.cat} must not echo rejected input"
  }
}

test test_xz_applets_detect_lzma_files_by_content { |ctx|
  let root = test.temp_dir(ctx, name: "xz-detects-lzma")?
  let lzma_fixture = family_named("lzma").lines.base64_decode()?
  let raw = fp"{root}/data.lzma"
  raw.write(lzma_fixture)
  let plain = fp"{root}/plain"
  plain.write(b"plain, not compressed\n")
  let xz_archive = fp"{root}/data.xz"
  xz_archive.write(family_named("xz").lines.base64_decode()?)
  for entry in ["xz", "unxz", "xzcat"] {
    let args = if entry == "xz" { ["-dc", raw.display()] } else { ["-c", raw.display()] }
    let result = invoke(ctx, entry, args)?
    assert result.status == 0, f"{entry}: {result.stderr}"
    assert result.stdout == plain_lines(), f"{entry} must read .lzma content"
  }
  assert invoke(ctx, "xz", ["-t", raw.display(), xz_archive.display()])?.status == 0
  # Decoding in place strips the .lzma suffix like any other archive name.
  assert invoke(ctx, "unxz", [raw.display()])?.status == 0
  assert fp"{root}/data".read_bytes()? == plain_lines()
  assert ! raw.exists()?
  # lzma has no magic of its own: -f passes non-lzma files through, and
  # without -f they are rejected.
  for entry in ["lzcat", "unlzma"] {
    let forced = invoke(ctx, entry, ["-cf", plain.display()])?
    assert forced.status == 0, f"{entry} -cf: {forced.stderr}"
    assert forced.stdout == b"plain, not compressed\n", entry
    let refused = invoke(ctx, entry, ["-c", plain.display()])?
    assert refused.status != 0 and refused.stdout == b"", f"{entry} without -f"
  }
  assert invoke(ctx, "lzcat", [xz_archive.display()])?.status != 0, "lzcat must not read .xz"
}

test test_compress_decoder_aliases_name_the_output_after_the_archive { |ctx|
  for case in [
    {tool: "gzip", decoder: "gunzip", archive: "a.tgz", plain: "a.tar"},
    {tool: "bzip2", decoder: "bunzip2", archive: "a.tbz2", plain: "a.tar"},
    {tool: "bzip2", decoder: "bunzip2", archive: "a.tbz", plain: "a.tar"},
    {tool: "bzip2", decoder: "bunzip2", archive: "a.bz", plain: "a"},
    {tool: "xz", decoder: "unxz", archive: "a.txz", plain: "a.tar"},
    {tool: "lzma", decoder: "unlzma", archive: "a.tlz", plain: "a.tar"},
    {tool: "zstd", decoder: "unzstd", archive: "a.tzst", plain: "a.tar"},
  ] {
    let root = test.temp_dir(ctx, name: f"compress-suffix-{case.archive}")?
    let packed_file = fp"{root}/{case.archive}"
    packed_file.write(invoke(ctx, case.tool, ["-c"], plain_text())?.stdout)
    let result = invoke(ctx, case.decoder, [packed_file.display()])?
    assert result.status == 0, f"{case.archive}: {result.stderr}"
    assert fp"{root}/{case.plain}".read_bytes()? == plain_text(), case.archive
  }
}

test test_compress_decode_rejects_unknown_suffix_but_stdout_ignores_names { |ctx|
  for family in families() {
    let root = test.temp_dir(ctx, name: f"compress-unknown-{family.tool}")?
    let packed_file = fp"{root}/archive.unknown"
    packed_file.write(family.text.base64_decode()?)
    let refused = invoke(ctx, family.decoder, [packed_file.display()])?
    assert refused.status != 0, family.tool
    assert names_in(root)? == ["archive.unknown"], f"{family.tool} must not guess an output name"
    let printed = invoke(ctx, family.decoder, ["-c", packed_file.display()])?
    assert printed.status == 0, printed.stderr
    assert printed.stdout == plain_text(), f"{family.tool} -c ignores the archive name"
  }
}

test test_compress_verbose_reports_on_standard_error { |ctx|
  for family in families() {
    let root = test.temp_dir(ctx, name: f"compress-verbose-{family.tool}")?
    let source = fp"{root}/data"
    source.write(plain_text())
    let packed = invoke(ctx, family.tool, ["-v", source.display()])?
    assert packed.status == 0, packed.stderr
    assert packed.stdout == b"", f"{family.tool} -v must keep stdout for data"
    assert "data" in packed.stderr, f"{family.tool} -v names the file: {packed.stderr}"
    let streamed = invoke(ctx, family.tool, ["-cv"], plain_text())?
    assert streamed.status == 0, streamed.stderr
    assert streamed.stdout[0..magic(family.tool).len()] == magic(family.tool), f"{family.tool} -cv stdout is the archive"
  }
}

test test_compress_option_errors_leave_the_filesystem_alone { |ctx|
  for family in families() {
    let root = test.temp_dir(ctx, name: f"compress-options-{family.tool}")?
    let source = fp"{root}/data"
    source.write(plain_text())
    # gzip and bzip2 have no level 0; the other families accept it.
    let bad_options = if family.tool in ["gzip", "bzip2"] { [["--no-such-option"], ["-0"]] } else { [["--no-such-option"]] }
    for args in bad_options {
      let shown = args.join(" ")
      let result = invoke_in(ctx, family.tool, args.extend([source.display()]), root)?
      assert result.status != 0, f"{family.tool} {shown}"
      assert result.stdout == b""
      assert result.stderr != ""
      assert names_in(root)? == ["data"], f"{family.tool} {shown} must not touch files"
    }
  }
}

test test_compress_help_and_version_exit_cleanly { |ctx|
  for family in families() {
    let help = invoke(ctx, family.tool, ["--help"])?
    assert help.status == 0, f"{family.tool}: {help.stderr}"
    assert "Usage" in help.stdout as Str, family.tool
    let version = invoke(ctx, family.tool, ["--version"])?
    assert version.status == 0, f"{family.tool}: {version.stderr}"
    assert ! version.stdout.is_empty(), family.tool
  }
}

# Each archive is decoded by the matching reference tool and compared with its
# payload; any rejection or mismatch prints a line and fails the loop.
const VERIFY_ARCHIVES = r"""
status=0
for archive in *.*-*; do
  plain=${archive%%.*}
  tool=${archive#*.}
  tool=${tool%%-*}
  case $tool in
    gzip) gzip -dc "$archive" ;;
    bzip2) bzip2 -dc "$archive" ;;
    xz) xz -dc "$archive" ;;
    lzma) xz --format=lzma -dc "$archive" ;;
    zstd) zstd -q -dc "$archive" ;;
  esac | cmp -s - "$plain" || { echo "rejected: $archive"; status=1; }
done
exit 0
"""

# Archive names are PAYLOAD.TOOL-OPTION; the decoder side maps TOOL back to an
# applet and compares with the PAYLOAD file.
const MAKE_ARCHIVES = r"""
for plain in text empty bin lines big; do
  gzip -1 -c $plain > $plain.gzip-1
  gzip -9 -c $plain > $plain.gzip-9
  bzip2 -1 -c $plain > $plain.bzip2-1
  bzip2 -9 -c $plain > $plain.bzip2-9
  xz -0 -c $plain > $plain.xz-0
  xz -9e -c $plain > $plain.xz-9
  xz -6 --check=crc32 -c $plain > $plain.xz-crc32
  xz --format=lzma -1 -c $plain > $plain.lzma-1
  xz --format=lzma -9 -c $plain > $plain.lzma-9
  zstd -q -1 -c $plain > $plain.zstd-1
  zstd -q -19 -c $plain > $plain.zstd-19
  zstd -q --ultra -22 -c $plain > $plain.zstd-22
  zstd -q --fast=4 -c $plain > $plain.zstd-fast
done
"""

const IMAGE_PROBE = r"""docker image inspect "${XSH_ORACLE_IMAGE:-xsh-oracle}" >/dev/null 2>&1 && echo ready"""

const TOOL_PROBE = "command -v gzip bzip2 xz zstd >/dev/null && echo ready"

const ORACLE_SKIP = "requires docker with the xsh-oracle image (dev/compat/oracle.sh)"

# The reference tools run only inside the oracle container, never as a runtime
# dependency. Returns the oracle script, or null when this host has no docker
# or the image is not built: building it needs the network, so the test skips
# instead of triggering that build.
proc oracle_script(ctx: TestContext) [fs, process, error] -> Result[Path?] {
  let script = fp"{ctx.core_dir.parent()}/dev/compat/oracle.sh"
  guard script.exists() else { return Ok(null) }
  let image = try run.text sh -c $IMAGE_PROBE
  guard image is Ok(_) else { return Ok(null) }
  let tools = try run.text $script -- sh -c $TOOL_PROBE
  guard tools is Ok(_) else { return Ok(null) }
  Ok(script)
}

# Archives XSH writes at several levels and sizes must be accepted by the
# reference tools and decode to the original bytes there.
test test_reference_tools_accept_archives_written_by_xsh { |ctx|
  test.timeout(ctx, 120s)
  guard let oracle = oracle_script(ctx)? else { test.skip(ORACLE_SKIP); return }
  let root = test.temp_dir(ctx, name: "oracle-xsh-archives")?
  root.chmod(0o755)
  let payloads = [
    {name: "text", plain: plain_text()},
    {name: "empty", plain: b""},
    {name: "bin", plain: plain_bin()},
    {name: "lines", plain: plain_lines()},
    {name: "big", plain: plain_big()},
  ]
  for payload in payloads {
    let plain = fp"{root}/{payload.name}"
    plain.write(payload.plain, mode: 0o644)
    for family in families() {
      for level in ["1", "6", "9"] {
        let packed = invoke(ctx, family.tool, [f"-{level}c"], payload.plain)?
        assert packed.status == 0, f"{family.tool} -{level}: {packed.stderr}"
        let target = fp"{root}/{payload.name}.{family.tool}-{level}"
        target.write(packed.stdout, mode: 0o644)
      }
    }
  }
  let rejected = run.text $oracle --mount $root -- sh -c $VERIFY_ARCHIVES
  assert rejected == "", rejected
}

# The reverse direction: archives the reference tools write at assorted levels
# and container options must decode to the original bytes through XSH.
test test_xsh_decodes_archives_written_by_reference_tools { |ctx|
  test.timeout(ctx, 120s)
  guard let oracle = oracle_script(ctx)? else { test.skip(ORACLE_SKIP); return }
  let root = test.temp_dir(ctx, name: "oracle-reference-archives")?
  root.chmod(0o777)
  let payloads = [
    {name: "text", plain: plain_text()},
    {name: "empty", plain: b""},
    {name: "bin", plain: plain_bin()},
    {name: "lines", plain: plain_lines()},
    {name: "big", plain: plain_big()},
  ]
  for payload in payloads {
    let plain = fp"{root}/{payload.name}"
    plain.write(payload.plain, mode: 0o644)
  }
  run.text $oracle --mount $root --rw -- sh -c $MAKE_ARCHIVES
  var decoded = 0
  for entry in fs.children(root, ordered: true)? {
    guard "." in entry.name and "-" in entry.name else { continue }
    let dot = entry.name.find(".") ?? 0
    let plain_name = entry.name.byte_slice(0, length: dot)
    let rest = entry.name.byte_slice(dot + 1)
    let tool = rest.byte_slice(0, length: rest.find("-") ?? 0)
    let expected = fp"{root}/{plain_name}".read_bytes()?
    let result = invoke(ctx, tool, ["-dc", entry.path.display()])?
    assert result.status == 0, f"{entry.name}: {result.stderr}"
    assert result.stdout == expected, f"{entry.name} decoded differently"
    assert invoke(ctx, tool, ["-t", entry.path.display()])?.status == 0, f"{entry.name} -t"
    decoded += 1
  }
  assert decoded == 65, f"expected 65 reference archives, found {decoded}"
}

pure family_named(tool: Str) -> Family {
  let found = collect { for family in families() { yield family when family.tool == tool } }
  found[0]
}

pure variant_named(what: Str) -> Str {
  let found = collect { for variant in variants() { yield variant.data when variant.what == what } }
  found[0]
}

pure member_named(tool: Str) -> Str {
  let found = collect { for member in members() { yield member.data when member.tool == tool } }
  found[0]
}

type Staged = {name: Str, data: Str}

# Saves reference archives under short names in a fresh directory and returns
# it, so a listing shows those names.
proc stage(ctx: TestContext, label: Str, files: List[Staged]) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: label)?
  for file in files { fp"{root}/{file.name}".write(file.data.base64_decode()?) }
  Ok(root)
}

# The expected tables are what gzip 1.15, xz 5.8.4 and zstd 1.5.7 print for
# the same archives under the same names, so each case is a byte-exact check of
# the listing against the reference tool.
test test_gzip_list_matches_the_reference_table { |ctx|
  let gzip = family_named("gzip")
  let root = stage(ctx, "list-gzip", [
    {name: "text.gz", data: gzip.text},
    {name: "empty.gz", data: gzip.empty},
    {name: "lines.gz", data: gzip.lines},
    {name: "big.gz", data: gzip.big},
    {name: "named.gz", data: variant_named("gzip with stored name and time")},
    {name: "cat.gz", data: member_named("gzip")},
  ])?
  for case in [
    {args: ["-l", "text.gz"], table: "         compressed        uncompressed  ratio uncompressed_name\n                 38                  18 -11.1% text\n"},
    {args: ["-l", "empty.gz"], table: "         compressed        uncompressed  ratio uncompressed_name\n                 20                   0  -Inf% empty\n"},
    {args: ["-l", "lines.gz", "big.gz"], table: "         compressed        uncompressed  ratio uncompressed_name\n                503                5892  91.8% lines\n               1645              250000  99.3% big\n               2148              255892  99.2% (totals)\n"},
    {args: ["-l", "cat.gz"], table: "         compressed        uncompressed  ratio uncompressed_name\n                541                5910  90.8% cat\n"},
    {args: ["-lq", "text.gz", "lines.gz"], table: "                 38                  18 -11.1% text\n                503                5892  91.8% lines\n"},
    {args: ["-l", "named.gz"], table: "         compressed        uncompressed  ratio uncompressed_name\n                505                5892  91.8% named\n"},
    {args: ["-lN", "named.gz"], table: "         compressed        uncompressed  ratio uncompressed_name\n                505                5892  91.8% lines\n"},
  ] {
    let result = invoke_in(ctx, "gzip", case.args, root)?
    assert result.status == 0, f"{case.args.join(" ")}: {result.stderr}"
    assert result.stdout as Str == case.table, f"gzip {case.args.join(" ")}"
  }
  let long = invoke_in(ctx, "gzip", ["--list", "text.gz"], root)?
  assert long.stdout as Str == "         compressed        uncompressed  ratio uncompressed_name\n                 38                  18 -11.1% text\n"
  let from_stdin = invoke_in(ctx, "gzip", ["-l"], root, family_named("gzip").text.base64_decode()?)?
  assert from_stdin.status == 0, from_stdin.stderr
  assert from_stdin.stdout as Str == "         compressed        uncompressed  ratio uncompressed_name\n                 38                  18 -11.1% stdout\n"
}

test test_gzip_list_reports_unreadable_operands { |ctx|
  let root = stage(ctx, "list-gzip-errors", [
    {name: "good.gz", data: family_named("gzip").text},
    {name: "plain", data: family_named("gzip").text},
  ])?
  fp"{root}/plain".write(b"not gzip data\n")
  fp"{root}/truncated.gz".write(drop_tail(family_named("gzip").lines.base64_decode()?, 5))
  for name in ["plain", "truncated.gz", "missing.gz"] {
    let result = invoke_in(ctx, "gzip", ["-l", name], root)?
    assert result.status == 1, name
    assert result.stdout == b"", f"{name} must not list a row"
    assert name in result.stderr, result.stderr
  }
  let mixed = invoke_in(ctx, "gzip", ["-l", "missing.gz", "good.gz"], root)?
  assert mixed.status == 1
  assert "good" in mixed.stdout as Str, "later operands are still listed"
  # The verbose table needs local-time formatting this applet cannot reach, so
  # it fails loudly instead of printing the short table under a verbose flag.
  let verbose = invoke_in(ctx, "gzip", ["-lv", "good.gz"], root)?
  assert verbose.status != 0 and verbose.stdout == b""
}

test test_xz_list_matches_the_reference_table { |ctx|
  let xz = family_named("xz")
  let root = stage(ctx, "list-xz", [
    {name: "text.xz", data: xz.text},
    {name: "empty.xz", data: xz.empty},
    {name: "lines.xz", data: xz.lines},
    {name: "big.xz", data: xz.big},
    {name: "cat.xz", data: member_named("xz")},
    {name: "crc32.xz", data: variant_named("xz --check=crc32")},
    {name: "sha256.xz", data: variant_named("xz --check=sha256")},
    {name: "none.xz", data: variant_named("xz --check=none")},
  ])?
  for case in [
    {args: ["-l", "text.xz"], table: "Strms  Blocks   Compressed Uncompressed  Ratio  Check   Filename\n    1       1         84 B         18 B  4.667  CRC64   text.xz\n"},
    {args: ["-l", "empty.xz"], table: "Strms  Blocks   Compressed Uncompressed  Ratio  Check   Filename\n    1       0         32 B          0 B    ---  CRC64   empty.xz\n"},
    {args: ["-l", "big.xz"], table: "Strms  Blocks   Compressed Uncompressed  Ratio  Check   Filename\n    1       3        468 B    244.1 KiB  0.002  CRC64   big.xz\n"},
    {args: ["-l", "text.xz", "lines.xz"], table: "Strms  Blocks   Compressed Uncompressed  Ratio  Check   Filename\n    1       1         84 B         18 B  4.667  CRC64   text.xz\n    1       1        348 B       5892 B  0.059  CRC64   lines.xz\n-------------------------------------------------------------------------------\n    2       2        432 B       5910 B  0.073  CRC64   2 files\n"},
    {args: ["-l", "cat.xz"], table: "Strms  Blocks   Compressed Uncompressed  Ratio  Check   Filename\n    2       2        432 B       5910 B  0.073  CRC64   cat.xz\n"},
    {args: ["-l", "crc32.xz", "sha256.xz", "none.xz"], table: "Strms  Blocks   Compressed Uncompressed  Ratio  Check   Filename\n    1       1        344 B       5892 B  0.058  CRC32   crc32.xz\n    1       1        372 B       5892 B  0.063  SHA-256 sha256.xz\n    1       1        340 B       5892 B  0.058  None    none.xz\n-------------------------------------------------------------------------------\n    3       3       1056 B     17.3 KiB  0.060  None,CRC32,SHA-256 3 files\n"},
  ] {
    let result = invoke_in(ctx, "xz", case.args, root)?
    assert result.status == 0, f"{case.args.join(" ")}: {result.stderr}"
    assert result.stdout as Str == case.table, f"xz {case.args.join(" ")}"
  }
  assert invoke_in(ctx, "xz", ["--list", "text.xz"], root)?.status == 0
}

test test_xz_list_rejects_what_is_not_an_xz_file { |ctx|
  let root = stage(ctx, "list-xz-errors", [{name: "good.xz", data: family_named("xz").text}])?
  fp"{root}/plain".write(b"hello compression, not xz at all\n")
  fp"{root}/tiny".write(b"tiny")
  fp"{root}/truncated.xz".write(drop_tail(family_named("xz").lines.base64_decode()?, 40))
  for name in ["plain", "tiny", "truncated.xz", "missing.xz"] {
    let result = invoke_in(ctx, "xz", ["-l", name], root)?
    assert result.status == 1, name
    assert result.stdout == b"", f"{name} must not list a row"
    assert name in result.stderr, result.stderr
  }
  let stdin = invoke_in(ctx, "xz", ["-l"], root, b"anything")?
  assert stdin.status == 1 and stdin.stdout == b""
}

test test_zstd_list_matches_the_reference_table { |ctx|
  let zstd = family_named("zstd")
  let root = stage(ctx, "list-zstd", [
    {name: "text.zst", data: zstd.text},
    {name: "empty.zst", data: zstd.empty},
    {name: "lines.zst", data: zstd.lines},
    {name: "big.zst", data: zstd.big},
    {name: "cat.zst", data: member_named("zstd")},
    {name: "nocheck.zst", data: variant_named("zstd --no-check")},
  ])?
  for case in [
    {args: ["-l", "text.zst"], table: "Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n     1      0      31   B        18   B  0.581  XXH64  text.zst\n"},
    {args: ["-l", "empty.zst"], table: "Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n     1      0      13   B         0   B  0.000  XXH64  empty.zst\n"},
    {args: ["-l", "lines.zst", "big.zst"], table: "Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n     1      0     286   B      5.75 KiB  20.601  XXH64  lines.zst\n     1      0      60   B       244 KiB  4166.667  XXH64  big.zst\n----------------------------------------------------------------- \n     2      0     346   B       250 KiB  739.572  XXH64  2 files\n"},
    {args: ["-l", "cat.zst"], table: "Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n     2      0     317   B      5.77 KiB  18.644  XXH64  cat.zst\n"},
    {args: ["-l", "nocheck.zst", "text.zst"], table: "Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n     1      0     261   B      5.75 KiB  22.575   None  nocheck.zst\n     1      0      31   B        18   B  0.581  XXH64  text.zst\n----------------------------------------------------------------- \n     2      0     292   B      5.77 KiB  20.240         2 files\n"},
    {args: ["-l", "big.zst"], table: "Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n     1      0      60   B       244 KiB  4166.667  XXH64  big.zst\n"},
  ] {
    let result = invoke_in(ctx, "zstd", case.args, root)?
    assert result.status == 0, f"{case.args.join(" ")}: {result.stderr}"
    assert result.stdout as Str == case.table, f"zstd {case.args.join(" ")}"
  }
  assert invoke_in(ctx, "zstd", ["--list", "text.zst"], root)?.status == 0
}

test test_zstd_list_counts_skippable_frames_and_rejects_plain_files { |ctx|
  let root = stage(ctx, "list-zstd-edges", [{name: "text.zst", data: family_named("zstd").text}])?
  let frame = family_named("zstd").text.base64_decode()?
  let skip = b"\x50\x2a\x4d\x18\x04\0\0\0skip"
  fp"{root}/skipped.zst".write(bytes.concat([skip, frame, skip]))
  fp"{root}/plain".write(b"hello compression, not zstd\n")
  let skipped = invoke_in(ctx, "zstd", ["-l", "skipped.zst"], root)?
  assert skipped.status == 0, skipped.stderr
  assert skipped.stdout as Str == "Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n     3      2      55   B        18   B  0.327  XXH64  skipped.zst\n"
  let plain = invoke_in(ctx, "zstd", ["-l", "plain"], root)?
  assert plain.status == 1
  assert "plain" in plain.stdout as Str
  let missing = invoke_in(ctx, "zstd", ["-l", "missing.zst"], root)?
  assert missing.status == 1
  let stdin = invoke_in(ctx, "zstd", ["-l"], root, frame)?
  assert stdin.status == 1 and stdin.stdout == b""
}
