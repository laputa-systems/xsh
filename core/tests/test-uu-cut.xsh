##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_cut.rs.

use support.uu as uu

# origin: uutils test_cut::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_cut_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f", "1,4-2", "/dev/null"])?
    uu.fails_with_code(r, 1)
    assert r.stderr.utf8()?.starts_with("cut: invalid decreasing range\n")
    assert !("╭" in r.stderr.utf8()?)
  }
}

# origin: uutils test_cut::test_8bit_non_utf8_delimiter
test test_uu_cut_8bit_non_utf8_delimiter { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cut", "8bit-delim.txt", "8bit-delim.txt")?
  {
    let r = uu.invoke_paths(s, "cut", [Path("-d"), Path.parse_bytes(b"\xad")?, Path("--out=_"), Path("-f2,3"), Path("8bit-delim.txt")])?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, b"b_c\n")
  }
}

# origin: uutils test_cut::test_byte_no_split_partially_selected_char
test test_uu_cut_byte_no_split_partially_selected_char { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-b1-2,4-5", "-n"] , stdin: b"\xf0\x9f\x97\xbfw\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"w\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-b2,4", "-n"] , stdin: b"\xf0\x9f\x97\xbfw\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-b3-", "-n"] , stdin: b"\xf0\x9f\x97\xbfw\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\xf0\x9f\x97\xbfw\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-b1-3,4-5", "-n", "--output-d=|"] , stdin: b"\xf0\x9f\x97\xbfw\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\xf0\x9f\x97\xbfw\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-b1,2,5-7", "-n", "--output-d=|"] , stdin: b"q\xe2\x82\xac\xc3\xa9r\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"q|\xc3\xa9r\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-b1,2-4,5-6", "-n", "--output-d=|"] , stdin: b"q\xe2\x82\xac\xc3\xa9r\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"q|\xe2\x82\xac|\xc3\xa9\n")
  }
}

# origin: uutils test_cut::test_byte_no_split_with_output_delimiter
test test_uu_cut_byte_no_split_with_output_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-b1,3", "-n", "--output-d=|"] , stdin: b"\xc3\xbcZ\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"Z\n")
  }
}

# origin: uutils test_cut::test_byte_sequence
test test_uu_cut_byte_sequence { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cut", "lists.txt", "lists.txt")?
  {
    let r = uu.invoke(s, "cut", ["-b", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-b", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-b", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-b", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-b", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-b", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_subsumed.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--bytes", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--bytes", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--bytes", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--bytes", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--bytes", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--bytes", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_subsumed.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--byt", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--byt", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--byt", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--byt", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--byt", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--byt", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_subsumed.expected".read_bytes()?)
  }
}

# origin: uutils test_cut::test_char_sequence
test test_uu_cut_char_sequence { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cut", "lists.txt", "lists.txt")?
  {
    let r = uu.invoke(s, "cut", ["-c", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_subsumed.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--characters", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--characters", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--characters", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--characters", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--characters", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--characters", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_subsumed.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--char", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--char", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--char", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--char", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--char", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--char", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/byte_subsumed.expected".read_bytes()?)
  }
}

# origin: uutils test_cut::test_complement
test test_uu_cut_complement { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d_", "--complement", "-f", "2"] , stdin: b"9_1\n8_2\n7_3")?
    uu.succeeds(r)
    uu.stdout_only(r, "9\n8\n7\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d_", "--com", "-f", "2"] , stdin: b"9_1\n8_2\n7_3")?
    uu.succeeds(r)
    uu.stdout_only(r, "9\n8\n7\n")
  }
}

# origin: uutils test_cut::test_cut_bytes_no_split_gb18030
test test_uu_cut_cut_bytes_no_split_gb18030 { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-b1", "-n"] , stdin: b"\xb0\xa1w\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only(r, "\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-b2", "-n"] , stdin: b"\xb0\xa1w\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\xb0\xa1\n")
  }
}

# origin: uutils test_cut::test_cut_chars_gb18030
test test_uu_cut_cut_chars_gb18030 { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c2"] , stdin: b"\xb0\xa1w\xd6\xd0\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only(r, "w\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-c3"] , stdin: b"\xb0\xa1w\xd6\xd0\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\xd6\xd0\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-c1-2"] , stdin: b"\xb0\xa1w\xd6\xd0\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\xb0\xa1w\n")
  }
}

# origin: uutils test_cut::test_cut_chars_gb18030_ranges_and_complement
test test_uu_cut_cut_chars_gb18030_ranges_and_complement { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c1,3", "--output-delimiter=+"] , stdin: b"\xb0\xa1w\xd6\xd0\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\xb0\xa1+\xd6\xd0\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--complement", "-c2"] , stdin: b"\xb0\xa1w\xd6\xd0\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\xb0\xa1\xd6\xd0\n")
  }
}

# origin: uutils test_cut::test_cut_chars_utf8
test test_uu_cut_cut_chars_utf8 { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c3"] , stdin: b"na\xc3\xafve\n", vars: {LC_ALL: "en_US.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only(r, "ï\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--complement", "-c3"] , stdin: b"na\xc3\xafve\n", vars: {LC_ALL: "en_US.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only(r, "nave\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-c1,4", "--output-delimiter=+"] , stdin: b"na\xc3\xafve\n", vars: {LC_ALL: "en_US.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only(r, "n+v\n")
  }
}

# origin: uutils test_cut::test_cut_chars_utf8_mixed_ascii_lines
test test_uu_cut_cut_chars_utf8_mixed_ascii_lines { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c3-5"] , stdin: b"quokka\nfj\xc3\xa4rd\nwombat\nt\xc3\xb8ys\n", vars: {LC_ALL: "en_US.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only(r, "okk\närd\nmba\nys\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-b", "3-5", "-n"] , stdin: b"quokka\nfj\xc3\xa4rd\nwombat\nt\xc3\xb8ys\n", vars: {LC_ALL: "en_US.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only(r, "okk\när\nmba\nøys\n")
  }
}

# origin: uutils test_cut::test_cut_fields_gb18030_complement_and_gaps
test test_uu_cut_cut_fields_gb18030_complement_and_gaps { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke_paths(s, "cut", [Path("--complement"), Path("-d"), Path.parse_bytes(b"\xb0\xa1")?, Path("-f2")] , stdin: b"red\xb0\xa1green\xb0\xa1blue\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"red\xb0\xa1blue\n")
  }
  {
    let r = uu.invoke_paths(s, "cut", [Path("-d"), Path.parse_bytes(b"\xb0\xa1")?, Path("-f1-3"), Path("--output-delimiter=|")] , stdin: b"\xb0\xa1\xb0\xa1z\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only(r, "||z\n")
  }
}

# origin: uutils test_cut::test_cut_fields_gb18030_delimiter
test test_uu_cut_cut_fields_gb18030_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke_paths(s, "cut", [Path("-d"), Path.parse_bytes(b"\xb0\xa1")?, Path("-f3"), Path("--output-delimiter=/")] , stdin: b"red\xb0\xa1green\xb0\xa1blue\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only(r, "blue\n")
  }
  {
    let r = uu.invoke_paths(s, "cut", [Path("-d"), Path.parse_bytes(b"\xb0\xa1")?, Path("-f1,3")] , stdin: b"red\xb0\xa1green\xb0\xa1blue\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"red\xb0\xa1blue\n")
  }
}

# origin: uutils test_cut::test_cut_fields_single_byte_delimiter_in_mb_locale
test test_uu_cut_cut_fields_single_byte_delimiter_in_mb_locale { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke_paths(s, "cut", [Path("-d"), Path.parse_bytes(b"\x80")?, Path("-f1,3"), Path("--output-delimiter=-")] , stdin: b"a\x80b\x80c\n", vars: {LC_ALL: "zh_CN.gb18030"})?
    uu.succeeds(r)
    uu.stdout_only(r, "a-c\n")
  }
}

# origin: uutils test_cut::test_cut_non_utf8_paths
test test_uu_cut_cut_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let file = uu.at_bytes(s, b"test_\xff\xfe.txt")?
  file.write(b"a\tb\tc\n1\t2\t3\n")?
  {
    let r = uu.invoke_paths(s, "cut", [Path("-f1,3"), Path.parse_bytes(b"test_\xff\xfe.txt")?])?
    uu.succeeds(r)
    uu.stdout_only(r, "a\tc\n1\t3\n")
  }
}

# origin: uutils test_cut::test_delimiter
test test_uu_cut_delimiter { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cut", "lists.txt", "lists.txt")?
  {
    let r = uu.invoke(s, "cut", ["-d", ":", "-f", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/delimiter_specified.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--delimiter", ":", "-f", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/delimiter_specified.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--del", ":", "-f", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/delimiter_specified.expected".read_bytes()?)
  }
}

# origin: uutils test_cut::test_delimiter_and_whitespace_are_exclusive
test test_uu_cut_delimiter_and_whitespace_are_exclusive { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-w", "-d,", "-f3"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: -d and -w are mutually exclusive")
  }
}

# origin: uutils test_cut::test_delimiter_multibyte_rejected_in_c_locale
test test_uu_cut_delimiter_multibyte_rejected_in_c_locale { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "€", "-f1"] , vars: {LC_ALL: "C"})?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: the delimiter must be a single character")
  }
}

# origin: uutils test_cut::test_delimiter_with_byte_and_char
test test_uu_cut_delimiter_with_byte_and_char { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c", "9-,6-7,-2,4", "-d="])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: an input delimiter makes sense\n\tonly when operating on fields")
  }
  {
    let r = uu.invoke(s, "cut", ["-b", "9-,6-7,-2,4", "-d="])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: an input delimiter makes sense\n\tonly when operating on fields")
  }
}

# origin: uutils test_cut::test_delimiter_with_more_than_one_char
test test_uu_cut_delimiter_with_more_than_one_char { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "ab", "-f1"])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: the delimiter must be a single character")
    uu.no_stdout(r)
  }
}

# origin: uutils test_cut::test_emoji_delim
test test_uu_cut_emoji_delim { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d🗿", "-f1"] , stdin: b"\xf0\x9f\x92\x90\xf0\x9f\x97\xbf\xf0\x9f\x8c\xb9\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only(r, "💐\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d🗿", "-f2"] , stdin: b"\xf0\x9f\x92\x90\xf0\x9f\x97\xbf\xf0\x9f\x8c\xb9\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only(r, "🌹\n")
  }
}

# origin: uutils test_cut::test_empty_string_as_delimiter
test test_uu_cut_empty_string_as_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f2", "--delimiter="] , stdin: b"a\x00b\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "b\n")
  }
}

# origin: uutils test_cut::test_empty_string_as_delimiter_with_output_delimiter
test test_uu_cut_empty_string_as_delimiter_with_output_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f", "1,2", "--delimiter=", "--output-delimiter=Z"] , stdin: b"ab\x00cd\n")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"abZcd\n")
  }
}

# origin: uutils test_cut::test_equal_as_delimiter
test test_uu_cut_equal_as_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f2", "-d="] , stdin: b"--dir=./out/lib")?
    uu.succeeds(r)
    uu.stdout_only(r, "./out/lib\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-f2", "--delimiter=="] , stdin: b"--dir=./out/lib")?
    uu.succeeds(r)
    uu.stdout_only(r, "./out/lib\n")
  }
}

# origin: uutils test_cut::test_failed_write_is_reported
test test_uu_cut_failed_write_is_reported { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d=", "-f1"] , stdin: b"key=value", stdout: /dev/full)?
    uu.fails(r)
    uu.stderr_is(r, "cut: write error: No space left on device\n")
  }
}

# origin: uutils test_cut::test_field_delimiter_not_split_inside_multibyte_char
test test_uu_cut_field_delimiter_not_split_inside_multibyte_char { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke_paths(s, "cut", [Path("-d"), Path.parse_bytes(b"\xac")?, Path("-f2")] , stdin: b"1\xe2\x82\xac2\xac3\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"3\n")
  }
  {
    let r = uu.invoke_paths(s, "cut", [Path("-d"), Path.parse_bytes(b"\xac")?, Path("-f1")] , stdin: b"1\xe2\x82\xac2\xac3\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"1\xe2\x82\xac2\n")
  }
}

# origin: uutils test_cut::test_field_only_options_without_fields
test test_uu_cut_field_only_options_without_fields { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-s", "-c7"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: suppressing non-delimited lines makes sense\n\tonly when operating on fields")
  }
}

# origin: uutils test_cut::test_field_sequence
test test_uu_cut_field_sequence { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cut", "lists.txt", "lists.txt")?
  {
    let r = uu.invoke(s, "cut", ["-f", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_subsumed.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fields", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fields", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fields", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fields", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fields", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fields", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_subsumed.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fie", "2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_singular.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fie", "-2", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_prefix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fie", "2-", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_suffix.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fie", "2-4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_range.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fie", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_aggregate.expected".read_bytes()?)
  }
  {
    let r = uu.invoke(s, "cut", ["--fie", "2-,3", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/sequences/field_subsumed.expected".read_bytes()?)
  }
}

# origin: uutils test_cut::test_fields_merged
test test_uu_cut_fields_merged { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-F", "1,3"] , stdin: b"one\ttwo   three\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "one three\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-F", "1,3", "-O", "+"] , stdin: b"one\ttwo   three\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "one+three\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-F", "2,4", "-d", ";"] , stdin: b"p;q;r;s\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "q s\n")
  }
}

# origin: uutils test_cut::test_fields_merged_conflicts_with_fields
test test_uu_cut_fields_merged_conflicts_with_fields { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f", "3", "-F", "5"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: only one list may be specified")
  }
}

# origin: uutils test_cut::test_invalid_arg
test test_uu_cut_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["--definitely-invalid"])?
    uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_cut::test_is_a_directory
test test_uu_cut_is_a_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some")?
  {
    let r = uu.invoke(s, "cut", ["-b1", "some"])?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, "cut: some: Is a directory\n")
  }
}

# origin: uutils test_cut::test_multiple_delimiters
test test_uu_cut_multiple_delimiters { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f2", "-d:", "-d="] , stdin: b"a:=b\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "b\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-f2", "-d=", "-d:"] , stdin: b"a:=b\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "=b\n")
  }
}

# origin: uutils test_cut::test_multiple_mode_args
test test_uu_cut_multiple_mode_args { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-b1", "-b2"])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: only one list may be specified")
  }
  {
    let r = uu.invoke(s, "cut", ["-c1", "-c2"])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: only one list may be specified")
  }
  {
    let r = uu.invoke(s, "cut", ["-f1", "-f2"])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: only one list may be specified")
  }
  {
    let r = uu.invoke(s, "cut", ["-b1", "-c2"])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: only one list may be specified")
  }
  {
    let r = uu.invoke(s, "cut", ["-b1", "-f2"])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: only one list may be specified")
  }
  {
    let r = uu.invoke(s, "cut", ["-c1", "-f2"])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: only one list may be specified")
  }
  {
    let r = uu.invoke(s, "cut", ["-b1", "-c2", "-f3"])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: only one list may be specified")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter
test test_uu_cut_newline_as_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f", "1", "-d", "\n"] , stdin: b"a:1\nb:")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"a:1\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "2", "-d", "\n"] , stdin: b"a:1\nb:")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"b:\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_complement
test test_uu_cut_newline_as_delimiter_complement { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "2", "--complement"] , stdin: b"line1\nline2\nline3\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "line1\nline3\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_complement_last_record
test test_uu_cut_newline_as_delimiter_complement_last_record { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "1", "--complement"] , stdin: b"a\nb")?
    uu.succeeds(r)
    uu.stdout_only(r, "b\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_double_newline
test test_uu_cut_newline_as_delimiter_double_newline { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "2"] , stdin: b"abc\n\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "1,2"] , stdin: b"abc\n\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "abc\n\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_empty_first_record
test test_uu_cut_newline_as_delimiter_empty_first_record { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "2"] , stdin: b"\nb")?
    uu.succeeds(r)
    uu.stdout_only(r, "b\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_empty_input
test test_uu_cut_newline_as_delimiter_empty_input { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "1"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_found_not_suppressed
test test_uu_cut_newline_as_delimiter_found_not_suppressed { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "1"] , stdin: b"abc\ndef\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "abc\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "--only-delimited", "-f", "1"] , stdin: b"abc\ndef\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "abc\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "--only-del", "-f", "1"] , stdin: b"abc\ndef\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "abc\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_intervening_skipped_fields
test test_uu_cut_newline_as_delimiter_intervening_skipped_fields { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "1,3"] , stdin: b"line1\nline2\nline3\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "line1\nline3\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_last_field_no_newline
test test_uu_cut_newline_as_delimiter_last_field_no_newline { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "2"] , stdin: b"abc\ndef")?
    uu.succeeds(r)
    uu.stdout_only(r, "def\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_multibyte_normalization
test test_uu_cut_newline_as_delimiter_multibyte_normalization { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "2"] , stdin: b"\n\xf0\x9f\x98\xbc")?
    uu.succeeds(r)
    uu.stdout_only(r, "😼\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_multiple_fields
test test_uu_cut_newline_as_delimiter_multiple_fields { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "2"] , stdin: b"abc\ndef\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "def\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_no_delimiter_prints_all
test test_uu_cut_newline_as_delimiter_no_delimiter_prints_all { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "2"] , stdin: b"a")?
    uu.succeeds(r)
    uu.stdout_only(r, "a\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_no_delimiter_suppressed
test test_uu_cut_newline_as_delimiter_no_delimiter_suppressed { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "1"] , stdin: b"abc")?
    uu.succeeds(r)
    uu.no_output(r)
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "--only-delimited", "-f", "1"] , stdin: b"abc")?
    uu.succeeds(r)
    uu.no_output(r)
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "--only-del", "-f", "1"] , stdin: b"abc")?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_only_newlines
test test_uu_cut_newline_as_delimiter_only_newlines { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "1"] , stdin: b"\n\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "2"] , stdin: b"\n\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "1,2"] , stdin: b"\n\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "\n\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_out_of_bounds
test test_uu_cut_newline_as_delimiter_out_of_bounds { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "3"] , stdin: b"a\nb\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "1,3"] , stdin: b"a\nb\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_overlapping_unordered_ranges
test test_uu_cut_newline_as_delimiter_overlapping_unordered_ranges { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-f", "2-3,1,2"] , stdin: b"a\nb\nc\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a\nb\nc\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_s_flag_no_newline_at_all
test test_uu_cut_newline_as_delimiter_s_flag_no_newline_at_all { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "1"] , stdin: b"abc")?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_single_field_included
test test_uu_cut_newline_as_delimiter_single_field_included { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "-s", "-f", "1"] , stdin: b"abc\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "abc\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "--only-delimited", "-f", "1"] , stdin: b"abc\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "abc\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d", "\n", "--only-del", "-f", "1"] , stdin: b"abc\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "abc\n")
  }
}

# origin: uutils test_cut::test_newline_as_delimiter_with_output_delimiter
test test_uu_cut_newline_as_delimiter_with_output_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f1-", "-d", "\n", "--output-delimiter=:"] , stdin: b"a\nb\n")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"a:b\n")
  }
}

# origin: uutils test_cut::test_newline_delim_suppress_missing_field
test test_uu_cut_newline_delim_suppress_missing_field { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-s", "-d", "\n", "-f3"] , stdin: b"solo\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "")
  }
}

# origin: uutils test_cut::test_newline_preservation_with_f1_option
test test_uu_cut_newline_preservation_with_f1_option { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "1", "a\nb")?
  {
    let r = uu.invoke(s, "cut", ["-f1-", "1"])?
    uu.succeeds(r)
    uu.stdout_is(r, "a\nb\n")
  }
}

# origin: uutils test_cut::test_no_args
test test_uu_cut_no_args { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", [])?
    uu.fails(r)
    uu.stderr_contains(r, "cut: you must specify a list of bytes, characters, or fields")
  }
}

# origin: uutils test_cut::test_no_such_file
test test_uu_cut_no_such_file { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-b1", "some"])?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, "cut: some: No such file or directory\n")
  }
}

# origin: uutils test_cut::test_only_delimited
test test_uu_cut_only_delimited { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d_", "-s", "-f", "1"] , stdin: b"91\n82\n7_3")?
    uu.succeeds(r)
    uu.stdout_only(r, "7\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d_", "--only-delimited", "-f", "1"] , stdin: b"91\n82\n7_3")?
    uu.succeeds(r)
    uu.stdout_only(r, "7\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-d_", "--only-del", "-f", "1"] , stdin: b"91\n82\n7_3")?
    uu.succeeds(r)
    uu.stdout_only(r, "7\n")
  }
}

# origin: uutils test_cut::test_output_delimiter
test test_uu_cut_output_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["--output-delimiter=@", "-f1,2"] , stdin: b"a:\tb:\tc:\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a:@b:\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--output-delimiter=@", "-f1,2", "-d:"] , stdin: b"a:\tb:\tc\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a@\tb\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--output-delimiter=@", "-f1,2"] , stdin: b"a:b:c\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a:b:c\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--output-del=@", "-f1,2"] , stdin: b"a:\tb:\tc:\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a:@b:\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--output-del=@", "-f1,2", "-d:"] , stdin: b"a:\tb:\tc\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a@\tb\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--output-del=@", "-f1,2"] , stdin: b"a:b:c\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a:b:c\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-O@", "-f1,2"] , stdin: b"a:\tb:\tc:\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a:@b:\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-O@", "-f1,2", "-d:"] , stdin: b"a:\tb:\tc\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a@\tb\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-O@", "-f1,2"] , stdin: b"a:b:c\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "a:b:c\n")
  }
}

# origin: uutils test_cut::test_output_delimiter_with_adjacent_ranges
test test_uu_cut_output_delimiter_with_adjacent_ranges { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-b1-2,3-4", "--output-d=:"] , stdin: b"abcd\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "ab:cd\n")
  }
}

# origin: uutils test_cut::test_output_delimiter_with_character_ranges
test test_uu_cut_output_delimiter_with_character_ranges { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c2-3,4-", "--output-delim=:"] , stdin: b"abcdefg\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "bc:defg\n")
  }
}

# origin: uutils test_cut::test_range_error_messages
test test_uu_cut_range_error_messages { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c0"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: byte/character positions are numbered from 1")
  }
  {
    let r = uu.invoke(s, "cut", ["-b0-7"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: byte/character positions are numbered from 1")
  }
  {
    let r = uu.invoke(s, "cut", ["-f0-9"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: fields are numbered from 1")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", ""])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: fields are numbered from 1")
  }
  {
    let r = uu.invoke(s, "cut", ["-c", ""])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: byte/character positions are numbered from 1")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "q"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid field value 'q'")
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "zz"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid byte/character position 'zz'")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "9-4"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid decreasing range")
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "-"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid range with no endpoint: -")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "8,-"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid range with no endpoint: -")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "7k"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid field value 'k'")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "4-w2"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid field value 'w2'")
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "3x-5"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid byte/character position 'x-5'")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "+6"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid field value '+6'")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "2-5-8"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid field range")
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "3--6"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: invalid byte or character range")
  }
  {
    let r = uu.invoke(s, "cut", ["-f", "18446744073709551615"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: field number '18446744073709551615' is too large")
  }
  {
    let r = uu.invoke(s, "cut", ["-c", "4-77777777777777777777777"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: byte/character offset '77777777777777777777777' is too large")
  }
}

# origin: uutils test_cut::test_single_quote_pair_as_delimiter_is_invalid
test test_uu_cut_single_quote_pair_as_delimiter_is_invalid { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d", "''", "-f2"] , stdin: b"a''b\n")?
    uu.fails(r)
    uu.stderr_contains(r, "cut: the delimiter must be a single character")
    uu.no_stdout(r)
  }
  {
    let r = uu.invoke(s, "cut", ["--delimiter=''", "-f2"] , stdin: b"a''b\n")?
    uu.fails(r)
    uu.stderr_contains(r, "cut: the delimiter must be a single character")
    uu.no_stdout(r)
  }
}

# origin: uutils test_cut::test_single_quote_pair_as_output_delimiter_is_literal
test test_uu_cut_single_quote_pair_as_output_delimiter_is_literal { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-f", "1,2", "-d:", "--output-delimiter=''"] , stdin: b"ab:cd\n")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"ab''cd\n")
  }
}

# origin: uutils test_cut::test_suppresses_unterminated_segment
test test_uu_cut_suppresses_unterminated_segment { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-z", "-d", "", "-s", "-f", "1"] , stdin: b"unterminated")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"")
  }
  {
    let r = uu.invoke(s, "cut", ["-z", "-d", "", "-s", "-f", "1"] , stdin: b"terminated\x00unterminated")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"terminated\x00")
  }
}

# origin: uutils test_cut::test_too_large
test test_uu_cut_too_large { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-b1-18446744073709551615", "/dev/null"])?
    uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_cut::test_unset_locale_is_byte_oriented
test test_uu_cut_unset_locale_is_byte_oriented { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c3"] , stdin: b"p\xd0\xb6t\n", vars: {LC_ALL: ""})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\xb6\n")
  }
}

# origin: uutils test_cut::test_whitespace_delimited
test test_uu_cut_whitespace_delimited { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cut", "lists.txt", "lists.txt")?
  {
    let r = uu.invoke(s, "cut", ["-w", "-f", "9-,6-7,-2,4", "lists.txt"])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/cut/whitespace_delimited.expected".read_bytes()?)
  }
}

# origin: uutils test_cut::test_whitespace_delimited_long_and_trimmed
test test_uu_cut_whitespace_delimited_long_and_trimmed { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["--whitespace-delimited", "-f1,2"] , stdin: b"   alpha beta\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "\talpha\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--whitespace-delimited=trimmed", "-f1,2"] , stdin: b"  hello world  \n")?
    uu.succeeds(r)
    uu.stdout_only(r, "hello\tworld\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--whitespace-delimited=tri", "-f1,2"] , stdin: b"  hello world  \n")?
    uu.succeeds(r)
    uu.stdout_only(r, "hello\tworld\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--whitespace-delimited=", "-f1,2"] , stdin: b"  hello world  \n")?
    uu.succeeds(r)
    uu.stdout_only(r, "hello\tworld\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-s", "--whitespace-delimited=trimmed", "-f1"] , stdin: b"   solo   \n")?
    uu.succeeds(r)
    uu.stdout_only(r, "")
  }
  {
    let r = uu.invoke(s, "cut", ["--whitespace-delimited=trimmed", "-f4"] , stdin: b"  loner\n\t\n one two\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "loner\n\n\n")
  }
  {
    let r = uu.invoke(s, "cut", ["--whitespace-delimited=middle", "-f1"])?
    uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_cut::test_whitespace_delimited_trimmed_zero_terminated
test test_uu_cut_whitespace_delimited_trimmed_zero_terminated { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-z", "--whitespace-delimited=trimmed", "-f2"] , stdin: b"  red  blue  \x00 green  pink \x00")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"blue\x00pink\x00")
  }
  {
    let r = uu.invoke(s, "cut", ["-z", "--whitespace-delimited=trimmed", "-f3"] , stdin: b"  red  blue  \x00")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"\x00")
  }
  {
    let r = uu.invoke(s, "cut", ["-z", "-s", "--whitespace-delimited=trimmed", "-f1"] , stdin: b"\t \x00 amber violet \x00")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"amber\x00")
  }
}

# origin: uutils test_cut::test_whitespace_delimiter_unicode_blank
test test_uu_cut_whitespace_delimiter_unicode_blank { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-w", "-f2"] , stdin: b"x\xe2\x80\x82y\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"y\n")
  }
  {
    let r = uu.invoke(s, "cut", ["-s", "-w", "-f2"] , stdin: b"x\xe2\x80\x87y\n", vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"")
  }
}

# origin: uutils test_cut::test_whitespace_with_byte
test test_uu_cut_whitespace_with_byte { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-w", "-b", "9-,6-7,-2,4"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: an input delimiter makes sense\n\tonly when operating on fields")
  }
}

# origin: uutils test_cut::test_whitespace_with_char
test test_uu_cut_whitespace_with_char { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-c", "9-,6-7,-2,4", "-w"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cut: an input delimiter makes sense\n\tonly when operating on fields")
  }
}

# origin: uutils test_cut::test_whitespace_with_explicit_delimiter
test test_uu_cut_whitespace_with_explicit_delimiter { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-w", "-f", "9-,6-7,-2,4", "-d:"])?
    uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_cut::test_zero_terminated
test test_uu_cut_zero_terminated { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d_", "-z", "-f", "1"] , stdin: b"9_1\n8_2\n\x007_3")?
    uu.succeeds(r)
    uu.stdout_only(r, "9\x007\x00")
  }
}

# origin: uutils test_cut::test_zero_terminated_only_delimited
test test_uu_cut_zero_terminated_only_delimited { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "cut", ["-d_", "-z", "-s", "-f", "1"] , stdin: b"91\n\x0082\n7_3")?
    uu.succeeds(r)
    uu.stdout_only(r, "82\n7\x00")
  }
}
