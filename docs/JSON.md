# JSON Boundaries

JSON is a boundary format in XSH, not the internal language of a script. Decode
it at the edge, check the shape you intend to trust, and keep the rest of the
program in typed XSH values.

`examples/json.xsh` is the curated persistence and JSON-lines composition
showcase. `tests/xsh/stdlib/json.xsh` owns focused acceptance and error cases.

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

## System Report Snapshots

`core/lib/system_report.xsh::SystemReport` keeps observation, section, and
source states as closed tag unions. JSON v1 uses the `SystemReportJson` wire
schema and stable lower snake case strings for those union values. Use
`encode_report_json` and `decode_report_json` at this boundary; the decoder
rejects unknown state spellings and schema versions. The encoder applies the
same default redaction to JSON that the report renderer uses unless the caller
explicitly selects sensitive output.

Live collection currently reports `live_linux` and describes the
process-visible source view and namespaces in `ObservationScope`; it does not
claim that the process sees a physical host or the host's outer namespaces.
`container_live` and `physical_live` remain distinct source modes for captures
whose provenance establishes those boundaries.

`render_text` applies that redaction by default and escapes terminal controls,
bidi controls, and line separators in untrusted text. The default redaction
also removes source-root paths and sensitive observation payloads while
preserving numeric relationship indexes. It removes PCI bus addresses, USB
port and device addresses, block-device names and numbers, and their copied
model or firmware labels. Numeric PCI/USB vendor, product, and class IDs and
indexed parent, holder, slave, and mount relationships remain available.
Issue details and dynamic device identifiers in field paths receive the same
redaction. Redaction reduces exposure but does not guarantee anonymity.
`ObservationScope.page_size_bytes` and
`clock_ticks_per_second` record the runtime units used when interpreting
process page and clock-tick counters. Cgroup resource values carry a unit and
keep maximum, current, quota, and period observations separate; an unlimited
maximum is represented explicitly. Mount inventory is independent of usage:
`usage_state` is `not_requested` when filesystem capacity was skipped by the
fixture policy or local-filesystem eligibility policy, and exact byte counters
are present only when a rooted `statvfs` query succeeded. `parse_cpu_list`
expands sparse Linux ranges, sorts the identifiers, rejects duplicates and
malformed ranges, and limits the expanded list to 65,536 identifiers.
`ProcessRecord.cgroup` contains the parsed unified cgroup path rather than the
raw `/proc` row; `cgroup_resource_index` resolves an exact path match in the
collected cgroup resource list and remains null when no match was collected or
when section projection removes the memory section.
`BlockDevice.parent_device_index` describes block stacking, while
`parent_pci_function_index` points to an enumerated PCI controller when the
sysfs device path resolves to one. Default redaction preserves both indexed
relationships.
`select_report_section` retains identity,
the selected section, and PCI/USB sections required to preserve the selected
USB, network, or device-class relationships. It clears other sections and
their issues and marks them `not_requested` with unsuccessful enumeration
states.
`core/lib/system_report_collect.xsh::parse_pci_address` preserves the PCI
domain, bus, device, and function components during collection. Default
redaction clears the address and components; sensitive output retains them.
The route-netlink report keeps unknown numeric enum values in explicit
`*_N` strings and stores each attribute payload as base64 in
`NetworkAttribute.data`. Routes preserve multipath entries as typed
`NetworkNexthop` records with interface index, kernel flags, raw hop weight, and
an optional gateway observation; a single output interface stays in
`output_ifindex` and does not create a synthetic nexthop. Default redaction preserves attribute kinds and
relationship indices while marking payload observations `redacted`; sensitive
JSON retains their base64 bytes. Network section states distinguish denied,
unsupported, malformed, raced, and truncated dumps from empty successful
enumerations.
Its USB descriptor parser bounds input to 1 MiB, validates every descriptor
length, and preserves unknown descriptor bytes for later typed interpretation.

`system-report --from FILE` uses the same decoder and renderer for saved
snapshots. It reads at most 16 MiB. Replay does not query the current host;
`--section` projects the decoded report before rendering, and `--json` emits
one v1 JSON document. Default text includes source scope and section status,
groups CPUFreq policies only when their observed settings match, and reports
how many mount capacity queries ran or were skipped. `--full` adds per-item
detail without changing which observations were collected.
