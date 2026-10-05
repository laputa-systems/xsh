#!/bin/xsh
use lib.bytes_enc

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  bytes_enc.execute(argv, "base32")
}
