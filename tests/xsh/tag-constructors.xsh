test test_tag_constructor_payload_preserves_source_order_and_nominal_member [error] { |ctx|
  let executed = test.run_script(ctx, r"""enum Choice { Made(Str, Str), Other(Str, Str), Empty }
proc marker(label: Str) [io] -> Str { print $label; label }
proc made() [io] -> Choice { Made(marker("first"), marker("second")) }
let value = made()
match value { Made(first, second) => print $first $second; _ => print "wrong member" }
print (Empty == Empty)
""")?
  assert executed.success, executed.stderr
  executed.stdout == "first\nsecond\nfirst second\ntrue\n"
}

test test_tag_constructor_payload_requires_complete_declared_slots [error] { |ctx|
  for source in [
    "enum Choice { Made(Str, Str) }\nlet value = Made(\"first\")\n",
    "enum Choice { Made(Str, Str) }\nlet value = Made(\"first\", 2)\n",
    "enum First { Made(Str) }\nenum Second { Other(Str) }\nlet value: First = Other(\"payload\")\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert !rejected.success, rejected.stderr
    "check." in rejected.stderr
  }
}

test test_tag_constructor_declared_any_slots_preserve_independent_parameter_instances [error] { |ctx|
  let executed = test.run_script(ctx, r"""enum Boxed { Payload(Any) }
pure boxed(value) -> Boxed { Payload(value) }
match boxed(7) { Payload(value) => print $value }
match boxed("text") { Payload(value) => print $value }
""")?
  assert executed.success, executed.stderr
  executed.stdout == "7\ntext\n"
}
