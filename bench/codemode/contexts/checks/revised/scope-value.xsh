tempdir dir {
  fp"{dir}/a".write("1")
  let listed = cd (dir) {
    run.capture --text ls
  }?
  let seen = env ({GREETING: "hi"}) {
    run.text printenv GREETING
  }?
  print f"{listed.stdout.trim()} {seen.trim()}"
}
