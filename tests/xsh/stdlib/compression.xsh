const PLAIN = b"hello hello hello compression sniff\n"

# A child script that decodes standard input to standard output with the
# format named by its first argument, passing unrecognized input through when
# a second argument is present.
const DECODE_STDIN = r"""
let passing = args.len() > 1
compression.transform(null, null, args[0], decode: true, pass_through: passing)?
"""

type Ran = {status: Int, stdout: Bytes}

proc pack(ctx: TestContext, format: Str) [fs, error] -> Result[Bytes] {
  let root = test.temp_dir(ctx, name: f"pack-{format}")?
  let source = fp"{root}/plain"
  let packed = fp"{root}/packed"
  source.write(PLAIN)
  compression.transform(source, packed, format, metadata: false)?
  packed.read_bytes()
}

proc decode_stdin(ctx: TestContext, format: Str, input: Bytes, force: Bool) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "decode-stdin")?
  let script = test.temp_file(ctx, name: "decode-stdin.xsh", contents: bytes.from_text(DECODE_STDIN))?
  let out = fp"{root}/out"
  let argv = [ctx.xsh_bin.display(), script.display(), "--", format].extend(if force { ["force"] } else { [] })
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {}, input, out, fp"{root}/err", timeout: 10s))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?})
}

test test_compression_auto_detects_every_format_from_the_stream { |ctx|
  let root = test.temp_dir(ctx, name: "compression-auto")?
  for format in ["gzip", "bzip2", "xz", "lzma", "zstd"] {
    let packed = fp"{root}/plain.{format}"
    packed.write(pack(ctx, format)?)
    let decoded = fp"{root}/decoded.{format}"
    compression.transform(packed, decoded, "auto", decode: true)?
    assert decoded.read_bytes()? == PLAIN, format
    let ran = decode_stdin(ctx, "auto", packed.read_bytes()?, false)?
    assert ran.status == 0, format
    assert ran.stdout == PLAIN, f"{format} from standard input"
  }
}

test test_compression_candidate_list_limits_detection { |ctx|
  let root = test.temp_dir(ctx, name: "compression-candidates")?
  for format in ["xz", "lzma"] {
    let ran = decode_stdin(ctx, "xz,lzma", pack(ctx, format)?, false)?
    assert ran.status == 0, format
    assert ran.stdout == PLAIN, format
  }
  # gzip is not a candidate, so its stream is unrecognized rather than decoded.
  let gzip = pack(ctx, "gzip")?
  let refused = decode_stdin(ctx, "xz,lzma", gzip, false)?
  assert refused.status != 0
  assert refused.stdout == b""
  # Forcing pass-through copies the unrecognized stream byte for byte.
  let forced = decode_stdin(ctx, "xz,lzma", gzip, true)?
  assert forced.status == 0
  assert forced.stdout == gzip
  let source = fp"{root}/plain.gz"
  source.write(gzip)
  test.error_kind(compression.transform(source, fp"{root}/out", "xz,lzma", decode: true), "compression-transform")
  assert ! fp"{root}/out".exists()?
}

test test_compression_detection_passes_unrecognized_input_through_only_when_asked { |ctx|
  let plain = b"not compressed\0\xff\n"
  for format in ["auto", "xz", "xz,lzma", "lzma", "gzip", "bzip2", "zstd"] {
    let forced = decode_stdin(ctx, format, plain, true)?
    assert forced.status == 0, format
    assert forced.stdout == plain, format
    let refused = decode_stdin(ctx, format, plain, false)?
    assert refused.status != 0, format
    assert refused.stdout == b"", format
  }
  # A named lzma stream is not recognized in a different container.
  let xz = pack(ctx, "xz")?
  let passed = decode_stdin(ctx, "lzma", xz, true)?
  assert passed.status == 0
  assert passed.stdout == xz
}

test test_compression_detected_lzma_header_is_strict_and_named_lzma_is_lenient { |ctx|
  # A plausible raw-lzma header whose dictionary size is neither 2^n nor
  # 2^n + 2^(n-1): detection will not claim it, but a caller that named lzma
  # still hands it to the lzma decoder, which rejects the junk body.
  let odd = bytes.concat([b"\x5d\x05\0\0\0", b"\xff\xff\xff\xff\xff\xff\xff\xff", b"junk junk"])
  let detected = decode_stdin(ctx, "xz,lzma", odd, true)?
  assert detected.status == 0
  assert detected.stdout == odd
  let named = decode_stdin(ctx, "lzma", odd, true)?
  assert named.status != 0
  assert named.stdout != odd
}

test test_compression_detection_contract_errors { |ctx|
  let root = test.temp_dir(ctx, name: "compression-auto-errors")?
  let source = fp"{root}/plain"
  source.write(PLAIN)
  # Encoding has no input to inspect, so it needs one explicit format.
  test.error_kind(compression.transform(source, fp"{root}/a", "auto"), "compression-transform")
  test.error_kind(compression.transform(source, fp"{root}/b", "xz,lzma"), "compression-transform")
  test.error_kind(compression.transform(source, null, "xz,auto", decode: true), "compression-transform")
  test.error_kind(compression.transform(source, null, "xz,", decode: true), "compression-transform")
  # Lzip streams are recognized but cannot be decoded here.
  let lzip = fp"{root}/plain.lz"
  lzip.write(b"LZIP\x01\x0c\0\0\0\0\0\0\0\0\0")
  test.error_kind(compression.transform(lzip, null, "auto", decode: true), "compression-transform")
}
