use support.uu

# The shell owns descriptor setup; the applet argv still comes from the same
# launch helper used by the oracle, including its isolated environment.
proc descriptor_run(s: uu.Scene, args: List[Str], setup: Str) [fs, process, env, error] -> Result[uu.Ran, Error] {
  wrapper_run(s, args, setup, uu.argv(s, "cat", [Path(arg) for arg in args])?)
}

proc wrapper_run(s: uu.Scene, args: List[Str], setup: Str, launch: List[Path]) [fs, process, error] -> Result[uu.Ran, Error] {
  let out = uu.at(s, ".descriptor-out")
  let err = uu.at(s, ".descriptor-err")
  let words = [p"/bin/sh", p"-c", Path(setup), p"cat-descriptors"].extend(launch)
  let command = process.command_argv(p"/bin/sh", words, s.root, {LC_ALL: "C"}, b"", out, err, timeout: 10s)
  let status = process.run(command)?
  Ok({util: "cat", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: gnu cat/cat-E.log
test test_gnu_cat_cat_E_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "in", b"a\rb\r\nc\n\r\nd\r")?
  let single = uu.invoke(s, "cat", ["-E", "in"])?
  uu.succeeds(single)
  uu.stdout_only_bytes(single, b"a\rb^M$\nc$\n^M$\nd\r")

  uu.write_bytes(s, "in2", b"1\r")?
  uu.write_bytes(s, "in2b", b"\n2\r\n")?
  let split = uu.invoke(s, "cat", ["-E", "in2", "in2b"])?
  uu.succeeds(split)
  uu.stdout_only_bytes(split, b"1^M$\n2^M$\n")

  uu.write_bytes(s, "in2b", b"2\r\n")?
  let separate = uu.invoke(s, "cat", ["-E", "in2", "in2b"])?
  uu.succeeds(separate)
  uu.stdout_only_bytes(separate, b"1\r2^M$\n")
}

# origin: gnu cat/cat-proc.log
test test_gnu_cat_cat_proc_log { |ctx|
  if ! p"/proc/cpuinfo".is_file()? { test.skip("requires /proc/cpuinfo"); return }
  let s = uu.scene(ctx)?
  let marked = uu.invoke(s, "cat", ["-E", "/proc/cpuinfo"])?
  let plain = uu.invoke(s, "cat", ["/proc/cpuinfo"])?
  uu.succeeds(marked)
  uu.succeeds(plain)
  uu.no_stderr(marked)
  uu.no_stderr(plain)
  let digits = regex.compile("[0-9]+")?
  assert digits.replace(marked.stdout.utf8()?, with: "D").replace("$", with: "") == digits.replace(plain.stdout.utf8()?, with: "D").replace("$", with: "")
}

# origin: gnu cat/cat-self.log
test test_gnu_cat_cat_self_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "out", "x\n")?
  let append = uu.invoke(s, "cat", ["out"], stdout: uu.at(s, "out"), stdout_append: true)?
  uu.fails_with_code(append, 1)
  uu.stderr_is(append, "cat: out: input file is output file\n")
  uu.file_is(s, "out", "x\n")

  uu.write(s, "doc", "x\n")?
  uu.write(s, "doc.end", "y\n")?
  let truncate = uu.invoke(s, "cat", ["doc", "doc.end"], stdout: uu.at(s, "doc"))?
  uu.succeeds(truncate)
  uu.no_stderr(truncate)
  uu.file_is(s, "doc", "y\n")

  uu.write(s, "fx", "x\n")?
  uu.write(s, "fy", "y\n")?
  let joined = uu.invoke(s, "cat", ["fx", "fy"], stdout: uu.at(s, "fxy"))?
  uu.succeeds(joined)
  uu.no_stderr(joined)
  for name in ["fxy1", "fxy2", "fx3", "fx4", "fx5", "fx6"] {
    let copied = uu.invoke(s, "cat", ["fx"], stdout: uu.at(s, name))?
    uu.succeeds(copied)
    uu.no_stderr(copied)
  }

  let stdin_self = descriptor_run(s, ["-", "fy"], r"""exec "$@" <fxy1 1<>fxy1""")?
  uu.succeeds(stdin_self)
  uu.no_output(stdin_self)
  assert uu.read(s, "fxy1")? == uu.read(s, "fxy")?
  let named_self = descriptor_run(s, ["fxy2", "fy"], r"""exec "$@" 1<>fxy2""")?
  uu.succeeds(named_self)
  uu.no_output(named_self)
  assert uu.read(s, "fxy2")? == uu.read(s, "fxy")?

  for item in [
    {args: ["fx", "fx3"], setup: r"""exec "$@" 1<>fx3""", operand: "fx3"},
    {args: ["-", "fx4"], setup: r"""exec "$@" <fx 1<>fx4""", operand: "fx4"},
    {args: ["fx5"], setup: r"""exec "$@" >>fx5""", operand: "fx5"},
    {args: [], setup: r"""exec "$@" <fx6 >>fx6""", operand: "-"},
  ] {
    let conflict = descriptor_run(s, item.args, item.setup)?
    uu.fails_with_code(conflict, 1)
    uu.stderr_only(conflict, f"cat: {item.operand}: input file is output file\n")
  }

  uu.write(s, "file", "foo\n")?
  let exhausted = descriptor_run(s, [], r"""exec 3<file; "$@" <&3 >/dev/null || exit; exec 4>>file; exec "$@" <&3 >&4""")?
  uu.succeeds(exhausted)
  uu.no_output(exhausted)
}

# origin: gnu cat/cat-buf.log
test test_gnu_cat_cat_buf_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  var matched = false
  for delay in ["0.1", "0.2", "0.4", "0.8", "1.6", "3.2"] {
    let out = uu.at(s, ".buffer-out")
    let err = uu.at(s, ".buffer-err")
    # The FIFO reader exits after its first read. Two separately timed input
    # writes must therefore leave only the first line in the captured file.
    let setup = r"""delay=$1; shift; dd if=fifo count=1 >first & reader=$!; trap 'kill "$reader" 2>/dev/null; wait "$reader" 2>/dev/null' EXIT; { printf '1\n'; sleep "$delay"; printf '2\n'; } | "$@" -v >fifo; wait "$reader"; result=$?; trap - EXIT; exit "$result";"""
    let words = [p"/bin/sh", p"-c", Path(setup), p"cat-buffer", Path(delay)].extend(uu.argv(s, "cat", [])?)
    let command = process.command_argv(p"/bin/sh", words, s.root, {LC_ALL: "C"}, b"", out, err, timeout: 10s)
    assert process.run(command)?.shell_code()? == 0, err.read_text()?
    if uu.read(s, "first")? == b"1\n" { matched = true; break }
  }
  assert matched, "cat -v must flush the first line before the delayed second line"
}

# origin: gnu cat/splice.log
test test_gnu_cat_splice_log { |ctx|
  let s = uu.scene(ctx)?
  let cat = uu.argv(s, "cat", [p"/dev/zero"])?
  let timeout = uu.argv(s, "timeout", [p".1"].extend(cat))?
  let direct = wrapper_run(s, ["/dev/zero"], r"""exec "$@" >/dev/null""", timeout)?
  uu.fails_with_code(direct, 124)
  uu.no_output(direct)

  let fallback = descriptor_run(s, ["/dev/zero"], r"""strace -f -o /dev/null -e inject=io_uring_setup,io_uring_enter,io_uring_register,memfd_create,sendfile,splice,tee,vmsplice:error=ENOSYS "$@" | head -c 2 | tr '\000' y""")?
  uu.succeeds(fallback)
  uu.stdout_only(fallback, "yy")

  let no_pipe = wrapper_run(s, ["/dev/zero"], r"""exec strace -f -o /dev/null -e inject=pipe,pipe2:error=EMFILE "$@" >/dev/null""", timeout)?
  uu.fails_with_code(no_pipe, 124)
  uu.no_output(no_pipe)
}
