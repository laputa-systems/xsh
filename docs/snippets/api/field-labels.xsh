type Entry = {type: Str, in: Int}

let entry = Entry("file", 2)
let {type: entry_kind, in: ordinal, ..} = entry
let label = entry.type
let dotted_key = {"wire.type": entry_kind}
