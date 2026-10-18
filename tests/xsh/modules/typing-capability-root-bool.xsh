use typing-capability-identity as identity

let value: Bool = identity.public_identity(false)
let name: Bool = identity.public_name({name: false, enabled: true})
assert value == false
assert name == false
assert !value
assert !name
