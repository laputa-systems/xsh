use support.uu

# origin: busybox comm/comm test 1
test test_bb_comm_comm_test_1_8e5d7efa { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", ["456", "abc"].join("\n") + "\n")?
  let input = ["123", "def"].join("\n") + "\n"
  let result = uu.invoke(s, "comm", ["input", "-"], stdin: bytes.from_text(input))?
  uu.succeeds(result)
  uu.stdout_only(result, ["\t123", "456", "abc", "\tdef"].join("\n") + "\n")
}

# origin: busybox comm/comm test 2
test test_bb_comm_comm_test_2_3e78e368 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", ["456", "abc"].join("\n") + "\n")?
  let input = ["123", "def"].join("\n") + "\n"
  let result = uu.invoke(s, "comm", ["-", "input"], stdin: bytes.from_text(input))?
  uu.succeeds(result)
  uu.stdout_only(result, ["123", "\t456", "\tabc", "def"].join("\n") + "\n")
}

# origin: busybox comm/comm test 3
test test_bb_comm_comm_test_3_4a7b6249 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", ["abc", "xyz"].join("\n") + "\n")?
  let input = ["def"].join("\n") + "\n"
  let result = uu.invoke(s, "comm", ["input", "-"], stdin: bytes.from_text(input))?
  uu.succeeds(result)
  uu.stdout_only(result, ["abc", "\tdef", "xyz"].join("\n") + "\n")
}

# origin: busybox comm/comm test 4
test test_bb_comm_comm_test_4_752baf4f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", ["abc", "xyz"].join("\n") + "\n")?
  let input = ["def"].join("\n") + "\n"
  let result = uu.invoke(s, "comm", ["-", "input"], stdin: bytes.from_text(input))?
  uu.succeeds(result)
  uu.stdout_only(result, ["\tabc", "def", "\txyz"].join("\n") + "\n")
}

# origin: busybox comm/comm test 5
test test_bb_comm_comm_test_5_59de7762 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", ["123", "abc"].join("\n") + "\n")?
  let input = ["def"].join("\n") + "\n"
  let result = uu.invoke(s, "comm", ["input", "-"], stdin: bytes.from_text(input))?
  uu.succeeds(result)
  uu.stdout_only(result, ["123", "abc", "\tdef"].join("\n") + "\n")
}

# origin: busybox comm/comm test 6
test test_bb_comm_comm_test_6_1308f8f3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", ["123", "abc"].join("\n") + "\n")?
  let input = ["def"].join("\n") + "\n"
  let result = uu.invoke(s, "comm", ["-", "input"], stdin: bytes.from_text(input))?
  uu.succeeds(result)
  uu.stdout_only(result, ["\t123", "\tabc", "def"].join("\n") + "\n")
}

# origin: busybox comm/comm unterminated line 1
test test_bb_comm_comm_unterminated_line_1_fe134e58 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", ["abc"].join("\n"))?
  let input = ["def"].join("\n")
  let result = uu.invoke(s, "comm", ["input", "-"], stdin: bytes.from_text(input))?
  uu.succeeds(result)
  uu.stdout_only(result, ["abc", "\tdef"].join("\n") + "\n")
}

# origin: busybox comm/comm unterminated line 2
test test_bb_comm_comm_unterminated_line_2_1ee7d021 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", ["abc"].join("\n"))?
  let input = ["def"].join("\n")
  let result = uu.invoke(s, "comm", ["-", "input"], stdin: bytes.from_text(input))?
  uu.succeeds(result)
  uu.stdout_only(result, ["\tabc", "def"].join("\n") + "\n")
}
