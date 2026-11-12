test test_main_optional_path_binds_supplied_text_and_omitted_null { |ctx|
  let root = test.temp_dir(ctx, name: "main-optional-path")?
  let input = fp"{root}/optional path.txt"
  input.write("optional path contents")
  let declaration = r"""proc main(source: Path? = null) [fs, error] -> Result[Unit, Error] {
  if source != null {
    print source.read_text()?
  } else {
    print "no source"
  }
}
"""
  for invocation in ["", "main(@args)?\n"] {
    let source = declaration + invocation
    let supplied = test.expect(ctx, source, status: 0, args: [input])?
    assert supplied.stdout == "optional path contents\n"
    let omitted = test.expect(ctx, source, status: 0, args: [])?
    assert omitted.stdout == "no source\n"
  }
}

test test_main_optional_text_keeps_text_arguments { |ctx|
  let output = test.expect(ctx, r"""proc main(text: Str? = null) [io] {
  print (text ?? "no text")
}
main(@args)?
""", status: 0, args: ["ordinary text"])?
  assert output.stdout == "ordinary text\n"
}

test test_main_path_and_text_rest_keep_positional_binding { |ctx|
  let root = test.temp_dir(ctx, name: "main-path-and-rest")?
  let input = fp"{root}/input.txt"
  input.write("required path contents")
  let output = test.expect(ctx, r"""proc main(source: Path, ...rest: List[Str]) [fs, error] {
  print source.read_text()?
  print rest.join("|")
}
main(@args)?
""", status: 0, args: [input, "first text", "second text"])?
  assert output.stdout == "required path contents\nfirst text|second text\n"
}

test test_conditional_main_splice_converts_paths_and_preserves_invocations { |ctx|
  let declaration = r"""proc main(first: Path, second: Path) {
  print first.display() second.display()
}
"""
  let conditional = declaration + r"""if args.len() == 3 {
  print "control"
} else {
  main(@args)?
}
"""
  let called = test.expect(ctx, conditional, status: 0, args: ["one path", "two"])?
  assert called.stdout == "one path two\none path two\n"
  let nonfinal = test.expect(ctx, declaration + "main(@args)?\nprint \"after\"\n", status: 0, args: ["one", "two"])?
  assert nonfinal.stdout == "one two\nafter\none two\n"
  for arguments in [["one"], ["one", "two", "three", "four"]] {
    let rejected = test.expect(ctx, conditional, status: 3, args: arguments)?
    assert "arity" in rejected.stderr, rejected.stderr
    assert "compact.indexed-build" not in rejected.stderr, rejected.stderr
  }
}

test test_conditional_main_splice_keeps_optional_paths_and_defaults { |ctx|
  let source = r"""proc main(first: Path = p"default", second: Path? = null) {
  print first.display() (if second != null { second.display() } else { "none" })
}

if args.len() < 3 { main(@args)? }
"""
  for case in [
    {arguments: [], expected: "default none\ndefault none\n"},
    {arguments: ["supplied"], expected: "supplied none\nsupplied none\n"},
    {arguments: ["supplied", "optional path"], expected: "supplied optional path\nsupplied optional path\n"},
  ] {
    let output = test.expect(ctx, source, status: 0, args: case.arguments)?
    assert output.stdout == case.expected
  }
}

test test_quantified_main_splice_preserves_its_checked_invocation { |ctx|
  let output = test.expect(ctx, r"""proc main(first, ...rest: List[Str]) {
  print (first) rest.join(",")
}
main("fixed", @args)?
exit 0
""", status: 0, args: ["word"])?
  assert output.stdout == "fixed word\n"
}

test test_entry_splice_keeps_refined_optional_parameter_checks { |ctx|
  let source = r"""proc main(target: RelPath? = null) { print "ok" }
if args.len() < 2 { main(@args)? }
"""
  let omitted = test.expect(ctx, source, status: 0)?
  assert omitted.stdout == "ok\nok\n"
  for argument in ["../outside", "/outside"] {
    let rejected = test.expect(ctx, source, status: 3, args: [argument])?
    assert rejected.stdout == ""
    assert "type-error" in rejected.stderr, rejected.stderr
  }
}

test test_fixed_splice_exception_belongs_only_to_script_main { |ctx|
  let rejected = test.run_script(ctx, r"""proc ordinary(first: Str, second: Str) {
  print (first) (second)
}
if args.len() == 2 { ordinary(@args)? }
""")?
  assert !rejected.success
  assert "[check.splice-target]" in rejected.stderr, rejected.stderr
  let root = test.temp_dir(ctx, name: "imported-main-splice")?
  fp"{root}/entry_module.xsh".write(r"""##! A module main is an ordinary callable.
export proc main(first: Str, second: Str) { print (first) (second) }
""")
  let imported = test.run_script(ctx, r"""use entry_module
if args.len() == 2 { entry_module.main(@args)? }
""", env: {XSH_MODULE_PATH: root})?
  assert !imported.success
  assert "[check.splice-target]" in imported.stderr, imported.stderr
}
