const name = "demo"
let unit = f"""
  [service]
  name=$name
  """
const expected = "[service]\nname=demo"
print ${unit == expected}
