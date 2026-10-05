test test_filesystem_path_methods_require_the_fs_effect { |ctx|
  for call in ["source.lines()", "source.bytes_lines()", "source.touch_from(source)", "source.read_lines()"] {
    let denied = test.run_script(ctx, effect_probe("source: Path", "error", call))?
    assert ! denied.success, call
    assert "check.effect-violation" in denied.stderr, denied.stderr
    assert "requires the `fs` effect" in denied.stderr, denied.stderr
    let _ = test.expect(ctx, effect_probe("source: Path", "fs, error", call), status: 0)?
  }
}

test test_env_path_methods_require_the_env_effect { |ctx|
  for call in ["env.PATH.prepend(tools)", "env.PATH.append(tools)", "env.PATH.pop()"] {
    let denied = test.run_script(ctx, effect_probe("tools: Path", "fs, error", call))?
    assert ! denied.success, call
    assert "check.effect-violation" in denied.stderr, denied.stderr
    assert "requires the `env` effect" in denied.stderr, denied.stderr
    let _ = test.expect(ctx, effect_probe("tools: Path", "env, error", call), status: 0)?
  }
}

# A proc that is declared and never called, so only the checker decides.
pure effect_probe(parameter: Str, effects: Str, call: Str) -> Str {
  f"""proc probe({parameter}) [{effects}] {{
  let _ = {call}
}}
"""
}
