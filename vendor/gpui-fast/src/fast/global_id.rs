// Modified by Longbridge for gpui-fast.
//! Reuse global element ids across frames using upstream's incremental path hash.

use crate::GlobalElementId;
use collections::FxHashMap;
use std::mem;

/// The global id of the element id stack of `window`, handed out again from
/// the [`GlobalIdCache`] when it was handed out this frame or the last.
#[inline(always)]
pub(crate) fn current(window: &mut crate::Window) -> GlobalElementId {
    window.global_ids.get(&window.element_id_stack)
}

/// The global ids handed out this frame and the last, by the hash of their
/// path.
///
/// An element whose path is the one it had on the frame before, which is
/// nearly every element, is given the id it had then, rather than a copy of
/// the whole element id stack to be dropped id by id when the frame is done.
#[derive(Default)]
pub(crate) struct GlobalIdCache {
    previous: FxHashMap<u64, GlobalElementId>,
    current: FxHashMap<u64, GlobalElementId>,
}

impl GlobalIdCache {
    /// The global id of `path`: one handed out already if there is one, a
    /// new one otherwise.
    pub(crate) fn get(&mut self, path: &crate::window::ElementIdStack) -> GlobalElementId {
        let hash = path.hash();
        if let Some(id) = self.current.get(&hash)
            && *id.ids == **path
        {
            return id.clone();
        }
        let id = match self.previous.get(&hash) {
            Some(id) if *id.ids == **path => id.clone(),
            _ => path.global_id(),
        };
        self.current.insert(hash, id.clone());
        id
    }

    /// Keeps this frame's ids for the next one to find, and forgets the
    /// previous frame's.
    pub(crate) fn finish_frame(&mut self) {
        mem::swap(&mut self.previous, &mut self.current);
        self.current.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{GlobalElementId, GlobalIdCache};
    use crate::ElementId;
    use std::hash::{BuildHasher, BuildHasherDefault};
    use std::sync::Arc;

    /// An id's hash is worked out from its path when it is made, so ids made
    /// apart from the same path have to agree, and ids of different paths
    /// must not be taken for one another even where their hashes would meet.
    #[test]
    fn global_ids_compare_and_hash_by_path() {
        let path = |ids: &[&'static str]| {
            GlobalElementId::new(
                &ids.iter()
                    .map(|id| ElementId::from(*id))
                    .collect::<Vec<_>>(),
            )
        };
        let hash = |id: &GlobalElementId| {
            BuildHasherDefault::<collections::FxHasher>::default().hash_one(id)
        };

        let a = path(&["root", "table", "row"]);
        let b = path(&["root", "table", "row"]);
        assert_eq!(a, b);
        assert_eq!(hash(&a), hash(&b));

        let c = path(&["root", "table", "cell"]);
        assert_ne!(a, c);

        // A path that happens to share another's hash is still a different id.
        let mut forged = c;
        forged.hash = a.hash;
        assert_ne!(a, forged);

        assert_eq!(GlobalElementId::default(), path(&[]));
    }

    /// A path asked for again this frame or the next gets the id already
    /// made for it, not a new copy; one unused for a whole frame is let go,
    /// and one whose hash is shared with another path is never mistaken
    /// for it.
    #[test]
    fn global_ids_are_reused_while_their_path_is_in_use() {
        let path = |ids: &[&'static str]| {
            let mut stack = crate::window::ElementIdStack::default();
            for id in ids {
                stack.push(ElementId::from(*id));
            }
            stack
        };
        let row = path(&["root", "table", "row"]);
        let cell = path(&["root", "table", "cell"]);
        let mut cache = GlobalIdCache::default();
        let hash = GlobalElementId::new(&row).hash;

        let first = cache.get(&row);
        assert!(Arc::ptr_eq(&first.ids, &cache.get(&row).ids));

        cache.finish_frame();
        let next = cache.get(&row);
        assert!(
            Arc::ptr_eq(&first.ids, &next.ids),
            "a path in use last frame keeps its id"
        );

        cache.finish_frame();
        cache.finish_frame();
        let later = cache.get(&row);
        assert!(
            !Arc::ptr_eq(&first.ids, &later.ids),
            "a path unused for a frame is let go"
        );
        assert_eq!(first, later);

        cache.current.insert(
            hash,
            GlobalElementId {
                ids: Arc::from(&*cell),
                hash,
            },
        );
        assert_eq!(&*cache.get(&row).ids, &*row);
    }
}
