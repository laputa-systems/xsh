use svc

var cursor: Str? = null
var errors: List[Str] = []
var pages = 0

for _ in range(20) {
  let page = svc.audit_page(cursor)?
  pages += 1
  errors += [e.id for e in page.events if e.kind == "error"]
  if page.next == null {
    break
  }

  cursor = page.next
}

print (json.encode({errors: errors, pages: pages})?)
