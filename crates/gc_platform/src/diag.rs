//! Diagnostics for shipped builds: a rotating log file and crash reports.
//!
//! A friend's crash on a Finder-launched Mac or a double-clicked Windows exe
//! leaves nothing on screen and nothing in a terminal. [`init`] fixes that:
//! the game's log lines ([`logln!`](crate::logln)) go to stderr and to
//! `<dir>/logs/grubcraft.log`, and a panic writes a report with the message,
//! location, backtrace and a few facts about the run (build, data hash, GPU,
//! operating system, simulation tick) to `<dir>/crashes/`. Release builds
//! abort on panic, but the panic hook runs first, so the report is written
//! either way.

use crate::logln;
use std::backtrace::Backtrace;
use std::fmt::{Arguments, Write as _};
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Log files kept: `grubcraft.log` is this run, `grubcraft.1.log` the run
/// before, and so on.
const LOG_FILES: usize = 4;
/// Crash reports kept; older ones are deleted.
const CRASH_REPORTS: usize = 20;

static STATE: OnceLock<State> = OnceLock::new();
static TICK: AtomicU64 = AtomicU64::new(0);

struct State {
    dir: PathBuf,
    started: Instant,
    /// Unbuffered on purpose: release builds abort on panic, so anything
    /// held back in a buffer would be lost.
    log: Mutex<Option<File>>,
    context: Mutex<Vec<(&'static str, String)>>,
}

/// Start the log file and install the panic hook. `dir` is the game's
/// per-user folder; the log and the crash reports go in subfolders of it.
/// Call once, first thing in `main`. A folder that cannot be written only
/// turns the file log and crash reports off; the game still runs.
pub fn init(dir: &Path) {
    let mut log = None;
    let logs = dir.join("logs");
    if fs::create_dir_all(&logs).is_ok() {
        rotate(&logs);
        log = File::create(logs.join("grubcraft.log")).ok();
    }
    let state = State {
        dir: dir.to_path_buf(),
        started: Instant::now(),
        log: Mutex::new(log),
        context: Mutex::new(Vec::new()),
    };
    if STATE.set(state).is_err() {
        return;
    }
    logln!("log started, unix time {}", unix_now());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        on_panic(info);
        previous(info);
    }));
}

/// Record a fact for crash reports, replacing the earlier value for `key`
/// (build, data hash, GPU, ...).
pub fn set_context(key: &'static str, value: impl Into<String>) {
    let Some(state) = STATE.get() else { return };
    let value = value.into();
    if let Ok(mut ctx) = state.context.lock() {
        match ctx.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => ctx.push((key, value)),
        }
    }
}

/// The simulation tick, kept up to date so a crash report says how far the
/// battle had got.
pub fn set_tick(tick: u64) {
    TICK.store(tick, Ordering::Relaxed);
}

/// Write one line to stderr and to the log file, prefixed with the seconds
/// since the game started. Use through [`logln!`](crate::logln).
pub fn log(args: Arguments<'_>) {
    eprintln!("{args}");
    let Some(state) = STATE.get() else { return };
    let t = state.started.elapsed().as_secs_f32();
    // One write a line. `try_lock`: a line from inside the panic hook must
    // not deadlock on a lock the panicking thread holds.
    if let Ok(mut file) = state.log.try_lock()
        && let Some(f) = file.as_mut()
    {
        // A full disk is not worth interrupting the game for.
        let _ = f.write_all(format!("[{t:8.2}] {args}\n").as_bytes());
    }
}

/// `eprintln!` that also goes to the log file. Defined here so every crate
/// can use it as `gc_platform::logln!`.
#[macro_export]
macro_rules! logln {
    ($($arg:tt)*) => {
        $crate::diag::log(::std::format_args!($($arg)*))
    };
}

/// Shift `grubcraft.log` to `grubcraft.1.log`, and so on, dropping the
/// oldest.
fn rotate(logs: &Path) {
    let name = |i: usize| match i {
        0 => logs.join("grubcraft.log"),
        i => logs.join(format!("grubcraft.{i}.log")),
    };
    // Renaming over an existing file replaces it on every platform we ship
    // to except Windows, so remove the target first.
    for i in (0..LOG_FILES - 1).rev() {
        if name(i).exists() {
            let _ = fs::remove_file(name(i + 1));
            let _ = fs::rename(name(i), name(i + 1));
        }
    }
}

/// Seconds since 1970 now.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn on_panic(info: &std::panic::PanicHookInfo<'_>) {
    let Some(state) = STATE.get() else { return };
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".into());
    let location = info
        .location()
        .map_or_else(|| "unknown".into(), ToString::to_string);
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("unnamed");
    let unix_time = unix_now();
    let backtrace = Backtrace::force_capture().to_string();
    let context = state
        .context
        .try_lock()
        .map(|c| c.clone())
        .unwrap_or_default();
    let report = crash_report(&CrashInfo {
        message: &message,
        location: &location,
        thread,
        backtrace: &backtrace,
        context: &context,
        tick: TICK.load(Ordering::Relaxed),
        unix_time,
        log_file: &state.dir.join("logs").join("grubcraft.log"),
    });
    log(format_args!(
        "PANIC on thread {thread} at {location}: {message}"
    ));
    match write_report(&state.dir.join("crashes"), unix_time, &report) {
        Ok(path) => logln!("crash report saved to {}", path.display()),
        Err(e) => logln!("could not save a crash report: {e}"),
    }
}

struct CrashInfo<'a> {
    message: &'a str,
    location: &'a str,
    thread: &'a str,
    backtrace: &'a str,
    context: &'a [(&'static str, String)],
    tick: u64,
    unix_time: u64,
    log_file: &'a Path,
}

fn crash_report(c: &CrashInfo<'_>) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "Grubcraft crash report");
    let _ = writeln!(s, "unix time: {}", c.unix_time);
    let _ = writeln!(
        s,
        "system: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for (k, v) in c.context {
        let _ = writeln!(s, "{k}: {v}");
    }
    let _ = writeln!(s, "last simulation tick: {}", c.tick);
    let _ = writeln!(s, "thread: {}", c.thread);
    let _ = writeln!(s, "location: {}", c.location);
    let _ = writeln!(s, "message: {}", c.message);
    let _ = writeln!(s, "log file: {}", c.log_file.display());
    let _ = writeln!(s, "\nbacktrace:\n{}", c.backtrace);
    s
}

/// Save `report` as `crash-<unix time>.txt` in `dir`, then delete all but
/// the newest few reports.
fn write_report(dir: &Path, unix_time: u64, report: &str) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let mut path = dir.join(format!("crash-{unix_time}.txt"));
    // Two crashes in one second (several threads) keep both.
    for n in 1.. {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("crash-{unix_time}-{n}.txt"));
    }
    fs::write(&path, report)?;
    let mut old: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .collect();
    // Names carry the time, so sorting by name sorts by age (the digits are
    // the same width until the year 2286).
    old.sort();
    let excess = old.len().saturating_sub(CRASH_REPORTS);
    for p in &old[..excess] {
        let _ = fs::remove_file(p);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gc_diag_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rotation_keeps_the_newest_logs() {
        let dir = temp("rotate");
        for run in 0..6 {
            rotate(&dir);
            fs::write(dir.join("grubcraft.log"), format!("run {run}")).unwrap();
        }
        let read = |n: &str| fs::read_to_string(dir.join(n)).unwrap();
        assert_eq!(read("grubcraft.log"), "run 5");
        assert_eq!(read("grubcraft.1.log"), "run 4");
        assert_eq!(read("grubcraft.2.log"), "run 3");
        assert_eq!(read("grubcraft.3.log"), "run 2");
        assert!(!dir.join("grubcraft.4.log").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_names_what_was_going_on() {
        let context = [("build", "0.1.0 (abc1234)".to_string())];
        let text = crash_report(&CrashInfo {
            message: "boom",
            location: "src/main.rs:1:1",
            thread: "main",
            backtrace: "frame 0",
            context: &context,
            tick: 1234,
            unix_time: 99,
            log_file: Path::new("/x/grubcraft.log"),
        });
        for want in [
            "build: 0.1.0 (abc1234)",
            "last simulation tick: 1234",
            "message: boom",
            "location: src/main.rs:1:1",
            "frame 0",
            "/x/grubcraft.log",
        ] {
            assert!(text.contains(want), "{want} missing from:\n{text}");
        }
    }

    #[test]
    fn old_crash_reports_are_pruned() {
        let dir = temp("prune");
        for t in 0..(CRASH_REPORTS as u64 + 5) {
            write_report(&dir, 1_000 + t, "x").unwrap();
        }
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), CRASH_REPORTS);
        // The oldest five are gone.
        assert!(!names.contains(&"crash-1000.txt".to_string()));
        assert!(names.contains(&format!("crash-{}.txt", 1_000 + CRASH_REPORTS as u64 + 4)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_crashes_in_one_second_keep_both() {
        let dir = temp("twice");
        let a = write_report(&dir, 5, "a").unwrap();
        let b = write_report(&dir, 5, "b").unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read_to_string(a).unwrap(), "a");
        assert_eq!(fs::read_to_string(b).unwrap(), "b");
        let _ = fs::remove_dir_all(&dir);
    }
}
