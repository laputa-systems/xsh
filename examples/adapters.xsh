let files_text = run.text printf "%s\n" "src/main.xsh" "src/lib.xsh" ?

let paths = files_text
  |> text.lines
  |> map { |line|
    fp"${line}"
  }

print paths[0].ext paths[1].name
let chunks = b"print ok\n" |> bytes.chunks(2)
print (chunks[0] == b"pr") (chunks[3] == b"ok")

let records = """{"name":"small","size":1}
{"name":"large","size":4}
"""
  |> json.lines
  |> sort-by .size

print records[1].name records[0].size
