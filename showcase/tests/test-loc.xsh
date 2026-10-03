test test_loc { |ctx|
  let root = test.temp_dir(ctx, name: "loc-root")?

  fp"${root}/main.rs".write("""fn main() {
  println!("hello");
}
""")?

  fp"${root}/lib.rs".write("""// empty
""")?

  let output = run.text "xsh" "showcase/loc.xsh" -- $root ?
  "rs" in output
  "2 files" in output
}
