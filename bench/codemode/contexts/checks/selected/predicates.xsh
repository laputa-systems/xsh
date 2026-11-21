let present = p"/tmp".exists()?
let missing = p"/no/such/dir".is_dir() ?? false
if present and ! missing {
  print "as stated"
}
