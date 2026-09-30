proc silent(value) { value }
pure bad(value) { silent(value) }
print ${bad(1)}
