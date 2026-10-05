test test_write_with_a_mode_creates_the_file_with_exactly_those_bits { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-new")?

  # Wider than a default umask leaves a plain write, and narrower: the bits
  # are the mode, not the mode under the umask.
  for mode in [0o600, 0o640, 0o755, 0o777, 0o000] {
    let by_method = fp"{root}/method-{mode}"
    by_method.write("text\n", mode:)
    assert fs.metadata(by_method)?.mode % 4096 == mode

    let by_module = fp"{root}/module-{mode}"
    fs.write(by_module, b"bytes\n", mode:)
    assert fs.metadata(by_module)?.mode % 4096 == mode
  }

  assert fp"{root}/method-{0o640}".read_text()? == "text\n"
  assert fp"{root}/module-{0o755}".read_bytes()? == b"bytes\n"

  # The mode may be positional, and each data type has it.
  let positional = fp"{root}/positional"
  positional.write(b"one", 0o604)
  assert fs.metadata(positional)?.mode % 4096 == 0o604
  fs.write(positional, "two", 0o640)
  assert fs.metadata(positional)?.mode % 4096 == 0o640
  assert positional.read_text()? == "two"
}

test test_write_with_a_mode_sets_the_bits_of_an_existing_file { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-existing")?
  let file = fp"{root}/existing"
  file.write("a longer first version\n", mode: 0o666)

  file.write("short\n", mode: 0o600)
  assert file.read_text()? == "short\n"
  assert fs.metadata(file)?.mode % 4096 == 0o600

  # It is the write and the chmod it replaces.
  let pair = fp"{root}/pair"
  pair.write("short\n")
  assert pair.exists()?
  pair.chmod(0o600)
  assert fs.metadata(pair)?.mode % 4096 == fs.metadata(file)?.mode % 4096
  assert pair.read_bytes()? == file.read_bytes()?

  # A write without a mode still leaves an existing file's bits alone.
  file.write("again\n")
  assert fs.metadata(file)?.mode % 4096 == 0o600
}

test test_write_with_a_mode_out_of_range_writes_nothing { |ctx|
  let root = test.temp_dir(ctx, name: "write-mode-range")?
  let absent = fp"{root}/absent"
  for mode in [-1, 0o10000] {
    test.error_kind(absent.write("text", mode:), "fs-chmod")
    test.error_kind(fs.write(absent, "text", mode:), "fs-chmod")
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

  let rejected = test.run_script(ctx, "fs.write p\"/tmp/xsh-write-mode-never\" \"words\" 384\n")?
  assert rejected.status == 2, rejected.stderr
  assert "check.arity" in rejected.stderr, rejected.stderr
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
""".replace("ROOT", root.display())
  let candidate = test.temp_file(ctx, name: "write-mode.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.prefer-write-mode --fix $candidate ?
  assert applied.status.exited_with(0), applied.stderr
  let fixed = candidate.read_text()?
  let merged = source.replace("  key.write(secret)?\n  key.chmod(0o640)?\n", "  key.write(secret, mode: 0o640)?\n")
    .replace(
      r"""  fs.write(fp"{key}.pub", "public\n")?
  fs.chmod(fp"{key}.pub", 0o604)?
""",
      r"""  fs.write(fp"{key}.pub", "public\n", mode: 0o604)?
""",
    )
  assert fixed == merged
  assert fixed != source

  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  assert before.stdout == f"{0o640} {0o604} secret\n"
  assert after.stdout == before.stdout

  let repeated = run.capture --text "xsht" lint --only lint.prefer-write-mode $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.prefer-write-mode" not in repeated.stderr
}
