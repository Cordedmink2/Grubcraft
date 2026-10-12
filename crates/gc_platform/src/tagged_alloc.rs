//! A global allocator that attributes every heap allocation to a subsystem tag,
//! so memory budgets (docs/12-performance.md) can be measured and enforced.
//!
//! Install it in the executable:
//!
//! ```ignore
//! #[global_allocator]
//! static ALLOC: gc_platform::tagged_alloc::TaggedAlloc = gc_platform::tagged_alloc::TaggedAlloc;
//! ```
//!
//! and wrap subsystem work in [`scope`]. Each allocation carries a small header
//! recording its tag, so frees are charged back to the right subsystem even if
//! they happen on another thread.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

/// Subsystem tags, matching the memory budget table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
    Other = 0,
    Sim,
    Map,
    Render,
    Present,
    Audio,
    Ui,
    Net,
    Assets,
}

impl Tag {
    pub const ALL: [Tag; 9] = [
        Tag::Other,
        Tag::Sim,
        Tag::Map,
        Tag::Render,
        Tag::Present,
        Tag::Audio,
        Tag::Ui,
        Tag::Net,
        Tag::Assets,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tag::Other => "other",
            Tag::Sim => "sim",
            Tag::Map => "map",
            Tag::Render => "render",
            Tag::Present => "present",
            Tag::Audio => "audio",
            Tag::Ui => "ui",
            Tag::Net => "net",
            Tag::Assets => "assets",
        }
    }
}

const TAG_COUNT: usize = Tag::ALL.len();
/// Header size; also the minimum alignment of the underlying block.
const HEADER: usize = 16;

static LIVE_BYTES: [AtomicIsize; TAG_COUNT] = [const { AtomicIsize::new(0) }; TAG_COUNT];
static TOTAL_ALLOCS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static CURRENT: Cell<u8> = const { Cell::new(Tag::Other as u8) };
}

/// Run `f` with allocations on this thread charged to `tag`.
pub fn scope<R>(tag: Tag, f: impl FnOnce() -> R) -> R {
    let prev = CURRENT.with(|c| c.replace(tag as u8));
    let result = f();
    CURRENT.with(|c| c.set(prev));
    result
}

/// Bytes currently allocated under `tag` (excluding headers).
pub fn live_bytes(tag: Tag) -> usize {
    LIVE_BYTES[tag as usize].load(Ordering::Relaxed).max(0) as usize
}

/// Total heap bytes currently allocated through this allocator.
pub fn total_live_bytes() -> usize {
    Tag::ALL.iter().map(|&t| live_bytes(t)).sum()
}

/// Number of allocations made since start (for "no allocations here" checks).
pub fn allocation_count() -> usize {
    TOTAL_ALLOCS.load(Ordering::Relaxed)
}

pub struct TaggedAlloc;

#[inline]
fn offset_for(layout: Layout) -> usize {
    layout.align().max(HEADER)
}

#[inline]
fn current_tag() -> u8 {
    // `try_with` so allocations during thread teardown still work.
    CURRENT.try_with(|c| c.get()).unwrap_or(Tag::Other as u8)
}

unsafe impl GlobalAlloc for TaggedAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let offset = offset_for(layout);
        let Ok(inner) = Layout::from_size_align(layout.size() + offset, offset) else {
            return std::ptr::null_mut();
        };
        let base = unsafe { System.alloc(inner) };
        if base.is_null() {
            return base;
        }
        let tag = current_tag();
        let user = unsafe { base.add(offset) };
        unsafe { user.sub(1).write(tag) };
        LIVE_BYTES[tag as usize].fetch_add(layout.size() as isize, Ordering::Relaxed);
        TOTAL_ALLOCS.fetch_add(1, Ordering::Relaxed);
        user
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let offset = offset_for(layout);
        let tag = unsafe { ptr.sub(1).read() };
        LIVE_BYTES[tag as usize].fetch_sub(layout.size() as isize, Ordering::Relaxed);
        let inner = unsafe { Layout::from_size_align_unchecked(layout.size() + offset, offset) };
        unsafe { System.dealloc(ptr.sub(offset), inner) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let offset = offset_for(layout);
        let tag = unsafe { ptr.sub(1).read() };
        let inner = unsafe { Layout::from_size_align_unchecked(layout.size() + offset, offset) };
        let base = unsafe { System.realloc(ptr.sub(offset), inner, new_size + offset) };
        if base.is_null() {
            return base;
        }
        // The header moved with the block, so the tag is preserved.
        LIVE_BYTES[tag as usize].fetch_add(
            new_size as isize - layout.size() as isize,
            Ordering::Relaxed,
        );
        TOTAL_ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { base.add(offset) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[global_allocator]
    static ALLOC: TaggedAlloc = TaggedAlloc;

    #[test]
    fn charges_and_releases_by_tag() {
        // Tags are global, so use one this crate's other tests never touch.
        let before = live_bytes(Tag::Audio);
        let v: Vec<u8> = scope(Tag::Audio, || Vec::with_capacity(10_000));
        assert_eq!(live_bytes(Tag::Audio), before + 10_000);

        // Growth from another tag's scope stays charged to the original tag.
        let mut v = v;
        scope(Tag::Ui, || v.reserve_exact(30_000));
        assert_eq!(live_bytes(Tag::Audio), before + v.capacity());

        drop(v);
        assert_eq!(live_bytes(Tag::Audio), before);
    }

    #[test]
    fn respects_large_alignment() {
        #[repr(align(64))]
        struct Aligned([u8; 64]);
        let b = scope(Tag::Net, || Box::new(Aligned([7; 64])));
        assert_eq!((&*b as *const Aligned as usize) % 64, 0);
        assert_eq!(b.0[63], 7);
    }
}
