test test_ecount_counts_extensions { |ctx|
  let root = test.temp_dir(ctx, name: "ecount-root")?
  fp"${root}/a.txt".write("alpha")?
  fp"${root}/b.txt".write("beta")?

  fp"${root}/script.xsh".write("""print "hi"
""")?

  fp"${root}/README".write("ignored")?
  fp"${root}/.hidden.txt".write("ignored")?
  let output = run.text "xsh" "showcase/ecount.xsh" -- $root ?
  "   1 (none)" in output
  "   1 xsh" in output
  "   2 txt" in output
  output.split("README").len() == 1
}

test test_ecount_can_sum_sizes { |ctx|
  let root = test.temp_dir(ctx, name: "ecount-size-root")?
  fp"${root}/a.bin".write("abcd")?
  fp"${root}/b.log".write("x")?
  fp"${root}/c.log".write("yz")?
  fp"${root}/README".write("zz")?
  let output = run.text "xsh" "showcase/ecount.xsh" -- "--size" $root ?
  "   1            2 (none)" in output
  "   2            3 log" in output
  "   1            4 bin" in output
}
