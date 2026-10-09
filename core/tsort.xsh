#!/bin/xsh
use lib.gnu

const USAGE = """Usage: tsort [OPTION] [FILE]
Write totally ordered list consistent with the partial ordering in FILE.

With no FILE, or when FILE is -, read standard input.

      --help        display this help and exit
      --version     output version information and exit
"""

type TsortOptions = {help: Bool, version: Bool, files: List[Str]}

pure raw_for(argv: List[Str], raw: List[Bytes], name: Str) -> Bytes {
  for index in range(argv.len()) {
    if argv[index] == name { return raw[index] }
  }
  bytes.from_text(name)
}

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

# Past this many unprinted nodes GNU's loop search is quadratic and finishes
# in no useful time, so a depth-first search is used instead.
const LARGE_GRAPH = 5000

# A loop among the unprinted nodes, found by depth-first search from each in
# name order: the nodes on it from where the search met its own path again.
pure find_loop(top: List[List[Int]], done: List[Bool], order: List[Int]) -> List[Int] {
  let total = top.len()
  var state: List[Int] = [0 for _ in range(total)]
  var stack: List[Int] = [0 for _ in range(total + 1)]
  var next: List[Int] = [0 for _ in range(total + 1)]

  for start in order {
    if done[start] or state[start] != 0 {
      continue
    }

    var depth = 1

    stack[0] = start
    next[0] = 0
    state[start] = 1

    while depth > 0 {
      let node = stack[depth - 1]
      let at = next[depth - 1]

      if at >= top[node].len() {
        state[node] = 2
        depth -= 1
        continue
      }

      next[depth - 1] = at + 1

      let target = top[node][at]

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
  let raw_args = cli.argv_bytes()
  let opts: TsortOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "-h --help", default: false, stop: true},
      version: {form: "-V --version", default: false, stop: true},
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
  let raw_name = if opts.files.len() > 0 { raw_for(argv, raw_args, name) } else { b"-" }
  let data = if name == "-" {
    guard let found = gnu.read_operand(name) else { |failure|
      gnu.name_error(name, failure)
      exit 1
    }
    found
  } else {
    let target = Path.parse_bytes(raw_name)?
    if let Ok(found) = fs.stat(target, follow_symlinks: true) {
      if found.kind == "dir" {
        gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: read error: Is a directory")
        exit 1
      }
    }
    guard let found = target.read_bytes() else { |failure|
      gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: {gnu.strerror(failure)}")
      exit 1
    }
    found
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

  # Successors in GNU's order: the most recently added edge first.
  var top: List[List[Int]] = [[succ[node][succ[node].len() - 1 - index] for index in range(succ[node].len())] for node in range(total)]
  var count = preds
  var done: List[Bool] = [false for _ in range(total)]
  var link: List[Int] = [-1 for _ in range(total)]
  let order = [item.node for item in [{key: keys[node], node: node} for node in range(total)] |> sort-by .key]
  var queue: List[Int] = [node for node in order if count[node] == 0]
  var head = 0
  var remaining = total
  var looped = false
  var out: List[Bytes] = []

  while remaining > 0 {
    if head >= queue.len() {
      gnu.error(f"{gnu.quote_maybe(name)}: input contains a loop:")
      looped = true

      if remaining > LARGE_GRAPH {
        let cycle = find_loop(top, done, order)
        let last = cycle[cycle.len() - 1]
        let first = cycle[0]

        for node in cycle {
          gnu.error(keys[node])
        }

        var at = 0

        while top[last][at] != first {
          at += 1
        }

        top[last] = top[last][..at] + top[last][at + 1..]
        count[first] -= 1
      } else {
        # GNU's loop search: walk the nodes in name order, chaining each unprinted
        # node that has an edge to the chain's head, until a node that is already
        # chained closes a cycle; the cycle is printed and the closing edge dropped.
        var chain = -1
        var found = false

        while ! found {
          for node in order {
            if count[node] == 0 or found {
              continue
            }

            if chain < 0 {
              chain = node
              continue
            }

            var at = 0

            while at < top[node].len() and ! found {
              if top[node][at] == chain {
                if link[node] >= 0 {
                  var walk = chain

                  while walk >= 0 {
                    let next = link[walk]

                    gnu.error(keys[walk])

                    if walk == node {
                      count[chain] -= 1
                      top[node] = top[node][..at] + top[node][at + 1..]
                      break
                    }

                    link[walk] = -1
                    walk = next
                  }

                  while walk >= 0 {
                    let next = link[walk]

                    link[walk] = -1
                    walk = next
                  }

                  chain = -1
                  found = true
                } else {
                  link[node] = chain
                  chain = node
                  break
                }
              }

              at += 1
            }
          }
        }
      }

      for node in order {
        if count[node] == 0 and ! done[node] {
          queue += [node]
        }
      }

      continue
    }

    let node = queue[head]

    head += 1
    done[node] = true
    remaining -= 1
    out += [labels[node], b"\n"]

    for target in top[node] {
      count[target] -= 1

      if count[target] == 0 {
        queue += [target]
      }
    }
  }

  gnu.write_bytes(bytes.concat(out))

  if looped {
    exit 1
  }
}
