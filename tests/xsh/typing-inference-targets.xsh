test generic_identity_payloads [error] { |ctx|
  let output = test.run_script(ctx, r"""pure identity(value) { value }
pure unit() -> Unit {}
let number: Int = identity(1)
let text: Str = identity("one")
let no: Bool = identity(false)
let nothing: Unit = identity(unit())
let maybe: Int? = 7
let nullable: Int? = identity(maybe)
let nested: Result[Result[Int]] = Ok(Ok(9))
let retained: Result[Result[Int]] = identity(nested)
print $number $text $no ${nullable ?? 0} ${(retained?)?}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "1 one false 7 9\n"
}

test generic_discard_fixed [error] { |ctx|
  let output = test.run_script(ctx, r"""pure discard(value) { let _ = value; 1 }
let result: Result[Int] = Ok(7)
print ${discard(false)} ${discard(result)} ${discard(null)}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "1 1 1\n"
}

test omitted_proc_bool_value [error] { |ctx|
  let output = test.run_script(ctx, r"""proc boolean() [error] { false }
proc early() [error] { return false }
print ${boolean()} ${early()}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "false false\n"
}

test inferred_result_tail_and_early [error] { |ctx|
  let output = test.run_script(ctx, r"""pure tail(value) { value.parse_int()? }
pure early(value) { let parsed = value.parse_int()?; return parsed }
print ${tail("7")?} ${early("8")?}
print ${tail("bad") is Err(_)}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "7 8\ntrue\n"
}

test generic_result_payload_nesting [error] { |ctx|
  let output = test.run_script(ctx, r"""error LocalError = Bad(message: Str)
pure wrap(value, gate: Result[Unit]) { gate?; value }
let gate: Result[Unit] = Ok()
let inner: Result[Int] = Ok(7)
let nested: Result[Result[Int]] = wrap(inner, gate)
print ${(nested?)?}
let boolean: Result[Bool] = wrap(false, gate)
print ${boolean?}
pure unit() -> Unit {}
let nothing: Result[Unit] = wrap(unit(), gate)
let maybe: Int? = null
let nullable: Result[Int?] = wrap(maybe, gate)
let failed: Result[Int] = Err(LocalError.Bad("inner error"))
let error_data: Result[Result[Int]] = wrap(failed, gate)
print ${nothing is Ok(_)} ${nullable? == null} ${error_data? is Err(_)}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "7\nfalse\ntrue true true\n"
}

test independent_result_return [error] { |ctx|
  let output = test.run_script(ctx, r"""pure parsed(value) { let parsed = value.parse_int()?; Ok(parsed) }
let result: Result[Int] = parsed("7")
print ${result?}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "7\n"
}

test inferred_producer_lifecycle [error] { |ctx|
  let output = test.run_script(ctx, r"""proc mark(label, value) { let _ = time.now(); print $label; value }
stream child(item = mark("default", 7)) {
  defer { print "child-close" }
  yield item
  yield 8
}
stream parent() { defer { print "parent-close" }; yield @child() }
let _ = child()
let source = parent()
let _ = child(mark("supplied", 9))
print created
for item in source { print $item; break }
print after
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "supplied\ncreated\ndefault\n7\nchild-close\nparent-close\nafter\n"
}

test inferred_default_rest_context [error] { |ctx|
  let output = test.run_script(ctx, r"""let seed = 6
pure fallback(value = null) { value ?? seed }
pure count(...values) { values.len() }
pure chosen(value = []) { value }
let empty: List[Int] = chosen()
print ${fallback()} ${fallback(9)} ${count(1, 2)} ${empty.len()}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "6 9 2 0\n"
}
