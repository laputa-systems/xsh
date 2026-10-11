use support.uu

type ExpressionCase = {args: List[Str], status: Int}

proc expression(s: uu.Scene, args: List[Str], status: Int) [fs, process, env, error] {
  let r = uu.invoke(s, "test", args)?
  uu.fails_with_code(r, status)
  uu.no_output(r)
}

# origin: gnu test/test-N.log
test test_gnu_test_test_N_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let instant = time.now() * 1000000
  fs.set_times(uu.at(s, "file"), atime_ns: instant, mtime_ns: instant)?
  expression(s, ["-N", "file"], 1)
  let noon = time.now() / 1000 / 86400 * 86400 + 12 * 3600
  fs.set_times(uu.at(s, "file"), atime_sec: noon - 2 * 86400)?
  expression(s, ["-N", "file"], 0)
  fs.set_times(uu.at(s, "file"), mtime_sec: noon - 4 * 86400)?
  expression(s, ["-N", "file"], 1)
  uu.write(s, "file", "data\n")?
  expression(s, ["-N", "file"], 0)
}

# origin: gnu test/test-diag.log
test test_gnu_test_test_diag_log { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-o", "-a"] {
    let r = uu.invoke(s, "test", [option, "arg"])?
    uu.fails_with_code(r, 2)
    uu.stderr_only(r, f"test: '{option}': unary operator expected\n")
  }
}

# origin: gnu test/test-file.log
test test_gnu_test_test_file_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.set_mode(s, "file", 0)?
  assert uu.read(s, "file") is Err(is PermissionDenied)
  expression(s, ["-f", "file"], 0)
  expression(s, ["-f", "fail"], 1)
  for option in ["-r", "-x", "-d", "-s", "-S", "-c", "-b", "-p"] {
    expression(s, [option, "file"], 1)
  }
  for case in [
    {args: ["file", "-nt", "missing"], status: 0},
    {args: ["missing", "-nt", "file"], status: 1},
    {args: ["missing", "-ot", "file"], status: 0},
    {args: ["file", "-ot", "missing"], status: 1},
  ] { expression(s, case.args, case.status) }
  uu.touch(s, "file1")?
  uu.touch(s, "file2")?
  fs.set_times(uu.at(s, "file1"), mtime_sec: 1761188400)?
  fs.set_times(uu.at(s, "file2"), mtime_sec: 1761192000)?
  for case in [
    {args: ["file2", "-nt", "file1"], status: 0},
    {args: ["file1", "-nt", "file2"], status: 1},
    {args: ["file1", "-ot", "file2"], status: 0},
    {args: ["file2", "-ot", "file1"], status: 1},
    {args: ["missing1", "-ef", "missing2"], status: 1},
    {args: ["missing2", "-ef", "missing1"], status: 1},
    {args: ["file1", "-ef", "missing1"], status: 1},
    {args: ["missing1", "-ef", "file1"], status: 1},
    {args: ["file1", "-ef", "file1"], status: 0},
    {args: ["file1", "-ef", "file2"], status: 1},
  ] { expression(s, case.args, case.status) }
  uu.symlink(s, "file1", "symlink1")?
  uu.symlink(s, "file2", "symlink2")?
  uu.hard_link(s, "file1", "hardlink1")?
  uu.hard_link(s, "file2", "hardlink2")?
  for link in ["symlink", "hardlink"] {
    expression(s, ["file1", "-ef", f"{link}1"], 0)
    expression(s, [f"{link}1", "-ef", "file1"], 0)
    expression(s, ["file1", "-ef", f"{link}2"], 1)
    expression(s, [f"{link}2", "-ef", "file1"], 1)
    expression(s, [f"{link}1", "-ef", f"{link}2"], 1)
    expression(s, [f"{link}2", "-ef", f"{link}1"], 1)
  }
}

# origin: gnu test/test.log
test test_gnu_test_test_log { |ctx|
  let s = uu.scene(ctx)?
  expression(s, [], 1)
  let invalid = uu.invoke(s, "test", ["0x0", "-eq", "00"])?
  uu.fails_with_code(invalid, 2)
  uu.stderr_only(invalid, "test: invalid integer '0x0'\n")
  var cases: List[ExpressionCase] = [
    {args: ["-z", ""], status: 0},
    {args: ["any-string"], status: 0},
    {args: ["-n", "any-string"], status: 0},
    {args: [""], status: 1},
    {args: ["-"], status: 0},
    {args: ["--"], status: 0},
    {args: ["-0"], status: 0},
    {args: ["-f"], status: 0},
    {args: ["--help"], status: 0},
    {args: ["--version"], status: 0},
    {args: ["t", "=", "t"], status: 0},
    {args: ["t", "=", "f"], status: 1},
    {args: ["t", "==", "t"], status: 0},
    {args: ["t", "==", "f"], status: 1},
    {args: ["!", "=", "!"], status: 0},
    {args: ["=", "=", "="], status: 0},
    {args: ["(", "=", "("], status: 0},
    {args: ["t", "!=", "t"], status: 1},
    {args: ["t", "!=", "f"], status: 0},
    {args: ["!", "!=", "!"], status: 1},
    {args: ["=", "!=", "="], status: 1},
    {args: ["(", "!=", "("], status: 1},
    {args: ["t", "-a", "t"], status: 0},
    {args: ["", "-a", "t"], status: 1},
    {args: ["t", "-a", ""], status: 1},
    {args: ["", "-a", ""], status: 1},
    {args: ["t", "-o", "t"], status: 0},
    {args: ["", "-o", "t"], status: 0},
    {args: ["t", "-o", ""], status: 0},
    {args: ["", "-o", ""], status: 1},
    {args: ["-t"], status: 0},
    {args: ["-t", "1"], status: 1},
    {args: ["a", "<", "b"], status: 0},
    {args: ["a", "<", "a"], status: 1},
    {args: ["b", "<", "a"], status: 1},
    {args: ["b", ">", "a"], status: 0},
    {args: ["a", ">", "a"], status: 1},
    {args: ["a", ">", "b"], status: 1},
  ]
  let numeric: List[ExpressionCase] = [
    {args: ["9", "-eq", "9"], status: 0},
    {args: ["0", "-eq", "0"], status: 0},
    {args: ["0", "-eq", "00"], status: 0},
    {args: ["8", "-eq", "9"], status: 1},
    {args: ["1", "-eq", "0"], status: 1},
    {args: ["18446744073709551616", "-eq", "0"], status: 1},
    {args: ["0", "-eq", " 0 "], status: 0},
    {args: ["-l", "abc", "-eq", "3"], status: 0},
    {args: ["-l", "abc", "-eq", "2"], status: 1},
    {args: ["5", "-gt", "5"], status: 1},
    {args: ["5", "-gt", "4"], status: 0},
    {args: ["4", "-gt", "5"], status: 1},
    {args: ["-1", "-gt", "-2"], status: 0},
    {args: ["18446744073709551616", "-gt", "-9223372036854775809"], status: 0},
    {args: ["-l", "abc", "-gt", "3"], status: 1},
    {args: ["-l", "abc", "-gt", "2"], status: 0},
    {args: ["2", "-gt", "-l", "abc"], status: 1},
    {args: ["5", "-lt", "5"], status: 1},
    {args: ["5", "-lt", "4"], status: 1},
    {args: ["4", "-lt", "5"], status: 0},
    {args: ["-1", "-lt", "-2"], status: 1},
    {args: ["-9223372036854775809", "-lt", "18446744073709551616"], status: 0},
    {args: ["-l", "abc", "-lt", "3"], status: 1},
    {args: ["-l", "abc", "-lt", "2"], status: 1},
    {args: ["2", "-lt", "-l", "abc"], status: 0},
  ]
  cases = cases.extend(numeric)
  for case in numeric {
    let inverse = [if word == "-eq" { "-ne" } else if word == "-gt" { "-le" } else if word == "-lt" { "-ge" } else { word } for word in case.args]
    cases += [{args: inverse, status: 1 - case.status}]
  }
  for operator in ["-a", "-o"] {
    for left in ["-a", "-o", ""] {
      for right in ["-a", "-o", ""] {
        let false_value = if operator == "-o" { left == "" and right == "" } else { left == "" or right == "" }
        cases += [{args: [left, operator, right], status: if false_value { 1 } else { 0 }}]
      }
    }
  }
  assert cases.len() == 106
  for case in cases {
    expression(s, case.args, case.status)
    expression(s, ["!"].extend(case.args), 1 - case.status)
    let grouped = ["("].extend(case.args).extend([")"])
    expression(s, grouped, case.status)
    expression(s, ["!"].extend(grouped), 1 - case.status)
    expression(s, ["!", "!"].extend(grouped), case.status)
  }
  for case in [
    {args: ["(", "=", ")"], status: 1},
    {args: ["(", "!=", ")"], status: 0},
    {args: ["(", "", ")"], status: 1},
    {args: ["(", "(", ")"], status: 0},
    {args: ["(", ")", ")"], status: 0},
    {args: ["(", "!", ")"], status: 0},
    {args: ["(", "-a", ")"], status: 0},
  ] {
    expression(s, case.args, case.status)
    expression(s, ["!"].extend(case.args), 1 - case.status)
  }
}
