type Setting = {key: Str, value: Str}

# begin example
pure setting(line: Str) -> Setting? {
  if let f"{key}={value}" = line {
    return {key, value}
  }

  null
}

pure describe(line: Str) -> Str {
  match line {
    f"#define {name} {body}" => f"{name} is {body}"
    f"#include <{header}>" => f"system header {header}"
    f"{host}:{port:d}" => f"{host} on port {port}"
    else => "other"
  }
}

# end example

let found = setting("PATH=/bin:/usr/bin") ?? {key: "", value: ""}
print f"{found.key} {found.value}"
print f"{describe("#define MAX 4 + 4")}"
print f"{describe("#include <stdio.h>")}"
print f"{describe("localhost:8080")}"
print f"{describe("localhost:http")}"
