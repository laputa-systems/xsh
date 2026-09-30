test generic_statement_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure bad(value) { value; 1 }
print ${bad(false)}
""")?
  output.status == 2
  output.stdout == ""
  assert "discard" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test inferred_error_only_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""error LocalError = Bad(message: Str)
pure failed() { Err(LocalError.Bad("unknown success")) }
let _ = failed()
""")?
  output.status == 2
  output.stdout == ""
  assert "annotation" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test inferred_missing_completion_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure missing(flag) { if flag { return 1 } }
print ${missing(false)}
""")?
  output.status == 2
  output.stdout == ""
  assert "return" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_effect_bound_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""proc bad(value) [] { let _ = time.now(); value }
let _ = bad(1)
""")?
  output.status == 2
  output.stdout == ""
  assert "check.effect-violation" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_proc_kind_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""proc silent(value) { value }
pure bad(value) { silent(value) }
print ${bad(1)}
""")?
  output.status == 2
  output.stdout == ""
  assert "pure" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test inferred_result_payload_versus_boundary_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure ambiguous(value, flag: Bool) { if flag { value } else { Ok(value) } }
let _ = ambiguous(1, true)
""")?
  output.status == 2
  output.stdout == ""
  assert "annotation" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}
