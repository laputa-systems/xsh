proc main() [error, io] {
  let encoded = json.encode({
    kernel: {
      command_line: {
        state: "observed",
        value: "  root=UUID=private  quiet  \n",
        raw_bytes_base64: null,
      },
    },
  })?
  print $encoded
}
