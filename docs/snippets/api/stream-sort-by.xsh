let entries = [{ size: 1 }, { size: 3 }]
let largest = entries
  |> sort-by --desc { |e| e.size }
