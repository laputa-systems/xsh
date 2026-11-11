##! Owns fdu's directory tree, per-node state, and path helpers.
##!
##! Nodes are addressed by their index in `Tree.nodes`; children are a sibling
##! chain, so a directory with many entries does not copy a per-node child list.
use tui_surface

## Names the filesystem object kind a node records.
export enum Kind { Root, Directory, File, Symlink, Other }

## Names the scan or deletion state of a node.
export enum State { Pending, Complete, Incomplete, Mount, Alias, Stale, Tombstone }

## One directory entry and its accumulated subtree totals.
export type Node = {
  parent: Int?,
  first_child: Int?,
  last_child: Int?,
  next_sibling: Int?,
  name: Bytes,
  absolute: Path,
  kind: Kind,
  own_apparent: Int,
  own_allocated: Int,
  apparent: Int,
  allocated: Int,
  modified: Int?,
  nlink: Int,
  state: State,
  error: Str?,
  descendants: Int,
}

## Retains the node arena and the root index.
export type Tree = {nodes: List[Node], root: Int}

type Appended = {tree: Tree, id: Int}

## Starts an empty tree.
export pure empty() -> Tree {
  {nodes: [], root: 0}
}

## Appends one node and links it after its parent's existing children.
export pure append(
  tree: Tree,
  parent: Int?,
  name: Bytes,
  location: Path,
  kind: Kind,
  own_apparent: Int,
  own_allocated: Int,
  modified: Int?,
  nlink: Int,
  state: State,
  error: Str? = null,
) -> Appended {
  let id = tree.nodes.len()
  let node: Node = {
    parent: parent,
    first_child: null,
    last_child: null,
    next_sibling: null,
    name: name,
    absolute: location,
    kind: kind,
    own_apparent: own_apparent,
    own_allocated: own_allocated,
    apparent: own_apparent,
    allocated: own_allocated,
    modified: modified,
    nlink: nlink,
    state: state,
    error: error,
    descendants: 0,
  }
  var nodes = tree.nodes
  if let parent_id = parent {
    var upper = nodes[parent_id]
    if let last = upper.last_child {
      var sibling = nodes[last]
      sibling.next_sibling = id
      nodes[last] = sibling
    } else {
      upper.first_child = id
    }
    upper.last_child = id
    nodes[parent_id] = upper
  }
  nodes += [node]
  {tree: {nodes: nodes, root: tree.root}, id: id}
}

## Settles one node with its final state and adds its totals to its parent.
export pure settle(tree: Tree, id: Int, state: State, error: Str? = null) -> Tree {
  var nodes = tree.nodes
  var node = nodes[id]
  let final_state = if node.state == Mount {
    Mount
  } else if node.state == Alias {
    Alias
  } else if node.state == Incomplete or state == Incomplete {
    Incomplete
  } else {
    Complete
  }
  node.state = final_state
  if error != null {
    node.error = error
  }
  nodes[id] = node
  if let parent_id = node.parent {
    var upper = nodes[parent_id]
    upper.apparent += node.apparent
    upper.allocated += node.allocated
    upper.descendants += node.descendants + 1
    if final_state != Complete and upper.state == Pending {
      upper.state = Incomplete
    }
    nodes[parent_id] = upper
  }
  {nodes: nodes, root: tree.root}
}

## Marks one node with an explicit state without changing totals.
export pure set_state(tree: Tree, id: Int, state: State) -> Tree {
  var nodes = tree.nodes
  var node = nodes[id]
  node.state = state
  nodes[id] = node
  {nodes: nodes, root: tree.root}
}

## Marks one node and every live ancestor stale.
export pure mark_stale(tree: Tree, id: Int) -> Tree {
  var nodes = tree.nodes
  var current: Int? = id
  while current != null {
    let at = current ?? 0
    var node = nodes[at]
    if node.state != Tombstone {
      node.state = Stale
      nodes[at] = node
    }
    current = node.parent
  }
  {nodes: nodes, root: tree.root}
}

## Removes one node's own bytes and entry count from its ancestors.
export pure tombstone(tree: Tree, id: Int) -> Tree {
  var nodes = tree.nodes
  var node = nodes[id]
  node.state = Tombstone
  nodes[id] = node
  var current = node.parent
  while current != null {
    let at = current ?? 0
    var upper = nodes[at]
    upper.apparent -= node.own_apparent
    upper.allocated -= node.own_allocated
    upper.descendants -= 1
    nodes[at] = upper
    current = upper.parent
  }
  {nodes: nodes, root: tree.root}
}

## Returns a node's direct children in index order.
export pure children(tree: Tree, id: Int) -> List[Int] {
  var found: List[Int] = []
  var child = tree.nodes[id].first_child
  while child != null {
    let at = child ?? 0
    found += [at]
    child = tree.nodes[at].next_sibling
  }
  found
}

## Returns the node indices from `id` up to the root, excluding the root.
export pure chain_to_root(tree: Tree, id: Int) -> List[Int] {
  var found: List[Int] = []
  var current: Int? = id
  while current != null {
    let at = current ?? 0
    if at != tree.root {
      found += [at]
    }
    current = tree.nodes[at].parent
  }
  found
}

## Reports whether `node` lies anywhere below `ancestor`.
export pure is_inside(tree: Tree, node: Int, ancestor: Int) -> Bool {
  var current = tree.nodes[node].parent
  while current != null {
    let at = current ?? 0
    return true when at == ancestor
    current = tree.nodes[at].parent
  }
  false
}

## Returns the nearest marked directory above `id`, if any.
export pure marked_ancestor(tree: Tree, id: Int, marks: Set[Int]) -> Int? {
  var current = tree.nodes[id].parent
  while current != null {
    let at = current ?? 0
    return at when at in marks
    current = tree.nodes[at].parent
  }
  null
}

## Renders a root-relative path with control and malformed bytes escaped.
export pure relative_display(tree: Tree, id: Int) -> Str {
  let chain = chain_to_root(tree, id)
  var parts: List[Str] = []
  var index = chain.len()
  while index > 0 {
    index -= 1
    parts += [tui_surface.escape_name(tree.nodes[chain[index]].name)]
  }
  parts.join("/")
}
