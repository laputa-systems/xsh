use support.uu

# origin: busybox sha512sum/sha512sum
test test_bb_sha512sum_sha512sum_5f794a17 { |ctx|
  let s = uu.scene(ctx)?
  let block = bytes.from_text(["The quick brown fox jumps over the lazy dog\n" for _ in range(24)].join(""))
  var checksums: List[Bytes] = []
  for length in range(1000) {
    let r = uu.invoke(s, "sha512sum", [], stdin: block.slice(0, length))?
    uu.succeeds(r)
    checksums += [r.stdout]
  }
  let total = uu.invoke(s, "sha512sum", [], stdin: bytes.concat(checksums))?
  uu.succeeds(total)
  uu.stdout_is(total, "fe413e0f177324d1353893ca0772ceba83fd41512ba63895a0eebb703ef9feac2fb4e92b2cb430b3bda41b46b0cb4ea8307190a5cc795157cfb680a9cd635d0f  -\n")
}

# origin: busybox sha512sum/sha512sum -c EMPTY
test test_bb_sha512sum_sha512sum_c_EMPTY_820a2c1d { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "EMPTY")?
  uu.fails_with_code(uu.invoke(s, "sha512sum", ["-c", "EMPTY"])?, 1)
}
