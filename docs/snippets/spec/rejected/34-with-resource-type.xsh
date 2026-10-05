# begin example
with text = p"notes.txt".read_text()? { # error: check.with-resource
  print $text
}
# end example
