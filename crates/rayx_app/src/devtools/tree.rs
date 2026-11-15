//! The tree's answers, from NSH's hooks on the engine's thread: `dev_tree` for the nodes,
//! `dev_node` for a node's role, events and box, `dev_source` and `dev_instances` for where it was
//! written and which component instances rendered it. Every hook answers in the next commit, so
//! one answer belongs to one layout.
//!
//! The authored tree: where a node's chain of component instances extends its parent's, a
//! pseudo-node of kind `instance` stands for each instance that begins there, so the tree reads
//! as the source does. A pseudo-node's id is [`INSTANCE_IDS`] plus the instance's id: above every
//! node id, so it can never name a node.
//!
//! The devtools layer's own subtree is left out of every answer: from the `DevLayer` component's
//! host down, the overlay included. The host has no name of its own in NSH's tree, so the layer
//! is found by its shape: the view whose test id is [`LAYER_TEST_ID`], the overlay it is in, and
//! the host that holds that overlay alone.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use futures::future::join_all;
use nsh_core::NodeId;
use nsh_core::element::{dev_instances, dev_node, dev_pick, dev_source, dev_tree};
use nsh_core::inspect::{DevDetail, DevInstance, DevNode, DevSource, DevTree};
use rayx_devtools_protocol::{self as proto, NodeLine, RpcError};

use super::agent::{Agent, count_hook};

/// The test id of the view inside the layer's overlay: its subtree is never in an answer.
pub(crate) const LAYER_TEST_ID: &str = "rayx-devtools-layer";

/// Where pseudo-nodes' ids start: above every node id (`u32`).
pub(crate) const INSTANCE_IDS: u64 = 1 << 32;

/// How many nodes `find` lists when the request names no limit.
const FIND_LIMIT: u32 = 50;

/// The tree as the next commit lays it out.
pub(crate) async fn snapshot() -> DevTree {
    count_hook();
    dev_tree().await
}

/// The tree's fingerprint: its shape and what every node shows.
fn fingerprint(tree: &DevTree) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tree.root.hash(&mut hasher);
    for node in &tree.nodes {
        node.id.hash(&mut hasher);
        node.kind.hash(&mut hasher);
        node.children.hash(&mut hasher);
        node.class.hash(&mut hasher);
        node.test_id.hash(&mut hasher);
        node.text.hash(&mut hasher);
        for value in [
            node.frame.origin.x,
            node.frame.origin.y,
            node.frame.size.width,
            node.frame.size.height,
        ] {
            f32::from(value).to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Looks at the tree now: the agent's revision after it.
pub(crate) async fn look(agent: &Agent) -> u64 {
    let tree = snapshot().await;
    seen(agent, &tree)
}

/// The agent saw `tree`: its revision after it, and the parents it remembers for ids that go.
fn seen(agent: &Agent, tree: &DevTree) -> u64 {
    agent.remember(
        tree.nodes
            .iter()
            .filter_map(|node| Some((node.id, node.parent?))),
    );
    agent.saw(fingerprint(tree))
}

/// The roots of the layer's subtrees, outermost: from each view whose test id is
/// [`LAYER_TEST_ID`] up to the overlay it is in, and on to that overlay's parent when it holds
/// the overlay alone (the `DevLayer` component's host). Without an overlay, the view itself.
fn layer_roots(tree: &DevTree, nodes: &HashMap<NodeId, &DevNode>) -> Vec<NodeId> {
    tree.nodes
        .iter()
        .filter(|node| node.test_id.as_deref() == Some(LAYER_TEST_ID))
        .map(|node| {
            let Some(overlay) = node
                .parent
                .and_then(|id| nodes.get(&id))
                .filter(|parent| parent.kind == "overlay")
            else {
                return node.id;
            };
            match overlay.parent.and_then(|id| nodes.get(&id)) {
                Some(host) if host.parent.is_some() && host.children == [overlay.id] => host.id,
                _ => overlay.id,
            }
        })
        .collect()
}

/// The nodes of `tree` by id, and the ones inside the layer.
pub(crate) struct Index<'a> {
    nodes: HashMap<NodeId, &'a DevNode>,
    layer: HashSet<NodeId>,
}

impl<'a> Index<'a> {
    pub(crate) fn of(tree: &'a DevTree) -> Self {
        let nodes: HashMap<NodeId, &DevNode> = tree.nodes.iter().map(|n| (n.id, n)).collect();
        let mut layer = HashSet::new();
        for root in layer_roots(tree, &nodes) {
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                if layer.insert(id)
                    && let Some(node) = nodes.get(&id)
                {
                    stack.extend(node.children.iter().copied());
                }
            }
        }
        Self { nodes, layer }
    }

    fn visible(&self, id: NodeId) -> Option<&'a DevNode> {
        if self.layer.contains(&id) {
            return None;
        }
        self.nodes.get(&id).copied()
    }

    /// The nodes under `root` in tree order with their depth, `depth` levels at most.
    fn below(&self, root: NodeId, depth: Option<u32>) -> Vec<(&'a DevNode, u32)> {
        let mut out = Vec::new();
        let mut stack = vec![(root, 0u32)];
        while let Some((id, level)) = stack.pop() {
            let Some(node) = self.visible(id) else {
                continue;
            };
            out.push((node, level));
            if depth.is_none_or(|depth| level < depth) {
                for child in node.children.iter().rev() {
                    stack.push((*child, level + 1));
                }
            }
        }
        out
    }
}

/// What the hooks say of one node beyond the tree.
struct Facts {
    detail: Option<DevDetail>,
    source: Option<DevSource>,
    instances: Vec<DevInstance>,
}

/// The facts of `ids`, all in the same commit.
async fn facts(ids: &[NodeId]) -> HashMap<NodeId, Facts> {
    let asked = ids.iter().map(|&id| async move {
        for _ in 0..3 {
            count_hook();
        }
        let (detail, source, instances) =
            futures::join!(dev_node(id), dev_source(id), dev_instances(id));
        (
            id,
            Facts {
                detail,
                source,
                instances,
            },
        )
    });
    join_all(asked).await.into_iter().collect()
}

fn stamp(source: &DevSource) -> proto::SourceStamp {
    proto::SourceStamp {
        module: source.module.clone(),
        anchor: source.anchor.clone(),
        line: source.line,
        column: source.column,
    }
}

fn component(instance: &DevInstance) -> proto::ComponentRef {
    proto::ComponentRef {
        id: instance.id,
        name: instance.key.clone(),
        call_site: instance.call_site.as_ref().map(stamp),
    }
}

/// A node's role in words (`button`, `link`, …), `None` for none.
fn role(detail: &DevDetail) -> Option<String> {
    let role = detail.accessibility.as_ref()?.role;
    let name = format!("{role:?}");
    (name != "None").then(|| lower_first(&name))
}

/// A node's events in words (`press`, `hover`, …).
fn events(detail: &DevDetail) -> Vec<String> {
    nsh_core::EventKind::ALL
        .iter()
        .filter(|event| detail.events.contains(**event))
        .map(|event| lower_first(&format!("{event:?}")))
        .collect()
}

fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

/// Whether a person can act on the node: it listens to events, or takes text or scrolls.
fn interactive(node: &DevNode, facts: Option<&Facts>) -> bool {
    matches!(
        node.kind,
        "input" | "textInput" | "scroll" | "switch" | "slider"
    ) || facts
        .and_then(|facts| facts.detail.as_ref())
        .is_some_and(|detail| !detail.events.is_empty())
}

fn flags(node: &DevNode) -> Vec<String> {
    let flags = node.flags;
    [
        (flags.hidden, "hidden"),
        (flags.culled, "culled"),
        (flags.spare, "spare"),
        (flags.placeholder, "placeholder"),
        (flags.overlay, "overlay"),
        (flags.overridden, "overridden"),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, name)| name.to_owned())
    .collect()
}

/// A length as the protocol carries it: a layout that has not run (or a NaN) is `0`, since JSON
/// has no NaN.
fn finite(points: nsh_core::geometry::Points) -> f32 {
    let value = f32::from(points);
    if value.is_finite() { value } else { 0.0 }
}

/// Where node `id` is on its surface, when it is in the tree (a screenshot's crop).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn frame_of(index: &Index<'_>, id: NodeId) -> Option<proto::Rect> {
    index.visible(id).map(frame)
}

/// Where a node is on its surface.
fn frame(node: &DevNode) -> proto::Rect {
    let origin = node.page.unwrap_or(node.frame.origin);
    proto::Rect {
        x: finite(origin.x),
        y: finite(origin.y),
        width: finite(node.frame.size.width),
        height: finite(node.frame.size.height),
    }
}

/// A node's line, at `depth` below the answer's root.
fn line(node: &DevNode, parent: Option<u64>, depth: u32, facts: Option<&Facts>) -> NodeLine {
    NodeLine {
        id: u64::from(node.id),
        parent,
        depth,
        kind: node.kind.to_owned(),
        role: facts.and_then(|f| f.detail.as_ref()).and_then(role),
        text: node.text.clone(),
        test_id: node.test_id.as_deref().map(str::to_owned),
        source: facts.and_then(|f| f.source.as_ref()).map(stamp),
        components: facts.map_or_else(Vec::new, |f| f.instances.iter().map(component).collect()),
        frame: frame(node),
        flags: flags(node),
    }
}

/// Cuts `lines` to `max_chars` characters of JSON; whether it cut.
fn bound(lines: &mut Vec<NodeLine>, max_chars: Option<u32>) -> bool {
    let Some(max) = max_chars else {
        return false;
    };
    let mut used = 0usize;
    for (index, line) in lines.iter().enumerate() {
        used += serde_json::to_string(line).map_or(0, |text| text.len());
        if used > max as usize {
            lines.truncate(index);
            return true;
        }
    }
    false
}

/// `tree`.
pub(crate) async fn tree(
    agent: Rc<Agent>,
    params: proto::TreeParams,
) -> Result<proto::TreeResult, RpcError> {
    let tree = snapshot().await;
    let index = Index::of(&tree);
    let root = match params.root {
        Some(root) => alive(&agent, &index, root)?,
        None => tree.root,
    };
    let revision = seen(&agent, &tree);
    let nodes = index.below(root, params.depth);
    let ids: Vec<NodeId> = nodes.iter().map(|(node, _)| node.id).collect();
    let facts = facts(&ids).await;

    // The nodes kept: all, or the interactive ones and the ones holding them.
    let keep: HashSet<NodeId> = if params.interactive_only {
        let mut keep = HashSet::new();
        for (node, _) in &nodes {
            if interactive(node, facts.get(&node.id)) {
                let mut at = Some(node.id);
                while let Some(id) = at {
                    if !keep.insert(id) || id == root {
                        break;
                    }
                    at = index.nodes.get(&id).and_then(|node| node.parent);
                }
            }
        }
        keep
    } else {
        ids.iter().copied().collect()
    };

    let mut lines = Vec::new();
    // The instance chain of each node's line, to insert pseudo-nodes where it grows.
    let mut chains: HashMap<NodeId, Vec<u32>> = HashMap::new();
    // The line id a node's children hang under (the node, or its last pseudo-node).
    let mut extra: HashMap<NodeId, u32> = HashMap::new();
    for (node, depth) in nodes {
        if !keep.contains(&node.id) {
            continue;
        }
        let fact = facts.get(&node.id);
        let chain: Vec<u32> = fact.map_or_else(Vec::new, |f| {
            f.instances
                .iter()
                .rev()
                .map(|instance| instance.id)
                .collect()
        });
        let parent_chain = node
            .parent
            .and_then(|parent| chains.get(&parent))
            .cloned()
            .unwrap_or_default();
        let shift = node
            .parent
            .and_then(|parent| extra.get(&parent))
            .copied()
            .unwrap_or(0);
        let mut parent_line = node.parent.filter(|_| node.id != root).map(u64::from);
        let mut depth = depth + shift;
        let mut added = 0;
        if node.id != root && chain.len() > parent_chain.len() && chain.starts_with(&parent_chain) {
            for instance in &chain[parent_chain.len()..] {
                let found = fact.and_then(|f| f.instances.iter().find(|i| i.id == *instance));
                let id = INSTANCE_IDS + u64::from(*instance);
                lines.push(NodeLine {
                    id,
                    parent: parent_line,
                    depth,
                    kind: "instance".to_owned(),
                    text: found.map(|i| i.key.clone()),
                    source: found.and_then(|i| i.call_site.as_ref()).map(stamp),
                    ..NodeLine::default()
                });
                parent_line = Some(id);
                depth += 1;
                added += 1;
            }
        }
        lines.push(line(node, parent_line, depth, fact));
        chains.insert(node.id, chain);
        extra.insert(node.id, shift + added);
    }
    // The depth counts the pseudo-nodes: the tree as authored.
    if let Some(max) = params.depth {
        lines.retain(|line| line.depth <= max);
    }
    let truncated = bound(&mut lines, params.max_chars);
    Ok(proto::TreeResult {
        revision,
        root: u64::from(root),
        nodes: lines,
        truncated,
    })
}

/// `node`.
pub(crate) async fn node(
    agent: Rc<Agent>,
    params: proto::NodeParams,
) -> Result<proto::NodeResult, RpcError> {
    let tree = snapshot().await;
    let index = Index::of(&tree);
    let id = alive(&agent, &index, params.id)?;
    let revision = seen(&agent, &tree);
    let Some(node) = index.visible(id) else {
        return Err(RpcError::stale_node(params.id, None));
    };
    let facts = facts(&[id]).await;
    let fact = facts.get(&id);
    let detail = fact.and_then(|f| f.detail.as_ref());
    let layout = detail.map_or_else(proto::BoxModel::default, |detail| {
        let edges = |edges: &nsh_core::inspect::DevEdges| proto::Edges {
            top: finite(edges.top),
            right: finite(edges.right),
            bottom: finite(edges.bottom),
            left: finite(edges.left),
        };
        proto::BoxModel {
            frame: frame(node),
            padding: edges(&detail.layout.padding),
            border: edges(&detail.layout.border),
            margin: edges(&detail.layout.margin),
        }
    });
    Ok(proto::NodeResult {
        revision,
        node: line(node, node.parent.map(u64::from), 0, fact),
        layout,
        class: node.class.clone(),
        style: detail.map_or_else(Vec::new, |detail| {
            detail
                .inline_style
                .iter()
                .map(|declaration| {
                    let text = format!("{declaration:?}");
                    let (name, value) = text.split_once('(').unwrap_or((&text, ""));
                    proto::StyleEntry {
                        name: lower_first(name),
                        value: value.trim_end_matches(')').to_owned(),
                    }
                })
                .collect()
        }),
        label: detail
            .and_then(|detail| detail.accessibility.as_ref())
            .and_then(|accessibility| accessibility.label.as_deref())
            .map(str::to_owned),
        events: detail.map_or_else(Vec::new, events),
    })
}

/// Node `id` when it is in the tree now; else the error naming the subtree to take a new
/// snapshot of: the nearest ancestor of it the agent saw that is still there.
pub(crate) fn alive(agent: &Agent, index: &Index<'_>, id: u64) -> Result<NodeId, RpcError> {
    let gone = || RpcError::stale_node(id, agent.nearest(id, |id| index.visible(id).is_some()));
    let node = NodeId::try_from(id).map_err(|_| gone())?;
    index.visible(node).map(|node| node.id).ok_or_else(gone)
}

/// `find`.
pub(crate) async fn find(
    agent: Rc<Agent>,
    params: proto::FindParams,
) -> Result<proto::FindResult, RpcError> {
    let tree = snapshot().await;
    let revision = seen(&agent, &tree);
    let index = Index::of(&tree);
    let nodes = index.below(tree.root, None);
    // Text a node's subtree reads, for a match on a view that holds the text.
    let reads = |node: &DevNode, text: &str| -> bool {
        let mut stack = vec![node.id];
        while let Some(id) = stack.pop() {
            if let Some(node) = index.visible(id) {
                if node.text.as_deref().is_some_and(|t| t.contains(text)) {
                    return true;
                }
                stack.extend(node.children.iter().copied());
            }
        }
        false
    };
    let candidates: Vec<&DevNode> = nodes
        .iter()
        .map(|(node, _)| *node)
        .filter(|node| {
            params
                .test_id
                .as_deref()
                .is_none_or(|id| node.test_id.as_deref() == Some(id))
        })
        .filter(|node| params.kind.as_deref().is_none_or(|kind| node.kind == kind))
        .filter(|node| params.text.as_deref().is_none_or(|text| reads(node, text)))
        .collect();
    let ids: Vec<NodeId> = candidates.iter().map(|node| node.id).collect();
    let facts = facts(&ids).await;
    let limit = params.limit.unwrap_or(FIND_LIMIT) as usize;
    let mut matched: Vec<NodeLine> = candidates
        .into_iter()
        .map(|node| line(node, node.parent.map(u64::from), 0, facts.get(&node.id)))
        .filter(|line| params.role.is_none() || line.role == params.role)
        .collect();
    // The innermost match of a text: a view holding the text reads it too.
    if params.text.is_some() {
        let parents: HashSet<u64> = matched.iter().filter_map(|line| line.parent).collect();
        matched.retain(|line| !parents.contains(&line.id) || params.role.is_some());
    }
    let truncated = matched.len() > limit;
    matched.truncate(limit);
    Ok(proto::FindResult {
        revision,
        nodes: matched,
        truncated,
    })
}

/// `pick`: the nodes under a point, innermost first, the layer's left out.
pub(crate) async fn pick(
    agent: Rc<Agent>,
    params: proto::PickParams,
) -> Result<proto::PickResult, RpcError> {
    let tree = snapshot().await;
    let revision = seen(&agent, &tree);
    let index = Index::of(&tree);
    // The layer's outermost node: its overlay is above the app at every point.
    let skip = layer_roots(&tree, &index.nodes).into_iter().next();
    count_hook();
    let picked = dev_pick(
        nsh_core::geometry::Point {
            x: params.x.into(),
            y: params.y.into(),
        },
        skip,
    )
    .await;
    Ok(proto::PickResult {
        revision,
        nodes: picked
            .into_iter()
            .filter(|id| index.visible(*id).is_some())
            .map(u64::from)
            .collect(),
    })
}

/// The nodes under page point `x`, `y`, innermost first, the layer's left out (the layer's
/// pick mode).
pub(crate) async fn pick_at(agent: &Rc<Agent>, x: f32, y: f32) -> Vec<u64> {
    pick(Rc::clone(agent), proto::PickParams { x, y })
        .await
        .map(|picked| picked.nodes)
        .unwrap_or_default()
}

/// A node's box on its surface: its frame, and its margin and padding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Outline {
    pub(crate) frame: proto::Rect,
    pub(crate) margin: proto::Edges,
    pub(crate) padding: proto::Edges,
}

/// Node `id`'s box, when it is in the tree.
pub(crate) async fn outline(agent: &Rc<Agent>, id: u64) -> Option<Outline> {
    let answer = node(Rc::clone(agent), proto::NodeParams { id })
        .await
        .ok()?;
    Some(Outline {
        frame: answer.layout.frame,
        margin: answer.layout.margin,
        padding: answer.layout.padding,
    })
}
