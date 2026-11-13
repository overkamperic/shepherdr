//! [real programs] A job the board starts and leaves to run (a runner, a loose job, the notify command) is no child of
//! the board: when it ends, no dead process (a zombie) stays behind waiting to be collected. keeper-work, which runs
//! for days and never collects its children, once held 57,267 of them after 8 days, until the machine could start no
//! program at all. Linux only (it reads /proc).
use crate::common;
use std::path::Path;
use std::time::{Duration, Instant};

const PROBE: &str = "hplr-probe";
/// The name the kernel keeps for a process (its first 15 bytes), as /proc/<pid>/stat shows it.
fn comm_of(p: &Path) -> String { p.file_name().unwrap().to_string_lossy().chars().take(15).collect() }
/// One process's (name, state, parent, session) from /proc/<pid>/stat.
fn stat(pid: i32) -> Option<(String, char, i32, i32)> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (head, rest) = s.rsplit_once(')')?;
    let name = head.split_once('(')?.1.to_string();
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some((name, f.first()?.chars().next()?, f.get(1)?.parse().ok()?, f.get(3)?.parse().ok()?))
}
/// This process's children (of every thread) that these jobs are: by their name, with their state.
fn ours(names: &[String]) -> Vec<(i32, char)> {
    let mut out = vec![];
    for t in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
        for pid in std::fs::read_to_string(t.path().join("children")).unwrap_or_default().split_whitespace().filter_map(|p| p.parse::<i32>().ok()) {
            if let Some((name, state, _, _)) = stat(pid) { if names.contains(&name) { out.push((pid, state)); } }
        }
    }
    out
}
fn until(ms: u64, done: impl Fn() -> bool) -> bool { let end = Instant::now() + Duration::from_millis(ms); while Instant::now() < end { if done() { return true; } std::thread::sleep(Duration::from_millis(20)); } done() }
fn linux() -> bool { Path::new("/proc/self/task").is_dir() && std::fs::read_dir("/proc/self/task").unwrap().flatten().any(|t| t.path().join("children").exists()) }

#[test]
fn real_programs_detached_jobs_leave_no_zombies() {
    if !linux() { eprintln!("skipped: no /proc/<pid>/task/<tid>/children here"); return; }
    let w = common::world();
    // The jobs run for real here, not in the test's own queue.
    unsafe { std::env::remove_var("HERDR_PROJECTS_INLINE") };
    let probe = w.base.join(PROBE);
    std::fs::copy("/bin/true", &probe).unwrap();
    // The runner and the loose job are this test program itself: `--list` only lists its tests and ends.
    let me = comm_of(&std::env::current_exe().unwrap());
    let names = vec![comm_of(&probe), me];
    let mut pids = vec![];
    for i in 0..3 {
        sw::runner::detached_sh(&format!("touch '{}/done.{i}'; exec '{}'", w.base.display(), probe.display()), &[]);
        pids.push(sw::runner::spawn_runner("--list", "-", &[]));
        pids.push(sw::housekeeping::spawn_loose(&["--list"]));
    }
    // Every job ended: each notify command touched its file, and none of them runs any more.
    let ended = until(10000, || (0..3).all(|i| w.base.join(format!("done.{i}")).exists())
        && ours(&names).iter().all(|(_, s)| *s == 'Z')
        && pids.iter().all(|p| stat(*p as i32).map(|(_, s, _, _)| s == 'Z').unwrap_or(true)));
    unsafe { std::env::set_var("HERDR_PROJECTS_INLINE", "1") };
    assert!(ended, "the jobs did not end in 10 s: {:?}", ours(&names));
    assert!(pids.iter().all(|p| *p != 0), "every runner and loose job started and has its process number: {pids:?}");
    let left = ours(&names);
    assert!(left.is_empty(), "dead jobs still wait under the board to be collected (zombies): {left:?}");
}

#[test]
fn real_programs_a_detached_job_keeps_its_own_pid_and_session() {
    if !linux() { eprintln!("skipped: no /proc here"); return; }
    // The pid a runner's marker holds, and what stopping a runner signals (its whole group, `kill(-pid)`), is the job's.
    let pid = sw::runner::spawn_detached(std::process::Command::new("sleep").arg("30").stdin(std::process::Stdio::null())) as i32;
    assert!(pid > 0, "the job started");
    let seen = until(5000, || stat(pid).map(|(n, ..)| n == "sleep").unwrap_or(false));
    let st = stat(pid);
    unsafe { libc::kill(pid, libc::SIGKILL) };
    assert!(seen, "the pid returned is the job itself: {st:?}");
    let (_, _, parent, session) = st.unwrap();
    assert_ne!(parent, std::process::id() as i32, "the job is no child of the board's process");
    assert_eq!(session, pid, "the job leads a session of its own");
}
