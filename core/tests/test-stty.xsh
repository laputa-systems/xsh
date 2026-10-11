type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/stty.xsh against a pseudo-terminal: `--file` names the replica,
# so the applet's descriptor is a real terminal while its streams are files.
proc stty_run(
  ctx: TestContext,
  pty: UnixPty,
  args: List[Str],
  vars: Record = {LC_ALL: "C", COLUMNS: "80"},
) [fs, process, error] -> Result[Ran] {
  stty_plain(ctx, ["--file", pty.name].extend(args), vars)
}

proc stty_plain(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C", COLUMNS: "80"},
  input = b"",
  sink: Path? = null,
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "stty")?
  let out = sink ?? fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/stty.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: if sink == null { out.read_text()? } else { "" }, stderr: err.read_text()?})
}

proc show(ctx: TestContext, pty: UnixPty, args: List[Str] = []) [fs, process, error] -> Result[Str] {
  Ok(stty_run(ctx, pty, args)?.stdout)
}

# A whole word of a settings listing.
pure has(text: Str, word: Str) -> Bool {
  word in text.words()
}

# A fresh pseudo-terminal pair.
proc with_pty() [process, error] -> Result[UnixPty] {
  unix.open_pty()
}

test test_stty_prints_speed_line_and_deviations_from_sane { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let result = stty_run(ctx, pty, [])?
  assert result.status == 0
  assert result.stderr == ""
  assert result.stdout == "speed 38400 baud; line = 0;\n-brkint -imaxbel\n", result.stdout
}

test test_stty_all_lists_every_setting_in_gnu_order { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let result = stty_run(ctx, pty, ["-a"])?
  assert result.status == 0
  assert result.stdout == r"""speed 38400 baud; rows 0; columns 0; line = 0;
intr = ^C; quit = ^\; erase = ^?; kill = ^U; eof = ^D; eol = <undef>;
eol2 = <undef>; swtch = <undef>; start = ^Q; stop = ^S; susp = ^Z; rprnt = ^R;
werase = ^W; lnext = ^V; discard = ^O; min = 1; time = 0;
-parenb -parodd -cmspar cs8 -hupcl -cstopb cread -clocal -crtscts
-ignbrk -brkint -ignpar -parmrk -inpck -istrip -inlcr -igncr icrnl ixon -ixoff
-iuclc -ixany -imaxbel -iutf8
opost -olcuc -ocrnl onlcr -onocr -onlret -ofill -ofdel nl0 cr0 tab0 bs0 vt0 ff0
isig icanon iexten echo echoe echok -echonl -noflsh -xcase -tostop -echoprt
echoctl echoke -flusho -extproc
""", result.stdout
  assert stty_run(ctx, pty, ["--all"])?.stdout == result.stdout
}

test test_stty_flags_set_and_clear_and_show_when_they_differ_from_sane { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert stty_run(ctx, pty, ["-echo", "-icanon", "ixany", "tostop"])?.status == 0
  assert show(ctx, pty)? == "speed 38400 baud; line = 0;\nmin = 1; time = 0;\n-brkint ixany -imaxbel\n-icanon -echo tostop\n"

  let all = show(ctx, pty, ["-a"])?
  assert has(all, "ixany") and has(all, "-icanon") and has(all, "-echo") and has(all, "tostop")

  assert stty_run(ctx, pty, ["echo", "icanon", "-ixany", "-tostop"])?.status == 0
  assert show(ctx, pty)? == "speed 38400 baud; line = 0;\n-brkint -imaxbel\n"
}

test test_stty_flag_aliases_name_the_same_bits { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert stty_run(ctx, pty, ["tandem", "-crterase", "prterase", "-ctlecho", "-crtkill"])?.status == 0
  let all = show(ctx, pty, ["-a"])?
  assert has(all, "ixoff"), all
  assert has(all, "-echoe") and has(all, "echoprt") and has(all, "-echoctl") and has(all, "-echoke"), all
}

test test_stty_control_characters_take_every_spelling { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  for case in [
    ["intr", "^A", "intr = ^A;"],
    ["intr", "a", "intr = a;"],
    ["intr", "0x41", "intr = A;"],
    ["intr", "0101", "intr = A;"],
    ["intr", "65", "intr = A;"],
    ["quit", "^?", "quit = ^?;"],
    ["erase", "^-", "erase = <undef>;"],
    ["kill", "undef", "kill = <undef>;"],
    ["eof", "", "eof = <undef>;"],
    ["eol", "0xe1", "eol = M-a;"],
    ["eol2", "0x81", "eol2 = M-^A;"],
    ["susp", "^Zjunk", "susp = ^Z;"],
  ] {
    assert stty_run(ctx, pty, [case[0], case[1]])?.status == 0, f"{case[0]} {case[1]}"
    let line = show(ctx, pty, ["-a"])?
    assert case[2] in line, f"{case[0]} {case[1]}: {line}"
  }

  assert stty_run(ctx, pty, ["sane"])?.status == 0
  assert show(ctx, pty)? == "speed 38400 baud; line = 0;\n", "sane restores the characters and the flags it owns"
}

test test_stty_min_and_time_show_under_non_canonical_input { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert stty_run(ctx, pty, ["-icanon", "min", "5", "time", "7"])?.status == 0
  let plain = show(ctx, pty)?
  assert plain.starts_with("speed 38400 baud; line = 0;\nmin = 5; time = 7;\n"), plain
  assert stty_run(ctx, pty, ["min", "255", "time", "0"])?.status == 0
  assert "min = 255; time = 0;" in show(ctx, pty)?
}

test test_stty_combination_settings_follow_the_gnu_definitions { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  assert stty_run(ctx, pty, ["raw"])?.status == 0
  let raw = show(ctx, pty, ["-a"])?
  assert has(raw, "-icanon") and has(raw, "-isig") and has(raw, "-opost") and has(raw, "-ixon") and has(raw, "-icrnl") and "min = 1; time = 0;" in raw, raw

  assert stty_run(ctx, pty, ["cooked"])?.status == 0
  let cooked = show(ctx, pty, ["-a"])?
  assert has(cooked, "icanon") and has(cooked, "isig") and has(cooked, "opost") and has(cooked, "brkint") and has(
    cooked,
    "istrip",
  ), cooked
  assert stty_run(ctx, pty, ["-cooked"])?.status == 0
  assert "-icanon" in show(ctx, pty, ["-a"])?, "-cooked is raw"
  assert stty_run(ctx, pty, ["-raw"])?.status == 0
  assert " icanon " in show(ctx, pty, ["-a"])?, "-raw is cooked"

  assert stty_run(ctx, pty, ["cbreak"])?.status == 0
  assert "-icanon" in show(ctx, pty, ["-a"])?
  assert stty_run(ctx, pty, ["-cbreak"])?.status == 0
  assert " icanon " in show(ctx, pty, ["-a"])?

  assert stty_run(ctx, pty, ["nl"])?.status == 0
  let nl = show(ctx, pty, ["-a"])?
  assert has(nl, "-icrnl") and has(nl, "-onlcr"), nl
  assert stty_run(ctx, pty, ["-nl"])?.status == 0
  let back = show(ctx, pty, ["-a"])?
  assert has(back, "icrnl") and has(back, "onlcr") and has(back, "-inlcr") and has(back, "-ocrnl"), back

  assert stty_run(ctx, pty, ["intr", "^A", "erase", "^B", "kill", "^C", "ek"])?.status == 0
  let ek = show(ctx, pty, ["-a"])?
  assert "intr = ^A;" in ek and "erase = ^?;" in ek and "kill = ^U;" in ek, ek

  assert stty_run(ctx, pty, ["dec"])?.status == 0
  let dec = show(ctx, pty, ["-a"])?
  assert "intr = ^C;" in dec and "erase = ^?;" in dec and "kill = ^U;" in dec and has(dec, "-ixany") and has(
    dec,
    "echoctl",
  ), dec

  assert stty_run(ctx, pty, ["-echoe", "-echoctl", "-echoke", "crt"])?.status == 0
  let crt = show(ctx, pty, ["-a"])?
  assert has(crt, "echoe") and has(crt, "echoctl") and has(crt, "echoke"), crt

  assert stty_run(ctx, pty, ["lcase"])?.status == 0
  let lcase = show(ctx, pty, ["-a"])?
  assert has(lcase, "xcase") and has(lcase, "iuclc") and has(lcase, "olcuc"), lcase
  assert stty_run(ctx, pty, ["-LCASE"])?.status == 0
  let plain = show(ctx, pty, ["-a"])?
  assert has(plain, "-xcase") and has(plain, "-iuclc") and has(plain, "-olcuc"), plain

  assert stty_run(ctx, pty, ["-tabs"])?.status == 0
  assert " tab3 " in show(ctx, pty, ["-a"])?
  assert stty_run(ctx, pty, ["tabs"])?.status == 0
  assert " tab0 " in show(ctx, pty, ["-a"])?

  assert stty_run(ctx, pty, ["decctlq"])?.status == 0
  assert "-ixany" in show(ctx, pty, ["-a"])?, "GNU applies decctlq as -ixany"
  assert stty_run(ctx, pty, ["-decctlq"])?.status == 0
  assert " ixany " in show(ctx, pty, ["-a"])?
}

test test_stty_delay_styles_replace_each_other { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert stty_run(ctx, pty, ["tab3", "nl1", "cr2", "bs1", "vt1", "ff1"])?.status == 0
  assert show(ctx, pty)? == "speed 38400 baud; line = 0;\n-brkint -imaxbel\nnl1 cr2 tab3 bs1 vt1 ff1\n"
  assert stty_run(ctx, pty, ["tab1", "cr0"])?.status == 0
  let all = show(ctx, pty, ["-a"])?
  assert has(all, "tab1") and has(all, "cr0") and "tab3" not in all and "cr2" not in all, all
}

test test_stty_sets_and_reports_speeds { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  for case in [
    ["9600", "9600"],
    ["115200", "115200"],
    ["ispeed", "19200", "19200"],
    ["ospeed", "4800", "4800"],
    ["exta", "19200"],
    ["extb", "38400"],
    ["50", "50"],
    ["ispeed", "9600.49", "9600"],
    ["ispeed", "9600.50", "9600"],
    ["ispeed", "9599.51", "9600"],
    ["ispeed", "  +9600", "9600"],
    ["ispeed", "  9600.", "9600"],
  ] {
    let speed = case[-1]
    let words = case[0..case.len() - 1]
    assert stty_run(ctx, pty, words)?.status == 0, words.join(" ")
    assert stty_run(ctx, pty, ["speed"])?.stdout == f"{speed}\n", words.join(" ")
  }

  assert stty_run(ctx, pty, ["38400"])?.status == 0
  let mixed = stty_run(ctx, pty, ["ispeed", "9600", "ospeed", "4800"])?
  assert mixed.status == 1
  assert mixed.stderr == "stty: asymmetric input (9600), output (4800) speeds not supported\n", mixed.stderr
  assert stty_run(ctx, pty, ["speed"])?.stdout == "38400\n", "nothing was applied"
  assert stty_run(ctx, pty, ["ispeed", "9600", "ospeed", "9600"])?.status == 0
}

test test_stty_rejects_speeds_the_host_does_not_have { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  for speed in [
    "995",
    "abc",
    "999999999",
    "9599..",
    "9600..",
    "9600.5.",
    "9600.50.",
    "9600.0.",
    "++9600",
    "0x2580",
    "96E2",
    "9600,0",
    "9600.0 ",
    "-1",
  ] {
    let result = stty_run(ctx, pty, ["ispeed", speed])?
    assert result.status == 1, speed
    assert result.stderr.starts_with(f"stty: invalid ispeed '{speed}'\nTry 'stty --help' for more information.\n"), result.stderr
  }

  let ospeed = stty_run(ctx, pty, ["ospeed", "995"])?
  assert ospeed.stderr.starts_with("stty: invalid ospeed '995'\n"), ospeed.stderr
  let bare = stty_run(ctx, pty, ["100"])?
  assert bare.stderr == "stty: invalid argument '100'\nTry 'stty --help' for more information.\n", bare.stderr
}

test test_stty_window_size_settings_and_size { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert stty_run(ctx, pty, ["rows", "24", "cols", "80"])?.status == 0
  assert stty_run(ctx, pty, ["size"])?.stdout == "24 80\n"
  assert stty_run(ctx, pty, ["columns", "100"])?.status == 0
  assert stty_run(ctx, pty, ["size"])?.stdout == "24 100\n"
  assert stty_run(ctx, pty, ["rows", "0x1E", "cols", "036"])?.status == 0
  assert stty_run(ctx, pty, ["size"])?.stdout == "30 30\n", "hexadecimal and octal arguments"
  assert stty_run(ctx, pty, ["rows", "65537"])?.status == 0
  assert stty_run(ctx, pty, ["size"])?.stdout == "1 30\n", "sizes wrap to 16 bits"
  assert stty_run(ctx, pty, ["rows", "40", "cols", "120", "size", "rows", "50", "size"])?.stdout == "40 120\n50 120\n"
  assert "rows 50; columns 120;" in show(ctx, pty, ["-a"])?
}

test test_stty_integer_arguments_accept_gnu_byte_block_suffixes { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  assert stty_run(ctx, pty, ["rows", "1B", "cols", "2b", "size"])?.stdout == "1024 1024\n"
  assert stty_run(ctx, pty, ["rows", "b", "size"])?.stdout == "512 1024\n"

  let invalid = stty_run(ctx, pty, ["rows", "1k"])?
  assert invalid.status == 1
  assert invalid.stderr == "stty: invalid integer argument: '1k'\n"
}

test test_stty_line_discipline_number { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert stty_run(ctx, pty, ["line", "0"])?.status == 0
  assert "line = 0;" in show(ctx, pty)?
  let overflow = stty_run(ctx, pty, ["line", "256"])?
  let diagnostic = "stty: invalid line discipline '256': Value too large for defined data type\n"
  assert overflow.status == 0
  assert overflow.stderr == diagnostic + diagnostic, overflow.stderr
}

test test_stty_save_round_trips_the_whole_state { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let saved = stty_run(ctx, pty, ["--save"])?.stdout
  assert rx"^[0-9a-f]+(:[0-9a-f]+){35}\n$".matches(saved), saved
  assert stty_run(ctx, pty, ["-g"])?.stdout == saved
  assert saved.starts_with("500:5:bf:"), f"no input-speed bits in a terminal that never had them: {saved}"

  assert stty_run(ctx, pty, ["raw", "-echo", "intr", "^A", "9600"])?.status == 0
  assert stty_run(ctx, pty, ["-g"])?.stdout != saved
  assert stty_run(ctx, pty, [saved.trim()])?.status == 0
  assert stty_run(ctx, pty, ["-g"])?.stdout == saved
  assert stty_run(ctx, pty, ["speed"])?.stdout == "38400\n"
}

test test_stty_malformed_saved_states_are_invalid_arguments { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let zeros = ["0" for _ in range(32)].join(":")
  let chars = ["1c" for _ in range(32)]

  for state in [
    "500:5:4bf",
    "500:5:4bf:8a3b",
    f"500:5:{zeros}:8a3b:extra",
    f"500::4bf:8a3b:{zeros}",
    "500:5:4bf:8a3b:" + [""].extend(chars[1..32]).join(":"),
    "500:5:4bf:8a3b:" + ["xyz"].extend(chars[1..32]).join(":"),
    "500:5:4bf:8a3b:" + ["1c "].extend(chars[1..32]).join(":"),
    "500:5:4bf:8a3b:" + ["100"].extend(chars[1..32]).join(":"),
  ] {
    let result = stty_run(ctx, pty, [state])?
    assert result.status == 1, state
    assert result.stdout == ""
    assert result.stderr == f"stty: invalid argument {gnu_quote(state)}\nTry 'stty --help' for more information.\n", result.stderr
  }
}

pure gnu_quote(text: Str) -> Str {
  f"'{text}'"
}

test test_stty_saved_state_with_kernel_unknown_characters_is_not_fully_applied { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let saved = stty_run(ctx, pty, ["-g"])?.stdout.trim()
  let fields = saved.split(":")
  let changed = fields[0..35].join(":") + ":1c"
  let result = stty_run(ctx, pty, [changed])?
  assert result.status == 1
  assert result.stderr == f"stty: {pty.name}: unable to perform all requested operations\n", result.stderr
}

test test_stty_usage_errors_use_the_gnu_wording { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  for case in [
    {args: ["intr"], text: "missing argument to 'intr'"},
    {args: ["ispeed"], text: "missing argument to 'ispeed'"},
    {args: ["rows"], text: "missing argument to 'rows'"},
    {args: ["min"], text: "missing argument to 'min'"},
    {args: ["-econl"], text: "invalid argument '-econl'"},
    {args: ["igpar"], text: "invalid argument 'igpar'"},
    {args: ["-1"], text: "invalid argument '-1'"},
    {args: ["notachar", "^C"], text: "invalid argument 'notachar'"},
    {args: ["-dec"], text: "invalid argument '-dec'"},
    {args: ["-crt"], text: "invalid argument '-crt'"},
    {args: ["-ek"], text: "invalid argument '-ek'"},
    {args: ["-sane"], text: "invalid argument '-sane'"},
    {args: ["-cs7"], text: "invalid argument '-cs7'"},
    {args: ["-cs8"], text: "invalid argument '-cs8'"},
    {args: ["-tab3"], text: "invalid argument '-tab3'"},
    {args: ["-intr"], text: "invalid argument '-intr'"},
    {args: ["-xyz"], text: "invalid argument '-xyz'"},
    {args: ["-F"], text: "invalid argument '-F'"},
    {args: ["-"], text: "invalid argument '-'"},
  ] {
    let result = stty_run(ctx, pty, case.args)?
    assert result.status == 1, case.text
    assert result.stdout == ""
    assert result.stderr == f"stty: {case.text}\nTry 'stty --help' for more information.\n", result.stderr
  }
}

test test_stty_integer_arguments_name_what_is_wrong { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  for case in [
    ["intr", "cc", "stty: invalid integer argument: 'cc'\n"],
    ["intr", "ABC", "stty: invalid integer argument: 'ABC'\n"],
    ["intr", "256", "stty: invalid integer argument: '256': Value too large for defined data type\n"],
    ["intr", "0x100", "stty: invalid integer argument: '0x100': Value too large for defined data type\n"],
    ["intr", "0400", "stty: invalid integer argument: '0400': Value too large for defined data type\n"],
    ["erase", "0xFFF", "stty: invalid integer argument: '0xFFF': Value too large for defined data type\n"],
    ["min", "256", "stty: invalid integer argument: '256': Value too large for defined data type\n"],
    ["time", "1000", "stty: invalid integer argument: '1000': Value too large for defined data type\n"],
    ["min", "-1", "stty: invalid integer argument: '-1'\n"],
    ["time", "abc", "stty: invalid integer argument: 'abc'\n"],
    ["rows", "-1", "stty: invalid integer argument: '-1'\n"],
    ["rows", "", "stty: invalid integer argument: ''\n"],
    ["cols", "xyz", "stty: invalid integer argument: 'xyz'\n"],
    ["columns", "12.5", "stty: invalid integer argument: '12.5'\n"],
    ["cols", "4294967296", "stty: invalid integer argument: '4294967296': Value too large for defined data type\n"],
    ["line", "-1", "stty: invalid integer argument: '-1'\n"],
    ["rows", "08", "stty: invalid integer argument: '08'\n"],
  ] {
    let words = case[0..case.len() - 1]
    let result = stty_run(ctx, pty, words)?
    assert result.status == 1, words.join(" ")
    assert result.stdout == ""
    assert result.stderr == case[-1], f"{words.join(" ")}: {result.stderr}"
  }
}

test test_stty_checks_every_setting_before_changing_anything { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let before = stty_run(ctx, pty, ["-g"])?.stdout
  let result = stty_run(ctx, pty, ["-echo", "intr", "^A", "bogus"])?
  assert result.status == 1
  assert stty_run(ctx, pty, ["-g"])?.stdout == before
}

test test_stty_output_styles_exclude_modes_and_each_other { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  for words in [
    ["--save", "nl0"],
    ["--all", "nl0"],
    ["--all", "size"],
    ["--save", "speed"],
    ["-a", "-echo"],
    ["echo", "-a"],
    ["-ax"],
    ["-g", "-x"],
    ["-gicanon"],
  ] {
    let result = stty_run(ctx, pty, words)?
    assert result.status == 1, words.join(" ")
    assert result.stderr == "stty: when specifying an output style, modes may not be set\n", f"{words.join(" ")}: {result.stderr}"
  }

  for words in [["--save", "--all"], ["--all", "--save"], ["-ag"], ["-ga"]] {
    let result = stty_run(ctx, pty, words)?
    assert result.status == 1, words.join(" ")
    assert result.stderr == "stty: the options for verbose and stty-readable output styles are\nmutually exclusive\n", result.stderr
  }
}

test test_stty_options_are_recognized_only_as_whole_elements { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert stty_run(ctx, pty, ["--al"])?.stdout.starts_with("speed 38400 baud; rows 0; columns 0; line = 0;")
  assert stty_run(ctx, pty, ["--sav"])?.stdout.starts_with("500:5:")
  assert stty_run(ctx, pty, ["-aa"])?.stdout.starts_with("speed 38400 baud; rows 0")
  assert stty_run(ctx, pty, ["-gg"])?.stdout.starts_with("500:5:")
  assert stty_run(ctx, pty, ["-icanon"])?.status == 0, "-icanon is a setting although it contains an a"
  assert stty_run(ctx, pty, ["icanon"])?.status == 0
  assert stty_plain(ctx, ["-F" + pty.name, "-g"])?.stdout.starts_with("500:5:")
  assert stty_plain(ctx, ["-gF", pty.name])?.stdout.starts_with("500:5:")
  assert stty_plain(ctx, ["--file=" + pty.name, "-a"])?.stdout.starts_with("speed 38400")
  assert stty_plain(ctx, ["--fi", pty.name])?.stdout.starts_with("speed 38400")
}

test test_stty_drain_alone_prints_and_otherwise_chooses_when_to_apply { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert show(ctx, pty, ["drain"])? == "speed 38400 baud; line = 0;\n-brkint -imaxbel\n"
  assert show(ctx, pty, ["-drain"])? == "speed 38400 baud; line = 0;\n-brkint -imaxbel\n"
  assert stty_run(ctx, pty, ["-drain", "-echo"])?.status == 0
  assert stty_run(ctx, pty, ["drain", "echo"])?.status == 0
  assert stty_run(ctx, pty, ["size", "drain"])?.stdout == "0 0\n"
}

test test_stty_output_wraps_at_columns { |ctx|
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  for width in [20, 40, 50, 80] {
    let result = stty_run(ctx, pty, ["-a"], {LC_ALL: "C", COLUMNS: f"{width}"})?
    for line in result.stdout.lines() {
      assert line.count_chars() <= width, f"COLUMNS={width}: {line}"
    }
  }

  let wide = stty_run(ctx, pty, ["-a"], {LC_ALL: "C", COLUMNS: "200"})?
  assert ! [line for line in wide.stdout.lines() if line.count_chars() > 80].is_empty()

  for bad in ["invalid", "0", "-10", ""] {
    let result = stty_run(ctx, pty, ["-a"], {LC_ALL: "C", COLUMNS: bad})?
    assert result.status == 0, bad
    for line in result.stdout.lines() {
      assert line.count_chars() <= 80, f"COLUMNS={bad}: {line}"
    }
  }

  let narrow = stty_run(ctx, pty, [], {LC_ALL: "C", COLUMNS: "30"})?
  for line in narrow.stdout.lines() {
    assert line.count_chars() <= 30, line
  }
}

test test_stty_that_cannot_use_the_terminal_names_the_device { |ctx|
  let silent = stty_plain(ctx, [], input: b"")?
  assert silent.status == 1
  assert silent.stdout == ""
  # The C libraries spell ENOTTY differently.
  assert silent.stderr in ["stty: 'standard input': Inappropriate ioctl for device\n", "stty: 'standard input': Not a tty\n"], silent.stderr

  let missing = stty_plain(ctx, ["--file", "/nonexistent/device"])?
  assert missing.status == 1
  assert missing.stderr == "stty: /nonexistent/device: No such file or directory\n", missing.stderr

  let device = stty_plain(ctx, ["-F", "/dev/null", "-a"])?
  assert device.status == 1
  assert device.stderr in ["stty: /dev/null: Inappropriate ioctl for device\n", "stty: /dev/null: Not a tty\n"], device.stderr
  assert stty_plain(ctx, ["-F", "/dev/null", "echo"])?.stderr in ["stty: /dev/null: Inappropriate ioctl for device\n", "stty: /dev/null: Not a tty\n"]
}

test test_stty_help_and_version_go_to_stdout { |ctx|
  let help = stty_plain(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: stty [-F DEVICE | --file=DEVICE] [SETTING]...\n")
  assert "Combination settings:" in help.stdout
  let version = stty_plain(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("stty (XSH core)")
}

test test_stty_reports_a_full_device { |ctx|
  if ! p"/dev/full".exists() {
    test.skip("/dev/full is not available")
  }

  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let result = stty_plain(ctx, ["--file", pty.name], sink: /dev/full)?
  assert result.status == 1
  assert result.stderr == "stty: write error: No space left on device\n", result.stderr
}

test test_stty_checks_terminal_before_window_sizes_and_numeric_speeds { |ctx|
  for args in [["rows", "abc"], ["cols", "xyz"], ["columns", "12.5"], ["rows", "-1"], ["cols", "4294967296"], ["rows", ""], ["100"], ["ispeed", "995"], ["ospeed", "999999999"]] {
    let result = stty_plain(ctx, args)?
    assert result.status == 1, args.join(" ")
    assert result.stdout == ""
    assert result.stderr == "stty: 'standard input': Inappropriate ioctl for device\n", result.stderr
  }
}

test test_stty_line_overflow_warns_in_both_setting_passes { |ctx|
  let diagnostic = "stty: invalid line discipline '256': Value too large for defined data type\n"
  let plain = stty_plain(ctx, ["line", "256"])?
  assert plain.status == 1
  assert plain.stderr == diagnostic + "stty: 'standard input': Inappropriate ioctl for device\n", plain.stderr
  let pty = with_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let result = stty_run(ctx, pty, ["line", "256"])?
  assert result.status == 0
  assert result.stderr == diagnostic + diagnostic, result.stderr
  assert "line = 0;" in show(ctx, pty)?
}
