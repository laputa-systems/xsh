# TELNET negotiation timeout

The canonical native run from source `0deeb9c7fb79e52740c82a264754dbb29d8be693` timed out in `core/tests/test-nc.xsh::test_nc_answers_telnet_negotiation` after two minutes. It ended with 13,915 passed, one failed, and 78 skipped. The isolated selector passed in 45 ms, so that result did not reproduce the suite failure.

The test started `nc -t -n -N` with empty stdin, then sent TELNET negotiation after accepting the connection. `-N` shuts down the socket write side after stdin EOF. If that happens before the server sends TELNET bytes, `answer_telnet` cannot write its refusals. The old test kept concatenating empty reads after EOF, so the test never reached six bytes.

The [wire probe](telnet-wire-probe.py) closes stdin, waits for the peer EOF, and then sends the same TELNET bytes. With `-N`, the client returned no refusal bytes. Without `-N`, it returned `fffc01fffe03`. Both cases printed `fffd01fffb03` to stdout and exited with status 0. This fixes the test sequence without changing `core/nc.xsh` behavior.

The repaired test leaves `-N` off, bounds the reply wait at ten seconds, fails when the peer closes early, and closes the accepted descriptor on every path. The peer close then lets the client exit. The focused `core/tests/test-nc.xsh` run also covers the existing `-N` and EOF behavior tests.

The probe and focused tests used image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`, xsh `02639f0ee23780371ddc38c0c241d412adc9d79ae32b4cb722c51d74c05fb2c4`, and xsht `5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836`.

The first invocation used a different root-level pair: xsh `1a4b3857c6bd0b989cf0f159a6745ddf6a846b6d6467a282f96e8737d1196a7b` and xsht `8e9c3f5aa24e5933daf74294ec54772623ab819594244a02646a05c8fbd4429f`. It passed in 51 ms and is withdrawn with zero qualification.

Commands inside the container:

```text
/usr/bin/python3 dev/consolidation/evidence/native-nc-repair/telnet-wire-probe.py --xsh /artifact/xsh --script core/nc.xsh
/artifact/xsht test --exact core/tests/test-nc.xsh::test_nc_answers_telnet_negotiation
/artifact/xsht test core/tests/test-nc.xsh
```

The [wire output](wire-probe.stdout), [exact selector output](exact-selector.stdout), and [focused file output](focused-file.stdout) record the passing runs. All three commands exited 0 with empty stderr.

The host's `/usr/bin/nc` is BusyBox 1.38.0, so the probe makes no claim against OpenBSD netcat.
