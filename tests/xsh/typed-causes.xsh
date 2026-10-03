test test_error_variant_payloads_evaluate_once_in_written_order { |ctx|
  let output = test.run_script(ctx, r"""
error Failure = Failed(kind: Str, message: Str) : InvalidData
proc marker(label: Str) [io] -> Str { print $label; label }
proc made() [io] -> Failure {
  Failure.Failed(message: marker("message"), kind: marker("kind"))
}
match made() {
  Failure.Failed {kind, message} => print $kind $message
  _ => print "wrong"
}
""")?
  assert output.success, output.stderr
  output.stdout == "message\nkind\nkind message\n"
}

test test_error_variant_wrong_payload_field_refuses_before_effects { |ctx|
  for expression in [
    "Failure.Failed(message: 4)",
    "Failure.Failed(other: \"wrong\")",
    "Err(Failure.Failed(message: \"outer\"), cause: \"wrong\")",
  ] {
    let source = "error Failure = Failed(message: Str)\nprint \"executed\"\nlet value = " + expression + "\n"
    let output = test.run_script(ctx, source)?
    output.status == 2
    output.stdout == ""
    "compact-unsupported" not in output.stderr
  }
}

test test_err_typed_cause_preserves_outer_nominal_contract { |ctx|
  let output = test.run_script(ctx, """
error BuildError = Failed(message: Str, cause: Str) : InvalidData
error InputError = Missing(message: Str) : NotFound
let outer = BuildError.Failed(message: "build", cause: "payload")
let inner = InputError.Missing(message: "input")
let translated: Result[Str, BuildError] = Err(outer, cause: inner)
match translated {
  Err(BuildError.Failed {message, cause}) => print $message $cause
  _ => print "wrong"
}
pure present(failure: Error) -> Error { failure }
let observed = match translated { Err(failure) => present(failure); _ => outer }
match observed {
  is NotFound => print "wrong inner facet"
  _ => print "outer only"
}
print "constructed"
translated?
""")?
  (output.status) == (3)
  (output.stdout) == ("build payload\nouter only\nconstructed\n")
  ("BuildError.Failed" in output.stderr)
  ("InputError.Missing" in output.stderr)
}

test test_err_typed_cause_aliases_and_generic_one_argument_remain_data { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
let outer = OuterError.Failed(message: "outer")
let alias = outer
let attached = Err(outer, cause: InnerError.Failed(message: "inner"))
let generic: Result[Int, Str] = Err("ordinary generic error")
match generic { Err(text) => print $text; _ => print "wrong" }
print "constructed"
Err(alias)?
""")?
  (output.status) == (3)
  (output.stdout) == ("ordinary generic error\nconstructed\n")
  ("OuterError.Failed" in output.stderr)
  {
    let assertion_condition = ("InnerError" not in output.stderr)
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
}

test test_err_typed_cause_arguments_run_once_in_written_order { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
var history = 0
proc outer() -> OuterError { history = history * 10 + 1; OuterError.Failed(message: "outer") }
proc inner() -> InnerError { history = history * 10 + 2; InnerError.Failed(message: "inner") }
let first = Err(outer(), cause: inner())
print $history
history = 0
let reordered = Err(cause: inner(), outer())
print $history
history = 0
let spread = Err(outer(), ...{cause: inner()})
print $history
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("12\n21\n12\n")
}

test test_err_typed_cause_explicit_replacement_preserves_supplied_chain { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
error LeafError = Failed(message: Str)
let outer = OuterError.Failed(message: "outer")
let first = Err(outer, cause: LeafError.Failed(message: "replaced"))
let inherited = match first { Err(failure) => failure; _ => outer }
let inner = InnerError.Failed(message: "inner")
let second = Err(inner, cause: LeafError.Failed(message: "retained"))
let supplied = match second { Err(failure) => failure; _ => inner }
let translated = Err(inherited, cause: supplied)
translated?
""")?
  (output.status) == (3)
  ("OuterError.Failed" in output.stderr)
  ("InnerError.Failed" in output.stderr)
  ("LeafError.Failed" in output.stderr)
  ("retained" in output.stderr)
  {
    let assertion_condition = ("replaced" not in output.stderr)
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
}

test test_err_typed_cause_rejects_invalid_arguments { |ctx|
  for source in [
    "error E = Failed(message: Str)\nlet value = Err(E.Failed(message: \"outer\"), cause: \"text\")\n",
    "error E = Failed(message: Str)\nlet value = Err(\"text\", cause: E.Failed(message: \"inner\"))\n",
    "error E = Failed(message: Str)\nlet value = Err(E.Failed(message: \"outer\"), E.Failed(message: \"inner\"))\n",
    "error E = Failed(message: Str)\nlet value = Err(E.Failed(message: \"outer\"), cause: E.Failed(message: \"inner\"), cause: E.Failed(message: \"again\"))\n",
    "error E = Failed(message: Str)\nlet value = Err(E.Failed(message: \"outer\"), other: E.Failed(message: \"inner\"))\n",
  ] {
    let output = test.run_script(ctx, source)?
    (output.status) == (2)
    {
      let assertion_condition = ("compact-unsupported" not in output.stderr)
      let assertion_message = output.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_err_typed_cause_human_rendering_escapes_control_characters { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
let translated = Err(OuterError.Failed(message: "outer"), cause: InnerError.Failed(message: "line\nforged\u{1b}[31m"))
translated?
""")?
  (output.status) == (3)
  ("caused by: InnerError.Failed" in output.stderr)
  {
    let assertion_condition = ("\nforged" not in output.stderr)
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = ("\u{1b}" not in output.stderr)
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
}

test test_err_typed_cause_trace_preserves_process_status_and_context_spans { |ctx|
  let output = test.run_xsht_trace(ctx, r"""
error BuildError = Failed(message: Str)
let outcome: Result[Unit, ProcessError] = try {
  ctx "execute input" { run sh -c "exit 7" }
}
let translated = match outcome {
  Err(failure) => Err(BuildError.Failed(message: "build"), cause: failure)
  _ => Err(BuildError.Failed(message: "unexpected success"))
}
ctx "publish" { translated? }
""", ["--raw", "--trace-format", "jsonl"])?
  (output.status) == (3)
  ("\"family\":\"BuildError\"" in output.stderr)
  ("\"family\":\"ProcessError\"" in output.stderr)
  ("\"variant\":\"NonzeroExit\"" in output.stderr)
  ("\"code\":7" in output.stderr)
  ("\"message\":\"execute input\"" in output.stderr)
  ("\"message\":\"publish\"" in output.stderr)
  ("\"start_line\"" in output.stderr)
  ("\"causes_truncated\":false" in output.stderr)
}

test test_err_typed_cause_one_argument_retains_existing_chain_through_try { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
let translated: Result[Unit, OuterError] = Err(OuterError.Failed(message: "outer"), cause: InnerError.Failed(message: "inner"))
let captured: Result[Unit, OuterError] = try { ctx "attempt" { translated? } }
let preserved: Result[Unit, OuterError] = match captured { Err(failure) => Err(failure); _ => translated }
ctx "caller" { preserved? }
""")?
  (output.status) == (3)
  ("OuterError.Failed" in output.stderr)
  ("InnerError.Failed" in output.stderr)
  ("attempt" in output.stderr)
  ("caller" in output.stderr)
}

test test_err_typed_cause_abort_operand_keeps_control_transfer { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
proc cause() -> InnerError { abort(17); InnerError.Failed(message: "unreachable") }
let translated: Result[Unit] = try { let data: Result[Unit, OuterError] = Err(OuterError.Failed(message: "outer"), cause: cause()) }
print "wrong after abort"
""")?
  {
    let assertion_condition = output.status == 17
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  (output.status) == (17)
  (output.stdout) == ("")
  (output.stderr) == ("")
}

test test_err_typed_cause_procedure_frames_construct_result_data { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
proc translate(failure: InnerError) -> Result[Int, OuterError] {
  Err(OuterError.Failed(message: "translated"), cause: failure)
}
let data = translate(InnerError.Failed(message: "original"))
print "returned data"
data?
""")?
  (output.status) == (3)
  (output.stdout) == ("returned data\n")
  ("OuterError.Failed" in output.stderr)
  ("InnerError.Failed" in output.stderr)
}

test test_err_typed_cause_long_native_chain_reports_truncation { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
var current: Error = OuterError.Failed(message: "leaf")
for index in range(1000) {
  let attached = Err(OuterError.Failed(message: "translation"), cause: current)
  current = match attached { Err(failure) => failure; _ => current }
}
let outcome: Result[Unit] = Err(current)
outcome?
""")?
  (output.status) == (3)
  ("cause chain truncated" in output.stderr)
  {
    let assertion_condition = output.stderr.count_chars() < 10000
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
}

test test_err_typed_cause_retains_checked_assertion_failure { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
let outcome: Result[Unit] = try { assert false, "checked leaf" }
let translated = match outcome {
  Err(failure) => Err(OuterError.Failed(message: "translation"), cause: failure)
  _ => Err(OuterError.Failed(message: "unexpected success"))
}
translated?
""")?
  (output.status) == (3)
  ("OuterError.Failed" in output.stderr)
  ("AssertionError" in output.stderr)
  ("checked leaf" in output.stderr)
}

test test_err_one_argument_generic_propagation_keeps_existing_diagnostic { |ctx|
  let output = test.run_script(ctx, r"""
let value: Result[Unit, Str] = Err("generic error data")
value?
""")?
  (output.status) == (3)
  ("error: error: propagated error" in output.stderr)
  {
    let assertion_condition = ("caused by" not in output.stderr)
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
}

test test_err_typed_cause_process_outer_survives_cleanup_transport { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
proc cleanup(original: ProcessError) -> Result[Unit, ProcessError] {
  Err(original, cause: OuterError.Failed(message: "cleanup cause"))
}
let original_result: Result[Unit, ProcessError] = try { run sh -c "exit 7" }
match original_result {
  Err(original) => {
    let captured: Result[Unit, ProcessError] = try {
      defer cleanup(original)?
      print "body completed"
    }
    captured?
  }
  _ => abort(19)
}
""")?
  (output.status) == (3)
  (output.stdout) == ("body completed\n")
  ("ProcessError.NonzeroExit" in output.stderr)
  ("OuterError.Failed" in output.stderr)
  ("[exit 7]" in output.stderr)
  ("cleanup cause" in output.stderr)
}
