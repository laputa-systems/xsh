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
