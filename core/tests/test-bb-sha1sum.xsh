use support.uu

# origin: busybox sha1sum/sha1sum
test test_bb_sha1sum_sha1sum_96663b5f { |ctx|
  let s = uu.scene(ctx)?
  let block = bytes.from_text(["The quick brown fox jumps over the lazy dog\n" for _ in range(24)].join(""))
  var checksums: List[Bytes] = []
  for length in range(1000) {
    let r = uu.invoke(s, "sha1sum", [], stdin: block.slice(0, length))?
    uu.succeeds(r)
    checksums += [r.stdout]
  }
  let total = uu.invoke(s, "sha1sum", [], stdin: bytes.concat(checksums))?
  uu.succeeds(total)
  uu.stdout_is(total, "d41337e834377140ae7f98460d71d908598ef04f  -\n")
}

# origin: busybox sha1sum/sha1sum -c EMPTY
test test_bb_sha1sum_sha1sum_c_EMPTY_31f096b5 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "EMPTY")?
  uu.fails_with_code(uu.invoke(s, "sha1sum", ["-c", "EMPTY"])?, 1)
}
