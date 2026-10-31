# Native SSH

`ssh` is the native client surface. It opens one TCP connection, negotiates
strict key exchange, verifies a mandatory `SHA256:` host-key pin, authenticates
with an unencrypted Ed25519 OpenSSH key, and runs commands on that connection.

```xsh
let options = {
  host: "web-1.example",
  port: 22,
  user: "deploy",
  key: p"/etc/xinfra/id_ed25519",
  host_key: "SHA256:...",
  timeout_ms: 10000,
}

with session = ssh.connect(options)? {
  let result = ssh.exec(session, "printf 'hello'", {})?
  print f"{result.exit_code} {result.stdout.utf8()?}"
}
```

The returned session is an owned capability. Its scope closes it after the
body's defers; use `ssh.close(session)` for early release. Concurrent use of one
session is rejected with `ssh-session-busy`, never serialized silently.

`ssh.exec` options: `timeout_ms`, `max_output`, `stdin` (`Bytes`),
`stdin_path`, `stdout_path`, and `stderr_path`. Supply both `stdin` and
`stdin_path` is an error. Path options stream file contents without loading
them into the controller. The result carries `exit_code`, `signal`, `missing`,
`stdout`, `stderr`, and `certainty`; a closed channel without an exit status has
`missing = true`, never exit zero.

The transport uses Graviola and the vendored OxiSH packet and key-exchange
code. It does not invoke the system `ssh`, read `~/.ssh`, write `known_hosts`,
prompt for passwords, allocate a PTY, forward ports, or use SFTP. Host keys are
pinned by fingerprint only; agent, password, keyboard-interactive, certificate
host keys, and non-Ed25519 authentication are deferred with explicit errors.

## Qualification (W6)

The macOS arm64 lane passed the complete `xsh-ssh` suite, the seven native
`tests/xsh/ssh-` tests, the example against `xinfra-a`, and
`scripts/check-ssh-crypto.sh`. The Linux x86_64 lane and
`cargo dev gate crates/xsh-ssh` were not executed: the owner's
`ubuntu@oracle` host is unavailable and the local OrbStack Docker runtime hangs
on `docker run --rm alpine:3.23 true`. Both remain unqualified and do not block
macOS local completion.

On this host a clean release build of `xsh` and `xsht` took 98.92 s; the `dist`
profile took 303.22 s. Startup (`xsh --startup`) measured 0.02 s. Stripped
`dist` sizes were 23,704,272 bytes for `xsh` and 32,515,056 bytes for `xsht`,
which are 8.6% and 12.3% below the recorded 25,945,608 and 37,090,696 byte
baselines; both pass the 5% size gate.
