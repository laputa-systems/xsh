##! Transcribed from the uutils coreutils integration tests for split.

use support.uu

proc split_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  for name in ["asciilowercase.txt", "fivelines.txt", "letters.txt", "ninetyonebytes.txt", "noeof.txt", "onehundredlines.txt", "separator_nul.txt", "separator_semicolon.txt", "sixhundredfiftyonebytes.txt", "threebytes.txt", "twohundredfortyonebytes.txt"] { uu.fixture(s, "split", name, name) }
  Ok(s)
}

proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  match uu.file_exists(s, name) {
    Ok(found) => Ok(found),
    Err(failure) => if failure.errno == 2 { Ok(false) } else { Err(failure) },
  }
}

proc matching(s: uu.Scene, pattern: Regex) [fs, error] -> Result[List[Str], Error] {
  let names = collect {
    for entry in fs.children(s.root, ordered: true)? {
      yield entry.name when pattern.matches(entry.name)
    }
  }
  Ok(names)
}

proc collate(s: uu.Scene, pattern: Regex) [fs, error] -> Result[Bytes, Error] {
  Ok(bytes.concat([uu.read(s, name)? for name in matching(s, pattern)?]))
}

# Every byte-oriented input starts at the same seed and keeps the frozen random stream.
proc seeded_bytes(s: uu.Scene, name: Str, length: Int) [fs, error] {
  let source = fp"{s.ctx.core_dir}/tests/data/uutils/split/seeded-123.bin".read_bytes()?
  uu.write_bytes(s, name, source[..length])
  Ok()
}

# The upstream line corpus uses independent alphanumeric data and includes each newline.
proc random_lines(s: uu.Scene, name: Str, count: Int, width = 32) [fs, error] {
  let source = bytes.read_at(p"/dev/urandom", 0, count * width)?
  assert source.len() == count * width
  let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
  let mapped = bytes.from_ints([alphabet.byte_at((source.byte_at(i) ?? 0) % 62) ?? 0 for i in range(source.len())])?
  let lines = bytes.concat([bytes.concat([mapped[i * width..(i + 1) * width], b"\n"]) for i in range(count)])
  uu.write_bytes(s, name, lines)
  Ok()
}

# The replica remains open while polling, so draining cannot block on terminal EOF.
proc terminal_stderr(s: uu.Scene, args: List[Str]) [fs, process, net, env, error] -> Result[uu.Ran, Error] {
  let pair = unix.open_pty()?
  defer unix.close_fd(pair.master)
  defer unix.close_fd(pair.replica)
  unix.set_window_size(30, 80, xpixel: 640, ypixel: 300, fd: pair.replica)
  let out = uu.at(s, "stdout")
  let argv = uu.argv(s, "split", [fp"{word}" for word in args])?
  let plan = process.command_argv(s.ctx.xsh_bin, argv, s.root, {}, b"", out, fp"{pair.name}", timeout: 5s)
  let status = process.run(plan)?
  var errors = b""
  while "readable" in unix.poll_fd(pair.master, ["readable"], timeout_ms: 0)? {
    let chunk = unix.read_fd(pair.master, 8192)?
    break when chunk.is_empty()
    errors = bytes.concat([errors, chunk])
  }
  Ok({util: "split", args: args, status: status.exit_code()?, stdout: out.read_bytes()?, stderr: errors})
}

# origin: uutils test_split::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_split_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-b", "7zq", "/dev/null"], timeout: 5s)?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "split: invalid number of bytes: '7zq'\n")
}

# origin: uutils test_split::diagnostics::test_snippet_underlines_a_count_with_no_number
test test_uu_split_diagnostics_snippet_underlines_a_count_with_no_number { |ctx|
  let s = split_scene(ctx)?
  let r = terminal_stderr(s, ["-l", "qq", "/dev/null"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "invalid number of lines")
  assert !("not a known unit" in r.stderr as Str)
}

# origin: uutils test_split::test_additional_suffix_dir_separator
test test_uu_split_additional_suffix_dir_separator { |ctx|
  let s = split_scene(ctx)?
  let r = uu.invoke(s, "split", ["--additional-suffix", "a/b"])?
  uu.fails(r)
  uu.stderr_only(r, "split: invalid suffix 'a/b', contains directory separator\nTry 'split --help' for more information.\n")
}

# origin: uutils test_split::test_allow_empty_files
test test_uu_split_allow_empty_files { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "xaa", "a")
  uu.file_is(s, "xab", "b")
  uu.file_is(s, "xac", "c")
  uu.file_is(s, "xad", "")
}

# origin: uutils test_split::test_alphabetic_dynamic_suffix_length
test test_uu_split_alphabetic_dynamic_suffix_length { |ctx|
  let s = split_scene(ctx)?
  let r = uu.invoke(s, "split", ["-b", "1", "sixhundredfiftyonebytes.txt"])?
  uu.succeeds(r)
  let alphabet = "abcdefghijklmnopqrstuvwxyz"
  for i in range(25) {
    for j in range(26) {
      uu.file_is(s, "x" + alphabet.byte_slice(i, length: 1) + alphabet.byte_slice(j, length: 1), "a")
    }
  }
  uu.file_is(s, "xzaaa", "a")
}

# origin: uutils test_split::test_dynamic_suffix_length_off_with_suffix_start
test test_uu_split_dynamic_suffix_length_off_with_suffix_start { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-b", "1", "--numeric-suffixes=89", "ninetyonebytes.txt"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_only(r1, "split: output file suffixes exhausted\n")
}

# origin: uutils test_split::test_dynamic_suffix_length_on_with_suffix_start_no_value
test test_uu_split_dynamic_suffix_length_on_with_suffix_start_no_value { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-b", "1", "--numeric-suffixes", "ninetyonebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.file_is(s, "x9000", "a")
}

# origin: uutils test_split::test_effective_suffix_hex_last
test test_uu_split_effective_suffix_hex_last { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "--hex-suffixes=7", "--numeric-suffixes=4", "-x", "-d", "--hex-suffixes=9", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x09", "a")
  uu.file_is(s, "x0a", "b")
  uu.file_is(s, "x0b", "c")
  uu.file_is(s, "x0c", "")
}

# origin: uutils test_split::test_effective_suffix_numeric_last
test test_uu_split_effective_suffix_numeric_last { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "--numeric-suffixes=7", "--hex-suffixes=4", "-d", "-x", "--numeric-suffixes=9", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x09", "a")
  uu.file_is(s, "x10", "b")
  uu.file_is(s, "x11", "c")
  uu.file_is(s, "x12", "")
}

# origin: uutils test_split::test_elide_dev_null_l_chunks
test test_uu_split_elide_dev_null_l_chunks { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-e", "-n", "l/3", "/dev/null"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert !uu.exists(s, "xaa")?
  assert !uu.exists(s, "xab")?
  assert !uu.exists(s, "xac")?
}

# origin: uutils test_split::test_elide_dev_null_n_chunks
test test_uu_split_elide_dev_null_n_chunks { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-e", "-n", "3", "/dev/null"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert !uu.exists(s, "xaa")?
  assert !uu.exists(s, "xab")?
  assert !uu.exists(s, "xac")?
}

# origin: uutils test_split::test_elide_empty_files_l_chunks
test test_uu_split_elide_empty_files_l_chunks { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-e", "-n", "l/7", "fivelines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "xaa", "1\n")
  uu.file_is(s, "xab", "2\n")
  uu.file_is(s, "xac", "3\n")
  uu.file_is(s, "xad", "4\n")
  uu.file_is(s, "xae", "5\n")
  assert !uu.exists(s, "xaf")?
  assert !uu.exists(s, "xag")?
}

# origin: uutils test_split::test_elide_empty_files_n_chunks
test test_uu_split_elide_empty_files_n_chunks { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-e", "-n", "4", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "xaa", "a")
  uu.file_is(s, "xab", "b")
  uu.file_is(s, "xac", "c")
  assert !uu.exists(s, "xad")?
}

# origin: uutils test_split::test_empty_input_does_not_create_output
test test_uu_split_empty_input_does_not_create_output { |ctx|
  let s = split_scene(ctx)?
  for args in [[], ["-b", "1"], ["-l", "1"], ["-C", "1"]] {
    let s = split_scene(ctx)?
    let r = uu.invoke(s, "split", args.extend(["--verbose"]), stdin: b"")?
    uu.succeeds(r)
    uu.no_output(r)
    assert !uu.exists(s, "xaa")?
    uu.touch(s, "empty")
    let file = uu.invoke(s, "split", args.extend(["empty"]))?
    uu.succeeds(file)
    uu.no_output(file)
    assert !uu.exists(s, "xaa")?
  }
}

# origin: uutils test_split::test_empty_input_preserves_existing_output
test test_uu_split_empty_input_preserves_existing_output { |ctx|
  let s = split_scene(ctx)?
  for args in [[], ["-b", "1"], ["-l", "1"], ["-C", "1"]] {
    let s = split_scene(ctx)?
    uu.write(s, "xaa", "keep this output\n")
    let r = uu.invoke(s, "split", args, stdin: b"")?
    uu.succeeds(r)
    uu.no_output(r)
    uu.file_is(s, "xaa", "keep this output\n")
    assert !uu.exists(s, "xab")?
  }
}

# origin: uutils test_split::test_empty_input_with_output_directory
test test_uu_split_empty_input_with_output_directory { |ctx|
  let s = split_scene(ctx)?
  for args in [[], ["-b", "1"], ["-l", "1"], ["-C", "1"]] {
    let s = split_scene(ctx)?
    uu.mkdir(s, "xaa")
    let r = uu.invoke(s, "split", args, stdin: b"")?
    uu.succeeds(r)
    uu.no_output(r)
    assert uu.dir_exists(s, "xaa")?
    assert !uu.exists(s, "xab")?
  }
}

# origin: uutils test_split::test_filter
test test_uu_split_filter { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "filtered", 3)
  let r = uu.invoke(s, "split", ["--filter=sed s/./i/g > $FILE", "filtered"], timeout: 5s)?
  uu.succeeds(r)
  let data = collate(s, rx"x[[:alpha:]][[:alpha:]]$")?
  for i in range(data.len()) { assert (data.byte_at(i) ?? -1) in [105, 10] }
}

# origin: uutils test_split::test_filter_broken_pipe
test test_uu_split_filter_broken_pipe { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "filter-big-input", 1024 * 10)
  let r = uu.invoke(s, "split", ["--filter=head -c1 > /dev/null", "-n", "r/1", "filter-big-input"], timeout: 5s)?
  uu.succeeds(r)
}

# origin: uutils test_split::test_filter_command_fails
test test_uu_split_filter_command_fails { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "filter-will-fail", 4)
  let r = uu.invoke(s, "split", ["--filter=/a/path/that/totally/does/not/exist", "filter-will-fail"], timeout: 5s)?
  uu.fails(r)
}

# origin: uutils test_split::test_filter_with_env_var_set
test test_uu_split_filter_with_env_var_set { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "filtered", 3)
  let r = uu.invoke(s, "split", ["--filter=cat > $FILE", "filtered"], vars: {FILE: "some-value"}, timeout: 5s)?
  uu.succeeds(r)
  assert collate(s, rx"x[[:alpha:]][[:alpha:]]$")? == uu.read(s, "filtered")?
}

# origin: uutils test_split::test_filter_with_kth_chunk
test test_uu_split_filter_with_kth_chunk { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["--filter='some'", "--number=1/2"], timeout: 5s, stdin: bytes.from_text("a\n"))?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "--filter does not process a chunk extracted to standard output")
  let r2 = uu.invoke(s, "split", ["--filter='some'", "--number=l/1/2"], timeout: 5s, stdin: bytes.from_text("a\n"))?
  uu.fails(r2)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "--filter does not process a chunk extracted to standard output")
  let r3 = uu.invoke(s, "split", ["--filter='some'", "--number=r/1/2"], timeout: 5s, stdin: bytes.from_text("a\n"))?
  uu.fails(r3)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "--filter does not process a chunk extracted to standard output")
}

# origin: uutils test_split::test_guard_input
test test_uu_split_guard_input { |ctx|
  let s = split_scene(ctx)?
  repeat 2 times {
    let r = uu.invoke(s, "split", ["-C", "6"], stdin: b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n")?
    uu.succeeds(r)
    uu.no_output(r)
    uu.file_is(s, "xaa", "1\n2\n3\n")
  }
  let r = uu.invoke(s, "split", ["-C", "6", "xaa"])?
  uu.fails(r)
  uu.stderr_only(r, "split: 'xaa' would overwrite input; aborting\n")
  uu.file_is(s, "xaa", "1\n2\n3\n")
}

# origin: uutils test_split::test_hex_dynamic_suffix_length
test test_uu_split_hex_dynamic_suffix_length { |ctx|
  let s = split_scene(ctx)?
  let r = uu.invoke(s, "split", ["-x", "-b", "1", "twohundredfortyonebytes.txt"])?
  uu.succeeds(r)
  for i in range(240) { uu.file_is(s, "x" + "0123456789abcdef".byte_slice(i / 16, length: 1) + "0123456789abcdef".byte_slice(i % 16, length: 1), "a") }
  uu.file_is(s, "xf000", "a")
}

# origin: uutils test_split::test_hex_suffix
test test_uu_split_hex_suffix { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "--hex-suffixes=9", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x09", "a")
  uu.file_is(s, "x0a", "b")
  uu.file_is(s, "x0b", "c")
  uu.file_is(s, "x0c", "")
}

# origin: uutils test_split::test_hex_suffix_alias
test test_uu_split_hex_suffix_alias { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "--hex=9", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x09", "a")
  uu.file_is(s, "x0a", "b")
  uu.file_is(s, "x0b", "c")
  uu.file_is(s, "x0c", "")
}

# origin: uutils test_split::test_hex_suffix_no_equal
test test_uu_split_hex_suffix_no_equal { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "--hex-suffixes", "9", "threebytes.txt"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_contains(r1, "split: cannot open '9' for reading: No such file or directory")
}

# origin: uutils test_split::test_hex_suffix_no_value
test test_uu_split_hex_suffix_no_value { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-l", "9", "--hex-suffixes", "onehundredlines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x00", "00\n01\n02\n03\n04\n05\n06\n07\n08\n")
  uu.file_is(s, "x01", "09\n10\n11\n12\n13\n14\n15\n16\n17\n")
  uu.file_is(s, "x02", "18\n19\n20\n21\n22\n23\n24\n25\n26\n")
  uu.file_is(s, "x03", "27\n28\n29\n30\n31\n32\n33\n34\n35\n")
  uu.file_is(s, "x04", "36\n37\n38\n39\n40\n41\n42\n43\n44\n")
  uu.file_is(s, "x05", "45\n46\n47\n48\n49\n50\n51\n52\n53\n")
  uu.file_is(s, "x06", "54\n55\n56\n57\n58\n59\n60\n61\n62\n")
  uu.file_is(s, "x07", "63\n64\n65\n66\n67\n68\n69\n70\n71\n")
  uu.file_is(s, "x08", "72\n73\n74\n75\n76\n77\n78\n79\n80\n")
  uu.file_is(s, "x09", "81\n82\n83\n84\n85\n86\n87\n88\n89\n")
  uu.file_is(s, "x0a", "90\n91\n92\n93\n94\n95\n96\n97\n98\n")
  uu.file_is(s, "x0b", "99\n")
}

# origin: uutils test_split::test_include_newlines
test test_uu_split_include_newlines { |ctx|
  let s = split_scene(ctx)?
  let r = uu.invoke(s, "split", ["-l", "2", "fivelines.txt"])?
  uu.succeeds(r)
  uu.file_is(s, "xaa", "1\n2\n")
  uu.file_is(s, "xab", "3\n4\n")
  uu.file_is(s, "xac", "5\n")
}

# origin: uutils test_split::test_invalid_arg
test test_uu_split_invalid_arg { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["--definitely-invalid"], timeout: 5s)?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_split::test_invalid_suffix_length
test test_uu_split_invalid_suffix_length { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-a", "xyz"], timeout: 5s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "invalid suffix length: 'xyz'")
}

# origin: uutils test_split::test_large_suffix_length_is_rejected
test test_uu_split_large_suffix_length_is_rejected { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-a", "66542562175252"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
}

# origin: uutils test_split::test_line_bytes
test test_uu_split_line_bytes { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-C", "8", "letters.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.file_is(s, "xaa", "aaaaaaaa")
  uu.file_is(s, "xab", "a\nbbbb\n")
  uu.file_is(s, "xac", "cccc\ndd\n")
  uu.file_is(s, "xad", "ee\n")
}

# origin: uutils test_split::test_line_bytes_concatenated_with_value
test test_uu_split_line_bytes_concatenated_with_value { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-C8", "letters.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.file_is(s, "xaa", "aaaaaaaa")
  uu.file_is(s, "xab", "a\nbbbb\n")
  uu.file_is(s, "xac", "cccc\ndd\n")
  uu.file_is(s, "xad", "ee\n")
}

# origin: uutils test_split::test_line_bytes_no_empty_file
test test_uu_split_line_bytes_no_empty_file { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-C", "1"], timeout: 5s, stdin: bytes.from_text("1\n2222\n3\n4"))?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "xaa", "1")
  uu.file_is(s, "xab", "\n")
  uu.file_is(s, "xac", "2")
  uu.file_is(s, "xad", "2")
  uu.file_is(s, "xae", "2")
  uu.file_is(s, "xaf", "2")
  uu.file_is(s, "xag", "\n")
  uu.file_is(s, "xah", "3")
  uu.file_is(s, "xai", "\n")
  uu.file_is(s, "xaj", "4")
  assert !uu.exists(s, "xak")?
}

# origin: uutils test_split::test_line_bytes_no_eof
test test_uu_split_line_bytes_no_eof { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-C", "3"], timeout: 5s, stdin: bytes.from_text("1\n2222\n3\n4"))?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "xaa", "1\n")
  uu.file_is(s, "xab", "222")
  uu.file_is(s, "xac", "2\n")
  uu.file_is(s, "xad", "3\n")
  uu.file_is(s, "xae", "4")
  assert !uu.exists(s, "xaf")?
}

# origin: uutils test_split::test_line_bytes_no_final_newline
test test_uu_split_line_bytes_no_final_newline { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-C", "2"], timeout: 5s, stdin: bytes.from_text("1\n2222\n3\n4"))?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "xaa", "1\n")
  uu.file_is(s, "xab", "22")
  uu.file_is(s, "xac", "22")
  uu.file_is(s, "xad", "\n")
  uu.file_is(s, "xae", "3\n")
  uu.file_is(s, "xaf", "4")
}

# origin: uutils test_split::test_line_bytes_overflow
test test_uu_split_line_bytes_overflow { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-C", "18446744073709551616", "letters.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.file_is(s, "xaa", "aaaaaaaaa\nbbbb\ncccc\ndd\nee\n")
}

# origin: uutils test_split::test_long_lines
test test_uu_split_long_lines { |ctx|
  let s = split_scene(ctx)?
  let line1 = bytes.concat([bytes.from_ints([32 for _ in range(131070)])?, b"\n"])
  let line2 = b" \n"
  let line3 = bytes.concat([bytes.from_ints([32 for _ in range(131071)])?, b"\n"])
  let input = bytes.concat([line1, line2, line3])
  let r = uu.invoke(s, "split", ["-C", "131072"], stdin: input)?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read(s, "xaa")?.len() == 131071
  assert uu.read(s, "xab")?.len() == 2
  assert uu.read(s, "xac")?.len() == 131072
  assert !uu.exists(s, "xad")?
}

# origin: uutils test_split::test_multiple_of_input_chunk
test test_uu_split_multiple_of_input_chunk { |ctx|
  let s = split_scene(ctx)?
  seeded_bytes(s, "multiple_of_input_chunk", 16384)
  let r = uu.invoke(s, "split", ["-b", "8K", "multiple_of_input_chunk", "b"], timeout: 5s)?
  uu.succeeds(r)
  let names = matching(s, rx"b[[:alpha:]][[:alpha:]]$")?
  assert names.len() == 2
  for name in names { assert uu.size(s, name)? == 8192 }
  assert collate(s, rx"b[[:alpha:]][[:alpha:]]$")? == uu.read(s, "multiple_of_input_chunk")?
}

# origin: uutils test_split::test_number_by_lines
test test_uu_split_number_by_lines { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "l/2", "fivelines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.file_is(s, "xaa", "1\n2\n3\n")
  uu.file_is(s, "xab", "4\n5\n")
}

# origin: uutils test_split::test_number_by_lines_fewer_bytes_than_chunks_elide
test test_uu_split_number_by_lines_fewer_bytes_than_chunks_elide { |ctx|
  let s = split_scene(ctx)?
  uu.write(s, "in", "a")
  let r1 = uu.invoke(s, "split", ["-e", "-n", "l/3", "in"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "xaa", "a")
  assert !uu.exists(s, "xab")?
  assert !uu.exists(s, "xac")?
}

# origin: uutils test_split::test_number_by_lines_kth
test test_uu_split_number_by_lines_kth { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "l/3/10", "onehundredlines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "20\n21\n22\n23\n24\n25\n26\n27\n28\n29\n")
}

# origin: uutils test_split::test_number_by_lines_kth_dev_null
test test_uu_split_number_by_lines_kth_dev_null { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "l/3/10", "/dev/null"], timeout: 5s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "")
}

# origin: uutils test_split::test_number_by_lines_kth_huge_number_of_chunks
test test_uu_split_number_by_lines_kth_huge_number_of_chunks { |ctx|
  let s = split_scene(ctx)?
  uu.write(s, "in", "a")
  let r1 = uu.invoke(s, "split", ["-n", "l/1/9999999999", "in"], timeout: 5s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a")
}

# origin: uutils test_split::test_number_by_lines_kth_no_end_sep
test test_uu_split_number_by_lines_kth_no_end_sep { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "l/3/10"], timeout: 5s, stdin: bytes.from_text("1\n2222\n3\n4"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2222\n")
  let r2 = uu.invoke(s, "split", ["-e", "-n", "l/2/2"], timeout: 5s, stdin: bytes.from_text("1\n2222\n3\n4"))?
  uu.succeeds(r2)
  uu.stdout_only(r2, "3\n4")
}

# origin: uutils test_split::test_number_by_lines_rr_kth_no_end_sep
test test_uu_split_number_by_lines_rr_kth_no_end_sep { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "r/2/3"], timeout: 5s, stdin: bytes.from_text("1\n2\n3\n4\n5"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2\n5")
}

# origin: uutils test_split::test_number_kth_of_n
test test_uu_split_number_kth_of_n { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["--number=3/5", "asciilowercase.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "mnopq")
  let r2 = uu.invoke(s, "split", ["--number=5/5", "asciilowercase.txt"], timeout: 5s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "wxyz\n")
  let r3 = uu.invoke(s, "split", ["-e", "--number=99/100", "asciilowercase.txt"], timeout: 5s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "")
  let r4 = uu.invoke(s, "split", ["--number=3/10", "/dev/null"], timeout: 5s)?
  uu.succeeds(r4)
  uu.stdout_only(r4, "")
  let r5 = uu.invoke(s, "split", ["--number=r/9223372036854775807/18446744073709551615", "asciilowercase.txt"], timeout: 5s)?
  uu.succeeds(r5)
  uu.stdout_only(r5, "")
  let r6 = uu.invoke(s, "split", ["--number=0/5", "asciilowercase.txt"], timeout: 5s)?
  uu.fails(r6)
  uu.stderr_contains(r6, "split: invalid chunk number: '0'")
  let r7 = uu.invoke(s, "split", ["--number=10/5", "asciilowercase.txt"], timeout: 5s)?
  uu.fails(r7)
  uu.stderr_contains(r7, "split: invalid chunk number: '10'")
  let r8 = uu.invoke(s, "split", ["--number=9223372036854775807/18446744073709551616", "asciilowercase.txt"], timeout: 5s)?
  uu.succeeds(r8)
  uu.stdout_only(r8, "")
}

# origin: uutils test_split::test_number_kth_of_n_round_robin
test test_uu_split_number_kth_of_n_round_robin { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["--number", "r/2/3", "fivelines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2\n5\n")
  let r2 = uu.invoke(s, "split", ["--number", "r/1/4", "fivelines.txt"], timeout: 5s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "1\n5\n")
  let r3 = uu.invoke(s, "split", ["-e", "--number", "r/7/7", "fivelines.txt"], timeout: 5s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "")
  let r4 = uu.invoke(s, "split", ["--number", "r/9223372036854775807/18446744073709551615", "fivelines.txt"], timeout: 5s)?
  uu.succeeds(r4)
  uu.stdout_only(r4, "")
  let r5 = uu.invoke(s, "split", ["--number", "r/9223372036854775807/18446744073709551616", "fivelines.txt"], timeout: 5s)?
  uu.succeeds(r5)
  uu.stdout_only(r5, "")
  let r6 = uu.invoke(s, "split", ["--number", "r/0/3", "fivelines.txt"], timeout: 5s)?
  uu.fails(r6)
  uu.stderr_contains(r6, "split: invalid chunk number: '0'")
  let r7 = uu.invoke(s, "split", ["--number", "r/10/3", "fivelines.txt"], timeout: 5s)?
  uu.fails(r7)
  uu.stderr_contains(r7, "split: invalid chunk number: '10'")
}

# origin: uutils test_split::test_number_n
test test_uu_split_number_n { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "5", "asciilowercase.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.file_is(s, "xaa", "abcdef")
  uu.file_is(s, "xab", "ghijkl")
  uu.file_is(s, "xac", "mnopq")
  uu.file_is(s, "xad", "rstuv")
  uu.file_is(s, "xae", "wxyz\n")
  let r2 = uu.invoke(s, "split", ["--number=100", "/dev/null"], timeout: 5s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "")
}

# origin: uutils test_split::test_numeric_dynamic_suffix_length
test test_uu_split_numeric_dynamic_suffix_length { |ctx|
  let s = split_scene(ctx)?
  let r = uu.invoke(s, "split", ["-d", "-b", "1", "ninetyonebytes.txt"])?
  uu.succeeds(r)
  for i in range(90) { uu.file_is(s, f"x{i:02}", "a") }
  uu.file_is(s, "x9000", "a")
}

# origin: uutils test_split::test_numeric_suffix
test test_uu_split_numeric_suffix { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "--numeric-suffixes=9", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x09", "a")
  uu.file_is(s, "x10", "b")
  uu.file_is(s, "x11", "c")
  uu.file_is(s, "x12", "")
}

# origin: uutils test_split::test_numeric_suffix_inferred
test test_uu_split_numeric_suffix_inferred { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "--numeric=9", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x09", "a")
  uu.file_is(s, "x10", "b")
  uu.file_is(s, "x11", "c")
  uu.file_is(s, "x12", "")
}

# origin: uutils test_split::test_numeric_suffix_no_equal
test test_uu_split_numeric_suffix_no_equal { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "--numeric-suffixes", "9", "threebytes.txt"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_contains(r1, "split: cannot open '9' for reading: No such file or directory")
}

# origin: uutils test_split::test_numeric_suffix_no_value
test test_uu_split_numeric_suffix_no_value { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-l", "9", "--numeric-suffixes", "onehundredlines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x00", "00\n01\n02\n03\n04\n05\n06\n07\n08\n")
  uu.file_is(s, "x01", "09\n10\n11\n12\n13\n14\n15\n16\n17\n")
  uu.file_is(s, "x02", "18\n19\n20\n21\n22\n23\n24\n25\n26\n")
  uu.file_is(s, "x03", "27\n28\n29\n30\n31\n32\n33\n34\n35\n")
  uu.file_is(s, "x04", "36\n37\n38\n39\n40\n41\n42\n43\n44\n")
  uu.file_is(s, "x05", "45\n46\n47\n48\n49\n50\n51\n52\n53\n")
  uu.file_is(s, "x06", "54\n55\n56\n57\n58\n59\n60\n61\n62\n")
  uu.file_is(s, "x07", "63\n64\n65\n66\n67\n68\n69\n70\n71\n")
  uu.file_is(s, "x08", "72\n73\n74\n75\n76\n77\n78\n79\n80\n")
  uu.file_is(s, "x09", "81\n82\n83\n84\n85\n86\n87\n88\n89\n")
  uu.file_is(s, "x10", "90\n91\n92\n93\n94\n95\n96\n97\n98\n")
  uu.file_is(s, "x11", "99\n")
}

# origin: uutils test_split::test_obsolete_lines_not_read_after_double_dash
test test_uu_split_obsolete_lines_not_read_after_double_dash { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["--", "-1"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot open '-1' for reading")
}

# origin: uutils test_split::test_round_robin
test test_uu_split_round_robin { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "r/2", "fivelines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.file_is(s, "xaa", "1\n3\n5\n")
  uu.file_is(s, "xab", "2\n4\n")
}

# origin: uutils test_split::test_round_robin_limited_file_descriptors
test test_uu_split_round_robin_limited_file_descriptors { |ctx|
  let s = split_scene(ctx)?
  let args = ["-n", "r/40", "onehundredlines.txt"]
  let applet_words = uu.argv(s, "split", [fp"{word}" for word in args])?
  let argv = [p"/bin/sh", p"-c", p"ulimit -n 9; exec \"$@\"", p"sh"].extend(applet_words)
  let out = uu.at(s, "out")
  let err = uu.at(s, "err")
  let status = process.run(process.command_argv(p"/bin/sh", argv, s.root, {}, b"", out, err, timeout: 5s))?
  assert status.exit_code()? == 0
}

# origin: uutils test_split::test_short_combination
test test_uu_split_short_combination { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-dxen", "4", "threebytes.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x00", "a")
  uu.file_is(s, "x01", "b")
  uu.file_is(s, "x02", "c")
  assert !file_exists(s, "x03")?
}

# origin: uutils test_split::test_short_hex_suffix_no_value
test test_uu_split_short_hex_suffix_no_value { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-l", "9", "-x", "onehundredlines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x00", "00\n01\n02\n03\n04\n05\n06\n07\n08\n")
  uu.file_is(s, "x01", "09\n10\n11\n12\n13\n14\n15\n16\n17\n")
  uu.file_is(s, "x02", "18\n19\n20\n21\n22\n23\n24\n25\n26\n")
  uu.file_is(s, "x03", "27\n28\n29\n30\n31\n32\n33\n34\n35\n")
  uu.file_is(s, "x04", "36\n37\n38\n39\n40\n41\n42\n43\n44\n")
  uu.file_is(s, "x05", "45\n46\n47\n48\n49\n50\n51\n52\n53\n")
  uu.file_is(s, "x06", "54\n55\n56\n57\n58\n59\n60\n61\n62\n")
  uu.file_is(s, "x07", "63\n64\n65\n66\n67\n68\n69\n70\n71\n")
  uu.file_is(s, "x08", "72\n73\n74\n75\n76\n77\n78\n79\n80\n")
  uu.file_is(s, "x09", "81\n82\n83\n84\n85\n86\n87\n88\n89\n")
  uu.file_is(s, "x0a", "90\n91\n92\n93\n94\n95\n96\n97\n98\n")
  uu.file_is(s, "x0b", "99\n")
}

# origin: uutils test_split::test_short_hex_suffix_with_value_spaced
test test_uu_split_short_hex_suffix_with_value_spaced { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "-x", "9", "threebytes.txt"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_contains(r1, "split: cannot open '9' for reading: No such file or directory")
}

# origin: uutils test_split::test_short_numeric_suffix_no_value
test test_uu_split_short_numeric_suffix_no_value { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-l", "9", "-d", "onehundredlines.txt"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "x00", "00\n01\n02\n03\n04\n05\n06\n07\n08\n")
  uu.file_is(s, "x01", "09\n10\n11\n12\n13\n14\n15\n16\n17\n")
  uu.file_is(s, "x02", "18\n19\n20\n21\n22\n23\n24\n25\n26\n")
  uu.file_is(s, "x03", "27\n28\n29\n30\n31\n32\n33\n34\n35\n")
  uu.file_is(s, "x04", "36\n37\n38\n39\n40\n41\n42\n43\n44\n")
  uu.file_is(s, "x05", "45\n46\n47\n48\n49\n50\n51\n52\n53\n")
  uu.file_is(s, "x06", "54\n55\n56\n57\n58\n59\n60\n61\n62\n")
  uu.file_is(s, "x07", "63\n64\n65\n66\n67\n68\n69\n70\n71\n")
  uu.file_is(s, "x08", "72\n73\n74\n75\n76\n77\n78\n79\n80\n")
  uu.file_is(s, "x09", "81\n82\n83\n84\n85\n86\n87\n88\n89\n")
  uu.file_is(s, "x10", "90\n91\n92\n93\n94\n95\n96\n97\n98\n")
  uu.file_is(s, "x11", "99\n")
}

# origin: uutils test_split::test_short_numeric_suffix_with_value_spaced
test test_uu_split_short_numeric_suffix_with_value_spaced { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-n", "4", "-d", "9", "threebytes.txt"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_contains(r1, "split: cannot open '9' for reading: No such file or directory")
}

# origin: uutils test_split::test_split_additional_suffix
test test_uu_split_split_additional_suffix { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "split_additional_suffix", 2000)
  let r = uu.invoke(s, "split", ["--additional-suffix", ".txt", "split_additional_suffix"], timeout: 5s)?
  uu.succeeds(r)
  let names = matching(s, rx"x[[:alpha:]][[:alpha:]].txt$")?
  assert names.len() == 2
  assert collate(s, rx"x[[:alpha:]][[:alpha:]].txt$")? == uu.read(s, "split_additional_suffix")?
}

# origin: uutils test_split::test_split_additional_suffix_hyphen_value
test test_uu_split_split_additional_suffix_hyphen_value { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "split_additional_suffix", 2000)
  let r = uu.invoke(s, "split", ["--additional-suffix", "-300", "split_additional_suffix"], timeout: 5s)?
  uu.succeeds(r)
  let names = matching(s, rx"x[[:alpha:]][[:alpha:]]-300$")?
  assert names.len() == 2
  assert collate(s, rx"x[[:alpha:]][[:alpha:]]-300$")? == uu.read(s, "split_additional_suffix")?
}

# origin: uutils test_split::test_split_both_lines_and_obs_lines_standalone
test test_uu_split_split_both_lines_and_obs_lines_standalone { |ctx|
  let s = split_scene(ctx)?
  uu.touch(s, "file")
  let r1 = uu.invoke(s, "split", ["-l", "2", "-2", "file"], timeout: 5s)?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "split: cannot split in more than one way\n")
  let r2 = uu.invoke(s, "split", ["--lines", "2", "-2", "file"], timeout: 5s)?
  uu.fails_with_code(r2, 1)
  uu.stderr_contains(r2, "split: cannot split in more than one way\n")
}

# origin: uutils test_split::test_split_by_bytes_short_concatenated_with_value
test test_uu_split_split_by_bytes_short_concatenated_with_value { |ctx|
  let s = split_scene(ctx)?
  seeded_bytes(s, "split_by_bytes_short_concatenated_with_value", 10000)
  let r = uu.invoke(s, "split", ["-b1000", "split_by_bytes_short_concatenated_with_value"], timeout: 5s)?
  uu.succeeds(r)
  let names = matching(s, rx"x[[:alpha:]][[:alpha:]]$")?
  assert names.len() == 10
  for name in names { assert uu.size(s, name)? == 1000 }
  assert collate(s, rx"x[[:alpha:]][[:alpha:]]$")? == uu.read(s, "split_by_bytes_short_concatenated_with_value")?
}

# origin: uutils test_split::test_split_bytes_prime_part_size
test test_uu_split_split_bytes_prime_part_size { |ctx|
  let s = split_scene(ctx)?
  seeded_bytes(s, "test_split_bytes_prime_part_size", 10000)
  let r = uu.invoke(s, "split", ["-b", "1753", "test_split_bytes_prime_part_size", "b"], timeout: 5s)?
  uu.succeeds(r)
  let names = matching(s, rx"b[[:alpha:]][[:alpha:]]$")?
  assert names.len() == 6
  for i in range(5) { assert uu.size(s, names[i])? == 1753 }
  assert uu.size(s, names[5])? == 1235
  assert collate(s, rx"b[[:alpha:]][[:alpha:]]$")? == uu.read(s, "test_split_bytes_prime_part_size")?
}

# origin: uutils test_split::test_split_chunks_by_line_or_round_robin_zero_chunks
test test_uu_split_split_chunks_by_line_or_round_robin_zero_chunks { |ctx|
  let s = split_scene(ctx)?
  uu.write(s, "file", "a\nb\nc\nd\n")
  let r1 = uu.invoke(s, "split", ["-n", "l/0", "file"], timeout: 5s)?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "split: invalid number of chunks: '0'\n")
  let r2 = uu.invoke(s, "split", ["-n", "r/0", "file"], timeout: 5s)?
  uu.fails_with_code(r2, 1)
  uu.stderr_only(r2, "split: invalid number of chunks: '0'\n")
}

# origin: uutils test_split::test_split_default
test test_uu_split_split_default { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "split_default", 2000)
  let r = uu.invoke(s, "split", ["split_default"], timeout: 5s)?
  uu.succeeds(r)
  let names = matching(s, rx"x[[:alpha:]][[:alpha:]]$")?
  assert names.len() == 2
  assert collate(s, rx"x[[:alpha:]][[:alpha:]]$")? == uu.read(s, "split_default")?
}

# origin: uutils test_split::test_split_default_with_io_blksize
test test_uu_split_split_default_with_io_blksize { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "split_default_with_io_blksize", 2000)
  let r = uu.invoke(s, "split", ["split_default_with_io_blksize", "---io-blksize", "2M"], timeout: 5s)?
  uu.succeeds(r)
  let names = matching(s, rx"x[[:alpha:]][[:alpha:]]$")?
  assert names.len() == 2
  assert collate(s, rx"x[[:alpha:]][[:alpha:]]$")? == uu.read(s, "split_default_with_io_blksize")?
}

# origin: uutils test_split::test_split_directory_already_exists
test test_uu_split_split_directory_already_exists { |ctx|
  let s = split_scene(ctx)?
  for args in [[], ["-b", "1"], ["-l", "1"], ["-C", "1"]] {
    let s = split_scene(ctx)?
    uu.mkdir(s, "xaa")
    let r = uu.invoke(s, "split", args, stdin: b"data\n")?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_is(r, "split: xaa: Is a directory\n")
    assert uu.dir_exists(s, "xaa")?
    assert !uu.exists(s, "xab")?
  }
}

# origin: uutils test_split::test_split_invalid_bytes_size
test test_uu_split_split_invalid_bytes_size { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["-b", "1024W"], timeout: 5s)?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "split: invalid number of bytes: '1024W'\n")
}

# origin: uutils test_split::test_split_invalid_input
test test_uu_split_split_invalid_input { |ctx|
  let s = split_scene(ctx)?
  uu.touch(s, "file")
  let r1 = uu.invoke(s, "split", ["--lines", "0", "file"], timeout: 5s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "split: invalid number of lines: '0'")
  let r2 = uu.invoke(s, "split", ["-C", "0", "file"], timeout: 5s)?
  uu.fails(r2)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "split: invalid number of lines: '0'")
  let r3 = uu.invoke(s, "split", ["-b", "0", "file"], timeout: 5s)?
  uu.fails(r3)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "split: invalid number of bytes: '0'")
  let r4 = uu.invoke(s, "split", ["-n", "0", "file"], timeout: 5s)?
  uu.fails(r4)
  uu.no_stdout(r4)
  uu.stderr_contains(r4, "split: invalid number of chunks: '0'")
}

# origin: uutils test_split::test_split_invalid_io_blksize
test test_uu_split_split_invalid_io_blksize { |ctx|
  let s = split_scene(ctx)?
  let r1 = uu.invoke(s, "split", ["---io-blksize=XYZ", "threebytes.txt"], timeout: 5s)?
  uu.fails(r1)
  uu.stderr_only(r1, "split: invalid IO block size: 'XYZ'\n")
  let r2 = uu.invoke(s, "split", ["---io-blksize=5000000000", "threebytes.txt"], timeout: 5s)?
  uu.fails(r2)
  uu.stderr_only(r2, "split: invalid IO block size: '5000000000': Value too large for defined data type\n")
}

# origin: uutils test_split::test_split_lines_interfere_with_io_buf_capacity
test test_uu_split_split_lines_interfere_with_io_buf_capacity { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "split_lines_interfere_with_io_buf_capacity", 2, 8190)
  let r = uu.invoke(s, "split", ["-l", "1", "split_lines_interfere_with_io_buf_capacity"])?
  uu.succeeds(r)
  assert uu.read(s, "xaa")?.len() == 8191
  assert uu.read(s, "xab")?.len() == 8191
}

# origin: uutils test_split::test_split_lines_number
test test_uu_split_split_lines_number { |ctx|
  let s = split_scene(ctx)?
  uu.touch(s, "file")
  let r1 = uu.invoke(s, "split", ["--lines", "2", "file"], timeout: 5s)?
  uu.succeeds(r1)
  uu.no_output(r1)
  let r2 = uu.invoke(s, "split", ["--lines", "0", "file"], timeout: 5s)?
  uu.fails_with_code(r2, 1)
  uu.stderr_only(r2, "split: invalid number of lines: '0'\n")
  let r3 = uu.invoke(s, "split", ["-0", "file"], timeout: 5s)?
  uu.fails_with_code(r3, 1)
  uu.stderr_only(r3, "split: invalid number of lines: '0'\nTry 'split --help' for more information.\n")
  let r4 = uu.invoke(s, "split", ["--lines", "2fb", "file"], timeout: 5s)?
  uu.fails_with_code(r4, 1)
  uu.stderr_only(r4, "split: invalid number of lines: '2fb'\n")
  let r5 = uu.invoke(s, "split", ["--lines", "file"], timeout: 5s)?
  uu.fails_with_code(r5, 1)
  uu.stderr_only(r5, "split: invalid number of lines: 'file'\n")
}

# origin: uutils test_split::test_split_lines_short_concatenated_with_value
test test_uu_split_split_lines_short_concatenated_with_value { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "split_num_prefixed_chunks_by_lines", 10000)
  let r = uu.invoke(s, "split", ["-l1000", "split_num_prefixed_chunks_by_lines"], timeout: 5s)?
  uu.succeeds(r)
  let names = matching(s, rx"x[[:alpha:]][[:alpha:]]$")?
  assert names.len() == 10
  assert collate(s, rx"x[[:alpha:]][[:alpha:]]$")? == uu.read(s, "split_num_prefixed_chunks_by_lines")?
}

# origin: uutils test_split::test_split_multiple_obs_lines_standalone
test test_uu_split_split_multiple_obs_lines_standalone { |ctx|
  let s = split_scene(ctx)?
  random_lines(s, "multiple-obs-lines", 400)
  let r = uu.invoke(s, "split", ["-3000", "-200", "multiple-obs-lines"], timeout: 5s)?
  uu.succeeds(r)
  uu.no_output(r)
  let names = matching(s, rx"x[[:alpha:]][[:alpha:]]$")?
  assert names.len() == 2
  assert collate(s, rx"x[[:alpha:]][[:alpha:]]$")? == uu.read(s, "multiple-obs-lines")?
}
