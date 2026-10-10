"""Case definitions for the awk differential matrix; see regenerate.py.

Every case is an argument vector (after the applet name), optional stdin, and
optional files created in the working directory. Expected results are not
written here: `regenerate.py generate` records what GNU awk produced.
"""

CASES = []

TEXT = "alpha beta gamma\none two three four\n\nlast line here\n"
NUMS = "10 20 30\n4 5 6\n7 8 9\n"
CSV = "a,b,c\n1,2,3\nx,,z\n"


def C(name, prog, stdin=None, files=None, pre=(), post=(), env=None, **extra):
    case = {"name": name, "args": list(pre) + [prog] + list(post)}
    case.update(extra)
    if stdin is not None:
        case["stdin"] = stdin
    if files:
        case["files"] = files
    if env:
        case["env"] = env
    CASES.append(case)


def R(name, args, stdin=None, files=None, env=None, **extra):
    case = {"name": name, "args": list(args)}
    if stdin is not None:
        case["stdin"] = stdin
    if files:
        case["files"] = files
    if env:
        case["env"] = env
    case.update(extra)
    CASES.append(case)


def group(prefix, rows, **common):
    for row in rows:
        name, prog = row[0], row[1]
        stdin = row[2] if len(row) > 2 else None
        extra = dict(common)
        if len(row) > 3:
            extra.update(row[3])
        C(f"{prefix}_{name}", prog, stdin=stdin, **extra)


# Patterns, ranges, BEGIN and END
group("pat", [
    ("empty_program", "", TEXT),
    ("begin_only_no_input", 'BEGIN { print "b" }', TEXT),
    ("end_only_nr", "END { print NR, $0 }", TEXT),
    ("end_keeps_last_record", "END { print $0, NF }", TEXT),
    ("two_begin", 'BEGIN { print 1 } BEGIN { print 2 }'),
    ("two_end", 'END { print 1 } END { print 2 }', "x\n"),
    ("regex_only", "/o/", TEXT),
    ("not_regex", "!/o/", TEXT),
    ("expr_only", "NF > 3", TEXT),
    ("nr_pattern", "NR == 2", TEXT),
    ("string_pattern_nonempty", '"a"', "x\ny\n"),
    ("string_pattern_empty", '""', "x\ny\n"),
    ("zero_pattern", "0", "x\n"),
    ("number_string_pattern", '$1', "0\n1\nabc\n\n0.0\n 0\n+0\n"),
    ("range_basic", "/two/,/last/", TEXT),
    ("range_same_line", "/a/,/a/", "a\nb\na\na\nb\n"),
    ("range_never_closes", "/two/,/nomatch/", TEXT),
    ("range_nr", "NR==2,NR==3 { print NR \": \" $0 }", TEXT),
    ("range_restart", "/x/,/y/ { print }", "x\ny\nz\nx\nq\ny\nx\n"),
    ("two_ranges", "/a/,/b/ { print \"1:\" $0 } /b/,/c/ { print \"2:\" $0 }", "a\nb\nc\nd\n"),
    ("range_with_next", "/a/,/b/ { next } { print }", "x\na\nm\nb\nz\n"),
    ("pattern_comma_only_start", "/x/,", "x\n"),
    ("compound_pattern", "$1 > 3 && $2 < 20", NUMS),
    ("or_pattern", "$1 == 4 || $3 == 30", NUMS),
    ("regex_match_op", "$1 ~ /^[0-9]+$/ { print \"num\", $1 }", "12\nab\n3x\n"),
    ("regex_nomatch_op", "$1 !~ /a/", "ba\nbc\n"),
    ("regex_as_value", "{ x = /o/; print x }", "one\ntwo\nabc\n"),
    ("begin_getline_then_main", "BEGIN { getline; print \"first:\", $0 } { print \"rest:\", $0 }", "a\nb\nc\n"),
    ("begin_then_end_no_main", 'BEGIN { print "x" } END { print NR }', "a\nb\n"),
    ("exit_in_begin_runs_end", 'BEGIN { exit } END { print "end" }', "a\n"),
    ("exit_code_in_begin", 'BEGIN { exit 3 }', None),
    ("exit_code_main", '{ exit 4 } END { print "e" }', "a\nb\n"),
    ("exit_end_overrides", 'BEGIN { exit 1 } END { exit 5 }', None),
    ("exit_end_noarg_keeps", 'BEGIN { exit 6 } END { exit }', None),
    ("exit_expr", 'BEGIN { exit 2+3 }', None),
    ("exit_string", 'BEGIN { exit "7" }', None),
    ("exit_negative", 'BEGIN { exit -1 }', None),
    ("exit_large", 'BEGIN { exit 256 }', None),
    ("exit_in_function", 'function f() { exit 9 } BEGIN { f(); print "no" } END { print "end" }', None),
    ("next_skips_rest", "NR==1 { next } { print }", TEXT),
    ("next_in_function", "function f() { next } /a/ { f() } { print }", "a\nb\n"),
    ("nextfile_basic", "FNR==2 { nextfile } { print FILENAME \":\" $0 }", None, {"post": ["@DIR@/a", "@DIR@/b"], "files": {"a": "a1\na2\na3\n", "b": "b1\nb2\n"}}),
    ("nextfile_in_function", "function f() { nextfile } { f(); print }", None, {"post": ["@DIR@/a"], "files": {"a": "a1\na2\n"}}),
    ("getline_bare_updates_nr", "NR==1 { getline; print NR, $0 }", "a\nb\nc\n"),
    ("assign_pattern", "x = $1 { print x }", "0\n3\nab\n"),
    ("pattern_regex_newline_sep", "/a/\n/b/", "a\nb\nc\n"),
    ("pattern_and_action_semicolon", "/a/ { print \"A\" }; /b/ { print \"B\" }", "a\nb\n"),
    ("action_on_next_line_is_new_rule", "/a/\n{ print \"all\" }", "a\nb\n"),
    ("comment_everywhere", "# c1\nBEGIN { # c2\n print 1 # c3\n}\n# c4\n", None),
    ("semicolon_rules", "BEGIN { print 1 };;; BEGIN { print 2 }", None),
])

# Fields, NF, $0 and OFS
group("fld", [
    ("default_split", "{ print $2 }", "  a   b\tc \n"),
    ("tabs_newlines", "{ print NF, $1 }", "a\tb  c\n"),
    ("nf_value", "{ print NF }", TEXT),
    ("nf_assign_shrink", "{ NF = 2; print; print NF }", "a b c d\n"),
    ("nf_assign_grow", "{ NF = 5; print; print NF }", "a b\n"),
    ("nf_assign_grow_ofs", "BEGIN { OFS = \"-\" } { NF = 4; print }", "a b\n"),
    ("nf_assign_zero", "{ NF = 0; print \"[\" $0 \"]\" }", "a b\n"),
    ("nf_decrement", "{ NF--; print }", "a b c\n"),
    ("nf_increment_field", "{ $(NF+1) = \"new\"; print; print NF }", "a b\n"),
    ("field_beyond_nf", "{ $7 = \"x\"; print; print NF }", "a b\n"),
    ("field_assign_rebuilds", "BEGIN { OFS = \":\" } { $1 = $1; print }", "a  b   c\n"),
    ("field_assign_no_rebuild_before", "BEGIN { OFS = \":\" } { print }", "a  b   c\n"),
    ("ofs_change_after_assign", "{ $1 = $1; OFS = \"-\"; print; $1 = $1; print }", "a b c\n"),
    ("assign_dollar0", "{ $0 = \"x y z\"; print NF, $2 }", "a\n"),
    ("assign_dollar0_in_end", "END { $0 = \"p q\"; print NF, $1 }", "a\n"),
    ("dollar0_mod_updates_fields", "{ $0 = toupper($0); print $1 }", "abc def\n"),
    ("field_zero_after_field_edit", "{ $2 = \"X\"; print $0; print $1 }", "a b c\n"),
    ("negative_field", "{ print $(-1) }", "a\n"),
    ("nf_negative_assign", "{ NF = -1 }", "a\n"),
    ("dollar_nf", "{ print $NF }", "a b c\n"),
    ("dollar_nf_minus", "{ print $(NF-1), $NF-1 }", "a b 5\n"),
    ("dollar_incr", "{ print $1++, $1; print ++$2, $2 }", "3 4\n"),
    ("dollar_expr_float", "{ print $1.9, $(1.9) }", "a b c\n"),
    ("dollar_string_index", "{ print $\"2\" }", "a b c\n"),
    ("dollar_var_index", "{ i = 2; print $i, $i+1 }", "a 5 c\n"),
    ("empty_record_nf", "{ print NF }", "\n\n"),
    ("assign_field_numeric_string", "{ $2 = 0.1 + 0.2; print }", "a b\n"),
    ("convfmt_in_field_assign", "BEGIN { CONVFMT = \"%.2g\" } { $2 = 3.14159; print }", "a b\n"),
    ("field_numeric_compare", "{ print ($1 < $2) }", "10 9\nabc abd\n10 abc\n1e1 10\n0x10 16\n"),
    ("field_leading_space_numeric", "{ print ($1 == 5) }", " 5 \n"),
    ("field_nf_in_begin", "BEGIN { print NF, $0, $1 }", None),
    ("nf_in_end_after_input", "END { print NF }", "a b c\n"),
    ("long_record", "{ print NF, length($0) }", " ".join(["w"] * 2000) + "\n"),
    ("many_fields_assign", "{ $2000 = \"z\"; print NF; print length($0) }", "a\n"),
    ("set_dollar0_resets_nf_var", "{ $0 = \"a b c d e\"; print NF; NF = 2; print $0 }", "x\n"),
    ("print_modified_nf_field", "{ $3 = \"\"; print; print NF }", "a b c d\n"),
    ("getline_var_keeps_fields", "NR==1 { getline x; print $0 \"|\" x \"|\" NF }", "a b\nc d e\n"),
    ("fields_utf8", "{ print length($1), substr($1, 2, 1), toupper($2) }", "héllo wörld\n"),
    ("dollar_0_numeric_string", "{ print ($0 == 5), ($0 < 10) }", "5.0\n"),
    ("field_assign_nonnumeric_then_arith", "{ $1 = \"3x\"; print $1 + 1 }", "a\n"),
])

# Field separators
group("fs", [
    ("comma_opt", "{ print $2, NF }", CSV, {"pre": ["-F,"]}),
    ("comma_sep_opt_arg", "{ print $2, NF }", CSV, {"pre": ["-F", ","]}),
    ("tab_literal", "{ print $2 }", "a b\tc d\te\n", {"pre": ["-F", "\t"]}),
    ("t_means_tab", "{ print $2 }", "a b\tc d\te\n", {"pre": ["-Ft"]}),
    ("t_arg_means_tab", "{ print $2 }", "a b\tc d\te\n", {"pre": ["-F", "t"]}),
    ("tt_not_tab", "{ print $2 }", "attb\n", {"pre": ["-Ftt"]}),
    ("space_opt", "{ print $2 }", "  a   b \n", {"pre": ["-F", " "]}),
    ("single_space_bracket", "{ print NF, $2 }", "a  b\n", {"pre": ["-F", "[ ]"]}),
    ("pipe_literal", "{ print $2 }", "a|b|c\n", {"pre": ["-F|"]}),
    ("escaped_pipe_regex", "{ print $2 }", "a|b|c\n", {"pre": ["-F", "\\|"]}),
    ("dot_literal_single", "{ print $2 }", "a.b.c\n", {"pre": ["-F."]}),
    ("star_single", "{ print $2 }", "a*b*c\n", {"pre": ["-F*"]}),
    ("plus_single", "{ print $2 }", "a+b+c\n", {"pre": ["-F+"]}),
    ("question_single", "{ print $2 }", "a?b?c\n", {"pre": ["-F?"]}),
    ("caret_single", "{ print $2 }", "a^b^c\n", {"pre": ["-F^"]}),
    ("dollar_single", "{ print $2 }", "a$b$c\n", {"pre": ["-F$"]}),
    ("backslash_single", "{ print $2 }", "a\\b\\c\n", {"pre": ["-F\\\\"]}),
    ("bracket_class", "{ print $2 }", "a1b22c\n", {"pre": ["-F", "[0-9]+"]}),
    ("regex_alternation", "{ print $2, $3 }", "a--b::c\n", {"pre": ["-F", "--|::"]}),
    ("regex_anchored_start", "{ print NF }", "abab\n", {"pre": ["-F", "^a"]}),
    ("regex_nonmatching", "{ print NF }", "abc\n", {"pre": ["-F", "x+"]}),
    ("empty_fs_chars", "{ print NF, $2 }", "abc\n", {"pre": ["-F", ""]}),
    ("fs_empty_in_begin", "BEGIN { FS = \"\" } { print NF, $1, $3 }", "héy\n"),
    ("fs_assign_applies_next_record", "{ FS = \":\"; print $1 }", "a:b c:d\ne:f g:h\n"),
    ("fs_assign_in_begin", "BEGIN { FS = \":\" } { print $2 }", "a:b\n"),
    ("fs_v_option", "{ print $2 }", "a:b\n", {"pre": ["-v", "FS=:"]}),
    ("fs_regex_spaces", "{ print NF }", "a   b    c\n", {"pre": ["-F", " +"]}),
    ("fs_regex_leading", "{ print NF, \"[\" $1 \"]\" }", "  a b\n", {"pre": ["-F", " +"]}),
    ("fs_trailing_sep", "{ print NF }", "a,b,\n", {"pre": ["-F,"]}),
    ("fs_leading_sep", "{ print NF, \"[\" $1 \"]\" }", ",a\n", {"pre": ["-F,"]}),
    ("fs_only_sep", "{ print NF }", ",\n", {"pre": ["-F,"]}),
    ("fs_empty_record", "{ print NF }", "\n", {"pre": ["-F,"]}),
    ("fs_escape_x", "{ print $1 }", "a!b\n", {"pre": ["-F", "\\x21"]}),
    ("fs_escape_octal", "{ print $1 }", "a!b\n", {"pre": ["-F", "\\041"]}),
    ("fs_tab_escape", "{ print $2 }", "a\tb\n", {"pre": ["-F", "\\t"]}),
    ("fs_regex_dollar", "{ print NF }", "a\n", {"pre": ["-F", "x$"]}),
    ("fs_with_newline_in_paragraph", "BEGIN { RS = \"\"; FS = \",\" } { print NF; for (i = 1; i <= NF; i++) print i \":\" $i }", "a,b\nc,d\n\ne\n"),
    ("fs_default_paragraph_newline", "BEGIN { RS = \"\" } { print NF; print $3 }", "a b\nc d\n\ne f\n"),
    ("fs_single_char_paragraph_newline_also", "BEGIN { RS = \"\"; FS = \":\" } { print NF }", "a:b\nc:d\n"),
    ("fs_tab_default_ws_not_tab_only", "BEGIN { FS = \"\\t\" } { print NF, $2 }", "a b\tc\n"),
    ("fs_dash_regex_empty_match", "{ print $1 \"-\" $2 \"=\" $3 \"*\" $4 }", "foo--bar\n", {"pre": ["-F", "-*"]}),
    ("fs_equals_plus", "{ print $NF }", "a=====123=\n", {"pre": ["-F", "=+"]}),
    ("fs_double_dash", "{ print NF, length($NF) }", "a--\na--b--\n", {"pre": ["-F--"]}),
    ("fs_case_sensitive", "{ print $2 }", "aXbxc\n", {"pre": ["-Fx"]}),
    ("fs_utf8_char", "{ print $2 }", "a→b→c\n", {"pre": ["-F→"]}),
    ("fs_dollar_var_inside_split_fs", "{ n = split($0, a, FS); print n, a[2] }", "a:b:c\n", {"pre": ["-F:"]}),
    ("fs_split_default_uses_fs_current", "BEGIN { FS = \",\"; n = split(\"a,b c\", a); print n, a[1] }"),
    ("fs_space_class", "{ print NF }", "a\tb\n", {"pre": ["-F", "[[:space:]]"]}),
    ("fs_unicode_space_not_ws", "{ print NF }", "a\u00a0b c\n"),
])

# Records and RS
group("rs", [
    ("default", "{ print NR \":\" $0 }", "a\nb\n"),
    ("no_trailing_newline", "{ print NR \":\" $0 }", "a\nb"),
    ("only_newlines", "{ print NR \":\" length($0) }", "\n\n\n"),
    ("empty_input", "{ print } END { print NR }", ""),
    ("crlf_kept", "{ print length($0) }", "ab\r\ncd\r\n"),
    ("single_char", "BEGIN { RS = \";\" } { print NR \":\" $0 }", "a;b;c\n"),
    ("single_char_trailing", "BEGIN { RS = \";\" } { print NR \":\" $0 }", "a;b;"),
    ("single_char_adjacent", "BEGIN { RS = \";\" } { print NR \":[\" $0 \"]\" }", "a;;b"),
    ("single_char_newline_is_data", "BEGIN { RS = \"x\" } { gsub(/\\n/, \"N\"); print }", "a\nbxc\nd\n"),
    ("paragraph", "BEGIN { RS = \"\" } { print NR \":\" $0 \"|\" NF }", "a b\nc\n\nd\n"),
    ("paragraph_leading_newlines", "BEGIN { RS = \"\" } { print NR \":\" $0 }", "\n\n\na\n\n\n\nb\n\n\n"),
    ("paragraph_trailing_single_newline", "BEGIN { RS = \"\" } { print NR \":\" $0 \"|\" }", "a\nb\n"),
    ("paragraph_no_newline", "BEGIN { RS = \"\" } { print NR \":\" $0 \"|\" }", "a\nb"),
    ("paragraph_only_newlines", "BEGIN { RS = \"\" } END { print NR }", "\n\n\n"),
    ("paragraph_fields_newline_sep", "BEGIN { RS = \"\" } { print NF; print $NF }", "a b\nc d\n"),
    ("paragraph_ofs_ors", "BEGIN { RS = \"\"; ORS = \"|\\n\"; OFS = \"-\" } { $1 = $1; print }", "a b\nc\n\nd\n"),
    ("regex_rs", "BEGIN { RS = \"[0-9]+\" } { print NR \":\" $0 }", "a1b22c333d"),
    ("regex_rs_rt", "BEGIN { RS = \"[0-9]+\" } { print NR \":\" $0 \"|\" RT }", "a1b22c333d\n"),
    ("regex_rs_two_char", "BEGIN { RS = \"ab\" } { print NR \":\" $0 }", "1ab2ab3\n"),
    ("rs_newline_explicit", "BEGIN { RS = \"\\n\" } { print NR \":\" $0 }", "a\nb\n"),
    ("rs_change_mid_input", "NR == 1 { RS = \";\" } { print NR \":\" $0 }", "a\nb;c;d\n"),
    ("rs_space", "BEGIN { RS = \" \" } { print NR \":\" $0 }", "a b c\n"),
    ("rs_case", "BEGIN { RS = \"x\" } { print $0 }", "aXbxc"),
    ("rs_alternation", "BEGIN { RS = \"a|b\" } { print NR \":\" $0 }", "1a2b3"),
    ("rt_default", "{ print \"[\" RT \"]\" }", "a\nb"),
    ("rs_dot_single", "BEGIN { RS = \".\" } { print NR \":\" $0 }", "a.b.c"),
    ("rs_star_single", "BEGIN { RS = \"*\" } { print NR \":\" $0 }", "a*b*c"),
    ("rs_zero_char_rs_pipe", "BEGIN { RS = \"|\" } { print NR \":\" $0 }", "a|b|c"),
    ("nr_fnr_two_files", "{ print FILENAME, NR, FNR }", None, {"post": ["@DIR@/a", "@DIR@/b"], "files": {"a": "1\n2\n", "b": "3\n4\n"}}),
    ("nr_assign", "NR == 2 { NR = 10 } { print NR, FNR }", "a\nb\nc\n"),
    ("fnr_assign", "{ FNR = 5; print FNR }", "a\nb\n"),
    ("nul_bytes", "{ print length($0) }", b"a\x00b\nc\n"),
    ("long_line", "{ print length($0) }", "x" * 100000 + "\n"),
    ("many_records", "END { print NR }", "x\n" * 5000),
    ("ors_ofs", "BEGIN { ORS = \"|\"; OFS = \"-\" } { print $1, $2 }", "a b\nc d\n"),
    ("ors_printf_unaffected", "BEGIN { ORS = \"|\" } { printf \"%s\\n\", $0 }", "a\nb\n"),
])

# getline in all its forms
FILE3 = {"f": "l1\nl2\nl3\n"}
group("get", [
    ("plain_in_main", "NR == 1 { r = getline; print r, $0, NR, FNR }", "a\nb\nc\n"),
    ("plain_at_eof", "END { r = getline; print r, $0 }", "a\nb\n"),
    ("plain_in_begin_consumes", 'BEGIN { while ((getline) > 0) n++; print n, NR }', "a\nb\nc\n"),
    ("var_form", "{ r = getline x; print r, x, $0, NR }", "a\nb\nc\n"),
    ("var_form_fields_untouched", '{ getline x; print NF, $1 }', "a b\nc d e\n"),
    ("file_form", 'BEGIN { while ((getline < "f") > 0) print "got", $0, NR }', None, {"files": FILE3}),
    ("file_var_form", 'BEGIN { while ((getline line < "f") > 0) print "got", line, NR }', None, {"files": FILE3}),
    ("file_form_sets_nf", 'BEGIN { getline < "f"; print NF, $0 }', None, {"files": {"f": "a b c\n"}}),
    ("file_missing", 'BEGIN { r = getline x < "nonexist"; print r }'),
    ("file_missing_loop_ends", 'BEGIN { while ((getline x < "nonexist") > 0) n++; print n + 0 }'),
    ("file_directory", 'BEGIN { r = getline x < "."; print r }'),
    ("file_second_read_continues", 'BEGIN { getline a < "f"; getline b < "f"; print a, b }', None, {"files": FILE3}),
    ("file_close_rereads", 'BEGIN { getline a < "f"; close("f"); getline b < "f"; print a, b }', None, {"files": FILE3}),
    ("file_eof_then_minus", 'BEGIN { while ((getline x < "f") > 0); print (getline x < "f") }', None, {"files": FILE3}),
    ("file_no_trailing_newline", 'BEGIN { getline x < "f"; getline y < "f"; print x "|" y }', None, {"files": {"f": "a\nb"}}),
    ("file_uses_rs", 'BEGIN { RS = ";"; while ((getline x < "f") > 0) print "[" x "]" }', None, {"files": {"f": "a;b;c\n"}}),
    ("file_does_not_touch_nr", 'BEGIN { getline x < "f"; print NR, FNR }', None, {"files": FILE3}),
    ("file_stdin_dash", 'BEGIN { getline x < "-"; print "[" x "]" }', "from stdin\n"),
    ("file_dev_stdin", 'BEGIN { getline x < "/dev/stdin"; print "[" x "]" }', "from stdin\n"),
    ("cmd_form", 'BEGIN { "echo hi there" | getline; print $2, NF, NR }'),
    ("cmd_var_form", 'BEGIN { "echo hi there" | getline x; print x, NR }'),
    ("cmd_loop", 'BEGIN { while (("printf \'a\\nb\\nc\\n\'" | getline x) > 0) print "got", x }'),
    ("cmd_status_zero_at_eof", 'BEGIN { "true" | getline x; print ("true" | getline y) }'),
    ("cmd_close_rerun", 'BEGIN { "echo one" | getline a; close("echo one"); "echo one" | getline b; print a, b }'),
    ("cmd_no_close_no_rerun", 'BEGIN { "echo one" | getline a; r = ("echo one" | getline b); print a, r, "[" b "]" }'),
    ("cmd_concatenated", 'BEGIN { x = "echo"; x " abc" | getline y; print y }'),
    ("cmd_getline_compare", 'BEGIN { while ("echo a" | getline > 0) print "got" }'),
    ("cmd_stderr_passthrough", 'BEGIN { "echo err 1>&2; echo out" | getline x; print x }'),
    ("cmd_fails", 'BEGIN { r = ("exit 3" | getline x); print r; print close("exit 3") }'),
    ("cmd_with_main_input", '{ "echo " $1 | getline x; print x }', "a\nb\n"),
    ("cmd_nr_increments", '{ "echo z" | getline x; print NR }', "a\nb\n"),
    ("assign_arr_elem", 'BEGIN { "echo q" | getline a["k"]; print a["k"] }'),
    ("assign_field", 'BEGIN { $0 = "a b c"; "echo Z" | getline $2; print; print NF }'),
    ("precedence_concat", 'BEGIN { "echo " "a" | getline x; print x }'),
    ("in_condition_loop_file", 'BEGIN { f = "f"; while ((getline l < f) > 0) s = s l; print s }', None, {"files": FILE3}),
    ("unparenthesized_file_concat", 'BEGIN { getline l < "f" ".x"; print l }', None, {"files": {"f": "A\n", "f.x": "B\n"}}),
    ("getline_returns_one", 'BEGIN { print getline < "f" }', None, {"files": FILE3}),
    ("getline_var_numeric_string", 'BEGIN { getline x < "f"; print (x == 10), (x < 9) }', None, {"files": {"f": "10\n"}}),
    ("getline_field_numeric_string", 'BEGIN { getline < "f"; print ($1 == 1.0) }', None, {"files": {"f": "1\n"}}),
    ("bare_getline_updates_nf", "NR==1 { getline; print NF }", "a\nb c d\n"),
    ("getline_file_path_expression", 'BEGIN { d = "."; while ((getline l < (d "/f")) > 0) print l }', None, {"files": FILE3}),
])

# printf and sprintf conversions
P = lambda fmt, *args: 'BEGIN { printf "%s\\n"' % fmt if False else None
rows = []
for name, fmt, args in [
    ("d", "%d", ["42"]), ("d_neg", "%d", ["-42"]), ("d_float", "%d", ["3.99"]), ("d_negfloat", "%d", ["-3.99"]),
    ("d_str", "%d", ['"12abc"']), ("d_big", "%d", ["2^31"]), ("d_big2", "%d", ["2^53"]), ("d_huge", "%d", ["2^64"]),
    ("d_neg_huge", "%d", ["-2^70"]), ("d_inf", "%d", ["log(0)"]), ("d_nan", "%d", ["-log(-1)"]),
    ("i", "%i", ["7"]), ("o", "%o", ["64"]), ("o_neg", "%o", ["-1"]), ("x", "%x", ["255"]), ("X", "%X", ["255"]),
    ("x_neg", "%x", ["-1"]), ("u", "%u", ["3"]), ("u_neg", "%u", ["-1"]),
    ("x_alt", "%#x", ["255"]), ("X_alt", "%#X", ["255"]), ("o_alt", "%#o", ["8"]), ("x_alt_zero", "%#x", ["0"]),
    ("c_num", "%c", ["65"]), ("c_str", "%c", ['"hello"']), ("c_zero", "%c", ["0"]), ("c_256", "%c", ["256"]),
    ("c_big", "%c", ["8364"]), ("c_utf8", "%c", ['"éa"']), ("c_empty", "%c", ['""']), ("c_numstr", "%c", ['"65"']),
    ("c_neg", "%c", ["-1"]), ("c_float", "%c", ["65.7"]),
    ("s", "%s", ['"str"']), ("s_num", "%s", ["3.0"]), ("s_float", "%s", ["0.1"]), ("s_prec", "%.2s", ['"hello"']),
    ("s_width", "%8s", ['"hi"']), ("s_left", "%-8s|", ['"hi"']), ("s_width_prec", "%8.3s", ['"hello"']),
    ("s_utf8_width", "%5s", ['"é"']), ("s_utf8_prec", "%.1s", ['"éa"']), ("s_zero_flag", "%05s", ['"ab"']),
    ("f", "%f", ["3.14159"]), ("f_prec", "%.2f", ["3.14159"]), ("f_prec0", "%.0f", ["2.5"]), ("f_prec0b", "%.0f", ["3.5"]),
    ("f_width", "%10.3f", ["3.14159"]), ("f_neg", "%f", ["-0.5"]), ("f_alt", "%#.0f", ["3"]), ("f_big", "%f", ["1e20"]),
    ("F", "%F", ["1.5"]), ("f_inf", "%f", ["log(0)"]), ("f_ninf", "%f", ["-log(0)"]), ("f_nan", "%f", ["log(-1)"]),
    ("e", "%e", ["12345.678"]), ("E", "%E", ["12345.678"]), ("e_prec", "%.2e", ["0.000123"]), ("e_zero", "%e", ["0"]),
    ("e_neg_exp", "%e", ["1e-100"]), ("e_big_exp", "%e", ["1e300"]),
    ("g", "%g", ["100000"]), ("g_1e6", "%g", ["1000000"]), ("g_small", "%g", ["0.0001"]), ("g_smaller", "%g", ["0.00001"]),
    ("G", "%G", ["0.000001234"]), ("g_prec", "%.3g", ["1234.5678"]), ("g_alt", "%#g", ["1"]), ("g_prec0", "%.0g", ["12"]),
    ("g_trailing", "%g", ["1.50000"]), ("g_third", "%g", ["1/3"]),
    ("plus", "%+d", ["5"]), ("plus_neg", "%+d", ["-5"]), ("space", "% d", ["5"]), ("space_neg", "% d", ["-5"]),
    ("plus_space", "%+ d", ["5"]), ("zero_pad", "%05d", ["42"]), ("zero_pad_neg", "%05d", ["-42"]), ("left", "%-5d|", ["42"]),
    ("left_zero", "%-05d|", ["42"]), ("zero_prec", "%05.3d", ["7"]), ("prec_d", "%.3d", ["7"]), ("prec_d0", "%.0d", ["0"]),
    ("width_star", "%*d", ["6", "42"]), ("width_star_neg", "%*d|", ["-6", "42"]), ("prec_star", "%.*f", ["2", "3.14159"]),
    ("both_star", "%*.*f", ["8", "2", "3.14159"]), ("plus_f", "%+.1f", ["2.25"]), ("space_f", "% .1f", ["2.25"]),
    ("zero_f", "%08.3f", ["-3.14159"]), ("zero_e", "%012.3e", ["31415.9"]), ("zero_s_alt", "%#5s", ['"a"']),
    ("percent", "%%", []), ("percent_width", "%5%", []), ("percent_in_text", "100%% sure", []),
    ("l_modifier", "%ld", ["5"]), ("ll_modifier", "%lld", ["5"]), ("h_modifier", "%hd", ["5"]),
    ("lf_modifier", "%lf", ["1.5"]), ("lu_modifier", "%lu", ["5"]), ("Lf_modifier", "%Lf", ["1.5"]),
    ("z_modifier", "%zd", ["5"]), ("j_modifier", "%jd", ["5"]),
    ("hex_arg_str", "%d", ['"0x1A"']), ("exp_str", "%d", ['"1e3"']), ("empty_str_d", "%d", ['""']),
    ("uninit_s_d", "[%s][%d]", ["u", "u"]),
    ("positional", "%2$s %1$s", ['"a"', '"b"']),
    ("quote_flag", "%'d", ["1234567"]),
    ("multiple", "%s-%s-%s", ['"a"', '"b"', '"c"']), ("extra_args", "%s", ['"a"', '"b"']),
    ("missing_arg", "%s %s", ['"a"']), ("missing_all", "%d", []),
    ("unknown_conv", "%z", ['"a"']), ("unknown_conv_y", "%y", ['5']), ("trailing_percent", "abc%", []),
    ("trailing_flags", "abc%5", []), ("only_percent_arg", "%", ['1']),
    ("escapes", "a\\tb\\\\c\\/d\\\"e\\101\\x41\\e", []), ("bad_escape", "\\q", []),
    ("octal_short", "\\1x\\18", []), ("a_b_f_v", "\\a\\b\\f\\v|", []),
    ("c_in_middle", "[%c]", ["10"]),
    ("s_dollar0", "%s", ["$0"]),
]:
    rows.append((name, 'BEGIN { printf "%s\\n"%s }' % (fmt, "".join(", " + a for a in args))))
group("printf", rows)
group("printf_nonl", [
    ("paren_form", 'BEGIN { printf("%d-%d\\n", 1, 2) }'),
    ("paren_form_redirect", 'BEGIN { printf("%s\\n", "x") > "/dev/stdout" }'),
    ("print_paren_list", 'BEGIN { print("a", "b") }'),
    ("print_paren_list_redirect", 'BEGIN { print("a", "b") > "/dev/stdout" }'),
    ("print_paren_expr_concat", 'BEGIN { print (1)(2) }'),
    ("print_paren_then_more", 'BEGIN { print (1)+2, 3 }'),
    ("print_gt_in_paren", 'BEGIN { print (1 > 2) }'),
    ("print_gt_unparen_redirects", 'BEGIN { print 1 > 2; getline x < "2"; print x }'),
    ("printf_no_args", 'BEGIN { printf }'),
    ("print_empty_print_dollar0", '{ print }', "a b\n"),
    ("printf_reuse_format_not", 'BEGIN { printf "%s %s\\n", "a", "b", "c" }'),
    ("printf_format_number", 'BEGIN { printf 42; print "" }'),
    ("printf_format_from_var", 'BEGIN { f = "%05.1f|\\n"; printf f, 2.345 }'),
    ("printf_uninit_format", 'BEGIN { printf x; print "ok" }'),
    ("sprintf_basic", 'BEGIN { x = sprintf("%3d|%-3d|%03d", 5, 5, 5); print x }'),
    ("sprintf_no_args", 'BEGIN { print sprintf("plain") }'),
    ("sprintf_one_arg_percent", 'BEGIN { print sprintf("%d%%", 50) }'),
    ("sprintf_too_few", 'BEGIN { print sprintf() }'),
    ("sprintf_int_arg_format", 'BEGIN { print sprintf(5) }'),
    ("sprintf_long", 'BEGIN { s = sprintf("%1000d", 1); print length(s) }'),
    ("sprintf_nested", 'BEGIN { print sprintf("%s", sprintf("%d", 3.7)) }'),
    ("printf_c_loop", 'BEGIN { for (i = 72; i < 76; i++) printf "%c", i; print "" }'),
    ("printf_numeric_string_s", '{ printf "%s|%d|%.1f\\n", $1, $1, $1 }', "3.10\n0x10\n1e2\n"),
    ("printf_very_long_s", 'BEGIN { s = sprintf("%s", "x"); for (i = 0; i < 12; i++) s = s s; print length(sprintf("%s%s", s, s)) }'),
    ("ofmt_in_print_not_printf", 'BEGIN { OFMT = "%.2f"; x = 3.14159; print x; printf "%s\\n", x; print x "" }'),
    ("ofmt_integer_unaffected", 'BEGIN { OFMT = "%.2f"; print 3, 3.0, 1e3 }'),
    ("convfmt_concat", 'BEGIN { CONVFMT = "%.2f"; x = 3.14159; y = x ""; print y; a[x] = 1; for (k in a) print k }'),
    ("convfmt_integer", 'BEGIN { CONVFMT = "%.2f"; x = 17; print (x "") }'),
    ("convfmt_array_subscript_int_valued", 'BEGIN { a[1.0] = "x"; print a[1], (1 in a), ("1" in a) }'),
])

# String functions
group("str", [
    ("length_forms", '{ print length, length(), length($0), length($1), length("héllo"), length(12345), length(1/3) }', "abc def\n"),
    ("length_uninit", 'BEGIN { print length(x), length() }'),
    ("length_array", 'BEGIN { a[1]; a[2]; a["x"]; print length(a) }'),
    ("length_after_delete", 'BEGIN { a[1]; a[2]; delete a[1]; print length(a); delete a; print length(a) }'),
    ("substr_basic", 'BEGIN { s = "hello"; print substr(s, 2), substr(s, 2, 3), substr(s, 0), substr(s, -1, 3), substr(s, 4, 100), substr(s, 6), substr(s, 5, 1) }'),
    ("substr_fractions", 'BEGIN { s = "hello"; print substr(s, 1.5, 2), substr(s, 1.5), substr(s, 0.5, 1), substr(s, 2, 1.5), substr(s, 2, 0.4) }'),
    ("substr_zero_len", 'BEGIN { print "[" substr("hello", 2, 0) "]", "[" substr("hello", 2, -1) "]" }'),
    ("substr_nan", 'BEGIN { print "[" substr("hello", "x") "]", "[" substr("hello", 2, "x") "]" }'),
    ("substr_utf8", 'BEGIN { print substr("héllo wörld", 2, 4) }'),
    ("substr_number_arg", 'BEGIN { print substr(12345, 2, 3) }'),
    ("substr_big", 'BEGIN { print substr("hello", 2, 1e10), substr("hello", -1e10, 1e10+3) }'),
    ("index_basic", 'BEGIN { print index("abcabc", "ca"), index("abc", ""), index("", "a"), index("abc", "abcd"), index("héllo", "l"), index("a.c", ".") }'),
    ("index_regex_literal_not_allowed_runtime", 'BEGIN { print index("a1b", 1) }'),
    ("split_basic", 'BEGIN { n = split("a b  c", arr); print n, arr[1], arr[3] }'),
    ("split_char", 'BEGIN { n = split("a:b:c", arr, ":"); print n, arr[1], arr[3] }'),
    ("split_regex", 'BEGIN { n = split("a1b22c", arr, /[0-9]+/); print n, arr[1], arr[2], arr[3] }'),
    ("split_string_regex", 'BEGIN { n = split("a1b22c", arr, "[0-9]+"); print n, arr[3] }'),
    ("split_empty_string", 'BEGIN { n = split("", arr); print n, length(arr) }'),
    ("split_empty_sep", 'BEGIN { n = split("abc", arr, ""); print n, arr[1], arr[2], arr[3] }'),
    ("split_space_string_sep", 'BEGIN { n = split("  a  b ", arr, " "); print n, arr[1], arr[2] }'),
    ("split_clears_array", 'BEGIN { arr[9] = 1; n = split("a b", arr); print n, (9 in arr) }'),
    ("split_single_regex_char_literal", 'BEGIN { n = split("a.b.c", arr, "."); print n }'),
    ("split_pipe_char", 'BEGIN { n = split("a|b|c", arr, "|"); print n }'),
    ("split_seps_gawk", 'BEGIN { n = split("a1b22c", arr, /[0-9]+/, seps); print n, seps[1], seps[2] }'),
    ("split_numeric_strings", 'BEGIN { split("10 9", a); print (a[1] < a[2]), (a[1] > a[2]) }'),
    ("split_leading_sep", 'BEGIN { n = split(",a,b", arr, ","); print n, "[" arr[1] "]" }'),
    ("split_trailing_sep", 'BEGIN { n = split("a,b,", arr, ","); print n, "[" arr[3] "]" }'),
    ("split_tab_sep", 'BEGIN { n = split("a\\tb c", arr, "\\t"); print n, arr[2] }'),
    ("split_regex_anchor", 'BEGIN { n = split("abab", arr, /^a/); print n, "[" arr[1] "]" }'),
    ("split_fs_assigned", 'BEGIN { FS = ","; n = split("a,b c", arr); print n }'),
    ("split_scalar_target", 'BEGIN { x = 1; split("a b", x) }'),
    ("sub_basic", '{ n = sub(/o/, "0"); print n, $0 }', "foo boo\n"),
    ("sub_no_match", '{ n = sub(/z/, "0"); print n, $0 }', "foo boo\n"),
    ("sub_field", '{ sub(/o+/, "0", $1); print; print NF }', "foo boo\n"),
    ("sub_field_whole_rebuild", 'BEGIN { OFS = "-" } { sub(/o/, "0", $2); print }', "foo boo bar\n"),
    ("sub_dollar0_resplits", '{ sub(/ /, "  "); print NF }', "a b\n"),
    ("sub_var", 'BEGIN { s = "hello"; sub(/l+/, "[&]", s); print s }'),
    ("sub_array_elem", 'BEGIN { a["k"] = "hello"; sub(/l/, "L", a["k"]); print a["k"] }'),
    ("sub_ampersand", 'BEGIN { s = "abc"; gsub(/b/, "[&&]", s); print s }'),
    ("sub_escaped_ampersand", 'BEGIN { s = "abc"; gsub(/b/, "\\\\&", s); print s }'),
    ("sub_backslash_amp", 'BEGIN { s = "abc"; gsub(/b/, "\\\\\\\\&", s); print s }'),
    ("sub_backslash_literal", 'BEGIN { s = "abc"; gsub(/b/, "\\\\n", s); print s }'),
    ("sub_double_backslash_lit", 'BEGIN { s = "abc"; sub(/b/, "x\\\\\\\\y", s); print s }'),
    ("gsub_all", 'BEGIN { s = "banana"; n = gsub(/a/, "A", s); print n, s }'),
    ("gsub_empty_match", 'BEGIN { s = "abc"; n = gsub(/x*/, "-", s); print n, s }'),
    ("gsub_empty_match_after", 'BEGIN { s = "abc"; n = gsub(/b*/, "-", s); print n, s }'),
    ("gsub_anchor_start", 'BEGIN { s = "aaa"; n = gsub(/^a/, "b", s); print n, s }'),
    ("gsub_anchor_end", 'BEGIN { s = "aaa"; n = gsub(/a$/, "b", s); print n, s }'),
    ("gsub_empty_string_input", 'BEGIN { s = ""; n = gsub(/^/, "x", s); print n, s }'),
    ("gsub_dot", 'BEGIN { s = "a.b"; gsub(".", "x", s); print s }'),
    ("gsub_string_regex_escaped_dot", 'BEGIN { s = "a.b"; gsub("\\\\.", "x", s); print s }'),
    ("gsub_string_regex_single_escape", 'BEGIN { s = "a.b"; gsub("\\.", "x", s); print s }'),
    ("gsub_regex_literal_slash", 'BEGIN { s = "a/b"; gsub(/\\//, "-", s); print s }'),
    ("gsub_in_dollar0", '{ gsub(/[aeiou]/, "#"); print; print NF }', "hello world\n"),
    ("gsub_fields_nf_change", '{ gsub(/ /, ""); print NF }', "a b c\n"),
    ("gsub_replace_with_number", 'BEGIN { s = "a1"; gsub(/1/, 2.5, s); print s }'),
    ("gsub_target_number", 'BEGIN { x = 1121; gsub(/1/, "x", x); print x }'),
    ("gsub_returns_zero_noassign", 'BEGIN { s = "abc"; print gsub(/z/, "y", s), s }'),
    ("gsub_utf8", 'BEGIN { s = "héllo"; gsub(/é/, "e", s); print s; gsub(/./, "x", s); print s }'),
    ("gsub_overlapping", 'BEGIN { s = "aaaa"; print gsub(/aa/, "b", s), s }'),
    ("gsub_longest", 'BEGIN { s = "abcabc"; print gsub(/a|ab|abc/, "X", s), s }'),
    ("gsub_bracket", 'BEGIN { s = "a-b_c"; gsub(/[-_]/, " ", s); print s }'),
    ("gsub_class", 'BEGIN { s = "a1 b2"; gsub(/[[:digit:]]/, "#", s); print s }'),
    ("gsub_interval", 'BEGIN { s = "aaaa"; gsub(/a{2}/, "X", s); print s }'),
    ("gsub_target_literal_error", 'BEGIN { gsub(/a/, "b", "abc") }'),
    ("gsub_target_expr_error", 'BEGIN { s = "a"; gsub(/a/, "b", s "x") }'),
    ("sub_regex_from_var", 'BEGIN { r = "o+"; s = "foo"; sub(r, "0", s); print s }'),
    ("sub_dynamic_escape_plus", 'BEGIN { s = "a+b"; sub("\\\\+", "-", s); print s }'),
    ("gsub_case", 'BEGIN { s = "Aa"; gsub(/a/, "x", s); print s }'),
    ("match_basic", 'BEGIN { print match("foobar", /o+/), RSTART, RLENGTH }'),
    ("match_nomatch", 'BEGIN { print match("foobar", /z/), RSTART, RLENGTH }'),
    ("match_empty", 'BEGIN { print match("foobar", /x*/), RSTART, RLENGTH }'),
    ("match_anchor", 'BEGIN { print match("foobar", /bar$/), RSTART, RLENGTH }'),
    ("match_utf8", 'BEGIN { print match("héllo", /l+/), RSTART, RLENGTH }'),
    ("match_string_regex", 'BEGIN { print match("a.b", "\\\\."), RSTART }'),
    ("match_array_gawk", 'BEGIN { print match("foobar", /(o+)(b)/, m), m[0], m[1], m[2], m[1, "start"], m[1, "length"] }'),
    ("match_longest_leftmost", 'BEGIN { match("xabcabcy", /(abc)+|ab/); print RSTART, RLENGTH }'),
    ("match_numbers", 'BEGIN { print match(12345, 34), RSTART, RLENGTH }'),
    ("tolower_toupper", 'BEGIN { print tolower("ABC Def 123"), toupper("abc Def 123") }'),
    ("tolower_utf8", 'BEGIN { print tolower("ÀÉÎ"), toupper("àéî ß") }'),
    ("tolower_number", 'BEGIN { print toupper(1e3), tolower(0.5) }'),
    ("concat_numbers", 'BEGIN { print 1 2, 1+2 3, 1 " " 2, -1 -2 }'),
    ("concat_compare_precedence", 'BEGIN { print (1 2 < 13), ("a" "b" == "ab") }'),
    ("string_comparison", 'BEGIN { print ("a" < "b"), ("abc" < "abd"), ("a" < "B"), ("" < "a"), ("10" < "9"), (10 < 9), ("é" > "z") }'),
    ("string_number_compare", 'BEGIN { print (10 < "9"), ("10" == 10), (1e1 == "10"), (x == 0), (x == "") }'),
    ("field_vs_string_const", '{ print ($1 == "10"), ($1 == 10), ($1 < "9") }', "10\n10.0\n"),
    ("getline_var_strnum", 'BEGIN { "echo 10" | getline v; print (v < 9), (v == 10.0) }'),
    ("substr_assign_back", '{ $0 = substr($0, 3); print $1 }', "abc def\n"),
    ("long_string", 'BEGIN { s = "x"; for (i = 0; i < 16; i++) s = s s; print length(s) }'),
    ("repeat_loop_concat", 'BEGIN { for (i = 0; i < 5; i++) s = s i; print s }'),
])

# Numbers, arithmetic and conversion
group("num", [
    ("integer_output", 'BEGIN { print 1e6, 1e16, 1e17, 1e18, 2^53, 2^53 + 1, 2^63, 2^64, 1e30, 123456789012345678 }'),
    ("negative_zero", 'BEGIN { print -0, 0 * -1, -0 "" , 1 / -1e400 }'),
    ("float_output", 'BEGIN { print 0.1 + 0.2, 1/3, 100/3, 1e-5, 123456.789, 1234567.89, 0.000001234, 3.0, 3.10 }'),
    ("inf_nan_output", 'BEGIN { x = 2^1024; print x, -x; y = x - x; print y }'),
    ("log_negative", 'BEGIN { print log(-1) }'),
    ("log_zero", 'BEGIN { print log(0) }'),
    ("sqrt_negative", 'BEGIN { print sqrt(-1) }'),
    ("exp_overflow", 'BEGIN { print exp(1000), exp(-1000) }'),
    ("exp_basic", 'BEGIN { printf "%.6f %.6f\\n", exp(1), exp(0) }'),
    ("trig", 'BEGIN { printf "%.6f %.6f %.6f %.6f\\n", sin(1), cos(1), atan2(1, 1), atan2(-1, -1) }'),
    ("int_func", 'BEGIN { print int(3.9), int(-3.9), int("4.7abc"), int(""), int("abc"), int(1e30), int(-0.5) }'),
    ("sqrt_func", 'BEGIN { print sqrt(16), sqrt(2), sqrt(0) }'),
    ("division", 'BEGIN { print 7 / 2, -7 / 2, 6 / 3, 1 / 4 }'),
    ("modulo", 'BEGIN { print 7 % 3, -7 % 3, 7 % -3, 7.5 % 2, -7.5 % 2, 5 % 0.3 }'),
    ("power", 'BEGIN { print 2^10, 2**10, 2^0.5, 2^-1, (-8)^(1/3), 0^0, 2^3^2, -2^2 }'),
    ("power_assign", 'BEGIN { x = 2; x ^= 3; print x; x **= 2; print x }'),
    ("div_zero_runtime", 'BEGIN { x = 0; print 1 / x }'),
    ("mod_zero_runtime", 'BEGIN { x = 0; print 1 % x }'),
    ("div_zero_assign", 'BEGIN { x = 0; y = 5; y /= x }'),
    ("mod_zero_assign", 'BEGIN { x = 0; y = 5; y %= x }'),
    ("unary_ops", 'BEGIN { x = 5; print -x, +x, !x, !0, !"", !"a", - -x, !!x, -"3x" }'),
    ("increment_ops", 'BEGIN { x = 5; print x++, x, ++x, x--, x, --x }'),
    ("increment_uninit", 'BEGIN { print x++, x; print ++y; print z--, z }'),
    ("pre_post_arrays", 'BEGIN { a[1]++; ++a[1]; print a[1]--; print a[1] }'),
    ("compound_assign", 'BEGIN { x = 10; x += 5; x -= 3; x *= 2; x /= 4; x %= 4; print x }'),
    ("assign_chain", 'BEGIN { a = b = c = 7; print a, b, c }'),
    ("assign_value", 'BEGIN { print (x = 5) + 1, x }'),
    ("precedence_mixed", 'BEGIN { print 1 + 2 * 3 - 4 / 2 % 3, 2 * 3 ^ 2, -3 ^ 2, !1 + 1, 1 - -1, 1 - - 1 }'),
    ("logical_ops", 'BEGIN { print (1 && 0), (1 || 0), (0 || 0), ("a" && "b"), ("" || 0), (1 && "0"), ("0" && 1) }'),
    ("comparison_chain", 'BEGIN { print (1 < 2) < 3, 1 < 2 < 3, (3 > 2) > 1 }'),
    ("ternary", 'BEGIN { print 1 ? "y" : "n", 0 ? "y" : "n", 1 ? 2 ? "a" : "b" : "c", 0 ? 1 : 0 ? 2 : 3 }'),
    ("ternary_assign", 'BEGIN { x = 1 ? 2 : 3; y = 0 ? 2 : 3; print x, y }'),
    ("in_precedence", 'BEGIN { a[1]; print (1 in a), !(2 in a), 1 in a == 1 }'),
    ("match_precedence", 'BEGIN { print "ab" ~ "a" "b", ("ab" ~ "a") "b" }'),
    ("string_to_number", 'BEGIN { print "3x"+0, "x3"+0, " 3 "+0, "3e2"+0, ".5"+0, "+5"+0, "-5"+0, "0x1f"+0, "1e"+0, "1e+"+0, "0b11"+0, "1_000"+0 }'),
    ("string_to_number_special", 'BEGIN { print "inf"+0, "nan"+0, "+inf"+0, "-nan"+0, "infinity"+0, "+nan"+0, "-inf"+0 }'),
    ("number_to_string", 'BEGIN { print (0.1 "") , (1e6 ""), (1e-6 ""), (123456789 ""), (1234567.5 ""), (0.30000000000000004 "") }'),
    ("convfmt_effect", 'BEGIN { CONVFMT = "%d"; x = 3.9; print (x ""), (x + 0 "") }'),
    ("convfmt_int_values", 'BEGIN { CONVFMT = "%.1f"; x = 12; y = x ""; print y; z = 12.34; print z "" }'),
    ("ofmt_effect", 'BEGIN { OFMT = "%.1f"; print 3.14159, 3.14159 "", 7 }'),
    ("numeric_string_input", '{ print ($1 == 1), ($1 == "1"), ($1 + 0 == 1) }', "1.0\n01\n+1\n1e0\n 1 \n"),
    ("hex_input_decimal", '{ print $1 + 0 }', "0x10\n0X1f\n010\n"),
    ("octal_program_literal", 'BEGIN { print 010, 0x10, 011 + 0, 1e2, 1E2, .5, 5., 0.5e1 }'),
    ("large_literals", 'BEGIN { print 99999999999999999999, 0.000000000000000000001, 1e308 * 10 }'),
    ("rand_range", 'BEGIN { r = rand(); print (r >= 0 && r < 1) }'),
    ("srand_return", 'BEGIN { print srand(5), srand(7), srand() }'),
    ("srand_default_returns_zero_first", 'BEGIN { print srand() }'),
    ("rand_deterministic_seed", 'BEGIN { srand(1); a = rand(); srand(1); b = rand(); print (a == b) }'),
    ("rand_first_value", 'BEGIN { print rand() }'),
    ("rand_sequence_seeded", 'BEGIN { srand(42); print rand(), rand(), rand() }'),
    ("rand_int_scaled", 'BEGIN { srand(7); for (i = 0; i < 5; i++) printf "%d ", int(rand() * 100); print "" }'),
    ("bitops", 'BEGIN { print and(12, 10), or(12, 10), xor(12, 10), lshift(1, 8), rshift(256, 4), compl(0) }'),
    ("bitops_neg", 'BEGIN { print and(-1, 1) }'),
    ("bitops_float", 'BEGIN { print and(12.9, 10.1) }'),
    ("bitops_multi", 'BEGIN { print and(15, 7, 3), or(1, 2, 4) }'),
    ("strtonum_hex", 'BEGIN { print strtonum("0x1F"), strtonum("017"), strtonum("12abc"), strtonum("abc") }'),
    ("int_conversion_string_array_index", 'BEGIN { a[01] = 1; a["01"] = 2; for (k in a) n++; print n, a[1], a["01"] }'),
    ("float_index_conversion", 'BEGIN { a[0.1 + 0.2] = 1; for (k in a) print k }'),
    ("number_index_integer_float", 'BEGIN { a[3.0] = 1; a[3] = 2; print length(a) }'),
    ("negative_index", 'BEGIN { a[-1] = 5; print a[-1], (-1 in a), ("-1" in a) }'),
    ("large_loop", 'BEGIN { for (i = 0; i < 20000; i++) s += i; print s }'),
    ("fp_loop", 'BEGIN { for (x = 0; x < 1; x += 0.1) n++; print n }'),
    ("timestamp_funcs_exist", 'BEGIN { t = systime(); print (t > 1000000000) }'),
    ("strftime_fixed", 'BEGIN { print strftime("%Y-%m-%d %H:%M:%S", 0, 1) }'),
    ("mktime_fixed", 'BEGIN { print mktime("2020 01 01 00 00 00") > 0 }'),
    ("gensub_basic", 'BEGIN { print gensub(/(a)(b)/, "\\\\2\\\\1", "g", "abab") }'),
    ("asort_basic", 'BEGIN { a[1] = "c"; a[2] = "a"; a[3] = "b"; n = asort(a); print n, a[1], a[2], a[3] }'),
    ("asorti_basic", 'BEGIN { a["z"]; a["x"]; a["y"]; n = asorti(a, b); print n, b[1], b[2], b[3] }'),
    ("patsplit_basic", 'BEGIN { n = patsplit("a1b2", arr, /[0-9]/); print n, arr[1] }'),
    ("fflush_exists", 'BEGIN { print fflush(), fflush("") }'),
    ("typeof_basic", 'BEGIN { x = 1; print typeof(x), typeof(y), typeof("a") }'),
    ("isarray_basic", 'BEGIN { a[1]; print isarray(a), isarray(x) }'),
])

# Arrays, subscripts and for-in
group("arr", [
    ("basic", 'BEGIN { a["x"] = 1; a["y"] = 2; print a["x"] + a["y"], length(a) }'),
    ("reference_creates", 'BEGIN { if (a["k"] == "") print "empty"; print ("k" in a), length(a) }'),
    ("in_does_not_create", 'BEGIN { print ("k" in a), length(a) }'),
    ("delete_elem", 'BEGIN { a[1]; a[2]; delete a[1]; print (1 in a), (2 in a) }'),
    ("delete_whole", 'BEGIN { a[1]; a[2]; delete a; print length(a) }'),
    ("delete_missing", 'BEGIN { delete a["x"]; print length(a) }'),
    ("delete_in_loop", 'BEGIN { for (i = 0; i < 5; i++) a[i]; for (k in a) delete a[k]; print length(a) }'),
    ("multi_dim", 'BEGIN { a[1, 2] = "x"; for (k in a) { n = split(k, p, SUBSEP); print n, p[1], p[2] }; print ((1, 2) in a), ((2, 1) in a) }'),
    ("subsep_custom", 'BEGIN { SUBSEP = ":"; a[1, 2] = 3; for (k in a) print k }'),
    ("subsep_default_char", 'BEGIN { a["a", "b"] = 1; for (k in a) print length(k) }'),
    ("number_vs_string_key", 'BEGIN { a[1] = "n"; a["1"] = "s"; print length(a), a[1] }'),
    ("float_key", 'BEGIN { a[0.5] = 1; a["0.5"] = 2; print length(a) }'),
    ("key_from_field", '{ c[$1]++ } END { for (k in c) print k, c[k] | "sort" }', "a\nb\na\nc\na\n"),
    ("count_words", '{ for (i = 1; i <= NF; i++) w[$i]++ } END { n = 0; for (k in w) n += w[k]; print n, length(w) }', TEXT),
    ("for_in_sorted_via_pipe", 'BEGIN { a["b"]; a["a"]; a["c"]; for (k in a) print k | "sort"; close("sort"); print "done" }'),
    ("for_in_numeric_small", 'BEGIN { for (i = 1; i <= 5; i++) a[i] = i; for (k in a) printf "%s ", k; print "" }'),
    ("for_in_string_keys", 'BEGIN { a["one"]; a["two"]; a["three"]; a["four"]; for (k in a) printf "%s ", k; print "" }'),
    ("for_in_ten", 'BEGIN { for (i = 1; i <= 10; i++) a[i]; for (k in a) printf "%s ", k; print "" }'),
    ("for_in_zero_based", 'BEGIN { for (i = 0; i < 8; i++) a[i]; for (k in a) printf "%s ", k; print "" }'),
    ("for_in_single", 'BEGIN { a["only"]; for (k in a) print k }'),
    ("for_in_break", 'BEGIN { a[1]; a[2]; a[3]; for (k in a) { n++; break }; print n }'),
    ("for_in_modify_during", 'BEGIN { a[1]; a[2]; for (k in a) { a[k + 10] = 1; n++ }; print n >= 2 }'),
    ("for_in_loop_var_persists", 'BEGIN { a["x"]; for (k in a); print k }'),
    ("array_as_scalar_error", 'BEGIN { a[1] = 1; print a }'),
    ("scalar_as_array_error", 'BEGIN { x = 1; x[1] = 2 }'),
    ("scalar_in_array_op_error", 'BEGIN { x = 1; print (1 in x) }'),
    ("uninit_as_array_then_scalar_error", 'BEGIN { a[1]; a = 5 }'),
    ("delete_scalar_error", 'BEGIN { x = 1; delete x }'),
    ("array_assign_whole_error", 'BEGIN { a[1]; b = a }'),
    ("param_array_passed", 'function fill(arr, n,   i) { for (i = 1; i <= n; i++) arr[i] = i * i } BEGIN { fill(sq, 4); print sq[1], sq[4], length(sq) }'),
    ("param_array_untyped_callee_creates", 'function f(a) { a["k"] = 1 } BEGIN { f(x); print length(x), x["k"] }'),
    ("array_function_return_values", 'function total(a,   s, k) { for (k in a) s += a[k]; return s } BEGIN { v[1] = 10; v[2] = 20; print total(v) }'),
    ("array_in_condition", 'BEGIN { a["x"]; if ("x" in a) print "yes"; if (!("y" in a)) print "no" }'),
    ("array_uninit_elem_type", 'BEGIN { if (a[1] == 0 && a[1] == "") print "both" }'),
    ("array_elem_numeric_string", '{ a[NR] = $1 } END { print (a[1] < a[2]), (a[1] == 10) }', "10\n9\n"),
    ("array_assign_op", 'BEGIN { a["x"] += 5; a["x"] *= 2; print a["x"] }'),
    ("array_incr_in_subscript", 'BEGIN { i = 0; a[i++] = "a"; a[i++] = "b"; print i, a[0], a[1] }'),
    ("array_subscript_expr", 'BEGIN { a[1 + 1] = "two"; a["x" "y"] = "xy"; print a[2], a["xy"] }'),
    ("nested_subscript", 'BEGIN { a[1] = 2; b[2] = 3; print b[a[1]] }'),
    ("huge_array", 'BEGIN { for (i = 0; i < 3000; i++) a[i] = i; print length(a), a[2999] }'),
    ("subscript_empty_string", 'BEGIN { a[""] = 1; print length(a), a[""] }'),
    ("subscript_uninit", 'BEGIN { a[u] = 1; print length(a), ("" in a) }'),
    ("subscript_ofmt_not_used", 'BEGIN { OFMT = "%.1f"; a[3.14159] = 1; for (k in a) print k }'),
    ("subscript_convfmt_used", 'BEGIN { CONVFMT = "%.2f"; a[3.14159] = 1; for (k in a) print k }'),
    ("delete_then_length_loop", 'BEGIN { a[1]; a[2]; a[3]; delete a[2]; for (k in a) n++; print n }'),
    ("array_split_then_delete_all", 'BEGIN { n = split("a b c", p); delete p; print length(p) }'),
    ("environ_exists", 'BEGIN { print ("HOME" in ENVIRON), ENVIRON["AWKTEST"] }', None, {"env": {"AWKTEST": "v1"}}),
    ("environ_numeric_string", 'BEGIN { print (ENVIRON["N"] == 10), (ENVIRON["N"] < 9) }', None, {"env": {"N": "10"}}),
    ("environ_loop_count", 'BEGIN { for (k in ENVIRON) if (k == "AWKTEST") print "found" }', None, {"env": {"AWKTEST": "v1"}}),
])

# User-defined functions and scoping
group("fn", [
    ("basic", 'function add(a, b) { return a + b } BEGIN { print add(2, 3) }'),
    ("recursion", 'function fact(n) { return n <= 1 ? 1 : n * fact(n - 1) } BEGIN { print fact(10), fact(20) }'),
    ("fib", 'function fib(n) { return n < 2 ? n : fib(n - 1) + fib(n - 2) } BEGIN { print fib(15) }'),
    ("locals", 'function f(a,   t) { t = a * 2; return t } BEGIN { t = 5; print f(3), t }'),
    ("local_array", 'function f(  loc) { loc[1] = 1; return length(loc) } BEGIN { print f(), f() }'),
    ("missing_args_uninit", 'function f(a, b) { return "[" a "][" b "]" } BEGIN { print f(1) }'),
    ("too_many_args_error", 'function f(a) { return a } BEGIN { print f(1, 2) }'),
    ("return_no_value", 'function f() { return } BEGIN { x = f(); print "[" x "]" }'),
    ("no_return", 'function f() { } BEGIN { x = f(); print "[" x "]", length(x) }'),
    ("globals_visible", 'function f() { g = 5 } BEGIN { f(); print g }'),
    ("scalar_by_value", 'function f(a) { a = 99 } BEGIN { x = 1; f(x); print x }'),
    ("array_by_ref", 'function f(a) { a[1] = 99 } BEGIN { x[1] = 1; f(x); print x[1] }'),
    ("func_keyword", 'func f() { return 7 } BEGIN { print f() }'),
    ("call_before_define", 'BEGIN { print later(2) } function later(n) { return n * 3 }'),
    ("undefined_function", 'BEGIN { print nosuch(1) }'),
    ("newline_before_brace", 'function f(a)\n{\n return a + 1\n}\nBEGIN { print f(1) }'),
    ("name_is_variable_error", 'function f() { return 1 } BEGIN { f = 2 }'),
    ("param_same_as_function_error", 'function f(f) { return 1 } BEGIN { print f(1) }'),
    ("duplicate_param_error", 'function f(a, a) { return 1 } BEGIN { print f(1, 2) }'),
    ("duplicate_function_error", 'function f() { return 1 } function f() { return 2 } BEGIN { print f() }'),
    ("builtin_name_error", 'function length() { return 1 } BEGIN { print 1 }'),
    ("recursion_with_array", 'function walk(a, n) { if (n == 0) return; a[n] = n; walk(a, n - 1) } BEGIN { walk(arr, 5); print length(arr) }'),
    ("deep_recursion", 'function d(n) { return n == 0 ? 0 : 1 + d(n - 1) } BEGIN { print d(150) }'),
    ("function_modifies_fields", 'function up() { $0 = toupper($0) } { up(); print }', "abc\n"),
    ("function_sets_nf", 'function f() { NF = 1 } { f(); print }', "a b c\n"),
    ("function_in_pattern", 'function big(x) { return x > 5 } big($1)', "3\n9\n6\n"),
    ("call_with_regex_arg", 'function m(s) { return s ~ /a/ } BEGIN { print m("abc"), m("xyz") }'),
    ("call_with_array_elem", 'function f(v) { return v + 1 } BEGIN { a[1] = 4; print f(a[1]) }'),
    ("function_getline_inside", 'function rd(   l) { getline l < "f"; return l } BEGIN { print rd(), rd() }', None, {"files": {"f": "one\ntwo\n"}}),
    ("space_before_paren_call_error", 'function f(x) { return x } BEGIN { print f (1) }'),
    ("function_params_shadow_global_array", 'function f(a) { a = 1; return a } BEGIN { a[1] = 5; print f(2) }'),
    ("untyped_param_scalar_then_array_use", 'function g(p) { p["k"] = 1 } function f(q) { g(q); return length(q) } BEGIN { print f(z), length(z) }'),
    ("function_return_array_elem", 'function f(a) { return a["x"] } BEGIN { b["x"] = "bx"; print f(b) }'),
    ("return_outside_function", 'BEGIN { return 1 }'),
    ("next_in_begin", 'BEGIN { next }'),
    ("next_in_end", 'END { next }', "a\n"),
    ("next_in_function_in_begin", 'function f() { next } BEGIN { f() }'),
    ("getline_into_param", 'function f(v) { "echo hi" | getline v; return v } BEGIN { print f() }'),
])

# Output redirection, close, system, ARGV, uninitialized, errors
FIL = {"f": "l1\nl2\n"}
group("io", [
    ("print_to_file", 'BEGIN { print "hello" > "out"; close("out"); while ((getline l < "out") > 0) print "read:", l }'),
    ("print_to_file_truncates_once", 'BEGIN { print "a" > "out"; print "b" > "out"; close("out"); while ((getline l < "out") > 0) print l }'),
    ("append_file", 'BEGIN { print "a" >> "out"; close("out"); print "b" >> "out"; close("out"); while ((getline l < "out") > 0) print l }', None, {"files": {"out": "old\n"}}),
    ("truncate_existing", 'BEGIN { print "new" > "out"; close("out"); while ((getline l < "out") > 0) print l }', None, {"files": {"out": "old line\nmore\n"}}),
    ("close_reopen_truncates", 'BEGIN { print "a" > "out"; close("out"); print "b" > "out"; close("out"); while ((getline l < "out") > 0) print l }'),
    ("file_visible_at_exit", '{ print $0 > "copy" } END { close("copy"); while ((getline l < "copy") > 0) n++; print n }', "a\nb\nc\n"),
    ("redirect_expression_name", 'BEGIN { f = "o" "ut"; print "x" > f; close(f); getline l < f; print l }'),
    ("redirect_concat_unparenthesized", 'BEGIN { print "x" > "o" "ut"; close("out"); getline l < "out"; print l }'),
    ("redirect_printf", 'BEGIN { printf "%d-%s\\n", 5, "x" > "out"; close("out"); getline l < "out"; print l }'),
    ("redirect_multiple_files", 'BEGIN { print "1" > "o1"; print "2" > "o2"; close("o1"); close("o2"); getline a < "o1"; getline b < "o2"; print a, b }'),
    ("redirect_files_by_field", '{ print $2 > $1 ".txt" } END { close("a.txt"); close("b.txt"); while ((getline l < "a.txt") > 0) print "a:", l; while ((getline l < "b.txt") > 0) print "b:", l }', "a 1\nb 2\na 3\n"),
    ("redirect_unwritable", 'BEGIN { print "x" > "/nonexistent/dir/f" }'),
    ("redirect_directory", 'BEGIN { print "x" > "." }'),
    ("redirect_empty_name", 'BEGIN { print "x" > "" }'),
    ("dev_stdout", 'BEGIN { print "a" > "/dev/stdout"; print "b" }'),
    ("dev_stderr", 'BEGIN { print "e" > "/dev/stderr"; print "o" }'),
    ("dev_stderr_printf", 'BEGIN { printf "e%d\\n", 1 > "/dev/stderr" }'),
    ("dev_null", 'BEGIN { print "x" > "/dev/null"; print "y" }'),
    ("dev_fd1", 'BEGIN { print "x" > "/dev/fd/1"; print "y" }'),
    ("dev_fd2", 'BEGIN { print "x" > "/dev/fd/2" }'),
    ("stdout_stderr_streams_separate", 'BEGIN { print "o1"; print "e1" > "/dev/stderr"; print "o2" }'),
    ("pipe_to_sort", 'BEGIN { print "b" | "sort"; print "a" | "sort"; print "c" | "sort" }'),
    ("pipe_to_sort_close", 'BEGIN { print "b" | "sort"; print "a" | "sort"; close("sort"); print "after" }'),
    ("pipe_ordering_with_stdout", 'BEGIN { print "first"; print "x" | "cat"; print "last" }'),
    ("pipe_ordering_close", 'BEGIN { print "first"; print "x" | "cat"; close("cat"); print "last" }'),
    ("pipe_cat_to_stderr", 'BEGIN { print "x" | "cat 1>&2" }'),
    ("pipe_with_fields", '{ print $1 | "sort -r" } END { close("sort -r"); print "end" }', "a\nc\nb\n"),
    ("pipe_two_commands", 'BEGIN { print "x" | "cat"; close("cat"); print "y" | "tr a-z A-Z"; close("tr a-z A-Z") }'),
    ("pipe_command_expression", 'BEGIN { c = "tr a-z A-Z"; print "abc" | c }'),
    ("pipe_printf", 'BEGIN { printf "%s\\n", "pf" | "cat" }'),
    ("pipe_to_failing_cmd_close", 'BEGIN { print "x" | "cat >/dev/null; exit 3"; print close("cat >/dev/null; exit 3") }'),
    ("close_return_unopened", 'BEGIN { print close("nothing") }'),
    ("close_file_return", 'BEGIN { print "x" > "out"; print close("out") }'),
    ("close_input_cmd_status", 'BEGIN { "exit 5" | getline; print close("exit 5") }'),
    ("close_input_cmd_ok", 'BEGIN { "echo x" | getline; print close("echo x") }'),
    ("close_output_cmd_status", 'BEGIN { print "x" | "cat >/dev/null"; print close("cat >/dev/null") }'),
    ("close_twice", 'BEGIN { print "x" > "out"; print close("out"), close("out") }'),
    ("close_to_from", 'BEGIN { print "x" > "out"; "cat out" | getline v; close("out"); close("cat out"); print "[" v "]" }'),
    ("system_basic", 'BEGIN { r = system("echo from-system"); print "r=" r }'),
    ("system_status", 'BEGIN { print system("exit 3"), system("true"), system("false") }'),
    ("system_ordering", 'BEGIN { print "before"; system("echo mid"); print "after" }'),
    ("system_signal", 'BEGIN { print system("kill -9 $$") }'),
    ("system_sigterm", 'BEGIN { print system("kill -15 $$") }'),
    ("system_not_found", 'BEGIN { print system("nosuchcommand_xyz 2>/dev/null") }'),
    ("system_reads_stdin", 'BEGIN { system("cat") }', "from stdin\n"),
    ("system_after_getline_stdin", 'BEGIN { getline x; print "got", x; system("cat") }', "l1\nl2\nl3\n"),
    ("system_empty", 'BEGIN { print system("") }'),
    ("system_in_main", '{ system("echo " $1) }', "a\nb\n"),
    ("system_output_to_file", 'BEGIN { print "x" > "o"; close("o"); system("cat o") }'),
    ("system_flushes_awk_files", 'BEGIN { print "x" > "o"; system("cat o") }'),
    ("fflush_all", 'BEGIN { print "x" > "o"; fflush(); system("cat o") }'),
    ("fflush_named", 'BEGIN { print "x" > "o"; fflush("o"); system("cat o") }'),
    ("fflush_unknown", 'BEGIN { print fflush("nope") }'),
    ("fflush_stdout", 'BEGIN { print "a"; fflush(); print "b" }'),
    ("close_all_via_end", 'BEGIN { print "x" > "o" } END { }', "a\n"),
    ("exit_closes_pipes", 'BEGIN { print "bye" | "cat"; exit 0 }'),
    ("exit_flushes_files", '{ print $0 > "o" } END { while ((getline l < "o") > 0) n++; print n + 0 }', "a\nb\n"),
    ("output_after_close_pipe", 'BEGIN { print "a" | "cat"; close("cat"); print "b" | "cat" }'),
    ("stdin_dash_operand", '{ print FILENAME ":" $0 }', "s\n", {"post": ["-"]}),
    ("stdin_dash_between", '{ print FILENAME ":" $0 }', "s\n", {"post": ["@DIR@/f", "-", "@DIR@/f"], "files": FIL}),
    ("filename_in_begin_end", 'BEGIN { print "[" FILENAME "]" } END { print FILENAME }', None, {"post": ["@DIR@/f"], "files": FIL}),
    ("filename_stdin_default", '{ print "[" FILENAME "]" }', "a\n"),
    ("filename_assigned", '{ FILENAME = "x"; print FILENAME }', "a\nb\n"),
    ("missing_file", '{ print }', None, {"post": ["@DIR@/nonexistent"]}),
    ("missing_file_then_ok", '{ print FILENAME ": " $0 } END { print NR }', None, {"post": ["@DIR@/nonexistent", "@DIR@/f"], "files": FIL}),
    ("directory_operand", '{ print } END { print NR }', None, {"post": ["@DIR@/d", "@DIR@/f"], "files": FIL, "dirs": ["d"]}),
    ("unreadable_file", '{ print } END { print NR }', None, {"post": ["@DIR@/u", "@DIR@/f"], "files": {"u": "secret\n", "f": "l1\n"}, "modes": {"u": 0}}),
    ("many_files", '{ print FILENAME, FNR }', None, {"post": ["@DIR@/f%d" % i for i in range(8)], "files": {"f%d" % i: "x%d\n" % i for i in range(8)}}),
    ("empty_file_operand", 'END { print NR, FILENAME }', None, {"post": ["@DIR@/e"], "files": {"e": ""}}),
    ("symlink_operand", '{ print }', None, {"post": ["@DIR@/l"], "files": FIL, "links": {"l": "f"}}),
    ("operand_assign_before_file", '{ print x, $0 }', None, {"post": ["x=1", "@DIR@/f", "x=2", "@DIR@/f"], "files": FIL}),
    ("operand_assign_escape", '{ print x }', "a\n", {"post": ["x=a\\tb"]}),
    ("operand_assign_only_stdin", '{ print x, $0 }', "a\n", {"post": ["x=5"]}),
    ("operand_assign_end", 'END { print x }', None, {"post": ["@DIR@/f", "x=late"], "files": FIL}),
    ("operand_assign_begin_unseen", 'BEGIN { print "[" x "]" } { }', None, {"post": ["x=1", "@DIR@/f"], "files": FIL}),
    ("operand_not_assign_has_slash", '{ print }', None, {"post": ["@DIR@/a=b"], "files": {"a=b": "eq\n"}}),
    ("operand_assign_invalid_name_is_file", '{ print }', None, {"post": ["1x=2"]}),
    ("operand_assign_numeric_string", '{ print (n == 10), (n < 9) }', "a\n", {"post": ["n=10"]}),
    ("operand_assign_fs", '{ print $2 }', "a:b\n", {"post": ["FS=:"]}),
    ("operand_assign_fs_between_files", '{ print $1 }', None, {"post": ["@DIR@/c", "FS=:", "@DIR@/c"], "files": {"c": "a:b c\n"}}),
    ("argv_contents", 'BEGIN { for (i = 0; i < ARGC; i++) print i, ARGV[i] }', None, {"post": ["p", "q=1", "r"]}),
    ("argc", 'BEGIN { print ARGC }', None, {"post": ["a", "b", "c"]}),
    ("argv0", 'BEGIN { print ARGV[0] }'),
    ("argv_modify_adds_file", 'BEGIN { ARGV[1] = "@DIR@/f"; ARGC = 2 } { print }', None, {"files": FIL}),
    ("argv_var_in_options", 'BEGIN { print ARGC, ARGV[1], ARGV[2] }', None, {"pre": ["-v", "x=1"], "post": ["a", "b"]}),
    ("environ_modify_not_passed_to_children", 'BEGIN { ENVIRON["ZZ"] = "1"; system("echo [$ZZ]") }'),
    ("rstart_rlength_init", 'BEGIN { print RSTART, RLENGTH }'),
    ("builtin_var_defaults", 'BEGIN { printf "[%s][%s][%s][%s][%s][%s][%s][%s]\\n", FS, OFS, ORS, RS, SUBSEP == "\\034", CONVFMT, OFMT, NF }'),
    ("nr_in_begin", 'BEGIN { print NR, FNR, NF, "[" FILENAME "]" }'),
    ("rt_in_end", 'END { print "[" RT "]" }', "a\nb\n"),
])

# Uninitialized values, errors, and option processing
group("opt", [
    ("uninit_numeric_and_string", 'BEGIN { print x + 0, "[" x "]", length(x), (x == 0), (x == ""), !x }'),
    ("uninit_in_output", 'BEGIN { print x; print x + 1; printf "[%s][%d]\\n", y, y }'),
    ("uninit_compare_field", '{ print (u == $1) }', "0\n\nabc\n"),
    ("uninit_param_passes_through", 'function f(p) { return p == 0 && p == "" } BEGIN { print f(q) }'),
    ("unset_nf_field_compare", '{ print ($3 == 0), ($3 == "") }', "a b\n"),
    ("assign_uninit_to_var", 'BEGIN { y = x; print (y == 0), (y == "") }'),
])
R("opt_version_long", ["--version"], stdin="")
R("opt_version_short", ["-V"], stdin="")
R("opt_help", ["--help"], stdin="")
R("opt_usage_no_args", [], stdin="")
R("opt_unknown_option", ["--nosuchoption", "BEGIN { print 1 }"], stdin="")
R("opt_unknown_short", ["-Z", "BEGIN { print 1 }"], stdin="")
R("opt_missing_f_arg", ["-f"], stdin="")
R("opt_missing_v_arg", ["-v"], stdin="")
R("opt_missing_F_arg", ["-F"], stdin="")
R("opt_f_file", ["-f", "prog.awk"], stdin="a b\n", files={"prog.awk": "{ print $2 }\n"})
R("opt_f_two_files", ["-f", "p1", "-f", "p2"], stdin="a b\n", files={"p1": "function f(x) { return x \"!\" }\n", "p2": "{ print f($1) }\n"})
R("opt_f_stdin", ["-f", "-", "@DIR@/data"], stdin="{ print $1 }\n", files={"data": "x y\n"})
R("opt_f_missing", ["-f", "nofile.awk"], stdin="")
R("opt_f_with_program_operand_is_file", ["-f", "p", "BEGIN"], stdin="", files={"p": "{ print }\n"})
R("opt_f_attached", ["-fp"], stdin="a\n", files={"p": "{ print \"A\" $0 }\n"})
R("opt_f_long", ["--file=p"], stdin="a\n", files={"p": "{ print \"A\" $0 }\n"})
R("opt_f_long_space", ["--file", "p"], stdin="a\n", files={"p": "{ print \"A\" $0 }\n"})
R("opt_source_e", ["-e", "BEGIN { print 1 }"], stdin="")
R("opt_source_e_two", ["-e", "BEGIN { print 1 }", "-e", "BEGIN { print 2 }"], stdin="")
R("opt_source_long", ["--source=BEGIN { print 3 }"], stdin="")
R("opt_v_assign", ["-v", "x=5", "BEGIN { print x + 1 }"], stdin="")
R("opt_v_attached", ["-vx=5", "BEGIN { print x + 1 }"], stdin="")
R("opt_v_assign_long", ["--assign=x=7", "BEGIN { print x }"], stdin="")
R("opt_v_escape", ["-v", "x=a\\tb\\n", "BEGIN { printf \"%s|\", x }"], stdin="")
R("opt_v_empty_value", ["-v", "x=", "BEGIN { print \"[\" x \"]\", length(x) }"], stdin="")
R("opt_v_numeric_string", ["-v", "x=10", "BEGIN { print (x < 9), (x == 10.0) }"], stdin="")
R("opt_v_no_equals", ["-v", "x", "BEGIN { print 1 }"], stdin="")
R("opt_v_bad_name", ["-v", "1x=2", "BEGIN { print 1 }"], stdin="")
R("opt_v_special_vars", ["-v", "OFS=-", "-v", "ORS=!\\n", "BEGIN { print 1, 2 }"], stdin="")
R("opt_v_nr", ["-v", "NR=10", "{ print NR }"], stdin="a\nb\n")
R("opt_v_multiple", ["-v", "a=1", "-v", "b=2", "BEGIN { print a + b }"], stdin="")
R("opt_v_keyword_name", ["-v", "if=1", "BEGIN { print 1 }"], stdin="")
R("opt_v_function_name", ["-v", "f=1", "function f() { } BEGIN { print 1 }"], stdin="")
R("opt_F_regex_special_ere", ["-F", "[,;]", "{ print $2 }"], stdin="a,b;c\n")
R("opt_F_long", ["--field-separator=:", "{ print $2 }"], stdin="a:b\n")
R("opt_F_long_space", ["--field-separator", ":", "{ print $2 }"], stdin="a:b\n")
R("opt_double_dash", ["--", "{ print $1 }"], stdin="a b\n")
R("opt_double_dash_then_dash_prog", ["-F:", "--", "{ print $2 }", "@DIR@/f"], files={"f": "a:b\n"})
R("opt_options_after_program_are_files", ["{ print }", "-F:"], stdin="", files={})
R("opt_posix", ["--posix", "BEGIN { print 1 }"], stdin="")
R("opt_traditional", ["--traditional", "BEGIN { print 1 }"], stdin="")
R("opt_short_c", ["-c", "BEGIN { print 1 }"], stdin="")
R("opt_re_interval", ["--re-interval", "BEGIN { print (\"aa\" ~ /^a{2}$/) }"], stdin="")
R("opt_use_lc_numeric", ["--use-lc-numeric", "BEGIN { print 1 }"], stdin="")
R("opt_non_decimal_data", ["-n", "{ print $1 + 0 }"], stdin="0x10\n011\n")
R("opt_non_decimal_data_long", ["--non-decimal-data", "{ print $1 + 0 }"], stdin="0x10\n011\n")
R("opt_characters_as_bytes", ["-b", "{ print length($0) }"], stdin="é\n")
R("opt_lint", ["--lint", "BEGIN { print 1 }"], stdin="")
R("opt_dump_variables", ["-d", "BEGIN { print 1 }"], stdin="")
R("opt_sandbox", ["-S", "BEGIN { system(\"echo hi\") }"], stdin="")
R("opt_sandbox_getline", ["--sandbox", "BEGIN { \"echo\" | getline }"], stdin="")
R("opt_sandbox_redirect", ["--sandbox", "BEGIN { print 1 > \"x\" }"], stdin="")
R("opt_optimize", ["-O", "BEGIN { print 1 }"], stdin="")
R("opt_profile", ["--profile=prof.out", "BEGIN { print 1 }"], stdin="")
R("opt_debug", ["-D", "BEGIN { print 1 }"], stdin="")
R("opt_load_ext", ["-l", "nosuchext", "BEGIN { print 1 }"], stdin="")
R("opt_include", ["-i", "nosuchinc", "BEGIN { print 1 }"], stdin="")
R("opt_exec", ["-E", "p"], stdin="", files={"p": "BEGIN { print 9 }\n"})
R("opt_bignum", ["-M", "BEGIN { print 2^100 }"], stdin="")
R("opt_ignorecase", ["-i", "BEGIN { print 1 }"], stdin="")
R("opt_stdin_default_program_missing", ["-v", "x=1"], stdin="")
R("opt_prog_empty_string", [""], stdin="a\n")
R("opt_prog_whitespace_only", ["  \n  "], stdin="a\n")
R("opt_dash_as_program_operand", ["-"], stdin="")
R("opt_no_input_files_begin_only_no_read", ["BEGIN { print 1 }", "@DIR@/nonexistent"], stdin="")
R("opt_end_reads_nonexistent", ["END { print NR }", "@DIR@/nonexistent"], stdin="")

# Expression grammar quirks: unary versus binary operators, concatenation,
# non-associative operators, increment placement and getline precedence.
group("gram", [
    ("concat_minus_space", 'BEGIN { print 1 " " -1 }'),
    ("concat_minus_tight", 'BEGIN { print 1 -1 }'),
    ("concat_str_minus", 'BEGIN { print "a" -1 }'),
    ("concat_var_minus", 'BEGIN { x = 5; print x -1, x " " -1, x " " -x }'),
    ("concat_plus", 'BEGIN { print 2 " " +1 }'),
    ("concat_neg_start", 'BEGIN { print -1 " " -1 }'),
    ("concat_not", 'BEGIN { x = 0; print 1 !x }'),
    ("concat_not_space", 'BEGIN { print 1 ! 2 }'),
    ("concat_then_compare", 'BEGIN { print 1 2 == 12, (1 2) == 12 }'),
    ("concat_in_add", 'BEGIN { print 1 + 2 3, 1 2 + 3 }'),
    ("pow_neg", 'BEGIN { print -2 ^ 2, 2 ^ -2, !2 ^ 2, 2 ^ 3 ^ 2, -2 ^ -2 }'),
    ("neg_chain", 'BEGIN { print 1 - -1, - - 1, !-1, !!2, -!0 }'),
    ("incr_forms", 'BEGIN { x = 1; print x++ + ++x, x }'),
    ("incr_plus_plus", 'BEGIN { x = 1; y = 2; print x+++y, x, y }'),
    ("incr_minus_minus", 'BEGIN { x = 5; y = 2; print x---y, x, y }'),
    ("incr_field", '{ i = 1; print $i++, i; print $++i, i }', "a b c\n"),
    ("field_incr_vs_postfix", '{ print $1++ + 1, $1 }', "5 x\n"),
    ("field_forms", '{ print $ NF, $NF - 1, $(NF) - 1, -$1, $1^2, $NF^2 }', "3 4\n"),
    ("field_neg_literal", '{ print $-0 }', "a\n"),
    ("field_of_field", '{ print $$1 }', "2 b c\n"),
    ("field_dollar_incr_then", '{ n = 1; print $n++ }', "7 8\n"),
    ("match_in_assign", 'BEGIN { a = "x" ~ "x"; print a }'),
    ("match_chain_error", 'BEGIN { print "a" ~ "a" ~ 1 }'),
    ("equals_chain_error", 'BEGIN { print 1 == 1 == 1 }'),
    ("noteq_chain_error", 'BEGIN { print 1 != 2 != 3 }'),
    ("lt_gt_mix_parenthesized", 'BEGIN { print (1 < 2) (3 > 2), (1 < 2) + (3 > 2) }'),
    ("ternary_in_print_paren", 'BEGIN { print (1 > 2 ? "a" : "b") }'),
    ("ternary_nested_right", 'BEGIN { print 1 ? 2 : 3 ? 4 : 5, 0 ? 2 : 0 ? 4 : 5 }'),
    ("ternary_assign_else", 'BEGIN { 0 ? a = 1 : b = 2; print a "|" b }'),
    ("ternary_in_concat", 'BEGIN { print "x" (1 ? "y" : "z") "w" }'),
    ("ternary_with_regex", '{ print ($0 ~ /a/ ? "has" : "no") }', "abc\nxyz\n"),
    ("length_forms", '{ print length "x", length + 1, length(), length($0) }', "abcd\n"),
    ("length_space_paren", '{ print length ($0) }', "abcd\n"),
    ("length_in_condition", 'length > 2 { print "long" } length { print "nonempty" }', "ab\nabc\n\n"),
    ("in_postfix_cmp", 'BEGIN { a[1]; print 1 in a == 1, (1 in a) == 1, 1 in a ? "y" : "n" }'),
    ("in_with_concat", 'BEGIN { a["12"]; print 1 2 in a }'),
    ("in_with_add", 'BEGIN { a[3]; print 1 + 2 in a }'),
    ("in_multi_paren", 'BEGIN { a[1, 2]; print ((1, 2) in a), (1, 2) in a }'),
    ("not_in", 'BEGIN { a[1]; print !(2 in a), ! 2 in a }'),
    ("in_chain_error", 'BEGIN { a[1]; b[1]; print 1 in a in b }'),
    ("assign_in_cond", 'BEGIN { if ((x = 5) > 3) print "big", x }'),
    ("assign_right_assoc", 'BEGIN { a = b = 3; a += b -= 1; print a, b }'),
    ("assign_to_field_expr", '{ $(1 + 1) = "X"; print }', "a b c\n"),
    ("assign_field_concat_rhs", '{ $2 = $2 "!"; print }', "a b c\n"),
    ("compound_assign_field", '{ $1 += 5; $2 *= 2; print }', "1 2\n"),
    ("pre_incr_array_in_cond", 'BEGIN { if (++a["x"] == 1) print "first" }'),
    ("regex_as_division", 'BEGIN { x = 6; y = 3; print x / y, x/y/1 }'),
    ("regex_after_paren", 'BEGIN { x = 8; print (x) / 2 }'),
    ("regex_literal_in_match_call", 'BEGIN { print match("xay", /a/) }'),
    ("regex_with_slash_in_bracket", '/[/]/ { print "slash" }', "a/b\nxyz\n"),
    ("regex_with_escaped_slash", '/a\\/b/ { print "m" }', "a/b\n"),
    ("regex_empty", '// { print "all" }', "x\n"),
    ("regex_div_equals_ambiguity", 'BEGIN { x = 8; x /= 2; print x }'),
    ("regex_starts_with_equals", '$0 ~ /=x/ { print "eq" }', "=x\n"),
    ("string_escapes", 'BEGIN { print "a\\tb\\nc\\\\d\\"e\\/f\\101\\x41" }'),
    ("string_newline_escape_continuation", 'BEGIN { print "ab\\\ncd" }'),
    ("backslash_continuation", 'BEGIN { x = 1 + \\\n 2; print x }'),
    ("comment_after_continuation", 'BEGIN { print 1 # c\n print 2 }'),
    ("semicolon_optional_newline", 'BEGIN { x = 1\n y = 2\n print x + y }'),
    ("if_else_chain", 'BEGIN { x = 2; if (x == 1) print "one"; else if (x == 2) print "two"; else print "other" }'),
    ("if_else_newline", 'BEGIN { x = 2\n if (x == 1)\n print "one"\n else\n print "other" }'),
    ("if_no_braces_semicolon_else", 'BEGIN { if (0) print "a"; else print "b" }'),
    ("while_loop", 'BEGIN { while (i < 3) { print i; i++ } }'),
    ("do_while", 'BEGIN { do { print i; i++ } while (i < 3) }'),
    ("do_while_once", 'BEGIN { do print "once"; while (0) }'),
    ("for_empty_parts", 'BEGIN { for (;;) { if (++i > 3) break }; print i }'),
    ("for_continue", 'BEGIN { for (i = 0; i < 5; i++) { if (i % 2) continue; printf "%d ", i }; print "" }'),
    ("nested_break", 'BEGIN { for (i = 0; i < 3; i++) for (j = 0; j < 3; j++) { if (j == 1) break; print i, j } }'),
    ("break_outside_loop", 'BEGIN { break }'),
    ("continue_outside_loop", 'BEGIN { continue }'),
    ("while_newline_body", 'BEGIN { while (i < 2)\n i++\n print i }'),
    ("empty_statement_forms", 'BEGIN { ; ; x = 1; ; print x }'),
    ("block_in_block", 'BEGIN { { { print "deep" } } }'),
    ("action_without_braces_error", 'BEGIN print 1'),
    ("getline_prec_cmd_concat", 'BEGIN { "echo a" "b" | getline x; print x }'),
    ("getline_prec_add", 'BEGIN { "echo 5" | getline x; print x + 1 }'),
    ("getline_unary_prec", 'BEGIN { while ("echo a; echo b" | getline x) n++; print n }'),
    ("getline_lt_prec_compare", 'BEGIN { print (getline x < "nonexistent") }'),
    ("getline_var_then_field_assign", '{ getline x; $0 = x; print NF }', "a\nb c d\n"),
    ("printf_parens_with_redirect_gt", 'BEGIN { printf("%d\\n", 1 > 2) }'),
    ("print_paren_comparison", 'BEGIN { print (1)(2), (1)+(2) }'),
    ("print_comma_newline", 'BEGIN { print 1,\n 2 }'),
    ("and_newline", 'BEGIN { if (1 &&\n 1) print "ok" }'),
    ("or_newline", 'BEGIN { if (0 ||\n 1) print "ok" }'),
    ("comma_in_call_newline", 'BEGIN { print substr("hello",\n 2, 3) }'),
    ("keyword_as_var_error", 'BEGIN { if = 1 }'),
    ("builtin_as_var_error", 'BEGIN { length = 1 }'),
    ("getline_as_var_error", 'BEGIN { getline = 1 }'),
    ("missing_brace_error", 'BEGIN { print 1'),
    ("extra_brace_error", 'BEGIN { print 1 } }'),
    ("unterminated_regex_error", 'BEGIN { print /abc }'),
    ("unterminated_string_error", 'BEGIN { print "abc }'),
    ("bad_char_error", 'BEGIN { print @ }'),
    ("lone_operator_error", 'BEGIN { print + }'),
    ("missing_paren_error", 'BEGIN { print (1 + 2 }'),
    ("unbalanced_bracket_error", 'BEGIN { a[1 = 2 }'),
    ("else_without_if_error", 'BEGIN { else print 1 }'),
    ("empty_regex_group_error", '/(/ { print }', "a\n"),
    ("regex_unmatched_bracket_error", '/[a/ { print }', "a\n"),
    ("regex_bad_interval", '/a{2,1}/ { print }', "a\n"),
    ("regex_star_start", '/*a/ { print "m" }', "a\n*a\n"),
    ("regex_plus_start", '/+a/ { print "m" }', "a\n+a\n"),
    ("regex_dynamic_unmatched_paren", 'BEGIN { print match("a", "(") }'),
    ("dollar_nan_field", 'BEGIN { print $(log(-1)) "x" }'),
])

# Regular expressions: matching semantics of the ERE engine inside awk.
RXI = "abc\nABC\na.c\na+c\nac\naac\naaac\nfoo bar\nfoo_bar\n x\n\n[x]\na]b\na-b\na\\b\n^caret\ndollar$\n12 34\ntab\there\n"
rx = []
for name, pat in [
    ("literal", "abc"), ("dot", "a.c"), ("star", "a*c"), ("plus", "a+c"), ("question", "ab?c"), ("alt", "abc|foo"),
    ("group_alt", "(a|f)(b|o)"), ("anchor_start", "^a"), ("anchor_end", "c$"), ("anchor_both", "^ac$"),
    ("class_alpha", "[[:alpha:]]+ [[:alpha:]]+"), ("class_digit", "[[:digit:]]+"), ("class_space", "[[:space:]]"),
    ("class_upper", "^[[:upper:]]+$"), ("class_punct", "[[:punct:]]"), ("class_alnum_neg", "[^[:alnum:]]"),
    ("class_blank", "[[:blank:]]x"), ("class_word", "\\w+"), ("class_nonword", "\\W"), ("class_space_esc", "\\s"),
    ("class_nonspace", "\\S+ \\S+"), ("word_boundary", "\\<foo\\>"), ("word_boundary_y", "\\yfoo\\y"), ("not_boundary", "\\Boo"),
    ("bracket_range", "[a-c]+$"), ("bracket_neg", "^[^a-c]"), ("bracket_close_first", "[]]"), ("bracket_neg_close", "[^]a]x"),
    ("bracket_dash_last", "a[b-]b"), ("bracket_dash_first", "a[-b]b"), ("bracket_caret_not_first", "[a^]"), ("bracket_backslash", "a[\\\\]b"),
    ("bracket_escaped_close", "a[\\]]b"), ("bracket_dot_literal", "a[.]c"), ("bracket_star_literal", "[*]"),
    ("interval_exact", "^a{2}$"), ("interval_range", "^a{2,3}$"), ("interval_open", "^a{2,}$"), ("interval_zero", "^a{0}c"),
    ("interval_group", "(ab){2}"), ("escaped_dot", "a\\.c"), ("escaped_plus", "a\\+c"), ("escaped_dollar", "\\$$"),
    ("escaped_caret", "^\\^"), ("escaped_bracket", "\\[x\\]"), ("escaped_backslash", "a\\\\b"), ("tab_escape", "\\t"),
    ("newline_escape_none", "\\n"), ("dot_matches_tab", "tab.here"), ("star_after_group", "(ab)*c"), ("nested_group", "((a)b)c"),
    ("empty_alt_branch", "a(|b)c"), ("anchor_middle", "a^b"), ("dollar_middle", "a$b"), ("case_sensitive", "ABC"),
    ("backref_literal", "\\1"), ("brace_literal", "a{"), ("brace_literal_close", "a}"), ("brace_comma_literal", "a{,2}"),
    ("question_star_combo", "a*?c"), ("double_plus", "a++c"), ("star_after_anchor", "^*"), ("paren_star", "(*a)"),
    ("alternation_longest", "a|ab|abc"), ("dot_star_greedy", "a.*c"), ("space_class_vs_literal", "foo bar"),
    ("percent_slash_chars", "[/%]"), ("equiv_class", "[[=a=]]"), ("collating", "[[.a.]]"), ("nested_bracket_class_range", "[[:digit:]a-c]"),
]:
    rx.append((name, 'BEGIN { RS = "\\n" } $0 ~ /' + pat.replace("/", "\\/") + '/ { n++; printf "%d:%s\\n", NR, $0 } END { print n + 0 }', RXI))
group("rx", rx)
group("rxdyn", [
    ("dynamic_string", 'BEGIN { r = "a.c"; if ("abc" ~ r) print "y"; if ("a.c" ~ "a\\\\.c") print "y2"; if ("abc" ~ "a\\\\.c") print "bad" }'),
    ("dynamic_from_field", '$1 ~ $2 { print "match" } !($1 ~ $2) { print "no" }', "abc b+\nabc x\n"),
    ("dynamic_anchor", 'BEGIN { if ("abc" ~ "^" "a") print "y" }'),
    ("dynamic_number", 'BEGIN { if (12345 ~ 34) print "y"; if (12345 ~ 1.5) print "bad" }'),
    ("dynamic_empty", 'BEGIN { if ("abc" ~ "") print "empty matches" }'),
    ("dynamic_backslash_dot", 'BEGIN { s = "a.b"; n = gsub("\\\\.", "-", s); print n, s }'),
    ("dynamic_class_in_string", 'BEGIN { s = "a1b2"; gsub("[[:digit:]]", "#", s); print s }'),
    ("dynamic_dollar_in_string", 'BEGIN { s = "a$b"; sub("\\\\$", "S", s); print s }'),
    ("dynamic_caret_literal", 'BEGIN { s = "a^b"; sub("\\\\^", "C", s); print s }'),
    ("dynamic_slash", 'BEGIN { s = "a/b"; sub("/", "|", s); print s }'),
    ("dynamic_newline", 'BEGIN { s = "a\\nb"; if (s ~ "a\\nb") print "nl"; if (s ~ /a.b/) print "dot-nl" }'),
    ("dynamic_regex_special_fs", 'BEGIN { n = split("a.b|c", p, "[.|]"); print n, p[1], p[2], p[3] }'),
    ("dynamic_longest_leftmost_sub", 'BEGIN { s = "xaaay"; sub(/a+/, "<&>", s); print s }'),
    ("dynamic_match_rstart_rlength_zero_width", 'BEGIN { print match("abc", /$/), RSTART, RLENGTH; print match("abc", /^/), RSTART, RLENGTH }'),
    ("gsub_zero_width_between", 'BEGIN { s = "abc"; gsub(//, "-", s); print s }'),
    ("gsub_star_empty_boundaries", 'BEGIN { s = "baaac"; gsub(/a*/, "X", s); print s }'),
    ("gsub_caret_multiple", 'BEGIN { s = "a\\na"; gsub(/^a/, "X", s); print s }'),
    ("gsub_dollar_multiple", 'BEGIN { s = "a\\na"; gsub(/a$/, "X", s); print s }'),
    ("regex_newline_in_record_dot", 'BEGIN { RS = ""; } { print ($0 ~ /a.b/), ($0 ~ /^b/), ($0 ~ /a$/) }', "a\nb\n"),
    ("regex_utf8_dot_length", 'BEGIN { s = "héé"; gsub(/./, "x", s); print s }'),
    ("regex_utf8_class", 'BEGIN { s = "añb"; print match(s, /[ñ]/), RSTART, RLENGTH }'),
    ("regex_case_insensitive_var_absent", 'BEGIN { IGNORECASE = 1; print ("ABC" ~ /abc/) }'),
])


# Known deviations from the oracle. A deviation either records what this
# implementation prints instead (`xsh`) or names a channel that is not
# compared (`loose`); each carries the reason.
def deviate(name, reason, xsh=None, loose=None):
    for case in CASES:
        if case["name"] == name:
            if xsh:
                case["xsh"] = xsh
            if loose:
                case["loose"] = loose
            case["reason"] = reason
            return
    raise KeyError(name)


REFUSED = "awk: fatal: option `--%s' is not supported\n"
deviate("printf_c_neg", "a negative %c code is one raw byte in GNU awk; text here is UTF-8 only", xsh={"stdout": "\ufffd\n"})
deviate("num_string_to_number_special", "the sign of a NaN is not carried through arithmetic", xsh={"stdout": "0 0 +inf -nan 0 -nan -inf\n"})
for _name in ("num_rand_first_value", "num_rand_sequence_seeded", "num_rand_int_scaled"):
    deviate(_name, "the pseudo-random sequence differs from GNU awk's generator", loose=["stdout"])
deviate("fn_too_many_args_error", "the BusyBox suite requires no warning for surplus function arguments", loose=["stderr"])
deviate("opt_non_decimal_data", "hexadecimal and octal input strings are not supported", xsh={"stdout": "", "stderr": REFUSED % "non-decimal-data", "status": 2})
deviate("opt_non_decimal_data_long", "hexadecimal and octal input strings are not supported", xsh={"stdout": "", "stderr": REFUSED % "non-decimal-data", "status": 2})
deviate("opt_characters_as_bytes", "text is always UTF-8 characters", xsh={"stdout": "", "stderr": REFUSED % "characters-as-bytes", "status": 2})
deviate("opt_dump_variables", "variable dumping is not supported", xsh={"stdout": "", "stderr": REFUSED % "dump-variables", "status": 2})
deviate("opt_profile", "profiling is not supported", xsh={"stdout": "", "stderr": REFUSED % "profile", "status": 2})
deviate("opt_debug", "the debugger is not supported", xsh={"stdout": "", "stderr": REFUSED % "debug", "status": 2})
