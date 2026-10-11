use support.uu

# origin: gnu fmt/long-line.log
test test_gnu_fmt_long_line_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", [" y" for _ in range(1015)].join("") + "\n")?
  let line = [" y" for _ in range(35)].join("") + "\n"
  let expected = [line for _ in range(29)].join("")
  let r = uu.invoke(s, "fmt", ["-s", "in"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: gnu fmt/non-space.log
test test_gnu_fmt_non_space_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [{locale: "en_US.iso8859-1", chars: [b"\xa0"]}, {locale: "en_US.UTF-8", chars: [b"\xc2\xa0", b"\xe2\x80\x87", b"\xe2\x80\xaf", b"\xe2\x81\xa0", b"\xd1\x85"]}, {locale: "ru_RU.KOI8-R", chars: [b"\x9a"]}] {
    for character in row.chars {
      let input = bytes.concat([b"=", character, b"="])
      let r = uu.invoke(s, "fmt", ["-s", "-w1"], stdin: input, vars: {LC_ALL: row.locale})?
      assert r.stdout.count_lines() == 1
    }
  }
}

# origin: gnu fmt/width.log
test test_gnu_fmt_width_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [{width: "8", out: "aa bb cc\ndd ee\n"}, {width: "7", out: "aa\nbb cc\ndd ee\n"}] {
    let r = uu.invoke(s, "fmt", ["-w", row.width], stdin: b"aa bb cc dd ee")?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
}

# origin: gnu fmt/goal-option.log
test test_gnu_fmt_goal_option_log { |ctx|
  let s = uu.scene(ctx)?
  let input = fp"{ctx.core_dir}/tests/data/gnu/fmt/goal-input.txt".read_bytes()?
  uu.write_bytes(s, "base", input)?
  let expected = fp"{ctx.core_dir}/tests/data/gnu/fmt/goal-output.txt".read_bytes()?
  let r = uu.invoke(s, "fmt", ["-g", "60", "-w", "72", "base"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
}
