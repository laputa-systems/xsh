use support.uu

# origin: busybox dd/dd-accepts-if
test test_bb_dd_dd_accepts_if_0d8639a2 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "I WANT\n")?
  let r = uu.invoke(s, "dd", ["if=foo"])?
  uu.succeeds(r)
  uu.stdout_is(r, "I WANT\n")
}

# origin: busybox dd/dd-accepts-of
test test_bb_dd_dd_accepts_of_966ca519 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["of=foo"], stdin: b"I WANT\n")?
  uu.succeeds(r)
  uu.no_stdout(r)
  uu.file_is(s, "foo", "I WANT\n")
}

# origin: busybox dd/dd-copies-from-standard-input-to-standard-output
test test_bb_dd_dd_copies_from_standard_input_to_standard_output_2d9a16ab { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", [], stdin: b"I WANT\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "I WANT\n")
}

# origin: busybox dd/dd-count-bytes
test test_bb_dd_dd_count_bytes_29da685c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["count=3", "iflag=count_bytes"], stdin: b"I WANT\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "I W")
}

# origin: busybox dd/dd-prints-count-to-standard-error
test test_bb_dd_dd_prints_count_to_standard_error_4a3efef4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["of=foo"], stdin: b"I WANT\n")?
  uu.succeeds(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "records")
}

# origin: busybox dd/dd-reports-write-errors
test test_bb_dd_dd_reports_write_errors_e4045320 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "dd-reports-write-errors", "A nonempty original fixture exercises a failing device write.\n")?
  let r = uu.invoke(s, "dd", ["if=dd-reports-write-errors", "of=/dev/full"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
}

