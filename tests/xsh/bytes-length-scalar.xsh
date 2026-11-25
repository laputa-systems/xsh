pure byte_lengths(data: Bytes) -> List[Int] {
  var lengths = []
  for line in data.lines() {
    let length = line.len()
    lengths += [length + 1]
  }
  lengths
}

test test_bytes_length_in_scalar_expressions {
  assert byte_lengths(b"a\0b\r\n\xff\n\nlast") == [4, 2, 1, 5]
  assert b"".len() == 0
  assert b"\xc3\xa9".len() == 2
  let data = b"abc\0\xff"
  assert data.len() == 5
  assert data[1..4].len() == 3
  assert [1, 2].len() == 2
}
