let name = "demo"
let unit = f"""
  [service]
  name=$name
  """
let expected = "[service]\nname=demo"
print ${unit == expected}
