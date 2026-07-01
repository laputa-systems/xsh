# JSON Boundaries

JSON is a boundary format in XSH, not the internal language of a script. Decode
it at the edge, check the shape you intend to trust, and keep the rest of the
program in typed XSH values.

## The Default Pattern

Use a named schema when later code depends on fields having a stable shape.

```xsh
type Package = {name: Str, version: Str, files: List[Str]}

let raw = json.read(manifest_path)?
let package = raw.require(Package)?

for file in package.files {
  print f"${package.name}-${package.version}: ${file}"
}
```

`json.read` and `json.decode` return `Any` because valid JSON only proves that
the text parsed. `.require(Package)?` is the trust boundary: it checks the
runtime value and gives the checker a concrete type for the rest of the script.

Prefer this whenever the script knows what it needs. It produces better errors,
keeps field access ordinary, and avoids scattering dynamic checks through the
program.

## Do Not Schema Every Temporary Value

Do not invent a named type for every throwaway JSON fragment. Add a schema where
data crosses a boundary or where later code needs stable fields.

```xsh
let event = {
  service: "worker",
  event: "done",
  ok: status.ok,
}

json.write(log_path, event)?
```

The record is already typed in XSH. A separate `Event` type is useful only if
the script will read the value back, accept it from another process, or pass it
through an API that depends on that shape.

## Dynamic JSON Tools

Some programs are about unknown JSON itself: formatters, filters, validators,
diff tools, recursive walkers, and compatibility adapters. Those programs need
to branch on runtime shape because there is no single schema to require.

Use type-pattern matching on `Any` for that case:

```xsh
pure scalar_label(v: Any) -> Result[Str] {
  match v {
    n is Null => return Ok("null")
    b is Bool => return Ok(if b { "true" } else { "false" })
    i is Int => return Ok(f"integer ${i}")
    f is Float => return Ok(f"float ${f}")
    s is Str => return Ok(f"string ${s.count_chars()}")
    _ => return Err(Error(kind: "json-type", message: "expected scalar JSON"))
  }
}
```

This is different from a `type_name()` string helper. A type pattern both tests
the runtime value and narrows the binding inside the arm. That keeps the dynamic
case explicit without turning ordinary typed code into string comparisons.

Prefer `.require(Type)?` for known shapes and keep generic dynamic code
localized behind helper functions.

## Dynamic Fields

For known object shapes, require a schema before accessing fields.

```xsh
type User = {id: Int, name: Str}

let user = json.decode(input)?.require(User)?
print f"${user.id}: ${user.name}"
```

For genuinely dynamic object access, keep the dynamic operation visible.

```xsh
let value = json.decode(input)?
let name = json.get(value, ["name"], null)
```

That says the field name is data. If the field is part of the contract, use a
schema instead.

## Practical Rule

Use `.require(Type)?` when the program knows the shape it needs. Use dynamic
matching only when the program is intentionally operating on unknown JSON
shapes. Treat `Any` as a short-lived boundary value, not as the normal way to
model application data.
