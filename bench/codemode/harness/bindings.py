import json
import re
from dataclasses import dataclass

from config import SERVICE_PORT

KINDS = ("Timeout", "Unavailable", "RateLimited", "NotFound", "PermissionDenied", "Invalid", "Conflict", "Failed")
SCALARS = {"Str": "string", "Int": "number", "Float": "number", "Bool": "boolean"}
TOKEN = re.compile(r"\s*([A-Za-z_][A-Za-z0-9_]*|[\[\]{}:,?])")
DEFAULT_TIMEOUT_MS = 10_000

ERROR_DOC = (
    "A failed service call. The variant names the failure kind; Timeout, Unavailable and RateLimited "
    "are transient, the others are not."
)
BEHAVIOR_NOTES = [
    "Every operation is one call to a local fixture service. A failed call carries a kind: "
    "`Timeout`, `Unavailable`, `RateLimited`, `NotFound`, `PermissionDenied`, `Invalid`, `Conflict`, or `Failed`, "
    "and a message. `Timeout`, `Unavailable` and `RateLimited` are transient; the others are not.",
    f"A call that has not answered within its timeout (default {DEFAULT_TIMEOUT_MS // 1000} seconds) is abandoned "
    "and fails with `Timeout`.",
    "An operation marked `read` has no effect and may be repeated. An operation marked `write` changes the "
    "service's state; repeating a write that already took effect fails with `Conflict`.",
    "An input the operation does not accept fails with `Invalid`, and the message says so.",
]


@dataclass(frozen=True, slots=True)
class TypeExpr:
    kind: str
    name: str = ""
    inner: "TypeExpr | None" = None
    fields: tuple[tuple[str, "TypeExpr"], ...] = ()


def parse_type(text: str) -> TypeExpr:
    tokens = [m.group(1) for m in TOKEN.finditer(text)]
    if "".join(tokens) != re.sub(r"\s+", "", text):
        raise ValueError(f"cannot read type {text!r}")
    position = 0

    def take(expected: str | None = None) -> str:
        nonlocal position
        if position >= len(tokens):
            raise ValueError(f"type {text!r} ends early")
        token = tokens[position]
        if expected is not None and token != expected:
            raise ValueError(f"expected {expected!r} in {text!r}, found {token!r}")
        position += 1
        return token

    def peek() -> str | None:
        return tokens[position] if position < len(tokens) else None

    def one() -> TypeExpr:
        token = take()
        if token == "{":
            fields: list[tuple[str, TypeExpr]] = []
            while peek() != "}":
                name = take()
                take(":")
                fields.append((name, one()))
                if peek() == ",":
                    take(",")
            take("}")
            found = TypeExpr("record", fields=tuple(fields))
        elif token == "List":
            take("[")
            element = one()
            take("]")
            found = TypeExpr("list", inner=element)
        elif token in SCALARS:
            found = TypeExpr("scalar", name=token)
        else:
            found = TypeExpr("named", name=token)
        if peek() == "?":
            take("?")
            found = TypeExpr("optional", inner=found)
        return found

    result = one()
    if position != len(tokens):
        raise ValueError(f"unexpected {tokens[position]!r} in {text!r}")
    return result


def xsh_type(t: TypeExpr) -> str:
    match t.kind:
        case "scalar" | "named":
            return t.name
        case "list":
            return f"List[{xsh_type(t.inner)}]"
        case "optional":
            return f"{xsh_type(t.inner)}?"
        case "record":
            return "{" + ", ".join(f"{n}: {xsh_type(f)}" for n, f in t.fields) + "}"
    raise ValueError(t)


def ts_type(t: TypeExpr) -> str:
    match t.kind:
        case "scalar":
            return SCALARS[t.name]
        case "named":
            return t.name
        case "list":
            inner = ts_type(t.inner)
            return f"({inner})[]" if "|" in inner else f"{inner}[]"
        case "optional":
            return f"{ts_type(t.inner)} | null"
        case "record":
            return "{ " + "; ".join(f"{n}: {ts_type(f)}" for n, f in t.fields) + " }"
    raise ValueError(t)


def schema(t: TypeExpr) -> object:
    match t.kind:
        case "scalar":
            return t.name
        case "named":
            return {"ref": t.name}
        case "list":
            return {"list": schema(t.inner)}
        case "optional":
            return {"optional": schema(t.inner)}
        case "record":
            return {"record": [[n, schema(f)] for n, f in t.fields]}
    raise ValueError(t)


def xsh_name(capability: str) -> str:
    return capability.replace(".", "_")


def ts_name(capability: str) -> str:
    parts = re.split(r"[._]", capability)
    return parts[0] + "".join(p[:1].upper() + p[1:] for p in parts[1:])


@dataclass(frozen=True, slots=True)
class Capability:
    id: str
    effect: str
    idempotent: bool
    input: tuple[tuple[str, TypeExpr], ...]
    output: TypeExpr
    description: str


def capabilities(task: dict) -> list[Capability]:
    found = []
    for c in task.get("capabilities", []):
        fields = tuple((name, parse_type(text)) for name, text in c["input"].items())
        found.append(Capability(
            c["id"], c["effect"], bool(c.get("idempotent", c["effect"] == "read")), fields,
            parse_type(c["output"]), c["description"],
        ))
    return found


def types(task: dict) -> list[tuple[str, TypeExpr, str]]:
    return [(name, parse_type(d["type"]), d["doc"]) for name, d in task.get("types", {}).items()]


def effect_note(c: Capability) -> str:
    if c.effect == "read":
        return "read"
    return "write, idempotent" if c.idempotent else "write, not idempotent"


def xsh_signature(c: Capability) -> str:
    params = ", ".join([f"{n}: {xsh_type(t)}" for n, t in c.input] + ["timeout: Duration = 10s"])
    return f"export proc {xsh_name(c.id)}({params}) -> Result[{xsh_type(c.output)}, Error]"


def xsh_declarations(task: dict) -> str:
    lines = [f"## {ERROR_DOC}", "export error ServiceError = " + " | ".join(KINDS), ""]
    for name, t, doc in types(task):
        lines += [f"## {doc}", f"export type {name} = {xsh_type(t)}", ""]
    for c in capabilities(task):
        lines += [f"## {c.description} Effect: {effect_note(c)}.", xsh_signature(c), ""]
    return "\n".join(lines).rstrip() + "\n"


def xsh_input_literal(c: Capability) -> str:
    if not c.input:
        return "json.decode(\"{}\")?"
    return "{" + ", ".join(n for n, _ in c.input) + "}"


def xsh_module(task: dict) -> str:
    kinds = "\n".join(f'    "{k}" => ServiceError.{k}(message)' for k in KINDS[:-1])
    lines = [
        "##! Bindings for this task's fixture service.",
        "",
        f"## {ERROR_DOC}",
        "export error ServiceError = " + " | ".join(KINDS),
        "",
    ]
    for name, t, doc in types(task):
        lines += [f"## {doc}", f"export type {name} = {xsh_type(t)}", ""]
    lines += [
        "type Reply = {kind: Str, message: Str}",
        "",
        "pure failure(kind: Str, message: Str) -> ServiceError {",
        "  match kind {",
        kinds,
        "    else => ServiceError.Failed(message)",
        "  }",
        "}",
        "",
        "proc call(operation: Str, input: Any, timeout: Duration) -> Result[Any] {",
        "  let request = {",
        '    method: "POST",',
        f'    url: f"http://127.0.0.1:{SERVICE_PORT}/call/{{operation}}",',
        '    headers: [{name: "content-type", value: "application/json"}],',
        "    body: bytes.from_text(json.encode(input)?),",
        "    timeout: timeout,",
        "    fail_status: false,",
        "  }",
        "  let response = match net.request(request) {",
        "    Ok(answer) => answer",
        "    Err(is Timeout) => {",
        '      return Err(ServiceError.Timeout(f"{operation} did not answer within {timeout}"))',
        "    }",
        "    Err(error) => {",
        '      return Err(ServiceError.Unavailable(f"{operation}: {error.message}"))',
        "    }",
        "  }",
        "  let decoded = json.decode(response.body.utf8()?)?",
        "  if response.status == 200 {",
        '    return json.get(decoded, ["output"])',
        "  }",
        "  let reply = decoded.require(Reply)?",
        "  Err(failure(reply.kind, reply.message))",
        "}",
        "",
    ]
    for c in capabilities(task):
        lines += [
            f"## {c.description} Effect: {effect_note(c)}.",
            xsh_signature(c) + " {",
            f'  call("{c.id}", {xsh_input_literal(c)}, timeout)?.require({xsh_type(c.output)})',
            "}",
            "",
        ]
    return "\n".join(lines).rstrip() + "\n"


def ts_input(c: Capability) -> str:
    if not c.input:
        return "input: Record<string, never> = {}"
    return "input: { " + "; ".join(f"{n}: {ts_type(t)}" for n, t in c.input) + " }"


def ts_signature(c: Capability) -> str:
    return f"export function {ts_name(c.id)}({ts_input(c)}, options?: CallOptions): Promise<{ts_type(c.output)}>"


def ts_declarations(task: dict) -> str:
    kinds = " | ".join(f'"{k}"' for k in KINDS)
    lines = [
        f"export type ServiceErrorKind = {kinds};",
        f"/** {ERROR_DOC} */",
        "export declare class ServiceError extends Error {",
        "  readonly kind: ServiceErrorKind;",
        "}",
        f"/** `timeoutMs` defaults to {DEFAULT_TIMEOUT_MS}. */",
        "export type CallOptions = { timeoutMs?: number };",
        "",
    ]
    for name, t, doc in types(task):
        lines += [f"/** {doc} */", f"export type {name} = {ts_type(t)};", ""]
    for c in capabilities(task):
        lines += [f"/** {c.description} Effect: {effect_note(c)}. */", ts_signature(c).replace("export function", "export declare function").replace(" = {}", "") + ";", ""]
    return "\n".join(lines).rstrip() + "\n"


def ts_module(task: dict) -> str:
    kinds = " | ".join(f'"{k}"' for k in KINDS)
    schemas = {name: schema(t) for name, t, _ in types(task)}
    lines = [
        f"export type ServiceErrorKind = {kinds};",
        f"const KINDS: readonly string[] = {json.dumps(list(KINDS))};",
        "",
        f"/** {ERROR_DOC} */",
        "export class ServiceError extends Error {",
        "  readonly kind: ServiceErrorKind;",
        "  constructor(kind: ServiceErrorKind, message: string) {",
        "    super(message);",
        '    this.name = "ServiceError";',
        "    this.kind = kind;",
        "  }",
        "}",
        "",
        f"/** `timeoutMs` defaults to {DEFAULT_TIMEOUT_MS}. */",
        "export type CallOptions = { timeoutMs?: number };",
        "",
    ]
    for name, t, doc in types(task):
        lines += [f"/** {doc} */", f"export type {name} = {ts_type(t)};", ""]
    lines += [
        "type Schema = string | { ref: string } | { list: Schema } | { optional: Schema } | { record: [string, Schema][] };",
        f"const SCHEMAS: Record<string, Schema> = {json.dumps(schemas)};",
        "",
        "function conform(value: unknown, shape: Schema, at: string): void {",
        '  if (typeof shape === "string") {',
        '    const ok = shape === "Str" ? typeof value === "string" : shape === "Bool" ? typeof value === "boolean"',
        '      : shape === "Int" ? Number.isInteger(value) : typeof value === "number";',
        '    if (!ok) throw new ServiceError("Failed", `schema check failed at ${at || "result"}: expected ${shape}`);',
        "    return;",
        "  }",
        '  if ("ref" in shape) return conform(value, SCHEMAS[shape.ref], at);',
        '  if ("optional" in shape) {',
        "    if (value === null || value === undefined) return;",
        "    return conform(value, shape.optional, at);",
        "  }",
        '  if ("list" in shape) {',
        '    if (!Array.isArray(value)) throw new ServiceError("Failed", `schema check failed at ${at || "result"}: expected List`);',
        "    value.forEach((item, index) => conform(item, shape.list, `${at}[${index}]`));",
        "    return;",
        "  }",
        '  if (typeof value !== "object" || value === null || Array.isArray(value)) {',
        '    throw new ServiceError("Failed", `schema check failed at ${at || "result"}: expected Record`);',
        "  }",
        "  for (const [field, inner] of shape.record) {",
        "    conform((value as Record<string, unknown>)[field], inner, at ? `${at}.${field}` : field);",
        "  }",
        "}",
        "",
        "async function call(operation: string, input: unknown, options: CallOptions | undefined, shape: Schema): Promise<unknown> {",
        f"  const timeoutMs = options?.timeoutMs ?? {DEFAULT_TIMEOUT_MS};",
        "  let response: Response;",
        "  let text: string;",
        "  try {",
        f"    response = await fetch(`http://127.0.0.1:{SERVICE_PORT}/call/${{operation}}`, {{",
        '      method: "POST",',
        '      headers: { "content-type": "application/json" },',
        "      body: JSON.stringify(input),",
        "      signal: AbortSignal.timeout(timeoutMs),",
        "    });",
        "    text = await response.text();",
        "  } catch (error) {",
        '    if (error instanceof Error && (error.name === "TimeoutError" || error.name === "AbortError")) {',
        '      throw new ServiceError("Timeout", `${operation} did not answer within ${timeoutMs} ms`);',
        "    }",
        '    throw new ServiceError("Unavailable", `${operation}: ${String(error)}`);',
        "  }",
        "  const decoded = JSON.parse(text) as { output?: unknown; kind?: string; message?: string };",
        "  if (response.status === 200) {",
        "    conform(decoded.output, shape, \"\");",
        "    return decoded.output;",
        "  }",
        '  const kind = (KINDS.includes(String(decoded.kind)) ? decoded.kind : "Failed") as ServiceErrorKind;',
        "  throw new ServiceError(kind, String(decoded.message));",
        "}",
        "",
    ]
    for c in capabilities(task):
        lines += [
            f"/** {c.description} Effect: {effect_note(c)}. */",
            f"export async function {ts_name(c.id)}({ts_input(c)}, options?: CallOptions): Promise<{ts_type(c.output)}> {{",
            f'  return (await call("{c.id}", input, options, {json.dumps(schema(c.output))})) as {ts_type(c.output)};',
            "}",
            "",
        ]
    return "\n".join(lines).rstrip() + "\n"


def service_spec(task: dict, responses: list[dict]) -> dict:
    return {
        "capabilities": [{"id": c["id"], "effect": c["effect"]} for c in task.get("capabilities", [])],
        "responses": responses,
    }
