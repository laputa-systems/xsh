##! Transcribed from the uutils coreutils fmt integration suite.

use support.uu as uu

# origin: uutils test_fmt::fmt_reflow_unicode
test test_uu_fmt_fmt_reflow_unicode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fmt", ["-w", "4"], stdin: bytes.from_text("漢字漢字 💐 日本語の文字\n"))?
  uu.succeeds(r)
  uu.stdout_is(r, "漢字漢字\n💐\n日本語の文字\n")
}

# origin: uutils test_fmt::prefix_equal
test test_uu_fmt_prefix_equal { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "prefixed-one-word-per-line.txt", "prefixed-one-word-per-line.txt")?
  for prefix_args in [
        
        ["-p", "="],
        ["--prefix=="],
        ["--prefix", "="],
        ["--pref=="],
        ["--pref", "="],
        
        ["--prefix=-", "--prefix=="],
    ] {
        let r = uu.invoke(s, "fmt", [@prefix_args, "prefixed-one-word-per-line.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/fmt/prefixed-one-word-per-line_p=.txt".read_bytes()?)
    }
}

# origin: uutils test_fmt::prefix_ignores_leading_whitespace_without_exact_prefix
test test_uu_fmt_prefix_ignores_leading_whitespace_without_exact_prefix { |ctx|
  let s = uu.scene(ctx)?
  for prefix_args in [["-p", "> "], ["--prefix", "> "]] {
        let r = uu.invoke(s, "fmt", [@prefix_args], stdin: bytes.from_text("  > alpha\n  > beta\n"))?
  uu.succeeds(r)
  uu.stdout_only(r, "  > alpha beta\n")
    }
}

# origin: uutils test_fmt::prefix_minus
test test_uu_fmt_prefix_minus { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "prefixed-one-word-per-line.txt", "prefixed-one-word-per-line.txt")?
  for prefix_args in [
        ["-p-"],
        ["-p", "-"],
        ["--prefix=-"],
        ["--prefix", "-"],
        ["--pref=-"],
        ["--pref", "-"],
        
        ["--prefix==", "--prefix=-"],
    ] {
        let r = uu.invoke(s, "fmt", [@prefix_args, "prefixed-one-word-per-line.txt"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/fmt/prefixed-one-word-per-line_p-.txt".read_bytes()?)
    }
}

# origin: uutils test_fmt::split_does_not_reflow
test test_uu_fmt_split_does_not_reflow { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  for arg in ["-s", "-ss", "--split-only"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", arg])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fp"{ctx.core_dir}/tests/data/uutils/fmt/one-word-per-line.txt".read_bytes()?)
    }
}

# origin: uutils test_fmt::test_fmt
test test_uu_fmt_fmt { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  let r = uu.invoke(s, "fmt", ["one-word-per-line.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "this is a file with one word per line\n")
}

# origin: uutils test_fmt::test_fmt_goal_bigger_than_default_width_of_75
test test_uu_fmt_fmt_goal_bigger_than_default_width_of_75 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  # GNU reports a goal beyond the selected width as a width range error.
  for param in ["-g", "--goal"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", param, "76"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "fmt: invalid width: '76': Value too large for defined data type\n")
    }
}

# origin: uutils test_fmt::test_fmt_goal_only_defaults_width_to_goal_plus_ten
test test_uu_fmt_fmt_goal_only_defaults_width_to_goal_plus_ten { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  for goal in [5, 10, 20, 30, 50, 65] {
    let widened = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-w", f"{goal + 10}", "-g", f"{goal}"])?
    uu.succeeds(widened)
    let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-g", f"{goal}"])?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, widened.stdout)
  }
  let second = uu.invoke(s, "fmt", ["one-word-per-line.txt", "--goal", "30"])?
  uu.succeeds(second)
  uu.stdout_is(second, "this is a file with one word per line\n")
}

# origin: uutils test_fmt::test_fmt_goal_too_big
test test_uu_fmt_fmt_goal_too_big { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  # GNU reports a goal beyond the selected width as a width range error.
  for param in ["-g", "--goal"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", "--width=75", param, "76"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "fmt: invalid width: '76': Value too large for defined data type\n")
    }
}

# origin: uutils test_fmt::test_fmt_goal_too_small_to_check_negative_minlength
test test_uu_fmt_fmt_goal_too_small_to_check_negative_minlength { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  for param in ["-g", "--goal"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", "--width=75", param, "10"])?
  uu.succeeds(r)
  uu.stdout_is(r, "this is a file with one word per line\n")
    }
}

# origin: uutils test_fmt::test_fmt_invalid_goal
test test_uu_fmt_fmt_invalid_goal { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  # GNU uses the same diagnostic name for goal and width validation.
  for param in ["-g", "--goal"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", param, "invalid"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "invalid width: 'invalid'")
    }
}

# origin: uutils test_fmt::test_fmt_invalid_goal_override
test test_uu_fmt_fmt_invalid_goal_override { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-g", "apple", "-g", "74"])?
  uu.succeeds(r)
  uu.stdout_is(r, "this is a file with one word per line\n")
}

# origin: uutils test_fmt::test_fmt_invalid_goal_width_priority
test test_uu_fmt_fmt_invalid_goal_width_priority { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-g", "apple", "-w", "banana"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "fmt: invalid width: 'banana'\n")
    let second = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-w", "banana", "-g", "apple"])?
  uu.fails_with_code(second, 1)
  uu.no_stdout(second)
  uu.stderr_is(second, "fmt: invalid width: 'banana'\n")
}

# origin: uutils test_fmt::test_fmt_invalid_utf8
test test_uu_fmt_fmt_invalid_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let input = b"=\xa0="
    let r = uu.invoke(s, "fmt", ["-s", "-w1"], stdin: input)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"=\xa0=\n")
}

# origin: uutils test_fmt::test_fmt_invalid_width
test test_uu_fmt_fmt_invalid_width { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  for param in ["-w", "--width"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", param, "invalid"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "invalid width: 'invalid'")
    }
}

# origin: uutils test_fmt::test_fmt_knuth_plass_line_breaking
test test_uu_fmt_fmt_knuth_plass_line_breaking { |ctx|
  let s = uu.scene(ctx)?
  let input = "@command{fmt} prefers breaking lines at the end of a sentence, and tries to\navoid line breaks after the first word of a sentence or before the last word\nof a sentence.  A @dfn{sentence break} is defined as either the end of a\nparagraph or a word ending in any of @samp{.?!}, followed by two spaces or end\nof line, ignoring any intervening parentheses or quotes.  Like @TeX{},\n@command{fmt} reads entire ''paragraphs'' before choosing line breaks; the\nalgorithm is a variant of that given by\nDonald E. Knuth and Michael F. Plass\nin ''Breaking Paragraphs Into Lines'',\n@cite{Software---Practice & Experience}\n@b{11}, 11 (November 1981), 1119--1184."



    let expected = "@command{fmt} prefers breaking lines at the end of a sentence,\nand tries to avoid line breaks after the first word of a sentence\nor before the last word of a sentence.  A @dfn{sentence break}\nis defined as either the end of a paragraph or a word ending\nin any of @samp{.?!}, followed by two spaces or end of line,\nignoring any intervening parentheses or quotes.  Like @TeX{},\n@command{fmt} reads entire ''paragraphs'' before choosing line\nbreaks; the algorithm is a variant of that given by Donald\nE. Knuth and Michael F. Plass in ''Breaking Paragraphs Into\nLines'', @cite{Software---Practice & Experience} @b{11}, 11\n(November 1981), 1119--1184.\n"


    let r = uu.invoke(s, "fmt", ["-g", "60", "-w", "72"], stdin: bytes.from_text(input))?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: uutils test_fmt::test_fmt_non_existent_file
test test_uu_fmt_fmt_non_existent_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fmt", ["non-existing"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "fmt: cannot open 'non-existing' for reading: No such file or directory\n")
}

# origin: uutils test_fmt::test_fmt_non_utf8_paths
test test_uu_fmt_fmt_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let filename = uu.at_bytes(s, b"\xff\xfe")?
  filename.write(b"hello world this is a test")?
  let r = uu.invoke_paths(s, "fmt", [Path.parse_bytes(b"\xff\xfe")?])?
  uu.succeeds(r)
}

# origin: uutils test_fmt::test_fmt_positional_width
test test_uu_fmt_fmt_positional_width { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  let r = uu.invoke(s, "fmt", ["-10", "one-word-per-line.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "this is a\nfile with\none word\nper line\n")
}

# origin: uutils test_fmt::test_fmt_positional_width_not_first
test test_uu_fmt_fmt_positional_width_not_first { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-10"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "fmt: invalid option -- 1; -WIDTH is recognized only when it is the first\noption; use -w N instead")
}

# origin: uutils test_fmt::test_fmt_set_goal_not_contain_width
test test_uu_fmt_fmt_set_goal_not_contain_width { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  for param in ["-g", "--goal"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", param, "74"])?
  uu.succeeds(r)
  uu.stdout_is(r, "this is a file with one word per line\n")
    }
}

# origin: uutils test_fmt::test_fmt_unicode_whitespace_handling
test test_uu_fmt_fmt_unicode_whitespace_handling { |ctx|
  let s = uu.scene(ctx)?
  for char in [" ", " ", " ", "⁠", "х"] {
    let r = uu.invoke(s, "fmt", ["-s", "-w1"], stdin: bytes.from_text(f"={char}="))?
    uu.succeeds(r)
    assert r.stdout.utf8()?.lines().len() == 1
  }
}

# origin: uutils test_fmt::test_fmt_width
test test_uu_fmt_fmt_width { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  for param in ["-w", "--width"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", param, "10"])?
  uu.succeeds(r)
  uu.stdout_is(r, "this is a\nfile with\none word\nper line\n")
    }
    let second = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-w50", "--width", "10"])?
  uu.succeeds(second)
  uu.stdout_is(second, "this is a\nfile with\none word\nper line\n")
}

# origin: uutils test_fmt::test_fmt_width_invalid
test test_uu_fmt_fmt_width_invalid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-w", "apple"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "fmt: invalid width: 'apple'\n")
    
    let second = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-w", "apple", "-w10"])?
  uu.succeeds(second)
  uu.stdout_is(second, "this is a\nfile with\none word\nper line\n")
}

# origin: uutils test_fmt::test_fmt_width_max_display_width
test test_uu_fmt_fmt_width_max_display_width { |ctx|
  let s = uu.scene(ctx)?
  let input = "aa bb cc dd ee"
    let r = uu.invoke(s, "fmt", ["-w", "8"], stdin: bytes.from_text(input))?
  uu.succeeds(r)
  uu.stdout_is(r, "aa bb cc\ndd ee\n")
    let second = uu.invoke(s, "fmt", ["-w", "7"], stdin: bytes.from_text(input))?
  uu.succeeds(second)
  uu.stdout_is(second, "aa\nbb cc\ndd ee\n")
}

# origin: uutils test_fmt::test_fmt_width_multibyte_gnu_compatible
test test_uu_fmt_fmt_width_multibyte_gnu_compatible { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fmt", ["-w", "10"], stdin: bytes.from_text("漢 字 test 日 本 語"))?
  uu.succeeds(r)
  uu.stdout_is(r, "漢 字\ntest 日\n本 語\n")

    let second = uu.invoke(s, "fmt", ["-w", "15"], stdin: bytes.from_text("漢字 test 日本語"))?
  uu.succeeds(second)
  uu.stdout_is(second, "漢字 test\n日本語\n")
}

# origin: uutils test_fmt::test_fmt_width_multiplication_overflow
test test_uu_fmt_fmt_width_multiplication_overflow { |ctx|
  let s = uu.scene(ctx)?
  # GNU retains the numerical range suffix for a parsed value this large.
  let r = uu.invoke(s, "fmt", ["-w", "267672676527678256"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "fmt: invalid width: '267672676527678256': Numerical result out of range\n")
}

# origin: uutils test_fmt::test_fmt_width_not_valid_number
test test_uu_fmt_fmt_width_not_valid_number { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  let r = uu.invoke(s, "fmt", ["-25x", "one-word-per-line.txt"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "fmt: invalid width: '25x'")
}

# origin: uutils test_fmt::test_fmt_width_too_big
test test_uu_fmt_fmt_width_too_big { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  for param in ["-w", "--width"] {
        let r = uu.invoke(s, "fmt", ["one-word-per-line.txt", param, "2501"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "fmt: invalid width: '2501': Numerical result out of range\n")
    }
    
    let second = uu.invoke(s, "fmt", ["one-word-per-line.txt", "-w2501", "--width", "10"])?
  uu.succeeds(second)
  uu.stdout_is(second, "this is a\nfile with\none word\nper line\n")
}

# origin: uutils test_fmt::test_invalid_arg
test test_uu_fmt_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fmt", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_fmt::test_invalid_input
test test_uu_fmt_invalid_input { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fmt", ["."])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_fmt::test_small_width
test test_uu_fmt_small_width { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fmt", "one-word-per-line.txt", "one-word-per-line.txt")?
  for width in ["0", "1", "2", "3"] {
        for param in ["-w", "--width"] {
            let r = uu.invoke(s, "fmt", [param, width, "one-word-per-line.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "this\nis\na\nfile\nwith\none\nword\nper\nline\n")
        }
    }
}
