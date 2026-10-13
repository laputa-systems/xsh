use support.uu as uu

proc stat_text(s: uu.Scene, args: List[Str], vars: Record = {}) -> Result[Str] {
  let result = uu.invoke(s, "stat", args, vars: vars)?
  uu.succeeds(result)
  Ok(result.stdout.utf8()?)
}

proc hidden_proc(s: uu.Scene, words: List[Path]) -> Result[Status] {
  let adapter = [p"/usr/bin/unshare", p"-rm", p"/bin/sh", p"-c",
    Path(r"""mount -t tmpfs tmpfs /proc || exit; exec "$@"; """), p"without-proc"].extend(words)
  process.run(process.command_argv(adapter[0], adapter, s.root,
    stdout: uu.at(s, "namespace-output"), stderr: uu.at(s, "namespace-error"), timeout: 5s))
}

# origin: gnu stat/stat-birthtime.log
test test_gnu_stat_stat_birthtime_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "touch", ["a"])?)
  let birth = stat_text(s, ["--format", "%W", "a"])?
  let accessed = stat_text(s, ["--format", "%X", "a"])?
  let modified = stat_text(s, ["--format", "%Y", "a"])?
  let changed = stat_text(s, ["--format", "%Z", "a"])?
  var updated = false
  for milliseconds in [70, 140, 280, 560, 1120] {
    time.sleep(time.millis(milliseconds))
    uu.succeeds(uu.invoke(s, "touch", ["a"])?)
    let current_birth = stat_text(s, ["--format", "%W", "a"])?
    let current_access = stat_text(s, ["--format", "%X", "a"])?
    let current_modification = stat_text(s, ["--format", "%Y", "a"])?
    let current_change = stat_text(s, ["--format", "%Z", "a"])?
    updated = birth == current_birth and (accessed != current_access or accessed == "0\n") and modified != current_modification and changed != current_change
    if updated { break }
  }
  assert updated, "birth time must stay fixed while access, modification and change times advance"
}

# origin: gnu stat/stat-fmt.log
test test_gnu_stat_stat_fmt_log { |ctx|
  let s = uu.scene(ctx)?
  for width in range(1, 51) {
    let format = [" " for _ in range(width - 1)].join("") + "%"
    assert stat_text(s, ["--form=" + format, "."])? == format + "\n"
  }
  uu.touch(s, "'")
  for directive in ["-c%N", "-c%Qn"] {
    assert stat_text(s, [directive, "'"])? == "\"'\"\n"
    assert stat_text(s, [directive, "'"], {QUOTING_STYLE: "locale"})? == "'\\''\n"
  }
  let name = "a\n\nb\nc"
  uu.touch(s, name)
  let quoted = "'a'$'\\n\\n''b'$'\\n''c'"
  assert stat_text(s, ["-c%N", name])? == quoted + "\n"
  let default = stat_text(s, [name])?
  let file_lines = [line for line in default.split("\n") if "File: " in line]
  assert file_lines == ["  File: " + quoted]
  for args in [["--terse", name], ["-f", "--terse", name]] {
    assert stat_text(s, args)?.split(" ")[0] == quoted
  }
  for style in ["", "abcdef"] {
    let result = uu.invoke(s, "stat", ["-c%%%N", "'"], vars: {QUOTING_STYLE: style})?
    uu.succeeds(result)
    uu.stdout_is(result, "%\"'\"\n")
    uu.stderr_is(result, f"stat: ignoring invalid value of environment variable QUOTING_STYLE: '{style}'\n")
    for item in [{format: "%%N", output: "%N"}, {format: "abc%%Ndef", output: "abc%Ndef"}, {format: "abc%%Ndef%%N", output: "abc%Ndef%N"}, {format: "%%%%N", output: "%%N"}] {
      let literal = uu.invoke(s, "stat", ["-c" + item.format, "'"], vars: {QUOTING_STYLE: style})?
      uu.succeeds(literal)
      uu.stdout_only(literal, item.output + "\n")
    }
  }
  assert stat_text(s, ["-cabc%%Ndef%N", "'"])? == "abc%Ndef\"'\"\n"
  assert stat_text(s, ["-cabc%%Ndef%N", "'"], {QUOTING_STYLE: "locale"})? == "abc%Ndef'\\''\n"
  assert "?" not in stat_text(s, ["-c", "%r %R %Hd,%Ld %Hr,%Lr", "."])?
}

# origin: gnu stat/stat-hyphen.log
test test_gnu_stat_stat_hyphen_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")
  let filename = uu.invoke_from_path(s, "stat", ["--format=%n", "-"], uu.at(s, "f"))?
  uu.succeeds(filename)
  uu.stdout_is(filename, "-\n")
  uu.fails_with_code(uu.invoke_from_path(s, "stat", ["-f", "-"], uu.at(s, "f"))?, 1)
}

# origin: gnu stat/stat-mount.log
test test_gnu_stat_stat_mount_log { |ctx|
  let s = uu.scene(ctx)?
  assert stat_text(s, ["-c%m", "."])?.starts_with("/")
  let version = uu.argv(s, "stat", [p"--version"])?
  if hidden_proc(s, version)?.exited_with(0) {
    let mounts = hidden_proc(s, [p"/bin/mount"])?
    if mounts.exited_with(2) {
      let stat = uu.argv(s, "stat", [p"-c", p"0%#a", p"/"])?
      assert hidden_proc(s, stat)?.exited_with(0), uu.read_text(s, "namespace-error")?
    }
  }
}

# origin: gnu stat/stat-nanoseconds.log
test test_gnu_stat_stat_nanoseconds_log { |ctx|
  let s = uu.scene(ctx)?
  let vars = {TZ: "UTC0"}
  uu.succeeds(uu.invoke(s, "touch", ["-d", "1970-01-01 18:43:33.023456789", "k"], vars: vars)?)
  let listing = uu.invoke(s, "ls", ["--full-time"], vars: vars)?
  uu.succeeds(listing)
  if "18:43:33.023456789" not in listing.stdout.utf8()? { test.skip("filesystem does not retain subsecond timestamps") }
  for item in [
    {format: "%Y", output: "67413"},
    {format: "%.Y", output: "67413.023456789"},
    {format: "%.1Y", output: "67413.0"},
    {format: "%.3Y", output: "67413.023"},
    {format: "%.6Y", output: "67413.023456"},
    {format: "%.9Y", output: "67413.023456789"},
    {format: "%13.6Y", output: " 67413.023456"},
    {format: "%013.6Y", output: "067413.023456"},
    {format: "%-13.6Y", output: "67413.023456 "},
    {format: "%18.10Y", output: "  67413.0234567890"},
    {format: "%I18.10Y", output: "  67413.0234567890"},
    {format: "%018.10Y", output: "0067413.0234567890"},
    {format: "%-18.10Y", output: "67413.0234567890  "},
  ] { assert stat_text(s, ["-c", item.format, "k"], vars)? == item.output + "\n" }
  uu.succeeds(uu.invoke(s, "touch", ["-d", "@1777863721.987654321", "recent"], vars: vars)?)
  for item in [{format: "%Y", output: "1777863721"}, {format: "%.Y", output: "1777863721.987654321"}, {format: "%.3Y", output: "1777863721.987"}, {format: "%.9Y", output: "1777863721.987654321"}] {
    assert stat_text(s, ["-c", item.format, "recent"], vars)? == item.output + "\n"
  }
  uu.succeeds(uu.invoke(s, "touch", ["-d", "@-0.876543211", "old"], vars: vars)?)
  let seconds = stat_text(s, ["-c", "%Y", "old"], vars)?.trim().parse_int()?
  let tenth = stat_text(s, ["-c", "%.1Y", "old"], vars)?.trim()
  if seconds < 1 and tenth != "0.0" {
    for item in [{format: "%Y", output: "-1"}, {format: "%.Y", output: "-0.876543211"}, {format: "%.3Y", output: "-0.876"}, {format: "%.9Y", output: "-0.876543211"}] {
      assert stat_text(s, ["-c", item.format, "old"], vars)? == item.output + "\n"
    }
  }
}

# origin: gnu stat/stat-slash.log
test test_gnu_stat_stat_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")
  uu.mkdir(s, "dir")
  uu.symlink(s, "file", "link1")
  uu.symlink(s, "dir", "link2")
  assert stat_text(s, ["--format=%n", "link1"])? == "link1\n"
  let trailing_file = uu.invoke(s, "stat", ["--format=%n", "link1/"])?
  uu.fails_with_code(trailing_file, 1)
  uu.no_stdout(trailing_file)
  assert stat_text(s, ["--format=%F", "link2"])? == "symbolic link\n"
  assert stat_text(s, ["-L", "--format=%F", "link2"])? == "directory\n"
  assert stat_text(s, ["--format=%F", "link2/"])? == "directory\n"
}
