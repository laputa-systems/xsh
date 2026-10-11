use support.uu as uu

# origin: gnu date/date-next-dow.log
test test_gnu_date_date_next_dow_log { |ctx|
  let s = uu.scene(ctx)?
  let baseline = time.now() * 1000000
  let today = time.format(baseline, "%Y-%m-%d", utc: true)?
  let weekday = time.format(baseline, "%a", utc: true)?
  let next_week = time.format(baseline + 604800000000000, "%Y-%m-%d", utc: true)?
  let abbreviation = uu.invoke(s, "date", ["-d", weekday.lower(), "+%a"], vars: {TZ: "UTC0"})?
  let same_day = uu.invoke(s, "date", ["-d", weekday.lower(), "+%Y-%m-%d"], vars: {TZ: "UTC0"})?
  let following = uu.invoke(s, "date", ["-d", "next " + weekday.lower(), "+%Y-%m-%d"], vars: {TZ: "UTC0"})?
  if time.format(time.now() * 1000000, "%Y-%m-%d", utc: true)? != today { test.skip("clock crossed midnight during weekday checks") }
  uu.succeeds(abbreviation); uu.succeeds(same_day); uu.succeeds(following)
  uu.stdout_is(abbreviation, weekday + "\n")
  uu.stdout_is(same_day, today + "\n")
  uu.stdout_is(following, next_week + "\n")
}

# origin: gnu date/date-sec.log
test test_gnu_date_date_sec_log { |ctx|
  let s = uu.scene(ctx)?
  let second = uu.invoke(s, "date", ["+%S"])?
  uu.succeeds(second)
  match second.stdout.utf8()?.trim() {
    "58" => time.sleep(3s)?,
    "59" => time.sleep(2s)?,
    "00" => time.sleep(1s)?,
    _ => {},
  }
  let result = uu.invoke(s, "date", ["--date=21:04 +0100", "+%S"])?
  uu.succeeds(result)
  uu.stdout_is(result, "00\n")
}

# origin: gnu date/date-tz.log
test test_gnu_date_date_tz_log { |ctx|
  let s = uu.scene(ctx)?
  let zone = ["a" for _ in range(2000)].join("") + "0"
  let long_zone = uu.invoke(s, "date", ["-d", f"TZ=\"{zone}\" 2017"])?
  if long_zone.status != 0 { uu.stderr_contains(long_zone, "date: invalid date") }
  let epoch = uu.invoke(s, "date", ["-d", "1970-01-01 UTC 1780318971 seconds", "+%s"], vars: {TZ: "Europe/Berlin"})?
  uu.succeeds(epoch)
  uu.stdout_is(epoch, "1780318971\n")
  let database = uu.invoke(s, "date", ["+%z"], vars: {TZ: "America/Belize"})?
  if database.stdout == b"-0600\n" {
    let gap = uu.invoke(s, "date", ["-d", "2024-03-10 02:30", "+%T"], vars: {TZ: "America/New_York"})?
    uu.fails_with_code(gap, 1)
    uu.stderr_is(gap, "date: invalid date '2024-03-10 02:30'\n")
  }
}

# origin: gnu date/percent-percent.log
test test_gnu_date_percent_percent_log { |ctx|
  let s = uu.scene(ctx)?
  for locale in ["C", "fr_FR", "fr_FR.utf8"] {
    for conversions in ["HIklMNpPrRsSTXzZ", "aAbBcCdDeFgGhjmuUVwWxyY"] {
      let format = ["%%" + char for char in conversions].join("")
      let literal = ["%" + char for char in conversions].join("")
      let result = uu.invoke(s, "date", ["+" + format], vars: {LC_ALL: locale})?
      uu.succeeds(result)
      uu.stdout_is(result, literal + "\n")
    }
  }
}

# origin: gnu date/reference.log
test test_gnu_date_reference_log { |ctx|
  let s = uu.scene(ctx)?
  let vars = {TZ: "UTC0"}
  let earlier = "2025-10-23 03:00"
  let later = "2025-10-23 04:00"
  uu.succeeds(uu.invoke(s, "touch", ["-m", "-d", earlier, "a"], vars: vars)?)
  uu.succeeds(uu.invoke(s, "touch", ["-m", "-d", later, "b"], vars: vars)?)
  let a = uu.invoke(s, "date", ["+%s", "-r", "a"], vars: vars)?
  let b = uu.invoke(s, "date", ["+%s", "-r", "b"], vars: vars)?
  uu.succeeds(a); uu.succeeds(b)
  assert a.stdout.utf8()?.trim().parse_int()? < b.stdout.utf8()?.trim().parse_int()?
  let explicit = uu.invoke(s, "date", ["+%s", "-d", earlier], vars: vars)?
  let current = uu.invoke(s, "date", ["+%s"], vars: vars)?
  if explicit.stdout.utf8()?.trim().parse_int()? < current.stdout.utf8()?.trim().parse_int()? {
    assert a.stdout.utf8()?.trim().parse_int()? < current.stdout.utf8()?.trim().parse_int()?
  }
  uu.symlink(s, "t1", "t1s")?
  let target = uu.invoke(s, "date", ["-r", "t1"], vars: vars)?
  let symlink = uu.invoke(s, "date", ["-r", "t1s"], vars: vars)?
  assert target.stdout == symlink.stdout
  for args in [["--reference"], ["--reference="], ["--reference=missing"], ["-d", earlier, "-r/"]] {
    uu.fails_with_code(uu.invoke(s, "date", args, vars: vars)?, 1)
  }
}

# origin: gnu date/resolution.log
test test_gnu_date_resolution_log { |ctx|
  let s = uu.scene(ctx)?
  let resolution = uu.invoke(s, "date", ["--resolution"])?
  uu.succeeds(resolution)
  let fraction = resolution.stdout.utf8()?.trim().split(".")[1]
  var concise = fraction
  while concise.ends_with("0") { concise = concise.byte_slice(0, length: concise.byte_len() - 1) }
  let nanoseconds = uu.invoke(s, "date", ["+%-N"])?
  uu.succeeds(nanoseconds)
  assert nanoseconds.stdout.len() == concise.byte_len() + 1
}
