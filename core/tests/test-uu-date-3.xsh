##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_date.rs.

use support.uu as uu

# origin: uutils test_date::test_non_utf8_operands_are_octal_escaped
test test_uu_date_non_utf8_operands_are_octal_escaped { |ctx|
  let s = uu.scene(ctx)?
  let cases = [
    {args: [b"gr\xf6n"], expected: "invalid date 'gr\\366n'"},
    {args: [b"+%Y", b"\xf1ao"], expected: "extra operand '\\361ao'"},
    {args: [b"-d", b"2031-07-23", b"%Y\xd8"], expected: "the argument '%Y\\330' lacks a leading '+'"},
  ]
  for item in cases {
    let args = [Path.parse_bytes(arg)? for arg in item.args]
    let r = uu.invoke_paths(s, "date", args)?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, item.expected)
  }
}

# origin: uutils test_date::test_percent_percent_not_replaced
test test_uu_date_percent_percent_not_replaced { |ctx|
  let s = uu.scene(ctx)?
  let cases = [
    {format: "+%%H%%I%%k%%l%%M%%N%%p%%P%%r%%R%%s%%S%%T%%X%%z%%Z", expected: "%H%I%k%l%M%N%p%P%r%R%s%S%T%X%z%Z\n"},
    {format: "+%%a%%A%%b%%B%%c%%C%%d%%D%%e%%F%%g%%G%%h%%j%%m%%u%%U%%V%%w%%W%%x%%y%%Y", expected: "%a%A%b%B%c%C%d%D%e%F%g%G%h%j%m%u%U%V%w%W%x%y%Y\n"},
  ]
  for item in cases {
    let r = uu.invoke(s, "date", [item.format], vars: {TZ: "UTC"})?
    uu.succeeds(r)
    uu.stdout_is(r, item.expected)
    let localized = uu.invoke(s, "date", [item.format], vars: {TZ: "UTC", LC_ALL: "fr_FR.UTF-8"})?
    uu.succeeds(localized)
    uu.stdout_is(localized, item.expected)
  }
}

# origin: uutils test_date::test_relative_weekdays
test test_uu_date_relative_weekdays { |ctx|
  let s = uu.scene(ctx)?
  let day_ns = 86400000000000
  let today = time.now() / 86400000 * day_ns
  for offset in range(7) {
    let weekday = time.format(today + offset * day_ns, "%a", utc: true)?
    for direction in ["last", "this", "next"] {
      let r = uu.invoke(s, "date", ["-d", f"{direction} {weekday}", "--rfc-3339=seconds", "--utc"])?
      uu.succeeds(r)
      let expected = if direction == "last" {
        today - (7 - offset) * day_ns
      } else if direction == "this" and offset == 0 {
        today
      } else if direction == "next" and offset == 0 {
        today + 7 * day_ns
      } else {
        today + offset * day_ns
      }
      uu.stdout_is(r, time.format(expected, "%Y-%m-%d %H:%M:%S+00:00", utc: true)? + "\n")
    }
  }
}

# origin: uutils test_date::test_single_dash_as_date
test test_uu_date_single_dash_as_date { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "invalid date")
}

# origin: uutils test_date::test_single_dash_as_date_string
test test_uu_date_single_dash_as_date_string { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-u", "-d", "-", "+%T"])?
  uu.succeeds(r)
  uu.stdout_is(r, "00:00:00\n")
}

# origin: uutils test_date::test_write_error
test test_uu_date_write_error { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", [], stdout: p"/dev/full")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "date: write error: No space left on device\n")
}
