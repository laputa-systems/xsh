for character in "café" {
  print $character
}

for octet in b"\0\xff" {
  print $octet
}

let widths = [character.byte_len() for character in "éx"]
print widths.len()
