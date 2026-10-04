const log = p"check.log"
# begin example
let head = run.text git log --oneline | run head -n 5 ?
let report = run.capture --text make check | run tee $log ?
# end example
