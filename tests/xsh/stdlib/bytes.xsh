test test_bytes_construction_encoding_and_copy { |ctx|
  let data = bytes.concat([bytes.from_text("A"), bytes.from_ints([66, 67])?, bytes.zero(2)?])
  assert data == b"ABC\0\0"
  assert bytes.human(-1) == "-"
  assert bytes.human(0) == "0"
  assert bytes.human(9) == "9"
  assert bytes.human(1023) == "1023"
  assert bytes.human(1024) == "1.0K"
  assert bytes.human(1536) == "1.5K"
  assert bytes.human(10KiB) == "10K"
  assert bytes.human(1MiB) == "1.0M"
  assert bytes.human(5GiB) == "5.0G"
  assert bytes.pack_le(4660, 2)? == b"4\x12"
  assert bytes.pack_be(16909060, 4)? == b"\x01\x02\x03\x04"
  assert bytes.unpack_le(b"4\x12", 2)? == 4660
  assert bytes.unpack_be(b"\x01\x02\x03\x04", 4)? == 16909060
  assert bytes.unpack_float(b"\x3e\x00", 0, "binary16", "big")? == 1.5
  assert bytes.unpack_float(b"\x3f\xc0", 0, "bfloat16", "big")? == 1.5
  assert bytes.unpack_float(b"\x00\x00\xc0\x3f", 0, "binary32", "little")? == 1.5
  assert bytes.unpack_float(b"\x3f\xf8\0\0\0\0\0\0", 0, "binary64", "big")? == 1.5
  assert bytes.unpack_float(b"\x01\0", 0, "binary16", "little")? > 0.0
  test.error_kind(bytes.unpack_float(b"\x00", 0, "binary16", "big"), "bytes-unpack-float")
  test.error_kind(bytes.unpack_float(b"\0\0", -1, "binary16", "big"), "bytes-unpack-float")
  test.error_kind(bytes.unpack_float(b"\0\0", 0, "float", "big"), "bytes-unpack-float")
  test.error_kind(bytes.unpack_float(b"\0\0", 0, "binary16", "native"), "bytes-unpack-float")
  test.error_kind(bytes.from_ints([256]), "bytes-from-ints")
  test.error_kind(bytes.pack_be(1, 9), "bytes-pack")
  let data_path = test.temp_path(ctx, name: "data.bin")
  assert bytes.write_at(data_path, 2, b"abcdef", create: true)? == 6
  assert bytes.zero_at(data_path, 4, 2)? == 2
  assert bytes.read_at(data_path, 2, 6)? == b"ab\0\0ef"
  let copy = test.temp_path(ctx, name: "copy.bin")
  let copied = bytes.copy(data_path, copy, 2, 2, 1, 0, false)?
  assert copied.bytes == 4
  assert copied.blocks == 2

  let copied_file = bytes.copy_file(
    data_path,
    copy,
    source_offset: 6,
    dest_offset: 1,
    length: 2,
    create: false,
    truncate: false,
  )?

  assert copied_file.bytes == 2
  assert copy.read_bytes()?.dump("hex-u8") == "0000000 61 65 66 00"
  test.error_kind(bytes.copy(data_path, copy), "bytes-copy")
}

test test_bytes_squeeze_collapses_only_the_selected_byte {
  assert bytes.squeeze(b"   a\t\t  ", 32)? == b" a\t\t "
  assert bytes.squeeze(b"\xff\xffx\xff", 255)? == b"\xffx\xff"
  assert bytes.squeeze(b"", 0)? == b""
  test.error_kind(bytes.squeeze(b"x", -1), "bytes-squeeze")
  test.error_kind(bytes.squeeze(b"x", 256), "bytes-squeeze")
}

test test_bytes_counts_complete_repeated_prefixes {
  assert bytes.repeat_prefix_count(b"ababax", b"ab")? == 2
  assert bytes.repeat_prefix_count(b"xyxy", b"xy")? == 2
  assert bytes.repeat_prefix_count(b"", b"x")? == 0
  assert bytes.repeat_prefix_count(b"yxx", b"x")? == 0
  test.error_kind(bytes.repeat_prefix_count(b"abc", b""), "bytes-repeat-prefix-count")
}

test test_bytes_methods_and_decode_errors {
  let encoded = b"\0hello\xff".base64()
  assert encoded == "AGhlbGxv/w=="
  assert encoded.base64_decode()? == b"\0hello\xff"

  assert """Y
WJj""".base64_decode()? == b"abc"

  assert "Zm9v".base64_decode()? == b"foo"
  let base32 = b"foobar".base32()
  assert base32 == "MZXW6YTBOI======"
  assert base32.base32_decode()? == b"foobar"
  assert "mzxw6ytboi======".base32_decode()? == b"foobar"
  assert "mzxw6ytboi".base32_decode()? == b"foobar"
  assert b"abcdef"[2..5] == b"cde"
  assert b"abc".len() == 3
  let report = b"  Header\r\nalpha\nTODO item\nomega  "
  assert report.trim() == b"Header\r\nalpha\nTODO item\nomega"
  assert b"TODO" in report
  assert report.trim().starts_with(b"Header")
  assert report.trim().ends_with(b"omega")
  assert report.lines().collect() == [b"  Header", b"alpha", b"TODO item", b"omega  "]
  assert report.count_lines() == 4
  assert b"AbC\xff".lower() == b"abc\xff"
  assert (report.byte_at(2) ?? -1) == 72
  assert (report.byte_at(999) ?? -1) == -1
  assert b"\0hello marker-one\0xx marker-two!!\xff".strings(min_len: 7)[0] == "hello marker-one"
  assert "68 65 6c 6c 6f" in b"hello".dump("hex-u8")
  assert b"hello".dump("octal-u8") == "0000000 150 145 154 154 157"
  assert b"hello" as Str == "hello"
  assert b"abcdef".chunks(2).len() == 3
  let comparison = b"abc\nxyz".compare(b"abc\nxqz")
  let eof = b"abc".compare(b"abcd")
  assert b"abc".compare(b"abc").equal == true
  assert comparison.equal == false
  assert comparison.byte == 6
  assert comparison.line == 2
  assert comparison.left == 121
  assert comparison.right == 113
  assert eof.byte == 4
  assert eof.left == -1
  assert eof.right == 100
  assert b"abc".md5().hex() == hash.md5(b"abc").hex()
  assert b"abc".sha1().hex() == hash.sha1(b"abc").hex()
  assert b"abc".sha256().hex() == hash.sha256(b"abc").hex()
  assert b"abc".sha512().hex() == hash.sha512(b"abc").hex()
  test.error_kind(b"\xff".utf8(), "invalid-utf8")
  test.error_kind("%%%".base64_decode(), "invalid-base64")
  test.error_kind("M!".base32_decode(), "invalid-base32")
}

pure bytes_byte_at(data: Bytes, index: Int) -> Int? {
  data.byte_at(index)
}

pure text_byte_at(text: Str, index: Int) -> Int? {
  text.byte_at(index)
}

# `byte_at` is one lowered operation for both receivers; it reads the bytes of
# a `Bytes` value, not its text, and is null outside the value on either.
test test_byte_at_reads_bytes_and_text_through_typed_receivers {
  let data = b"a\xffc"
  assert bytes_byte_at(data, 0) == 97
  assert bytes_byte_at(data, 1) == 255
  assert bytes_byte_at(data, 3) == null
  assert bytes_byte_at(data, -1) == null
  assert text_byte_at("abc", 2) == 99
  assert text_byte_at("abc", 3) == null
  let dynamic: Any = data
  assert dynamic.byte_at(1).require(Int)? == 255
}
