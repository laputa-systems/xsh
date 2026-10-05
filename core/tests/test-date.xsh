test test_date_format { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u +%Y
  assert output.trim().count_chars() == 4
  let offset = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u +%z
  assert offset.trim() == "+0000"
}

test test_date_native_epoch_and_nanoseconds { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d "@-0.000000001" "+%Y-%m-%d %H:%M:%S.%N %s"
  assert output == "1969-12-31 23:59:59.999999999 -1\n"
  let plus = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d "@0" "+literal+%Y"
  assert plus == "literal+1970\n"
  let iso = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d "2000-02-29 12:34:56" -Iseconds
  assert iso == "2000-02-29T12:34:56+00:00\n"
}

test test_date_batch_invalid_bytes_continue_and_nul_terminates { |ctx|
  let target = test.temp_file(ctx, name: "batch-dates.txt", contents: b"2024-01-15 12:00:00\x00ignored\nHello\xffx\n2024-01-16 13:00:00\n")?
  let output = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -f $target "+%F %T"
  assert output.status.exited_with(1)
  assert output.stdout == "2024-01-15 12:00:00\n2024-01-16 13:00:00\n"
  assert output.stderr == "date: invalid date 'Hello\\377x'\n"
}
