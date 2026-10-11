use support.uu

# Nested commands use the same launcher as the outer nice invocation so both
# levels observe the inherited priority under the native and oracle runs.
proc nested(s: uu.Scene, options: List[Str], util: Str = "nice", stderr: Path? = null) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let child = uu.argv(s, util, [])?
  if stderr == null {
    return uu.invoke_paths(s, "nice", [Path(word) for word in options].extend(child))
  }
  uu.invoke(s, "nice", options.extend([word.display() for word in child]), stderr: stderr)
}

# origin: gnu nice/nice-fail.log
test test_gnu_nice_nice_fail_log { |ctx|
  let s = uu.scene(ctx)?
  for options in [["-n", "1"], ["---"], ["-n", "1a"], ["-n", "1+2-3", "nice"]] {
    uu.fails_with_code(uu.invoke(s, "nice", options)?, 125)
  }
  uu.fails_with_code(uu.invoke(s, "nice", ["sh", "-c", "exit 2"])?, 2)
  uu.fails_with_code(uu.invoke(s, "env", ["."])?, 126)
  uu.fails_with_code(uu.invoke(s, "nice", ["."])?, 126)
  uu.fails_with_code(uu.invoke(s, "nice", ["no_such"])?, 127)
}

# origin: gnu nice/nice.log
test test_gnu_nice_nice_log { |ctx|
  let s = uu.scene(ctx)?
  let baseline = uu.invoke(s, "nice", [])?
  uu.succeeds(baseline)
  uu.stdout_only(baseline, "0\n")
  for case in [
    {options: [], expected: 10},
    {options: ["-1"], expected: 1},
    {options: ["-12"], expected: 12},
    {options: ["-1", "-2"], expected: 2},
    {options: ["-n", "1"], expected: 1},
    {options: ["-n", "1", "-2"], expected: 2},
    {options: ["-n", "1", "-+12"], expected: 12},
    {options: ["-2", "-n", "1"], expected: 1},
    {options: ["-2", "-n", "12"], expected: 12},
    {options: ["-+1"], expected: 1},
    {options: ["-+12"], expected: 12},
    {options: ["-+1", "-+12"], expected: 12},
    {options: ["-n", "+1"], expected: 1},
    {options: ["--1", "-2"], expected: 2},
    {options: ["--1", "-2", "-13"], expected: 13},
    {options: ["--1", "-n", "2"], expected: 2},
    {options: ["--1", "-n", "2", "-3"], expected: 3},
    {options: ["--1", "-n", "2", "-13"], expected: 13},
    {options: ["-n", "-1", "-12"], expected: 12},
    {options: ["--1", "-12"], expected: 12},
  ] {
    let r = nested(s, case.options)?
    uu.succeeds(r)
    uu.stdout_is(r, f"{case.expected}\n")
  }
  let lowering = nested(s, ["-n", "-1"])?
  uu.succeeds(lowering)
  if lowering.stdout == b"0\n" {
    let advisory = nested(s, ["-n", "-1"], "true")?
    uu.succeeds(advisory)
    assert advisory.stderr.len() > 0
    for options in [["--1"], ["--adj", "-1"]] {
      let r = nested(s, options, "true")?
      uu.succeeds(r)
      assert r.stderr == advisory.stderr
    }
    let unwritable = nested(s, ["-n", "-1"], stderr: p"/dev/full")?
    uu.fails_with_code(unwritable, 125)
    uu.no_stdout(unwritable)
  } else {
    let permitted = nested(s, ["-n", "-1"])?
    uu.succeeds(permitted)
    uu.stdout_only(permitted, "-1\n")
    for options in [["-n", "-1"], ["--1"]] {
      let r = nested(s, options)?
      uu.succeeds(r)
      uu.stdout_is(r, "-1\n")
    }
  }
  let clamped = nested(s, ["-n", "18446744073709551616"])?
  uu.succeeds(clamped)
}
