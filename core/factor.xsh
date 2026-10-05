#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: factor [OPTION] [NUMBER]...
  or:  factor OPTION
Print the prime factors of each specified integer NUMBER.  If none
are specified on the command line, read them from standard input.

  -h, --exponents   print repeated factors in form p^e unless e is 1
      --help        display this help and exit
      --version     output version information and exit
"""

type FactorOptions = {
  exponents: Bool,
  help: Bool,
  version: Bool,
  numbers: List[Str],
}

# Magnitudes are little-endian base 10^9 limbs without high zero limbs; zero
# is the empty list.
const BASE = 1000000000

pure trim_limbs(parts: List[Int]) -> List[Int] {
  var end = parts.len()

  while end > 0 and parts[end - 1] == 0 {
    end -= 1
  }

  if end == parts.len() { parts } else { parts[..end] }
}

pure big_from(text: Str) -> List[Int] {
  var out: List[Int] = []
  var end = text.byte_len()

  while end > 0 {
    let start = if end > 9 { end - 9 } else { 0 }
    out += [text.byte_slice(start, length: end - start).parse_int() ?? 0]
    end = start
  }

  trim_limbs(out)
}

pure big_text(parts: List[Int]) -> Str {
  return "0" when parts.len() == 0

  var out = f"{parts[parts.len() - 1]}"
  var at = parts.len() - 2

  while at >= 0 {
    let piece = f"{parts[at]}"
    out = out + "000000000".byte_slice(0, length: 9 - piece.byte_len()) + piece
    at -= 1
  }

  out
}

pure big_cmp(a: List[Int], b: List[Int]) -> Int {
  return -1 when a.len() < b.len()
  return 1 when a.len() > b.len()

  var at = a.len() - 1

  while at >= 0 {
    return -1 when a[at] < b[at]
    return 1 when a[at] > b[at]

    at -= 1
  }

  0
}

pure big_add(a: List[Int], b: List[Int]) -> List[Int] {
  var out: List[Int] = []
  var carry = 0
  var at = 0
  let count = if a.len() > b.len() { a.len() } else { b.len() }

  while at < count or carry > 0 {
    let sum = (if at < a.len() { a[at] } else { 0 }) + (if at < b.len() { b[at] } else { 0 }) + carry
    out += [sum % BASE]
    carry = sum / BASE
    at += 1
  }

  out
}

# A - B for A >= B.
pure big_sub(a: List[Int], b: List[Int]) -> List[Int] {
  var out: List[Int] = []
  var borrow = 0
  var at = 0

  while at < a.len() {
    var gap = a[at] - (if at < b.len() { b[at] } else { 0 }) - borrow

    if gap < 0 {
      gap += BASE
      borrow = 1
    } else {
      borrow = 0
    }

    out += [gap]
    at += 1
  }

  trim_limbs(out)
}

pure big_mul_small(a: List[Int], factor: Int) -> List[Int] {
  return [] when factor == 0 or a.len() == 0

  var out: List[Int] = []
  var carry = 0

  for part in a {
    let product = part * factor + carry
    out += [product % BASE]
    carry = product / BASE
  }

  while carry > 0 {
    out += [carry % BASE]
    carry = carry / BASE
  }

  out
}

pure big_mul(a: List[Int], b: List[Int]) -> List[Int] {
  return [] when a.len() == 0 or b.len() == 0

  var out: List[Int] = [0 for slot in range(a.len() + b.len())]

  for i in range(a.len()) {
    var carry = 0
    let factor = a[i]

    if factor != 0 {
      for j in range(b.len()) {
        let current = out[i + j] + factor * b[j] + carry
        out[i + j] = current % BASE
        carry = current / BASE
      }

      out[i + b.len()] += carry
    }
  }

  trim_limbs(out)
}

type Division = {quotient: List[Int], remainder: List[Int]}

# A / B and A % B for nonzero B.
pure big_divmod(a: List[Int], b: List[Int]) -> Division {
  return {quotient: [], remainder: a} when big_cmp(a, b) < 0

  if b.len() == 1 {
    var quotient: List[Int] = [0 for slot in range(a.len())]
    var rest = 0
    var at = a.len() - 1

    while at >= 0 {
      let current = rest * BASE + a[at]
      quotient[at] = current / b[0]
      rest = current % b[0]
      at -= 1
    }

    return {quotient: trim_limbs(quotient), remainder: if rest == 0 { [] } else { [rest] }}
  }

  let width = b.len()
  let divisor_top = b[width - 1].float() * 1000000000.0 + b[width - 2].float()
  var quotient: List[Int] = [0 for slot in range(a.len())]
  var rest: List[Int] = []
  var at = a.len() - 1

  while at >= 0 {
    rest = trim_limbs([a[at]] + rest)

    if big_cmp(rest, b) >= 0 {
      let top = if rest.len() == width {
        rest[width - 1].float() * 1000000000.0 + rest[width - 2].float()
      } else {
        (rest[width].float() * 1000000000.0 + rest[width - 1].float()) * 1000000000.0 + rest[width - 2].float()
      }
      var guess = (top / divisor_top).floor() ?? 0

      guess = if guess >= BASE { BASE - 1 } else { guess }

      var product = big_mul_small(b, guess)

      while big_cmp(product, rest) > 0 {
        guess -= 1
        product = big_sub(product, b)
      }

      var left = big_sub(rest, product)

      while big_cmp(left, b) >= 0 {
        guess += 1
        left = big_sub(left, b)
      }

      quotient[at] = guess
      rest = left
    }

    at -= 1
  }

  {quotient: trim_limbs(quotient), remainder: rest}
}


pure big_from_int(value: Int) -> List[Int] {
  return [] when value == 0
  return [value] when value < BASE

  [value % BASE, value / BASE]
}

# A value below 10^18.
pure big_to_int(a: List[Int]) -> Int {
  (if a.len() > 0 { a[0] } else { 0 }) + (if a.len() > 1 { a[1] * BASE } else { 0 })
}

# A mod D and A / D for a divisor below 9 * 10^9.
pure big_div_small(a: List[Int], divisor: Int) -> Division {
  var quotient: List[Int] = [0 for slot in range(a.len())]
  var rest = 0
  var at = a.len() - 1

  while at >= 0 {
    let current = rest * BASE + a[at]
    quotient[at] = current / divisor
    rest = current % divisor
    at -= 1
  }

  {quotient: trim_limbs(quotient), remainder: if rest == 0 { [] } else { [rest] }}
}

pure big_float(a: List[Int]) -> Float {
  var total = 0.0
  var at = a.len() - 1

  while at >= 0 {
    total = total * 1000000000.0 + a[at].float()
    at -= 1
  }

  total
}

pure big_is_one(a: List[Int]) -> Bool {
  a.len() == 1 and a[0] == 1
}

pure big_pow(base: List[Int], exponent: Int) -> List[Int] {
  var out = [1]

  repeat exponent times {
    out = big_mul(out, base)
  }

  out
}

pure gcd_int(a: Int, b: Int) -> Int {
  var x = a
  var y = b

  while y != 0 {
    let rest = x % y
    x = y
    y = rest
  }

  x
}

pure primes_below(limit: Int) -> List[Int] {
  var composite: List[Bool] = [false for slot in range(limit)]
  var found: List[Int] = []

  for candidate in range(2, limit) {
    if ! composite[candidate] {
      found += [candidate]

      var multiple = candidate * candidate

      while multiple < limit {
        composite[multiple] = true
        multiple += candidate
      }
    }
  }

  found
}

# Multiplication modulo `m` below 2^62 without overflow: the multiplier is
# taken in chunks of `shift` (a power of two) so each partial product fits.
type Mod = {m: Int, shift: Int, top: Int}

pure mod_info(m: Int) -> Mod {
  return {m: m, shift: 0, top: 0} when m < 2147483648

  var bits = 0
  var power = 1

  while power <= m {
    power = power * 2
    bits += 1
  }

  var shift = 1

  repeat 62 - bits times {
    shift = shift * 2
  }

  var top = 1

  while top <= m / shift {
    top = top * shift
  }

  {m: m, shift: shift, top: top}
}

pure mulmod(a: Int, b: Int, info: Mod) -> Int {
  return a * b % info.m when info.shift == 0

  var result = 0
  var rest = b
  var scale = info.top

  while scale > 0 {
    let digit = rest / scale
    rest = rest - digit * scale
    result = (result * info.shift % info.m + a * digit % info.m) % info.m
    scale = scale / info.shift
  }

  result
}

pure powmod(base: Int, exponent: Int, info: Mod) -> Int {
  var result = 1
  var factor = base % info.m
  var left = exponent

  while left > 0 {
    if left % 2 == 1 {
      result = mulmod(result, factor, info)
    }

    factor = mulmod(factor, factor, info)
    left = left / 2
  }

  result
}

# Miller-Rabin with base sets that are exact for every number below 3.4 * 10^14
# (the first seven primes are, above 3.4 * 10^14 up to 2^64 the set of seven
# bases below is, which covers every number below 10^18).
pure is_prime_int(n: Int) -> Bool {
  return n >= 2 when n < 4
  return false when n % 2 == 0

  let info = mod_info(n)
  var odd = n - 1
  var twos = 0

  while odd % 2 == 0 {
    odd = odd / 2
    twos += 1
  }

  let bases = if n < 4759123141 {
    [2, 7, 61]
  } else if n < 1122004669633 {
    [2, 13, 23, 1662803]
  } else if n < 2152302898747 {
    [2, 3, 5, 7, 11]
  } else if n < 3474749660383 {
    [2, 3, 5, 7, 11, 13]
  } else {
    [2, 3, 5, 7, 11, 13, 17]
  }
  let wide = if n < 341550071728321 { bases } else { [2, 325, 9375, 28178, 450775, 9780504, 1795265022] }

  for base in wide {
    if n % base == 0 {
      return n == base
    }

    var x = powmod(base % n, odd, info)

    if x != 0 and x != 1 and x != n - 1 {
      var witness = true

      repeat twos - 1 times {
        x = mulmod(x, x, info)

        if x == n - 1 {
          witness = false
          break
        }
      }

      if witness {
        return false
      }
    }
  }

  true
}

pure rho_step(x: Int, c: Int, info: Mod) -> Int {
  (mulmod(x, x, info) + c) % info.m
}

# Pollard's rho (Brent's cycle detection, batched gcds) for an odd composite
# below 2^62; 0 when this constant finds nothing within `limit` steps.
pure rho_int(n: Int, c: Int, limit: Int) -> Int {
  let info = mod_info(n)
  var y = 2
  var x = 2
  var ys = 2
  var q = 1
  var g = 1
  var r = 1
  var steps = 0

  while g == 1 and steps < limit {
    x = y

    repeat r times {
      y = rho_step(y, c, info)
    }

    var k = 0

    while k < r and g == 1 {
      ys = y

      let count = if r - k < 32 { r - k } else { 32 }

      repeat count times {
        y = rho_step(y, c, info)
        q = mulmod(q, if x > y { x - y } else { y - x }, info)
      }

      g = gcd_int(q, n)
      k += 32
      steps += count
    }

    r = r * 2
  }

  return 0 when g == 1

  if g == n {
    loop {
      ys = rho_step(ys, c, info)
      g = gcd_int(if x > ys { x - ys } else { ys - x }, n)

      break when g != 1
    }
  }

  if g == n { 0 } else { g }
}

pure mulmod_big(a: List[Int], b: List[Int], m: List[Int]) -> List[Int] {
  big_divmod(big_mul(a, b), m).remainder
}

pure powmod_big(base: List[Int], exponent: List[Int], m: List[Int]) -> List[Int] {
  var result = [1]
  var factor = big_divmod(base, m).remainder
  var left = exponent

  while left.len() > 0 {
    let half = big_div_small(left, 2)

    if half.remainder.len() > 0 {
      result = mulmod_big(result, factor, m)
    }

    factor = mulmod_big(factor, factor, m)
    left = half.quotient
  }

  result
}

# Miller-Rabin for a number with 19 digits or more: the bases are exact below
# 3.3 * 10^24 and beyond that the error is below 4^-20.
pure is_prime_big(n: List[Int]) -> Bool {
  let below = big_sub(n, [1])
  var odd = below
  var twos = 0

  while odd.len() > 0 and odd[0] % 2 == 0 {
    odd = big_div_small(odd, 2).quotient
    twos += 1
  }

  let size = big_float(n)
  let bases = if size < 18446744073709551616.0 {
    [2, 325, 9375, 28178, 450775, 9780504, 1795265022]
  } else if size < 3.3e24 {
    [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37]
  } else {
    [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71]
  }

  for base in bases {
    var x = powmod_big(big_divmod(big_from_int(base), n).remainder, odd, n)

    if x.len() > 0 and ! big_is_one(x) and big_cmp(x, below) != 0 {
      var witness = true

      repeat twos - 1 times {
        x = mulmod_big(x, x, n)

        if big_cmp(x, below) == 0 {
          witness = false
          break
        }
      }

      if witness {
        return false
      }
    }
  }

  true
}

pure gcd_big(a: List[Int], b: List[Int]) -> List[Int] {
  var x = a
  var y = b

  while y.len() > 0 {
    let rest = big_divmod(x, y).remainder
    x = y
    y = rest
  }

  x
}

pure rho_step_big(x: List[Int], c: Int, n: List[Int]) -> List[Int] {
  let next = big_add(mulmod_big(x, x, n), [c])

  if big_cmp(next, n) >= 0 { big_sub(next, n) } else { next }
}

pure distance(a: List[Int], b: List[Int]) -> List[Int] {
  if big_cmp(a, b) > 0 { big_sub(a, b) } else { big_sub(b, a) }
}

# Pollard's rho for a number of any size (Brent's cycle detection, batched
# gcds); the result is one factor as limbs, or [] when this constant finds
# nothing within `limit` steps.
pure rho_big(n: List[Int], c: Int, limit: Int) -> List[Int] {
  var y = [2]
  var x = [2]
  var ys = [2]
  var q = [1]
  var g = [1]
  var r = 1
  var steps = 0

  while big_is_one(g) and steps < limit {
    x = y

    repeat r times {
      y = rho_step_big(y, c, n)
    }

    var k = 0

    while k < r and big_is_one(g) {
      ys = y

      let count = if r - k < 24 { r - k } else { 24 }

      repeat count times {
        y = rho_step_big(y, c, n)
        q = mulmod_big(q, distance(x, y), n)
      }

      g = gcd_big(q, n)
      k += 24
      steps += count
    }

    r = r * 2
  }

  return [] when big_is_one(g)

  if big_cmp(g, n) == 0 {
    loop {
      ys = rho_step_big(ys, c, n)
      g = gcd_big(distance(x, ys), n)

      break when ! big_is_one(g)
    }
  }

  if big_cmp(g, n) == 0 { [] } else { g }
}

pure isqrt_int(value: Int) -> Int {
  var root = value.float().sqrt().floor() ?? 0

  while root * root > value {
    root -= 1
  }

  while (root + 1) * (root + 1) <= value {
    root += 1
  }

  root
}

pure isqrt_big(value: List[Int]) -> Int {
  var root = big_float(value).sqrt().floor() ?? 0

  while big_cmp(big_mul(big_from_int(root), big_from_int(root)), value) > 0 {
    root -= 1
  }

  while big_cmp(big_mul(big_from_int(root + 1), big_from_int(root + 1)), value) <= 0 {
    root += 1
  }

  root
}

const SQUFOF_MULTIPLIERS = [1, 3, 5, 7, 11, 15, 21, 33, 35, 55, 77, 105, 165, 231, 385, 1155]

# Shanks's square forms factorization of an odd composite below about 2^75:
# the recurrence stays near sqrt(k n), so it fits an Int. Returns a factor or 0.
pure squfof(n: List[Int]) -> Int {
  for multiplier in SQUFOF_MULTIPLIERS {
    let target = big_mul_small(n, multiplier)
    let start = isqrt_big(target)

    if big_cmp(big_mul(big_from_int(start), big_from_int(start)), target) == 0 {
      continue
    }

    let limit = 6 * isqrt_int(2 * start) + 100
    var p = start
    var previous_q = 1
    var q = big_to_int(big_sub(target, big_mul(big_from_int(start), big_from_int(start))))
    var root = 0
    var found = false
    var round = 2

    # Each pass takes two steps and tests for a perfect square after the
    # first, which is the even-numbered form; a perfect square has an exact
    # float square root, so no adjustment is needed.
    while round < limit {
      var step = (start + p) / q
      var next_p = step * q - p
      var next_q = previous_q + step * (p - next_p)

      previous_q = q
      p = next_p
      q = next_q
      root = q.float().sqrt().floor() ?? 0

      if root * root == q {
        found = true
        break
      }

      step = (start + p) / q
      next_p = step * q - p
      next_q = previous_q + step * (p - next_p)
      previous_q = q
      p = next_p
      q = next_q
      round += 2
    }

    if found {
      let step = (start - p) / root

      p = step * root + p
      previous_q = root

      let rest = big_sub(target, big_mul(big_from_int(p), big_from_int(p)))

      q = big_to_int(big_div_small_or_big(rest, previous_q))

      loop {
        let leap = (start + p) / q
        let next_p = leap * q - p
        let next_q = previous_q + leap * (p - next_p)

        if next_p == p {
          break
        }

        previous_q = q
        p = next_p
        q = next_q
      }

      let remainder = big_to_int(big_divmod(n, big_from_int(p)).remainder)
      let g = gcd_int(p, remainder)

      if g != 1 and big_cmp(big_from_int(g), n) != 0 {
        return g
      }
    }
  }

  0
}

pure big_div_small_or_big(a: List[Int], divisor: Int) -> List[Int] {
  big_divmod(a, big_from_int(divisor)).quotient
}

# A root and exponent with root^exponent = n for the smallest exponent
# (at least 2) that has one; an exponent of 1 means n is not a perfect power.
type Power = {root: List[Int], exponent: Int}

pure perfect_power(n: List[Int]) -> Power {
  let size = big_float(n)

  return {root: n, exponent: 1} when size > 1.0e300

  var exponent = 2
  let logarithm = size.ln()

  while exponent <= 64 and logarithm / exponent.float() > 0.5 {
    let estimate = (logarithm / exponent.float()).exp().round() ?? 0

    for guess in [estimate - 1, estimate, estimate + 1] {
      if guess > 1 {
        let root = big_from_int(guess)

        if big_cmp(big_pow(root, exponent), n) == 0 {
          return {root: root, exponent: exponent}
        }
      }
    }

    exponent += 1
  }

  {root: n, exponent: 1}
}

# The factorization of one number.
type Factors = {primes: List[Str], incomplete: Bool}

# What is left after the small primes are divided out. `proven` says the rest
# is itself prime: no prime up to its square root divides it.
type Stripped = {primes: List[Str], rest: List[Int], proven: Bool}

pure strip_small_primes(n: List[Int], small: List[Int]) -> Stripped {
  var primes: List[Str] = []
  var rest = n

  if rest.len() <= 2 {
    var value = big_to_int(rest)

    for p in small {
      if p * p > value and value > 1 {
        return {primes: primes, rest: big_from_int(value), proven: true}
      }

      while value > 1 and value % p == 0 {
        primes += [f"{p}"]
        value = value / p
      }
    }

    return {primes: primes, rest: big_from_int(value), proven: value < 4194304 and value > 1}
  }

  for p in small {
    var division = big_div_small(rest, p)

    while division.remainder.len() == 0 {
      primes += [f"{p}"]
      rest = division.quotient
      division = big_div_small(rest, p)
    }
  }

  {primes: primes, rest: rest, proven: rest.len() <= 1 and big_to_int(rest) < 4194304 and rest.len() > 0 and big_to_int(rest) > 1}
}

# One nontrivial factor of an odd composite with no factor below 2048, as
# limbs, or [] when none was found. Small cofactors take Pollard's rho (a long
# run below 2^50, a short one above) and anything below about 2^72 then takes
# square forms factorization, whose cost grows with the fourth root of n.
pure find_factor(n: List[Int]) -> List[Int] {
  if n.len() <= 2 {
    let value = big_to_int(n)

    if value < 1125899906842624 {
      var c = 1

      while c < 20 {
        let found = rho_int(value, c, 400000)

        return big_from_int(found) when found != 0

        c += 1
      }
    } else {
      let found = rho_int(value, 1, 1500)

      return big_from_int(found) when found != 0
    }
  }

  if n.len() <= 3 and big_float(n) < 4.7e21 {
    let found = squfof(n)

    return big_from_int(found) when found != 0
  }

  var c = 1

  while c < 12 {
    let found = rho_big(n, c, 2000000)

    return found when found.len() > 0

    c += 1
  }

  []
}

pure is_prime(n: List[Int]) -> Bool {
  return is_prime_int(big_to_int(n)) when n.len() <= 2

  is_prime_big(n)
}

pure factorize(n: List[Int], small: List[Int]) -> Factors {
  let stripped = strip_small_primes(n, small)
  var primes = stripped.primes
  var work: List[List[Int]] = []

  if stripped.proven {
    primes += [big_text(stripped.rest)]
  } else if stripped.rest.len() > 0 and ! big_is_one(stripped.rest) {
    work = [stripped.rest]
  }

  var incomplete = false

  while work.len() > 0 {
    let current = work[work.len() - 1]

    work = work[..work.len() - 1]

    if is_prime(current) {
      primes += [big_text(current)]
    } else {
      let power = perfect_power(current)

      if power.exponent > 1 {
        repeat power.exponent times {
          work += [power.root]
        }
      } else {
        let factor = find_factor(current)

        if factor.len() == 0 {
          incomplete = true
          primes += [big_text(current)]
        } else {
          work += [factor, big_divmod(current, factor).quotient]
        }
      }
    }
  }

  {primes: primes, incomplete: incomplete}
}

# Numeric order of digit strings: shorter is smaller, equal lengths compare as text.
pure before(a: Str, b: Str) -> Bool {
  if a.byte_len() != b.byte_len() { a.byte_len() < b.byte_len() } else { a < b }
}

pure sorted_primes(primes: List[Str]) -> List[Str] {
  var out: List[Str] = []

  for item in primes {
    var at = out.len()

    while at > 0 and before(item, out[at - 1]) {
      at -= 1
    }

    out = out[..at] + [item] + out[at..]
  }

  out
}

pure render(text: Str, primes: List[Str], exponents: Bool) -> Str {
  var line = f"{text}:"
  var at = 0

  while at < primes.len() {
    var count = 1

    while at + count < primes.len() and primes[at + count] == primes[at] {
      count += 1
    }

    if exponents and count > 1 {
      line += f" {primes[at]}^{count}"
    } else {
      repeat count times {
        line += f" {primes[at]}"
      }
    }

    at += count
  }

  line
}

pure continuation(data: Bytes, at: Int) -> Bool {
  let byte = data.byte_at(at) ?? 0
  byte >= 128 and byte < 192
}

pure within(data: Bytes, at: Int, low: Int, high: Int) -> Bool {
  let byte = data.byte_at(at) ?? 0
  byte >= low and byte <= high
}

pure sequence_width(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0

  return 1 when lead < 128
  return 2 when lead >= 194 and lead <= 223 and continuation(data, at + 1)
  return 3 when lead == 224 and within(data, at + 1, 160, 191) and continuation(data, at + 2)
  return 3 when ((lead >= 225 and lead <= 236) or lead == 238 or lead == 239) and continuation(data, at + 1) and continuation(data, at + 2)
  return 3 when lead == 237 and within(data, at + 1, 128, 159) and continuation(data, at + 2)
  return 4 when lead == 240 and within(data, at + 1, 144, 191) and continuation(data, at + 2) and continuation(data, at + 3)
  return 4 when lead >= 241 and lead <= 243 and continuation(data, at + 1) and continuation(data, at + 2) and continuation(data, at + 3)
  return 4 when lead == 244 and within(data, at + 1, 128, 143) and continuation(data, at + 2) and continuation(data, at + 3)

  0
}

# A token for a message: valid text as is, other bytes as octal escapes.
pure shown(raw: Bytes) -> Str {
  var out = ""
  var at = 0

  while at < raw.len() {
    let width = sequence_width(raw, at)

    if width > 0 {
      out += raw[at..at + width].utf8() ?? ""
      at += width
    } else {
      let byte = raw.byte_at(at) ?? 0
      out += f"\\{byte / 64}{byte / 8 % 8}{byte % 8}"
      at += 1
    }
  }

  out
}

# The numbers on one input line: blanks, tabs, and NULs separate them, and a
# NUL also discards the chunk after it, as GNU does.
pure tokens_of(line: Bytes) -> List[Bytes] {
  var out: List[Bytes] = []
  var display = true
  var previous = 0
  let total = line.len()

  for index in range(total + 1) {
    let byte = if index < total { line.byte_at(index) ?? 1 } else { 1 }
    let end = index == total
    let has_null = ! end and byte == 0

    if end or byte == 32 or byte == 9 or has_null {
      if display and (previous != index or has_null) {
        out += [line[previous..index]]
      }

      display = ! has_null
      previous = index + 1
    }
  }

  out
}

# One result per token: the output line, or the message that rejects it.
type Outcome = {line: Str, error: Str}

pure outcome_for(raw: Bytes, exponents: Bool, small: List[Int]) -> Outcome {
  let text = raw.utf8() ?? ""
  let body = if text.starts_with("+") { text.byte_slice(1) } else { text }

  if text == "" or ! rx"^[0-9]+$".matches(body) {
    return {line: "", error: f"'{shown(raw)}' is not a valid positive integer"}
  }

  let n = big_from(body)
  let canonical = big_text(n)

  if n.len() == 0 or big_is_one(n) {
    return {line: f"{canonical}:", error: ""}
  }

  let found = factorize(n, small)

  if found.incomplete {
    return {line: "", error: f"cannot factor {canonical}: no factor found within the effort limit"}
  }

  {line: render(canonical, sorted_primes(found.primes), exponents), error: ""}
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: FactorOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      exponents: {form: "-h --exponents", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      numbers: {form: "...NUMBER"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("factor")
    return
  }

  let small = primes_below(2048)
  var tokens: List[Bytes] = []

  if opts.numbers.len() > 0 {
    tokens = [bytes.from_text(item.trim()) for item in opts.numbers]
  } else {
    var data = b""

    match io.stdin_bytes() {
      Ok(read) => data = read
      Err(failure) => {
        gnu.error(f"error reading input: {gnu.strerror(failure)}")
        exit 1
      }
    }

    var start = 0

    for stop in tio.line_ends(data, false) {
      tokens += tokens_of(data[start..stop - 1])
      start = stop
    }

    if start < data.len() {
      tokens += tokens_of(data[start..data.len()])
    }
  }

  var out = ""
  var failed = false

  for raw in tokens {
    let result = outcome_for(raw, opts.exponents, small)

    if result.error != "" {
      gnu.write_text(out)
      out = ""
      gnu.error(result.error)
      failed = true
    } else {
      out += result.line + "\n"
    }
  }

  gnu.write_text(out)

  if failed {
    exit 1
  }
}
