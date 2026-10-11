use support.uu as uu

# origin: busybox patch/patch with old_file == new_file
test test_bb_patch_patch_with_old_file_new_file_f9d626ef { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "qwe\nzxc\n")?
  let r = uu.invoke(s, "patch", [], stdin: bytes.from_text("--- input\tJan 01 01:01:01 2000\n+++ input\tJan 01 01:01:01 2000\n@@ -1,2 +1,3 @@\n qwe\n+asd\n zxc\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "patching file input\n")
  uu.file_is(s, "input", "qwe\nasd\nzxc\n")
}

# origin: busybox patch/patch with nonexistent old_file
test test_bb_patch_patch_with_nonexistent_old_file_be97bfc1 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "qwe\nzxc\n")?
  let r = uu.invoke(s, "patch", [], stdin: bytes.from_text("--- input.doesnt_exist\tJan 01 01:01:01 2000\n+++ input\tJan 01 01:01:01 2000\n@@ -1,2 +1,3 @@\n qwe\n+asd\n zxc\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "patching file input\n")
  uu.file_is(s, "input", "qwe\nasd\nzxc\n")
}

# origin: busybox patch/patch -R with nonexistent old_file
test test_bb_patch_patch_R_with_nonexistent_old_file_3a0b4a54 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "qwe\nasd\nzxc\n")?
  let r = uu.invoke(s, "patch", ["-R"], stdin: bytes.from_text("--- input.doesnt_exist\tJan 01 01:01:01 2000\n+++ input\tJan 01 01:01:01 2000\n@@ -1,2 +1,3 @@\n qwe\n+asd\n zxc\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "patching file input\n")
  uu.file_is(s, "input", "qwe\nzxc\n")
}

# origin: busybox patch/patch FILE PATCH
test test_bb_patch_patch_FILE_PATCH_0bd88dd2 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "abc\n123\n")?
  uu.write(s, "a.patch", "--- foo.old\n+++ foo\n@@ -1,2 +1,3 @@\n abc\n+def\n 123\n")?
  let r = uu.invoke(s, "patch", ["input", "a.patch"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "patching file input\n")
  uu.file_is(s, "input", "abc\ndef\n123\n")
}

# origin: busybox patch/patch at the beginning
test test_bb_patch_patch_at_the_beginning_562924b8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "111\n222\n333\n444\n555\n666\n777\n888\n999\n")?
  let r = uu.invoke(s, "patch", [], stdin: bytes.from_text("--- input\n+++ input\n@@ -1,6 +1,4 @@\n-111\n-222\n-333\n+111changed\n 444\n 555\n 666\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "patching file input\n")
  uu.file_is(s, "input", "111changed\n444\n555\n666\n777\n888\n999\n")
}

# origin: busybox patch/patch internal buffering bug?
test test_bb_patch_patch_internal_buffering_bug_4c13bf86 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "foo\n\n\n\n\n\n\n\nbar\n")?
  let r = uu.invoke(s, "patch", ["-p1"], stdin: bytes.from_text("--- a/input.orig\n+++ b/input\n@@ -5,5 +5,8 @@ foo\n \n \n \n+1\n+2\n+3\n \n bar\n-- \n2.9.2\n"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "patching file input\n")
  uu.file_is(s, "input", "foo\n\n\n\n\n\n\n1\n2\n3\n\nbar\n")
}

