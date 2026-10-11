use support.uu

type CutCase = {name: Str, args: List[Bytes], input: Bytes?, output: Bytes, status: Int, stderr: Str}
proc case(name: Str, args: List[Str], input: Bytes?, output: Bytes = b"", status: Int = 0, stderr: Str = "") -> CutCase {
  {name: name, args: [bytes.from_text(arg) for arg in args], input: input, output: output, status: status, stderr: stderr}
}
proc byte_case(name: Str, args: List[Bytes], input: Bytes?, output: Bytes = b"", status: Int = 0, stderr: Str = "") -> CutCase {
  {name: name, args: args, input: input, output: output, status: status, stderr: stderr}
}
proc repeated(value: Bytes, count: Int) -> Bytes { bytes.concat([value for _ in range(count)]) }
proc diagnostic(message: Str) -> Str { f"cut: {message}\nTry 'cut --help' for more information.\n" }
proc run_case(s: uu.Scene, c: CutCase, transport: Str, locale: Str) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let args = [Path.parse_bytes(word)? for word in c.args]
  let actual_args = if transport == "file" { args.extend([p"input"]) } else { args }
  let vars = {LC_ALL: locale}
  let argv = uu.argv(s, "cut", actual_args, vars)?
  let out = uu.at(s, "out")
  let err = uu.at(s, "err")
  let plan = if transport == "redirect" {
    process.command_argv(argv[0], argv, s.root, vars, uu.at(s, "input"), out, err, timeout: 30s)
  } else {
    process.command_argv(argv[0], argv, s.root, vars, c.input ?? b"", out, err, timeout: 30s)
  }
  let status = process.run(plan)?
  Ok({util: "cut", args: [word.display() for word in actual_args], status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}
proc check_case(s: uu.Scene, c: CutCase, locale: Str) [fs, process, env, error] {
  if let input = c.input { uu.write_bytes(s, "input", input)? }
  let transports = if c.input == null { ["none"] } else { ["file", "redirect", "pipe"] }
  for transport in transports {
    let r = run_case(s, c, transport, locale)?
    assert r.status == c.status, f"{c.name}/{locale}/{transport}: expected status {c.status}, got {r.status}; {r.stderr.utf8()?}"
    assert r.stdout == c.output, f"{c.name}/{locale}/{transport}: output length {r.stdout.len()}, expected {c.output.len()}"
    assert r.stderr == bytes.from_text(c.stderr), f"{c.name}/{locale}/{transport}: {r.stderr.utf8()?}"
  }
}
# origin: gnu cut/cut.log
test test_gnu_cut_cut_log { |ctx|
  let s = uu.scene(ctx)?
  let position_zero = diagnostic("byte/character positions are numbered from 1")
  let field_zero = diagnostic("fields are numbered from 1")
  let no_endpoint = diagnostic("invalid range with no endpoint: -")
  let no_field = diagnostic("an input delimiter makes sense\n\tonly when operating on fields")
  # The input puts the final byte before the 256 KiB I/O buffer boundary.
  let boundary = repeated(b"a", 262143)
  let cases = [
    case("zero-1", ["-b0"], null, status: 1, stderr: position_zero),
    case("zero-2", ["-f0-2"], null, status: 1, stderr: field_zero),
    case("zero-3b", ["-b0-"], null, status: 1, stderr: position_zero),
    case("zero-3c", ["-c0-"], null, status: 1, stderr: position_zero),
    case("zero-3f", ["-f0-"], null, status: 1, stderr: field_zero),
    case("1", ["-d:", "-f1,3-"], b"a:b:c\n", b"a:c\n"),
    case("2", ["-d:", "-f1,3-"], b"a:b:c\n", b"a:c\n"),
    case("3", ["-d:", "-f2-"], b"a:b:c\n", b"b:c\n"),
    case("4", ["-d:", "-f4"], b"a:b:c\n", b"\n"),
    case("5", ["-d:", "-f4"], b"", b""),
    case("6", ["-c4"], b"123\n", b"\n"),
    case("7", ["-c4"], b"123", b"\n"),
    case("8", ["-c4"], b"123\n1", b"\n\n"),
    case("9", ["-c4"], b"", b""),
    case("byte-newline-1", ["-b1"], b"a\n", b"a\n"),
    case("a", ["-s", "-d:", "-f3-"], b"a:b:c\n", b"c\n"),
    case("b", ["-s", "-d:", "-f2,3"], b"a:b:c\n", b"b:c\n"),
    case("c", ["-s", "-d:", "-f1,3"], b"a:b:c\n", b"a:c\n"),
    case("d", ["-s", "-d:", "-f1,3"], b"a:b:c:\n", b"a:c\n"),
    case("e", ["-s", "-d:", "-f3-"], b"a:b:c:\n", b"c:\n"),
    case("f", ["-s", "-d:", "-f3-4"], b"a:b:c:\n", b"c:\n"),
    case("g", ["-s", "-d:", "-f3,4"], b"a:b:c:\n", b"c:\n"),
    case("h", ["-s", "-d:", "-f2,3"], b"abc\n", b""),
    case("i", ["-d:", "-f1-3"], b":::\n", b"::\n"),
    case("j", ["-d:", "-f1-4"], b":::\n", b":::\n"),
    case("k", ["-d:", "-f2-3"], b":::\n", b":\n"),
    case("l", ["-d:", "-f2-4"], b":::\n", b"::\n"),
    case("m", ["-s", "-d:", "-f1-3"], b":::\n", b"::\n"),
    case("n", ["-s", "-d:", "-f1-4"], b":::\n", b":::\n"),
    case("o", ["-s", "-d:", "-f2-3"], b":::\n", b":\n"),
    case("p", ["-s", "-d:", "-f2-4"], b":::\n", b"::\n"),
    case("q", ["-s", "-d:", "-f2-4"], b":::\n:\n", b"::\n\n"),
    case("r", ["-s", "-d:", "-f2-4"], b":::\n:1\n", b"::\n1\n"),
    case("s", ["-s", "-d:", "-f1-4"], b":::\n:a\n", b":::\n:a\n"),
    case("t", ["-s", "-d:", "-f3-"], b":::\n:1\n", b":\n\n"),
    case("u", ["-s", "-f3-"], b"", b""),
    case("v", ["-f3-"], b"", b""),
    case("w", ["-b", "1"], b"", b""),
    case("x", ["-s", "-d:", "-f2-4"], b":\n", b"\n"),
    case("y", ["-s", "-b4"], b":\n", status: 1, stderr: diagnostic("suppressing non-delimited lines makes sense\n\tonly when operating on fields")),
    case("z", [], b":\n", status: 1, stderr: diagnostic("you must specify a list of bytes, characters, or fields")),
    case("empty-fl", ["-f", ""], b":\n", status: 1, stderr: field_zero),
    case("missing-fl", ["-f", "--"], b":\n", status: 1, stderr: diagnostic("invalid field range")),
    case("empty-bl", ["-b", ""], b":\n", status: 1, stderr: position_zero),
    case("missing-bl", ["-b", "--"], b":\n", status: 1, stderr: diagnostic("invalid byte or character range")),
    case("multi-list-1", ["-f", "1", "-F", "2"], null, status: 1, stderr: diagnostic("only one list may be specified")),
    case("empty-f1", ["-f1"], b"", b""),
    case("empty-f2", ["-f2"], b"", b""),
    case("o-delim", ["-d:", "--out=_", "-f2,3"], b"a:b:c\n", b"b_c\n"),
    case("nul-idelim", ["-d", "", "--out=_", "-f2,3"], b"a\0b\0c\n", b"b_c\n"),
    case("nul-odelim", ["-d:", "--out=", "-f2,3"], b"a:b:c\n", b"b\0c\n"),
    case("multichar-od", ["-d:", "-O", "_._", "-f2,3"], b"a:b:c\n", b"b_._c\n"),
    case("delim-no-field1", ["-d", "", "-b1"], null, status: 1, stderr: no_field),
    case("delim-no-field2", ["-d:", "-b1"], null, status: 1, stderr: no_field),
    byte_case("8bit-delim", [b"-d", b"\xad", b"--out=_", b"-f2,3"], b"a\xadb\xadc\n", b"b_c\n"),
    case("w-delim-1", ["-w", "-f2,3"], b"a\tb  c\n", b"b\tc\n"),
    case("w-delim-2", ["-w", "-f1,2"], b"  a b\n", b"\ta\n"),
    case("w-delim-3", ["-s", "-w", "-f2"], b"abc\n", b""),
    case("w-delim-4", ["-s", "-w", "-f1"], b"a b c\n", b"a\n"),
    case("w-delim-5", ["-w", "-d:", "-f1"], null, status: 1, stderr: diagnostic("-d and -w are mutually exclusive")),
    case("w-delim-6", ["-w", "-f1,2"], b"a  \n", b"a\t\n"),
    case("w-delim-7", ["--whitespace-delimited", "-f1,2"], b"  a b\n", b"\ta\n"),
    case("F-delim-1", ["-F", "2,3"], b"a\tb  c\n", b"b c\n"),
    case("F-delim-2", ["-F", "2,3", "-O", "_"], b"a\tb  c\n", b"b_c\n"),
    case("F-delim-3", ["-F", "2,3", "-d", ","], b"1,2,3\n", b"2 3\n"),
    case("w-trim-1", ["--whitespace-delimited=trimmed", "-f1,2"], b"  a b  \n", b"a\tb\n"),
    case("w-trim-2", ["-s", "--whitespace-delimited=trimmed", "-f1"], b"  a  \n", b""),
    case("newline-1", ["-f1-"], b"a\nb", b"a\nb\n"),
    case("newline-2", ["-f1-"], b"", b""),
    case("newline-3", ["-d:", "-f1"], b"a:1\nb:2\n", b"a\nb\n"),
    case("newline-4", ["-d:", "-f1"], b"a:1\nb:2", b"a\nb\n"),
    case("newline-5", ["-d:", "-f2"], b"a:1\nb:2\n", b"1\n2\n"),
    case("newline-6", ["-d:", "-f2"], b"a:1\nb:2", b"1\n2\n"),
    case("newline-6a", ["-d:", "-f2"], b"a\nb", b"a\nb\n"),
    case("newline-7", ["-s", "-d:", "-f1"], b"a:1\nb:2", b"a\nb\n"),
    case("newline-8", ["-s", "-d:", "-f1"], b"a:1\nb:2\n", b"a\nb\n"),
    case("newline-9", ["-s", "-d:", "-f1"], b"a1\nb2", b""),
    case("newline-10", ["-s", "-d:", "-f1,2"], b"a:1\nb:2", b"a:1\nb:2\n"),
    case("newline-11", ["-s", "-d:", "-f1,2"], b"a:1\nb:2\n", b"a:1\nb:2\n"),
    case("newline-12", ["-s", "-d:", "-f1"], b"a:1\nb:", b"a\nb\n"),
    case("newline-13", ["-d:", "-f1-"], b"a1:\n:", b"a1:\n:\n"),
    case("newline-14", ["-d\n", "-f1"], b"a:1\nb:", b"a:1\n"),
    case("newline-15", ["-s", "-d\n", "-f1"], b"a:1\nb:", b"a:1\n"),
    case("newline-16", ["-s", "-d\n", "-f2"], b"\nb", b"b\n"),
    case("newline-17", ["-s", "-d\n", "-f1"], b"\nb", b"\n"),
    case("newline-18", ["-d\n", "-f2"], b"\nb", b"b\n"),
    case("newline-19", ["-d\n", "-f1"], b"\nb", b"\n"),
    case("newline-20", ["-s", "-d\n", "-f1-"], b"\n", b"\n"),
    case("newline-21", ["-s", "-d\n", "-f1-"], b"\nb", b"\nb\n"),
    case("newline-22", ["-d\n", "-f1-"], b"\nb", b"\nb\n"),
    case("newline-23", ["-d\n", "-f1-", "--ou=:"], b"a\nb\n", b"a:b\n"),
    case("newline-24", ["-d\n", "-f1,2", "--ou=:"], b"a\nb\n", b"a:b\n"),
    case("newline-26", ["-d\n", "-f2"], b"a\n", b"\n"),
    case("newline-27", ["-s", "-d\n", "-f2"], b"a\n", b""),
    case("newline-28", ["-s", "-d\n", "-f2"], bytes.concat([boundary, b"\n"]), b""),
    case("newline-29", ["-s", "-d\n", "-f2"], bytes.concat([boundary, b"\nb"]), b"b\n"),
    case("newline-25", ["-s", "-d\n", "-f1"], b"abc", b""),
    case("line-only-1", ["-d:", "-f1"], bytes.concat([b"a:", repeated(b"b", 262144), b"\n"]), b"a\n"),
    case("zerot-1", ["-z", "-c1"], b"ab\0cd\0", b"a\0c\0"),
    case("zerot-2", ["-z", "-c1"], b"ab\0cd", b"a\0c\0"),
    case("zerot-3", ["-z", "-f1-"], b"", b""),
    case("zerot-4", ["-z", "-d:", "-f1"], b"a:1\0b:2", b"a\0b\0"),
    case("zerot-5", ["-z", "-d:", "-f1-"], b"a1:\0:", b"a1:\0:\0"),
    case("zerot-6", ["-z", "-d", "", "-f1,2", "--ou=:"], b"a\0b\0", b"a:b\0"),
    case("zerot-7", ["-z", "-d", "", "-s", "-f1"], b"abc", b""),
    case("out-delim1", ["-c1-3,5-", "--output-d=:"], b"abcdefg\n", b"abc:efg\n"),
    case("out-delim2", ["-c1-3,2,5-", "--output-d=:"], b"abcdefg\n", b"abc:efg\n"),
    case("out-delim3", ["-c1-3,2-4,6", "--output-d=:"], b"abcdefg\n", b"abcd:f\n"),
    case("out-delim3a", ["-c1-3,2-4,6-", "--output-d=:"], b"abcdefg\n", b"abcd:fg\n"),
    case("out-delim4", ["-c4-,2-3", "--output-d=:"], b"abcdefg\n", b"bc:defg\n"),
    case("out-delim5", ["-c2-3,4-", "--output-d=:"], b"abcdefg\n", b"bc:defg\n"),
    case("out-delim6", ["-c2,1-3", "--output-d=:"], b"abc\n", b"abc\n"),
    case("od-abut", ["-b1-2,3-4", "--output-d=:"], b"abcd\n", b"ab:cd\n"),
    case("od-overlap", ["-b1-2,2", "--output-d=:"], b"abc\n", b"ab\n"),
    case("od-overlap2", ["-b1-2,2-", "--output-d=:"], b"abc\n", b"abc\n"),
    case("od-overlap3", ["-b1-3,2-", "--output-d=:"], b"abcd\n", b"abcd\n"),
    case("od-overlap4", ["-b1-3,2-3", "--output-d=:"], b"abcd\n", b"abc\n"),
    case("od-overlap5", ["-b1-3,1-4", "--output-d=:"], b"abcde\n", b"abcd\n"),
    case("inval1", ["-f", "2-0"], b"", status: 1, stderr: diagnostic("invalid decreasing range")),
    case("inval2", ["-f", "-"], b"", status: 1, stderr: no_endpoint),
    case("inval3", ["-f", "4,-"], b"", status: 1, stderr: no_endpoint),
    case("inval4", ["-f", "1-2,-"], b"", status: 1, stderr: no_endpoint),
    case("inval5", ["-f", "1-,-"], b"", status: 1, stderr: no_endpoint),
    case("inval6", ["-f", "-1,-"], b"", status: 1, stderr: no_endpoint),
    case("big-unbounded-b", ["--output-d=:", "-b1234567890-"], b"", b""),
    case("big-unbounded-b2a", ["--output-d=:", "-b1,9-"], b"123456789", b"1:9\n"),
    case("big-unbounded-b2b", ["--output-d=:", "-b1,1234567890-"], b"", b""),
    case("big-unbounded-c", ["--output-d=:", "-c1234567890-"], b"", b""),
    case("big-unbounded-f", ["--output-d=:", "-f1234567890-"], b"", b""),
    case("overlapping-unbounded-1", ["-b3-,2-"], b"1234\n", b"234\n"),
    case("overlapping-unbounded-2", ["-b2-,3-"], b"1234\n", b"234\n"),
    case("EOL-subsumed-1", ["--output-d=:", "-b2-,3,4-4,5"], b"123456\n", b"23456\n"),
    case("EOL-subsumed-2", ["--output-d=:", "-b3,4-4,5,2-"], b"123456\n", b"23456\n"),
    case("EOL-subsumed-3", ["--complement", "-b3,4-4,5,2-"], b"123456\n", b"1\n"),
    case("EOL-subsumed-4", ["--output-d=:", "-b1-2,2-3,3-"], b"1234\n", b"1234\n"),
  ]
  for locale in ["C", "fr_FR.utf-8"] {
    uu.write(s, "f", "x")?
    uu.write(s, "g", "y")?
    let multi = uu.invoke(s, "cut", ["-f2-", "f", "g"], vars: {LC_ALL: locale})?
    uu.succeeds(multi)
    uu.stdout_only(multi, "x\ny\n")
    for c in cases { check_case(s, c, locale) }
  }
  let multibyte = [
    case("mb-char-1", ["-c1"], b"\xc3\xa9x\n", b"\xc3\xa9\n"),
    case("mb-char-2", ["-c2"], b"\xc3\xa9x\n", b"x\n"),
    case("mb-char-3", ["-c1,3", "--output-d=:"], b"\xc3\xa9a\xe2\x82\xacb\n", b"\xc3\xa9:\xe2\x82\xac\n"),
    case("mb-char-4", ["-c1,3", "--output-d=␞"], b"\xc3\xa9ab\n", b"\xc3\xa9\xe2\x90\x9eb\n"),
    case("mb-char-5", ["-c1-2"], b"\xc3x\n", b"\xc3x\n"),
    case("mb-byte-n-1", ["-b1", "-n"], b"\xc3\xa9x\n", b"\n"),
    case("mb-byte-n-2", ["-b2", "-n"], b"\xc3\xa9x\n", b"\xc3\xa9\n"),
    case("mb-byte-n-3", ["-b1-2", "-n"], b"\xc3\xa9x\n", b"\xc3\xa9\n"),
    case("mb-byte-n-4", ["-b1,3", "-n"], b"\xc3\xa9x\n", b"x\n"),
    case("mb-byte-n-5", ["-b2-3", "-n"], b"\xc3\xa9x\n", b"\xc3\xa9x\n"),
    case("mb-byte-n-6", ["-b2", "-n"], b"\xe2\x82\xacx\n", b"\n"),
    case("mb-byte-n-7", ["-b3", "-n"], b"\xe2\x82\xacx\n", b"\xe2\x82\xac\n"),
    case("mb-byte-n-8", ["-b2-3", "-n"], b"\xe2\x82\xacx\n", b"\xe2\x82\xac\n"),
    case("mb-delim-1", ["-d", "é", "-f2"], b"a\xc3\xa9b\xc3\xa9c\n", b"b\n"),
    case("mb-delim-2", ["-d", "é", "-f1,3"], b"a\xc3\xa9b\xc3\xa9c\n", b"a\xc3\xa9c\n"),
    case("mb-delim-3", ["-s", "-d", "é", "-f2"], b"abc\n", b""),
    case("mb-delim-4", ["-s", "-d", "é", "-f1"], b"a\xc3\xa9b\n", b"a\n"),
    byte_case("mb-delim-5", [b"-d", b"\xa9", b"-f2"], b"A\xc3\xa9B\xa9C\n", b"C\n"),
    case("mb-delim-6", ["-d", "é", "-f1,3"], b"a\xc3\xa9b\xc3\xa9c", b"a\xc3\xa9c\n"),
    case("mb-delim-7", ["-d", "é", "-f2"], b"a\0b\xc3\xa9c\n", b"c\n"),
    byte_case("mb-delim-8", [b"-d", b"\xff", b"-f2"], b"a\xffb\n", b"b\n"),
    case("mb-delim-9", ["-d", "é", "-f2"], bytes.concat([boundary, b"\xc3\xa9b\n"]), b"b\n"),
    case("mb-delim-10", ["-s", "-d", "é", "-f2"], b"a\0b\0", b""),
    case("mb-delim-11", ["-f1", "-d", "😀"], b"a\xf0\x9f\x98\x80b\n", b"a\n"),
    case("mb-w-delim-1", ["-w", "-f2"], b"a\xe2\x80\x83b\n", b"b\n"),
    case("mb-w-delim-2", ["-sw", "-f2"], b"a\xc2\xa0b\n", b""),
    case("mb-w-nodelim-1", ["-w", "-f2"], b"abc", b"abc\n"),
    case("mb-compl-c1", ["--complement", "-c1"], b"\xc3\xa9x\n", b"x\n"),
    case("mb-compl-c2", ["--complement", "-c2"], b"\xc3\xa9x\n", b"\xc3\xa9\n"),
    case("mb-compl-f1", ["--complement", "-d", "é", "-f1"], b"a\xc3\xa9b\xc3\xa9c\n", b"b\xc3\xa9c\n"),
    case("mb-compl-bn1", ["--complement", "-b1", "-n"], b"\xc3\xa9x\n", b"\xc3\xa9x\n"),
    case("mb-zerot-c1", ["-z", "-c1"], b"\xc3\xa9x\0\xc3\xa9y\0", b"\xc3\xa9\0\xc3\xa9\0"),
    case("mb-zerot-f1", ["-z", "-d", "é", "-f2"], b"a\xc3\xa9b\0c\xc3\xa9d\0", b"b\0d\0"),
    case("mb-zerot-f2", ["-z", "-d", "é", "-f1"], b"a\xc3\xa9b\0c\xc3\xa9d", b"a\0c\0"),
    case("mb-empty-f1", ["-d", "é", "-f1"], b"\xc3\xa9\xc3\xa9c\n", b"\n"),
    case("mb-empty-f2", ["-d", "é", "-f2"], b"\xc3\xa9\xc3\xa9c\n", b"\n"),
    case("mb-empty-f3", ["-d", "é", "-f3"], b"\xc3\xa9\xc3\xa9c\n", b"c\n"),
    case("mb-empty-f1-3", ["-d", "é", "-f1-3", "--output-d=:"], b"\xc3\xa9\xc3\xa9c\n", b"::c\n"),
    case("mb-odelim-f1", ["-d", "é", "-f1,3", "--output-d=€"], b"a\xc3\xa9b\xc3\xa9c\n", b"a\xe2\x82\xacc\n"),
    case("mb-multiline-1", ["-d", "é", "-f2"], b"a\xc3\xa9b\nc\xc3\xa9d", b"b\nd\n"),
    case("mb-w-content-1", ["-w", "-f1,2"], b"\xc3\xa9\t\xc3\xbc\n", b"\xc3\xa9\t\xc3\xbc\n"),
    case("mb-w-exhausted-1", ["-w", "-f1"], b"a\xe2\x80\x83ignored\nb\xe2\x80\x83ignored", b"a\nb\n"),
    case("mb-w-exhausted-2", ["-w", "-f1"], b"a  \nb  ", b"a\nb\n"),
    case("mb-w-exhausted-initial", ["-s", "-w", "--complement", "-f1-"], b"a\xe2\x80\x83ignored\nplain", b"\n"),
    byte_case("mb-delim-exhausted", [b"-d", b"\xa9", b"-f1"], b"A\xc3\xa9B\xa9ignored\nC\xa9ignored", b"A\xc3\xa9B\nC\n"),
    case("mb-bn-odelim", ["-b1,3", "-n", "--output-d=:"], b"\xc3\xa9x\n", b"x\n"),
    case("mb-bn-odelim-2", ["-b1-2,4", "-n", "--output-d=:"], b"\xc3\xa9\xc3\xbcx\n", b"\xc3\xa9:\xc3\xbc\n"),
    case("mb-compl-bn2", ["--complement", "-b3", "-n"], b"\xc3\xa9x\n", b"\xc3\xa9\n"),
    case("mb-F-1", ["-F", "2"], b"\xc3\xa9\t\xc3\xbc\n", b"\xc3\xbc\n"),
    case("mb-mismatch", ["-f1", "-d", "😀", "-d", "，"], b"a\xef\xbc\x8cb\n", b"a\n"),
  ]
  for c in multibyte { check_case(s, c, "fr_FR.utf-8") }
  let single_byte = [
    case("mb-delim-C", ["-d", "é", "-f1"], null, status: 1, stderr: diagnostic("the delimiter must be a single character")),
    case("c-locale-byte", ["-c2"], b"a\xc3\xa9b\n", b"\xc3\n"),
    case("c-locale-nosplit", ["-b2", "-n"], b"a\xc3\xa9b\n", b"\xc3\n"),
  ]
  for c in single_byte { check_case(s, c, "C") }
}
