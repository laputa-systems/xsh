##! Test support: the data types the scripted DHCP server, the scenario driver
##! and the test helpers share. A script is a list of rules, a scenario is a
##! list of steps, and both travel between processes as JSON.

## One reply a rule sends. `xid` and `chaddr` are "same" (copy the request) or
## "other" (an unrelated transaction or client); `dest` is "broadcast",
## "source" (the request's source address), "ciaddr" or "yiaddr"; `corrupt` is
## "none", "truncate", "cookie", "option_length", "no_end" or "garbage".
export type Reply = {
  kind: Str,
  address: Str,
  lease: Int,
  t1: Int?,
  t2: Int?,
  mask: Str,
  routers: List[Str],
  dns: List[Str],
  domain: Str?,
  server_id: Str,
  xid: Str,
  chaddr: Str,
  dest: Str,
  corrupt: Str,
  repeat: Int,
  delay_ms: Int,
}

## A rule answers the `nth` request of kind `on` (every one when `nth` is
## null, every one from `from` on when that is set) with `replies`; an empty
## list is silence.
export type Rule = {on: Str, nth: Int?, from: Int?, replies: List[Reply]}

## The whole script: rules checked in order, first match wins, no match is
## silence.
export type Script = {rules: List[Rule]}

## One step. `do` names the action; the other fields apply to some actions.
export type Step = {
  do: Str,
  name: Str?,
  file: Str?,
  ms: Int?,
  args: List[Str],
  text: Str?,
  signal: Str?,
  side: Str?,
}

## A whole scenario.
export type Scenario = {xsh: Str, core: Str, dir: Str, steps: List[Step]}
