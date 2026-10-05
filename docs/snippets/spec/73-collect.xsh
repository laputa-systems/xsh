type Service = {name: Str, wants: List[Str], enabled: Bool}

# begin example
pure start_order(units: List[Service]) -> List[Str] {
  collect {
    for unit in units {
      continue unless unit.enabled
      yield @unit.wants
      yield unit.name
    }
  }
}

# end example

let order = start_order(
  [
    {name: "net", wants: [], enabled: true},
    {name: "ssh", wants: ["net", "keys"], enabled: true},
    {name: "cron", wants: [], enabled: false},
  ],
)
print order.join(" ")
