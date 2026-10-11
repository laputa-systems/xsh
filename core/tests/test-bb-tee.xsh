use support.uu

# origin: busybox tee/tee-appends-input
test test_bb_tee_tee_appends_input_94f81e98 { |ctx|
  let s = uu.scene(ctx)?
  let line = "i'm a little teapot\n"
  uu.write(s, "bar", line)?
  let r = uu.invoke(s, "tee", ["-a", "bar"], stdin: bytes.from_text(line))?
  uu.succeeds(r)
  uu.stdout_only(r, line)
  uu.file_is(s, "bar", line + line)
}

# origin: busybox tee/tee-tees-input
test test_bb_tee_tee_tees_input_9e6cf500 { |ctx|
  let s = uu.scene(ctx)?
  let line = "i'm a little teapot\n"
  let r = uu.invoke(s, "tee", ["bar"], stdin: bytes.from_text(line))?
  uu.succeeds(r)
  uu.stdout_only(r, line)
  uu.file_is(s, "bar", line)
}

