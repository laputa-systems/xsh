use svc

type Local = {key: Str, digest: Str, content: Str}

type Change = {key: Str, upload: Bool, digest: Str, content: Str}

type Done = {key: Str, upload: Bool, ok: Bool}

proc apply(bucket: Str, change: Change) -> Done {
  let written = if change.upload {
    svc.store_put(bucket, change.key, change.digest, change.content)
  } else {
    svc.store_delete(bucket, change.key)
  }

  let ok = match written {
    Ok(_) => true
    Err(svc.ServiceError.NotFound) => ! change.upload
    Err(_) => false
  }

  Done(key: change.key, upload: change.upload, ok:)
}

## Mirror /work/site into an object store bucket.
cli main(bucket: Str, delete = false) {
  let root = p"/work/site"
  let locals = fs.walk(root)?
    |> where .kind == "file"
    |> map { |entry|
      let data = entry.path.read_bytes()?
      Local(key: entry.path.relative_to(root).display(), digest: data.sha256().hex(), content: data.base64())
    }

  let stored = svc.store_list(bucket)?
  let known = {o.key: o.sha256 for o in stored}
  let local_keys = [l.key for l in locals]

  var changes: List[Change] = []
  var unchanged = 0
  for item in locals {
    if (known.get(item.key) ?? "") == item.digest {
      unchanged += 1
    } else {
      changes += [Change(key: item.key, upload: true, digest: item.digest, content: item.content)]
    }
  }

  if delete {
    for object in stored {
      if object.key not in local_keys {
        changes += [Change(key: object.key, upload: false, digest: "", content: "")]
      }
    }
  }

  let done = changes |> par-map(jobs: 3) { |change| apply(bucket, change) }
  let uploaded = [d.key for d in done if d.ok and d.upload] |> sort
  let deleted = [d.key for d in done if d.ok and ! d.upload] |> sort
  let failed = [d.key for d in done if ! d.ok] |> sort

  print (json.encode({uploaded, deleted, failed, unchanged})?)
  if ! failed.is_empty() {
    exit 4
  }
}
