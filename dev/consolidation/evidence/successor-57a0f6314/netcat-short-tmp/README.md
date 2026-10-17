# Netcat selectors with a short temporary path

The failed C profile skipped these selectors because the temporary socket path
was too long. A focused replay used the same source and protected C products
with `TMPDIR=/t`, backed by a writable scratch mount. Each exact selector
reported one pass, zero failures, and zero skips:

- `core/tests/test-nc.xsh::test_nc_unix_datagram_listener_answers_a_client`
- `core/tests/test-nc.xsh::test_nc_unix_stream_client_and_listener`

The source was `57a0f63145a4ea3b1db6cfc38b4744a5c7ab02c1`, tree
`42d48a35f740fc99a525c11d8f8da7ff41ffb106`. The test file's before and after
hashes match. The protected product hashes also match before and after:

- `xsh`: `e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b`
- `xsht`: `daa6c47667f65e8a2fe06a04ff179ee99e6286496c6d441b193889b3b9fec7fa`
- `xsh-test-helper`: `cf8005649749a43cc22aa5e40092a8f90365efc370b857fce5de2004adb2e621`

The [container inspection](container.inspect.json) preserves the image, the
effective test command and selector arguments, mounts, environment, and
resources. It records CPUs `16-19`, four CPUs, 8 GiB memory with no swap,
`OOMKilled=false`, and exit status zero. The repository and product mounts
were read-only; the `/t` scratch mount was writable. The [result](result.txt)
records source and tree pins, unchanged products, container removal, and no
remaining owned containers. The exact [test output](stdout.log),
[source hashes](source.before), [post-run source hashes](source.after),
[product hashes](products.before), and [post-run product hashes](products.after)
are retained unchanged.

No separate host-side Docker launcher or complete `docker run` argument list
was retained. The inspection record preserves the effective container setup
and the exact in-container test invocation. This focused replay resolves the
two path-length skips only; it does not change the failed C profile or grant
profile, timing, canonical, or resource credit.
