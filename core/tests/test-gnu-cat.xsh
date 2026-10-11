use support.uu

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
