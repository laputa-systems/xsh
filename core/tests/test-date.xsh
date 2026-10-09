type DateRun = {status: Int, stdout: Str, stderr: Str}

proc date_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C", TZ: "UTC0"}, stdin = b"") [fs, process, error] -> Result[DateRun] {
  let root = test.temp_dir(ctx, name: "date")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/date.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_date_formats_current_utc_date { |ctx|
  let year = date_run(ctx, ["-u", "+%Y"])?
  assert year.status == 0 and rx"^[0-9]{4}\n$".matches(year.stdout), year.stdout
  let offset = date_run(ctx, ["-u", "+%z %:z"])?
  assert offset.stdout == "+0000 +00:00\n", offset.stdout
}

test test_date_epoch_iso_and_modifiers { |ctx|
  let epoch = date_run(ctx, ["-u", "-d", "@0", "+%F %T %j %q %N"])?
  assert epoch.status == 0 and epoch.stdout == "1970-01-01 00:00:00 001 1 000000000\n", epoch.stdout
  let posix_clock = date_run(ctx, ["-u", "-d", "@0", "+%r"])?
  assert posix_clock.status == 0 and posix_clock.stdout == "12:00:00 AM\n", posix_clock.stdout
  let comment = date_run(ctx, ["-u", "-d", "2024(note)-01-03", "+%F"])?
  assert comment.status == 0 and comment.stdout == "2024-01-03\n", comment.stdout
  let padded = date_run(ctx, ["-u", "-d", "2024-01-03 04:05:06", "+%_d %-d %3N"])?
  assert padded.status == 0 and padded.stdout == " 3 3 000\n", padded.stdout
  let flags = date_run(ctx, ["-u", "-d", "@0", "+%^B %#Z %+5Y"])?
  assert flags.status == 0 and flags.stdout == "JANUARY utc +1970\n", flags.stdout
  let iso = date_run(ctx, ["-u", "-d", "@0", "--iso-8601=seconds"])?
  assert iso.stdout == "1970-01-01T00:00:00+00:00\n", iso.stdout
  let short_iso = date_run(ctx, ["-u", "-d", "@0", "--iso-8601=second"])?
  assert short_iso.status == 0 and short_iso.stdout == iso.stdout, short_iso.stdout
  let iso_auto = date_run(ctx, ["-u", "-d", "@0", "--iso-8601=auto"])?
  assert iso_auto.status == 0 and iso_auto.stdout == "1970-01-01\n", iso_auto.stdout
  let iso_hours = date_run(ctx, ["-u", "-d", "@0", "--iso-8601=hours"])?
  assert iso_hours.stdout == "1970-01-01T00+00:00\n", iso_hours.stdout
  let iso_minutes = date_run(ctx, ["-u", "-d", "@0", "--iso-8601=minutes"])?
  assert iso_minutes.stdout == "1970-01-01T00:00+00:00\n", iso_minutes.stdout
  let iso_ns = date_run(ctx, ["-u", "-d", "@0", "--iso-8601=ns"])?
  assert iso_ns.stdout == "1970-01-01T00:00:00,000000000+00:00\n", iso_ns.stdout
  let rfc = date_run(ctx, ["-u", "-d", "@0", "--rfc-3339=ns"])?
  assert rfc.status == 0 and rfc.stdout == "1970-01-01 00:00:00.000000000+00:00\n", rfc.stdout
  let email = date_run(ctx, ["-u", "-d", "@0", "-R"])?
  assert email.status == 0 and email.stdout == "Thu, 01 Jan 1970 00:00:00 +0000\n", email.stdout
  let email_locale = date_run(ctx, ["-u", "-d", "@0", "-R"], {LC_ALL: "fr_FR.UTF-8", TZ: "UTC0"})?
  assert email_locale.status == 0 and email_locale.stdout == email.stdout, email_locale.stdout
}

test test_date_file_reference_and_error_options { |ctx|
  let batch = date_run(ctx, ["-u", "-f", "-", "+%F"] , {LC_ALL: "C", TZ: "UTC0"}, b"1970-01-01\n2000-02-29\n")?
  assert batch.stdout == "1970-01-01\n2000-02-29\n", batch.stdout
  let root = test.temp_dir(ctx, name: "date-file")?
  let file_input = fp"{root}/dates"
  file_input.write("1970-01-01\n2000-02-29\n")
  let from_file = date_run(ctx, ["-u", "-f", file_input.display(), "+%F"])?
  assert from_file.status == 0 and from_file.stdout == "1970-01-01\n2000-02-29\n", from_file.stdout
  let file = fp"{root}/ref"
  file.write("ref")
  fs.set_times(file, mtime_ns: 1234000000000)?
  let reference = date_run(ctx, ["-u", "-r", file.display(), "+%s %N"])?
  assert reference.status == 0 and reference.stdout == "1234 000000000\n", reference.stdout
  let rejected = date_run(ctx, ["--set", "not-a-date"])?
  assert rejected.status == 1 and "invalid date 'not-a-date'" in rejected.stderr, rejected.stderr
  let debug = date_run(ctx, ["--debug"])?
  assert debug.status == 0 and debug.stderr == "", debug.stderr
}

test test_date_midnight_and_resolution_options { |ctx|
  let dash = date_run(ctx, ["-u", "-d", "-", "+%T"])?
  assert dash.status == 0 and dash.stdout == "00:00:00\n", dash.stdout
  let empty = date_run(ctx, ["-u", "-d", "", "+%T"])?
  assert empty.status == 0 and empty.stdout == "00:00:00\n", empty.stdout
  let resolution = date_run(ctx, ["--resolution"])?
  assert resolution.status == 0 and rx"^0\.[0-9]{9}\n$".matches(resolution.stdout), resolution.stdout
  let resolution_iso = date_run(ctx, ["--resolution", "-Iseconds"])?
  assert resolution_iso.status == 0 and resolution_iso.stdout.starts_with("1970-01-01T00:00:00"), resolution_iso.stdout
}

test test_date_parse_negative_fraction_and_invalid_file_line { |ctx|
  let negative = date_run(ctx, ["-u", "-d", "@-1.5", "+%s %N"])?
  assert negative.status == 0 and negative.stdout == "-2 500000000\n", negative.stdout
  let batch = date_run(ctx, ["-u", "-f", "-", "+%F"], {LC_ALL: "C", TZ: "UTC0"}, b"@0\nbad-date\n@86400\n")?
  assert batch.status == 1 and batch.stdout == "1970-01-01\n1970-01-02\n", batch.stdout
  assert "invalid date 'bad-date'" in batch.stderr, batch.stderr
  let nul = date_run(ctx, ["-u", "-f", "-", "+%F"], {LC_ALL: "C", TZ: "UTC0"}, b"@0\0ignored\n")?
  assert nul.status == 0 and nul.stdout == "1970-01-01\n", nul.stdout
  let invalid_utf8 = date_run(ctx, ["-u", "-f", "-", "+%F"], {LC_ALL: "C", TZ: "UTC0"}, b"@0\nHello\xffx\n@86400\n")?
  assert invalid_utf8.status == 1 and invalid_utf8.stdout == "1970-01-01\n1970-01-02\n", invalid_utf8.stdout
  assert "invalid date 'Hello\\377x'" in invalid_utf8.stderr, invalid_utf8.stderr
}

test test_date_requires_plus_for_explicit_date_formats { |ctx|
  let bare = date_run(ctx, ["%Y"])?
  assert bare.status == 1 and "invalid date '%Y'" in bare.stderr, bare.stderr
  let missing = date_run(ctx, ["-d", "@0", "%Y"])?
  assert missing.status == 1 and "lacks a leading '+'" in missing.stderr, missing.stderr
  let bad = date_run(ctx, ["-u", "-d", "not-a-date", "+%F"])?
  assert bad.status == 1 and "invalid date 'not-a-date'" in bad.stderr, bad.stderr
}

test test_date_rejects_unbounded_format_width { |ctx|
  let output = date_run(ctx, ["+%99999999999c"])?
  assert output.status == 1 and output.stdout == ""
  assert "format modifier width '" in output.stderr and "specifier '%c'" in output.stderr, output.stderr
  let width = date_run(ctx, ["+%65536Y"])?
  assert width.status == 1 and width.stdout == "" and "format modifier width '65536'" in width.stderr and "specifier '%Y'" in width.stderr, width.stderr
}

test test_date_rejects_resolution_with_other_sources { |ctx|
  let conflict = date_run(ctx, ["--resolution", "-d", "2025-01-01"])?
  assert conflict.status == 1 and "--resolution cannot be used" in conflict.stderr, conflict.stderr
}

test test_date_accepts_australian_timezone_abbreviations { |ctx|
  let awst = date_run(ctx, ["-u", "-d", "2021-03-20 14:53:01 AWST", "+%F %T"])?
  assert awst.status == 0 and awst.stdout == "2021-03-20 06:53:01\n", awst.stderr
  let acdt = date_run(ctx, ["-u", "-d", "2021-03-20 14:53:01 ACDT", "+%F %T"])?
  assert acdt.status == 0 and acdt.stdout == "2021-03-20 04:23:01\n", acdt.stderr
}

test test_date_debug_reports_input_without_changing_output { |ctx|
  let debug = date_run(ctx, ["--debug", "-u", "-d", "2005-01-01", "+%Y"])?
  assert debug.status == 0 and debug.stdout == "2005\n", debug.stdout
  assert "date: input string: 2005-01-01" in debug.stderr, debug.stderr
  assert "date: parsed date part: (Y-M-D) 2005-01-01" in debug.stderr, debug.stderr
  assert "date: parsed time part:" in debug.stderr, debug.stderr
  assert "date: input timezone:" in debug.stderr, debug.stderr
  assert "date: warning: using midnight" in debug.stderr, debug.stderr
}
