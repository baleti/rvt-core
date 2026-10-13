//! Worker pool per process: rayon's global pool does not survive fork() (its threads are gone in the child, so the first parallel call there waits
//! forever: viewer item 12). Every parallel section runs through run(), which builds a fresh pool when the process id changed.
use std::sync::{Arc, Mutex};

static POOL: Mutex<Option<(u32, Arc<rayon::ThreadPool>)>> = Mutex::new(None);

/// worker threads: RVT_THREADS, default min(6, cpus)
pub fn threads() -> usize {
    std::env::var("RVT_THREADS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(6)).max(1)
}

fn get() -> Arc<rayon::ThreadPool> {
    let pid = std::process::id();
    let mut g = POOL.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((p, pool)) = g.as_ref() { if *p == pid { return pool.clone(); } }
    // a forked child inherits the parent's (dead) pool object: leak it (dropping would try to join threads that do not exist here)
    if let Some(old) = g.take() { std::mem::forget(old); }
    let pool = Arc::new(rayon::ThreadPoolBuilder::new().num_threads(threads()).build().expect("rayon pool"));
    *g = Some((pid, pool.clone()));
    pool
}

/// run f inside this process's pool (parallel iterators inside use it)
pub fn run<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    if rayon::current_thread_index().is_some() { return f(); }      // already on a pool thread (nested section)
    get().install(f)
}
