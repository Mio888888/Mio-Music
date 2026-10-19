// Modified by Longbridge for gpui-fast.
//! Everything gpui-fast adds to the Metal renderer lives under this module.
//!
//! The rest of this crate is kept as close to upstream GPUI (Zed's
//! `crates/gpui_apple`) as possible, so that pulling in upstream changes stays
//! a matter of merging rather than of rewriting. Upstream files only *call
//! into* this module: a field holding this module's state, a line forwarding a
//! call to it. The logic itself — data structures, algorithms, bookkeeping,
//! tests — is written here, one file per topic.

pub(crate) mod composition;
pub(crate) mod layers;
pub(crate) mod paths;
