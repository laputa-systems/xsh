#!/bin/xsh
use lib.gnu

const USAGE = """Usage: nproc [OPTION]...
Print the number of processing units available to the current process,
which may be less than the number of online processors.
If the 'OMP_NUM_THREADS' or 'OMP_THREAD_LIMIT' environment variables are set,
then they will determine the minimum and maximum returned value respectively.

      --all      print the number of installed processors,
                   disregarding any OpenMP environment variables, or CPU quotas.
      --ignore=N  if possible, exclude N processing units.
                   The result is guaranteed to be at least 1.
      --help     display this help and exit
      --version  output version information and exit
"""

type NprocOptions = {all: Bool, ignore: Str?, help: Bool, version: Bool}

# A count as `unsigned long` holds it. Values past the Int range clamp to the
# 64-bit maximum, kept as text because XSH integers are signed 63-bit.
const ULONG_MAX = "18446744073709551615"

type Count = {saturated: Bool, value: Int}

type Leading = {count: Count, rest: Str}

pure is_space(text: Str) -> Bool {
  text == " " or text == "\t" or text == "\n" or text == "\u{b}" or text == "\u{c}" or text == "\r"
}

pure count_of(digits: Str) -> Count {
  return {saturated: true, value: 0} when digits.byte_len() > 18

  {saturated: false, value: digits.parse_int() ?? 0}
}

# The leading unsigned decimal of TEXT after blanks, and what follows it, or
# null when TEXT does not start with a digit (strtoul also accepts a sign).
pure leading_count(text: Str) -> Leading? {
  var start = 0

  while start < text.byte_len() and is_space(text.byte_slice(start, length: 1)) {
    start += 1
  }

  let found = rx"^[0-9]+".captures(text.byte_slice(start))

  return null when found.is_empty()

  {count: count_of(found[0]), rest: text.byte_slice(start + found[0].byte_len())}
}

# One OMP_NUM_THREADS / OMP_THREAD_LIMIT value: a positive number optionally
# followed by a comma and ignored list items; anything else is unset.
pure omp_value(text: Str) -> Count? {
  let parsed = leading_count(text)

  return null when parsed == null

  let rest = parsed.rest.trim()

  return null when rest != "" and ! rest.starts_with(",")
  return null when ! parsed.count.saturated and parsed.count.value == 0

  parsed.count
}

pure smaller(left: Count, right: Count) -> Count {
  return right when left.saturated
  return left when right.saturated

  if left.value < right.value { left } else { right }
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: NprocOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      all: {form: "--all", default: false},
      ignore: {form: "--ignore N"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("nproc")
    return
  }

  var ignore = {saturated: false, value: 0}

  let given = opts.ignore

  if given != null {
    let parsed = leading_count(given)

    if parsed == null or parsed.rest != "" {
      gnu.error(f"invalid number: {gnu.quote_value(given)}")
      exit 1
    }

    ignore = parsed.count
  }

  let processors = {saturated: false, value: cpu.count()}
  var count = processors

  if ! opts.all {
    var limit: Count? = null

    if let Ok(text) = env.get("OMP_THREAD_LIMIT") {
      limit = omp_value(text)
    }

    var requested: Count? = null

    if let Ok(text) = env.get("OMP_NUM_THREADS") {
      requested = omp_value(text)
    }

    if requested != null {
      count = if limit != null { smaller(requested, limit) } else { requested }
    } else if limit != null {
      count = smaller(processors, limit)
    }
  }

  if ignore.saturated or (! count.saturated and ignore.value >= count.value) {
    count = {saturated: false, value: 1}
  } else if ! count.saturated {
    count = {saturated: false, value: count.value - ignore.value}
  }

  gnu.write_text(f"{if count.saturated { ULONG_MAX } else { f"{count.value}" }}\n")
}
