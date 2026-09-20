##! Embedded implementation of the public `json` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# in-memory path policy and JSON-lines composition live here.
#
# JSON codecs, number restrictions, runtime-value conversions, pretty
# formatting needed by native callers, schema checks, and streaming decode stay
# native.
#
# Shape of the path policy, and why it is written this way.
#
# The path is interpreted once, as a whole, before anything is traversed, so an
# invalid later segment is reported before a missing earlier key: a caller that
# passes a bad path always gets the path rejection, never a traversal failure.
# Interpretation is the validation in `path_segments`, which is what makes each
# element a `Str` key or a non-negative `Int` index; the interpreted path is
# carried as those elements themselves rather than as a declared segment union,
# because a declared union lowers only as a `List` element, never as a fallible
# function's payload or as a parameter type, and the path crosses both
# boundaries. Each walk therefore re-tests the element's type, and repeats the
# rejection `path_segments` already reports as the last arm of that match, so
# the walk stays total. Every rejection carries the baseline spellings, which
# is how the error family's `kind` payload keeps the baseline error kinds
# visible to callers.
#
# `get` walks iteratively over the interpreted segments; `set` and `remove`
# recurse on the remaining segments, because each level rebuilds the container
# it visited. A `Record` updates to a `Record` and a `Map` to a `Map`: the two
# are never converted into each other and values are never round-tripped
# through JSON, so a container's ordinary non-JSON members (`Path`, `Bytes`,
# `Duration`, errors) survive untouched.
#
# A fallible helper here returns `Result[JsonValue]`, never `Result[Any]`.
# Lowering wraps a value matching an `Any` payload back into `Ok`, an error
# value included, so a helper declared `Result[Any]` cannot report its own
# rejections: `get_with_fallback` would read a rejected path as a found value
# and raise it instead of returning the fallback. The declared record payload
# keeps a rejection apart from a value, including a value that is itself an
# error. The public entries still declare the registry's `Result[Any]`, so only
# the boundary those entries present is wrapped; `get` returns what it found or
# the rejection, and `set`/`remove` report their failure the same way.
#
# Container rebuilds go through bulk operations. A `Record` has no persistent
# single-field update except `record_with_field`, which copies the record once,
# so each visited record costs exactly one call per changed field. Lists are
# rebuilt with one comprehension rather than per-element appends, and the
# recursively updated value is spliced in by index rather than accumulated.

## Represent a record with one field set.
##
## Runtime representation bridge: lowering replaces every call with the private
## operation, so the body below is unreachable and raises if it is ever
## reached. It exists only to give the signature a shape that type-checks.
export pure record_with_field(record: Record, field: Str, value: Any) -> Record {
  return [record][record.keys().len()]
}

## Represent a record with one field removed.
##
## Runtime representation bridge, as above.
export pure record_remove_field(record: Record, field: Str) -> Record {
  return [record][record.keys().len()]
}

## The baseline's type name for a runtime value.
##
## Diagnostic bridge: lowering replaces every call with the private operation.
export pure type_name(value: Any) -> Str {
  return [value][1]
}

# The two error kinds this module reports.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spellings
# `json-path` and `type-error` visible to callers. The variants are separate so
# a caller can tell a path rejection from an argument-type rejection without
# branching on a string.
error JsonError = Path(kind: Str, message: Str) | Lines(kind: Str, message: Str)

# A rejected path interpretation, traversal, update, or removal.
pure path_error(message: Str) -> JsonError {
  return JsonError.Path(kind: "json-path", message: message)
}

# A rejected `encode_lines` argument.
pure lines_error(message: Str) -> JsonError {
  return JsonError.Lines(kind: "type-error", message: message)
}

# The value a successful path walk produced.
#
# The payload is a declared record rather than `Any` so that a rejection stays
# distinguishable from a found value; see the module header.
type JsonValue = {value: Any}

# Interpret a whole path.
#
# The path must be a list, and every element is interpreted before any of them
# is used, so the first offending element is the one reported and a valid
# prefix never reaches a container. A `Str` is an object key. An `Int` is a
# list index and must be non-negative; the non-negative rejection is checked
# before the element is accepted as an index, so it is reported for a negative
# `Int` rather than the generic element-type rejection. Every other type shares
# the generic rejection.
pure path_segments(path: Any) -> Result[List[Any]] {
  match path {
    items is List[Any] => {
      for item in items {
        match item {
          key is Str => {}
          index is Int => {
            if index < 0 {
              return Err(path_error("path list indexes must be non-negative"))
            }
          }
          other => {
            return Err(path_error(f"path segments must be Str or Int, found ${type_name(other)}"))
          }
        }
      }
      return Ok(items)
    }
    _ => return Err(path_error(f"path expected List, found ${type_name(path)}"))
  }
}

# Present a walk outcome in the public `Result[Any]` shape.
#
# The unwrapping is the only thing the public entries do to a value that they
# did not find themselves; a rejection keeps its own error value.
pure public_result(outcome: Result[JsonValue]) -> Result[Any] {
  match outcome {
    Ok(found) => return Ok(found.value)
    Err(failure) => return Err(failure)
  }
}

# Walk one interpreted path iteratively.
#
# A key step needs an object: a missing key is reported separately from a
# non-object, and the two containers are walked with their own accessors. An
# index step needs a list: the bound is checked before the element is read, so
# an out-of-range index is reported separately from a non-list.
pure path_get(value: Any, path: Any) -> Result[JsonValue] {
  match path_segments(path) {
    Ok(segments) => {
      var current = value
      for segment in segments {
        match segment {
          key is Str => {
            match current {
              record is Record => {
                match record.get(key) {
                  Ok(found) => { current = found }
                  Err(_) => { return Err(path_error(f"missing object key `${key}`")) }
                }
              }
              fields is Map[Any] => {
                if !fields.has(key) {
                  return Err(path_error(f"missing object key `${key}`"))
                }
                current = fields.get(key, null)
              }
              other => {
                return Err(path_error(f"expected object at key `${key}`, found ${type_name(other)}"))
              }
            }
          }
          index is Int => {
            match current {
              items is List[Any] => {
                if index >= items.len() {
                  return Err(path_error(f"list index ${index} out of bounds"))
                }
                current = items[index]
              }
              other => {
                return Err(path_error(f"expected list at index ${index}, found ${type_name(other)}"))
              }
            }
          }
          other => {
            return Err(path_error(f"path segments must be Str or Int, found ${type_name(other)}"))
          }
        }
      }
      return Ok({value: current})
    }
    Err(failure) => return Err(failure)
  }
}

# Replace one position of a list, keeping every other element.
pure list_replaced(items: List[Any], index: Int, replacement: Any) -> List[Any] {
  return [if position == index { replacement } else { items[position] } for position in range(0, items.len())]
}

# Drop one position of a list, shifting every later element left.
pure list_without(items: List[Any], index: Int) -> List[Any] {
  return [items[position] for position in range(0, items.len()) if position != index]
}

# Set a value at the remaining segments, from `position` on.
#
# An empty remaining path replaces the value itself. A key step into an object
# may add a missing field when it is the last segment, but a missing field on
# the way to a deeper segment is an error, because there is nothing to descend
# into. An index step always needs an existing element: a list is never grown,
# so an out-of-range index is an error whether or not it is the last segment.
# The container keeps its own type: a record updates to a record and a map to a
# map.
pure set_at(value: Any, segments: List[Any], position: Int, replacement: Any) -> Result[JsonValue] {
  if position >= segments.len() {
    return Ok({value: replacement})
  }
  let segment = segments[position]
  let is_leaf = position + 1 == segments.len()
  match segment {
    key is Str => {
      match value {
        record is Record => {
          if is_leaf {
            return Ok({value: record_with_field(record, key, replacement)})
          }
          match record.get(key) {
            Ok(child) => {
              let updated = set_at(child, segments, position + 1, replacement)?
              return Ok({value: record_with_field(record, key, updated.value)})
            }
            Err(_) => {
              return Err(path_error(f"missing intermediate object key `${key}`"))
            }
          }
        }
        fields is Map[Any] => {
          if is_leaf {
            return Ok({value: fields.set(key, replacement)})
          }
          if !fields.has(key) {
            return Err(path_error(f"missing intermediate object key `${key}`"))
          }
          let updated = set_at(fields.get(key, null), segments, position + 1, replacement)?
          return Ok({value: fields.set(key, updated.value)})
        }
        other => {
          return Err(path_error(f"expected object at key `${key}`, found ${type_name(other)}"))
        }
      }
    }
    index is Int => {
      match value {
        items is List[Any] => {
          if index >= items.len() {
            return Err(path_error(f"list index ${index} out of bounds"))
          }
          if is_leaf {
            return Ok({value: list_replaced(items, index, replacement)})
          }
          let updated = set_at(items[index], segments, position + 1, replacement)?
          return Ok({value: list_replaced(items, index, updated.value)})
        }
        other => {
          return Err(path_error(f"expected list at index ${index}, found ${type_name(other)}"))
        }
      }
    }
    other => {
      return Err(path_error(f"path segments must be Str or Int, found ${type_name(other)}"))
    }
  }
}

# Remove the position named by the remaining segments, from `position` on.
#
# An empty remaining path removes nothing and reports the null the baseline
# returns. A key step must find its field at every depth, so a removal never
# creates a container: a missing field is reported as a missing key when it is
# the last segment and as a missing intermediate key on the way to a deeper
# one. An index step always needs an existing element and drops it, shifting
# every later element left.
pure remove_at(value: Any, segments: List[Any], position: Int) -> Result[JsonValue] {
  if position >= segments.len() {
    return Ok({value: null})
  }
  let segment = segments[position]
  let is_leaf = position + 1 == segments.len()
  match segment {
    key is Str => {
      match value {
        record is Record => {
          if !record.has(key) {
            if is_leaf {
              return Err(path_error(f"missing object key `${key}`"))
            }
            return Err(path_error(f"missing intermediate object key `${key}`"))
          }
          if is_leaf {
            return Ok({value: record_remove_field(record, key)})
          }
          match record.get(key) {
            Ok(child) => {
              let updated = remove_at(child, segments, position + 1)?
              return Ok({value: record_with_field(record, key, updated.value)})
            }
            Err(_) => {
              return Err(path_error(f"missing object key `${key}`"))
            }
          }
        }
        fields is Map[Any] => {
          if !fields.has(key) {
            if is_leaf {
              return Err(path_error(f"missing object key `${key}`"))
            }
            return Err(path_error(f"missing intermediate object key `${key}`"))
          }
          if is_leaf {
            return Ok({value: fields.remove(key)})
          }
          let updated = remove_at(fields.get(key, null), segments, position + 1)?
          return Ok({value: fields.set(key, updated.value)})
        }
        other => {
          return Err(path_error(f"expected object at key `${key}`, found ${type_name(other)}"))
        }
      }
    }
    index is Int => {
      match value {
        items is List[Any] => {
          if index >= items.len() {
            return Err(path_error(f"list index ${index} out of bounds"))
          }
          if is_leaf {
            return Ok({value: list_without(items, index)})
          }
          let updated = remove_at(items[index], segments, position + 1)?
          return Ok({value: list_replaced(items, index, updated.value)})
        }
        other => {
          return Err(path_error(f"expected list at index ${index}, found ${type_name(other)}"))
        }
      }
    }
    other => {
      return Err(path_error(f"path segments must be Str or Int, found ${type_name(other)}"))
    }
  }
}

## Read the value at a path.
##
## The path is a list of segments: a `Str` names an object key and a
## non-negative `Int` names a list index. Every segment is validated before the
## value is read, so an invalid path is rejected as a whole rather than at the
## point where traversal reaches it. An empty path reads the value itself.
##
## A `Record` and a `Map` are both objects but stay distinct: a key step reads
## whichever one the value actually is and never converts between them, and
## `Record`/`Map`/`List` members are returned as they are, so non-JSON runtime
## values pass through untouched. A missing key, a missing index, and a value
## of the wrong shape for the next segment are reported separately.
export pure get(value: Any, path: List[Any]) -> Result[Any] {
  return public_result(path_get(value, path))
}

## Read the value at a path, or a fallback when the path finds nothing.
##
## A path rejection — a malformed path, a missing key, an index out of bounds,
## or a value of the wrong shape — evaluates to `fallback`. Every other failure
## propagates, so a caller cannot mistake a real failure for an absent value.
##
## The result is the value itself rather than a `Result`, so a caller reads the
## answer directly: the fallback is the caller-visible value, not something to
## unwrap, and `??`, which needs a `Result` or an optional, does not apply.
export pure get_with_fallback(value: Any, path: List[Any], fallback: Any) -> Any {
  match path_get(value, path) {
    Ok(found) => return found.value
    Err(failure) => {
      match failure {
        JsonError.Path { kind, message } => return fallback
        _ => return Err(failure)
      }
    }
  }
}

## Replace the value at a path.
##
## The path is validated before anything is updated. An empty path replaces the
## whole value. A key step adds a missing field when it is the last segment and
## descends only through fields that already exist, so one update never creates
## more than the final field; an index step needs an existing element because a
## list is never grown. A `Record` updates to a `Record` and a `Map` to a
## `Map`, and the value must be JSON-encodable, so an update cannot introduce a
## member the encoding boundary would reject.
##
## Both the value being updated and the replacement are checked for
## encodability, the value first, and only then is the update performed.
export pure set(value: Any, path: List[Any], replacement: Any) -> Result[Any] {
  match json.encode(value) {
    Ok(_) => {}
    Err(failure) => return Err(failure)
  }
  match json.encode(replacement) {
    Ok(_) => {}
    Err(failure) => return Err(failure)
  }
  match path_segments(path) {
    Ok(segments) => return public_result(set_at(value, segments, 0, replacement))
    Err(failure) => return Err(failure)
  }
}

## Remove the value at a path.
##
## The path is validated before anything is removed. An empty path removes
## nothing and evaluates to `null`. A key step must find its field at every
## depth, so a removal never creates a container; a list element is dropped and
## every later element shifts left. A `Record` stays a `Record` and a `Map`
## stays a `Map`.
export pure remove(value: Any, path: List[Any]) -> Result[Any] {
  match path_segments(path) {
    Ok(segments) => return public_result(remove_at(value, segments, 0))
    Err(failure) => return Err(failure)
  }
}

## Encode values as JSON Lines text.
##
## Every item is encoded compactly on its own line, one newline per item, so an
## empty list produces the empty string. The whole text is built before it is
## returned, so an item that cannot be encoded reports its own failure and no
## partial text is produced.
export pure encode_lines(values: List[Any]) -> Result[Str] {
  match values {
    items is List[Any] => return Ok([(json.encode(item)?) + "\n" for item in items].join(""))
    _ => return Err(lines_error(f"expected List, found ${type_name(values)}"))
  }
}
