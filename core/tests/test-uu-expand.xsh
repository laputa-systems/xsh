##! Native ports of the uutils expand integration tests.
use support.uu as uu

# origin: uutils test_expand::test_invalid_arg
test test_uu_expand_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_expand::test_with_tab
test test_uu_expand_with_tab { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "with-tab.txt", "with-tab.txt")?
  let r = uu.invoke(s, "expand", ["with-tab.txt"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "        ")
  assert !("\t" in r.stdout.utf8()?)
}

# origin: uutils test_expand::test_with_trailing_tab
test test_uu_expand_with_trailing_tab { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "with-trailing-tab.txt", "with-trailing-tab.txt")?
  let r = uu.invoke(s, "expand", ["with-trailing-tab.txt"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "with tabs=>  ")
  assert !("\t" in r.stdout.utf8()?)
}

# origin: uutils test_expand::test_with_trailing_tab_i
test test_uu_expand_with_trailing_tab_i { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "with-trailing-tab.txt", "with-trailing-tab.txt")?
  let r = uu.invoke(s, "expand", ["with-trailing-tab.txt", "-i"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "        // with tabs=>\t")
}

# origin: uutils test_expand::test_with_tab_size
test test_uu_expand_with_tab_size { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "with-tab.txt", "with-tab.txt")?
  let r = uu.invoke(s, "expand", ["with-tab.txt", "--tabs=10"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "          ")
}

# origin: uutils test_expand::test_with_space
test test_uu_expand_with_space { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "with-spaces.txt", "with-spaces.txt")?
  let r = uu.invoke(s, "expand", ["with-spaces.txt"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "    return")
}

# origin: uutils test_expand::test_with_multiple_files
test test_uu_expand_with_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "with-tab.txt", "with-tab.txt")?
  uu.fixture(s, "expand", "with-spaces.txt", "with-spaces.txt")?
  let r = uu.invoke(s, "expand", ["with-spaces.txt", "with-tab.txt"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "    return")
  uu.stdout_contains(r, "        ")
}

# origin: uutils test_expand::test_multiple_tabs_args
test test_uu_expand_multiple_tabs_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=3", "--tabs=6", "--tabs=9"], stdin: bytes.from_text("a\tb\tc\td\te"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a  b  c  d e")
}

# origin: uutils test_expand::test_tabs_empty_string
test test_uu_expand_tabs_empty_string { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs", ""], stdin: bytes.from_text("a\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a       b       c")
}

# origin: uutils test_expand::test_tabs_comma_only
test test_uu_expand_tabs_comma_only { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs", ","], stdin: bytes.from_text("a\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a       b       c")
}

# origin: uutils test_expand::test_tabs_space_only
test test_uu_expand_tabs_space_only { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs", " "], stdin: bytes.from_text("a\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a       b       c")
}

# origin: uutils test_expand::test_tabs_slash
test test_uu_expand_tabs_slash { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs", "/"], stdin: bytes.from_text("a\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a       b       c")
}

# origin: uutils test_expand::test_tabs_plus
test test_uu_expand_tabs_plus { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs", "+"], stdin: bytes.from_text("a\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a       b       c")
}

# origin: uutils test_expand::test_tabs_trailing_slash
test test_uu_expand_tabs_trailing_slash { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=1,/5"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, " a   b    c")
}

# origin: uutils test_expand::test_tabs_trailing_slash_long_columns
test test_uu_expand_tabs_trailing_slash_long_columns { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=1,/3"], stdin: bytes.from_text("\taaaa\tbbbb\tcccc"))?
  uu.succeeds(r)
  uu.stdout_is(r, " aaaa bbbb  cccc")
}

# origin: uutils test_expand::test_tabs_trailing_plus
test test_uu_expand_tabs_trailing_plus { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=1,+5"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, " a    b    c")
}

# origin: uutils test_expand::test_tabs_trailing_plus_long_columns
test test_uu_expand_tabs_trailing_plus_long_columns { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=1,+3"], stdin: bytes.from_text("\taaaa\tbbbb\tcccc"))?
  uu.succeeds(r)
  uu.stdout_is(r, " aaaa  bbbb  cccc")
}

# origin: uutils test_expand::test_tabs_must_be_ascending
test test_uu_expand_tabs_must_be_ascending { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=1,1"])?
  uu.fails(r)
  uu.stderr_contains(r, "tab sizes must be ascending")
}

# origin: uutils test_expand::test_tabs_cannot_be_zero
test test_uu_expand_tabs_cannot_be_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=0"])?
  uu.fails(r)
  uu.stderr_contains(r, "tab size cannot be 0")
}

# origin: uutils test_expand::test_tabs_keep_last_trailing_specifier
test test_uu_expand_tabs_keep_last_trailing_specifier { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=1,+/+/5"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, " a   b    c")
}

# origin: uutils test_expand::test_tabs_comma_separated_no_numbers
test test_uu_expand_tabs_comma_separated_no_numbers { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=+,/,+,/"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "        a       b       c")
}

# origin: uutils test_expand::test_tabs_shortcut
test test_uu_expand_tabs_shortcut { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["-2", "-5", "-7"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "  a  b c")
}

# origin: uutils test_expand::test_comma_separated_tabs_shortcut
test test_uu_expand_comma_separated_tabs_shortcut { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["-2,5", "-7"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "  a  b c")
}

# origin: uutils test_expand::test_tabs_and_tabs_shortcut_mixed
test test_uu_expand_tabs_and_tabs_shortcut_mixed { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["-2", "--tabs=5", "-7"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "  a  b c")
}

# origin: uutils test_expand::test_ignore_initial_plus
test test_uu_expand_ignore_initial_plus { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=+3"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "   a  b  c")
}

# origin: uutils test_expand::test_ignore_initial_pluses
test test_uu_expand_ignore_initial_pluses { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=++3"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "   a  b  c")
}

# origin: uutils test_expand::test_ignore_initial_slash
test test_uu_expand_ignore_initial_slash { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=/3"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "   a  b  c")
}

# origin: uutils test_expand::test_ignore_initial_slashes
test test_uu_expand_ignore_initial_slashes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=//3"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "   a  b  c")
}

# origin: uutils test_expand::test_ignore_initial_plus_slash_combination
test test_uu_expand_ignore_initial_plus_slash_combination { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=+/3"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "   a  b  c")
}

# origin: uutils test_expand::test_comma_with_plus_1
test test_uu_expand_comma_with_plus_1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=3,+6"], stdin: bytes.from_text("\t111\t222\t333"))?
  uu.succeeds(r)
  uu.stdout_is(r, "   111   222   333")
}

# origin: uutils test_expand::test_comma_with_plus_2
test test_uu_expand_comma_with_plus_2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=1,+5"], stdin: bytes.from_text("\ta\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, " a    b    c")
}

# origin: uutils test_expand::test_comma_with_plus_3
test test_uu_expand_comma_with_plus_3 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=2,+5"], stdin: bytes.from_text("a\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a b    c")
}

# origin: uutils test_expand::test_comma_with_plus_4
test test_uu_expand_comma_with_plus_4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["--tabs=1,3,+5"], stdin: bytes.from_text("a\tb\tc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a  b    c")
}

# origin: uutils test_expand::test_args_override
test test_uu_expand_args_override { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "with-trailing-tab.txt", "with-trailing-tab.txt")?
  let r = uu.invoke(s, "expand", ["-i", "-i", "with-trailing-tab.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "// !note: file contains significant whitespace\n// * indentation uses <TAB> characters\nint main() {\n        // * next line has both a leading & trailing tab\n        // with tabs=>\t\n        return 0;\n}\n")
}

# origin: uutils test_expand::test_expand_directory
test test_uu_expand_expand_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expand", ["."])?
  uu.fails(r)
  uu.stderr_contains(r, "expand: .: Is a directory")
}

# origin: uutils test_expand::test_nonexisting_file
test test_uu_expand_nonexisting_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "with-spaces.txt", "with-spaces.txt")?
  let r = uu.invoke(s, "expand", ["nonexistent", "with-spaces.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "expand: nonexistent: No such file or directory")
  assert "// !note: file contains significant whitespace" in r.stdout.utf8()?.lines()
}

# origin: uutils test_expand::test_buffered_reads_new_line_no_tabs_in_first_chunk
test test_uu_expand_buffered_reads_new_line_no_tabs_in_first_chunk { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "expand", "new_line_in_chunk.txt", "new_line_in_chunk.txt")?
  let r = uu.invoke(s, "expand", ["new_line_in_chunk.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/expand/new_line_in_chunk_expected.txt".read_bytes()?)
}

# origin: uutils test_expand::test_tabs
test test_uu_expand_tabs { |ctx|
  let s = uu.scene(ctx)?
  for list in ["3 6 9", "3,6,9", "3\t6\t9", ", \t3,\t6 9,"] {
    let r = uu.invoke(s, "expand", ["--tabs", list], stdin: b"a\tb\tc\td\te")?
    uu.succeeds(r)
    uu.stdout_is(r, "a  b  c  d e")
  }
}

# origin: uutils test_expand::test_tabs_with_specifier_not_at_start
test test_uu_expand_tabs_with_specifier_not_at_start { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "expand", ["--tabs=1/"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "'/' specifier not at start of number: '/'")
  let r1 = uu.invoke(s, "expand", ["--tabs=1/2"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "'/' specifier not at start of number: '/2'")
  let r2 = uu.invoke(s, "expand", ["--tabs=1+"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "'+' specifier not at start of number: '+'")
  let r3 = uu.invoke(s, "expand", ["--tabs=1+2"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "'+' specifier not at start of number: '+2'")
}

# origin: uutils test_expand::test_tabs_with_specifier_only_allowed_with_last_value
test test_uu_expand_tabs_with_specifier_only_allowed_with_last_value { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "expand", ["--tabs=/1,2,3"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "'/' specifier only allowed with the last value")
  let r1 = uu.invoke(s, "expand", ["--tabs=1,/2,3"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "'/' specifier only allowed with the last value")
  let r2 = uu.invoke(s, "expand", ["--tabs=+1,2,3"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "'+' specifier only allowed with the last value")
  let r3 = uu.invoke(s, "expand", ["--tabs=1,+2,3"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "'+' specifier only allowed with the last value")
  uu.succeeds(uu.invoke(s, "expand", ["--tabs=1,2,/3"])?)
  uu.succeeds(uu.invoke(s, "expand", ["--tabs=1,2,+3"])?)
}

# origin: uutils test_expand::test_tabs_with_invalid_chars
test test_uu_expand_tabs_with_invalid_chars { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "expand", ["--tabs=x"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "tab size contains invalid character(s): 'x'")
  let r1 = uu.invoke(s, "expand", ["--tabs=1x2"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "tab size contains invalid character(s): 'x2'")
}

# origin: uutils test_expand::test_tabs_with_too_large_size
test test_uu_expand_tabs_with_too_large_size { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "expand", ["--tabs=340282366920938463463374607431768211455"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "tab stop is too large '340282366920938463463374607431768211455'")
}

# origin: uutils test_expand::test_expand_non_utf8_paths
test test_uu_expand_expand_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let filename = uu.at_bytes(s, b"\xff\xfe")?
  filename.write(b"hello\tworld\ntest\tline\n")?
  let r = uu.invoke_paths(s, "expand", [filename])?
  uu.succeeds(r)
  uu.stdout_is(r, "hello   world\ntest    line\n")
}

# GNU expand counts encoded bytes as columns under LC_ALL=C.
# origin: uutils test_expand::test_wide_multibyte_char_width
test test_uu_expand_wide_multibyte_char_width { |ctx|
  let s = uu.scene(ctx)?
  for case in [{character: "中", expected: "中     |"}, {character: "😀", expected: "😀    |"}] {
    let r = uu.invoke(s, "expand", [], stdin: bytes.from_text(f"{case.character}\t|"))?
    uu.succeeds(r)
    uu.stdout_is(r, case.expected)
  }
}
