pure ambiguous(value, flag: Bool) { if flag { value } else { Ok(value) } }
let _ = ambiguous(1, true)
