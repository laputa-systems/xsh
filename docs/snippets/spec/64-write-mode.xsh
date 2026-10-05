const key = /tmp/xsh-spec-write-mode/host.key
const secret = "not a real key\n"

# begin example
key.write(secret, mode: 0o600)
fs.write(fp"{key}.pub", "public\n", mode: 0o644)
# end example
