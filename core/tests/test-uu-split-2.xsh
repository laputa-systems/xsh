##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_split.rs.

use support.uu as uu

proc split_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  for name in ["separator_nul.txt", "noeof.txt", "ninetyonebytes.txt", "fivelines.txt", "onehundredlines.txt", "sixhundredfiftyonebytes.txt", "threebytes.txt", "separator_semicolon.txt", "twohundredfortyonebytes.txt", "asciilowercase.txt", "letters.txt"] {
    uu.fixture(s, "split", name, name)?
  }
  Ok(s)
}

proc matching_outputs(s: uu.Scene, pattern: Str) [fs, error] -> Result[List[Path], Error] {
  let re = regex.compile(pattern)?
  Ok([entry for entry in s.root.glob("*")? if re.matches(entry.basename())])
}

proc collate_outputs(files: List[Path]) [fs, error] -> Result[Bytes, Error] {
  var output = b""
  for file in files { output = bytes.concat([output, file.read_bytes()?]) }
  Ok(output)
}

# A recorded entropy sample keeps each 32-character alphanumeric record identical in both runs.
proc random_line_input(s: uu.Scene, name: Str, count: Int) [fs, error] -> Result[Unit, Error] {
  let entropy = fp"{s.ctx.core_dir}/tests/data/uutils/split/seeded-123.bin".read_bytes()?
  assert entropy.len() == 16384
  let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
  var letters: List[Bytes] = []
  for index in range(entropy.len()) {
    guard let value = entropy.byte_at(index) else { return test.fail("recorded entropy index is missing") }
    letters += [bytes.from_text(alphabet.byte_slice(value % 62, length: 1))]
  }
  let alphanumerics = bytes.concat(letters)
  let lines = [bytes.concat([alphanumerics[line % 512 * 32..line % 512 * 32 + 32], b"\n"]) for line in range(count)]
  uu.write_bytes(s, name, bytes.concat(lines))?
  Ok()
}

proc random_byte_input(s: uu.Scene, name: Str, count: Int) [fs, error] -> Result[Unit, Error] {
  let recorded = fp"{s.ctx.core_dir}/tests/data/uutils/split/seeded-123.bin".read_bytes()?
  assert count <= recorded.len()
  uu.write_bytes(s, name, recorded[..count])?
  Ok()
}

# Preserve the upstream stdout pipe when the output file resolves through /dev/stdout.
proc split_to_stdout_pipe(s: uu.Scene, args: List[Str], input: Bytes) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let capture = test.temp_dir(s.ctx, name: "pipe-capture")?
  defer capture.remove()?
  let pipe = fp"{capture}/pipe"
  fs.mkfifo(pipe, 0o600)?
  let output = fp"{capture}/stdout"
  let errors = fp"{capture}/reader-errors"
  let cat = process.which("cat")?
  let plan = process.command_argv(cat, [cat, pipe], s.root, {}, b"", output, errors, timeout: 5s)
  let reader = spawn plan?
  defer reader.cancel(kill_after: 100ms)?
  let r = uu.invoke(s, "split", args, stdin: input, stdout: pipe, timeout: 5s)?
  assert (wait reader?).exited_with(0)
  assert errors.read_bytes()? == b""
  Ok({...r, stdout: output.read_bytes()?})
}

proc byte_named_outputs(s: uu.Scene, prefix: Bytes, suffix: Bytes = b"") [fs, error] -> Result[List[Bytes], Error] {
  let offset = s.root.bytes().len() + 1
  let entries = s.root.glob("*")? |> sort
  let names = [entry.bytes()[offset..] for entry in entries]
  Ok([name for name in names if name.starts_with(prefix) and name.ends_with(suffix)])
}

# origin: uutils test_split::test_split_non_existing_file
test test_uu_split_split_non_existing_file { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["non-existing"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_is(r0, "split: cannot open 'non-existing' for reading: No such file or directory\n")
}

# origin: uutils test_split::test_split_number_chunks_short_concatenated_with_value
test test_uu_split_split_number_chunks_short_concatenated_with_value { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["-n3", "threebytes.txt"])?
  uu.succeeds(r0)
  uu.no_output(r0)
  uu.file_is(s, "xaa", "a")
  uu.file_is(s, "xab", "b")
  uu.file_is(s, "xac", "c")
}

# origin: uutils test_split::test_split_number_with_io_blksize
test test_uu_split_split_number_with_io_blksize { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["-n", "5", "asciilowercase.txt", "---io-blksize", "1024"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "abcdef")
  uu.file_is(s, "xab", "ghijkl")
  uu.file_is(s, "xac", "mnopq")
  uu.file_is(s, "xad", "rstuv")
  uu.file_is(s, "xae", "wxyz\n")
}

# origin: uutils test_split::test_split_obs_lines_as_other_option_value
test test_uu_split_split_obs_lines_as_other_option_value { |ctx|
  let s = split_scene(ctx)?
  uu.touch(s, "file")?
  let r0 = uu.invoke(s, "split", ["--lines", "-200", "file"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_is(r0, "split: invalid number of lines: '-200'\n")
  let r1 = uu.invoke(s, "split", ["-l", "-200", "file"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "split: invalid number of lines: '-200'\n")
  let r2 = uu.invoke(s, "split", ["-a", "-200", "file"])?
  uu.fails_with_code(r2, 1)
  uu.stderr_is(r2, "split: invalid suffix length: '-200': Value too large for defined data type\n")
  let r3 = uu.invoke(s, "split", ["--suffix-length", "-d200e", "file"])?
  uu.fails_with_code(r3, 1)
  uu.stderr_is(r3, "split: invalid suffix length: '-d200e'\n")
  let r4 = uu.invoke(s, "split", ["-C", "-200", "file"])?
  uu.fails_with_code(r4, 1)
  uu.stderr_is(r4, "split: invalid number of lines: '-200'\n")
  let r5 = uu.invoke(s, "split", ["--line-bytes", "-x200a4", "file"])?
  uu.fails_with_code(r5, 1)
  uu.stderr_is(r5, "split: invalid number of lines: '-x200a4'\n")
  let r6 = uu.invoke(s, "split", ["-b", "-200", "file"])?
  uu.fails_with_code(r6, 1)
  uu.stderr_is(r6, "split: invalid number of bytes: '-200'\n")
  let r7 = uu.invoke(s, "split", ["--bytes", "-200xd", "file"])?
  uu.fails_with_code(r7, 1)
  uu.stderr_is(r7, "split: invalid number of bytes: '-200xd'\n")
  let r8 = uu.invoke(s, "split", ["-n", "-200", "file"])?
  uu.fails_with_code(r8, 1)
  uu.stderr_is(r8, "split: invalid number of chunks: '-200'\n")
  let r9 = uu.invoke(s, "split", ["--number", "-e200", "file"])?
  uu.fails_with_code(r9, 1)
  uu.stderr_is(r9, "split: invalid number of chunks: '-e200'\n")
}

# origin: uutils test_split::test_split_obs_lines_within_combined_with_number
test test_uu_split_split_obs_lines_within_combined_with_number { |ctx|
  let s = split_scene(ctx)?
  uu.touch(s, "file")?
  let r0 = uu.invoke(s, "split", ["-3dxen", "4", "file"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "split: cannot split in more than one way\n")
  let r1 = uu.invoke(s, "split", ["-dxe30n", "4", "file"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "split: cannot split in more than one way\n")
}

# origin: uutils test_split::test_split_obs_lines_within_invalid_combined_shorts
test test_uu_split_split_obs_lines_within_invalid_combined_shorts { |ctx|
  let s = split_scene(ctx)?
  uu.touch(s, "file")?
  let r0 = uu.invoke(s, "split", ["-2fb", "file"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_is(r0, "split: invalid option -- 'f'\nTry 'split --help' for more information.\n")
}

# origin: uutils test_split::test_split_separator_invalid_usage
test test_uu_split_split_separator_invalid_usage { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--separator=xx"], stdin: bytes.from_text("a\n"))?
  uu.fails(r0)
  uu.no_stdout(r0)
  uu.stderr_is(r0, "split: multi-character separator 'xx'\n")
  let r1 = uu.invoke(s, "split", ["-ta", "-tb"], stdin: bytes.from_text("a\n"))?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_is(r1, "split: multiple separator characters specified\n")
  let r2 = uu.invoke(s, "split", ["-t'\n'", "-tb"], stdin: bytes.from_text("a\n"))?
  uu.fails(r2)
  uu.no_stdout(r2)
  uu.stderr_is(r2, "split: multi-character separator '\\'\\n\\''\n")
}

# origin: uutils test_split::test_split_separator_nl_line_bytes
test test_uu_split_split_separator_nl_line_bytes { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--line-bytes=4", "-t", "\n"], stdin: bytes.from_text("1\n2\n3\n4\n5\n"))?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\n2\n")
  uu.file_is(s, "xab", "3\n4\n")
  uu.file_is(s, "xac", "5\n")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_nl_lines
test test_uu_split_split_separator_nl_lines { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--lines=2", "-t", "\n"], stdin: bytes.from_text("1\n2\n3\n4\n5\n"))?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\n2\n")
  uu.file_is(s, "xab", "3\n4\n")
  uu.file_is(s, "xac", "5\n")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_nl_number_l
test test_uu_split_split_separator_nl_number_l { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=l/3", "--separator=\n", "fivelines.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\n2\n")
  uu.file_is(s, "xab", "3\n4\n")
  uu.file_is(s, "xac", "5\n")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_nl_number_r
test test_uu_split_split_separator_nl_number_r { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=r/3", "--separator", "\n", "fivelines.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\n4\n")
  uu.file_is(s, "xab", "2\n5\n")
  uu.file_is(s, "xac", "3\n")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_no_value
test test_uu_split_split_separator_no_value { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["-t"], stdin: bytes.from_text("a\n"))?
  uu.fails(r0)
  uu.stderr_is(r0, "split: option requires an argument -- 't'\nTry 'split --help' for more information.\n")
}

# origin: uutils test_split::test_split_separator_nul_line_bytes
test test_uu_split_split_separator_nul_line_bytes { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--line-bytes=4", "-t", "\\0", "separator_nul.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\02\0")
  uu.file_is(s, "xab", "3\04\0")
  uu.file_is(s, "xac", "5\0")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_nul_lines
test test_uu_split_split_separator_nul_lines { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--lines=2", "-t", "\\0", "separator_nul.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\02\0")
  uu.file_is(s, "xab", "3\04\0")
  uu.file_is(s, "xac", "5\0")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_nul_number_l
test test_uu_split_split_separator_nul_number_l { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=l/3", "--separator=\\0", "separator_nul.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\02\0")
  uu.file_is(s, "xab", "3\04\0")
  uu.file_is(s, "xac", "5\0")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_nul_number_r
test test_uu_split_split_separator_nul_number_r { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=r/3", "--separator=\\0", "separator_nul.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\04\0")
  uu.file_is(s, "xab", "2\05\0")
  uu.file_is(s, "xac", "3\0")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_same_multiple
test test_uu_split_split_separator_same_multiple { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--separator=:", "--separator=:", "fivelines.txt"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "split", ["-t:", "--separator=:", "fivelines.txt"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "split", ["-t", ":", "-t", ":", "fivelines.txt"])?
  uu.succeeds(r2)
  let r3 = uu.invoke(s, "split", ["-t:", "-t:", "-t,", "fivelines.txt"])?
  uu.fails(r3)
}

# origin: uutils test_split::test_split_separator_semicolon_line_bytes
test test_uu_split_split_separator_semicolon_line_bytes { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--line-bytes=4", "-t", ";", "separator_semicolon.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1;2;")
  uu.file_is(s, "xab", "3;4;")
  uu.file_is(s, "xac", "5;")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_semicolon_lines
test test_uu_split_split_separator_semicolon_lines { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--lines=2", "-t", ";", "separator_semicolon.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1;2;")
  uu.file_is(s, "xab", "3;4;")
  uu.file_is(s, "xac", "5;")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_semicolon_number_kth_l
test test_uu_split_split_separator_semicolon_number_kth_l { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=l/1/3", "--separator", ";", "separator_semicolon.txt"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1;2;")
}

# origin: uutils test_split::test_split_separator_semicolon_number_kth_r
test test_uu_split_split_separator_semicolon_number_kth_r { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=r/1/3", "--separator", ";", "separator_semicolon.txt"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1;4;")
}

# origin: uutils test_split::test_split_separator_semicolon_number_l
test test_uu_split_split_separator_semicolon_number_l { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=l/3", "--separator=;", "separator_semicolon.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1;2;")
  uu.file_is(s, "xab", "3;4;")
  uu.file_is(s, "xac", "5;")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_separator_semicolon_number_r
test test_uu_split_split_separator_semicolon_number_r { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=r/3", "--separator=;", "separator_semicolon.txt"])?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1;4;")
  uu.file_is(s, "xab", "2;5;")
  uu.file_is(s, "xac", "3;")
  assert ! uu.exists(s, "xad")?
}

# origin: uutils test_split::test_split_stdin_num_chunks
test test_uu_split_split_stdin_num_chunks { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=1"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "")
  assert ! uu.exists(s, "xab")?
}

# origin: uutils test_split::test_split_stdin_num_kth_chunk
test test_uu_split_split_stdin_num_kth_chunk { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=1/2"], stdin: bytes.from_text("1\n2\n3\n4\n5\n"))?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1\n2\n3")
}

# origin: uutils test_split::test_split_stdin_num_kth_line_chunk
test test_uu_split_split_stdin_num_kth_line_chunk { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=l/2/5"], stdin: bytes.from_text("1\n2\n3\n4\n5\n"))?
  uu.succeeds(r0)
  uu.stdout_only(r0, "2\n")
}

# origin: uutils test_split::test_split_stdin_num_line_chunks
test test_uu_split_split_stdin_num_line_chunks { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["--number=l/2"], stdin: bytes.from_text("1\n2\n3\n4\n5\n"))?
  uu.succeeds(r0)
  uu.file_is(s, "xaa", "1\n2\n3\n")
  uu.file_is(s, "xab", "4\n5\n")
  assert ! uu.exists(s, "xac")?
}

# origin: uutils test_split::test_suffix_length_req
test test_uu_split_suffix_length_req { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["-n", "100", "-a", "1", "asciilowercase.txt"])?
  uu.fails(r0)
  uu.stderr_only(r0, "split: the suffix length needs to be at least 2\n")
}

# origin: uutils test_split::test_suffixes_exhausted
test test_uu_split_suffixes_exhausted { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["-b", "1", "-a", "1", "asciilowercase.txt"])?
  uu.fails(r0)
  uu.stderr_only(r0, "split: output file suffixes exhausted\n")
}

# origin: uutils test_split::test_verbose
test test_uu_split_verbose { |ctx|
  let s = split_scene(ctx)?
  let r0 = uu.invoke(s, "split", ["-b", "5", "--verbose", "asciilowercase.txt"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "creating file 'xaa'\ncreating file 'xab'\ncreating file 'xac'\ncreating file 'xad'\ncreating file 'xae'\ncreating file 'xaf'\n")
}

# origin: uutils test_split::test_split_multiple_obs_lines_within_combined
test test_uu_split_split_multiple_obs_lines_within_combined { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "multiple-obs-lines", 400)?
  let r = uu.invoke(s, "split", ["-d5000x", "-e200d", "multiple-obs-lines"])?
  uu.succeeds(r)
  uu.no_output(r)
  let files = matching_outputs(s, r"""x\d\d$""")?
  assert files.len() == 2
  assert collate_outputs(files)? == uu.read(s, "multiple-obs-lines")?
}

# origin: uutils test_split::test_split_num_prefixed_chunks_by_lines
test test_uu_split_split_num_prefixed_chunks_by_lines { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "split_num_prefixed_chunks_by_lines", 10000)?
  let r = uu.invoke(s, "split", ["-d", "-l", "1000", "split_num_prefixed_chunks_by_lines", "c"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"""c\d\d$""")?
  assert files.len() == 10
  assert collate_outputs(files)? == uu.read(s, "split_num_prefixed_chunks_by_lines")?
}

# origin: uutils test_split::test_split_numeric_prefixed_chunks_by_bytes
test test_uu_split_split_numeric_prefixed_chunks_by_bytes { |ctx|
  let s = split_scene(ctx)?
  random_byte_input(s, "split_num_prefixed_chunks_by_bytes", 10000)?
  let r = uu.invoke(s, "split", ["-d", "-b", "1000", "split_num_prefixed_chunks_by_bytes", "a"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"""a\d\d$""")?
  assert files.len() == 10
  for file in files { assert fs.stat(file)?.size == 1000 }
  assert collate_outputs(files)? == uu.read(s, "split_num_prefixed_chunks_by_bytes")?
}

# origin: uutils test_split::test_split_obs_lines_standalone
test test_uu_split_split_obs_lines_standalone { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "obs-lines-standalone", 4)?
  let r = uu.invoke(s, "split", ["-2", "obs-lines-standalone"])?
  uu.succeeds(r)
  uu.no_output(r)
  let files = matching_outputs(s, r"""x[[:alpha:]][[:alpha:]]$""")?
  assert files.len() == 2
  assert collate_outputs(files)? == uu.read(s, "obs-lines-standalone")?
}

# origin: uutils test_split::test_split_obs_lines_standalone_overflow
test test_uu_split_split_obs_lines_standalone_overflow { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "obs-lines-standalone", 4)?
  let r = uu.invoke(s, "split", ["-99999999999999999991", "obs-lines-standalone"])?
  uu.succeeds(r)
  uu.no_output(r)
  let files = matching_outputs(s, r"""x[[:alpha:]][[:alpha:]]$""")?
  assert files.len() == 1
  assert collate_outputs(files)? == uu.read(s, "obs-lines-standalone")?
}

# origin: uutils test_split::test_split_obs_lines_starts_combined_shorts
test test_uu_split_split_obs_lines_starts_combined_shorts { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "obs-lines-starts-shorts", 400)?
  let r = uu.invoke(s, "split", ["-200xd", "obs-lines-starts-shorts"])?
  uu.succeeds(r)
  uu.no_output(r)
  let files = matching_outputs(s, r"""x\d\d$""")?
  assert files.len() == 2
  assert collate_outputs(files)? == uu.read(s, "obs-lines-starts-shorts")?
}

# origin: uutils test_split::test_split_obs_lines_within_combined_shorts
test test_uu_split_split_obs_lines_within_combined_shorts { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "obs-lines-within-shorts", 400)?
  let r = uu.invoke(s, "split", ["-x200de", "obs-lines-within-shorts"])?
  uu.succeeds(r)
  uu.no_output(r)
  let files = matching_outputs(s, r"""x\d\d$""")?
  assert files.len() == 2
  assert collate_outputs(files)? == uu.read(s, "obs-lines-within-shorts")?
}

# origin: uutils test_split::test_split_obs_lines_within_combined_shorts_tailing_suffix_length
test test_uu_split_split_obs_lines_within_combined_shorts_tailing_suffix_length { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "obs-lines-combined-shorts-tailing-suffix-length", 1000)?
  let r = uu.invoke(s, "split", ["-d200a4", "obs-lines-combined-shorts-tailing-suffix-length"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"""x\d\d\d\d$""")?
  assert files.len() == 5
  assert collate_outputs(files)? == uu.read(s, "obs-lines-combined-shorts-tailing-suffix-length")?
}

# origin: uutils test_split::test_split_overflow_bytes_size
test test_uu_split_split_overflow_bytes_size { |ctx|
  let s = split_scene(ctx)?
  random_byte_input(s, "test_split_overflow_bytes_size", 1000)?
  let r = uu.invoke(s, "split", ["-b", "1Y", "test_split_overflow_bytes_size"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"""x[[:alpha:]][[:alpha:]]$""")?
  assert files.len() == 1
  assert collate_outputs(files)? == uu.read(s, "test_split_overflow_bytes_size")?
}

# origin: uutils test_split::test_split_str_prefixed_chunks_by_bytes
test test_uu_split_split_str_prefixed_chunks_by_bytes { |ctx|
  let s = split_scene(ctx)?
  random_byte_input(s, "split_str_prefixed_chunks_by_bytes", 10000)?
  let r = uu.invoke(s, "split", ["-b", "1000", "split_str_prefixed_chunks_by_bytes", "b"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"""b[[:alpha:]][[:alpha:]]$""")?
  assert files.len() == 10
  for file in files { assert fs.stat(file)?.size == 1000 }
  assert collate_outputs(files)? == uu.read(s, "split_str_prefixed_chunks_by_bytes")?
}

# origin: uutils test_split::test_split_str_prefixed_chunks_by_lines
test test_uu_split_split_str_prefixed_chunks_by_lines { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "split_str_prefixed_chunks_by_lines", 10000)?
  let r = uu.invoke(s, "split", ["-l", "1000", "split_str_prefixed_chunks_by_lines", "d"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"""d[[:alpha:]][[:alpha:]]$""")?
  assert files.len() == 10
  assert collate_outputs(files)? == uu.read(s, "split_str_prefixed_chunks_by_lines")?
}

# origin: uutils test_split::test_split_suffix_length_short_concatenated_with_value
test test_uu_split_split_suffix_length_short_concatenated_with_value { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "split_num_prefixed_chunks_by_lines", 10000)?
  let r = uu.invoke(s, "split", ["-a4", "split_num_prefixed_chunks_by_lines"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"""x[[:alpha:]][[:alpha:]][[:alpha:]][[:alpha:]]$""")?
  assert files.len() == 10
  assert collate_outputs(files)? == uu.read(s, "split_num_prefixed_chunks_by_lines")?
}

# origin: uutils test_split::test_split_number_oversized_stdin
test test_uu_split_split_number_oversized_stdin { |ctx|
  let s = split_scene(ctx)?
  let input = fp"{ctx.core_dir}/tests/data/uutils/split/sixhundredfiftyonebytes.txt".read_bytes()?
  let r = uu.invoke(s, "split", ["--number=3", "---io-blksize=600"], stdin: input)?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_split::test_suffix_auto_width_with_number
test test_uu_split_suffix_auto_width_with_number { |ctx|
  let s = split_scene(ctx)?
  let r = uu.invoke(s, "split", ["--numeric-suffixes=1", "--number=r/100", "fivelines.txt"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"x\d\d\d$")?
  assert files.len() == 100
  assert collate_outputs(files)? == uu.read(s, "fivelines.txt")?
  uu.file_is(s, "x001", "1\n")
  uu.file_is(s, "x100", "")
  let fresh = split_scene(ctx)?
  uu.fails(uu.invoke(fresh, "split", ["--numeric-suffixes=100", "--number=r/100", "fivelines.txt"])?)
}

# origin: uutils test_split::test_suffix_length_zero
test test_uu_split_suffix_length_zero { |ctx|
  let s = split_scene(ctx)?
  let r = uu.invoke(s, "split", ["--numeric-suffixes=1", "--number=r/100", "-a", "0", "fivelines.txt"])?
  uu.succeeds(r)
  let files = matching_outputs(s, r"x\d\d\d$")?
  assert files.len() == 100
  let second = split_scene(ctx)?
  uu.fails(uu.invoke(second, "split", ["--numeric-suffixes=100", "--number=r/100", "-a", "0", "fivelines.txt"])?)
  let third = split_scene(ctx)?
  let exhausted = uu.invoke(third, "split", ["-b", "1", "--numeric-suffixes=89", "-a", "0", "ninetyonebytes.txt"])?
  uu.fails(exhausted)
  uu.stderr_only(exhausted, "split: output file suffixes exhausted\n")
}

# origin: uutils test_split::test_split_to_non_seekable
test test_uu_split_split_to_non_seekable { |ctx|
  let s = split_scene(ctx)?
  uu.symlink(s, "/dev/stdout", "xaa")?
  let r = split_to_stdout_pipe(s, ["-"], b"string")?
  uu.succeeds(r)
  uu.stdout_is(r, "string")
}

# origin: uutils test_split::test_write_error_on_full_device
test test_uu_split_write_error_on_full_device { |ctx|
  let s = split_scene(ctx)?
  assert p"/dev/full".exists()?
  uu.symlink(s, "/dev/full", "xaa")?
  let r = uu.invoke(s, "split", ["-b", "1"], stdin: b"uv")?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "split: xaa: No space left on device\n")
  assert uu.is_symlink(s, "xaa")?
  assert uu.read_link(s, "xaa")? == "/dev/full"
  let next = uu.at(s, "xab")
  let file_exists = if next.exists()? { next.is_file()? } else { false }
  assert ! file_exists
}

# origin: uutils test_split::test_split_non_utf8_paths
test test_uu_split_split_non_utf8_paths { |ctx|
  let s = split_scene(ctx)?
  let file = uu.at_bytes(s, b"\xff\xfe")?
  file.write(b"line1\nline2\nline3\nline4\nline5\n")?
  let r = uu.invoke_paths(s, "split", [Path.parse_bytes(b"\xff\xfe")?])?
  uu.succeeds(r)
  assert uu.exists(s, "xaa")?
}

# origin: uutils test_split::test_split_non_utf8_argument_unix
test test_uu_split_split_non_utf8_argument_unix { |ctx|
  let s = split_scene(ctx)?
  random_line_input(s, "test_split_non_utf8_argument", 2000)?
  let r = uu.invoke_paths(s, "split", [p"--additional-suffix", Path.parse_bytes(b"fo\x80o")?, p"test_split_non_utf8_argument"])?
  uu.succeeds(r)
}

# origin: uutils test_split::test_split_non_utf8_prefix_is_byte_preserving
test test_uu_split_split_non_utf8_prefix_is_byte_preserving { |ctx|
  let s = split_scene(ctx)?
  uu.write(s, "input.txt", "AB")?
  uu.write(s, "p�aa", "keep-aa")?
  uu.write(s, "p�ab", "keep-ab")?
  let r = uu.invoke_paths(s, "split", [p"-b", p"1", p"input.txt", Path.parse_bytes(b"p\xff")?])?
  uu.succeeds(r)
  assert byte_named_outputs(s, b"p\xff")? == [b"p\xffaa", b"p\xffab"]
  uu.file_is(s, "p�aa", "keep-aa")
  uu.file_is(s, "p�ab", "keep-ab")
}

# origin: uutils test_split::test_split_non_utf8_additional_suffix_is_byte_preserving
test test_uu_split_split_non_utf8_additional_suffix_is_byte_preserving { |ctx|
  let s = split_scene(ctx)?
  uu.write(s, "input.txt", "AB")?
  let r = uu.invoke_paths(s, "split", [p"-b", p"1", p"input.txt", p"--additional-suffix", Path.parse_bytes(b"\xff\xfe")?])?
  uu.succeeds(r)
  assert byte_named_outputs(s, b"xa", b"\xff\xfe")? == [b"xaa\xff\xfe", b"xab\xff\xfe"]
}
