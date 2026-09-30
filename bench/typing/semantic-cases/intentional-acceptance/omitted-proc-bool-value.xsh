proc boolean() [error] { false }
proc early() [error] { return false }
print ${boolean()} ${early()}
