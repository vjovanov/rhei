//! Linux liveness when a dead headless supervisor leaves its run lock held.
//!
//! This uses matching real descriptors and the public `rhei runs` command:
//! contention says somebody holds the lock, while exact ownership says whether
//! that somebody is the recorded supervisor. §FS-rhei-run-headless.3

#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::headless_support::Workspace;
use super::python_command;
use super::{stderr, stdout};

struct RetainedRunLock {
    holder_pid: u32,
    release: PathBuf,
}

impl Drop for RetainedRunLock {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, []);
        let process = PathBuf::from(format!("/proc/{}", self.holder_pid));
        for _ in 0..100 {
            if !process.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        unsafe { kill_process(self.holder_pid as i32, 9) };
    }
}

/// The recorded process stamps the real run lock, forks a child that inherits
/// that open description, then exits. The guard releases the surviving child.
fn retain_lock_after_recorded_process_exits(ws: &Workspace, id: &str) -> (u32, RetainedRunLock) {
    let lock = ws.root.join(".rhei/run.lock");
    fs::create_dir_all(lock.parent().expect("lock directory")).expect("lock directory");
    let holder_file = ws.root.join("retained-lock-holder.pid");
    let release = ws.root.join("release-retained-lock");
    let code = r#"import fcntl,json,os,pathlib,signal,sys,time
lock,workspace,run_id,holder_file,release=sys.argv[1:]
f=open(lock,'w+')
fcntl.flock(f,fcntl.LOCK_EX)
fields=open('/proc/self/stat').read().rsplit(') ',1)[1].split()
json.dump({'version':1,'id':run_id,'pid':os.getpid(),'workspace':workspace,'process_start_ticks':int(fields[19])},f)
f.write('\n')
f.flush()
os.fsync(f.fileno())
holder=os.fork()
if holder == 0:
    os.close(1)
    os.close(2)
    signal.signal(signal.SIGHUP,signal.SIG_IGN)
    signal.signal(signal.SIGTERM,signal.SIG_IGN)
    signal.signal(signal.SIGINT,signal.SIG_IGN)
    while not pathlib.Path(release).exists(): time.sleep(0.01)
    os._exit(0)
pathlib.Path(holder_file).write_text(str(holder))
os._exit(0)
"#;
    let runner = Command::new(python_command())
        .arg("-c")
        .arg(code)
        .arg(&lock)
        .arg(&*ws.root)
        .arg(id)
        .arg(&holder_file)
        .arg(&release)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn recorded lock owner");
    let recorded_pid = runner.id();
    let output = runner.wait_with_output().expect("wait for recorded process to exit");
    assert!(
        output.status.success(),
        "recorded process fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let holder_pid = fs::read_to_string(&holder_file)
        .expect("retained holder pid")
        .parse()
        .expect("numeric retained holder pid");
    assert!(
        !Path::new(&format!("/proc/{recorded_pid}")).exists(),
        "the recorded supervisor must be gone before rhei runs"
    );
    (recorded_pid, RetainedRunLock { holder_pid, release })
}

fn publish_running_descriptor(ws: &Workspace, id: &str, pid: u32) {
    let descriptor = serde_json::json!({
        "id": id,
        "pid": pid,
        "status": "running",
        "workspace": &*ws.root,
        "plan": ws.plan(),
        "started_at": "2026-09-10T00:00:00Z",
        "headless": true,
        "parallel": 1,
        "log": ws.root.join("runtime/run.log"),
        "events": ws.root.join("runtime/events.jsonl"),
        "exit_code": null,
    });
    let body = serde_json::to_string_pretty(&descriptor).expect("render descriptor");
    fs::create_dir_all(ws.root.join("runtime")).expect("runtime directory");
    fs::write(ws.root.join("runtime/run.json"), &body).expect("workspace descriptor");
    let registry = ws.home.join("state/rhei/runs");
    fs::create_dir_all(&registry).expect("registry directory");
    fs::write(registry.join(format!("{id}.json")), body).expect("registry descriptor");
}

fn registry_path(ws: &Workspace, id: &str) -> PathBuf {
    ws.home.join("state/rhei/runs").join(format!("{id}.json"))
}

unsafe fn kill_process(pid: i32, signal: i32) {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    kill(pid, signal);
}

/// Contention proves only that somebody retained the current lock pathname.
/// Once its recorded supervisor is gone, neither public listing may call the
/// matching non-terminal descriptor live, and its retained entry still
/// resolves as ended. §FS-rhei-run-headless.3
#[test]
fn runs_ends_a_dead_supervisor_when_a_child_retains_the_current_lock() {
    let ws = Workspace::new("headless-retained-lock", 0);
    let id = "dead42";
    let (recorded_pid, _retained_lock) = retain_lock_after_recorded_process_exits(&ws, id);
    publish_running_descriptor(&ws, id, recorded_pid);

    let text = ws.rhei(&["runs"]);
    let json = ws.rhei(&["runs", "--json"]);
    assert!(text.status.success(), "text listing failed: {}", stderr(&text));
    assert!(json.status.success(), "JSON listing failed: {}", stderr(&json));
    let entries: serde_json::Value =
        serde_json::from_str(&stdout(&json)).expect("JSON listing is an array");
    let live_in_json =
        entries.as_array().expect("JSON listing is an array").iter().any(|entry| entry["id"] == id);
    assert!(
        !stdout(&text).contains(id) && !live_in_json,
        "dead recorded supervisor {recorded_pid} remained live\ntext:\n{}\njson:\n{}",
        stdout(&text),
        stdout(&json)
    );

    let entry = registry_path(&ws, id);
    assert!(entry.is_file(), "a decided end remains retained");
    let attached = ws.rhei(&["attach", id]);
    assert!(attached.status.success(), "retained entry no longer resolves: {}", stderr(&attached));
    assert!(stdout(&attached).contains("has ended"), "got: {}", stdout(&attached));
}
