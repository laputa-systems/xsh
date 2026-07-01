pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_loc(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "loc-root")?

  fp"${root}/main.rs".write("""fn main() {
  println!("hello");
}
""")?

  fp"${root}/lib.rs".write("""// empty
""")?

  let output = run.text xsh_bin() "showcase/loc.xsh" -- $root ?
  test.contains(output, "rs")?
  test.contains(output, "2 files")?
}
