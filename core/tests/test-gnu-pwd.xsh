use support.uu

# origin: gnu pwd/argument.log
test test_gnu_pwd_argument_log { |ctx|
  let s = uu.scene(ctx)?
  let base = uu.invoke(s, "pwd", [])?
  uu.mkdir(s, "a/b/c")?
  let nested = {ctx: s.ctx, root: uu.at(s, "a/b/c")}
  let r = uu.invoke(nested, "pwd", ["a"])?
  uu.succeeds(r)
  uu.stdout_is(r, base.stdout.utf8()?.trim() + "/a/b/c\n")
  uu.stderr_is(r, "pwd: ignoring non-option arguments\n")
}

# origin: gnu pwd/pwd-option.log
test test_gnu_pwd_pwd_option_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.symlink(s, "a/b", "c")?
  let physical = uu.invoke(s, "pwd", ["-P"])?
  let base = physical.stdout.utf8()?.trim()
  let nested = {ctx: s.ctx, root: uu.at(s, "c")}
  for row in [
    {args: ["-L"], vars: {PWD: f"{base}/c"}, out: f"{base}/c\n"},
    {args: ["--logical", "-P"], vars: {PWD: f"{base}/c"}, out: f"{base}/a/b\n"},
    {args: ["--physical"], vars: {PWD: f"{base}/c"}, out: f"{base}/a/b\n"},
    {args: [], vars: {PWD: f"{base}/c"}, out: f"{base}/a/b\n"},
    {args: [], vars: {PWD: f"{base}/c", POSIXLY_CORRECT: "1"}, out: f"{base}/c\n"},
    {args: ["-L"], vars: {PWD: f"{base}/c/."}, out: f"{base}/a/b\n"},
    {args: ["-L"], vars: {PWD: "bogus"}, out: f"{base}/a/b\n"},
    {args: ["-L"], vars: {PWD: f"{base}/a/../c"}, out: f"{base}/a/b\n"}
  ] {
    let r = uu.invoke(nested, "pwd", row.args, vars: row.vars)?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
}
