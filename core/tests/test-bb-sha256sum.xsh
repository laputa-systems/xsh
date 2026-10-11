use support.uu

# origin: busybox sha256sum/sha256sum
test test_bb_sha256sum_sha256sum_01719a4c { |ctx|
  let s = uu.scene(ctx)?
  let block = bytes.from_text(["The quick brown fox jumps over the lazy dog\n" for _ in range(24)].join(""))
  var checksums: List[Bytes] = []
  for length in range(1000) {
    let r = uu.invoke(s, "sha256sum", [], stdin: block.slice(0, length))?
    uu.succeeds(r)
    checksums += [r.stdout]
  }
  let total = uu.invoke(s, "sha256sum", [], stdin: bytes.concat(checksums))?
  uu.succeeds(total)
  uu.stdout_is(total, "8e1d3ed57ebc130f0f72508446559eeae06451ae6d61b1e8ce46370cfb8963c3  -\n")
}

# origin: busybox sha256sum/sha256sum -c EMPTY
test test_bb_sha256sum_sha256sum_c_EMPTY_4cb981a2 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "EMPTY")?
  uu.fails_with_code(uu.invoke(s, "sha256sum", ["-c", "EMPTY"])?, 1)
}
