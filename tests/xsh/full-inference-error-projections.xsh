test test_full_inference_error_fields_keep_late_nominal_receivers { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""error Failure = Raised
pure consume(value: Error) -> Str { value.message }
pure fields_first(value) -> Str {
  let code = value.errno ?? 0
  let message = value.message
  let result = consume(value)
  f"{code}:{message}:{result}"
}
pure call_first(value) -> Str {
  let message = consume(value)
  let code = value.errno ?? 0
  f"{code}:{message}:{value.message}"
}
pure relay(value) { fields_first(value) }
assert fields_first(Failure.Raised("retained")) == "0:retained:retained"
assert call_first(Failure.Raised("retained")) == "0:retained:retained"
assert relay(Failure.Raised("retained")) == "0:retained:retained"
print "nominal error fields preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "nominal error fields preserved\n"
}

test test_full_inference_field_requirements_keep_record_and_error_domains { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""error Failure = Raised
pure code(value) { value.errno ?? 0 }
pure relay(value) { code(value) }
pure message(value) { value.message }
pure preserve(value) { let _ = value.message; value }
assert code(Failure.Raised("retained")) == 0
let present: Int? = 7
assert code({errno: present}) == 7
assert relay(Failure.Raised("retained")) == 0
let another: Int? = 11
assert relay({errno: another, message: "extra"}) == 11
let opaque: Any = 13
let selected: Any = message({message: opaque})
assert selected == 13
let retained: Failure = preserve(Failure.Raised("nominal"))
assert retained.message == "nominal"
print "field domains preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "field domains preserved\n"
}

test test_full_inference_error_field_requirements_reject_record_coercion { |ctx|
  test.expect(
    ctx,
    r"""error Failure = Raised
pure consume(value: Error) -> Str { value.message }
pure fields_first(value) -> Str {
  let code = value.errno ?? 0
  consume(value)
}
let invalid = fields_first({errno: null, message: "record"})
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_field_requirements_reject_unsupported_domains { |ctx|
  test.expect(
    ctx,
    r"""pure message(value) { value.message }
let invalid = message(7)
""",
    status: 2,
    stderr: ["[check.field-access]"],
  )?
  test.expect(
    ctx,
    r"""error Failure = Raised
pure removed(value) { value.kind }
let invalid = removed(Failure.Raised("retained"))
""",
    status: 2,
    stderr: ["[check.field-access]"],
  )?
}

test test_full_inference_error_kind_stays_removed_after_a_late_anchor { |ctx|
  test.expect(
    ctx,
    r"""pure removed(value: Error) -> Str { value.kind }
""",
    status: 2,
    stderr: ["[check.error-removed]"],
  )?
  test.expect(
    ctx,
    r"""pure consume(value: Error) -> Str { value.message }
pure removed(value) -> Str {
  let kind = value.kind
  let _ = consume(value)
  kind
}
""",
    status: 2,
    stderr: ["[check.field-access]"],
  )?
}

test test_full_inference_closed_record_arguments_preserve_empty_fields { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure identity(value) { value }
pure forward(value) { identity(value) }
pure consume(value: Record) -> Int { let _ = value; 9 }
pure guarded(value, flag) -> Int {
  if flag != null { consume(value.inner) } else { 0 }
}
let selected = identity({inner: {}})
let forwarded = forward({inner: {}, other: 7})
assert selected.inner == {}
assert forwarded.inner == {} and forwarded.other == 7
let present: Str? = "present"
assert guarded({inner: {}}, present) == 9
assert guarded({inner: {}}, null) == 0
print "closed record arguments preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "closed record arguments preserved\n"
}

test test_full_inference_closed_record_arguments_keep_unknown_domains_unresolved { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value) { value }
let invalid = identity({inner: []})
""",
    status: 2,
    stderr: ["[check.local-inference]"],
  )?
  test.expect(
    ctx,
    r"""pure identity(value) { value }
let invalid = identity({inner: map.empty()})
""",
    status: 2,
    stderr: ["[check.local-inference]"],
  )?
  test.expect(
    ctx,
    r"""pure field(value) { value.inner }
let opaque: Any = {inner: {}}
let invalid = field(opaque)
""",
    status: 2,
    stderr: ["[check.field-access]"],
  )?
}
