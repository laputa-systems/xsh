##! Publishes a text specialization of the generic identity export.

let marker = 99

use typing-capability-identity as identity

## Exposes the identity result as text.
export let value: Str = identity.public_identity("demo")

## Exposes the projected name as text.
export let name: Str = identity.public_name({name: "demo", version: "1.0", marker: identity.marker})

## Exposes the imported shared marker through this module.
export let source_marker: Int = identity.marker
