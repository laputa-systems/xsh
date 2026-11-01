test test_inferred_record_parameter_preserves_a_pattern_bound_argument { |ctx|
  let output = test.run_xsh(ctx, r"""type Options = {srcaddr: Str?, numeric: Bool}
proc lookup(host: Str, opts) -> Str {
  if opts.numeric { host } else { "off" }
}
proc selected(opts: Options) -> Str {
  if let given = opts.srcaddr { return lookup(given, opts) }
  "absent"
}
print $selected({srcaddr: "127.0.0.1", numeric: true})
print $selected({srcaddr: "127.0.0.1", numeric: false})
print $selected({srcaddr: null, numeric: true})
""")?
  assert output.success, output.stderr
  assert output.stdout == "127.0.0.1\noff\nabsent\n"
}

test test_inferred_address_preserves_record_union_arguments_in_both_branches { |ctx|
  let output = test.run_xsh(ctx, r"""type Options = {timeout: Int}
proc connect_bounded(fd: Int, target, opts: Options) [net, process, error] {
  if opts.timeout < 0 { linux.connect(fd, target) } else { linux.connect(fd, target) }
}
for timeout in [-1, 0] {
  let outcome = connect_bounded(-1, {family: "inet", address: "127.0.0.1", port: 80}, {timeout: timeout})
  assert match outcome { Err(_) => true; Ok(_) => false }
  print "rejected"
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "rejected\nrejected\n"
}

test test_inferred_record_parameter_keeps_an_unguarded_optional_argument_rejected { |ctx|
  let output = test.run_xsh(ctx, r"""type Options = {srcaddr: Str?, numeric: Bool}
proc lookup(host: Str, opts) -> Bool { opts.numeric }
proc selected(opts: Options) { let _ = lookup(opts.srcaddr, opts) }
""")?
  assert ! output.success, output.stdout
  assert "[check.type-mismatch]" in output.stderr, output.stderr
}

test test_guarded_record_spread_preserves_the_source_field_view { |ctx|
  let output = test.run_xsh(ctx, r"""type Options = {srcaddr: Str?, numeric: Bool}
proc lookup(host: Str, opts) -> Str { if opts.numeric { host } else { "off" } }
proc selected(opts: Options) -> Str {
  if let given = opts.srcaddr { return lookup(given, {...opts, numeric: false}) }
  "absent"
}
print $selected({srcaddr: "127.0.0.1", numeric: true})
print $selected({srcaddr: null, numeric: true})
""")?
  assert output.success, output.stderr
  assert output.stdout == "off\nabsent\n"
}
