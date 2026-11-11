use core.lib.tui_terminal as terminal

test test_tui_terminal_cleanup_sequence_closes_every_mode {
  assert terminal.cleanup_sequences() == "\u{1b}[0m\u{1b}[?2026l\u{1b}[?2004l\u{1b}[?25h\u{1b}[?1049l"
}

test test_tui_terminal_hand_off_writes_pending_bytes_to_the_parent_descriptor { |ctx|
  let source = """
use core.lib.tui_input as input
use core.lib.tui_terminal as terminal
let prefix = bytes.from_ints([27, 91]) ?? b""
let decoded = input.push(input.initial(), prefix, 10)
let pending = input.unconsumed(decoded.state)
assert pending == prefix
assert terminal.hand_off(pending)?
"""
  let output = test.run_xsh(
    ctx,
    source,
    env: {
      XSH_MODULE_PATH: f"{ctx.core_dir}:{ctx.core_dir.parent()}",
      XSH_TUI_INPUT_FD: "1",
    },
    name: "tui-handoff.xsh",
  )?
  assert output.success, output.stderr
  assert output.stdout_bytes == b"\x1b["
}

test test_tui_terminal_hand_off_without_a_parent_descriptor_is_a_no_op { |ctx|
  let source = """
use core.lib.tui_terminal as terminal
assert terminal.hand_off(b"x")? == false
"""
  let output = test.run_xsh(
    ctx,
    source,
    env: {
      XSH_MODULE_PATH: f"{ctx.core_dir}:{ctx.core_dir.parent()}",
      XSH_TUI_INPUT_FD: "",
    },
    name: "tui-handoff-unset.xsh",
  )?
  assert output.success, output.stderr
  assert output.stdout_bytes == b""
}

test test_tui_terminal_rejects_an_invalid_parent_descriptor { |ctx|
  let source = """
use core.lib.tui_terminal as terminal
assert terminal.hand_off(b"x") is Err(_)
"""
  let output = test.run_xsh(
    ctx,
    source,
    env: {
      XSH_MODULE_PATH: f"{ctx.core_dir}:{ctx.core_dir.parent()}",
      XSH_TUI_INPUT_FD: "not-a-number",
    },
    name: "tui-handoff-invalid.xsh",
  )?
  assert output.success, output.stderr
}
