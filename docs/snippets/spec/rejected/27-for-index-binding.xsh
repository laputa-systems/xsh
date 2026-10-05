type Entry = {name: Str, size: Int}

const entries: List[Entry] = [{name: "a", size: 1}]
# begin example
for {name, size}, entry in entries {  # error: parse.for-index
  print $name $size
}
# end example
