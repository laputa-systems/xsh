test killed_child { |ctx|
  let _ = run.status timeout -s KILL 1 (ctx.xsh_bin) p"killed.xsh"
}
test suffixless { |ctx|
  let result = run.text (ctx.xsh_bin) p"suffixless"
  assert result.trim() == "suffixless"
}
