##! Native ports of the uutils who integration tests.

use support.uu as uu

proc default_utmp() [process, error] -> Result[List[UnixUtmp], Error] {
  match unix.read_utmp() {
    Ok(records) => Ok(records)
    Err(failure) => {
      if failure.errno == 2 { Ok([]) } else { Err(failure) }
    }
  }
}

# The container has no recorded login, init, dead, run-level, or clock-change
# entries. Check that input condition independently before expecting no lines.
proc empty_selector(kind: Str) [process, error] -> Result[Str, Error] {
  let records = default_utmp()?
  assert [entry for entry in records if entry.kind == kind].is_empty(), f"unexpected default utmp records of type {kind}"
  Ok("")
}

# The kernel boot epoch supplies the missing default utmp boot record; it is
# independent of the applet's clock and uptime arithmetic.
proc boot_stdout() [fs, process, time, error] -> Result[Str, Error] {
  let records = default_utmp()?
  let boots = [entry for entry in records if entry.kind == "boot_time"]
  var seconds: List[Int] = [entry.time_sec for entry in boots]
  if seconds.is_empty() {
    if p"/var/run/utmp".exists()? {
      seconds = [fs.stat(p"/var/run/utmp")?.mtime_ns / 1000000000]
    } else {
      let lines = [line for line in p"/proc/stat".read_text()?.lines().collect() if line.starts_with("btime ")]
      assert lines.len() == 1, "expected one kernel boot epoch"
      seconds = [lines[0].split(" ").collect()[1].parse_int()?]
    }
  }
  var expected = ""
  for second in seconds {
    let stamp = time.format(second * 1000000000, "%b %e %H:%M", utc: true)?
    expected += f"         system boot  {stamp}\n"
  }
  Ok(expected)
}

# origin: uutils test_who::test_arg1_arg2
test test_uu_who_arg1_arg2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "who", ["am", "i"])?
  uu.succeeds(r)
  uu.stdout_is(r, "")
}

# origin: uutils test_who::test_boot
test test_uu_who_boot { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-b", "--boot", "--b"] {
    let expected = boot_stdout()?
    let r = uu.invoke(s, "who", [option])?
    uu.succeeds(r)
    uu.stdout_is(r, expected)
  }
}

# origin: uutils test_who::test_dead
test test_uu_who_dead { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-d", "--dead", "--de"] {
    let expected = empty_selector("dead_process")?
    let r = uu.invoke(s, "who", [option])?
    uu.succeeds(r)
    uu.stdout_is(r, expected)
  }
}

# origin: uutils test_who::test_invalid_arg
test test_uu_who_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "who", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_who::test_login
test test_uu_who_login { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-l", "--login", "--log"] {
    let expected = empty_selector("login_process")?
    let r = uu.invoke(s, "who", [option])?
    uu.succeeds(r)
    uu.stdout_is(r, expected)
  }
}

# origin: uutils test_who::test_m
test test_uu_who_m { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "who", ["-m"])?
  uu.succeeds(r)
  uu.stdout_is(r, "")
}

# origin: uutils test_who::test_process
test test_uu_who_process { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-p", "--process", "--p"] {
    let expected = empty_selector("init_process")?
    let r = uu.invoke(s, "who", [option])?
    uu.succeeds(r)
    uu.stdout_is(r, expected)
  }
}

# origin: uutils test_who::test_runlevel
test test_uu_who_runlevel { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-r", "--runlevel", "--r"] {
    let expected = empty_selector("run_level")?
    let r = uu.invoke(s, "who", [option])?
    uu.succeeds(r)
    uu.stdout_is(r, expected)
  }
}

# origin: uutils test_who::test_time
test test_uu_who_time { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-t", "--time", "--t"] {
    let expected = empty_selector("new_time")?
    let r = uu.invoke(s, "who", [option])?
    uu.succeeds(r)
    uu.stdout_is(r, expected)
  }
}
