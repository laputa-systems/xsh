pure flag_named(table: UnixTtyTable, name: Str) -> UnixTtyFlag {
  return [flag for flag in table.flags if flag.name == name][0]
}

pure char_named(table: UnixTtyTable, name: Str) -> UnixTtyChar {
  return [char for char in table.chars if char.name == name][0]
}

pure errno_of(result: Result[Any, Error]) -> Int {
  match result {
    Err(failure) => return failure.errno ?? -1
    Ok(_) => return 0
  }
}

pure flag_on(table: UnixTtyTable, attrs: UnixTtyAttrs, name: Str) -> Bool {
  let flag = flag_named(table, name)
  let word = if flag.field == "iflag" {
    attrs.iflag
  } else if flag.field == "oflag" {
    attrs.oflag
  } else if flag.field == "cflag" {
    attrs.cflag
  } else {
    attrs.lflag
  }
  return word.bit_and(flag.mask) == flag.value
}

test test_pty_pairs_are_terminals_with_a_name { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert pty.master > 2
  assert pty.replica > 2
  assert pty.name.starts_with("/dev/")
  assert unix.isatty(pty.master)
  assert unix.isatty(pty.replica)
  assert unix.ttyname(pty.replica)? == pty.name
  assert ! unix.isatty(-1)
  assert ! unix.isatty(99999)
}

test test_non_terminals_fail_with_enotty { |ctx|
  let fd = unix.open_fd(/dev/null)?
  defer unix.close_fd(fd)
  assert ! unix.isatty(fd)
  assert errno_of(unix.ttyname(fd)) == 25
  assert errno_of(unix.tty_attrs(fd)) == 25
  assert errno_of(unix.window_size(fd)) == 25
  assert errno_of(unix.foreground_group(fd)) == 25
  assert errno_of(unix.tty_session(fd)) == 25
}

test test_open_fd_names_the_path_and_close_fd_protects_the_standard_streams { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  let fd = unix.open_fd(fp"{pty.name}")?
  assert fd > 2
  assert unix.isatty(fd)
  assert unix.ttyname(fd)? == pty.name
  unix.close_fd(fd)?
  assert errno_of(unix.close_fd(fd)) == 9

  let missing = unix.open_fd(fp"/nonexistent/xsh-tty")
  assert missing is Err(is NotFound)
  if let Err(failure) = missing {
    assert failure.errno == 2
    assert "/nonexistent/xsh-tty" in failure.message
  }
  test.error_kind(unix.close_fd(1), "invalid-argument")
  assert ! unix.isatty(fd)
}

test test_the_controlling_terminal_is_a_device_or_absent {
  match unix.controlling_tty() {
    Ok(name) => assert name.starts_with("/dev/")
    Err(failure) => assert failure.errno == 6
  }
}

test test_window_size_round_trips { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let initial = unix.window_size(pty.replica)?
  assert initial.rows == 0 and initial.cols == 0

  unix.set_window_size(24, 80, fd: pty.replica)?
  let size = unix.window_size(pty.replica)?
  assert size.rows == 24
  assert size.cols == 80
  assert size.xpixel == 0 and size.ypixel == 0
  assert unix.window_size(pty.master)? == size

  unix.set_window_size(50, 132, xpixel: 800, ypixel: 600, fd: pty.master)?
  let pixels = unix.window_size(pty.replica)?
  assert pixels.rows == 50 and pixels.cols == 132
  assert pixels.xpixel == 800 and pixels.ypixel == 600

  test.error_kind(unix.set_window_size(70000, 80, fd: pty.replica), "invalid-argument")
}

test test_terminals_without_a_session_have_no_foreground_group { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  # The pair is not this process's controlling terminal.
  assert errno_of(unix.foreground_group(pty.replica)) == 25
  assert errno_of(unix.tty_session(pty.replica)) == 25
  assert errno_of(unix.set_foreground_group(process.group_id()?, fd: pty.replica)) == 25
  test.error_kind(unix.set_foreground_group(0, fd: pty.replica), "invalid-argument")
}

test test_the_termios_table_names_flags_characters_and_speeds {
  let table = unix.tty_table()
  assert table.speeds[0] == 0
  assert 9600 in table.speeds and 38400 in table.speeds and 115200 in table.speeds

  let echo = flag_named(table, "echo")
  assert echo.field == "lflag"
  assert echo.mask == echo.value
  assert echo.sane
  assert ! flag_named(table, "echonl").sane

  let cs7 = flag_named(table, "cs7")
  let cs8 = flag_named(table, "cs8")
  assert cs7.field == "cflag" and cs8.field == "cflag"
  assert cs7.mask == cs8.mask
  assert cs7.value != cs8.value
  assert cs8.sane

  let intr = char_named(table, "intr")
  assert intr.sane == 3
  assert char_named(table, "erase").sane == 127
  assert char_named(table, "min").sane == 1
  assert char_named(table, "time").sane == 0
  assert intr.index != char_named(table, "quit").index

  for flag in table.flags {
    assert [other for other in table.flags if other.name == flag.name].len() == 1, f"{flag.name} is listed twice"
  }
}

test test_tty_attrs_follow_the_table_through_a_round_trip { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let table = unix.tty_table()
  let attrs = unix.tty_attrs(pty.replica)?
  assert attrs.control_chars.len() > char_named(table, "time").index
  assert attrs.control_chars[char_named(table, "intr").index] == 3
  assert flag_on(table, attrs, "echo") == attrs.echo
  assert flag_on(table, attrs, "icrnl") == attrs.crnl
  assert attrs.ispeed > 0 and attrs.ospeed > 0
  assert attrs.line == 0
  test.error_kind(unix.set_tty_attrs({...attrs, line: 300}, fd: pty.replica), "invalid-argument")

  # Clear ECHO through the table and write it back.
  let echo = flag_named(table, "echo")
  var chars = attrs.control_chars
  chars[char_named(table, "intr").index] = 24
  let changed = {...attrs, lflag: attrs.lflag.clear_bits(echo.mask), control_chars: chars}
  unix.set_tty_attrs(changed, fd: pty.replica)?
  let after = unix.tty_attrs(pty.replica)?
  assert ! after.echo
  assert after.control_chars[char_named(table, "intr").index] == 24
  assert after.iflag == attrs.iflag

  unix.set_tty_attrs(attrs, fd: pty.replica, when: "drain")?
  unix.set_tty_attrs(attrs, fd: pty.replica, when: "flush")?
  assert unix.tty_attrs(pty.replica)?.echo == attrs.echo
  test.error_kind(unix.set_tty_attrs(attrs, fd: pty.replica, when: "later"), "invalid-argument")
}

test test_tty_modes_follow_stty_definitions { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let table = unix.tty_table()
  let start = unix.tty_attrs(pty.replica)?

  let raw = unix.tty_mode(start, "raw")?
  for name in ["icanon", "isig", "opost", "icrnl", "ixon", "istrip", "brkint"] {
    assert ! flag_on(table, raw, name), f"raw leaves {name} on"
  }
  assert raw.control_chars[char_named(table, "min").index] == 1
  assert raw.control_chars[char_named(table, "time").index] == 0
  assert raw.ispeed == start.ispeed
  assert raw.cflag == start.cflag
  assert raw.raw == (! flag_on(table, raw, "icanon") and ! flag_on(table, raw, "echo") and ! flag_on(table, raw, "isig") and ! flag_on(table, raw, "icrnl") and ! flag_on(table, raw, "ixon"))

  let cooked = unix.tty_mode(raw, "cooked")?
  for name in ["brkint", "ignpar", "istrip", "icrnl", "ixon", "opost", "isig", "icanon"] {
    assert flag_on(table, cooked, name), f"cooked leaves {name} off"
  }
  assert cooked.control_chars[char_named(table, "eof").index] == 4
  assert cooked.control_chars[char_named(table, "eol").index] == 0
  assert cooked.crnl

  let cbreak = unix.tty_mode(start, "cbreak")?
  assert ! flag_on(table, cbreak, "icanon")
  assert flag_on(table, cbreak, "isig") == flag_on(table, start, "isig")

  # sane restores the defaults a fresh terminal has, whatever came before.
  let sane = unix.tty_mode(raw, "sane")?
  for flag in table.flags {
    if flag.sane {
      assert flag_on(table, sane, flag.name), f"sane leaves {flag.name} off"
    }
  }
  assert sane.control_chars[char_named(table, "intr").index] == 3
  assert sane.control_chars[char_named(table, "quit").index] == 28
  assert sane.control_chars[char_named(table, "erase").index] == 127
  assert sane.control_chars[char_named(table, "susp").index] == 26
  assert sane.echo and sane.crnl and ! sane.raw
  assert sane.lflag == start.lflag
  assert sane.control_chars == start.control_chars

  unix.set_tty_attrs(raw, fd: pty.replica)?
  let applied = unix.tty_attrs(pty.replica)?
  assert ! flag_on(table, applied, "icanon")
  assert applied.control_chars[char_named(table, "min").index] == 1

  test.error_kind(unix.tty_mode(start, "cooler"), "invalid-argument")
  test.error_kind(unix.tty_mode({iflag: 0}, "raw"), "invalid-argument")
}

test test_the_load_average_is_three_non_negative_floats {
  let load = unix.load_average()?
  assert load.one >= 0.0 and load.five >= 0.0 and load.fifteen >= 0.0
}
