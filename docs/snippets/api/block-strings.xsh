const name = "demo"
let unit = f"""
  [service]
  name=$name
  """
const expected = """[service]
name=demo"""
print ${unit == expected}
