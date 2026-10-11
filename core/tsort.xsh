#!/bin/xsh
use lib.gnu

const USAGE = """Usage: tsort [OPTION] [FILE]
Write totally ordered list consistent with the partial ordering in FILE.

With no FILE, or when FILE is -, read standard input.

      --help        display this help and exit
      --version     output version information and exit
"""

type TsortOptions = {help: Bool, version: Bool, warn: Bool, files: List[Str]}

const HEX = "0123456789abcdef"

# Past this many unprinted nodes GNU's loop search is quadratic and finishes
# in no useful time, so a depth-first search is used instead.
const LARGE_GRAPH = 5000

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
    return [
      bytes.from_text(word)
      for word in text.replace("\t", with: " ").replace("\n", with: " ").split(" ")
      if word != ""
    ]
  }

  var start = -1

  let out: List[Bytes] = collect {
    for index in range(data.len()) {
      if is_space(data.byte_at(index) ?? 0) {
        if start >= 0 {
          yield data[start..index]
          start = -1
        }
      } else if start < 0 {
        start = index
      }
    }

    yield data[start..] when start >= 0
  }

  out
}

# A loop among the unprinted nodes, found by depth-first search from each in
# name order: the nodes on it from where the search met its own path again.
pure find_loop(top: List[List[Int]], done: List[Bool], order: List[Int]) -> List[Int] {
  let total = top.len()
  var state: List[Int] = [0 for _ in range(total)]
  var stack: List[Int] = [0 for _ in range(total + 1)]
  var next: List[Int] = [0 for _ in range(total + 1)]

  for start in order {
    continue when done[start] or state[start] != 0

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

# Operands are raw bytes so a file name that is not UTF-8 can still be opened.
proc read_input(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"

  Path.parse_bytes(name)?.read_bytes()
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = gnu.prepare_arguments(argv)
  let opts: TsortOptions = cli.applet(
    prepared.text,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      warn: {form: "-w", default: false},
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
    let extra = gnu.argument_bytes(opts.files[1], prepared.raw)

    gnu.error(f"extra operand {gnu.quote_bytes(extra, always: true)}\nTry 'tsort --help' for more information.")
    exit 1
  }

  let name = gnu.argument_bytes(opts.files.get(0) ?? "-", prepared.raw)

  guard let data = read_input(name) else { |failure|
    if gnu.errno(failure) == 21 {
      gnu.error(f"{gnu.quote_bytes(name, always: false)}: read error: Is a directory")
    } else {
      gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
    }

    exit 1
  }

  let tokens = tokenize(data)

  if tokens.len() % 2 == 1 {
    gnu.error(f"{gnu.quote_bytes(name, always: false)}: input contains an odd number of tokens")
    exit 1
  }

  var ids: Map[Str, Int] = {}
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
  var top: List[List[Int]] = [
    [succ[node][succ[node].len() - 1 - index] for index in range(succ[node].len())]
    for node in range(total)
  ]
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
      gnu.error(f"{gnu.quote_bytes(name, always: false)}: input contains a loop:")
      looped = true

      if remaining > LARGE_GRAPH {
        let cycle = find_loop(top, done, order)
        let last = cycle[-1]
        let first = cycle[0]

        for node in cycle {
          gnu.error(keys[node])
        }

        var closing_edge = 0

        while top[last][closing_edge] != first {
          closing_edge += 1
        }

        top[last] = top[last][..closing_edge] + top[last][closing_edge + 1..]
        count[first] -= 1
      } else {
        # GNU's loop search: walk the nodes in name order, chaining each unprinted
        # node that has an edge to the chain's head, until a node that is already
        # chained closes a cycle; the cycle is printed and the closing edge dropped.
        var chain = -1
        var found = false

        while ! found {
          for node in order {
            continue when count[node] == 0 or found

            if chain < 0 {
              chain = node
              continue
            }

            var edge = 0

            while edge < top[node].len() and ! found {
              if top[node][edge] == chain {
                if link[node] >= 0 {
                  var walk = chain

                  while walk >= 0 {
                    let next = link[walk]

                    gnu.error(keys[walk])

                    if walk == node {
                      count[chain] -= 1
                      top[node] = top[node][..edge] + top[node][edge + 1..]
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

              edge += 1
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
