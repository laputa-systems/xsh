let count = ctx "reading configured count" {
  "7".parse_int()?
}
print $count
