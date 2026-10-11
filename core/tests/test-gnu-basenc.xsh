use support.uu

pure letters(count: Int) -> Str {
  ["a" for _ in range(count)].join("")
}

pure encoded_letters(util: Str, count: Int) -> Str {
  let block_size = if util == "base64" { 3 } else { 5 }
  let word = if util == "base64" { "YWFh" } else { "MFQWCYLB" }
  let tails = if util == "base64" {
    ["", "YQ==", "YWE="]
  } else {
    ["", "ME======", "MFQQ====", "MFQWC===", "MFQWCYI="]
  }
  [word for _ in range(count / block_size)].join("") + tails[count % block_size]
}

pure lines_at(text: Str, width: Int) -> Str {
  if text == "" or width == 0 { return text }
  [text.byte_slice(index * width, length: width) for index in range((text.byte_len() + width - 1) / width)].join("\n") + "\n"
}

proc file_case(s: uu.Scene, util: Str, args: List[Str], input: Str) -> Result[uu.Ran] {
  uu.write(s, "input", input)?
  uu.invoke(s, util, args.extend(["input"]), vars: {LANGUAGE: "C", LANG: "C", LC_ALL: "C"})
}

proc decoded_case(s: uu.Scene, util: Str, input: Str, expected: Str) -> Result[Unit] {
  let result = file_case(s, util, ["--decode"], input)?
  uu.succeeds(result)
  uu.stdout_only(result, expected)
  Ok()
}

proc encoded_case(s: uu.Scene, util: Str, args: List[Str], input: Str, expected: Str) -> Result[Unit] {
  let result = file_case(s, util, args, input)?
  uu.succeeds(result)
  uu.stdout_only(result, expected)
  decoded_case(s, util, expected, input)?
  for position in range((if expected.byte_len() < 10 { expected.byte_len() } else { 10 }) + 1) {
    let with_newline = expected.byte_slice(0, length: position) + "\n" + expected.byte_slice(position)
    decoded_case(s, util, with_newline, input)?
  }
  Ok()
}

# origin: gnu basenc/base64.log
test test_gnu_basenc_base64_log { |ctx|
  let s = uu.scene(ctx)?
  for util in ["base32", "base64"] {
    encoded_case(s, util, [], "", "")?
    for count in range(1, 6) {
      encoded_case(s, util, [], letters(count), encoded_letters(util, count) + "\n")?
    }
    encoded_case(s, util, ["--wrap", "0"], "a", encoded_letters(util, 1))?
    encoded_case(s, util, ["--wrap", "08"], "a", encoded_letters(util, 1) + "\n")?
    for count in range(39, 47) {
      encoded_case(s, util, ["--wrap=5"], letters(count), lines_at(encoded_letters(util, count), 5))?
    }
    for width in ["0x0", "1k", "-1"] {
      let result = file_case(s, util, [f"-w{width}"], "")?
      uu.fails_with_code(result, 1)
      uu.stderr_only(result, f"{util}: invalid wrap size: '{width}'\n")
    }
    for count in [1, 2, 3, 4, 3072, 3073, 3074, 3075, 3071, 3070, 3069, 3068] {
      decoded_case(s, util, encoded_letters(util, count), letters(count))?
    }
    for count in range(1, 4) {
      let prefix = ["\n" for _ in range(count)].join("")
      decoded_case(s, util, prefix + encoded_letters(util, 3072), letters(3072))?
    }
    for args in [["a", "b"], ["-di", "--wrap=40", "a", "b"]] {
      let result = file_case(s, util, args, "")?
      uu.fails_with_code(result, 1)
      uu.stderr_only(result, f"{util}: extra operand 'b'\nTry '{util} --help' for more information.\n")
    }
  }
  for example in [
    {input: "aQ", output: "i"},
    {input: "Zzw", output: "g<"},
    {input: "MTIzNA==MTIzNA", output: "12341234"},
    {input: "MTIzNA==\nMTIzNA", output: "12341234"},
  ] {
    decoded_case(s, "base64", example.input, example.output)?
  }
  # A malformed final quantum can emit complete bytes before the error.
  for invalid in [
    {input: "a", output: ""},
    {input: "Zz=", output: "g"},
    {input: "Z===", output: ""},
    {input: "SB==", output: "H"},
    {input: "SGVsbG9=", output: "Hello"},
    {input: "MTIzNA=", output: "1234"},
    {input: "MTIzNA=\n", output: "1234"},
  ] {
    let result = file_case(s, "base64", ["--decode"], invalid.input)?
    uu.fails_with_code(result, 1)
    uu.stdout_is(result, invalid.output)
    uu.stderr_is(result, "base64: invalid input\n")
  }
}

# origin: gnu basenc/large-input.log
test test_gnu_basenc_large_input_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file.zeros")?
  uu.truncate(s, "file.zeros", 20 * 1024 * 1024)?
  let output = uu.at(s, "encoded")
  let result = uu.invoke(s, "basenc", ["--base58", "file.zeros"], stdout: output)?
  assert uu.size(s, "encoded")? == 21247462
  uu.succeeds(result)
}
