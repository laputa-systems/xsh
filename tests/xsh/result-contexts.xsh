test test_result_return_match_preserves_constructor_context { |ctx|
  let output = test.run_script(ctx, r"""
error DecodeError = Invalid(message: Str)
enum Json { JNum(Float) }
pure decode_num(tok: Str) -> Result[Json] {
  return match json.decode(tok)? {
    i is Int => Ok(JNum(i.float())),
    f is Float => Ok(JNum(f)),
    _ => Err(DecodeError.Invalid(message: "invalid JSON number")),
  }
}
pure ordinary(value: Bool) -> Result[Int] {
  return match value { true => 7, false => 9 }
}
pure conditional(value: Bool) -> Result[Int] {
  return if value { Ok(11) } else { Err(DecodeError.Invalid(message: "conditional")) }
}
pure block_result() -> Result[Int] { return { Ok(13) } }
print ${decode_num("1")? is JNum(_)}
print ${decode_num("\"text\"") is Err(DecodeError.Invalid)}
print ${ordinary(true)?}
print ${conditional(true)?}
print ${conditional(false) is Err(DecodeError.Invalid)}
print ${block_result()?}
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert (output.stdout) == ("true\ntrue\n7\n11\ntrue\n13\n")
}

test test_result_unit_nested_match_tail_keeps_error_data { |ctx|
  let output = test.run_script(ctx, r"""
error E = Failed(message: Str)
let original: Result[Unit, E] = Err(E.Failed(message: "inner"))
let translated: Result[Unit, E] = match original {
  Err(outer) => {
    match original {
      Err(failure) => Err(failure),
      _ => Ok(),
    }
  },
  _ => Ok(),
}
print ${translated is Err(E.Failed)}
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert (output.stdout) == ("true\n")
}
