use core.lib.btop_terminal as terminal

test test_btop_terminal_capabilities_follow_environment_and_geometry {
  let linux_console = terminal.detect({
    term: "linux",
    colorterm: "",
    no_color: false,
    lc_all: "",
    lc_ctype: "C",
    lang: "en_US.UTF-8",
    rows: 40,
    cols: 120,
  })
  assert linux_console.color_depth == .Ansi16
  assert ! linux_console.utf8
  assert linux_console.symbols == .Ascii
  assert linux_console.minimum_size

  let truecolor = terminal.detect({
    term: "xterm-256color",
    colorterm: "truecolor",
    no_color: false,
    lc_all: "",
    lc_ctype: "",
    lang: "en_US.utf8",
    rows: 24,
    cols: 80,
  })
  assert truecolor.color_depth == .TrueColor
  assert truecolor.utf8
  assert truecolor.symbols == .Unicode
  assert truecolor.minimum_size

  let color_disabled = terminal.detect({
    term: "xterm-256color",
    colorterm: "24bit",
    no_color: true,
    lc_all: "C.UTF-8",
    lc_ctype: "",
    lang: "",
    rows: 23,
    cols: 79,
  })
  assert color_disabled.color_depth == .NoColor
  assert color_disabled.symbols == .Unicode
  assert ! color_disabled.minimum_size
  assert color_disabled.rows == 23 and color_disabled.cols == 79

  let dumb = terminal.detect({
    term: "dumb",
    colorterm: "truecolor",
    no_color: false,
    lc_all: "",
    lc_ctype: "",
    lang: "C",
    rows: 24,
    cols: 80,
  })
  assert dumb.color_depth == .NoColor
  assert dumb.symbols == .Ascii
}

test test_btop_terminal_refuses_piped_input_before_reading_it { |ctx|
  let source = """
use core.lib.btop_terminal as terminal
let result = terminal.begin()
assert result is Err(_)
assert io.stdin_read(1)? == b"q"
"""
  let output = test.run_xsh(
    ctx,
    source,
    [],
    [],
    env: {XSH_MODULE_PATH: f"{ctx.core_dir}:{ctx.core_dir.parent()}"},
    stdin: b"q",
    name: "btop-terminal-nontty.xsh",
  )?
  assert output.success, output.stderr
}
