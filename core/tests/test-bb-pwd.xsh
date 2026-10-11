use support.uu as uu

# origin: busybox pwd/pwd-prints-working-directory
test test_bb_pwd_pwd_prints_working_directory_8a6b3c21 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pwd", [])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root.resolve()?}\n")
}
