//! Thread spawning that keeps memory tags: a new thread starts untagged
//! (charged to `other`), so work handed to a worker must say whose it is.

use std::thread::{self, JoinHandle, Scope, ScopedJoinHandle};

use crate::tagged_alloc::{self, Tag};

/// Like [`std::thread::spawn`], but the new thread's allocations are charged
/// to `tag` for as long as it runs.
pub fn spawn_tagged<T, F>(tag: Tag, f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::spawn(move || tagged_alloc::scope(tag, f))
}

/// Like [`std::thread::Scope::spawn`], but the new thread's allocations are
/// charged to `tag` for as long as it runs.
pub fn spawn_scoped_tagged<'scope, 'env, T, F>(
    s: &'scope Scope<'scope, 'env>,
    tag: Tag,
    f: F,
) -> ScopedJoinHandle<'scope, T>
where
    F: FnOnce() -> T + Send + 'scope,
    T: Send + 'scope,
{
    s.spawn(move || tagged_alloc::scope(tag, f))
}

/// How many worker threads a background job may use: the machine's cores
/// less `spare` (kept free for the render and sim threads), and at least 1.
pub fn workers_leaving(spare: usize) -> usize {
    thread::available_parallelism().map_or(1, |n| worker_count(n.get(), spare))
}

fn worker_count(cores: usize, spare: usize) -> usize {
    cores.saturating_sub(spare).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workers_leave_cores_free_but_never_none() {
        assert_eq!(worker_count(8, 2), 6);
        assert_eq!(worker_count(3, 2), 1);
        assert_eq!(worker_count(2, 2), 1);
        assert_eq!(worker_count(1, 2), 1);
    }

    #[test]
    fn spawned_threads_run_and_return() {
        assert_eq!(spawn_tagged(Tag::Ai, || 7).join().unwrap(), 7);
        let n = thread::scope(|s| spawn_scoped_tagged(s, Tag::Ai, || 9).join().unwrap());
        assert_eq!(n, 9);
    }
}
