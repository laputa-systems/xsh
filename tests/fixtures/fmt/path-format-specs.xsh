print f"{p"/lib":<8}|{p"../bin":>8}|{p"./x.txt":010}"
print fp"{p"/lib":<8}/{p"../bin":>8}/{p"./x.txt":010}"
let absent: Path? = null
print f"{absent ?? p"/lib":<8}|{p"/lib" == p"/bin":>8}"
