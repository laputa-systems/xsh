proc external_wait() [process, error] -> Result[Int] {
  let text = run.text sh -c "sleep 0.2; printf done"
  text.byte_len()
}
print (external_wait()?)
