use crate::modules::json::{encode_json, parse_json};
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::sema::check::Checker;
use crate::source::{SourceId, Span};

use crate::syntax::parser::Parser;
use crate::{
    runtime::eval::{EvalOutput, Evaluator},
    source::SourceMap,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn btree_map<K: Ord, V>(entries: Vec<(K, V)>) -> BTreeMap<K, V> {
    let mut map = BTreeMap::new();
    map.extend(entries);
    map
}

pub struct PreparedProgram {
    arena: crate::syntax::arena::ArenaProgram,
    source_id: SourceId,
    sources: SourceMap,
}

#[derive(Clone, Debug, Default)]
pub struct FrontendLowerTimings {
    pub parse: Duration,
    pub evaluator_init: Duration,
    pub evaluator_current_dir: Duration,
    pub evaluator_struct_init: Duration,
    pub evaluator_args_bindings: Duration,
    pub compact_declarations: Duration,
    pub compact_runtime_declarations: Duration,
    pub compact_bodies: Duration,
    pub lower_functions: Duration,
    pub lower_top_level: Duration,
    pub compact_commit: Duration,
    pub compact_install: Duration,
    pub teardown: Duration,
    pub total: Duration,
}

pub fn json_fixture_text(rows: usize) -> String {
    let mut text = String::from("{\"rows\":[");
    for index in 0..rows {
        if index > 0 {
            text.push(',');
        }
        let enabled = if index % 2 == 0 { "true" } else { "false" };
        text.push_str(&format!(
            "{{\"enabled\":{enabled},\"id\":{index},\"name\":\"pkg-{index}\",\"nested\":{{\"a\":{},\"b\":{}}},\"values\":[{},{},{}]}}",
            index + 1,
            index + 2,
            index,
            index + 1,
            index + 2
        ));
    }
    text.push_str("],\"summary\":{\"count\":");
    text.push_str(&rows.to_string());
    text.push_str(",\"kind\":\"bench\"}}");
    text
}

pub fn json_fixture_value(rows: usize) -> Value {
    let row_values = (0..rows)
        .map(|index| {
            let mut fields = BTreeMap::new();
            fields.insert(Arc::from("enabled"), Value::Bool(index % 2 == 0));
            fields.insert(Arc::from("id"), Value::Int(index as i64));
            fields.insert(Arc::from("name"), Value::Str(format!("pkg-{index}").into()));
            fields.insert(
                Arc::from("nested"),
                Value::Record(RecordMap::from(btree_map(vec![
                    (Arc::from("a"), Value::Int(index as i64 + 1)),
                    (Arc::from("b"), Value::Int(index as i64 + 2)),
                ]))),
            );
            fields.insert(
                Arc::from("values"),
                Value::List(vec![
                    Value::Int(index as i64),
                    Value::Int(index as i64 + 1),
                    Value::Int(index as i64 + 2),
                ]),
            );
            Value::Record(RecordMap::from(fields))
        })
        .collect();

    Value::Record(RecordMap::from(btree_map(vec![
        (Arc::from("rows"), Value::List(row_values)),
        (
            Arc::from("summary"),
            Value::Record(RecordMap::from(btree_map(vec![
                (Arc::from("count"), Value::Int(rows as i64)),
                (Arc::from("kind"), Value::Str(Arc::from("bench"))),
            ]))),
        ),
    ])))
}

pub fn parse_json_fixture(text: &str) -> Result<Value, RuntimeError> {
    parse_json(text, Span::new(SourceId::new(0), 0, text.len()))
}

pub fn encode_json_fixture(value: &Value) -> Result<String, RuntimeError> {
    encode_json(value, false, Span::new(SourceId::new(0), 0, 0))
}

pub fn recursive_fib_source(n: usize) -> String {
    format!(
        "pure fib(n: Int) -> Int {{
  if n < 2 {{
return n
  }}
  return fib(n - 1) + fib(n - 2)
}}

let result = fib({n})
result % 256
"
    )
}

pub fn loop_sum_source(iterations: usize) -> String {
    format!(
        "var i = 0
var sum = 0

while i < {iterations} {{
  sum += i
  i += 1
}}

sum % 256
"
    )
}

pub fn method_dispatch_source(iterations: usize) -> String {
    format!(
        "let xs: List[Int] = [1, 2, 3, 4, 5]
let text = \"alpha beta gamma\"
var i = 0
var total = 0

while i < {iterations} {{
  total += xs.len()
  let item: Int = xs.get(2, 0)
  total += item
  if xs.contains(4) {{
total += 1
  }}
  total += text.count_words()
  total += text.count_chars()
  if text.contains(\"beta\") {{
total += 1
  }}
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn record_map_source(iterations: usize) -> String {
    format!(
        "var i = 0
var total = 0
var counts: Map[Int] = map.empty()

while i < {iterations} {{
  let row = {{name: \"pkg\", version: i, enabled: i % 2 == 0}}
  total += row.version
  let version: Int = row.get(\"version\")?
  total += version
  if row.has(\"enabled\") {{
total += 1
  }}
  counts = counts.set(\"pkg\", counts.get(\"pkg\", 0) + 1)
  total += counts.get(\"pkg\", 0)
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn result_fallback_ir_glue_source(iterations: usize) -> String {
    format!(
        "error E = E(message: Str)\n\npure parse_or_default(raw: Str, default: Int) -> Int {{
  return raw.parse_int() ?? default
}}

pure score(raw: Str, index: Int) -> Int {{
  let parsed = parse_or_default(raw, index % 13)
  return parsed + raw.count_chars()
}}

var i = 0
var total = 0

while i < {iterations} {{
  let raw = if i % 4 == 0 {{ \"bad\" }} else {{ f\"${{i}}\" }}
  total += score(raw, i)
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn error_helper_ir_glue_source(iterations: usize) -> String {
    format!(
        "error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {{
  return f\"usage: ${{applet_name}} ${{summary}}\"
}}

pure usage_error(applet_name: Str, summary: Str) -> Error {{
  return AppletError.Usage(usage(applet_name, summary))
}}

pure validate(raw: Str, index: Int) -> Result[Int] {{
  if raw.starts_with(\"-\") {{
return Err(usage_error(\"tool\", raw))
  }}
  return Ok(raw.count_chars() + index % 7)
}}

var i = 0
var total = 0

while i < {iterations} {{
  let raw = if i % 5 == 0 {{ \"-bad\" }} else {{ \"path\" }}
  total += validate(raw, i) ?? 3
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn result_propagation_source(iterations: usize) -> String {
    format!(
        "error NegativeError = Negative(message: Str)\n\npure normalize(n: Int) -> Result[Int] {{
  if n % 2 == 0 {{
return Ok(n)
  }}
  return Ok(n + 1)
}}

pure accumulate(limit: Int) -> Result[Int] {{
  var i = 0
  var total = 0

  while i < limit {{
total += normalize(i)?
i += 1
  }}

  return Ok(total)
}}

let result = accumulate({iterations})?
result % 256
"
    )
}

pub fn pure_loop_source(iterations: usize) -> String {
    format!(
        "pure checksum(limit: Int) -> Int {{
  var i = 0
  var total = 0

  while i < limit {{
total += (i % 17) * (i % 5)
if total > 1000000 {{
  total = total % 4096
}}
i += 1
  }}

  return total
}}

let result = checksum({iterations})
result % 256
"
    )
}

pub fn pure_call_chain_source(iterations: usize) -> String {
    format!(
        "pure bias(n: Int) -> Int {{
  return (n % 7) + 3
}}

pure weight(n: Int) -> Int {{
  return (n % 11) * bias(n)
}}

pure score(n: Int) -> Int {{
  if n % 5 == 0 {{
return weight(n) - bias(n)
  }}
  return weight(n) + bias(n + 1)
}}

var i = 0
var total = 0

while i < {iterations} {{
  total += score(i)
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn pure_result_validate_source(iterations: usize) -> String {
    format!(
        "error NegativeError = Negative(message: Str)\n\npure normalize(n: Int) -> Result[Int] {{
  if n < 0 {{
return Err(NegativeError.Negative(message: \"negative value\"))
  }}
  if n % 2 == 0 {{
return Ok(n / 2)
  }}
  return Ok((n * 3) + 1)
}}

pure accumulate(limit: Int) -> Result[Int] {{
  var i = 0
  var total = 0

  while i < limit {{
total += normalize(i)?
i += 1
  }}

  return Ok(total)
}}

let result = accumulate({iterations})?
result % 256
"
    )
}

pub fn stream_pipeline_source(size: usize) -> String {
    let values = (0..size)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "let values = [{values}]
let total = values
  |> where . % 3 != 0
  |> map {{ |n|
n * 2
  }}
  |> fold(0) {{ |acc|
acc + .
  }}

total % 256
"
    )
}

pub fn stream_callback_pure_source(size: usize) -> String {
    let values = (0..size)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "pure keep(n: Int) -> Bool {{
  return n % 3 != 0
}}

pure score(n: Int) -> Int {{
  return (n * 2) + (n % 5)
}}

let values = [{values}]
let total = values
  |> where keep(.)
  |> map {{ |n|
score(n)
  }}
  |> fold(0) {{ |acc|
acc + .
  }}

total % 256
"
    )
}

pub fn text_glue_source(iterations: usize) -> String {
    format!(
        "pure tag(name: Str, index: Int) -> Str {{
  return match index % 2 {{
0 => f\"${{name}}:${{index}}:even\"
_ => f\"${{name}}:odd\"
  }}
}}

pure score(label: Str) -> Result[Int] {{
  let cleaned = label.lower().replace(\":\", \",\")
  let parts = cleaned.split(\",\")
  let joined = parts.join(\":\")
  var width = 0
  for part in parts {{
width += part.count_chars()
  }}
  let scale = match \"alpha\" in cleaned or cleaned.ends_with(\"odd\") {{
true => 2
_ => 1
  }}
  let number = if parts.len() > 2 {{
parts.get(1, \"0\").parse_int()?
  }} else {{
0
  }}
  var base = 0
  if \"alpha\" in cleaned or cleaned.ends_with(\"odd\") {{
base = width * scale
  }} else {{
base = width
  }}
  let extra = if \":\" in label and cleaned.ends_with(\"odd\") {{
cleaned.count_bytes() + number
  }} else {{
number
  }}
  return Ok(base + extra)
}}

var i = 0
var total = 0

while i < {iterations} {{
  let base = if i % 3 == 0 {{ \"alpha\" }} else {{ \"beta\" }}
  let label = tag(base, i)
  total += score(label)?
  i += 1
}}

total % 256
"
    )
}

pub fn record_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure score(name: Str, weight: Int, enabled: Bool) -> Result[Int] {{
  let row = {{name: name, weight: weight, enabled: enabled}}
  let base = row.name.count_chars() + row.get(\"weight\")?
  if row.enabled {{
return Ok(base * 2)
  }}
  return Ok(base)
}}

var i = 0
var total = 0

while i < {iterations} {{
  let name = if i % 2 == 0 {{ \"alpha\" }} else {{ \"beta\" }}
  total += score(name, i % 17, i % 3 == 0)?
  i += 1
}}

total % 256
"
    )
}

pub fn collection_ir_glue_source(iterations: usize) -> String {
    let weights = (0..64)
        .map(|index| (index % 17).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "pure score(weights: List[Int], index: Int) -> Int {{
  let row = {{weight: weights[index], enabled: index % 2 == 0}}
  if row.enabled {{
return row.weight * 2
  }}
  return row.weight
}}

pure bump(counts: Map[Int], key: Str, value: Int) -> Map[Int] {{
  return counts.set(key, counts.get(key, 0) + value)
}}

let weights = [{weights}]
var i = 0
var total = 0
var counts: Map[Int] = map.empty()

while i < {iterations} {{
  let value = score(weights, i % weights.len())
  total += value
  counts = bump(counts, \"pkg\", value % 17)
  i += 1
}}

(total + counts.get(\"pkg\", 0)) % 256
"
    )
}

pub fn collection_helpers_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure score(left: List[Str], right: List[Str], counts: Map[Int], row: Record) -> Int {{
  let names = left.extend(right)
  let map_keys = counts.keys()
  let map_values = counts.values()
  let row_keys = row.keys()
  var total = names.len() + map_keys.len() + map_values.len() + row_keys.len()

  for name in names {{
total += name.reverse().count_chars()
  }}

  for key in map_keys {{
total += counts.get(key, 0)
  }}

  for value in map_values {{
total += value
  }}

  for key in row_keys {{
total += key.count_chars()
  }}

  return total
}}

var i = 0
var total = 0
var counts: Map[Int] = map.empty()

while i < {iterations} {{
  counts = counts.set(\"alpha\", counts.get(\"alpha\", 0) + i % 7)
  counts = counts.set(\"beta\", counts.get(\"beta\", 0) + i % 5)
  let left = [\"pkg\", \"lib\"]
  let right = [f\"item-${{i}}\", \"tool\"]
  let row = {{name: \"pkg\", weight: i % 17, enabled: i % 2 == 0}}
  total += score(left, right, counts, row)
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn result_context_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure parse_count(raw: Str, label: Str) -> Result[Int] {{
  return raw.parse_int().context(\"usage\", f\"unsupported ${{label}} '${{raw}}'\")
}}

pure score(raw: Str, label: Str, fallback: Int) -> Int {{
  return parse_count(raw, label) ?? fallback
}}

var i = 0
var total = 0

while i < {iterations} {{
  let raw = if i % 9 == 0 {{ \"bad\" }} else {{ f\"${{i % 97}}\" }}
  total += score(raw, \"count\", 3)
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn path_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure score(path_value: Path, root: Path) -> Int {{
  let normalized = path_value.normalize()
  let relative = normalized.relative_to(root)
  let shown = normalized.display()
  let parent = normalized.parent()
  let name = path_value.name()
  let ext = path_value.ext()
  var total = shown.count_chars() + relative.display().count_chars() + name.count_chars() + ext.count_chars()
  if parent == root {{
total += 7
  }} else {{
total += parent.name().count_chars()
  }}
  return total
}}

let root = p\"/tmp/xsh\"
var i = 0
var total = 0

while i < {iterations} {{
  let dir = if i % 2 == 0 {{ \"xsh\" }} else {{ \"other\" }}
  let path_value = fp\"/tmp/xsh/../${{dir}}/./file-${{i}}.txt\"
  total += score(path_value, root)
  i += 1
}}

total % 256
"
    )
}

pub fn nominal_record_ir_glue_source(iterations: usize) -> String {
    format!(
        "type Row = {{root: Path, name: Str, weight: Int, enabled: Bool}}

pure make_row(root: Path, name: Str, weight: Int, enabled: Bool) -> Row {{
  return {{root: root, name: name, weight: weight, enabled: enabled}}
}}

pure row_score(row: Row) -> Int {{
  let base = row.name.count_chars() + row.root.name().count_chars() + row.weight
  if row.enabled {{
return base * 2
  }}
  return base
}}

pure score(root: Path, name: Str, weight: Int, enabled: Bool) -> Int {{
  return row_score(make_row(root, name, weight, enabled))
}}

let root = p\"/tmp/xsh\"
var i = 0
var total = 0

while i < {iterations} {{
  let name = if i % 2 == 0 {{ \"alpha\" }} else {{ \"beta\" }}
  total += score(root, name, i % 17, i % 3 == 0)
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn pipeline_slice_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure window(raw: Str, start: Int, width: Int) -> Str {{
  let chars = raw.split(\"\")
  let tail = chars |> drop(start)
  let head = tail |> take(width)
  return head.join(\"\")
}}

pure score(raw: Str, start: Int, width: Int) -> Int {{
  let clipped = window(raw, start, width)
  if clipped.contains(\"pkg\") {{
return clipped.count_chars() + 7
  }}
  return clipped.count_chars()
}}

var i = 0
var total = 0

while i < {iterations} {{
  let raw = f\"prefix-pkg-${{i}}-suffix\"
  total += score(raw, i % 4, 6)
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn pipeline_filter_map_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure make_row(name: Str, weight: Int) -> Record {{
  return {{name: name, weight: weight}}
}}

pure score(rows: List[Record], min_weight: Int, offset: Int) -> Int {{
  let selected = rows
|> where .enabled and .weight >= min_weight
|> map make_row(.name.lower(), .weight + offset)
|> drop(1)
|> take(8)
  var total = 0

  for row in selected {{
total += row.name.count_chars() + row.weight
  }}

  return total
}}

let rows = [
  {{name: \"Alpha\", weight: 3, enabled: true}},
  {{name: \"Beta\", weight: 7, enabled: false}},
  {{name: \"Gamma\", weight: 11, enabled: true}},
  {{name: \"Delta\", weight: 13, enabled: true}},
  {{name: \"Epsilon\", weight: 17, enabled: true}},
  {{name: \"Zeta\", weight: 19, enabled: false}},
  {{name: \"Eta\", weight: 23, enabled: true}},
  {{name: \"Theta\", weight: 29, enabled: true}},
  {{name: \"Iota\", weight: 31, enabled: true}},
  {{name: \"Kappa\", weight: 37, enabled: true}},
]
var i = 0
var total = 0

while i < {iterations} {{
  total += score(rows, i % 19, i % 5)
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn text_lines_pipeline_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure normalized_count_lines(input_text: Str) -> List[Str] {{
  let lines = input_text
|> text.lines
|> where .trim() != \"\"
|> map {{ |line|
  let fields = line.fields()
  f\"${{fields[0]}} ${{fields[1]}}\"
}}

  return lines
}}

let sample = \"pkg 10 alpha\\n\\npkg 20 beta\\ntool 30 gamma\\n\"
var i = 0
var total = 0

while i < {iterations} {{
  total += normalized_count_lines(sample).join(\",\").count_chars()
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn group_by_pipeline_ir_glue_source(iterations: usize) -> String {
    format!(
        "type Row = {{name: Str, group: Str, weight: Int}}

pure summarize(rows: List[Row], min_weight: Int) -> Str {{
  let labels = rows
|> where .weight >= min_weight
|> group-by .group
|> sort-by .key
|> map {{ |bucket|
  f\"${{bucket.key}}:${{bucket.items.len()}}:${{bucket.items[0].name.lower()}}\"
}}

  return labels.join(\"|\")
}}

let rows = [
  {{name: \"Alpha\", group: \"net\", weight: 3}},
  {{name: \"Beta\", group: \"fs\", weight: 7}},
  {{name: \"Gamma\", group: \"net\", weight: 11}},
  {{name: \"Delta\", group: \"proc\", weight: 13}},
  {{name: \"Epsilon\", group: \"fs\", weight: 17}},
  {{name: \"Zeta\", group: \"proc\", weight: 19}},
]
var i = 0
var total = 0

while i < {iterations} {{
  total += summarize(rows, i % 17).count_chars()
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn tag_union_ir_glue_source(iterations: usize) -> String {
    format!(
        "type Level = Info | Warn | Error | Debug
type Shape = Circle(Int) | Rect(Int, Int) | Point

pure classify(index: Int) -> Level {{
  return match index % 4 {{
0 => Info
1 => Warn
2 => Error
_ => Debug
  }}
}}

pure level_score(level: Level) -> Int {{
  if level == Info {{
return 1
  }}

  if level == Warn {{
return 2
  }}

  if level == Error {{
return 3
  }}

  4
}}

pure area(shape: Shape) -> Int {{
  match shape {{
Circle(r) => r * r * 3
Rect(w, h) => w * h
Point => 0
  }}
}}

var i = 0
var total = 0

while i < {iterations} {{
  let shape = if i % 3 == 0 {{
Circle(i % 11)
  }} else {{
if i % 3 == 1 {{
  Rect(i % 7 + 1, i % 5 + 2)
}} else {{
  Point
}}
  }}
  total += area(shape) + level_score(classify(i))
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn enumerate_list_comp_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure selected_index(index: Int, seed: Int) -> Bool {{
  return index % 3 == seed % 3 or index == 0
}}

pure select_parts(raw: Str, delimiter: Str, seed: Int) -> Str {{
  let parts = raw.split(delimiter)
  let selected = [item.value for item in parts |> enumerate() if selected_index(item.index, seed)]
  return selected.join(delimiter)
}}

var i = 0
var total = 0

while i < {iterations} {{
  let raw = f\"alpha:${{i}}:beta:${{i % 17}}:gamma:${{i % 5}}:delta\"
  let selected = select_parts(raw, \":\", i)
  total += selected.count_chars()
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn regex_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure score(line: Str, pattern: Str, seed: Int) -> Result[Int] {{
  let re = regex.compile(pattern)?
  if ! re.matches(line) {{
return Ok(seed % 7)
  }}

  let caps = re.captures(line)
  let normalized = re.replace(line, \"pkg:$1:$2\")
  return Ok(caps.len() + caps.get(1, \"\").count_chars() + caps.get(2, \"0\").parse_int()? + normalized.count_chars())
}}

var i = 0
var total = 0

while i < {iterations} {{
  let line = if i % 5 == 0 {{ f\"skip-${{i}}\" }} else {{ f\"pkg-${{i % 97}}-${{i % 13}}\" }}
  total += score(line, \"^pkg-(\\\\d+)-(\\\\d+)$\", i)?
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn status_ir_glue_source(iterations: usize) -> String {
    format!(
        "pure status_score(status: Status, expected: Int) -> Result[Int] {{
  var total = status.kind.count_chars()
  if status.ok {{
total += 3
  }}
  if status.success {{
total += 5
  }}
  if status.exited() {{
let code = status.exit_code()?
if status.exited_with(expected) {{
  total += code + 11
}} else {{
  total += code + 17
}}
  }} else if status.signaled() {{
total += status.signal_number()? + 23
  }}
  return Ok(total)
}}

let ok_status = run.status true
let fail_status = run.status false
var i = 0
var total = 0

while i < {iterations} {{
  let status = if i % 4 == 0 {{ ok_status }} else {{ fail_status }}
  total += status_score(status, i % 2)?
  i += 1
}}

print ${{total % 256}}
"
    )
}

pub fn mixed_glue_source(iterations: usize) -> String {
    format!(
        "pure score(row: Record) -> Result[Int] {{
  let name_len = row.name.count_chars()
  let weight = row.get(\"weight\")?
  if row.enabled {{
return Ok(name_len + weight)
  }}
  return Ok(weight)
}}

var i = 0
var total = 0
var counts: Map[Int] = map.empty()

while i < {iterations} {{
  let row = {{name: \"pkg\", weight: i % 17, enabled: i % 2 == 0}}
  let value = score(row)?
  counts = counts.set(row.name, counts.get(row.name, 0) + value)
  total += counts.get(row.name, 0)
  if row.name.starts_with(\"p\") and row.name.ends_with(\"g\") {{
total += row.name.count_bytes()
  }}
  i += 1
}}

total % 256
"
    )
}

pub fn json_record_glue_source(rows: usize) -> String {
    let records = (0..rows)
        .map(|index| {
            let enabled = if index % 2 == 0 { "true" } else { "false" };
            format!(
                "{{name: \"pkg-{index}\", weight: {}, enabled: {enabled}}}",
                index % 17
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "pure score(row: Record) -> Result[Int] {{
  let base = row.name.count_chars() + row.get(\"weight\")?
  if row.enabled {{
return Ok(base * 2)
  }}
  return Ok(base)
}}

let rows = [{records}]
var total = 0
var counts: Map[Int] = map.empty()

for row in rows {{
  let value = score(row)?
  counts = counts.set(row.name, counts.get(row.name, 0) + value)
  total += counts.get(row.name, 0)
}}

total % 256
"
    )
}

pub fn prepare_source(name: &str, source: &str) -> PreparedProgram {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(name, source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "benchmark source must parse: {:?}",
        parsed.diagnostics
    );
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked.diagnostics.is_empty(),
        "benchmark source must check: {:?}",
        checked.diagnostics
    );
    PreparedProgram {
        arena: parsed.arena,
        source_id,
        sources,
    }
}

pub fn parse_check_source(name: &str, source: &str) {
    let prepared = prepare_source(name, source);
    std::hint::black_box(prepared);
}

pub fn prepare_and_lower_source(name: &str, source: &str) {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(name, source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "benchmark source must parse: {:?}",
        parsed.diagnostics
    );
    let mut evaluator = Evaluator::new_with_sources_at_cwd(Vec::new(), sources, PathBuf::from("."));
    let diagnostics = evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        diagnostics.is_empty(),
        "benchmark source must compact-check/lower: {diagnostics:?}"
    );
    std::hint::black_box((parsed, evaluator));
}

pub fn time_prepare_and_lower_source(name: &str, source: &str) -> FrontendLowerTimings {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(name, source);
    let started = Instant::now();
    let parsed = Parser::parse_source_arena_only(source_id, source);
    let after_parse = Instant::now();
    assert!(
        parsed.diagnostics.is_empty(),
        "benchmark source must parse: {:?}",
        parsed.diagnostics
    );
    let (mut evaluator, evaluator_timings) =
        Evaluator::new_with_sources_at_cwd_profiled(Vec::new(), sources, PathBuf::from("."));
    let (diagnostics, install) =
        evaluator.install_compact_lowered_program_profiled(&parsed.arena, source_id);
    assert!(
        diagnostics.is_empty(),
        "benchmark source must compact-check/lower: {diagnostics:?}"
    );
    let before_teardown = Instant::now();
    drop(std::hint::black_box((parsed, evaluator)));
    let after_teardown = Instant::now();
    FrontendLowerTimings {
        parse: after_parse.duration_since(started),
        evaluator_init: evaluator_timings.total,
        evaluator_current_dir: evaluator_timings.current_dir,
        evaluator_struct_init: evaluator_timings.struct_init,
        evaluator_args_bindings: evaluator_timings.args_bindings,
        compact_declarations: install.declarations,
        compact_runtime_declarations: install.runtime_declarations,
        compact_bodies: install.bodies,
        lower_functions: install.functions,
        lower_top_level: install.top_level,
        compact_commit: install.commit,
        compact_install: install.total,
        teardown: after_teardown.duration_since(before_teardown),
        total: after_teardown.duration_since(started),
    }
}

pub fn eval_prepared(prepared: &PreparedProgram) -> u8 {
    let output = Evaluator::new_with_sources(Vec::new(), prepared.sources.clone())
        .eval(&prepared.arena, prepared.source_id);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(output.traceback.is_none(), "{:?}", output.traceback);
    output.status
}

pub fn eval_prepared_output(prepared: &PreparedProgram) -> EvalOutput {
    Evaluator::new_with_sources(Vec::new(), prepared.sources.clone())
        .eval(&prepared.arena, prepared.source_id)
}

#[cfg(test)]
mod bench_tests {
    use crate::runtime::bench;

    #[test]
    fn interpreter_benchmark_sources_prepare_and_eval() {
        let cases = [
            ("bench-fib.xsh", bench::recursive_fib_source(8)),
            ("bench-loop.xsh", bench::loop_sum_source(10)),
            ("bench-methods.xsh", bench::method_dispatch_source(10)),
            ("bench-record-map.xsh", bench::record_map_source(10)),
            ("bench-results.xsh", bench::result_propagation_source(10)),
            (
                "bench-result-fallback-ir-glue.xsh",
                bench::result_fallback_ir_glue_source(10),
            ),
            (
                "bench-error-helper-ir-glue.xsh",
                bench::error_helper_ir_glue_source(10),
            ),
            ("bench-pure-loop.xsh", bench::pure_loop_source(10)),
            (
                "bench-pure-call-chain.xsh",
                bench::pure_call_chain_source(10),
            ),
            (
                "bench-pure-result-validate.xsh",
                bench::pure_result_validate_source(10),
            ),
            ("bench-stream.xsh", bench::stream_pipeline_source(10)),
            (
                "bench-stream-callback-pure.xsh",
                bench::stream_callback_pure_source(10),
            ),
            ("bench-text-glue.xsh", bench::text_glue_source(10)),
            ("bench-record-ir-glue.xsh", bench::record_ir_glue_source(10)),
            (
                "bench-collection-ir-glue.xsh",
                bench::collection_ir_glue_source(10),
            ),
            (
                "bench-collection-helpers-ir-glue.xsh",
                bench::collection_helpers_ir_glue_source(10),
            ),
            (
                "bench-result-context-ir-glue.xsh",
                bench::result_context_ir_glue_source(10),
            ),
            ("bench-path-ir-glue.xsh", bench::path_ir_glue_source(10)),
            (
                "bench-nominal-record-ir-glue.xsh",
                bench::nominal_record_ir_glue_source(10),
            ),
            (
                "bench-pipeline-slice-ir-glue.xsh",
                bench::pipeline_slice_ir_glue_source(10),
            ),
            (
                "bench-pipeline-filter-map-ir-glue.xsh",
                bench::pipeline_filter_map_ir_glue_source(10),
            ),
            (
                "bench-text-lines-pipeline-ir-glue.xsh",
                bench::text_lines_pipeline_ir_glue_source(10),
            ),
            (
                "bench-group-by-pipeline-ir-glue.xsh",
                bench::group_by_pipeline_ir_glue_source(10),
            ),
            (
                "bench-tag-union-ir-glue.xsh",
                bench::tag_union_ir_glue_source(10),
            ),
            (
                "bench-enumerate-list-comp-ir-glue.xsh",
                bench::enumerate_list_comp_ir_glue_source(10),
            ),
            ("bench-regex-ir-glue.xsh", bench::regex_ir_glue_source(10)),
            ("bench-status-ir-glue.xsh", bench::status_ir_glue_source(10)),
            ("bench-mixed.xsh", bench::mixed_glue_source(10)),
            (
                "bench-json-record-glue.xsh",
                bench::json_record_glue_source(10),
            ),
        ];

        for (name, source) in cases {
            let prepared = bench::prepare_source(name, &source);
            bench::eval_prepared(&prepared);
        }
    }

    #[test]
    fn text_glue_benchmark_exercises_scalar_string_flow() {
        let source = bench::text_glue_source(10);
        let prepared = bench::prepare_source("bench-text-glue.xsh", &source);

        assert_eq!(bench::eval_prepared(&prepared), 203);
    }

    #[test]
    fn record_ir_benchmark_exercises_record_literal_and_fields() {
        let source = bench::record_ir_glue_source(10);
        let prepared = bench::prepare_source("bench-record-ir-glue.xsh", &source);

        assert_eq!(bench::eval_prepared(&prepared), 126);
    }

    #[test]
    fn collection_ir_benchmark_exercises_list_and_map_flow() {
        let source = bench::collection_ir_glue_source(10);
        let prepared = bench::prepare_source("bench-collection-ir-glue.xsh", &source);

        assert_eq!(bench::eval_prepared(&prepared), 130);
    }

    #[test]
    fn text_lines_pipeline_ir_benchmark_exercises_adapter_and_map_block() {
        let source = bench::text_lines_pipeline_ir_glue_source(10);
        let prepared = bench::prepare_source("bench-text-lines-pipeline-ir-glue.xsh", &source);
        let output = bench::eval_prepared_output(&prepared);

        assert_eq!(output.stdout, b"210\n");
        assert!(output.traceback.is_none());
    }

    #[test]
    fn group_by_pipeline_ir_benchmark_exercises_bucket_records() {
        let source = bench::group_by_pipeline_ir_glue_source(10);
        let prepared = bench::prepare_source("bench-group-by-pipeline-ir-glue.xsh", &source);
        let output = bench::eval_prepared_output(&prepared);

        assert_eq!(output.stdout, b"90\n");
        assert!(output.traceback.is_none());
    }

    #[test]
    fn tag_union_ir_benchmark_exercises_constructors_and_patterns() {
        let source = bench::tag_union_ir_glue_source(10);
        let prepared = bench::prepare_source("bench-tag-union-ir-glue.xsh", &source);
        let output = bench::eval_prepared_output(&prepared);

        assert_eq!(output.stdout, b"185\n");
        assert!(output.traceback.is_none());
    }

    #[test]
    fn lowered_collection_helpers_cover_extend_and_keys() {
        let source =
        "pure score(left: List[Str], right: List[Str], counts: Map[Int], row: Record) -> Int {
  let names = left.extend(right)
  let map_keys = counts.keys()
  let map_values = counts.values()
  let row_keys = row.keys()
  return names.join(\":\").reverse().count_chars() + map_keys.len() + map_values.len() + row_keys.len()
}

var counts: Map[Int] = map.empty()
counts = counts.set(\"alpha\", 1)
counts = counts.set(\"beta\", 2)
let row = {name: \"pkg\", weight: 4, enabled: true}
score([\"a\", \"b\"], [\"c\"], counts, row)
";
        let prepared = bench::prepare_source("bench-lowered-collection-helpers.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 12);
    }

    #[test]
    fn lowered_continue_skips_loop_remainder() {
        let source = "pure score(values: List[Int]) -> Int {
  var total = 0
  for value in values {
continue when value % 2 == 0
if value > 7 {
  continue
}
total += value
  }
  return total
}

score([1, 2, 3, 4, 9, 11])
";
        let prepared = bench::prepare_source("bench-lowered-continue.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 4);
    }

    #[test]
    fn lowered_break_exits_loop() {
        let source = "pure until(limit: Int, stop: Int) -> Int {
  var total = 0
  var i = 0

  while i < limit {
if i == stop {
  break
}

total += i
i += 1
  }

  return total
}

until(10, 4)
";
        let prepared = bench::prepare_source("bench-lowered-break.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 6);
    }

    #[test]
    fn lowered_result_context_wraps_err_and_preserves_ok() {
        let source = "pure parse_count(raw: Str) -> Result[Int] {
  return raw.parse_int().context(\"usage\", f\"bad count '${raw}'\")
}

let good = parse_count(\"41\")?
let bad = parse_count(\"nope\") ?? 1
print f\"${good}:${bad}\"
";
        let prepared = bench::prepare_source("bench-lowered-result-context.xsh", source);
        let output = bench::eval_prepared_output(&prepared);

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.traceback.is_none(), "{:?}", output.traceback);
        let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
        assert_eq!(stdout.trim(), "41:1");
    }

    #[test]
    fn lowered_result_unit_returns_ok_unit() {
        let source =
            "error E = E(message: Str)\n\npure ensure_positive(value: Int) -> Result[Unit] {
  if value < 0 {
return Err(E.E(message: \"value must be positive\"))
  }

  return
}

ensure_positive(1)?
print \"ok\"
";
        let prepared = bench::prepare_source("bench-lowered-result-unit.xsh", source);
        let output = bench::eval_prepared_output(&prepared);

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.traceback.is_none(), "{:?}", output.traceback);
        let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
        assert_eq!(stdout.trim(), "ok");
    }

    #[test]
    fn lowered_logical_operators_short_circuit_result_propagation() {
        let source = "error E = E(message: Str)\n\npure fail() -> Result[Bool] {
  return Err(E.E(message: \"right side evaluated\"))
}

pure score() -> Result[Int] {
  if false and fail()? {
return Ok(1)
  }
  if true or fail()? {
return Ok(0)
  }
  return Ok(2)
}

score()?
";
        let prepared = bench::prepare_source("bench-lowered-short-circuit.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 0);
    }

    #[test]
    fn lowered_text_methods_cover_split_replace_join_and_parse_int() {
        let source = "pure score(raw: Str) -> Result[Int] {
  let normalized = raw.lower().replace(\"|\", \":\")
  let parts = normalized.split(\":\")
  let joined = parts.join(\"/\")
  let value = parts.get(1, \"0\").parse_int()?
  return Ok(joined.count_chars() + value)
}

score(\"pkg|0x2a\")?
";
        let prepared = bench::prepare_source("bench-lowered-text-methods.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 50);
    }

    #[test]
    fn lowered_text_scanner_covers_byte_methods_and_bool_state() {
        let source = "pure scan_line(line: Str) -> Int {
  let n = line.byte_len()
  var index = 0
  var score = 0
  var in_string = false
  var delim = -1

  while index < n {
let ch = line.byte_at(index)
let next = line.byte_at(index + 1)

if in_string {
  if ch == delim {
    in_string = false
  } else {
    score += ch % 7
  }
} else if ch == 47 and next == 47 {
  return score
} else if ch == 34 or ch == 39 {
  in_string = true
  delim = ch
} else if ch != 32 and ch != 9 {
  score += 1
}

index += 1
  }

  return score
}

pure scan_many(line: Str, limit: Int) -> Int {
  var total = 0
  var i = 0

  while i < limit {
total += scan_line(line)
i += 1
  }

  return total
}

scan_many(\"ab \\\"cd\\\" // ef\", 3)
";
        let prepared = bench::prepare_source("bench-lowered-text-scanner.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 15);
    }

    #[test]
    fn lowered_text_lines_for_loop_counts_without_materialized_list() {
        let source = "pure count_nonblank(text: Str) -> Int {
  var total = 0

  for line in text.lines() {
if line.trim() != \"\" {
  total += 1
}
  }

  return total
}

count_nonblank(\"a\\n\\nb\\n\")
";
        let prepared = bench::prepare_source("bench-lowered-text-lines-for.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 2);
    }

    #[test]
    fn lowered_text_lines_loop_counts_prefixed_comments_with_views() {
        let source = "pure score(text: Str) -> Int {
  var blanks = 0
  var code = 0
  var comments = 0

  for line in text.lines() {
let trimmed = line.trim()

if trimmed == \"\" {
  blanks += 1
} else if trimmed.starts_with(\"#\") {
  comments += 1
} else {
  code += 1
}
  }

  return blanks * 100 + code * 10 + comments
}

score(\"alpha\\n  # note\\n\\n beta\\n\")
";
        let prepared = bench::prepare_source("bench-lowered-text-line-views.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 121);
    }

    #[test]
    fn lowered_list_helper_keeps_slot_typed_assignments_generic() {
        let source = "pure extend_unique(values: List[Str], extra: List[Str]) -> List[Str] {
  var result = values

  for item in extra {
if item != \"\" and ! result.contains(item) {
  result = result.push(item)
}
  }

  return result
}

let values = extend_unique([\"a\"], [\"a\", \"b\"])
values.len()
";
        let prepared = bench::prepare_source("bench-lowered-list-helper-slots.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 2);
    }

    #[test]
    fn lowered_regex_compile_matches_captures_and_replace() {
        let source = "pure score(line: Str, pattern: Str) -> Result[Int] {
  let re = regex.compile(pattern)?
  if ! re.matches(line) {
return Ok(3)
  }

  let caps = re.captures(line)
  let normalized = re.replace(line, \"pkg:$1:$2\")
  return Ok(caps.len() + caps[1].count_chars() + caps[2].parse_int()? + normalized.count_chars())
}

let good = score(\"pkg-42-5\", \"^pkg-(\\\\d+)-(\\\\d+)$\")?
let miss = score(\"skip\", \"^pkg-(\\\\d+)-(\\\\d+)$\")?
let bad = score(\"pkg-1-2\", \"(\") ?? 7
good + miss + bad
";
        let prepared = bench::prepare_source("bench-lowered-regex-methods.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 28);
    }

    #[test]
    fn lowered_status_methods_and_fields_cover_process_status_values() {
        let source = "pure status_score(status: Status, expected: Int) -> Result[Int] {
  var total = status.kind.count_chars()
  if status.ok {
total += 3
  }
  if status.success {
total += 5
  }
  if status.exited() {
let code = status.exit_code()?
if status.exited_with(expected) {
  total += code + 11
} else {
  total += code + 17
}
  } else if status.signaled() {
total += status.signal_number()? + 23
  }
  return Ok(total)
}

let ok_status = run.status true
let fail_status = run.status false
status_score(ok_status, 0)? + status_score(fail_status, 0)?
";
        let prepared = bench::prepare_source("bench-lowered-status-methods.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 45);
    }

    #[test]
    fn lowered_result_fallback_unwraps_ok_and_uses_lazy_default() {
        let source =
            "error E = E(message: Str)\n\npure parse_or_default(raw: Str, default: Int) -> Int {
  return raw.parse_int() ?? default
}

pure fail() -> Result[Int] {
  return Err(E.E(message: \"right side evaluated\"))
}

pure score() -> Result[Int] {
  let good = \"42\".parse_int() ?? fail()?
  let bad = parse_or_default(\"bad\", 5)
  return Ok(good + bad)
}

score()?
";
        let prepared = bench::prepare_source("bench-lowered-result-fallback.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 47);
    }

    #[test]
    fn lowered_error_return_and_err_constructor_cover_structured_errors() {
        let source = "error AppletError = Usage(message: Str) : Usage

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(f\"usage: ${applet_name} ${summary}\")
}

pure fail(applet_name: Str) -> Result[Int] {
  return Err(usage_error(applet_name, \"PATH...\"))
}

let err = usage_error(\"copy\", \"SRC DEST\")
let recovered = fail(\"copy\") ?? 7
print f\"${err.message}:${recovered}\"
";
        let prepared = bench::prepare_source("bench-lowered-error-constructor.xsh", source);
        let output = bench::eval_prepared_output(&prepared);

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.traceback.is_none(), "{:?}", output.traceback);
        let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
        assert_eq!(stdout.trim(), "usage: copy SRC DEST:7");
    }

    #[test]
    fn lowered_list_pipeline_covers_take_and_drop() {
        let source = "pure window(raw: Str, start: Int, width: Int) -> Str {
  let chars = raw.split(\"\")
  let tail = chars |> drop(start)
  let head = tail |> take(width)
  return head.join(\"\")
}

print f\"${window(\"abcdef\", 2, 3)}:${window(\"abcdef\", -2, 2)}:${window(\"abcdef\", 4, 20)}\"
";
        let prepared = bench::prepare_source("bench-lowered-list-pipeline.xsh", source);
        let output = bench::eval_prepared_output(&prepared);

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.traceback.is_none(), "{:?}", output.traceback);
        let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
        assert_eq!(stdout.trim(), "cde:ab:ef");
    }

    #[test]
    fn lowered_list_pipeline_covers_where_map_and_item_fields() {
        let source = "pure make_row(name: Str, weight: Int) -> Record {
  return {name: name, weight: weight}
}

pure score(rows: List[Record]) -> Int {
  let selected = rows
|> where .enabled and .weight >= 10
|> map make_row(.name.lower(), .weight + 1)
|> drop(1)
|> take(2)
  var total = 0

  for row in selected {
total += row.name.count_chars() + row.weight
  }

  return total
}

let rows = [
  {name: \"Alpha\", weight: 3, enabled: true},
  {name: \"Beta\", weight: 11, enabled: true},
  {name: \"Gamma\", weight: 13, enabled: true},
  {name: \"Delta\", weight: 17, enabled: true},
  {name: \"Epsilon\", weight: 19, enabled: false},
]
score(rows)
";
        let prepared = bench::prepare_source("bench-lowered-list-pipeline-filter-map.xsh", source);
        assert_eq!(bench::eval_prepared(&prepared), 42);
    }

    #[test]
    fn lowered_list_comprehension_covers_enumerate_pipeline() {
        let source = "pure selected_index(index: Int, seed: Int) -> Bool {
  return index % 2 == seed % 2
}

pure select_parts(raw: Str, delimiter: Str, seed: Int) -> Str {
  let parts = raw.split(delimiter)
  let selected = [item.value for item in parts |> enumerate() if selected_index(item.index, seed)]
  return selected.join(delimiter)
}

let selected = select_parts(\"a:b:c:d:e\", \":\", 1)
print ${selected}
";
        let prepared = bench::prepare_source("bench-lowered-enumerate-list-comp.xsh", source);
        let output = bench::eval_prepared_output(&prepared);

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.traceback.is_none(), "{:?}", output.traceback);
        let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
        assert_eq!(stdout.trim(), "b:d");
    }

    #[test]
    fn lowered_list_pipeline_covers_sort_and_sort_by() {
        let source = "type Row = {name: Str, rank: Int}

pure summarize(rows: List[Row]) -> Str {
  let names = rows
|> sort-by .rank
|> map .name
|> sort
  return names.join(\":\")
}

let rows = [
  {name: \"gamma\", rank: 3},
  {name: \"alpha\", rank: 1},
  {name: \"beta\", rank: 2},
]
print ${summarize(rows)}
";
        let prepared = bench::prepare_source("bench-lowered-pipeline-sort.xsh", source);
        let output = bench::eval_prepared_output(&prepared);

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.traceback.is_none(), "{:?}", output.traceback);
        let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
        assert_eq!(stdout.trim(), "alpha:beta:gamma");
    }

    #[test]
    fn lowered_for_over_list_accumulates_string_parts() {
        let source = "pure sum_parts(raw: Str) -> Result[Int] {
  var total = 0
  for part in raw.split(\":\") {
total += part.parse_int()?
  }
  return Ok(total)
}

sum_parts(\"1:0x2:0b11\")?
";
        let prepared = bench::prepare_source("bench-lowered-for-list.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 6);
    }

    #[test]
    fn lowered_membership_covers_strings_and_lists() {
        let source = "pure score(raw: Str, values: List[Int]) -> Int {
  var total = 0
  if \"warn\" in raw {
total += 10
  }
  if \"debug\" not in raw {
total += 5
  }
  if 3 in values {
total += 3
  }
  if 9 not in values {
total += 1
  }
  return total
}

score(\"warn:info\", [1, 3, 5])
";
        let prepared = bench::prepare_source("bench-lowered-membership.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 19);
    }

    #[test]
    fn lowered_statement_if_else_assigns_selected_branch() {
        let source = "pure score(flag: Bool) -> Int {
  var value = 0
  if flag {
value = 7
  } else {
value = 3
  }
  return value
}

score(true) + score(false)
";
        let prepared = bench::prepare_source("bench-lowered-if-else.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 10);
    }

    #[test]
    fn lowered_match_covers_scalar_expression_and_statement_arms() {
        let source = "type Kind = Alpha | Beta | Other

pure classify(name: Str, n: Int, kind: Kind) -> Int {
  var base = 0
  match name {
\"alpha\" => base = 10
\"beta\" => base = 20
_ => base = 30
  }
  let offset = match n % 3 {
0 => 1
1 => 2
_ => 3
  }
  let bonus = match name {
\"alpha\" => 10
\"gamma\" => 20
_ => 30
  }
  let kind_bonus = match kind {
Alpha => 1
Beta => 2
Other => 3
  }
  return base + offset + bonus + kind_bonus
}

classify(\"alpha\", 4, Alpha) + classify(\"beta\", 5, Beta) + classify(\"other\", 6, Other)
";
        let prepared = bench::prepare_source("bench-lowered-match-scalar.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 142);
    }

    #[test]
    fn lowered_path_methods_cover_display_name_ext_parent_normalize_relative_and_equality() {
        let source = "pure score(path_value: Path, root: Path) -> Int {
  let normalized = path_value.normalize()
  let parent = normalized.parent()
  let relative = normalized.relative_to(root)
  var total = normalized.display().count_chars() + relative.display().count_chars() + path_value.name().count_chars() + path_value.ext().count_chars()
  if parent == root {
total += 7
  }
  return total
}

let root = p\"/tmp/xsh\"
let path_value = p\"/tmp/xsh/../xsh/./file.txt\"
score(path_value, root)
";
        let prepared = bench::prepare_source("bench-lowered-path-methods.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 43);
    }

    #[test]
    fn lowered_named_record_schema_params_and_returns() {
        let source = "type Row = {path: Path, size: Int, name: Str}

pure make_row(path_value: Path, size: Int) -> Row {
  return {path: path_value, size: size, name: path_value.name()}
}

pure row_score(row: Row) -> Int {
  return row.path.name().count_chars() + row.size + row.name.count_chars()
}

let path_value = p\"/tmp/pkg.txt\"
row_score(make_row(path_value, 7))
";
        let prepared = bench::prepare_source("bench-lowered-named-record-schema.xsh", source);

        assert_eq!(bench::eval_prepared(&prepared), 21);
    }
}
