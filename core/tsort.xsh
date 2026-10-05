#!/bin/xsh
use lib.gnu

const USAGE = """Usage: tsort [OPTION] [FILE]
Write totally ordered list consistent with the partial ordering in FILE.

With no FILE, or when FILE is -, read standard input.

      --help        display this help and exit
      --version     output version information and exit
"""

type TsortOptions = {help: Bool, version: Bool, files: List[Str]}

const HEX = "0123456789abcdef"

# Hex of a token, so tokens that are not valid UTF-8 still order bytewise as text.
pure hex_key(token: Bytes) -> Str {
  var out = ""

  for index in range(token.len()) {
    let value = token.byte_at(index) ?? 0

    out = out + HEX.byte_slice(value / 16, length: 1) + HEX.byte_slice(value % 16, length: 1)
  }

  out
}

pure is_space(value: Int) -> Bool {
  value == 32 or value == 9 or value == 10
}

# The whitespace separated tokens of the input, as bytes.
pure tokenize(data: Bytes) -> List[Bytes] {
  if let Ok(text) = data.utf8() {
    return [bytes.from_text(word) for word in text.replace("\t", " ").replace("\n", " ").split(" ") if word != ""]
  }

  var out: List[Bytes] = []
  var start = -1

  for index in range(data.len()) {
    if is_space(data.byte_at(index) ?? 0) {
      if start >= 0 {
        out += [data[start..index]]
        start = -1
      }
    } else if start < 0 {
      start = index
    }
  }

  if start >= 0 {
    out += [data[start..]]
  }

  out
}

type Graph = {labels: List[Bytes], keys: List[Str], succ: List[List[Int]], preds: List[Int]}

# A loop among the nodes that are still unprinted, found by depth-first search
# from each node in name order; the nodes on it from the point where the search
# met its own path again.
pure find_loop(graph: Graph, done: List[Bool]) -> List[Int] {
  let total = graph.keys.len()
  var order = [{key: graph.keys[node], node: node} for node in range(total) if ! done[node]] |> sort-by .key
  var state: List[Int] = [0 for _ in range(total)]
  var stack: List[Int] = [0 for _ in range(total + 1)]
  var next: List[Int] = [0 for _ in range(total + 1)]

  for item in order {
    if state[item.node] != 0 {
      continue
    }

    var depth = 1

    stack[0] = item.node
    next[0] = 0
    state[item.node] = 1

    while depth > 0 {
      let top = stack[depth - 1]
      let at = next[depth - 1]

      if at >= graph.succ[top].len() {
        state[top] = 2
        depth -= 1
        continue
      }

      next[depth - 1] = at + 1

      let target = graph.succ[top][at]

      if state[target] == 0 {
        state[target] = 1
        stack[depth] = target
        next[depth] = 0
        depth += 1
      } else if state[target] == 1 {
        var from = depth - 1

        while stack[from] != target {
          from -= 1
        }

        return stack[from..depth]
      }
    }
  }

  []
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: TsortOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("tsort")
    return
  }

  if opts.files.len() > 1 {
    gnu.extra_operand(opts.files[1])
  }

  let name = opts.files.get(0) ?? "-"

  guard let data = gnu.read_operand(name) else { |failure|
    if gnu.errno(failure) == 21 {
      gnu.error(f"{gnu.quote_maybe(name)}: read error: Is a directory")
    } else {
      gnu.name_error(name, failure)
    }

    exit 1
  }

  let tokens = tokenize(data)

  if tokens.len() % 2 == 1 {
    gnu.error(f"{gnu.quote_maybe(name)}: input contains an odd number of tokens")
    exit 1
  }

  var ids: Map[Str, Int] = map.empty()
  var labels: List[Bytes] = []
  var keys: List[Str] = []
  var succ: List[List[Int]] = []
  var preds: List[Int] = []
  var ends: List[Int] = []
  let text_mode = data.utf8() is Ok(_)

  for token in tokens {
    let key = if text_mode { token.utf8() ?? "" } else { hex_key(token) }
    let known = ids.get(key) ?? -1

    if known < 0 {
      ids[key] = labels.len()
      ends += [labels.len()]
      labels += [token]
      keys += [key]
      succ += [[]]
      preds += [0]
    } else {
      ends += [known]
    }
  }

  var at = 0

  while at < ends.len() {
    let from = ends[at]
    let to = ends[at + 1]

    if from != to {
      succ[from] += [to]
      preds[to] += 1
    }

    at += 2
  }

  let total = labels.len()
  var graph: Graph = {labels: labels, keys: keys, succ: succ, preds: preds}
  var done: List[Bool] = [false for _ in range(total)]
  var queue: List[Int] = [item.node for item in [{key: keys[node], node: node} for node in range(total) if preds[node] == 0] |> sort-by .key]
  var head = 0
  var remaining = total
  var looped = false
  var out: List[Bytes] = []

  while remaining > 0 {
    if head >= queue.len() {
      let cycle = find_loop(graph, done)

      gnu.error(f"{gnu.quote_maybe(name)}: input contains a loop:")

      for node in cycle {
        gnu.error(graph.keys[node])
      }

      looped = true

      let last = cycle[cycle.len() - 1]
      let first = cycle[0]
      var rest: List[Int] = []
      var removed = false

      for target in graph.succ[last] {
        if target == first and ! removed {
          removed = true
        } else {
          rest += [target]
        }
      }

      graph.succ[last] = rest
      graph.preds[first] -= 1

      if graph.preds[first] == 0 {
        queue += [first]
      }

      continue
    }

    let node = queue[head]

    head += 1
    done[node] = true
    remaining -= 1
    out += [graph.labels[node], b"\n"]

    let targets = graph.succ[node]

    for index in range(targets.len()) {
      let target = targets[targets.len() - 1 - index]

      graph.preds[target] -= 1

      if graph.preds[target] == 0 {
        queue += [target]
      }
    }
  }

  gnu.write_bytes(bytes.concat(out))

  if looped {
    exit 1
  }
}
