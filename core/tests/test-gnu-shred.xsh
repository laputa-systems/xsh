use support.uu

# origin: gnu shred/shred-exact.log
test test_gnu_shred_shred_exact_log { |ctx|
  let s = uu.scene(ctx)?
  for option in ["--exact", "--zero"] {
    for row in [{name: "a", text: "a\n"}, {name: "b", text: "bb\n"}, {name: "c", text: "ccc\n"}] { uu.write(s, row.name, row.text)? }
    uu.succeeds(uu.invoke(s, "shred", ["--remove", option, "a", "b"])?)
    for name in ["a", "b"] { assert ! uu.exists(s, name)? }
    uu.succeeds(uu.invoke(s, "shred", ["--remove", option, "c"])?)
    assert ! uu.exists(s, "c")?
  }
  uu.touch(s, "file.slop")?
  uu.truncate(s, "file.slop", 1048577)?
  uu.succeeds(uu.invoke(s, "shred", ["--exact", "-n2", "file.slop"])?)
  uu.truncate(s, "file.slop", 1)?
  uu.succeeds(uu.invoke(s, "shred", ["--exact", "-n2", "file.slop"])?)
}

# origin: gnu shred/shred-passes.log
test test_gnu_shred_shred_passes_log { |ctx|
  let s = uu.scene(ctx)?
  let removal = "shred: f: removing\nshred: f: renamed to 0\nshred: f: removed\n"
  uu.write(s, "f", "1")?
  let first = uu.invoke(s, "shred", ["-v", "-u", "f"])?
  uu.succeeds(first)
  uu.stderr_is(first, "shred: f: pass 1/3 (random)...\nshred: f: pass 2/3 (random)...\nshred: f: pass 3/3 (random)...\n" + removal)
  uu.touch(s, "f")?
  let empty = uu.invoke(s, "shred", ["-v", "-u", "f"])?
  uu.succeeds(empty)
  uu.stderr_is(empty, removal)
  uu.write_bytes(s, "Us", bytes.from_ints([85 for _ in range(102400)])?)?
  uu.write(s, "f", "1")?
  let patterns = ["random", "ffffff", "924924", "888888", "db6db6", "777777", "492492", "bbbbbb", "555555", "aaaaaa", "random", "6db6db", "249249", "999999", "111111", "000000", "b6db6d", "eeeeee", "333333", "random"]
  let expected = [f"shred: f: pass {i + 1}/20 ({patterns[i]})...\n" for i in range(patterns.len())].join("") + removal
  let twenty = uu.invoke(s, "shred", ["-v", "-u", "-n20", "-s4096", "--random-source=Us", "f"])?
  uu.succeeds(twenty)
  uu.stderr_is(twenty, expected)
  for size in [1, 2, 6, 7, 8] {
    if ! uu.exists(s, "shred.pattern.umr.size")? { uu.touch(s, "shred.pattern.umr.size")? }
    uu.succeeds(uu.invoke(s, "shred", ["-n4", f"-s{size}", "shred.pattern.umr.size"])?)
  }
}

# origin: gnu shred/shred-size.log
test test_gnu_shred_shred_size_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "1234\n")?
  let negative = uu.invoke(s, "shred", ["-s-2", "f"])?
  uu.fails(negative)
  uu.stderr_is(negative, "shred: invalid file size: '-2'\n")
  for row in [{option: "-s010", size: 8}, {option: "-s0x10", size: 16}] {
    uu.succeeds(uu.invoke(s, "shred", [row.option, "f"])?)
    assert uu.size(s, "f")? == row.size
  }
}
