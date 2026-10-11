use support.uu

# origin: gnu join/join-utf8.log
test test_gnu_join_join_utf8_log { |ctx|
  let s = uu.scene(ctx)?
  for separator in [" ", "|", "×", "–", "𐏐"] {
    uu.write(s, "a", f"0{separator}A\n1{separator}a\n2{separator}b\n4{separator}c\n")?
    uu.write(s, "b", f"0{separator}B\n1{separator}d\n3{separator}e\n4{separator}\0f\n")?
    let delimiter = if separator == " " { [] } else { ["-t" + separator] }
    let r = uu.invoke(s, "join", delimiter.extend(["-a1", "-a2", "-eouch", "-o0,1.2,2.2", "a", "b"]), vars: {LC_ALL: "fr_FR.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, bytes.from_text(f"0{separator}A{separator}B\n1{separator}a{separator}d\n2{separator}b{separator}ouch\n3{separator}ouch{separator}e\n4{separator}c{separator}\0f\n"))
  }
}
