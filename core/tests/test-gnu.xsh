pure probe_source() -> Str {
  """use lib.gnu as gnu

proc main(...argv: List[Str]) [env, fs, io, process, error] -> Result[Unit] {
  let cmd = argv[0]
  let arg = if argv.len() > 1 { argv[1] } else { "" }

  if cmd == "prog" {
    print gnu.prog()
  } else if cmd == "phrase" {
    print gnu.phrase()
  } else if cmd == "path" {
    print gnu.invoked_path().display()
  } else if cmd == "quote" {
    print gnu.quote(arg)
    print gnu.quote_maybe(arg)
  } else if cmd == "quote-value" {
    print gnu.quote_value(arg)
  } else if cmd == "quote-bytes" {
    print gnu.quote_bytes(io.stdin_bytes()?)
  } else if cmd == "error" {
    gnu.error(arg)
  } else if cmd == "strerror" {
    match fp"{arg}".read_text() {
      Err(failure) => print f"{gnu.errno(failure)}|{gnu.strerror(failure)}"
      Ok(_) => print "ok"
    }
  } else if cmd == "diagnostics" {
    match fp"{arg}".read_text() {
      Err(failure) => {
        gnu.cannot_access(arg, failure)
        gnu.cannot_open(arg, failure)
        gnu.cannot_open(arg, failure, mode: "writing")
        gnu.error_reading(arg, failure)
        gnu.name_error(arg, failure)
        gnu.cannot("remove", arg, failure)
      }
      Ok(_) => print "ok"
    }
  } else if cmd == "synthetic" {
    let failure = error.fail(arg)
    match failure {
      Err(inner) => print f"{gnu.errno(inner)}|{gnu.strerror(inner)}"
      Ok(_) => print "ok"
    }
  } else if cmd == "usage" {
    gnu.usage_error("bad thing", status: 2)
  } else if cmd == "usage-default" {
    gnu.usage_error("bad thing")
  } else if cmd == "missing" {
    gnu.missing_operand()
  } else if cmd == "missing-after" {
    gnu.missing_operand_after(arg, status: 125)
  } else if cmd == "extra" {
    gnu.extra_operand(arg, status: 2)
  } else if cmd == "version" {
    gnu.version(arg)
  } else if cmd == "version-text" {
    print gnu.version_text(arg)
  } else if cmd == "help" {
    gnu.help(arg)
  } else if cmd == "write" {
    gnu.write_text(arg)
    gnu.write_bytes(b"\\xff\\x00z")
  } else if cmd == "write-failed" {
    match error.fail(arg) {
      Err(failure) => gnu.write_failed(failure)
      Ok(_) => {}
    }
  } else if cmd == "read" {
    match gnu.read_operand(arg) {
      Ok(data) => io.write_stdout_bytes(data)?
      Err(failure) => gnu.name_error(arg, failure)
    }
  }
}
"""
}

type ProbeResult = {status: Int, stderr: Str, stdout: Str, stdout_bytes: Bytes}

# Runs the probe script as `invoked`, a symlink to the real script, so the
# invoked name differs from the file name the way an installed alias does.
# `real.xsh` runs the script itself.
proc probe(
  ctx: TestContext,
  args: List[Str],
  invoked = "gnuprobe",
  phrase = "",
  locale = "C",
  stdin = b"",
) [fs, process, error] -> Result[ProbeResult] {
  let root = test.temp_dir(ctx, name: "gnu")?.resolve()?
  let script = fp"{root}/real.xsh"
  script.write(probe_source())
  let entry = fp"{root}/{invoked}"
  if invoked != "real.xsh" {
    entry.symlink(to: p"real.xsh")
  }

  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), entry.display()].extend(args)
  let run_env = {
    XSH_MODULE_PATH: ctx.core_dir.display(),
    XSH_EXECUTION_PHRASE: phrase,
    LC_ALL: locale,
  }
  let plan = process.command_argv(ctx.xsh_bin, argv, root, run_env, stdin, out, err)
  let status = process.run(plan)?
  let bytes_out = out.read_bytes()?
  {status: status.exit_code()?, stderr: err.read_text()?, stdout: bytes_out.utf8() ?? "", stdout_bytes: bytes_out}
}

test test_gnu_invoked_name_follows_symlink_alias_not_the_real_script { |ctx|
  let real = probe(ctx, ["prog"], invoked: "real.xsh")?
  assert real.stdout == "real\n", "the .xsh suffix is dropped from the name"

  for alias in ["dir", "vdir", "[", "egrep"] {
    let result = probe(ctx, ["prog"], invoked: alias)?
    assert result.status == 0, result.stderr
    assert result.stdout == f"{alias}\n"
  }

  let located = probe(ctx, ["path"], invoked: "dir")?
  assert located.stdout.ends_with("/dir\n"), "the accessor is not symlink-resolved"
}

test test_gnu_phrase_prefers_execution_phrase_when_set_and_non_empty { |ctx|
  let plain = probe(ctx, ["phrase"], invoked: "ls")?
  assert plain.stdout == "ls\n"

  let dispatched = probe(ctx, ["phrase"], invoked: "ls", phrase: "/path/adapter ls")?
  assert dispatched.stdout == "/path/adapter ls\n"

  let hint = probe(ctx, ["usage-default"], invoked: "ls", phrase: "/path/adapter ls")?
  assert hint.stderr == """ls: bad thing
Try '/path/adapter ls --help' for more information.
"""
}

test test_gnu_error_prefixes_the_program_name_on_stderr { |ctx|
  let result = probe(ctx, ["error", "boom: it's here"], invoked: "tool")?
  assert result.status == 0
  assert result.stdout == ""
  assert result.stderr == "tool: boom: it's here\n"
}

test test_gnu_quote_matches_shell_escape_quoting { |ctx|
  let cases = [
    ["plain", "'plain'", "plain"],
    ["dir/file-1.txt", "'dir/file-1.txt'", "dir/file-1.txt"],
    ["", "''", "''"],
    ["a b", "'a b'", "'a b'"],
    ["it's", "\"it's\"", "\"it's\""],
    ["a'b\"c", "'a'\\''b\"c'", "'a'\\''b\"c'"],
    ["a\nb", "'a'$'\\n''b'", "'a'$'\\n''b'"],
    ["-a\tb'c", "'-a'$'\\t''b'\\''c'", "'-a'$'\\t''b'\\''c'"],
    ["\t \n \r \u{1}", "''$'\\t'' '$'\\n'' '$'\\r'' '$'\\001'", "''$'\\t'' '$'\\n'' '$'\\r'' '$'\\001'"],
    ["~x", "'~x'", "'~x'"],
    ["x~", "'x~'", "x~"],
    ["#x", "'#x'", "'#x'"],
    ["a=b", "'a=b'", "'a=b'"],
    ["a$b", "'a$b'", "'a$b'"],
    ["a\\b", "'a\\b'", "'a\\b'"],
    ["(x)", "'(x)'", "'(x)'"],
    ["é", "''$'\\303\\251'", "''$'\\303\\251'"],
  ]

  for entry in cases {
    let result = probe(ctx, ["quote", entry[0]])?
    assert result.stdout == f"{entry[1]}\n{entry[2]}\n", f"quote of {entry[0].byte_len()} bytes: {result.stdout}"
  }
}

test test_gnu_quote_prints_utf8_names_as_is_only_in_a_utf8_locale { |ctx|
  let utf8 = probe(ctx, ["quote", "file_é"], locale: "en_US.UTF-8")?
  assert utf8.stdout == "'file_é'\nfile_é\n"

  let ascii = probe(ctx, ["quote", "file_é"], locale: "C")?
  assert ascii.stdout == "'file_'$'\\303\\251'\n'file_'$'\\303\\251'\n"
}

test test_gnu_quote_bytes_escapes_names_that_are_not_text { |ctx|
  let invalid = probe(ctx, ["quote-bytes"], stdin: b"missing-\xff")?
  assert invalid.stdout == "'missing-'$'\\377'\n"

  let utf8_invalid = probe(ctx, ["quote-bytes"], stdin: b"a\xc3\xa9\xffz", locale: "en_US.UTF-8")?
  assert utf8_invalid.stdout == "'aé'$'\\377''z'\n"

  let utf8_control = probe(ctx, ["quote-bytes"], stdin: b"\xc2\x81", locale: "en_US.UTF-8")?
  assert utf8_control.stdout == "''$'\\302\\201'\n"

  let truncated = probe(ctx, ["quote-bytes"], stdin: b"x\xe2\x82", locale: "en_US.UTF-8")?
  assert truncated.stdout == "'x'$'\\342\\202'\n"

  let apostrophe = probe(ctx, ["quote-bytes"], stdin: b"\xff'")?
  assert apostrophe.stdout == "''$'\\377'\\'''\n", "invalid bytes force single quotes around an apostrophe"
}

test test_gnu_strerror_and_errno_come_from_the_host_error { |ctx|
  let root = test.temp_dir(ctx, name: "gnu-errors")?.resolve()?

  let missing = probe(ctx, ["strerror", f"{root}/absent"])?
  assert missing.stdout == "2|No such file or directory\n"

  let directory = probe(ctx, ["strerror", root.display()])?
  assert directory.stdout == "21|Is a directory\n"

  let other = probe(ctx, ["synthetic", "plain failure"])?
  assert other.stdout == "0|plain failure\n"

  let pipe = probe(ctx, ["synthetic", "write failed: Broken pipe (os error 32)"])?
  assert pipe.stdout == "32|Broken pipe\n"

  let nested = probe(ctx, ["synthetic", "a: b: Permission denied (os error 13)"])?
  assert nested.stdout == "13|Permission denied\n", "only the text after the last ': ' is the strerror"
}

test test_gnu_diagnostics_use_gnu_wording_and_quote_names { |ctx|
  let root = test.temp_dir(ctx, name: "gnu-diagnostics")?.resolve()?
  let missing = f"{root}/no such file"
  let result = probe(ctx, ["diagnostics", missing], invoked: "ls")?
  assert result.status == 0
  assert result.stderr == f"""ls: cannot access '{missing}': No such file or directory
ls: cannot open '{missing}' for reading: No such file or directory
ls: cannot open '{missing}' for writing: No such file or directory
ls: error reading '{missing}': No such file or directory
ls: '{missing}': No such file or directory
ls: cannot remove '{missing}': No such file or directory
"""

  let plain = f"{root}/absent"
  let unquoted = probe(ctx, ["diagnostics", plain], invoked: "cat")?
  assert unquoted.stderr.lines()[4] == f"cat: {plain}: No such file or directory", "quote_maybe leaves plain names bare"
}

test test_gnu_permission_denied_reads_as_strerror { |ctx|
  if applet.current_euid() == 0 {
    test.skip("root bypasses file permissions")
  }

  let root = test.temp_dir(ctx, name: "gnu-denied")?.resolve()?
  let secret = fp"{root}/secret"
  secret.write("x", mode: 0o000)
  let result = probe(ctx, ["strerror", secret.display()])?
  assert result.stdout == "13|Permission denied\n"
}

test test_gnu_usage_helpers_print_hint_and_exit_with_the_chosen_status { |ctx|
  let usage = probe(ctx, ["usage"], invoked: "cmp")?
  assert usage.status == 2
  assert usage.stdout == ""
  assert usage.stderr == """cmp: bad thing
Try 'cmp --help' for more information.
"""

  let default_status = probe(ctx, ["usage-default"], invoked: "tool")?
  assert default_status.status == 1

  let missing = probe(ctx, ["missing"], invoked: "rm")?
  assert missing.status == 1
  assert missing.stderr == """rm: missing operand
Try 'rm --help' for more information.
"""

  let after = probe(ctx, ["missing-after", "it's"], invoked: "env")?
  assert after.status == 125
  assert after.stderr == """env: missing operand after "it's"
Try 'env --help' for more information.
"""

  let extra = probe(ctx, ["extra", "x y"], invoked: "diff")?
  assert extra.status == 2
  assert extra.stderr == """diff: extra operand 'x y'
Try 'diff --help' for more information.
"""
}

test test_gnu_version_and_help_print_to_stdout { |ctx|
  let version = probe(ctx, ["version", "ls"])?
  assert version.status == 0
  assert version.stderr == ""
  assert version.stdout.starts_with("ls (XSH core) ")
  assert version.stdout.lines().len() == 1

  let text = probe(ctx, ["version-text", "dir"])?
  assert text.stdout == version.stdout.replace("ls", with: "dir")

  let help = probe(ctx, ["help", "Usage: ls [OPTION]... [FILE]..."])?
  assert help.stdout == "Usage: ls [OPTION]... [FILE]...\n"
  assert help.stderr == ""

  let ended = probe(ctx, ["help", "Usage: ls\n  -a  all\n"])?
  assert ended.stdout == "Usage: ls\n  -a  all\n", "a trailing newline is not doubled"
}

test test_gnu_write_helpers_pass_bytes_through_untouched { |ctx|
  let result = probe(ctx, ["write", "text\n"])?
  assert result.status == 0
  assert result.stdout_bytes == b"text\n\xff\0z"
}

test test_gnu_write_failure_ends_quietly_on_a_closed_pipe { |ctx|
  let pipe = probe(ctx, ["write-failed", "Broken pipe (os error 32)"], invoked: "yes")?
  assert pipe.status == 141
  assert pipe.stderr == "", "SIGPIPE is silent"

  let full = probe(ctx, ["write-failed", "/dev/full: No space left on device (os error 28)"], invoked: "cat")?
  assert full.status == 1
  assert full.stderr == "cat: write error: No space left on device\n"
}

test test_gnu_read_operand_reads_bytes_from_files_and_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "gnu-read")?.resolve()?
  let file = fp"{root}/data"
  file.write(b"\xffbinary\0")

  let from_file = probe(ctx, ["read", file.display()])?
  assert from_file.stdout_bytes == b"\xffbinary\0"

  let from_stdin = probe(ctx, ["read", "-"], stdin: b"\0in\xfe")?
  assert from_stdin.stdout_bytes == b"\0in\xfe"

  let absent = probe(ctx, ["read", f"{root}/absent"], invoked: "cat")?
  assert absent.stderr == f"cat: {root}/absent: No such file or directory\n"

  let directory = probe(ctx, ["read", root.display()], invoked: "cat")?
  assert directory.stderr == f"cat: {root}: Is a directory\n"
}

test test_gnu_quote_value_uses_locale_style_c_escapes { |ctx|
  let cases = [
    ["1", "'1'"],
    ["a\tb", "'a\\tb'"],
    ["\t", "'\\t'"],
    ["1\n", "'1\\n'"],
    ["a b", "'a b'"],
    ["it's", "'it\\'s'"],
    ["back\\slash", "'back\\\\slash'"],
    ["\u{1}", "'\\001'"],
    ["\u{1b}x", "'\\033x'"],
    ["²", "'\\302\\262'"],
  ]

  for entry in cases {
    let result = probe(ctx, ["quote-value", entry[0]], locale: "C")?
    assert result.stdout == f"{entry[1]}\n", f"quote_value of {entry[0].byte_len()} bytes: {result.stdout}"
  }

  let utf8 = probe(ctx, ["quote-value", "a²\t"], locale: "en_US.UTF-8")?
  assert utf8.stdout == "‘a²\\t’\n", "a UTF-8 locale prints curly quotes and printable text as is"
}
