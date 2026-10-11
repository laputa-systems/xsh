use support.uu

# origin: busybox sum/sum -r file file does print both names
test test_bb_sum_sum_r_file_file_does_print_both_names_5c5c240d { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "sum.tests", "checksum filename coverage\n")?
  let r = uu.invoke(s, "sum", ["-r", "sum.tests", "sum.tests"])?
  uu.succeeds(r)
  assert [line for line in r.stdout.utf8()?.lines() if "sum.tests" in line].len() == 2
}

# origin: busybox sum/sum -s file does print file's name
test test_bb_sum_sum_s_file_does_print_file_s_name_69cda718 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "sum.tests", "checksum filename coverage\n")?
  let r = uu.invoke(s, "sum", ["-s", "sum.tests"])?
  uu.succeeds(r)
  assert [line for line in r.stdout.utf8()?.lines() if "sum.tests" in line].len() == 1
}
