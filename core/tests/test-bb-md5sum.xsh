use support.uu

# origin: busybox md5sum/md5sum
test test_bb_md5sum_md5sum_197bd121 { |ctx|
  let s = uu.scene(ctx)?
  let block = bytes.from_text(["The quick brown fox jumps over the lazy dog\n" for _ in range(24)].join(""))
  var checksums: List[Bytes] = []
  for length in range(1000) {
    let r = uu.invoke(s, "md5sum", [], stdin: block.slice(0, length))?
    uu.succeeds(r)
    checksums += [r.stdout]
  }
  let total = uu.invoke(s, "md5sum", [], stdin: bytes.concat(checksums))?
  uu.succeeds(total)
  uu.stdout_is(total, "efe30c482e0b687e0cca0612f42ca29b  -\n")
}

# origin: busybox md5sum/md5sum -c EMPTY
test test_bb_md5sum_md5sum_c_EMPTY_9c7796bd { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "EMPTY")?
  uu.fails_with_code(uu.invoke(s, "md5sum", ["-c", "EMPTY"])?, 1)
}

# origin: busybox md5sum/md5sum-verifies-non-binary-file
test test_bb_md5sum_md5sum_verifies_non_binary_file_394a9a7e { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  let listing = uu.invoke(s, "md5sum", ["foo"])?
  uu.succeeds(listing)
  uu.write_bytes(s, "bar", listing.stdout)?
  uu.succeeds(uu.invoke(s, "md5sum", ["-c", "bar"])?)
}
