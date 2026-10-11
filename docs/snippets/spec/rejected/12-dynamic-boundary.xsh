let text = """{"service": {"name": "web"}, "count": 2}"""
# begin example
let doc = json.decode(text)?
let service = json.get(doc, ["service", "name"])?          # Any
let n: Int = json.get(doc, ["count"])?                    # error: check.dynamic-boundary
let next = (json.get(doc, ["count"])?) + 1                # error: check.dynamic-boundary
print f"{service}"                                      # error: check.dynamic-boundary
let count = (json.get(doc, ["count"])?).require(Int)?      # Int
let label = json.get(doc, ["label"], null).require(Str?)? ?? "none"
# end example
