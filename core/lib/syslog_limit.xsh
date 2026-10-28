##! Bounds the cost of a syslog receiver: a token bucket per sender, a global
##! ceiling and suppression counters by reason.
##!
##! The sender table never holds more than `MAX_SENDERS` entries: a new sender
##! over the bound evicts the least recently used one, so churn of senders cannot
##! grow memory. Time is integer microseconds on the boot clock. Every drop is
##! counted under exactly one reason, so `offered` always equals `accepted` plus
##! the drops. Datagrams the kernel drops from a full socket queue are never seen
##! by the receiver; the summary says `kernel_loss=unobservable` instead of a count.

## Senders tracked at once.
export const MAX_SENDERS = 256
## Largest rate a bucket can be configured with, in messages per second.
export const MAX_RATE = 1000000
## Largest burst a bucket can be configured with, in messages.
export const MAX_BURST = 1000000
const MICROSECONDS = 1000000
const MAX_TIME = 4611686018427387903

## Why a datagram was not turned into a record.
export enum Reason { Rate, Oversize, ControlTruncated, Rights, SinkBlocked, SinkError }

## The identity the kernel reported for a datagram's sender.
export type Sender = {uid: Int, pid: Int}

## What the receive path knows about one datagram before it is parsed.
export type Datagram = {sender: Sender?, oversize: Bool, control_truncated: Bool, rights: Int}

## Message rates in messages per second and bursts in messages.
export type Config = {sender_rate: Int, sender_burst: Int, global_rate: Int, global_burst: Int}

## Drops by reason, plus the datagrams offered and accepted.
export type Counters = {
  offered: Int,
  accepted: Int,
  rate: Int,
  oversize: Int,
  control_truncated: Int,
  rights: Int,
  sink_blocked: Int,
  sink_error: Int,
  evicted_senders: Int,
}

type Slot = {allowed_at: Int, tick: Int}

## The limiter state. Treat it as opaque and thread it through `admit`.
export type Limiter = {
  config: Config,
  senders: Map[Str, Slot],
  order: Map[Int, Str],
  tick: Int,
  oldest: Int,
  global_allowed_at: Int,
  counters: Counters,
}

## A decision and the limiter after it.
export type Admission = {limiter: Limiter, accepted: Bool, reason: Reason?}

## Defaults: a sender may burst 200 messages and sustain 100 per second; the whole
## receiver may burst 4000 and sustain 2000 per second.
export const DEFAULT_CONFIG: Config = {sender_rate: 100, sender_burst: 200, global_rate: 2000, global_burst: 4000}

pure zero_counters() -> Counters {
  {offered: 0, accepted: 0, rate: 0, oversize: 0, control_truncated: 0, rights: 0, sink_blocked: 0, sink_error: 0, evicted_senders: 0}
}

pure out_of_range(name: Str, value: Int, maximum: Int) -> Error? {
  if value < 1 or value > maximum { error.failure(f"{name} must be between 1 and {maximum}") } else { null }
}

## A limiter with no senders and full buckets, or an error naming the bad field.
export pure new_limiter(config: Config) -> Result[Limiter, Error] {
  let problems = [
    out_of_range("sender_rate", config.sender_rate, MAX_RATE),
    out_of_range("global_rate", config.global_rate, MAX_RATE),
    out_of_range("sender_burst", config.sender_burst, MAX_BURST),
    out_of_range("global_burst", config.global_burst, MAX_BURST),
  ]
  for problem in problems {
    if problem != null { return Err(problem ?? error.failure("invalid limit")) }
  }
  Ok({config: config, senders: {}, order: {}, tick: 0, oldest: 1, global_allowed_at: 0, counters: zero_counters()})
}

pure clamp_time(now_us: Int) -> Int {
  if now_us < 0 { 0 } else if now_us > MAX_TIME { MAX_TIME } else { now_us }
}

pure interval_us(rate: Int) -> Int {
  MICROSECONDS / rate
}

pure window_us(rate: Int, burst: Int) -> Int {
  interval_us(rate) * burst
}

pure may_pass(allowed_at: Int, now_us: Int, rate: Int, burst: Int) -> Bool {
  allowed_at - now_us <= window_us(rate, burst) - interval_us(rate)
}

pure next_allowed(allowed_at: Int, now_us: Int, rate: Int) -> Int {
  let base = if allowed_at > now_us { allowed_at } else { now_us }
  base + interval_us(rate)
}

pure sender_key(sender: Sender?) -> Str {
  if sender == null { return "unknown" }
  let identity = sender ?? Sender(uid: 0, pid: 0)
  f"{identity.uid}:{identity.pid}"
}

pure bump(counters: Counters, reason: Reason, by: Int) -> Counters {
  match reason {
    Rate => {...counters, rate: counters.rate + by}
    Oversize => {...counters, oversize: counters.oversize + by}
    ControlTruncated => {...counters, control_truncated: counters.control_truncated + by}
    Rights => {...counters, rights: counters.rights + by}
    SinkBlocked => {...counters, sink_blocked: counters.sink_blocked + by}
    SinkError => {...counters, sink_error: counters.sink_error + by}
  }
}

pure structural_reason(datagram: Datagram) -> Reason? {
  if datagram.rights > 0 { return .Rights }
  if datagram.control_truncated { return .ControlTruncated }
  if datagram.oversize { return .Oversize }
  null
}

pure dropped(limiter: Limiter, reason: Reason) -> Admission {
  let counters = bump(limiter.counters, reason, 1)
  {limiter: {...limiter, counters: {...counters, offered: counters.offered + 1}}, accepted: false, reason: reason}
}

pure evict_oldest(limiter: Limiter) -> Limiter {
  var oldest = limiter.oldest
  while oldest <= limiter.tick and oldest not in limiter.order { oldest += 1 }
  if oldest > limiter.tick { return limiter }
  var senders = limiter.senders
  var order = limiter.order
  senders = senders.remove(order[oldest])
  order = order.remove(oldest)
  {...limiter, senders: senders, order: order, oldest: oldest + 1, counters: {...limiter.counters, evicted_senders: limiter.counters.evicted_senders + 1}}
}

pure admitted(limiter: Limiter, key: Str, known: Bool, sender_allowed_at: Int, global_allowed_at: Int) -> Admission {
  var base = limiter
  if ! known and base.senders.len() >= MAX_SENDERS { base = evict_oldest(base) }
  let tick = base.tick + 1
  var senders = base.senders
  var order = base.order
  if known {
    order = order.remove(senders[key].tick)
  }
  senders[key] = {allowed_at: sender_allowed_at, tick: tick}
  order[tick] = key
  let counters = base.counters
  {
    limiter: {...base, senders: senders, order: order, tick: tick, global_allowed_at: global_allowed_at, counters: {...counters, offered: counters.offered + 1, accepted: counters.accepted + 1}},
    accepted: true,
    reason: null,
  }
}

## Decides one datagram at `now_us` microseconds of boot time. Datagrams that
## are oversize, lost control data or carried descriptors are dropped without
## using budget; the rest spend one token from the sender's bucket and one from
## the global bucket, and a refusal by either leaves both unchanged.
export pure admit(limiter: Limiter, datagram: Datagram, now_us: Int) -> Admission {
  let structural = structural_reason(datagram)
  if structural != null { return dropped(limiter, structural ?? .Rate) }
  let now = clamp_time(now_us)
  let config = limiter.config
  let key = sender_key(datagram.sender)
  let known = key in limiter.senders
  let current = if known { limiter.senders[key].allowed_at } else { 0 }
  if ! may_pass(current, now, config.sender_rate, config.sender_burst) or ! may_pass(limiter.global_allowed_at, now, config.global_rate, config.global_burst) {
    return dropped(limiter, .Rate)
  }
  admitted(limiter, key, known, next_allowed(current, now, config.sender_rate), next_allowed(limiter.global_allowed_at, now, config.global_rate))
}

## Counts `count` datagrams dropped after admission, for a sink that could not take
## them. They were offered and accepted already, so `accepted` moves with them.
export pure sink_dropped(limiter: Limiter, reason: Reason, count: Int) -> Limiter {
  let counters = bump(limiter.counters, reason, count)
  {...limiter, counters: {...counters, accepted: counters.accepted - count}}
}

## The counters as `name=value` fields on one line.
export pure summary(limiter: Limiter) -> Str {
  let c = limiter.counters
  f"offered={c.offered} accepted={c.accepted} dropped_rate={c.rate} dropped_oversize={c.oversize} dropped_control_truncated={c.control_truncated} dropped_rights={c.rights} dropped_sink_blocked={c.sink_blocked} dropped_sink_error={c.sink_error} senders={limiter.senders.len()} evicted_senders={c.evicted_senders} kernel_loss=unobservable"
}
