use svc

var cursor: Str? = null
var errors: List[Str] = []
var pages = 0

for _ in range(20) {
  let page = svc.audit_page(cursor)?
  pages += 1
  for event in page.events {
    if event.kind == "error" and event.id not in errors {
      errors += [event.id]
    }
  }

  if page.next == null {
    break
  }

  cursor = page.next
}

print (json.encode({errors: errors |> sort(), pages: pages})?)
