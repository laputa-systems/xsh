use support.uu

# origin: gnu uniq/uniq-collate.log
test test_gnu_uniq_uniq_collate_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [
    {locale: "fr_FR.UTF-8", input: "ⁿᵘˡˡ\nܥܝܪܐܩ\n"},
    {locale: "fr_FR.UTF-8", input: "á\ná\n"},
    {locale: "ko_KR.utf8", input: "각\n각\n"},
    {locale: "en_US.utf8", input: "가\n각\n"},
    {locale: "en_US.utf8", input: "㐀\n㐁\n"},
    {locale: "fr_FR.UTF-8", input: ",a\n.a\n"},
  ] {
    uu.write(s, "in", row.input)?
    let r = uu.invoke_from_path(s, "uniq", [], uu.at(s, "in"), vars: {LC_ALL: row.locale}, timeout: 10s)?
    assert r.stdout.utf8()?.count_lines() == 2
  }
}

# origin: gnu uniq/uniq-perf.log
test test_gnu_uniq_uniq_perf_log { |ctx|
  let s = uu.scene(ctx)?
  let generated = uu.invoke(s, "seq", ["100"])?
  uu.succeeds(generated)
  uu.write_bytes(s, "in", generated.stdout)?
  uu.succeeds(uu.invoke(s, "uniq", ["-f", "10000000000", "in"], timeout: 10s)?)
}
