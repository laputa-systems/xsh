const INITIAL = {number: 0}

proc empty_body(payload) {}

proc literal_body(payload) {
  var current = INITIAL
  let _ = payload
  if current.number == 0 { print "literal" }
}

proc final_if(selected: Bool, payload) {
  let _ = payload
  if selected { print "selected" }
}

proc final_for(items: List[Int], payload) {
  let _ = payload
  for item in items { let _ = item }
}

proc final_while(selected: Bool, payload) {
  let _ = payload
  var remaining = selected
  while remaining { remaining = false }
}

proc final_loop(payload) {
  let _ = payload
  loop { break }
}

proc bind_text(binary: Str, payload) {
  let output = run.text $binary fixed
  {output, payload}
}

proc bind_capture(binary: Str, payload) {
  var captured = run.capture --text $binary fixed
  {output: captured.stdout, status_ok: captured.status.ok, payload}
}

test generic_proc_statement_tails_preserve_unit {
  assert empty_body(7) == Ok()
  assert empty_body("word") == Ok()
  assert literal_body(7) == Ok()
  assert literal_body("word") == Ok()
  assert final_if(false, 7) == Ok()
  assert final_if(true, "word") == Ok()
  assert final_for([], 7) == Ok()
  assert final_for([1, 2], "word") == Ok()
  assert final_while(false, 7) == Ok()
  assert final_while(true, "word") == Ok()
  assert final_loop(7) == Ok()
  assert final_loop("word") == Ok()
}

test generic_proc_command_bindings_preserve_outputs {
  let text_number = bind_text("/bin/echo", 7)?
  let text_word = bind_text("/bin/echo", "word")?
  assert text_number == {output: "fixed\n", payload: 7}
  assert text_word == {output: "fixed\n", payload: "word"}
  let capture_number = bind_capture("/bin/echo", 7)?
  let capture_word = bind_capture("/bin/echo", "word")?
  assert capture_number == {output: "fixed\n", status_ok: true, payload: 7}
  assert capture_word == {output: "fixed\n", status_ok: true, payload: "word"}
}
