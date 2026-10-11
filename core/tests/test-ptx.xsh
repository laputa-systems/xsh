type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/ptx.xsh by its real path inside `root`, capturing both streams.
proc ptx_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/ptx.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_ptx_default_output_aligns_the_keyword { |ctx|
  let root = test.temp_dir(ctx, name: "ptx")?
  let pad = tui.left_pad("", 35)

  assert ptx_run(ctx, root, [], b"a b")?.stdout == bytes.from_text(f"{pad}    a b\n{pad}a   b\n")
  assert ptx_run(ctx, root, ["-w", "10"], b"foo bar")?.stdout == b"     /   bar\n        foo/\n", "a narrow line marks what it dropped"
  assert ptx_run(ctx, root, [], b"012345678901234567890123456789\n")?.stdout == b"", "a token with no letter is not a keyword"
  assert ptx_run(ctx, root, ["-t"], b"bar\n")?.stdout == bytes.from_text(f"{tui.left_pad("", 53)}bar\n")
}

test test_ptx_roff_and_tex_formats { |ctx|
  let root = test.temp_dir(ctx, name: "ptx")?

  assert ptx_run(ctx, root, ["-G", "-A"], b"Rust is good language")?.stdout == b".xx \"\" \"\" \"Rust is good language\" \"\" \":1\"\n.xx \"\" \"Rust is\" \"good language\" \"\" \":1\"\n.xx \"\" \"Rust\" \"is good language\" \"\" \":1\"\n.xx \"\" \"Rust is good\" \"language\" \"\" \":1\"\n"
  assert ptx_run(ctx, root, ["-G", "-T"], b"a")?.stdout == b"\\xx {}{}{a}{}{}\n"
  assert ptx_run(ctx, root, ["-G", "--format=roff"], b"a")?.stdout == b".xx \"\" \"\" \"a\" \"\"\n"
  assert ptx_run(ctx, root, ["-G", "-O", "-M", "mac"], b"a")?.stdout == b".mac \"\" \"\" \"a\" \"\"\n"
  assert ptx_run(ctx, root, ["-G", "-f"], b"a _")?.stdout == b".xx \"\" \"\" \"a _\" \"\"\n.xx \"\" \"a\" \"_\" \"\"\n"
}

test test_ptx_word_selection_and_references { |ctx|
  let root = test.temp_dir(ctx, name: "ptx")?
  fp"{root}/in".write("alpha beta\ngamma beta\n")
  fp"{root}/only".write("beta\n")
  fp"{root}/ignore".write("beta\n")

  assert ptx_run(ctx, root, ["-G", "-o", "only", "in"])?.stdout == b".xx \"\" \"alpha\" \"beta\" \"\"\n.xx \"\" \"gamma\" \"beta\" \"\"\n"
  assert ptx_run(ctx, root, ["-G", "-i", "ignore", "in"])?.stdout.utf8()?.count_lines() == 2
  assert ptx_run(ctx, root, ["-G", "-A", "in"])?.stdout.utf8()?.find("in:1") != null, "auto references name the file and line"
  assert ptx_run(ctx, root, ["-G", "-r", "in"])?.stdout.utf8()?.find("\"alpha\"") != null, "-r treats the first field as a reference"
  assert ptx_run(ctx, root, ["-G", "-W", "[a-z]+", "-i", "ignore", "in"])?.stdout.utf8()?.count_lines() == 2
}

test test_ptx_errors { |ctx|
  let root = test.temp_dir(ctx, name: "ptx")?

  let missing = ptx_run(ctx, root, ["zxc"])?
  assert missing.status == 1
  assert missing.stderr == "ptx: zxc: No such file or directory\n", missing.stderr

  let width = ptx_run(ctx, root, ["-w", "0"])?
  assert width.status == 1
  assert width.stderr == "ptx: invalid line width: '0'\n", width.stderr

  let empty = ptx_run(ctx, root, ["-G", "-S", "^"])?
  assert empty.status == 0
  assert empty.stderr == "", empty.stderr

  let operands = ptx_run(ctx, root, ["-G", "-", "-", "-"])?
  assert operands.status == 1
  assert operands.stderr == "ptx: extra operand '-'\nTry 'ptx --help' for more information.\n", operands.stderr

  assert ptx_run(ctx, root, [], b"ab\xffcd\n")?.status == 0, "input that is not UTF-8 is still indexed"
}

test test_ptx_contexts_are_sentences_unless_traditional { |ctx|
  let root = test.temp_dir(ctx, name: "ptx")?
  let text = b"a b.  c d\ne f\n"
  let sentences = "                  a b.\n              a   b.\n                  c d e f\n              c   d e f\n            c d   e f\n          c d e   f\n"

  assert ptx_run(ctx, root, ["-w", "30"], text)?.stdout == bytes.from_text(sentences), "a sentence ends at a period before two spaces and may span lines"
  assert ptx_run(ctx, root, ["-G", "-w", "30"], b"a b\nc d\n")?.stdout.utf8()?.count_lines() == 4, "-G reads one context per line"
}

test test_ptx_gnu_regex_and_reference_boundaries { |ctx|
  let root = test.temp_dir(ctx, name: "ptx")?
  fp"{root}/breaks".write("b\n")

  let broken = ptx_run(ctx, root, ["-G", "-b", "breaks", "-W", ""], b"abc\n")?
  assert broken.stdout == b".xx \"\" \"\" \"abc\" \"\"\n.xx \"\" \"ab\" \"c\" \"\"\n"
  assert ptx_run(ctx, root, ["-W", "[ab]{0,}"], b"aa bb cc\n")?.stdout == b""
  assert ptx_run(ctx, root, ["-w", "1", "-A"], b"content")?.stdout == b":1:     content\n"
  let invalid = ptx_run(ctx, root, ["-S", "^["])?
  assert invalid.status == 1
  assert invalid.stderr == "ptx: Invalid regular expression (for regexp '^[')\n"
  assert ptx_run(ctx, root, ["-w", "10"], bytes.from_text("föö bar"))?.stdout == bytes.from_text("     /   bar\n        fö/\n")
}

test test_ptx_byte_layout_preserves_split_utf8 { |ctx|
  let root = test.temp_dir(ctx, name: "ptx")?
  let expected = b"    /\xa9 b\xc3\xa9   KEY cc\n             a\xc3\xa9 b\xc3\xa9/\n       a\xc3\xa9   b\xc3\xa9 KEY/\n     \xa9 KEY   cc     /b\xc3\n"
  assert ptx_run(ctx, root, ["-w", "20"], bytes.from_text("aé bé KEY cc\n"))?.stdout == expected
  let zero = ptx_run(ctx, root, ["-G", "-S", "^"], b"a\n")?
  assert zero.status == 1
  assert zero.stderr == "ptx: error: regular expression has a match of length zero: '^'\n"
}
