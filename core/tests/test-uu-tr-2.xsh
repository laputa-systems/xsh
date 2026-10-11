##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_tr.rs.
##! Each test keeps its frozen upstream arguments, input, and assertions.

use support.uu as uu

# origin: uutils test_tr::test_squeeze_multi
test test_uu_tr_squeeze_multi { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ss", "-s", "a-z"], stdin: bytes.from_text("aaBBcDcc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "aBBcDc")
}

# origin: uutils test_tr::test_to_upper
test test_uu_tr_to_upper { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a-z", "A-Z"], stdin: bytes.from_text("!abcd!"))?
  uu.succeeds(r)
  uu.stdout_is(r, "!ABCD!")
}

# origin: uutils test_tr::test_trailing_backslash
test test_uu_tr_trailing_backslash { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "tr", ["-d", "\\"], stdin: bytes.from_text("a\\b\\c\\"))?
  uu.succeeds(r1)
  uu.stderr_is(r1, "tr: warning: an unescaped backslash at end of string is not portable\n")
  uu.stdout_is(r1, "abc")
  let r2 = uu.invoke(s, "tr", ["-d", "\\\\\\"], stdin: bytes.from_text("a\\b\\c\\"))?
  uu.succeeds(r2)
  uu.stderr_is(r2, "tr: warning: an unescaped backslash at end of string is not portable\n")
  uu.stdout_is(r2, "abc")
  let r3 = uu.invoke(s, "tr", ["-d", "\\\\\\\\\\"], stdin: bytes.from_text("a\\b\\c\\"))?
  uu.succeeds(r3)
  uu.stderr_is(r3, "tr: warning: an unescaped backslash at end of string is not portable\n")
  uu.stdout_is(r3, "abc")
}

# origin: uutils test_tr::test_translate_and_squeeze
test test_uu_tr_translate_and_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "x", "y"], stdin: bytes.from_text("xx"))?
  uu.succeeds(r)
  uu.stdout_is(r, "y")
}

# origin: uutils test_tr::test_translate_and_squeeze_multiple_lines
test test_uu_tr_translate_and_squeeze_multiple_lines { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "x", "y"], stdin: bytes.from_text("xxaax\nxaaxx"))?
  uu.succeeds(r)
  uu.stdout_is(r, "yaay\nyaay")
}

# origin: uutils test_tr::test_truncate
test test_uu_tr_truncate { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-t", "abc", "xy"], stdin: bytes.from_text("abcde"))?
  uu.succeeds(r)
  uu.stdout_is(r, "xycde")
}

# origin: uutils test_tr::test_truncate_applies_before_complement_with_class
test test_uu_tr_truncate_applies_before_complement_with_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ct", "[:digit:]", "X"], stdin: bytes.from_text("A"))?
  uu.fails(r)
  uu.stderr_contains(r, "when translating with complemented character classes,\nstring2 must map all characters in the domain to one")
}

# origin: uutils test_tr::test_truncate_flag_fails_with_more_than_two_operand
test test_uu_tr_truncate_flag_fails_with_more_than_two_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-t", "a", "b", "c"])?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand 'c'")
}

# origin: uutils test_tr::test_truncate_multi
test test_uu_tr_truncate_multi { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-tt", "-t", "abc", "xy"], stdin: bytes.from_text("abcde"))?
  uu.succeeds(r)
  uu.stdout_is(r, "xycde")
}

# origin: uutils test_tr::test_truncate_non_utf8_set
test test_uu_tr_truncate_non_utf8_set { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "tr", [Path.parse_bytes(b"a\xfe\xffz")?, p"01234"], stdin: b"\x01amp\xfe\xff")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"\x010mp12")
}

# origin: uutils test_tr::test_truncate_with_set1_shorter_than_set2
test test_uu_tr_truncate_with_set1_shorter_than_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-t", "ab", "xyz"], stdin: bytes.from_text("abcde"))?
  uu.succeeds(r)
  uu.stdout_is(r, "xycde")
}

# origin: uutils test_tr::test_unescaped_backslash_warning_false_positive
test test_uu_tr_unescaped_backslash_warning_false_positive { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "tr", ["-d", "\\\\"], stdin: bytes.from_text("a\\b\\c\\"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "abc")
  let r2 = uu.invoke(s, "tr", ["-d", "\\\\\\\\"], stdin: bytes.from_text("a\\b\\c\\"))?
  uu.succeeds(r2)
  uu.stdout_only(r2, "abc")
  let r3 = uu.invoke(s, "tr", ["-d", "\\\\\\\\\\\\"], stdin: bytes.from_text("a\\b\\c\\"))?
  uu.succeeds(r3)
  uu.stdout_only(r3, "abc")
}

# origin: uutils test_tr::tr_bsd_set1_longer_extends_last_char
test test_uu_tr_tr_bsd_set1_longer_extends_last_char { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["mnop", "pq"], stdin: bytes.from_text("mnopq"))?
  uu.succeeds(r)
  uu.stdout_is(r, "pqqqq")
}

# origin: uutils test_tr::tr_delete_alnum_all
test test_uu_tr_tr_delete_alnum_all { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:alpha:]"], stdin: bytes.from_text("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_delete_alpha_all
test test_uu_tr_tr_delete_alpha_all { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:lower:][:upper:]"], stdin: bytes.from_text("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_delete_complement_alnum
test test_uu_tr_tr_delete_complement_alnum { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:alnum:]"], stdin: bytes.from_text(".abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789."))?
  uu.succeeds(r)
  uu.stdout_is(r, "..")
}

# origin: uutils test_tr::tr_delete_complement_alpha
test test_uu_tr_tr_delete_complement_alpha { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ds", "[:alnum:]", "."], stdin: bytes.from_text(".abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789."))?
  uu.succeeds(r)
  uu.stdout_is(r, ".")
}

# origin: uutils test_tr::tr_delete_digit_all
test test_uu_tr_tr_delete_digit_all { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:digit:]"], stdin: bytes.from_text("0123456789"))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_delete_digit_leaves_alpha
test test_uu_tr_tr_delete_digit_leaves_alpha { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:digit:]"], stdin: bytes.from_text("a0b1c2d3e4f5g6h7i8j9k"))?
  uu.succeeds(r)
  uu.stdout_is(r, "abcdefghijk")
}

# origin: uutils test_tr::tr_delete_equivalence_class_close
test test_uu_tr_tr_delete_equivalence_class_close { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[=]=]"], stdin: bytes.from_text("[[[[[[[]]]]]]]]"))?
  uu.succeeds(r)
  uu.stdout_is(r, "[[[[[[[")
}

# origin: uutils test_tr::tr_delete_equivalence_class_open
test test_uu_tr_tr_delete_equivalence_class_open { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[=[=]"], stdin: bytes.from_text("[[[[[[[]]]]]]]]"))?
  uu.succeeds(r)
  uu.stdout_is(r, "]]]]]]]]")
}

# origin: uutils test_tr::tr_delete_lower_all
test test_uu_tr_tr_delete_lower_all { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:lower:]"], stdin: bytes.from_text("abcdefghijklmnopqrstuvwxyz"))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_delete_space_class
test test_uu_tr_tr_delete_space_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:alnum:]"], stdin: bytes.from_text("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_delete_squeeze_combined
test test_uu_tr_tr_delete_squeeze_combined { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ds", "q", "p"], stdin: bytes.from_text("ppqqpp"))?
  uu.succeeds(r)
  uu.stdout_is(r, "p")
}

# origin: uutils test_tr::tr_delete_squeeze_xdigit
test test_uu_tr_tr_delete_squeeze_xdigit { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ds", "[:xdigit:]", "Z"], stdin: bytes.from_text("ZZ0123456789acbdefABCDEFZZ"))?
  uu.succeeds(r)
  uu.stdout_is(r, "Z")
}

# origin: uutils test_tr::tr_delete_upper_all
test test_uu_tr_tr_delete_upper_all { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:upper:]"], stdin: bytes.from_text("ABCDEFGHIJKLMNOPQRSTUVWXYZ"))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_delete_xdigit_all
test test_uu_tr_tr_delete_xdigit_all { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:xdigit:]"], stdin: bytes.from_text("0123456789acbdefABCDEF"))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_delete_xdigit_leaves_non_hex
test test_uu_tr_tr_delete_xdigit_leaves_non_hex { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[:xdigit:]"], stdin: bytes.from_text("w0x1y2z3456789acbdefABCDEFz"))?
  uu.succeeds(r)
  uu.stdout_is(r, "wxyzz")
}

# origin: uutils test_tr::tr_error_empty_char_class
test test_uu_tr_tr_error_empty_char_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[::]", "x"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_is(r, "tr: missing character class name '[::]'\n")
}

# origin: uutils test_tr::tr_error_empty_equivalence_class
test test_uu_tr_tr_error_empty_equivalence_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[==]", "x"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_is(r, "tr: missing equivalence class character '[==]'\n")
}

# origin: uutils test_tr::tr_error_invalid_char_class
test test_uu_tr_tr_error_invalid_char_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:fooclass:]", "x"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_is(r, "tr: invalid character class 'fooclass'\n")
}

# origin: uutils test_tr::tr_error_repeat_in_set1
test test_uu_tr_tr_error_repeat_in_set1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[a*]", "a"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_is(r, "tr: the [c*] repeat construct may not appear in string1\n")
}

# origin: uutils test_tr::tr_fowler_translate_basic
test test_uu_tr_tr_fowler_translate_basic { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["ah", "-H"], stdin: bytes.from_text("aha"))?
  uu.succeeds(r)
  uu.stdout_is(r, "-H-")
}

# origin: uutils test_tr::tr_posix_repeat_class_extension
test test_uu_tr_tr_posix_repeat_class_extension { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["mnop", "p[q*]"], stdin: bytes.from_text("mnopq"))?
  uu.succeeds(r)
  uu.stdout_is(r, "pqqqq")
}

# origin: uutils test_tr::tr_repeat_class_extends_set2
test test_uu_tr_tr_repeat_class_extends_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a[b*512]c", "1[x*]2"], stdin: bytes.from_text("abc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "1x2")
}

# origin: uutils test_tr::tr_repeat_class_in_set2_basic
test test_uu_tr_tr_repeat_class_in_set2_basic { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:*3][:digit:]", "a-m"], stdin: bytes.from_text(":1239"))?
  uu.succeeds(r)
  uu.stdout_is(r, "cefgm")
}

# origin: uutils test_tr::tr_repeat_class_with_squeeze
test test_uu_tr_tr_repeat_class_with_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a[b*513]c", "1[x*]2"], stdin: bytes.from_text("abc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "1x2")
}

# origin: uutils test_tr::tr_repeat_complement_char_class
test test_uu_tr_tr_repeat_complement_char_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a[=*2][=c=]", "xyyz"], stdin: bytes.from_text("a=c"))?
  uu.succeeds(r)
  uu.stdout_is(r, "xyz")
}

# origin: uutils test_tr::tr_ross_delete_complement
test test_uu_tr_tr_ross_delete_complement { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dcs", "[:lower:]", "n-rs-z"], stdin: bytes.from_text("amzAMZ123.-+amz"))?
  uu.succeeds(r)
  uu.stdout_is(r, "amzamz")
}

# origin: uutils test_tr::tr_ross_delete_complement_squeeze
test test_uu_tr_tr_ross_delete_complement_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ds", "[:xdigit:]", "[:alnum:]"], stdin: bytes.from_text(".ZABCDEFGzabcdefg.0123456788899.GG"))?
  uu.succeeds(r)
  uu.stdout_is(r, ".ZGzg..G")
}

# origin: uutils test_tr::tr_ross_delete_no_squeeze
test test_uu_tr_tr_ross_delete_no_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-cs", "[:upper:]", "X[Y*]"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_is(r, "tr: when translating with complemented character classes,\nstring2 must map all characters in the domain to one\n")
}

# origin: uutils test_tr::tr_ross_delete_with_squeeze
test test_uu_tr_tr_ross_delete_with_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-cs", "[:cntrl:]", "X[Y*]"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_is(r, "tr: when translating with complemented character classes,\nstring2 must map all characters in the domain to one\n")
}

# origin: uutils test_tr::tr_ross_translate_complement
test test_uu_tr_tr_ross_translate_complement { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-cs", "[:upper:]", "[X*]"], stdin: bytes.from_text("AMZamz123.-+AMZ"))?
  uu.succeeds(r)
  uu.stdout_is(r, "AMZXAMZ")
}

# origin: uutils test_tr::tr_ross_translate_complement_squeeze
test test_uu_tr_tr_ross_translate_complement_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-cs", "[:upper:][:digit:]", "[Z*]"], stdin: bytes.from_text(""))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_ross_translate_overlong
test test_uu_tr_tr_ross_translate_overlong { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dcs", "[:alnum:]", "[:digit:]"], stdin: bytes.from_text(""))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_ross_translate_squeeze_delete
test test_uu_tr_tr_ross_translate_squeeze_delete { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dc", "[:upper:]"], stdin: bytes.from_text(""))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_ross_translate_squeeze_overlong
test test_uu_tr_tr_ross_translate_squeeze_overlong { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dc", "[:lower:]"], stdin: bytes.from_text(""))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_tr::tr_squeeze_alnum_remove_spaces
test test_uu_tr_tr_squeeze_alnum_remove_spaces { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "abcdefghijklmn", "[:*016]"], stdin: bytes.from_text("abcdefghijklmnop"))?
  uu.succeeds(r)
  uu.stdout_is(r, ":op")
}

# origin: uutils test_tr::tr_squeeze_char_class_alnum
test test_uu_tr_tr_squeeze_char_class_alnum { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "[p-z]"], stdin: bytes.from_text("ppqqrr"))?
  uu.succeeds(r)
  uu.stdout_is(r, "pqr")
}

# origin: uutils test_tr::tr_squeeze_complement_alnum_to_newline_bsd
test test_uu_tr_tr_squeeze_complement_alnum_to_newline_bsd { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-cs", "[:alnum:]", "\n"], stdin: bytes.from_text("The quick brown fox jumps over the lazy dog."))?
  uu.succeeds(r)
  uu.stdout_is(r, "The\nquick\nbrown\nfox\njumps\nover\nthe\nlazy\ndog\n")
}

# origin: uutils test_tr::tr_squeeze_complement_alnum_to_newline_posix
test test_uu_tr_tr_squeeze_complement_alnum_to_newline_posix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-cs", "[:alnum:]", "[\n*]"], stdin: bytes.from_text("The big black fox jumped over the fence."))?
  uu.succeeds(r)
  uu.stdout_is(r, "The\nbig\nblack\nfox\njumped\nover\nthe\nfence\n")
}

# origin: uutils test_tr::tr_squeeze_explicit_range
test test_uu_tr_tr_squeeze_explicit_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "[p-r]"], stdin: bytes.from_text("ppqqrr"))?
  uu.succeeds(r)
  uu.stdout_is(r, "pqr")
}

# origin: uutils test_tr::tr_squeeze_nul_range
test test_uu_tr_tr_squeeze_nul_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "[\\0-\\5]"], stdin: bytes.from_text("\x00\x00a\x01\x01b\x02\x02\x02c\x03\x03\x03d\x04\x04\x04\x04e\x05\x05"))?
  uu.succeeds(r)
  uu.stdout_is(r, "\x00a\x01b\x02c\x03d\x04e\x05")
}

# origin: uutils test_tr::tr_squeeze_partial_range
test test_uu_tr_tr_squeeze_partial_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "[p-q]"], stdin: bytes.from_text("ppqqrr"))?
  uu.succeeds(r)
  uu.stdout_is(r, "pqrr")
}

# origin: uutils test_tr::tr_squeeze_range_to_class_prefix
test test_uu_tr_tr_squeeze_range_to_class_prefix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "a-p", "%[.*]"], stdin: bytes.from_text("abcdefghijklmnop"))?
  uu.succeeds(r)
  uu.stdout_is(r, "%.")
}

# origin: uutils test_tr::tr_squeeze_range_to_repeat_class
test test_uu_tr_tr_squeeze_range_to_repeat_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "a-p", "[.*]$"], stdin: bytes.from_text("abcdefghijklmnop"))?
  uu.succeeds(r)
  uu.stdout_is(r, ".$")
}

# origin: uutils test_tr::tr_squeeze_range_to_special_chars
test test_uu_tr_tr_squeeze_range_to_special_chars { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "a-p", "%[.*]$"], stdin: bytes.from_text("abcdefghijklmnop"))?
  uu.succeeds(r)
  uu.stdout_is(r, "%.$")
}

# origin: uutils test_tr::tr_squeeze_tail_range
test test_uu_tr_tr_squeeze_tail_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "[q-r]"], stdin: bytes.from_text("ppqqrr"))?
  uu.succeeds(r)
  uu.stdout_is(r, "ppqr")
}

# origin: uutils test_tr::tr_translate_alnum_to_rot13_plus_space
test test_uu_tr_tr_translate_alnum_to_rot13_plus_space { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ds", "\\350", "\\345"], stdin: bytes.from_ints([0o300, 0o301, 0o377, 0o345, 0o345, 0o350, 0o345])?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_ints([0o300, 0o301, 0o377, 0o345])?)
}

# origin: uutils test_tr::tr_translate_backslash_at_end
test test_uu_tr_tr_translate_backslash_at_end { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["\\", "x"], stdin: bytes.from_text("\\"))?
  uu.succeeds(r)
  uu.stdout_is(r, "x")
  uu.stderr_is(r, "tr: warning: an unescaped backslash at end of string is not portable\n")
}

# origin: uutils test_tr::tr_translate_control_chars
test test_uu_tr_tr_translate_control_chars { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ds", "a-z", "$."], stdin: bytes.from_text("a.b.c $$$$code\\"))?
  uu.succeeds(r)
  uu.stdout_is(r, ". $\\")
}

# origin: uutils test_tr::tr_translate_digits_to_letters
test test_uu_tr_tr_translate_digits_to_letters { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "a-z"], stdin: bytes.from_text("abc $code"))?
  uu.succeeds(r)
  uu.stdout_is(r, " $")
}

# origin: uutils test_tr::tr_translate_empty_set1_noop
test test_uu_tr_tr_translate_empty_set1_noop { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["", "[.*]"], stdin: bytes.from_text("rst"))?
  uu.succeeds(r)
  uu.stdout_is(r, "rst")
}

# origin: uutils test_tr::tr_translate_lower_to_upper
test test_uu_tr_tr_translate_lower_to_upper { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:lower:]", "[:upper:]"], stdin: bytes.from_text("abcxyzABCXYZ"))?
  uu.succeeds(r)
  uu.stdout_is(r, "ABCXYZABCXYZ")
}

# origin: uutils test_tr::tr_translate_no_abort_on_long_input
test test_uu_tr_tr_translate_no_abort_on_long_input { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "a", "[b*256]"], stdin: bytes.from_text("abc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "abb")
}

# origin: uutils test_tr::tr_translate_octal_backslash
test test_uu_tr_tr_translate_octal_backslash { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["aUb", "def"], stdin: bytes.from_text("aUb"))?
  uu.succeeds(r)
  uu.stdout_is(r, "def")
}

# origin: uutils test_tr::tr_translate_overlap_repeat
test test_uu_tr_tr_translate_overlap_repeat { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[b*08]", "[x*]"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_is(r, "tr: invalid repeat count '08' in [c*n] construct\n")
}

# origin: uutils test_tr::tr_translate_overlap_repeat_squeeze
test test_uu_tr_tr_translate_overlap_repeat_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[b*010]cd", "[a*7]BC[x*]"], stdin: bytes.from_text("bcd"))?
  uu.succeeds(r)
  uu.stdout_is(r, "BCx")
}

# origin: uutils test_tr::tr_translate_range_to_repeat_class
test test_uu_tr_tr_translate_range_to_repeat_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["rst", "[%*]uvw"], stdin: bytes.from_text("rst"))?
  uu.succeeds(r)
  uu.stdout_is(r, "uvw")
}

# origin: uutils test_tr::tr_translate_range_to_repeat_class_zero
test test_uu_tr_tr_translate_range_to_repeat_class_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abcd", "[]*]"], stdin: bytes.from_text("abcd"))?
  uu.succeeds(r)
  uu.stdout_is(r, "]]]]")
}

# origin: uutils test_tr::tr_translate_repeat_complement
test test_uu_tr_tr_translate_repeat_complement { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "[a*65536]\n", "[b*]"], stdin: bytes.from_text("abcd"))?
  uu.succeeds(r)
  uu.stdout_is(r, "abbb")
}

# origin: uutils test_tr::tr_translate_repeat_in_set2
test test_uu_tr_tr_translate_repeat_in_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a", "[a*][a*]"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_is(r, "tr: only one [c*] repeat construct may appear in string2\n")
}

# origin: uutils test_tr::tr_translate_repeat_multiple_zeros
test test_uu_tr_tr_translate_repeat_multiple_zeros { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abc", "[b*00000000000000000000]"], stdin: bytes.from_text("abcd"))?
  uu.succeeds(r)
  uu.stdout_is(r, "bbbd")
}

# origin: uutils test_tr::tr_translate_repeat_octal
test test_uu_tr_tr_translate_repeat_octal { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abc", "[b*\\9]"], stdin: bytes.from_text("abcd"))?
  uu.succeeds(r)
  uu.stdout_is(r, "[b*d")
}

# origin: uutils test_tr::tr_translate_repeat_x_complement
test test_uu_tr_tr_translate_repeat_x_complement { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-C", "[a*65536]\n", "[b*]"], stdin: bytes.from_text("abcd"))?
  uu.succeeds(r)
  uu.stdout_is(r, "abbb")
}

# origin: uutils test_tr::tr_translate_repeat_zero_count
test test_uu_tr_tr_translate_repeat_zero_count { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abc", "[b*0]"], stdin: bytes.from_text("abcd"))?
  uu.succeeds(r)
  uu.stdout_is(r, "bbbd")
}

# origin: uutils test_tr::tr_translate_single_char_range
test test_uu_tr_tr_translate_single_char_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a-a", "z"], stdin: bytes.from_text("abc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "zbc")
}

# origin: uutils test_tr::tr_translate_upper_to_lower
test test_uu_tr_tr_translate_upper_to_lower { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:upper:]", "[:lower:]"], stdin: bytes.from_text("abcxyzABCXYZ"))?
  uu.succeeds(r)
  uu.stdout_is(r, "abcxyzabcxyz")
}

# origin: uutils test_tr::tr_translate_with_escape_sequence
test test_uu_tr_tr_translate_with_escape_sequence { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a\\-z", "A-Z"], stdin: bytes.from_text("abc-z"))?
  uu.succeeds(r)
  uu.stdout_is(r, "AbcBC")
}
