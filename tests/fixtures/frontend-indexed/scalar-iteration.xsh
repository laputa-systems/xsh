proc scalar_count() [] -> Int {
  var count = 0
  for character in "é🙂" { count += character.count_bytes() }
  for octet in b"\x00\xff" { count += octet }
  count
}
proc scalar_comp() [] -> Int {
  let values = [octet for character in "éx" for octet in b"\x01\x02" if character == "é"]
  values[0] + values[1]
}
proc scalar_result() [error] -> Result[Int] {
  var count = 0
  for octet in Ok(b"\xff") { count += octet }
  count
}
error ScalarFailure = Missing(source: Str) : NotFound
proc missing_bytes() [error] -> Result[Bytes, ScalarFailure] { Err(ScalarFailure.Missing(source: "bytes")) }
proc scalar_failure() [error] -> Result[Int, ScalarFailure] {
  ctx "scalar source" { for octet in missing_bytes() { let _ = octet } }
  1
}
