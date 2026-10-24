enum JsonMode: Str { Ready = "ready" }

pure keep_json(payload, level: Int, kind: Str, kernel_id: Int?) {
  let encoded = json.encode({level, kind, kernel_id})?
  if kernel_id == null {
    assert encoded == """{"kernel_id":null,"kind":"L2","level":2}"""
  }
  Ok(payload)
}

pure keep_pretty_json(payload) {
  let compact = json.encode({enabled: true}, pretty: false)?
  let pretty = json.encode({enabled: true}, pretty: true)?
  assert compact == """{"enabled":true}"""
  assert pretty.byte_len() > compact.byte_len()
  Ok(payload)
}

pure keep_wire_json(payload, mode: JsonMode) {
  let encoded = json.encode({mode})?
  assert encoded == """{"mode":"ready"}"""
  Ok(payload)
}

pure checked_json(level: Int, kind: Str, kernel_id: Int?) -> Result[Str, Error] {
  json.encode({level, kind, kernel_id})
}

pure keep_dynamic_json(payload, value: Any) {
  {payload, encoded: json.encode(value)}
}

test generic_json_records [error] {
  assert keep_json(7, 2, "L2", null)? == 7
  assert keep_json("kept", 3, "L3", 5)? == "kept"
  assert keep_pretty_json(9)? == 9
  assert keep_wire_json(11, .Ready)? == 11
  assert checked_json(2, "L2", null)? == """{"kernel_id":null,"kind":"L2","level":2}"""
  let dynamic = keep_dynamic_json("kept", 7)
  assert dynamic.payload == "kept"
  assert dynamic.encoded? == "7"
  let unsupported = keep_dynamic_json(7, p"unsupported")
  assert unsupported.payload == 7
  assert match unsupported.encoded { Err(_) => true, Ok(_) => false }
}
