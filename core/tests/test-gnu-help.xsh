use support.uu as uu

# origin: gnu help/help-version-getopt.log
test test_gnu_help_help_version_getopt_log { |ctx|
  let s = uu.scene(ctx)?
  let utilities = ["cksum", "dd", "hostid", "hostname", "link", "logname", "nohup", "sleep", "tsort", "unlink", "uptime", "users", "whoami", "yes"]
  for util in utilities {
    for option in ["--help", "--version"] {
      let baseline = uu.invoke(s, util, [option], timeout: 5s)?
      uu.succeeds(baseline)
      let after = uu.invoke(s, util, [option, "AFTER"], timeout: 5s)?
      uu.succeeds(after)
      assert after.stdout == baseline.stdout, f"{util} {option} AFTER"
      let before = if util == "nohup" { "df" } else { "BEFORE" }
      # nohup executes its child through PATH; the reference must use that same
      # command rather than a script selected by the native applet launcher.
      let reference = if util == "nohup" {
        let out = uu.at(s, "df-out")
        let err = uu.at(s, "df-err")
        let result = process.run(process.command_argv(p"/bin/sh", [p"/bin/sh", p"-c", Path(r"""exec df "$@"; """), p"df-reference", Path(option)], s.root,
          {LC_ALL: "C"}, b"", out, err, timeout: 5s))?
        {status: result.exit_code()?, stdout: out.read_bytes()?}
      } else { {status: baseline.status, stdout: baseline.stdout} }
      for operands in [[before, option], [before, option, "AFTER"]] {
        let r = uu.invoke(s, util, operands, timeout: 5s)?
        assert r.status == reference.status
        assert r.stdout == reference.stdout, f"{util} {operands.join(" ")}"
      }
    }
  }
  let out = uu.at(s, "yes-out")
  let err = uu.at(s, "yes-err")
  let words = [p"/bin/sh", p"-c", Path(r""""$@" | head -n1; """), p"yes-pipe"]
    .extend(uu.argv(s, "yes", [p"--", p"--help"])?)
  let status = process.run(process.command_argv(p"/bin/sh", words, s.root, stdout: out, stderr: err, timeout: 5s))?
  assert status.exited_with(0)
  assert out.read_bytes()? == b"--help\n"
}
