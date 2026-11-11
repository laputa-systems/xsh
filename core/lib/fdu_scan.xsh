##! Turns the `fs.scan` stream into fdu's tree model.
##!
##! The stream reports one entry record per filesystem object and a closing
##! `done` record for each scanned directory. Sizes and entry counts are
##! accumulated here, so the native scan only observes the filesystem.
use core.lib.fdu_model as model

pure kind_from(text: Str) -> model.Kind {
  match text {
    "root" => .Root
    "dir" => .Directory
    "file" => .File
    "symlink" => .Symlink
    else => .Other
  }
}

pure state_from(text: Str) -> model.State {
  match text {
    "complete" => .Complete
    "pending" => .Pending
    "incomplete" => .Incomplete
    "mount" => .Mount
    "alias" => .Alias
    else => .Incomplete
  }
}

## Scans `root` and builds the complete tree, including excluded subtrees.
export proc tree(root: Path, excludes: List[Path] = [], name: Bytes? = null) [fs, error] -> Result[model.Tree, Error] {
  let entries = fs.scan(root, exclude: excludes)?
  var built = model.empty()
  var directories: List[Int] = []
  for record in entries {
    if record.done {
      guard !directories.is_empty() else {
        return Err(error.failure("scan stream closed an unknown directory"))
      }
      let id = directories[directories.len() - 1]
      directories = directories[..directories.len() - 1]
      built = model.settle(built, id, state_from(record.state), record.error)
    } else {
      let parent: Int? = if record.parent < 0 { null } else { record.parent }
      let kind = kind_from(record.kind)
      let state = state_from(record.state)
      let added = model.append(
        built,
        parent,
        record.name,
        record.path,
        kind,
        record.apparent,
        record.allocated,
        record.modified,
        record.nlink,
        state,
        record.error,
      )
      built = added.tree
      if (kind == .Directory or kind == .Root) and state == .Pending {
        directories += [added.id]
      } else {
        built = model.settle(built, added.id, state, record.error)
      }
    }
  }
  guard directories.is_empty() else {
    return Err(error.failure("scan stream ended before every directory was closed"))
  }
  guard !built.nodes.is_empty() else {
    return Err(error.failure("scan stream produced no root entry"))
  }
  var nodes = built.nodes
  var root_node = nodes[0]
  root_node.name = name ?? bytes.from_text(root.display())
  nodes[0] = root_node
  built = {...built, nodes: nodes, root: 0}
  Ok(built)
}
