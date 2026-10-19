##! Publishes an integer specialization of the generic identity export.

use typing-capability-identity as identity

## Exposes the identity result as an integer.
export let value: Int = identity.public_identity(42)

## Exposes the projected name as an integer.
export let name: Int = identity.public_name({name: 7, enabled: true, marker: identity.marker})

## Exposes the imported shared marker through this module.
export let source_marker: Int = identity.marker
