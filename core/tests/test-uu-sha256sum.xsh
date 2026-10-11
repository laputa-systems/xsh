##! Transcribed SHA256 checksum tests from the uutils coreutils suite.

use support.uu as uu

# origin: uutils test_sha256sum::sha256::test_single_file
test test_uu_sha256sum_sha256_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha256sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha256sum", "sha256.expected", "sha256.expected")?
  let r = uu.invoke(s, "sha256sum", ["input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "sha256.expected")?
}

# origin: uutils test_sha256sum::sha256::test_stdin
test test_uu_sha256sum_sha256_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha256sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha256sum", "sha256.expected", "sha256.expected")?
  let r = uu.invoke(s, "sha256sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "sha256.expected")?
}

# origin: uutils test_sha256sum::sha256::test_stdin_with_dash_directory
test test_uu_sha256sum_sha256_stdin_with_dash_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha256sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha256sum", "sha256.expected", "sha256.expected")?
  uu.mkdir(s, "-")?
  let r = uu.invoke(s, "sha256sum", [], stdin: uu.read(s, "input.txt")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "sha256.expected")?
}

# origin: uutils test_sha256sum::sha256::test_zero
test test_uu_sha256sum_sha256_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha256sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha256sum", "sha256.expected", "sha256.expected")?
  let r = uu.invoke(s, "sha256sum", ["--zero", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "sha256.expected")?
}

# origin: uutils test_sha256sum::sha256::test_check
test test_uu_sha256sum_sha256_check { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha256sum", "input.txt", "input.txt")?
  uu.fixture(s, "sha256sum", "sha256.checkfile", "sha256.checkfile")?
  let r = uu.invoke(s, "sha256sum", ["--check", "sha256.checkfile"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "input.txt: OK\n")
}

# origin: uutils test_sha256sum::sha256::test_missing_file
test test_uu_sha256sum_sha256_missing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "file1\n")?
  uu.write(s, "c", "file3\n")?
  let r = uu.invoke(s, "sha256sum", ["a", "b", "c"])?
  uu.fails(r)
  uu.stdout_contains(r, "a\n")
  uu.stdout_contains(r, "c\n")
  uu.stderr_contains(r, "b: No such file or directory")
}

# origin: uutils test_sha256sum::test_invalid_arg
test test_uu_sha256sum_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sha256sum", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_sha256sum::test_conflicting_arg
test test_uu_sha256sum_conflicting_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sha256sum", ["--tag", "--check"])?
  uu.fails_with_code(r, 1)
  let text_mode = uu.invoke(s, "sha256sum", ["--tag", "--text"])?
  uu.fails_with_code(text_mode, 1)
}

# origin: uutils test_sha256sum::test_tag
test test_uu_sha256sum_tag { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foobar", "foo bar\n")?
  let r = uu.invoke(s, "sha256sum", ["--tag", "foobar"])?
  uu.succeeds(r)
  uu.stdout_is(r, "SHA256 (foobar) = 1f2ec52b774368781bed1d1fb140a92e0eb6348090619c9291f9a5a3c8e8d151\n")
}

# origin: uutils test_sha256sum::test_sha256_binary
test test_uu_sha256sum_sha256_binary { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha256sum", "binary.png", "binary.png")?
  uu.fixture(s, "sha256sum", "binary.sha256.expected", "binary.sha256.expected")?
  let r = uu.invoke(s, "sha256sum", ["binary.png"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "binary.sha256.expected")?
}

# origin: uutils test_sha256sum::test_sha256_stdin_binary
test test_uu_sha256sum_sha256_stdin_binary { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha256sum", "binary.png", "binary.png")?
  uu.fixture(s, "sha256sum", "binary.sha256.expected", "binary.sha256.expected")?
  let r = uu.invoke(s, "sha256sum", [], stdin: uu.read(s, "binary.png")?)?
  uu.succeeds(r)
  uu.no_stderr(r)
  let digest = r.stdout.utf8()?.split(" ")[0]
  assert digest == uu.read_text(s, "binary.sha256.expected")?
}

# origin: uutils test_sha256sum::test_check_sha256_binary
test test_uu_sha256sum_check_sha256_binary { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "sha256sum", "binary.png", "binary.png")?
  uu.fixture(s, "sha256sum", "binary.sha256.checkfile", "binary.sha256.checkfile")?
  let r = uu.invoke(s, "sha256sum", ["--check", "binary.sha256.checkfile"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "binary.png: OK\n")
}

# origin: uutils test_sha256sum::test_check_binary_files_with_crlf_bytes
test test_uu_sha256sum_check_binary_files_with_crlf_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a.fake-ttf", b"\x00\x00\r\n\x00\x00")?
  uu.write_bytes(s, "b.fake-heic", b"HEIC\r\n\xff\x00\r\n*")?
  let untagged = uu.invoke(s, "sha256sum", ["a.fake-ttf", "b.fake-heic"])?
  uu.succeeds(untagged)
  uu.stdout_is(untagged, "1c671d7322d49cd2726475f4b8a8b50f27b454789e23a31c6ac14014740d8e58  a.fake-ttf\na54c776c4b43597b7f043ff59cd0a36753764eb6edec3c00a99dc54adcfbccbc  b.fake-heic\n")
  uu.write_bytes(s, "hash.256", untagged.stdout)?
  let tagged = uu.invoke(s, "sha256sum", ["--tag", "a.fake-ttf", "b.fake-heic"])?
  uu.succeeds(tagged)
  uu.write_bytes(s, "hash-tag.256", tagged.stdout)?
  for checksum_file in ["hash.256", "hash-tag.256"] {
    let checked = uu.invoke(s, "sha256sum", ["--check", checksum_file])?
    uu.succeeds(checked)
    uu.stdout_only(checked, "a.fake-ttf: OK\nb.fake-heic: OK\n")
  }
}
