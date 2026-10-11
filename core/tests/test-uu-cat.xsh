##! Transcribed from the uutils coreutils cat integration tests.

use support.uu as uu

# origin: uutils test_cat::test_output_simple
test test_uu_cat_output_simple { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  let r = uu.invoke(s, "cat", ["alpha.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "abcde\nfghij\nklmno\npqrst\nuvwxyz\n")
}

# origin: uutils test_cat::test_numbered_lines_no_trailing_newline
test test_uu_cat_numbered_lines_no_trailing_newline { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "nonewline.txt", "nonewline.txt")?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  let r = uu.invoke(s, "cat", ["nonewline.txt", "alpha.txt", "-n"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "     1\ttext without a trailing newlineabcde\n     2\tfghij\n     3\tklmno\n     4\tpqrst\n     5\tuvwxyz\n")
}

# origin: uutils test_cat::test_numbered_lines_with_crlf
test test_uu_cat_numbered_lines_with_crlf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-n"], stdin: b"Hello\r\nWorld")?
  uu.succeeds(r)
  uu.stdout_only(r, "     1\tHello\r\n     2\tWorld")
}

# origin: uutils test_cat::test_show_ends_crlf
test test_uu_cat_show_ends_crlf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-E"], stdin: b"a\nb\r\n\rc\n\r\n\r")?
  uu.succeeds(r)
  uu.stdout_only(r, "a$\nb^M$\n\rc$\n^M$\n\r")
}

# origin: uutils test_cat::test_stdin_nonprinting_and_endofline
test test_uu_cat_stdin_nonprinting_and_endofline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-e"], stdin: b"\t\0\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t^@$\n")
}

# origin: uutils test_cat::test_stdin_nonprinting_and_endofline_repeated
test test_uu_cat_stdin_nonprinting_and_endofline_repeated { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-ee", "-e"], stdin: b"\t\0\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t^@$\n")
}

# origin: uutils test_cat::test_stdin_nonprinting_and_tabs
test test_uu_cat_stdin_nonprinting_and_tabs { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-t"], stdin: b"\t\0\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "^I^@\n")
}

# origin: uutils test_cat::test_stdin_nonprinting_and_tabs_repeated
test test_uu_cat_stdin_nonprinting_and_tabs_repeated { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-tt", "-t"], stdin: b"\t\0\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "^I^@\n")
}

# origin: uutils test_cat::test_stdin_tabs_no_newline
test test_uu_cat_stdin_tabs_no_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-T"], stdin: b"\ta")?
  uu.succeeds(r)
  uu.stdout_only(r, "^Ia")
}

# origin: uutils test_cat::test_non_blank_overrides_number_even_when_present
test test_uu_cat_non_blank_overrides_number_even_when_present { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-n", "-b", "-n"], stdin: b"\na\nb\n\n\nc")?
  uu.succeeds(r)
  uu.stdout_only(r, "\n     1\ta\n     2\tb\n\n\n     3\tc")
}

# origin: uutils test_cat::test_output_multi_files_print_all_chars
test test_uu_cat_output_multi_files_print_all_chars { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  uu.fixture(s, "cat", "256.txt", "256.txt")?
  let r = uu.invoke(s, "cat", ["alpha.txt", "256.txt", "-A", "-n"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "     1\tabcde$\n     2\tfghij$\n     3\tklmno$\n     4\tpqrst$\n     5\tuvwxyz$\n     6\t^@^A^B^C^D^E^F^G^H^I$\n     7\t^K^L^M^N^O^P^Q^R^S^T^U^V^W^X^Y^Z^[^\\^]^^^_ !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~^?M-^@M-^AM-^BM-^CM-^DM-^EM-^FM-^GM-^HM-^IM-^JM-^KM-^LM-^MM-^NM-^OM-^PM-^QM-^RM-^SM-^TM-^UM-^VM-^WM-^XM-^YM-^ZM-^[M-^\\M-^]M-^^M-^_M- M-!M-\"M-#M-$M-%M-&M-'M-(M-)M-*M-+M-,M--M-.M-/M-0M-1M-2M-3M-4M-5M-6M-7M-8M-9M-:M-;M-<M-=M->M-?M-@M-AM-BM-CM-DM-EM-FM-GM-HM-IM-JM-KM-LM-MM-NM-OM-PM-QM-RM-SM-TM-UM-VM-WM-XM-YM-ZM-[M-\\M-]M-^M-_M-`M-aM-bM-cM-dM-eM-fM-gM-hM-iM-jM-kM-lM-mM-nM-oM-pM-qM-rM-sM-tM-uM-vM-wM-xM-yM-zM-{M-|M-}M-~M-^?")
}

# origin: uutils test_cat::test_output_multi_files_print_all_chars_repeated
test test_uu_cat_output_multi_files_print_all_chars_repeated { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  uu.fixture(s, "cat", "256.txt", "256.txt")?
  let r = uu.invoke(s, "cat", ["alpha.txt", "256.txt", "-A", "-n", "-A", "-n"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "     1\tabcde$\n     2\tfghij$\n     3\tklmno$\n     4\tpqrst$\n     5\tuvwxyz$\n     6\t^@^A^B^C^D^E^F^G^H^I$\n     7\t^K^L^M^N^O^P^Q^R^S^T^U^V^W^X^Y^Z^[^\\^]^^^_ !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~^?M-^@M-^AM-^BM-^CM-^DM-^EM-^FM-^GM-^HM-^IM-^JM-^KM-^LM-^MM-^NM-^OM-^PM-^QM-^RM-^SM-^TM-^UM-^VM-^WM-^XM-^YM-^ZM-^[M-^\\M-^]M-^^M-^_M- M-!M-\"M-#M-$M-%M-&M-'M-(M-)M-*M-+M-,M--M-.M-/M-0M-1M-2M-3M-4M-5M-6M-7M-8M-9M-:M-;M-<M-=M->M-?M-@M-AM-BM-CM-DM-EM-FM-GM-HM-IM-JM-KM-LM-MM-NM-OM-PM-QM-RM-SM-TM-UM-VM-WM-XM-YM-ZM-[M-\\M-]M-^M-_M-`M-aM-bM-cM-dM-eM-fM-gM-hM-iM-jM-kM-lM-mM-nM-oM-pM-qM-rM-sM-tM-uM-vM-wM-xM-yM-zM-{M-|M-}M-~M-^?")
}

# origin: uutils test_cat::test_stdin_show_nonprinting
test test_uu_cat_stdin_show_nonprinting { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-v", "-vv", "--show-nonprinting", "--show-non"] {
    let r = uu.invoke(s, "cat", [option], stdin: b"\t\0\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "\t^@\n")
  }
}

# origin: uutils test_cat::test_stdin_show_tabs
test test_uu_cat_stdin_show_tabs { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-T", "-TT", "--show-tabs", "--show-ta"] {
    let r = uu.invoke(s, "cat", [option], stdin: b"\t\0\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "^I\0\n")
  }
}

# origin: uutils test_cat::test_stdin_show_ends
test test_uu_cat_stdin_show_ends { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-E", "-EE", "--show-ends", "--show-e"] {
    let r = uu.invoke(s, "cat", [option, "-"], stdin: b"\t\0\n\t")?
    uu.succeeds(r)
    uu.stdout_only(r, "\t\0$\n\t")
  }
}

# origin: uutils test_cat::test_stdin_show_all
test test_uu_cat_stdin_show_all { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-A", "--show-all", "--show-a"] {
    let r = uu.invoke(s, "cat", [option], stdin: b"\t\0\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "^I^@$\n")
  }
}

# origin: uutils test_cat::test_stdin_squeeze_blank
test test_uu_cat_stdin_squeeze_blank { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-s", "--squeeze-blank", "--squeeze"] {
    let r = uu.invoke(s, "cat", [option], stdin: b"\n\na\n\n\n\n\nb\n\n\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "\na\n\nb\n\n")
  }
}

# origin: uutils test_cat::test_stdin_number_non_blank
test test_uu_cat_stdin_number_non_blank { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-b", "-bb", "--number-nonblank", "--number-non"] {
    let r = uu.invoke(s, "cat", [option, "-"], stdin: b"\na\nb\n\n\nc")?
    uu.succeeds(r)
    uu.stdout_only(r, "\n     1\ta\n     2\tb\n\n\n     3\tc")
  }
}

# origin: uutils test_cat::test_non_blank_overrides_number
test test_uu_cat_non_blank_overrides_number { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-b", "--number-nonblank"] {
    let r = uu.invoke(s, "cat", [option, "-"], stdin: b"\na\nb\n\n\nc")?
    uu.succeeds(r)
    uu.stdout_only(r, "\n     1\ta\n     2\tb\n\n\n     3\tc")
  }
}

# origin: uutils test_cat::test_squeeze_blank_before_numbering
test test_uu_cat_squeeze_blank_before_numbering { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-s", "--squeeze-blank"] {
    let r = uu.invoke(s, "cat", [option, "-n", "-"], stdin: b"a\n\n\nb")?
    uu.succeeds(r)
    uu.stdout_only(r, "     1\ta\n     2\t\n     3\tb")
  }
}

# origin: uutils test_cat::test_u_ignored
test test_uu_cat_u_ignored { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-u", "-uu"] {
    let r = uu.invoke(s, "cat", [option], stdin: b"hello")?
    uu.succeeds(r)
    uu.stdout_only(r, "hello")
  }
}

# origin: uutils test_cat::test_squeeze_all_files
test test_uu_cat_squeeze_all_files { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input1", "a\n\n")?
  uu.write(s, "input2", "\n\nb")?
  let r = uu.invoke(s, "cat", ["input1", "input2", "-s"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n\nb")
}

# origin: uutils test_cat::test_squeeze_all_files_repeated
test test_uu_cat_squeeze_all_files_repeated { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input1", "a\n\n")?
  uu.write(s, "input2", "\n\nb")?
  let r = uu.invoke(s, "cat", ["-s", "input1", "input2", "-s"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n\nb")
}

# origin: uutils test_cat::test_no_options
test test_uu_cat_no_options { |ctx|
  let s = uu.scene(ctx)?
  for fixture in ["empty.txt", "alpha.txt", "nonewline.txt"] {
    uu.fixture(s, "cat", fixture, fixture)?
    let expected = uu.read(s, fixture)?
    let file = uu.invoke(s, "cat", [fixture])?
    uu.succeeds(file)
    uu.stdout_is_bytes(file, expected)
    let stdin = uu.invoke(s, "cat", [], stdin: expected)?
    uu.succeeds(stdin)
    uu.stdout_is_bytes(stdin, expected)
  }
}

# origin: uutils test_cat::test_no_options_big_input
test test_uu_cat_no_options_big_input { |ctx|
  let s = uu.scene(ctx)?
  for n in [0, 1, 42, 16377, 16383, 16384, 16385, 16387, 32768, 65536, 81920, 98304, 114688, 131072] {
    let data = bytes.concat([b"a" for _ in range(n)])
    let copy = data
    assert data.len() == copy.len()
    let r = uu.invoke(s, "cat", [], stdin: data)?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, copy)
  }
}

# origin: uutils test_cat::test_directory
test test_uu_cat_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "test_directory")?
  let r = uu.invoke(s, "cat", ["test_directory"])?
  uu.fails(r)
  uu.stderr_is(r, "cat: test_directory: Is a directory\n")
}

# origin: uutils test_cat::test_directory_and_file
test test_uu_cat_directory_and_file { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "test_directory2")?
  for fixture in ["empty.txt", "alpha.txt", "nonewline.txt"] {
    uu.fixture(s, "cat", fixture, fixture)?
    let r = uu.invoke(s, "cat", ["test_directory2", fixture])?
    uu.fails(r)
    uu.stderr_is(r, "cat: test_directory2: Is a directory\n")
    uu.stdout_is_bytes(r, uu.read(s, fixture)?)
  }
}

# origin: uutils test_cat::test_three_directories_and_file_and_stdin
test test_uu_cat_three_directories_and_file_and_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  uu.fixture(s, "cat", "nonewline.txt", "nonewline.txt")?
  uu.fixture(s, "cat", "three_directories_and_file_and_stdin.stderr.expected", "three_directories_and_file_and_stdin.stderr.expected")?
  uu.mkdir(s, "test_directory3/test_directory4")?
  uu.mkdir(s, "test_directory3/test_directory5")?
  let r = uu.invoke(s, "cat", ["test_directory3/test_directory4", "alpha.txt", "-", "file_which_does_not_exist.txt", "nonewline.txt", "test_directory3/test_directory5", "test_directory3/../test_directory3/test_directory5", "test_directory3"], stdin: b"stdout bytes")?
  uu.fails(r)
  uu.stderr_is_bytes(r, uu.read(s, "three_directories_and_file_and_stdin.stderr.expected")?)
  uu.stdout_is(r, "abcde\nfghij\nklmno\npqrst\nuvwxyz\nstdout bytestext without a trailing newline")
}

# origin: uutils test_cat::test_piped_to_regular_file
test test_uu_cat_piped_to_regular_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  for append in [true, false] {
    let r = uu.invoke(s, "cat", [], stdin: uu.read(s, "alpha.txt")?, stdout: uu.at(s, "file.txt"), stdout_append: append)?
    uu.succeeds(r)
    uu.file_is(s, "file.txt", "abcde\nfghij\nklmno\npqrst\nuvwxyz\n")
    uu.remove(s, "file.txt")?
  }
}

# origin: uutils test_cat::test_piped_to_dev_null
test test_uu_cat_piped_to_dev_null { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  for append in [true, false] {
    let r = uu.invoke(s, "cat", [], stdin: uu.read(s, "alpha.txt")?, stdout: p"/dev/null", stdout_append: append)?
    uu.succeeds(r)
  }
}

# origin: uutils test_cat::test_piped_to_dev_full
test test_uu_cat_piped_to_dev_full { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  for append in [true, false] {
    let r = uu.invoke(s, "cat", [], stdin: uu.read(s, "alpha.txt")?, stdout: p"/dev/full", stdout_append: append)?
    uu.fails(r)
    uu.stderr_contains(r, "No space left on device")
  }
}

# origin: uutils test_cat::test_domain_socket
test test_uu_cat_domain_socket { |ctx|
  let s = uu.scene(ctx)?
  let c = linux.net_constants()
  let socket = linux.socket(c.AF_UNIX, c.SOCK_STREAM)?
  defer unix.close_fd(socket)
  linux.bind(socket, {family: "unix", address: uu.at(s, "sock").display()})?
  linux.listen(socket, 128)?
  let r = uu.invoke(s, "cat", [uu.at(s, "sock").display()])?
  uu.fails(r)
  uu.stderr_contains(r, "No such device or address")
}

# origin: uutils test_cat::test_error_loop
test test_uu_cat_error_loop { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "2", "1")?
  uu.symlink(s, "3", "2")?
  uu.symlink(s, "1", "3")?
  let r = uu.invoke(s, "cat", ["1"])?
  uu.fails(r)
  uu.stderr_is(r, "cat: 1: Too many levels of symbolic links\n")
}

# origin: uutils test_cat::test_exit_code_is_one_regardless_of_error_count
test test_uu_cat_exit_code_is_one_regardless_of_error_count { |ctx|
  let s = uu.scene(ctx)?
  let missing = [f"missing-{i}" for i in range(256)]
  let r = uu.invoke(s, "cat", missing)?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_cat::test_cat_non_utf8_paths
test test_uu_cat_cat_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let input_path = uu.at_bytes(s, b"test_\xff\xfe.txt")?
  input_path.write("Hello, non-UTF-8 world!\n")?
  let r = uu.invoke_paths(s, "cat", [input_path])?
  uu.succeeds(r)
  uu.stdout_is(r, "Hello, non-UTF-8 world!\n")
}

# origin: uutils test_cat::test_appending_same_input_output
test test_uu_cat_appending_same_input_output { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "content")?
  let r = uu.invoke_from_path(s, "cat", [], stdin: uu.at(s, "foo"), stdout: uu.at(s, "foo"), stdout_append: true)?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "input file is output file")
}

# origin: uutils test_cat::test_write_to_self_empty
test test_uu_cat_write_to_self_empty { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file.txt")?
  let r = uu.invoke(s, "cat", ["file.txt"], stdout: uu.at(s, "file.txt"), stdout_append: true)?
  uu.succeeds(r)
}

# origin: uutils test_cat::test_write_to_self
test test_uu_cat_write_to_self { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "first_file")?
  uu.write(s, "second_file", "second_file_content.")?
  uu.append(s, "first_file", "first_file_content.")?
  let r = uu.invoke(s, "cat", ["first_file", "first_file", "second_file"], stdout: uu.at(s, "first_file"), stdout_append: true)?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "cat: first_file: input file is output file\ncat: first_file: input file is output file\n")
  uu.file_is(s, "first_file", "first_file_content.second_file_content.")
}

# origin: uutils test_cat::test_write_fast_fallthrough_uses_flush
test test_uu_cat_write_fast_fallthrough_uses_flush { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cat", "alpha.txt", "alpha.txt")?
  let cmdline = p"/proc/1/cmdline".read_bytes()?
  let r = uu.invoke(s, "cat", ["/proc/1/cmdline", "alpha.txt"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.concat([cmdline, uu.read(s, "alpha.txt")?]))
}

# origin: uutils test_cat::test_write_fast_read_error
test test_uu_cat_write_fast_read_error { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "content")?
  uu.set_mode(s, "foo", 0o000)?
  let r = uu.invoke(s, "cat", ["foo"])?
  uu.fails(r)
  uu.stderr_contains(r, "Permission denied")
}

# origin: uutils test_cat::test_version_help_dev_full
test test_uu_cat_version_help_dev_full { |ctx|
  let s = uu.scene(ctx)?
  for option in ["--version", "--help"] {
    let r = uu.invoke(s, "cat", [option], stdout: p"/dev/full")?
    uu.fails(r)
    uu.stderr_contains(r, "No space left on device")
  }
}


# origin: uutils test_cat::test_fifo_symlink
test test_uu_cat_fifo_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.mkfifo(s, "dir/pipe")?
  assert fs.stat(uu.at(s, "dir/pipe"))?.kind == "fifo"
  uu.symlink(s, "dir/pipe", "sympipe")?
  uu.fixture(s, "cat", "fifo-writer.xsh", "fifo-writer.xsh")?
  let writer_plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, uu.at(s, "fifo-writer.xsh"), uu.at(s, "dir/pipe")], timeout: 5s)
  let writer = spawn writer_plan?
  let data = bytes.concat([b"a" for _ in range(131072)])
  let copy = data
  let r = uu.invoke(s, "cat", ["sympipe"])?
  uu.stdout_only_bytes(r, copy)
  assert (wait writer?).exited_with(0)
}
