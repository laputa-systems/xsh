test test_err_typed_cause_preserves_outer_nominal_contract [error] { |ctx|
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
  test.eq(output.status, 3)?
  test.eq(output.stdout, "build payload\nouter only\nconstructed\n")?
  test.contains(output.stderr, "BuildError.Failed")?
  test.contains(output.stderr, "InputError.Missing")?
}

test test_err_typed_cause_aliases_and_generic_one_argument_remain_data [error] { |ctx|
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
  test.eq(output.status, 3)?
  test.eq(output.stdout, "ordinary generic error\nconstructed\n")?
  test.contains(output.stderr, "OuterError.Failed")?
  test.ok(!output.stderr.contains("InnerError"), output.stderr)?
}

test test_err_typed_cause_arguments_run_once_in_written_order [error] { |ctx|
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "12\n21\n12\n")?
}

test test_err_typed_cause_explicit_replacement_preserves_supplied_chain [error] { |ctx|
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
  test.eq(output.status, 3)?
  test.contains(output.stderr, "OuterError.Failed")?
  test.contains(output.stderr, "InnerError.Failed")?
  test.contains(output.stderr, "LeafError.Failed")?
  test.contains(output.stderr, "retained")?
  test.ok(!output.stderr.contains("replaced"), output.stderr)?
}

test test_err_typed_cause_rejects_invalid_arguments [error] { |ctx|
  for source in [
    "error E = Failed(message: Str)\nlet value = Err(E.Failed(message: \"outer\"), cause: \"text\")\n",
    "error E = Failed(message: Str)\nlet value = Err(\"text\", cause: E.Failed(message: \"inner\"))\n",
    "error E = Failed(message: Str)\nlet value = Err(E.Failed(message: \"outer\"), E.Failed(message: \"inner\"))\n",
    "error E = Failed(message: Str)\nlet value = Err(E.Failed(message: \"outer\"), cause: E.Failed(message: \"inner\"), cause: E.Failed(message: \"again\"))\n",
    "error E = Failed(message: Str)\nlet value = Err(E.Failed(message: \"outer\"), other: E.Failed(message: \"inner\"))\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.status, 2)?
    test.ok(!output.stderr.contains("compact-unsupported"), output.stderr)?
  }
}

test test_err_typed_cause_human_rendering_escapes_control_characters [error] { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
let translated = Err(OuterError.Failed(message: "outer"), cause: InnerError.Failed(message: "line\nforged\u{1b}[31m"))
translated?
""")?
  test.eq(output.status, 3)?
  test.contains(output.stderr, "caused by: InnerError.Failed")?
  test.ok(!output.stderr.contains("\nforged"), output.stderr)?
  test.ok(!output.stderr.contains("\u{1b}"), output.stderr)?
}

test test_err_typed_cause_trace_preserves_process_status_and_context_spans [error] { |ctx|
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
  test.eq(output.status, 3)?
  test.contains(output.stderr, "\"family\":\"BuildError\"")?
  test.contains(output.stderr, "\"family\":\"ProcessError\"")?
  test.contains(output.stderr, "\"variant\":\"NonzeroExit\"")?
  test.contains(output.stderr, "\"code\":7")?
  test.contains(output.stderr, "\"message\":\"execute input\"")?
  test.contains(output.stderr, "\"message\":\"publish\"")?
  test.contains(output.stderr, "\"start_line\"")?
  test.contains(output.stderr, "\"causes_truncated\":false")?
}

test test_err_typed_cause_one_argument_retains_existing_chain_through_try [error] { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
let translated: Result[Unit, OuterError] = Err(OuterError.Failed(message: "outer"), cause: InnerError.Failed(message: "inner"))
let captured: Result[Unit, OuterError] = try { ctx "attempt" { translated? } }
let preserved: Result[Unit, OuterError] = match captured { Err(failure) => Err(failure); _ => translated }
ctx "caller" { preserved? }
""")?
  test.eq(output.status, 3)?
  test.contains(output.stderr, "OuterError.Failed")?
  test.contains(output.stderr, "InnerError.Failed")?
  test.contains(output.stderr, "attempt")?
  test.contains(output.stderr, "caller")?
}

test test_err_typed_cause_abort_operand_keeps_control_transfer [error] { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
error InnerError = Failed(message: Str)
proc cause() -> InnerError { abort(17); InnerError.Failed(message: "unreachable") }
let translated: Result[Unit] = try { let data: Result[Unit, OuterError] = Err(OuterError.Failed(message: "outer"), cause: cause()) }
print "wrong after abort"
""")?
  test.ok(output.status == 17, output.stderr)?
  test.eq(output.status, 17)?
  test.eq(output.stdout, "")?
  test.eq(output.stderr, "")?
}

test test_err_typed_cause_procedure_frames_construct_result_data [error] { |ctx|
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
  test.eq(output.status, 3)?
  test.eq(output.stdout, "returned data\n")?
  test.contains(output.stderr, "OuterError.Failed")?
  test.contains(output.stderr, "InnerError.Failed")?
}

test test_err_typed_cause_long_native_chain_reports_truncation [error] { |ctx|
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
  test.eq(output.status, 3)?
  test.contains(output.stderr, "cause chain truncated")?
  test.ok(output.stderr.count_chars() < 10000, output.stderr)?
}

test test_err_typed_cause_retains_checked_assertion_failure [error] { |ctx|
  let output = test.run_script(ctx, r"""
error OuterError = Failed(message: Str)
let outcome: Result[Unit] = try { assert false, "checked leaf" }
let translated = match outcome {
  Err(failure) => Err(OuterError.Failed(message: "translation"), cause: failure)
  _ => Err(OuterError.Failed(message: "unexpected success"))
}
translated?
""")?
  test.eq(output.status, 3)?
  test.contains(output.stderr, "OuterError.Failed")?
  test.contains(output.stderr, "assertion-failed")?
  test.contains(output.stderr, "checked leaf")?
}

test test_err_one_argument_generic_propagation_keeps_existing_diagnostic [error] { |ctx|
  let output = test.run_script(ctx, r"""
let value: Result[Unit, Str] = Err("generic error data")
value?
""")?
  test.eq(output.status, 3)?
  test.contains(output.stderr, "error: error: propagated error")?
  test.ok(!output.stderr.contains("caused by"), output.stderr)?
}

test test_err_typed_cause_process_outer_survives_cleanup_transport [error] { |ctx|
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
  test.eq(output.status, 3)?
  test.eq(output.stdout, "body completed\n")?
  test.contains(output.stderr, "ProcessError.NonzeroExit")?
  test.contains(output.stderr, "OuterError.Failed")?
  test.contains(output.stderr, "[exit 7]")?
  test.contains(output.stderr, "cleanup cause")?
}
