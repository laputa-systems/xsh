let text = """{"service": {"name": "web"}, "count": 2}"""
# begin example
let doc = json.decode(text)?
let service = doc.service.name              # Any
let n: Int = doc.count                      # error: check.dynamic-boundary
let next = doc.count + 1                    # error: check.dynamic-boundary
print f"{doc.service.name}"                 # error: check.dynamic-boundary
let count = doc.count.require(Int)?         # Int
let label = doc.label.require(Str?)? ?? "none"
# end example
