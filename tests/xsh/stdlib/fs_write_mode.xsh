test test_write_with_a_mode_creates_the_file_with_exactly_those_bits { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-new")?

  # Wider than a default umask leaves a plain write, and narrower: the bits
  # are the mode, not the mode under the umask.
  for mode in [0o600, 0o640, 0o755, 0o777, 0o000] {
    let by_method = fp"{root}/method-{mode}"
    by_method.write("text\n", mode:)
    assert by_method.metadata()?.mode % 4096 == mode

    let by_module = fp"{root}/module-{mode}"
    by_module.write(b"bytes\n", mode:)
    assert by_module.metadata()?.mode % 4096 == mode
  }

  assert fp"{root}/method-{0o640}".read_text()? == "text\n"
  assert fp"{root}/module-{0o755}".read_bytes()? == b"bytes\n"

  # The mode may be positional, and each data type has it.
  let positional = fp"{root}/positional"
  positional.write(b"one", 0o604)
  assert positional.metadata()?.mode % 4096 == 0o604
  positional.write("two", 0o640)
  assert positional.metadata()?.mode % 4096 == 0o640
  assert positional.read_text()? == "two"
}

test test_write_with_a_mode_sets_the_bits_of_an_existing_file { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-existing")?
  let file = fp"{root}/existing"
  file.write("a longer first version\n", mode: 0o666)

  file.write("short\n", mode: 0o600)
  assert file.read_text()? == "short\n"
  assert file.metadata()?.mode % 4096 == 0o600

  # It is the write and the chmod it replaces.
  let pair = fp"{root}/pair"
  pair.write("short\n")
  assert pair.exists()?
  pair.chmod(0o600)
  assert pair.metadata()?.mode % 4096 == file.metadata()?.mode % 4096
  assert pair.read_bytes()? == file.read_bytes()?

  # A write without a mode still leaves an existing file's bits alone.
  file.write("again\n")
  assert file.metadata()?.mode % 4096 == 0o600
}

test test_write_with_a_mode_out_of_range_writes_nothing { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-range")?
  let absent = fp"{root}/absent"
  for mode in [-1, 0o10000] {
    test.error_kind(absent.write("text", mode:), "fs-chmod")
    test.error_kind(absent.write("text", mode:), "fs-chmod")
  }

  assert ! absent.exists()?

  let kept = fp"{root}/kept"
  kept.write("kept\n")
  test.error_kind(kept.write("lost", mode: 0o10000), "fs-chmod")
  assert kept.read_text()? == "kept\n"

  # A failure to open is still a write failure.
  test.error_kind(fp"{root}/missing/file".write("text", mode: 0o600), "fs-write")
}

test test_fs_write_command_form_takes_no_mode { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-command")?
  let file = fp"{root}/command"
  fs.write $file "words"
  assert file.read_text()? == "words"

  let _ = test.expect(
    ctx,
    "fs.write p\"/tmp/xsh-write-mode-never\" \"words\" 384\n",
    status: 2,
    stderr: ["check.arity"],
  )?
}

test test_prefer_write_mode_lint_merges_a_write_and_its_chmod { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-lint")?
  let source = r"""proc install(key: Path, secret: Str) [fs, error] {
  key.write(secret)?
  key.chmod(0o640)?
  fs.write(fp"{key}.pub", "public\n")?
  fs.chmod(fp"{key}.pub", 0o604)?
  print (fs.metadata(key)?.mode % 4096) (fs.metadata(fp"{key}.pub")?.mode % 4096) (key.read_text()?)
}

install(p"ROOT/host.key", "secret")?
""".replace("ROOT", with: root.display())
  let candidate = test.temp_file(ctx, name: "write-mode.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.prefer-write-mode --fix $candidate
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  let merged = source.replace(
    "  key.write(secret)?\n  key.chmod(0o640)?\n",
    with: "  key.write(secret, mode: 0o640)?\n",
  )
    .replace(
      r"""  fs.write(fp"{key}.pub", "public\n")?
  fs.chmod(fp"{key}.pub", 0o604)?
""",
      with: r"""  fs.write(fp"{key}.pub", "public\n", mode: 0o604)?
""",
    )
  assert fixed == merged
  assert fixed != source

  let before = test.run_script(ctx, source)?
  let after = test.expect(ctx, fixed, status: 0)?
  assert before.stdout == f"{0o640} {0o604} secret\n"
  assert after.stdout == before.stdout

  let repeated = run.capture --text "xsht" lint --only lint.prefer-write-mode $candidate
  assert repeated.status.exited_with(0)
  assert "lint.prefer-write-mode" not in repeated.stderr
}

test test_write_with_a_mode_keeps_setuid_setgid_and_sticky_bits { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-special")?

  # A write by an unprivileged process clears a file's set-user-ID and
  # set-group-ID bits, so these modes are only exact if they outlive the data.
  for mode in [0o4755, 0o2755, 0o6711, 0o1644] {
    # The write and the chmod it replaces, kept apart so the lint leaves them.
    let pair = fp"{root}/pair-{mode}"
    pair.write("data\n")
    assert pair.exists()?
    pair.chmod(mode)
    let kept = pair.metadata()?.mode % 4096
    if kept != mode {
      test.skip(f"the filesystem under {root} stores mode {mode} as {kept}")
    }

    let created = fp"{root}/new-{mode}"
    created.write("data\n", mode:)
    assert created.metadata()?.mode % 4096 == mode
    assert created.read_text()? == "data\n"

    let existing = fp"{root}/existing-{mode}"
    existing.write("a longer first version\n", mode: 0o600)
    existing.write(b"data\n", mode:)
    assert existing.metadata()?.mode % 4096 == mode
    assert existing.read_bytes()? == pair.read_bytes()?

    # Writing again over bits that are already set keeps them too.
    existing.write("again\n", mode:)
    assert existing.metadata()?.mode % 4096 == mode
  }
}
