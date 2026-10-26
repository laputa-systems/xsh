type BinaryAnswer = {answer: Int}

test test_binary_codecs_round_trip_native_bytes {
  let value = {payload: b"\0\x80\xff"}
  let cbor = codec.encode_bytes("cbor", value)?
  let msgpack = codec.encode_bytes("msgpack", value)?
  assert codec.decode_bytes("cbor", cbor)? == value
  assert codec.decode_bytes("msgpack", msgpack)? == value
  assert codec.decode_bytes("cbor", b"\x43\0\x80\xff")? == b"\0\x80\xff"
  assert codec.decode_bytes("msgpack", b"\xc4\x03\0\x80\xff")? == b"\0\x80\xff"
}

test test_binary_codecs_preserve_scalar_map_keys_and_empty_maps {
  let cbor_ints = codec.decode_bytes("cbor", b"\xa1\x01\x02")?.require(Map[Int, Int])?
  let msgpack_ints = codec.decode_bytes("msgpack", b"\x81\x01\x02")?.require(Map[Int, Int])?
  assert cbor_ints[1] == 2
  assert msgpack_ints[1] == 2

  let cbor_bools = codec.decode_bytes("cbor", b"\xa1\xf4\x02")?.require(Map[Bool, Int])?
  let msgpack_bools = codec.decode_bytes("msgpack", b"\x81\xc2\x02")?.require(Map[Bool, Int])?
  assert cbor_bools[false] == 2
  assert msgpack_bools[false] == 2

  let cbor_bytes = codec.decode_bytes("cbor", b"\xa1\x42\x80\xff\x02")?.require(Map[Bytes, Int])?
  let msgpack_bytes = codec.decode_bytes("msgpack", b"\x81\xc4\x02\x80\xff\x02")?.require(Map[Bytes, Int])?
  assert cbor_bytes[b"\x80\xff"] == 2
  assert msgpack_bytes[b"\x80\xff"] == 2

  let cbor_record = codec.decode_bytes("cbor", b"\xa1\x66answer\x01")?.require(BinaryAnswer)?
  let msgpack_record = codec.decode_bytes("msgpack", b"\x81\xa6answer\x01")?.require(BinaryAnswer)?
  assert cbor_record.answer == 1
  assert msgpack_record.answer == 1

  let cbor_empty = codec.decode_bytes("cbor", b"\xa0")?.require(Map[Int, Int])?
  let msgpack_empty = codec.decode_bytes("msgpack", b"\x80")?.require(Map[Int, Int])?
  assert cbor_empty.is_empty()
  assert msgpack_empty.is_empty()
}

test test_binary_codecs_canonical_cbor_and_metadata_rejection {
  let value = {b: 2, a: 1}
  assert codec.encode_bytes("cbor", value, canonical: true)? == b"\xa2\x61a\x01\x61b\x02"
  assert codec.encode_bytes("msgpack", value)? == b"\x82\xa1a\x01\xa1b\x02"
  test.error_kind(codec.encode_bytes("msgpack", value, canonical: true), "CodecError.Options")

  test.error_kind(codec.decode_bytes("cbor", b"\xc0\x01"), "CodecError.UnsupportedValue")
  test.error_kind(codec.decode_bytes("msgpack", b"\xd6\xff\x51\x4b\x67\xb0"), "CodecError.UnsupportedValue")
  test.error_kind(codec.decode_bytes("msgpack", b"\xd4\x2a\0"), "CodecError.UnsupportedValue")
}

test test_binary_codecs_reject_overflow_complex_keys_and_incomplete_values {
  test.error_kind(codec.decode_bytes("cbor", b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff"), "CodecError.Overflow")
  test.error_kind(codec.decode_bytes("msgpack", b"\xcf\xff\xff\xff\xff\xff\xff\xff\xff"), "CodecError.Overflow")
  test.error_kind(codec.decode_bytes("cbor", b"\xa1\x80\x01"), "CodecError.UnsupportedValue")
  test.error_kind(codec.decode_bytes("msgpack", b"\x81\x90\x01"), "CodecError.UnsupportedValue")
  test.error_kind(codec.decode_bytes("cbor", b"\x5a\0\0"), "CodecError.Syntax")
  test.error_kind(codec.decode_bytes("msgpack", b"\xc6\0\0"), "CodecError.Syntax")
  test.error_kind(codec.decode_bytes("cbor", b"\x01\x02"), "CodecError.Syntax")
  test.error_kind(codec.decode_bytes("msgpack", b"\x01\x02"), "CodecError.Syntax")
}
