let home=e"HOME" ?? "/"
e"XSH_FMT_GREETING"   =   "hi"
e"XSH_FMT_PORT"=8080
let both = [ e"XSH_FMT_GREETING"?, e"XSH_FMT_PORT" ?? "" ]
print f"{e"XSH_FMT_GREETING"?} {home} {both.join(",")}"
run printenv XSH_FMT_GREETING
