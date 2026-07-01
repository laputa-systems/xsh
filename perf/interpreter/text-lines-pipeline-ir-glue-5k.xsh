pure normalized_count_lines(input_text: Str) -> List[Str] {
  input_text
    |> text.lines
    |> where .trim() != ""
    |> map { |line|
      let fields = line.fields()
      f"${fields[0]} ${fields[1]}"
    }
}

let sample = """pkg 10 alpha

pkg 20 beta
tool 30 gamma
"""

var i = 0
var total = 0

while i < 5000 {
  total += normalized_count_lines(sample).join(",").count_chars()
  i += 1
}

print $total % 256
