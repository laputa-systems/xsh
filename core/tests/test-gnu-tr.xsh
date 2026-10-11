use support.uu

proc check_case(s: uu.Scene, name: Str, args: List[Str], input: Bytes, output: Bytes, diagnostic: Str, code: Int) [fs, process, env, error] -> Result[Unit, Error] {
  let r = if name.ends_with(".r") {
    uu.write_bytes(s, "input", input)?
    uu.invoke_from_path(s, "tr", args, uu.at(s, "input"), timeout: 30s)?
  } else { uu.invoke(s, "tr", args, stdin: input, timeout: 30s)? }
  uu.fails_with_code(r, code)
  uu.stdout_is_bytes(r, output)
  uu.stderr_is(r, diagnostic)
  Ok()
}

# origin: gnu tr/tr.log
test test_gnu_tr_tr_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "1.r", ["abcd", "[]*]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([93, 93, 93, 93])?, "", 0)?
  check_case(s, "1.p", ["abcd", "[]*]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([93, 93, 93, 93])?, "", 0)?
  check_case(s, "2.r", ["abc", "[%*]xyz"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([120, 121, 122])?, "", 0)?
  check_case(s, "2.p", ["abc", "[%*]xyz"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([120, 121, 122])?, "", 0)?
  check_case(s, "3.r", ["", "[.*]"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([97, 98, 99])?, "", 0)?
  check_case(s, "3.p", ["", "[.*]"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([97, 98, 99])?, "", 0)?
  check_case(s, "4.r", ["-t", "abcd", "xy"], bytes.from_ints([97, 98, 99, 100, 101])?, bytes.from_ints([120, 121, 99, 100, 101])?, "", 0)?
  check_case(s, "4.p", ["-t", "abcd", "xy"], bytes.from_ints([97, 98, 99, 100, 101])?, bytes.from_ints([120, 121, 99, 100, 101])?, "", 0)?
  check_case(s, "5.r", ["abcd", "xy"], bytes.from_ints([97, 98, 99, 100, 101])?, bytes.from_ints([120, 121, 121, 121, 101])?, "", 0)?
  check_case(s, "5.p", ["abcd", "xy"], bytes.from_ints([97, 98, 99, 100, 101])?, bytes.from_ints([120, 121, 121, 121, 101])?, "", 0)?
  check_case(s, "6.r", ["abcd", "x[y*]"], bytes.from_ints([97, 98, 99, 100, 101])?, bytes.from_ints([120, 121, 121, 121, 101])?, "", 0)?
  check_case(s, "6.p", ["abcd", "x[y*]"], bytes.from_ints([97, 98, 99, 100, 101])?, bytes.from_ints([120, 121, 121, 121, 101])?, "", 0)?
  check_case(s, "7.r", ["-s", "a-p", "%[.*]$"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112])?, bytes.from_ints([37, 46, 36])?, "", 0)?
  check_case(s, "7.p", ["-s", "a-p", "%[.*]$"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112])?, bytes.from_ints([37, 46, 36])?, "", 0)?
  check_case(s, "8.r", ["-s", "a-p", "[.*]$"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112])?, bytes.from_ints([46, 36])?, "", 0)?
  check_case(s, "8.p", ["-s", "a-p", "[.*]$"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112])?, bytes.from_ints([46, 36])?, "", 0)?
  check_case(s, "9.r", ["-s", "a-p", "%[.*]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112])?, bytes.from_ints([37, 46])?, "", 0)?
  check_case(s, "9.p", ["-s", "a-p", "%[.*]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112])?, bytes.from_ints([37, 46])?, "", 0)?
  check_case(s, "a.r", ["-s", "[a-z]"], bytes.from_ints([97, 97, 98, 98, 99, 99])?, bytes.from_ints([97, 98, 99])?, "", 0)?
  check_case(s, "a.p", ["-s", "[a-z]"], bytes.from_ints([97, 97, 98, 98, 99, 99])?, bytes.from_ints([97, 98, 99])?, "", 0)?
  check_case(s, "b.r", ["-s", "[a-c]"], bytes.from_ints([97, 97, 98, 98, 99, 99])?, bytes.from_ints([97, 98, 99])?, "", 0)?
  check_case(s, "b.p", ["-s", "[a-c]"], bytes.from_ints([97, 97, 98, 98, 99, 99])?, bytes.from_ints([97, 98, 99])?, "", 0)?
  check_case(s, "c.r", ["-s", "[a-b]"], bytes.from_ints([97, 97, 98, 98, 99, 99])?, bytes.from_ints([97, 98, 99, 99])?, "", 0)?
  check_case(s, "c.p", ["-s", "[a-b]"], bytes.from_ints([97, 97, 98, 98, 99, 99])?, bytes.from_ints([97, 98, 99, 99])?, "", 0)?
  check_case(s, "d.r", ["-s", "[b-c]"], bytes.from_ints([97, 97, 98, 98, 99, 99])?, bytes.from_ints([97, 97, 98, 99])?, "", 0)?
  check_case(s, "d.p", ["-s", "[b-c]"], bytes.from_ints([97, 97, 98, 98, 99, 99])?, bytes.from_ints([97, 97, 98, 99])?, "", 0)?
  check_case(s, "e.r", ["-s", "[\\0-\\5]"], bytes.from_ints([0, 0, 97, 1, 1, 98, 2, 2, 2, 99, 3, 3, 3, 100, 4, 4, 4, 4, 101, 5, 5])?, bytes.from_ints([0, 97, 1, 98, 2, 99, 3, 100, 4, 101, 5])?, "", 0)?
  check_case(s, "e.p", ["-s", "[\\0-\\5]"], bytes.from_ints([0, 0, 97, 1, 1, 98, 2, 2, 2, 99, 3, 3, 3, 100, 4, 4, 4, 4, 101, 5, 5])?, bytes.from_ints([0, 97, 1, 98, 2, 99, 3, 100, 4, 101, 5])?, "", 0)?
  check_case(s, "f.r", ["-d", "[=[=]"], bytes.from_ints([91, 91, 91, 91, 91, 91, 91, 93, 93, 93, 93, 93, 93, 93, 93])?, bytes.from_ints([93, 93, 93, 93, 93, 93, 93, 93])?, "", 0)?
  check_case(s, "f.p", ["-d", "[=[=]"], bytes.from_ints([91, 91, 91, 91, 91, 91, 91, 93, 93, 93, 93, 93, 93, 93, 93])?, bytes.from_ints([93, 93, 93, 93, 93, 93, 93, 93])?, "", 0)?
  check_case(s, "g.r", ["-d", "[=]=]"], bytes.from_ints([91, 91, 91, 91, 91, 91, 91, 93, 93, 93, 93, 93, 93, 93, 93])?, bytes.from_ints([91, 91, 91, 91, 91, 91, 91])?, "", 0)?
  check_case(s, "g.p", ["-d", "[=]=]"], bytes.from_ints([91, 91, 91, 91, 91, 91, 91, 93, 93, 93, 93, 93, 93, 93, 93])?, bytes.from_ints([91, 91, 91, 91, 91, 91, 91])?, "", 0)?
  check_case(s, "h.r", ["-d", "[:xdigit:]"], bytes.from_ints([48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 97, 99, 98, 100, 101, 102, 65, 66, 67, 68, 69, 70])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "h.p", ["-d", "[:xdigit:]"], bytes.from_ints([48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 97, 99, 98, 100, 101, 102, 65, 66, 67, 68, 69, 70])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "i.r", ["-d", "[:xdigit:]"], bytes.from_ints([119, 48, 120, 49, 121, 50, 122, 51, 52, 53, 54, 55, 56, 57, 97, 99, 98, 100, 101, 102, 65, 66, 67, 68, 69, 70, 122])?, bytes.from_ints([119, 120, 121, 122, 122])?, "", 0)?
  check_case(s, "i.p", ["-d", "[:xdigit:]"], bytes.from_ints([119, 48, 120, 49, 121, 50, 122, 51, 52, 53, 54, 55, 56, 57, 97, 99, 98, 100, 101, 102, 65, 66, 67, 68, 69, 70, 122])?, bytes.from_ints([119, 120, 121, 122, 122])?, "", 0)?
  check_case(s, "j.r", ["-d", "[:digit:]"], bytes.from_ints([48, 49, 50, 51, 52, 53, 54, 55, 56, 57])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "j.p", ["-d", "[:digit:]"], bytes.from_ints([48, 49, 50, 51, 52, 53, 54, 55, 56, 57])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "k.r", ["-d", "[:digit:]"], bytes.from_ints([97, 48, 98, 49, 99, 50, 100, 51, 101, 52, 102, 53, 103, 54, 104, 55, 105, 56, 106, 57, 107])?, bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107])?, "", 0)?
  check_case(s, "k.p", ["-d", "[:digit:]"], bytes.from_ints([97, 48, 98, 49, 99, 50, 100, 51, 101, 52, 102, 53, 103, 54, 104, 55, 105, 56, 106, 57, 107])?, bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107])?, "", 0)?
  check_case(s, "l.r", ["-d", "[:lower:]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "l.p", ["-d", "[:lower:]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "m.r", ["-d", "[:upper:]"], bytes.from_ints([65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "m.p", ["-d", "[:upper:]"], bytes.from_ints([65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "n.r", ["-d", "[:lower:][:upper:]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "n.p", ["-d", "[:lower:][:upper:]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "o.r", ["-d", "[:alpha:]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "o.p", ["-d", "[:alpha:]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "p.r", ["-d", "[:alnum:]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "p.p", ["-d", "[:alnum:]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "q.r", ["-d", "[:alnum:]"], bytes.from_ints([46, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 46])?, bytes.from_ints([46, 46])?, "", 0)?
  check_case(s, "q.p", ["-d", "[:alnum:]"], bytes.from_ints([46, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 46])?, bytes.from_ints([46, 46])?, "", 0)?
  check_case(s, "r.r", ["-ds", "[:alnum:]", "."], bytes.from_ints([46, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 46])?, bytes.from_ints([46])?, "", 0)?
  check_case(s, "r.p", ["-ds", "[:alnum:]", "."], bytes.from_ints([46, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 46])?, bytes.from_ints([46])?, "", 0)?
  check_case(s, "s.r", ["-cs", "[:alnum:]", "\\n"], bytes.from_ints([84, 104, 101, 32, 98, 105, 103, 32, 98, 108, 97, 99, 107, 32, 102, 111, 120, 32, 106, 117, 109, 112, 101, 100, 32, 111, 118, 101, 114, 32, 116, 104, 101, 32, 102, 101, 110, 99, 101, 46])?, bytes.from_ints([84, 104, 101, 10, 98, 105, 103, 10, 98, 108, 97, 99, 107, 10, 102, 111, 120, 10, 106, 117, 109, 112, 101, 100, 10, 111, 118, 101, 114, 10, 116, 104, 101, 10, 102, 101, 110, 99, 101, 10])?, "", 0)?
  check_case(s, "s.p", ["-cs", "[:alnum:]", "\\n"], bytes.from_ints([84, 104, 101, 32, 98, 105, 103, 32, 98, 108, 97, 99, 107, 32, 102, 111, 120, 32, 106, 117, 109, 112, 101, 100, 32, 111, 118, 101, 114, 32, 116, 104, 101, 32, 102, 101, 110, 99, 101, 46])?, bytes.from_ints([84, 104, 101, 10, 98, 105, 103, 10, 98, 108, 97, 99, 107, 10, 102, 111, 120, 10, 106, 117, 109, 112, 101, 100, 10, 111, 118, 101, 114, 10, 116, 104, 101, 10, 102, 101, 110, 99, 101, 10])?, "", 0)?
  check_case(s, "t.r", ["-cs", "[:alnum:]", "[\\n*]"], bytes.from_ints([84, 104, 101, 32, 98, 105, 103, 32, 98, 108, 97, 99, 107, 32, 102, 111, 120, 32, 106, 117, 109, 112, 101, 100, 32, 111, 118, 101, 114, 32, 116, 104, 101, 32, 102, 101, 110, 99, 101, 46])?, bytes.from_ints([84, 104, 101, 10, 98, 105, 103, 10, 98, 108, 97, 99, 107, 10, 102, 111, 120, 10, 106, 117, 109, 112, 101, 100, 10, 111, 118, 101, 114, 10, 116, 104, 101, 10, 102, 101, 110, 99, 101, 10])?, "", 0)?
  check_case(s, "t.p", ["-cs", "[:alnum:]", "[\\n*]"], bytes.from_ints([84, 104, 101, 32, 98, 105, 103, 32, 98, 108, 97, 99, 107, 32, 102, 111, 120, 32, 106, 117, 109, 112, 101, 100, 32, 111, 118, 101, 114, 32, 116, 104, 101, 32, 102, 101, 110, 99, 101, 46])?, bytes.from_ints([84, 104, 101, 10, 98, 105, 103, 10, 98, 108, 97, 99, 107, 10, 102, 111, 120, 10, 106, 117, 109, 112, 101, 100, 10, 111, 118, 101, 114, 10, 116, 104, 101, 10, 102, 101, 110, 99, 101, 10])?, "", 0)?
  check_case(s, "u.r", ["-ds", "b", "a"], bytes.from_ints([97, 97, 98, 98, 97, 97])?, bytes.from_ints([97])?, "", 0)?
  check_case(s, "u.p", ["-ds", "b", "a"], bytes.from_ints([97, 97, 98, 98, 97, 97])?, bytes.from_ints([97])?, "", 0)?
  check_case(s, "v.r", ["-ds", "[:xdigit:]", "Z"], bytes.from_ints([90, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 97, 99, 98, 100, 101, 102, 65, 66, 67, 68, 69, 70, 90, 90])?, bytes.from_ints([90])?, "", 0)?
  check_case(s, "v.p", ["-ds", "[:xdigit:]", "Z"], bytes.from_ints([90, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 97, 99, 98, 100, 101, 102, 65, 66, 67, 68, 69, 70, 90, 90])?, bytes.from_ints([90])?, "", 0)?
  check_case(s, "w.r", ["-ds", "\\350", "\\345"], bytes.from_ints([192, 193, 255, 229, 229, 232, 229])?, bytes.from_ints([192, 193, 255, 229])?, "", 0)?
  check_case(s, "w.p", ["-ds", "\\350", "\\345"], bytes.from_ints([192, 193, 255, 229, 229, 232, 229])?, bytes.from_ints([192, 193, 255, 229])?, "", 0)?
  check_case(s, "x.r", ["-s", "abcdefghijklmn", "[:*016]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112])?, bytes.from_ints([58, 111, 112])?, "", 0)?
  check_case(s, "x.p", ["-s", "abcdefghijklmn", "[:*016]"], bytes.from_ints([97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112])?, bytes.from_ints([58, 111, 112])?, "", 0)?
  check_case(s, "y.r", ["-d", "a-z"], bytes.from_ints([97, 98, 99, 32, 36, 99, 111, 100, 101])?, bytes.from_ints([32, 36])?, "", 0)?
  check_case(s, "y.p", ["-d", "a-z"], bytes.from_ints([97, 98, 99, 32, 36, 99, 111, 100, 101])?, bytes.from_ints([32, 36])?, "", 0)?
  check_case(s, "z.r", ["-ds", "a-z", "$."], bytes.from_ints([97, 46, 98, 46, 99, 32, 36, 36, 36, 36, 99, 111, 100, 101, 92])?, bytes.from_ints([46, 32, 36, 92])?, "", 0)?
  check_case(s, "z.p", ["-ds", "a-z", "$."], bytes.from_ints([97, 46, 98, 46, 99, 32, 36, 36, 36, 36, 99, 111, 100, 101, 92])?, bytes.from_ints([46, 32, 36, 92])?, "", 0)?
  check_case(s, "range-a-a.r", ["a-a", "z"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([122, 98, 99])?, "", 0)?
  check_case(s, "range-a-a.p", ["a-a", "z"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([122, 98, 99])?, "", 0)?
  check_case(s, "null.r", ["a", ""], bytes.from_ints([])?, bytes.from_ints([])?, "tr: when not truncating set1, string2 must be non-empty\n", 1)?
  check_case(s, "null.p", ["a", ""], bytes.from_ints([])?, bytes.from_ints([])?, "tr: when not truncating set1, string2 must be non-empty\n", 1)?
  check_case(s, "upcase.r", ["[:lower:]", "[:upper:]"], bytes.from_ints([97, 98, 99, 120, 121, 122, 65, 66, 67, 88, 89, 90])?, bytes.from_ints([65, 66, 67, 88, 89, 90, 65, 66, 67, 88, 89, 90])?, "", 0)?
  check_case(s, "upcase.p", ["[:lower:]", "[:upper:]"], bytes.from_ints([97, 98, 99, 120, 121, 122, 65, 66, 67, 88, 89, 90])?, bytes.from_ints([65, 66, 67, 88, 89, 90, 65, 66, 67, 88, 89, 90])?, "", 0)?
  check_case(s, "dncase.r", ["[:upper:]", "[:lower:]"], bytes.from_ints([97, 98, 99, 120, 121, 122, 65, 66, 67, 88, 89, 90])?, bytes.from_ints([97, 98, 99, 120, 121, 122, 97, 98, 99, 120, 121, 122])?, "", 0)?
  check_case(s, "dncase.p", ["[:upper:]", "[:lower:]"], bytes.from_ints([97, 98, 99, 120, 121, 122, 65, 66, 67, 88, 89, 90])?, bytes.from_ints([97, 98, 99, 120, 121, 122, 97, 98, 99, 120, 121, 122])?, "", 0)?
  check_case(s, "rep-cclass.r", ["a[=*2][=c=]", "xyyz"], bytes.from_ints([97, 61, 99])?, bytes.from_ints([120, 121, 122])?, "", 0)?
  check_case(s, "rep-cclass.p", ["a[=*2][=c=]", "xyyz"], bytes.from_ints([97, 61, 99])?, bytes.from_ints([120, 121, 122])?, "", 0)?
  check_case(s, "rep-1.r", ["[:*3][:digit:]", "a-m"], bytes.from_ints([58, 49, 50, 51, 57])?, bytes.from_ints([99, 101, 102, 103, 109])?, "", 0)?
  check_case(s, "rep-1.p", ["[:*3][:digit:]", "a-m"], bytes.from_ints([58, 49, 50, 51, 57])?, bytes.from_ints([99, 101, 102, 103, 109])?, "", 0)?
  check_case(s, "rep-2.r", ["a[b*512]c", "1[x*]2"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([49, 120, 50])?, "", 0)?
  check_case(s, "rep-2.p", ["a[b*512]c", "1[x*]2"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([49, 120, 50])?, "", 0)?
  check_case(s, "rep-3.r", ["a[b*513]c", "1[x*]2"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([49, 120, 50])?, "", 0)?
  check_case(s, "rep-3.p", ["a[b*513]c", "1[x*]2"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([49, 120, 50])?, "", 0)?
  check_case(s, "o-rep-1.r", ["[b*08]", "[x*]"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: invalid repeat count '08' in [c*n] construct\n", 1)?
  check_case(s, "o-rep-1.p", ["[b*08]", "[x*]"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: invalid repeat count '08' in [c*n] construct\n", 1)?
  check_case(s, "o-rep-2.r", ["[b*010]cd", "[a*7]BC[x*]"], bytes.from_ints([98, 99, 100])?, bytes.from_ints([66, 67, 120])?, "", 0)?
  check_case(s, "o-rep-2.p", ["[b*010]cd", "[a*7]BC[x*]"], bytes.from_ints([98, 99, 100])?, bytes.from_ints([66, 67, 120])?, "", 0)?
  check_case(s, "esc.r", ["a\\-z", "A-Z"], bytes.from_ints([97, 98, 99, 45, 122])?, bytes.from_ints([65, 98, 99, 66, 67])?, "", 0)?
  check_case(s, "esc.p", ["a\\-z", "A-Z"], bytes.from_ints([97, 98, 99, 45, 122])?, bytes.from_ints([65, 98, 99, 66, 67])?, "", 0)?
  check_case(s, "bs-055.r", ["a\\055b", "def"], bytes.from_ints([97, 45, 98])?, bytes.from_ints([100, 101, 102])?, "", 0)?
  check_case(s, "bs-055.p", ["a\\055b", "def"], bytes.from_ints([97, 45, 98])?, bytes.from_ints([100, 101, 102])?, "", 0)?
  check_case(s, "bs-at-end.r", ["\\", "x"], bytes.from_ints([92])?, bytes.from_ints([120])?, "tr: warning: an unescaped backslash at end of string is not portable\n", 0)?
  check_case(s, "bs-at-end.p", ["\\", "x"], bytes.from_ints([92])?, bytes.from_ints([120])?, "tr: warning: an unescaped backslash at end of string is not portable\n", 0)?
  check_case(s, "ross-0a.r", ["-cs", "[:upper:]", "X[Y*]"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: when translating with complemented character classes,\nstring2 must map all characters in the domain to one\n", 1)?
  check_case(s, "ross-0a.p", ["-cs", "[:upper:]", "X[Y*]"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: when translating with complemented character classes,\nstring2 must map all characters in the domain to one\n", 1)?
  check_case(s, "ross-0b.r", ["-cs", "[:cntrl:]", "X[Y*]"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: when translating with complemented character classes,\nstring2 must map all characters in the domain to one\n", 1)?
  check_case(s, "ross-0b.p", ["-cs", "[:cntrl:]", "X[Y*]"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: when translating with complemented character classes,\nstring2 must map all characters in the domain to one\n", 1)?
  check_case(s, "ross-1a.r", ["-cs", "[:upper:]", "[X*]"], bytes.from_ints([65, 77, 90, 97, 109, 122, 49, 50, 51, 46, 45, 43, 65, 77, 90])?, bytes.from_ints([65, 77, 90, 88, 65, 77, 90])?, "", 0)?
  check_case(s, "ross-1a.p", ["-cs", "[:upper:]", "[X*]"], bytes.from_ints([65, 77, 90, 97, 109, 122, 49, 50, 51, 46, 45, 43, 65, 77, 90])?, bytes.from_ints([65, 77, 90, 88, 65, 77, 90])?, "", 0)?
  check_case(s, "ross-1b.r", ["-cs", "[:upper:][:digit:]", "[Z*]"], bytes.from_ints([])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "ross-1b.p", ["-cs", "[:upper:][:digit:]", "[Z*]"], bytes.from_ints([])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "ross-2.r", ["-dcs", "[:lower:]", "n-rs-z"], bytes.from_ints([97, 109, 122, 65, 77, 90, 49, 50, 51, 46, 45, 43, 97, 109, 122])?, bytes.from_ints([97, 109, 122, 97, 109, 122])?, "", 0)?
  check_case(s, "ross-2.p", ["-dcs", "[:lower:]", "n-rs-z"], bytes.from_ints([97, 109, 122, 65, 77, 90, 49, 50, 51, 46, 45, 43, 97, 109, 122])?, bytes.from_ints([97, 109, 122, 97, 109, 122])?, "", 0)?
  check_case(s, "ross-3.r", ["-ds", "[:xdigit:]", "[:alnum:]"], bytes.from_ints([46, 90, 65, 66, 67, 68, 69, 70, 71, 122, 97, 98, 99, 100, 101, 102, 103, 46, 48, 49, 50, 51, 52, 53, 54, 55, 56, 56, 56, 57, 57, 46, 71, 71])?, bytes.from_ints([46, 90, 71, 122, 103, 46, 46, 71])?, "", 0)?
  check_case(s, "ross-3.p", ["-ds", "[:xdigit:]", "[:alnum:]"], bytes.from_ints([46, 90, 65, 66, 67, 68, 69, 70, 71, 122, 97, 98, 99, 100, 101, 102, 103, 46, 48, 49, 50, 51, 52, 53, 54, 55, 56, 56, 56, 57, 57, 46, 71, 71])?, bytes.from_ints([46, 90, 71, 122, 103, 46, 46, 71])?, "", 0)?
  check_case(s, "ross-4.r", ["-dcs", "[:alnum:]", "[:digit:]"], bytes.from_ints([])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "ross-4.p", ["-dcs", "[:alnum:]", "[:digit:]"], bytes.from_ints([])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "ross-5.r", ["-dc", "[:lower:]"], bytes.from_ints([])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "ross-5.p", ["-dc", "[:lower:]"], bytes.from_ints([])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "ross-6.r", ["-dc", "[:upper:]"], bytes.from_ints([])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "ross-6.p", ["-dc", "[:upper:]"], bytes.from_ints([])?, bytes.from_ints([])?, "", 0)?
  check_case(s, "empty-eq.r", ["[==]", "x"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: missing equivalence class character '[==]'\n", 1)?
  check_case(s, "empty-eq.p", ["[==]", "x"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: missing equivalence class character '[==]'\n", 1)?
  check_case(s, "empty-cc.r", ["[::]", "x"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: missing character class name '[::]'\n", 1)?
  check_case(s, "empty-cc.p", ["[::]", "x"], bytes.from_ints([])?, bytes.from_ints([])?, "tr: missing character class name '[::]'\n", 1)?
  check_case(s, "repeat-bs-9.r", ["abc", "[b*\\9]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([91, 98, 42, 100])?, "", 0)?
  check_case(s, "repeat-bs-9.p", ["abc", "[b*\\9]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([91, 98, 42, 100])?, "", 0)?
  check_case(s, "repeat-0.r", ["abc", "[b*0]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([98, 98, 98, 100])?, "", 0)?
  check_case(s, "repeat-0.p", ["abc", "[b*0]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([98, 98, 98, 100])?, "", 0)?
  check_case(s, "repeat-zeros.r", ["abc", "[b*00000000000000000000]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([98, 98, 98, 100])?, "", 0)?
  check_case(s, "repeat-zeros.p", ["abc", "[b*00000000000000000000]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([98, 98, 98, 100])?, "", 0)?
  check_case(s, "repeat-compl.r", ["-c", "[a*65536]\\n", "[b*]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([97, 98, 98, 98])?, "", 0)?
  check_case(s, "repeat-compl.p", ["-c", "[a*65536]\\n", "[b*]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([97, 98, 98, 98])?, "", 0)?
  check_case(s, "repeat-xC.r", ["-C", "[a*65536]\\n", "[b*]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([97, 98, 98, 98])?, "", 0)?
  check_case(s, "repeat-xC.p", ["-C", "[a*65536]\\n", "[b*]"], bytes.from_ints([97, 98, 99, 100])?, bytes.from_ints([97, 98, 98, 98])?, "", 0)?
  check_case(s, "fowler-1.r", ["ah", "-H"], bytes.from_ints([97, 104, 97])?, bytes.from_ints([45, 72, 45])?, "", 0)?
  check_case(s, "fowler-1.p", ["ah", "-H"], bytes.from_ints([97, 104, 97])?, bytes.from_ints([45, 72, 45])?, "", 0)?
  check_case(s, "no-abort-1.r", ["-c", "a", "[b*256]"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([97, 98, 98])?, "", 0)?
  check_case(s, "no-abort-1.p", ["-c", "a", "[b*256]"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([97, 98, 98])?, "", 0)?
  check_case(s, "invalid-class.r", ["[:fooclass:]", "x"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([])?, "tr: invalid character class 'fooclass'\n", 1)?
  check_case(s, "invalid-class.p", ["[:fooclass:]", "x"], bytes.from_ints([97, 98, 99])?, bytes.from_ints([])?, "tr: invalid character class 'fooclass'\n", 1)?
}

# origin: gnu tr/tr-case-class.log
test test_gnu_tr_tr_case_class_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [["[:lower:]", "0-9"], ["[:lower:][:lower:]", "[:upper:]0-9"]] {
    let r = uu.invoke(s, "tr", args, stdin: b"abcdefghijklmnopqrstuvwxyz\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "01234567899999999999999999\n")
  }
  for args in [
    ["A-Z[:lower:]", "a-y[:upper:]"],
    ["[:upper:][:lower:]", "a-y[:upper:]"],
    ["A-Y[:lower:]", "a-z[:upper:]"],
    ["A-Z[:lower:]", "[:lower:][:upper:]"],
    ["A-Z[:lower:]", "[:lower:]A-Z"],
  ] { uu.fails_with_code(uu.invoke(s, "tr", args)?, 1) }
  for args in [["[:upper:][:lower:]", "a-z[:upper:]"], ["[:upper:][:lower:]", "[:upper:]a-z"]] {
    uu.succeeds(uu.invoke(s, "tr", args)?)
  }
  let message = "tr: when translating with string1 longer than string2,\nthe latter string must not end with a character class\n"
  let ending = uu.invoke(s, "tr", ["[:upper:] ", "[:lower:]"])?
  uu.fails(ending)
  uu.stderr_only(ending, message)
  for case in [
    {args: ["[:lower:]", "[.*]"], input: "#$%123abcABC\n", expected: "#$%123...ABC\n"},
    {args: ["[:upper:]", "[.*]"], input: "#$%123abcABC\n", expected: "#$%123abc...\n"},
    {args: ["[:lower:].", "[:upper:]x"], input: "abc.\n", expected: "ABCx\n"},
  ] {
    let r = uu.invoke(s, "tr", case.args, stdin: bytes.from_text(case.input))?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected)
  }
  let locale = {LC_ALL: "en_US.ISO-8859-1"}
  uu.succeeds(uu.invoke(s, "tr", ["[:upper:]", "[:lower:]"], vars: locale)?)
  let locale_ending = uu.invoke(s, "tr", ["[:upper:] ", "[:lower:]"], vars: locale)?
  uu.fails(locale_ending)
  uu.stderr_only(locale_ending, message)
  for case in [
    {args: ["ab[:lower:]", "0-1[:upper:]"], input: "abc.xyz\n", expected: "ABC.XYZ\n"},
    {args: ["[:upper:]- ", "[:lower:]_"], input: "ABC- XYZ\n", expected: "abc__xyz\n"},
    {args: ["[:upper:]A-B", "[:lower:]0"], input: "ABCDEFGHIJKLMNOPQRSTUVWXYZ\n", expected: "00cdefghijklmnopqrstuvwxyz\n"},
    {args: ["-t", "[:lower:]a", "[:upper:]0"], input: "a\n", expected: "0\n"},
    {args: ["-t", "[:lower:][:lower:]a", "[:lower:][:upper:]0"], input: "a\n", expected: "0\n"},
  ] {
    let r = uu.invoke(s, "tr", case.args, stdin: bytes.from_text(case.input), vars: locale)?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected)
  }
  uu.fails_with_code(uu.invoke(s, "tr", ["-c", "[:upper:]\\000-\\370", "[:lower:]"], vars: locale)?, 1)
}
