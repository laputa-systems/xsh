proc test_bytes_construction_encoding_and_copy(ctx: TestContext) [fs, error] {
  let data = bytes.concat([bytes.from_text("A"), bytes.from_ints([66, 67])?, bytes.zero(2)?])
  test.eq(data, b"ABC\0\0")?
  test.eq(bytes.human(-1), "-")?
  test.eq(bytes.human(0), "0")?
  test.eq(bytes.human(9), "9")?
  test.eq(bytes.human(1024), "1.0K")?
  test.eq(bytes.human(1536), "1.5K")?
  test.eq(bytes.human(10 * 1024), "10K")?
  test.eq(bytes.human(1024 * 1024), "1.0M")?
  test.eq(bytes.human(5 * 1024 * 1024 * 1024), "5.0G")?
  test.eq(bytes.pack_le(4660, 2)?, b"4\x12")?
  test.eq(bytes.pack_be(16909060, 4)?, b"\x01\x02\x03\x04")?
  test.eq(bytes.unpack_le(b"4\x12", 2)?, 4660)?
  test.eq(bytes.unpack_be(b"\x01\x02\x03\x04", 4)?, 16909060)?
  test.error_kind(bytes.from_ints([256]), "bytes-from-ints")?
  test.error_kind(bytes.pack_be(1, 9), "bytes-pack")?
  let data_path = test.temp_path(ctx, name: "data.bin")
  test.eq(bytes.write_at(data_path, 2, b"abcdef", create: true)?, 6)?
  test.eq(bytes.zero_at(data_path, 4, 2)?, 2)?
  test.eq(bytes.read_at(data_path, 2, 6)?, b"ab\0\0ef")?
  let copy = test.temp_path(ctx, name: "copy.bin")
  let copied = bytes.copy(data_path, copy, 2, 2, 1, 0, false)?
  test.eq(copied.bytes, 4)?
  test.eq(copied.blocks, 2)?

  let copied_file = bytes.copy_file(
    data_path,
    copy,
    source_offset: 6,
    dest_offset: 1,
    length: 2,
    create: false,
    truncate: false,
  )?

  test.eq(copied_file.bytes, 2)?
  test.eq(copy.read_bytes()?.dump("hex-u8"), "0000000 61 65 66 00")?
  test.error_kind(bytes.copy(data_path, copy), "bytes-copy")?
}

proc test_bytes_methods_and_decode_errors() [error] {
  let encoded = b"\0hello\xff".base64()
  test.eq(encoded, "AGhlbGxv/w==")?
  test.eq(encoded.base64_decode()?, b"\0hello\xff")?

  test.eq(
    """Y
WJj""".base64_decode()?,
    b"abc",
  )?

  test.eq("Zm9v".base64_decode()?, b"foo")?
  let base32 = b"foobar".base32()
  test.eq(base32, "MZXW6YTBOI======")?
  test.eq(base32.base32_decode()?, b"foobar")?
  test.eq("mzxw6ytboi======".base32_decode()?, b"foobar")?
  test.eq("mzxw6ytboi".base32_decode()?, b"foobar")?
  test.eq(b"abcdef".slice(2, length: 3), b"cde")?
  test.eq(b"abc".len(), 3)?
  let report = b"  Header\r\nalpha\nTODO item\nomega  "
  test.eq(report.trim(), b"Header\r\nalpha\nTODO item\nomega")?
  test.ok(b"TODO" in report)?
  test.ok(report.trim().starts_with(b"Header"))?
  test.ok(report.trim().ends_with(b"omega"))?
  test.eq(report.lines().collect(), [b"  Header", b"alpha", b"TODO item", b"omega  "])?
  test.eq(report.count_lines(), 4)?
  test.eq(b"AbC\xff".lower(), b"abc\xff")?
  test.eq(report.byte_at(2), 72)?
  test.eq(report.byte_at(999, -1), -1)?
  test.eq(b"\0hello marker-one\0xx marker-two!!\xff".strings(min_len: 7)[0], "hello marker-one")?
  test.contains(b"hello".dump("hex-u8"), "68 65 6c 6c 6f")?
  test.eq(b"hello".dump("octal-u8"), "0000000 150 145 154 154 157")?
  test.eq(b"hello".utf8()?, "hello")?
  test.eq(b"abcdef".chunks(2).len(), 3)?
  let comparison = b"abc\nxyz".compare(b"abc\nxqz")
  let eof = b"abc".compare(b"abcd")
  test.eq(b"abc".compare(b"abc").equal, true)?
  test.eq(comparison.equal, false)?
  test.eq(comparison.byte, 6)?
  test.eq(comparison.line, 2)?
  test.eq(comparison.left, 121)?
  test.eq(comparison.right, 113)?
  test.eq(eof.byte, 4)?
  test.eq(eof.left, -1)?
  test.eq(eof.right, 100)?
  test.eq(b"abc".md5().hex(), hash.md5(b"abc").hex())?
  test.eq(b"abc".sha1().hex(), hash.sha1(b"abc").hex())?
  test.eq(b"abc".sha256().hex(), hash.sha256(b"abc").hex())?
  test.eq(b"abc".sha512().hex(), hash.sha512(b"abc").hex())?
  test.error_kind(b"\xff".utf8(), "invalid-utf8")?
  test.error_kind("%%%".base64_decode(), "invalid-base64")?
  test.error_kind("M!".base32_decode(), "invalid-base32")?
}
