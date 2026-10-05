test test_getty_requires_baud_and_tty { |ctx|
  let err = test.temp_path(ctx, name: "getty.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" -- -n -i 2> $err
  assert ! result.exited_with(0)
  assert "missing operand" in err.read_text()?
}

test test_getty_no_prompt_hands_off_to_login_with_term { |ctx|
  let root = test.temp_dir(ctx, name: "getty")?
  let fake_login = fp"{root}/fake-login"
  fake_login.write(
    r"""#!/bin/sh
printf '%s:%s' "${TERM-}" "$#"
""",
    mode: 0o755,
  )

  let handed_off = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" -- -i -n -l $fake_login 0 /dev/null \
    vt100
  assert handed_off.status.exited_with(0), handed_off.stderr
  assert handed_off.stdout == "vt100:0"
}

test test_getty_prompts_and_passes_username { |ctx|
  let root = test.temp_dir(ctx, name: "getty")?
  let fake_login = fp"{root}/fake-login"
  fake_login.write(
    r"""#!/bin/sh
printf '%s' "$1"
""",
    mode: 0o755,
  )
  let typed = test.temp_file(ctx, name: "typed.txt", contents: b"alice\n")?

  let prompted = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" -- -i -l $fake_login 0 /dev/null \
    < ${typed}
  assert prompted.status.exited_with(0), prompted.stderr
  assert prompted.stdout == "login: alice"
}
