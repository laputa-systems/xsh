let folded = [1] |> fold(0) { |same, same| same } # error: check.duplicate-name
let reduced = [1] |> reduce(0) { |same, same| same } # error: check.duplicate-name
