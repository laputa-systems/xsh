use support.uu

type InvalidOption = {args: List[Str], diagnostic: Str, range_error: Bool}

# ERANGE has different C-locale strerror text in musl and glibc. Only that
# platform-provided suffix varies; the option context and complete message stay exact.
proc check_invalid(r: uu.Ran, c: InvalidOption) {
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  if c.range_error {
    let musl = bytes.from_text(c.diagnostic + ": Result not representable\n")
    let glibc = bytes.from_text(c.diagnostic + ": Numerical result out of range\n")
    assert r.stderr == musl or r.stderr == glibc, r.stderr.utf8()?
  } else { uu.stderr_is(r, c.diagnostic) }
}
# origin: gnu pr/options.log
test test_gnu_pr_options_log { |ctx|
  let s = uu.scene(ctx)?
  let cases: List[InvalidOption] = [
    {args: ["+0"], diagnostic: "pr: +0: No such file or directory\n", range_error: false},
    {args: ["+0foo"], diagnostic: "pr: +0foo: No such file or directory\n", range_error: false},
    {args: ["--pages=-0"], diagnostic: "pr: invalid --pages argument '-0'\n", range_error: false},
    {args: ["--pages=0"], diagnostic: "pr: invalid page range '0'\n", range_error: false},
    {args: ["-l0"], diagnostic: "pr: '-l PAGE_LENGTH' invalid number of lines: '0'", range_error: true},
    {args: ["-w0"], diagnostic: "pr: '-w PAGE_WIDTH' invalid number of characters: '0'", range_error: true},
    {args: ["-W0"], diagnostic: "pr: '-W PAGE_WIDTH' invalid number of characters: '0'", range_error: true},
    {args: ["-e=-1"], diagnostic: "pr: '-e' extra characters or invalid number in the argument: '-1'\nTry 'pr --help' for more information.\n", range_error: false},
  ]
  for c in cases { check_invalid(uu.invoke_from_path(s, "pr", c.args, p"/dev/null")?, c) }
  let tabs = bytes.concat([b"\t" for _ in range(1048576)])
  let overflow = uu.invoke(s, "pr", ["-t", "-e2048"], tabs, stdout: p"/dev/null", timeout: 60s)?
  uu.fails_with_code(overflow, 1)
  uu.stderr_is(overflow, "pr: integer overflow\n")
}
