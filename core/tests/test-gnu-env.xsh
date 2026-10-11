use support.uu

# A nested applet must inherit the environment set by its parent env command.
# The first three words are the isolation launcher and its metadata; the target
# following them is selected by uu, including when the reference oracle runs.
proc nested(s: uu.Scene, util: Str, args: List[Str]) [process, env, error] -> Result[List[Str], Error] {
  let words = uu.argv(s, util, [Path(word) for word in args])?
  Ok([words[index].display() for index in range(3, words.len())])
}

# The script interpreter receives its entire option tail as one argument,
# followed by the script path and caller arguments, without kernel line limits.
proc shebang(s: uu.Scene, name: Str, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let header = uu.read_text(s, name)?.lines()[0]
  let split = header.find(" ") ?? 0
  let rest = header.byte_slice(split + 1)
  uu.invoke(s, "env", [rest, "./" + name] + args, timeout: 30s)
}

# origin: gnu env/env-S-script.log
test test_gnu_env_env_S_script_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "env_test", "#!env sh\necho hello\n")?
  uu.set_mode(s, "env_test", 0o755)?
  let simple = shebang(s, "env_test", [])?
  uu.succeeds(simple)
  uu.stdout_only(simple, "hello\n")

  let printf_words = nested(s, "printf", [])?
  let printf = ["'" + word + "'" for word in printf_words].join(" ")
  for case in [
    {name: "env1", options: "-S " + printf + " x%sx\\n A B", args: ["C", "D", "E F"], expected: "xAx\nxBx\nx./env1x\nxCx\nxDx\nxE Fx\n"},
    {name: "env2", options: "-S " + printf + " x%sx\\n \"A B\"", args: [], expected: "xA Bx\nx./env2x\n"},
    {name: "env3", options: "-S" + printf + "\\_x%sx\\n\\_Y", args: ["W"], expected: "xYx\nx./env3x\nxWx\n"},
    {name: "env4", options: "-S" + printf + " x%sx\\n A#B #C D", args: ["Z"], expected: "xA#Bx\nx./env4x\nxZx\n"},
  ] {
    uu.write(s, case.name, "#!env " + case.options + "\n")?
    uu.set_mode(s, case.name, 0o755)?
    let r = shebang(s, case.name, case.args)?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected)
  }
  let perl = process.which("perl")?
  uu.write(s, "env5", f"#!env -S {perl} -w -T\n" + "print \"hello\\n\";\n")?
  uu.set_mode(s, "env5", 0o755)?
  let strict_perl = shebang(s, "env5", [])?
  uu.succeeds(strict_perl)
  uu.stdout_only(strict_perl, "hello\n")
  uu.write(s, "env6", f"#!env -S {perl} -mFile::Basename=basename -e " + "\"print basename(\\$ARGV[0]);\"\n")?
  uu.set_mode(s, "env6", 0o755)?
  let basename = shebang(s, "env6", [])?
  uu.succeeds(basename)
  uu.stdout_only(basename, "env6")
}

# origin: gnu env/env-null.log
test test_gnu_env_env_null_log { |ctx|
  let s = uu.scene(ctx)?
  let search_path = env.get("PATH") ?? ""
  var outputs: List[Bytes] = []
  for util in ["env", "printenv"] {
    for flag in ["-0", "--null"] {
      let target = nested(s, util, [flag])?
      let r = uu.invoke(s, "env", ["-i", "PATH=" + search_path] + target)?
      uu.succeeds(r)
      uu.no_stderr(r)
      outputs += [r.stdout]
    }
  }
  for output in outputs { assert output == outputs[0] }
  let incompatible = uu.invoke(s, "env", ["-0", "echo", "hi"])?
  uu.fails_with_code(incompatible, 125)
  uu.no_stdout(incompatible)
  let one = uu.invoke(s, "env", ["-i", "-0", "a=b\nc="])?
  uu.succeeds(one)
  uu.stdout_only_bytes(one, b"a=b\nc=\0")
  let present = uu.invoke(s, "env", ["a=b\nc="] + nested(s, "printenv", ["-0", "a"])?)?
  uu.succeeds(present)
  uu.stdout_only_bytes(present, b"b\nc=\0")
  let absent = uu.invoke(s, "env", ["-u", "a"] + nested(s, "printenv", ["-0", "a"])?)?
  uu.fails_with_code(absent, 1)
  uu.no_stdout(absent)
  let mixed = uu.invoke(s, "env", ["-u", "b", "a=b\nc="] + nested(s, "printenv", ["-0", "b", "a"])?)?
  uu.fails_with_code(mixed, 1)
  uu.stdout_only_bytes(mixed, b"b\nc=\0")
}
