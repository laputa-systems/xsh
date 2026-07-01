let sample = """alpha,beta,,gamma
last line"""

let fields = " alpha  beta\tgamma ".fields()
let csv = "alpha,beta,,gamma".split(",") |> where . != ""
let split = "ab".split("")
let joined = csv.join("|")
let rewritten = sample.replace("beta", "B")
let reversed = "stressed".reverse()
let slug = "alpha beta_gamma".translate(" _", "--")
let cleaned = "a-b_c".delete("-_")
let squeezed = "nooo   way".squeeze(chars: " o")
let wrapped = "alpha beta gamma".wrap(10)
let utf8_text = "h\u{e9}!"
print fields[1] csv[2] split[1] $joined
print rewritten.count_lines() sample.count_words() "h\u{e9}".count_chars() "h\u{e9}".count_bytes() $reversed
print $slug $cleaned $squeezed wrapped[1]
print utf8_text.byte_len() utf8_text.byte_at(1) utf8_text.byte_slice(1, 2) utf8_text.find("!")
