use support.uu

proc verify_total(output: Str) [error] {
  let row = regex.compile("^(.*?) +(-?[0-9]+|-) +(-?[0-9]+|-) +(-?[0-9]+|-) +(?:-|[0-9]+%) (.*)$")?
  let lines = output.trim().split("\n")
  var size = 0
  var used = 0
  var available = 0
  var saw_total = false
  for index in range(1, lines.len()) {
    assert ! saw_total, "extra rows after totals"
    let fields = row.captures(lines[index])
    assert fields.len() == 6, f"invalid df row: {lines[index]}"
    if fields[1] == "total" and fields[5] == "-" {
      assert size == fields[2].parse_int()?
      assert used == fields[3].parse_int()?
      assert available == fields[4].parse_int()?
      saw_total = true
    } else {
      if fields[2] != "-" { size += fields[2].parse_int()? }
      if fields[3] != "-" { used += fields[3].parse_int()? }
      if fields[4] != "-" { available += fields[4].parse_int()? }
    }
  }
  assert saw_total, "missing totals row"
}

# origin: gnu df/df-P.log
test test_gnu_df_df_P_log { |ctx|
  let s = uu.scene(ctx)?
  let plain = uu.invoke(s, "df", ["-P", "."])?
  let configured = uu.invoke(s, "df", ["-P", "."], vars: {BLOCK_SIZE: "1M"})?
  uu.succeeds(plain)
  uu.succeeds(configured)
  let spaces = regex.compile(" +")?
  assert spaces.replace(plain.stdout.utf8()?.split("\n")[0], with: " ") == spaces.replace(configured.stdout.utf8()?.split("\n")[0], with: " ")
}

# origin: gnu df/header.log
test test_gnu_df_header_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "df", ["."])?
  assert "\n" in r.stdout.utf8()?.trim(), "header and filesystem row must occupy separate lines"
}

# origin: gnu df/total-unprocessed.log
test test_gnu_df_total_unprocessed_log { |ctx|
  let s = uu.scene(ctx)?
  let filesystem = uu.invoke(s, "df", ["--output=fstype", "."])?
  if filesystem.stdout.utf8()?.trim().split("\n")[-1] != "-" {
    let filtered = uu.invoke(s, "df", ["-t", "_non_existent_fstype_", "--total", "."])?
    uu.fails(filtered)
    uu.stderr_is(filtered, "df: no file systems processed\n")
  }
  let missing = uu.invoke(s, "df", ["--total", "_does_not_exist_"])?
  uu.fails(missing)
  uu.stderr_is(missing, "df: _does_not_exist_: No such file or directory\n")
}

# origin: gnu df/total-verify.log
test test_gnu_df_total_verify_log { |ctx|
  let s = uu.scene(ctx)?
  let probe = uu.invoke(s, "df", [], timeout: 10s)?
  if probe.status != 0 { test.skip("df cannot enumerate the mounted filesystems"); return }
  for args in [["--total", "-P", "--block-size=512"], ["--total", "-i", "-P"]] {
    let r = uu.invoke(s, "df", args)?
    uu.succeeds(r)
    verify_total(r.stdout.utf8()?)
  }
}

# origin: gnu df/unreadable.log
test test_gnu_df_unreadable_log { |ctx|
  if unix.id()?.euid == 0 { test.skip("requires an unprivileged owner"); return }
  let s = uu.scene(ctx)?
  uu.touch(s, "unreadable")?
  uu.set_mode(s, "unreadable", 0o200)?
  uu.succeeds(uu.invoke(s, "df", ["unreadable"])?)
  uu.mkfifo(s, "fifo")?
  uu.succeeds(uu.invoke(s, "df", ["fifo"], timeout: 10s)?)
}
