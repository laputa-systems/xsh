##! Native ports of the uutils stty integration tests.

use support.uu as uu

# origin: uutils test_stty::all_and_print_setting
test test_uu_stty_all_and_print_setting { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["--all", "size"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "when specifying an output style, modes may not be set")
}

# origin: uutils test_stty::all_and_setting
test test_uu_stty_all_and_setting { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["--all", "nl0"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "when specifying an output style, modes may not be set")
}

# origin: uutils test_stty::conflicting_print_modes
test test_uu_stty_conflicting_print_modes { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["--save", "speed"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "when specifying an output style, modes may not be set")
  let r1 = uu.invoke(s, "stty", ["--all", "speed"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "when specifying an output style, modes may not be set")
}

# origin: uutils test_stty::control_char_decimal_overflow
test test_uu_stty_control_char_decimal_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["quit", "256"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "Value too large")
  let r1 = uu.invoke(s, "stty", ["susp", "1000"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "Value too large")
}

# origin: uutils test_stty::control_char_multiple_chars_error
test test_uu_stty_control_char_multiple_chars_error { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["intr", "ABC"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid integer argument")
}

# origin: uutils test_stty::control_char_overflow_hex
test test_uu_stty_control_char_overflow_hex { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["erase", "0xFFF"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "Value too large for defined data type")
}

# origin: uutils test_stty::control_char_overflow_octal
test test_uu_stty_control_char_overflow_octal { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["kill", "0777"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "Value too large for defined data type")
}

# origin: uutils test_stty::file_argument
test test_uu_stty_file_argument { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["--file", "/nonexistent/device"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "No such file or directory")
}

# origin: uutils test_stty::grouped_flag_removal
test test_uu_stty_grouped_flag_removal { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["-cs7"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid argument '-cs7'")
  let r1 = uu.invoke(s, "stty", ["-cs8"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid argument '-cs8'")
}

# origin: uutils test_stty::help_output
test test_uu_stty_help_output { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["--help"])?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "Usage:")
  uu.stdout_contains(r0, "stty")
}

# origin: uutils test_stty::invalid_baud_rate
test test_uu_stty_invalid_baud_rate { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["ispeed", "notabaud"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid ispeed")
  let r1 = uu.invoke(s, "stty", ["ospeed", "999999999"])?
  uu.fails(r1)
  uu.stderr_is(r1, "stty: 'standard input': Inappropriate ioctl for device\n")
}

# origin: uutils test_stty::invalid_control_char_names
test test_uu_stty_invalid_control_char_names { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["notachar", "^C"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid argument 'notachar'")
}

# origin: uutils test_stty::invalid_integer_cols
test test_uu_stty_invalid_integer_cols { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["cols", "xyz"])?
  uu.fails(r0)
  uu.stderr_is(r0, "stty: 'standard input': Inappropriate ioctl for device\n")
  let r1 = uu.invoke(s, "stty", ["columns", "12.5"])?
  uu.fails(r1)
  uu.stderr_is(r1, "stty: 'standard input': Inappropriate ioctl for device\n")
}

# origin: uutils test_stty::invalid_integer_rows
test test_uu_stty_invalid_integer_rows { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["rows", "abc"])?
  uu.fails(r0)
  uu.stderr_is(r0, "stty: 'standard input': Inappropriate ioctl for device\n")
  let r1 = uu.invoke(s, "stty", ["rows", "-1"])?
  uu.fails(r1)
  uu.stderr_is(r1, "stty: 'standard input': Inappropriate ioctl for device\n")
}

# origin: uutils test_stty::invalid_mapping
test test_uu_stty_invalid_mapping { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["intr", "cc"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid integer argument: 'cc'")
  let r1 = uu.invoke(s, "stty", ["intr", "256"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid integer argument: '256': Value too large for defined data type")
  let r2 = uu.invoke(s, "stty", ["intr", "0x100"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "invalid integer argument: '0x100': Value too large for defined data type")
  let r3 = uu.invoke(s, "stty", ["intr", "0400"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "invalid integer argument: '0400': Value too large for defined data type")
}

# origin: uutils test_stty::invalid_min_value
test test_uu_stty_invalid_min_value { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["min", "256"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "Value too large")
  let r1 = uu.invoke(s, "stty", ["min", "-1"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid integer argument")
}

# origin: uutils test_stty::invalid_setting
test test_uu_stty_invalid_setting { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["-econl"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid argument '-econl'")
  let r1 = uu.invoke(s, "stty", ["igpar"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid argument 'igpar'")
}

# origin: uutils test_stty::invalid_time_value
test test_uu_stty_invalid_time_value { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["time", "1000"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "Value too large")
  let r1 = uu.invoke(s, "stty", ["time", "abc"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid integer argument")
}

# origin: uutils test_stty::line
test test_uu_stty_line { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["line"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument to 'line'")
  let r1 = uu.invoke(s, "stty", ["line", "-1"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid integer argument: '-1'")
  let r2 = uu.invoke(s, "stty", ["line", "256"])?
  uu.fails(r2)
  uu.stderr_is(r2, "stty: invalid line discipline '256': Value too large for defined data type\nstty: 'standard input': Inappropriate ioctl for device\n")
}

# origin: uutils test_stty::min_and_time
test test_uu_stty_min_and_time { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["min"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument to 'min'")
  let r1 = uu.invoke(s, "stty", ["time"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "missing argument to 'time'")
  let r2 = uu.invoke(s, "stty", ["min", "-1"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "invalid integer argument: '-1'")
  let r3 = uu.invoke(s, "stty", ["time", "-1"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "invalid integer argument: '-1'")
  let r4 = uu.invoke(s, "stty", ["min", "256"])?
  uu.fails(r4)
  uu.stderr_contains(r4, "invalid integer argument: '256': Value too large for defined data type")
  let r5 = uu.invoke(s, "stty", ["time", "256"])?
  uu.fails(r5)
  uu.stderr_contains(r5, "invalid integer argument: '256': Value too large for defined data type")
}

# origin: uutils test_stty::missing_arg_cols
test test_uu_stty_missing_arg_cols { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["cols"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "cols")
}

# origin: uutils test_stty::missing_arg_columns
test test_uu_stty_missing_arg_columns { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["columns"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "columns")
}

# origin: uutils test_stty::missing_arg_control_char
test test_uu_stty_missing_arg_control_char { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["intr"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "intr")
  let r1 = uu.invoke(s, "stty", ["erase"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "missing argument")
  uu.stderr_contains(r1, "erase")
}

# origin: uutils test_stty::missing_arg_ispeed
test test_uu_stty_missing_arg_ispeed { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["ispeed"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "ispeed")
}

# origin: uutils test_stty::missing_arg_line
test test_uu_stty_missing_arg_line { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["line"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "line")
}

# origin: uutils test_stty::missing_arg_min
test test_uu_stty_missing_arg_min { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["min"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "min")
}

# origin: uutils test_stty::missing_arg_ospeed
test test_uu_stty_missing_arg_ospeed { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["ospeed"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "ospeed")
}

# origin: uutils test_stty::missing_arg_rows
test test_uu_stty_missing_arg_rows { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["rows"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "rows")
}

# origin: uutils test_stty::missing_arg_time
test test_uu_stty_missing_arg_time { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["time"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument")
  uu.stderr_contains(r0, "time")
}

# origin: uutils test_stty::multiple_invalid_args
test test_uu_stty_multiple_invalid_args { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["invalid1", "invalid2"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid argument")
}

# origin: uutils test_stty::no_mapping
test test_uu_stty_no_mapping { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["intr"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing argument to 'intr'")
}

# origin: uutils test_stty::non_negatable_combo
test test_uu_stty_non_negatable_combo { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["-dec"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid argument '-dec'")
  let r1 = uu.invoke(s, "stty", ["-crt"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid argument '-crt'")
  let r2 = uu.invoke(s, "stty", ["-ek"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "invalid argument '-ek'")
}

# origin: uutils test_stty::row_column_sizes
test test_uu_stty_row_column_sizes { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["rows", "-1"])?
  uu.fails(r0)
  uu.stderr_is(r0, "stty: 'standard input': Inappropriate ioctl for device\n")
  let r1 = uu.invoke(s, "stty", ["columns", "-1"])?
  uu.fails(r1)
  uu.stderr_is(r1, "stty: 'standard input': Inappropriate ioctl for device\n")
  let r2 = uu.invoke(s, "stty", ["cols", "4294967296"])?
  uu.fails(r2)
  uu.stderr_is(r2, "stty: 'standard input': Inappropriate ioctl for device\n")
  let r3 = uu.invoke(s, "stty", ["rows", ""])?
  uu.fails(r3)
  uu.stderr_is(r3, "stty: 'standard input': Inappropriate ioctl for device\n")
  let r4 = uu.invoke(s, "stty", ["columns"])?
  uu.fails(r4)
  uu.stderr_contains(r4, "missing argument to 'columns'")
  let r5 = uu.invoke(s, "stty", ["rows"])?
  uu.fails(r5)
  uu.stderr_contains(r5, "missing argument to 'rows'")
}

# origin: uutils test_stty::save_and_setting
test test_uu_stty_save_and_setting { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["--save", "nl0"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "when specifying an output style, modes may not be set")
}

# origin: uutils test_stty::test_invalid_arg
test test_uu_stty_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["--definitely-invalid"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "invalid argument")
  uu.stderr_contains(r0, "--definitely-invalid")
}

# origin: uutils test_stty::version_output
test test_uu_stty_version_output { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["--version"])?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "stty")
}

# origin: uutils test_stty::test_basic
test test_uu_stty_basic { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let r = uu.invoke(s, "stty", ["--file", tty.name])?
  uu.succeeds(r)
  uu.stdout_contains(r, "speed")
}

# origin: uutils test_stty::test_all_flag
test test_uu_stty_all_flag { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let r = uu.invoke(s, "stty", ["--all", "--file", tty.name])?
  uu.succeeds(r)
  for flag in ["parenb", "parmrk", "ixany", "onlcr", "icanon", "noflsh"] { uu.stdout_contains(r, flag) }
}

# origin: uutils test_stty::test_sane
test test_uu_stty_sane { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let changed = uu.invoke(s, "stty", ["--file", tty.name, "intr", "^A"])?
  uu.succeeds(changed)
  let shown = uu.invoke(s, "stty", ["--file", tty.name])?
  uu.succeeds(shown)
  uu.stdout_contains(shown, "intr = ^A")
  let sane = uu.invoke(s, "stty", ["--file", tty.name, "sane"])?
  uu.succeeds(sane)
  let restored = uu.invoke(s, "stty", ["--file", tty.name])?
  uu.succeeds(restored)
  assert ! ("intr = ^A" in restored.stdout.utf8()?)
}

# origin: uutils test_stty::valid_baud_formats
test test_uu_stty_valid_baud_formats { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  for speed in ["  +9600", "9600.49", "9600.50", "9599.51", "  9600."] {
    let r = uu.invoke(s, "stty", ["--file", tty.name, "ispeed", speed])?
    uu.succeeds(r)
  }
}

# origin: uutils test_stty::invalid_baud_setting
test test_uu_stty_invalid_baud_setting { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stty", ["100"])?
  uu.fails(r0)
  uu.stderr_is(r0, "stty: 'standard input': Inappropriate ioctl for device\n")
  let r1 = uu.invoke(s, "stty", ["-1"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid argument '-1'")
  let r2 = uu.invoke(s, "stty", ["ispeed"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "missing argument to 'ispeed'")
  let r3 = uu.invoke(s, "stty", ["ospeed"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "missing argument to 'ospeed'")
  let r4 = uu.invoke(s, "stty", ["ispeed", "995"])?
  uu.fails(r4)
  uu.stderr_is(r4, "stty: 'standard input': Inappropriate ioctl for device\n")
  let r5 = uu.invoke(s, "stty", ["ospeed", "995"])?
  uu.fails(r5)
  uu.stderr_is(r5, "stty: 'standard input': Inappropriate ioctl for device\n")
  for speed in ["9599..", "9600..", "9600.5.", "9600.50.", "9600.0.", "++9600", "0x2580", "96E2", "9600,0", "9600.0 "] {
    uu.fails(uu.invoke(s, "stty", ["ispeed", speed])?)
  }
}

# origin: uutils test_stty::test_save_and_restore
test test_uu_stty_save_and_restore { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let state = uu.invoke(s, "stty", ["--save", "--file", tty.name])?
  uu.succeeds(state)
  let saved = state.stdout.utf8()?.trim()
  assert ":" in saved
  let restored = uu.invoke(s, "stty", ["--file", tty.name, saved])?
  uu.succeeds(restored)
}

# origin: uutils test_stty::test_save_with_g_flag
test test_uu_stty_save_with_g_flag { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let state = uu.invoke(s, "stty", ["-g", "--file", tty.name])?
  uu.succeeds(state)
  let saved = state.stdout.utf8()?.trim()
  assert ":" in saved
  let restored = uu.invoke(s, "stty", ["--file", tty.name, saved])?
  uu.succeeds(restored)
}

# origin: uutils test_stty::test_save_restore_after_change
test test_uu_stty_save_restore_after_change { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let state = uu.invoke(s, "stty", ["--save", "--file", tty.name])?
  uu.succeeds(state)
  let saved = state.stdout.utf8()?.trim()
  let changed = uu.invoke(s, "stty", ["--file", tty.name, "intr", "^A"])?
  uu.succeeds(changed)
  let restored = uu.invoke(s, "stty", ["--file", tty.name, saved])?
  uu.succeeds(restored)
  let shown = uu.invoke(s, "stty", ["--file", tty.name])?
  uu.succeeds(shown)
  assert ! ("intr = ^A" in shown.stdout.utf8()?)
}

# origin: uutils test_stty::test_row_column_hex_octal
test test_uu_stty_row_column_hex_octal { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  for pair in [["rows", "0x1E"], ["rows", "0x1e"], ["rows", "0X1e"], ["rows", "036"], ["cols", "0X1E"], ["columns", "30"], ["rows", "0"]] {
    let r = uu.invoke(s, "stty", ["--file", tty.name, pair[0], pair[1]])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_stty::test_ispeed_ospeed_valid_speeds
test test_uu_stty_ispeed_ospeed_valid_speeds { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  for pair in [["ispeed", "50"], ["ispeed", "9600"], ["ispeed", "19200"], ["ospeed", "1200"], ["ospeed", "9600"], ["ospeed", "38400"]] {
    let r = uu.invoke(s, "stty", ["--file", tty.name, pair[0], pair[1]])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_stty::test_combo_tabs
test test_uu_stty_combo_tabs { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["tabs", "-tabs"] {
    let stdin = unix.open_pty()?
    defer unix.close_fd(stdin.master)
    defer unix.close_fd(stdin.replica)
    let stdout = unix.open_pty()?
    defer unix.close_fd(stdout.master)
    defer unix.close_fd(stdout.replica)
    let stderr = unix.open_pty()?
    defer unix.close_fd(stderr.master)
    defer unix.close_fd(stderr.replica)
    for fd in [stdin.replica, stdout.replica, stderr.replica] {
      unix.set_window_size(30, 80, xpixel: 640, ypixel: 300, fd: fd)?
    }
    let r = uu.invoke_from_path(s, "stty", [arg], stdin: Path(stdin.name), stdout: Path(stdout.name), stderr: Path(stderr.name))?
    uu.succeeds(r)
    # The child has exited; unread terminal bytes would make the master readable.
    assert unix.poll_fd(stderr.master, ["readable"], timeout_ms: 0)? == [], "expected empty terminal stderr"
  }
}

# origin: uutils test_stty::test_stty_uses_stdin
test test_uu_stty_stty_uses_stdin { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let plain = uu.invoke_from_path(s, "stty", [], stdin: Path(tty.name))?
  uu.succeeds(plain)
  uu.stdout_contains(plain, "speed")
  let state = uu.invoke_from_path(s, "stty", ["-g"], stdin: Path(tty.name))?
  uu.succeeds(state)
  let saved = state.stdout.utf8()?.trim()
  assert ":" in saved
  uu.succeeds(uu.invoke_from_path(s, "stty", [saved], stdin: Path(tty.name))?)
  uu.succeeds(uu.invoke_from_path(s, "stty", ["rows", "30", "cols", "100"], stdin: Path(tty.name))?)
  let shown = uu.invoke_from_path(s, "stty", ["--all"], stdin: Path(tty.name))?
  uu.succeeds(shown)
  uu.stdout_contains(shown, "rows 30")
  uu.stdout_contains(shown, "columns 100")
}

# origin: uutils test_stty::test_columns_env_wrapping
test test_uu_stty_columns_env_wrapping { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  for columns in [20, 40, 50] {
    let r = uu.invoke(s, "stty", ["--all", "--file", tty.name], vars: {COLUMNS: f"{columns}"})?
    uu.succeeds(r)
    for line in r.stdout.utf8()?.lines().collect() { assert line.byte_len() <= columns }
  }
  let wide = uu.invoke(s, "stty", ["--all", "--file", tty.name], vars: {COLUMNS: "200"})?
  uu.succeeds(wide)
  assert [line for line in wide.stdout.utf8()?.lines().collect() if line.byte_len() > 80].len() > 0
  for invalid in ["invalid", "0", "-10"] {
    uu.succeeds(uu.invoke(s, "stty", ["--all", "--file", tty.name], vars: {COLUMNS: invalid})?)
  }
  let plain = uu.invoke(s, "stty", ["--file", tty.name], vars: {COLUMNS: "30"})?
  uu.succeeds(plain)
  for line in plain.stdout.utf8()?.lines().collect() { assert line.byte_len() <= 30 }
}

# origin: uutils test_stty::test_saved_state_valid_formats
test test_uu_stty_saved_state_valid_formats { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let saved = "500:5:f00bf:8a3b:3:1c:7f:15:4:0:1:0:11:13:1a:0:12:f:17:16:0:0:0:0:0:0:0:0:0:0:0:0:0:0:0:0"
  let r = uu.invoke(s, "stty", ["--file", tty.name, saved])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_stty::test_saved_state_invalid_formats
test test_uu_stty_saved_state_invalid_formats { |ctx|
  let s = uu.scene(ctx)?
  let tty = unix.open_pty()?
  defer unix.close_fd(tty.master)
  defer unix.close_fd(tty.replica)
  let cc_zeros = ["0" for _ in range(32)].join(":")
  let invalid_states = [
    "500:5:4bf",
    "500:5:4bf:8a3b",
    f"500:5:{cc_zeros}:8a3b:extra",
    f"500::4bf:8a3b:{cc_zeros}",
    "500:5:4bf:8a3b:" + [if i == 0 { "" } else { "1c" } for i in range(32)].join(":"),
    "500:5:4bf:8a3b:" + [if i == 0 { "xyz" } else { "1c" } for i in range(32)].join(":"),
    "500:5:4bf:8a3b:" + [if i == 0 { "1c " } else { "1c" } for i in range(32)].join(":"),
    "500:5:4bf:8a3b:" + [if i == 0 { "100" } else { "1c" } for i in range(32)].join(":"),
  ]
  for state in invalid_states {
    let r = uu.invoke(s, "stty", ["--file", tty.name, state])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_is(r, f"stty: invalid argument '{state}'\nTry 'stty --help' for more information.\n")
  }
}

