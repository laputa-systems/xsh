const log_dir = /var/log/nginx
const today = p"access.log"
let rotated = fp"{log_dir}/{today}.1"

print $rotated
print f"name={rotated.name()} ext={rotated.ext()} parent={rotated.parent()}"
print f"as gzip: {rotated.with_ext("gz")}"
