// Modified by Longbridge for gpui-fast.
//! `fast::dispatch::copy_nodes` against upstream's node-by-node copy.

use std::{any::TypeId, cell::RefCell, mem, ops::Range, rc::Rc};

use crate::fast::dispatch::copy_nodes;
use crate::key_dispatch::{DispatchNode, DispatchNodeId, DispatchTree};
use crate::{ActionRegistry, EntityId, FocusId, KeyContext, Keymap};

/// A small deterministic generator, so a failure can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}

fn tree() -> DispatchTree {
    DispatchTree::new(
        Rc::new(RefCell::new(Keymap::new(Vec::new()))),
        Rc::new(ActionRegistry::default()),
    )
}

fn focus_id(n: u64) -> FocusId {
    slotmap::KeyData::from_ffi(n).into()
}

/// Gives the active node what an element might: a key context, a focus
/// handle, a view, action listeners.
fn decorate(tree: &mut DispatchTree, rng: &mut Rng) {
    if rng.one_in(4) {
        let name = ["a", "b", "c"][rng.below(3)];
        tree.set_key_context(KeyContext::parse(name).unwrap());
    }
    if rng.one_in(5) {
        tree.set_focus_id(focus_id(1 + rng.below(40) as u64));
    }
    if rng.one_in(4) {
        // Few views, so a node often names the view already around it.
        tree.set_view_id(EntityId::from(1 + rng.below(3) as u64));
    }
    for _ in 0..rng.below(3) {
        tree.on_action(TypeId::of::<u8>(), Rc::new(|_, _, _, _| {}));
    }
}

/// A frame's tree: a few roots, each a random subtree.
fn source(seed: u64, len: usize) -> DispatchTree {
    let mut rng = Rng(seed);
    let mut tree = tree();
    let mut depth = 0;
    while tree.len() < len {
        tree.push_node();
        depth += 1;
        decorate(&mut tree, &mut rng);
        // Close a few nodes, so siblings and deep chains both occur.
        let closes = if rng.one_in(3) {
            rng.below(depth + 1)
        } else {
            0
        };
        for _ in 0..closes {
            tree.pop_node();
            depth -= 1;
        }
    }
    for _ in 0..depth {
        tree.pop_node();
    }
    tree
}

/// Upstream's `DispatchTree::move_node`.
fn move_node(target: &mut DispatchTree, source: &mut DispatchNode) {
    target.push_node();
    if let Some(context) = source.context.clone() {
        target.set_key_context(context);
    }
    if let Some(focus_id) = source.focus_id {
        target.set_focus_id(focus_id);
    }
    if let Some(view_id) = source.view_id {
        target.set_view_id(view_id);
    }
    let node = target.nodes.last_mut().unwrap();
    node.key_listeners = mem::take(&mut source.key_listeners);
    node.action_listeners = mem::take(&mut source.action_listeners);
    node.modifiers_changed_listeners = mem::take(&mut source.modifiers_changed_listeners);
}

/// Upstream's node-by-node copy, as `DispatchTree::reuse_subtree` and
/// `fast::splice` did it.
fn copy_one_by_one(
    target: &mut DispatchTree,
    source: &mut DispatchTree,
    range: Range<usize>,
    open: Option<&mut Vec<DispatchNodeId>>,
    focus: Option<FocusId>,
) -> bool {
    let mut own = Vec::new();
    let open = open.unwrap_or(&mut own);
    let mut contains_focus = false;
    for index in range {
        let node = &mut source.nodes[index];
        while let Some(&last) = open.last() {
            if node.parent == Some(last) {
                break;
            }
            open.pop();
            target.pop_node();
        }
        open.push(DispatchNodeId(index));
        if node.focus_id.is_some() && node.focus_id == focus {
            contains_focus = true;
        }
        move_node(target, node);
    }
    while own.pop().is_some() {
        target.pop_node();
    }
    contains_focus
}

fn unwind_to(target: &mut DispatchTree, open: &mut Vec<DispatchNodeId>, parent: Option<usize>) {
    while let Some(&last) = open.last() {
        if Some(last.0) == parent {
            break;
        }
        open.pop();
        target.pop_node();
    }
}

/// A target tree with a chain of nodes already open, which the copy hangs
/// off.
fn target(rng: &mut Rng) -> DispatchTree {
    let mut tree = tree();
    for _ in 0..1 + rng.below(4) {
        tree.push_node();
        decorate(&mut tree, rng);
    }
    tree
}

fn assert_same(expected: &DispatchTree, actual: &DispatchTree, what: &str) {
    assert_eq!(
        expected.nodes.len(),
        actual.nodes.len(),
        "{what}: node count"
    );
    for (ix, (e, a)) in expected.nodes.iter().zip(&actual.nodes).enumerate() {
        assert_eq!(e.parent, a.parent, "{what}: parent of {ix}");
        assert_eq!(e.context, a.context, "{what}: context of {ix}");
        assert_eq!(e.focus_id, a.focus_id, "{what}: focus of {ix}");
        assert_eq!(e.view_id, a.view_id, "{what}: view of {ix}");
        assert_eq!(
            e.action_listeners.len(),
            a.action_listeners.len(),
            "{what}: listeners of {ix}"
        );
    }
    assert_eq!(expected.node_stack, actual.node_stack, "{what}: node stack");
    assert_eq!(
        expected.context_stack, actual.context_stack,
        "{what}: context stack"
    );
    assert_eq!(expected.view_stack, actual.view_stack, "{what}: view stack");
    assert_eq!(
        expected.focusable_node_ids, actual.focusable_node_ids,
        "{what}: focusable nodes"
    );
    assert_eq!(
        expected.view_node_ids, actual.view_node_ids,
        "{what}: view nodes"
    );
}

/// The end of the subtree at `node`.
fn subtree_end(tree: &DispatchTree, node: usize) -> usize {
    let mut end = node + 1;
    while end < tree.len() {
        let mut ancestor = tree.nodes[end].parent;
        while let Some(id) = ancestor.filter(|id| id.0 > node) {
            ancestor = tree.nodes[id.0].parent;
        }
        if ancestor.map(|id| id.0) != Some(node) {
            break;
        }
        end += 1;
    }
    end
}

#[test]
fn test_copy_nodes_matches_copying_one_by_one() {
    for seed in 1..400u64 {
        let len = 5 + (seed as usize * 7) % 120;
        let mut expected_source = source(seed, len);
        let mut actual_source = source(seed, len);
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let focus = Some(focus_id(1 + rng.below(40) as u64));

        // A reused view: one subtree, copied whole and closed again.
        let root = rng.below(len);
        let range = root..subtree_end(&expected_source, root);
        let mut expected = target(&mut Rng(seed));
        let mut actual = target(&mut Rng(seed));
        let e = copy_one_by_one(
            &mut expected,
            &mut expected_source,
            range.clone(),
            None,
            focus,
        );
        let a = copy_nodes(&mut actual, &mut actual_source, range.clone(), None, focus);
        assert_eq!(e, a, "seed {seed}: contains focus");
        assert_same(&expected, &actual, &format!("seed {seed}, whole"));

        // A spliced view: a subtree copied in stretches around nested
        // subtrees, with the copy left open between stretches.
        let mut expected_source = source(seed, len);
        let mut actual_source = source(seed, len);
        let root = rng.below(len);
        let end = subtree_end(&expected_source, root);
        let mut gaps = Vec::new();
        let mut cursor = root + 1;
        while cursor < end {
            let gap = cursor + rng.below(end - cursor);
            gaps.push(gap..subtree_end(&expected_source, gap));
            cursor = subtree_end(&expected_source, gap) + rng.below(3);
        }
        let mut expected = target(&mut Rng(seed));
        let mut actual = target(&mut Rng(seed));
        let mut expected_open = Vec::new();
        let mut actual_open = Vec::new();
        let mut cursor = root;
        for (ix, gap) in gaps.iter().chain([&(end..end)]).enumerate() {
            let what = format!("seed {seed}, stretch {ix}");
            let stretch = cursor..gap.start;
            let e = copy_one_by_one(
                &mut expected,
                &mut expected_source,
                stretch.clone(),
                Some(&mut expected_open),
                focus,
            );
            let a = copy_nodes(
                &mut actual,
                &mut actual_source,
                stretch,
                Some(&mut actual_open),
                focus,
            );
            assert_eq!(e, a, "{what}: contains focus");
            assert_eq!(expected_open, actual_open, "{what}: open");
            assert_same(&expected, &actual, &what);
            if gap.is_empty() {
                break;
            }
            // Where the gap hangs off, and what building it again pushes.
            let parent = expected_source.nodes[gap.start].parent.map(|id| id.0);
            unwind_to(&mut expected, &mut expected_open, parent);
            unwind_to(&mut actual, &mut actual_open, parent);
            assert_same(&expected, &actual, &format!("{what}, unwound"));
            for tree in [&mut expected, &mut actual] {
                tree.push_node();
                decorate(tree, &mut Rng(seed + ix as u64));
                tree.pop_node();
            }
            cursor = gap.end;
        }
    }
}
