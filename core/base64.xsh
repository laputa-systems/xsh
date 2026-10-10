#!/bin/xsh
use lib.bytes_enc

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  bytes_enc.execute(argv, "base64")
}
