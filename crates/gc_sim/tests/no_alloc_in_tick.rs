//! The simulation must not touch the heap while ticking (docs/12-performance.md,
//! PRF-5). A counting allocator wraps the system allocator for this test binary.
//! It counts only the test's own thread: the test harness allocates on other
//! threads while tests run, which made a process-wide count flaky.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use gc_sim::scenario::{DRILL_UNITS, Drill, ribs_map};
use gc_sim::{Capacities, MatchSetup, World};

struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

fn counting() -> bool {
    COUNTING.try_with(Cell::get).unwrap_or(false)
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if counting() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if counting() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn ticking_never_allocates() {
    // On the ridged map, so route searches run too, with woods and tall
    // grass in a checker of 16-tile squares, so regiments hide and spot.
    let nav = ribs_map(256);
    let cover: Vec<u8> = (0..256 * 256)
        .map(|i| (((i % 256) / 16 + (i / 256) / 16) % 3) as u8)
        .collect();
    let mut world = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground {
            heights: &[],
            water_level: 0,
            cover: &cover,
            growth: &[],
            footing: &[],
        },
        champions: true,
        generals: true,
    });
    assert!(world.terrain().has_cover());
    let mut drill = Drill::new(7);
    let mut commands = Vec::with_capacity(512);
    for _ in 0..1_000 {
        drill.commands_for(&world, &mut commands);
        COUNTING.set(true);
        world.tick(&commands);
        let _ = world.state_hash();
        COUNTING.set(false);
    }
    assert_eq!(
        ALLOCS.load(Ordering::SeqCst),
        0,
        "World::tick or state_hash allocated"
    );
}
