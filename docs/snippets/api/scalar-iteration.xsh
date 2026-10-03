for character in "caf\u{e9}" {
  print $character
}

for octet in b"\0\xff" {
  print $octet
}

let widths = [character.byte_len() for character in "\u{e9}x"]
print widths.len()
