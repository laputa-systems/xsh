let scratch = fs.tempdir()?
defer scratch.close()?
let dir = scratch.host_path()?

cd $dir {
  p"notes.txt".write("hi\n")?
  let listing = run.text ls ?
  print f"inside: {listing.trim()}"
}

env LC_ALL=C GREETING="hello world" {
  let said = run.text printenv GREETING ?
  print f"child saw: {said.trim()}"
}

let outside = e"GREETING" ?? "(unset)"
print f"after the block: {outside}"
