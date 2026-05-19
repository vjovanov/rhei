use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::event::{EventSink, RunEvent};

mod html;
mod state;

use html::DASHBOARD_HTML;
use state::{derive_plan_state, DashboardLink, DashboardState, SnapshotPayload, TaskRow};

const RECENT_LIMIT: usize = 200;
const SLOT_TRAFFIC_LIMIT: usize = 60;

/// One task entry surfaced in the Tasks tab. Constructed by the caller's
/// plan loader closure on demand so the dashboard never has to depend on
/// `rhei-core` directly.
#[derive(Clone, Debug, Serialize)]
pub struct DashboardTask {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub parent: Option<String>,
    pub depth: u8,
    pub state: String,
    pub assignee: Option<String>,
    pub prior: Vec<String>,
    pub result_link: Option<String>,
}

/// Snapshot returned by the plan loader: plan title plus the flattened task
/// list. Loader can return `None` if the plan cannot be re-read (file moved,
/// transient parse error during a write); the dashboard keeps the previous
/// snapshot in that case.
#[derive(Clone, Debug)]
pub struct PlanSnapshot {
    pub title: String,
    pub tasks: Vec<DashboardTask>,
}

/// Closure invoked on every `/snapshot` request to refresh the Tasks tab.
///
/// Wrapped in `Arc` rather than `Box` so the snapshot handler can keep a
/// reference without holding a mutex during the call (which would serialise
/// concurrent dashboard tabs behind a slow plan parse).
pub type PlanLoader = Arc<dyn Fn() -> Option<PlanSnapshot> + Send + Sync>;

pub struct DashboardSink {
    url: String,
    state: Arc<Mutex<DashboardState>>,
    stop: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl DashboardSink {
    /// Backwards-compatible constructor: no plan loader, Tasks tab will show
    /// "(no plan loaded)".
    pub fn start(workspace: PathBuf, parallel: u16, total_tasks: usize) -> io::Result<Self> {
        Self::start_with_plan(workspace, parallel, total_tasks, None)
    }

    /// Constructor with an optional plan loader. The loader is called on every
    /// `/snapshot` request and must be cheap (it parses a markdown file). Pass
    /// `None` to opt out of the Tasks tab.
    pub fn start_with_plan(
        workspace: PathBuf,
        parallel: u16,
        total_tasks: usize,
        plan_loader: Option<PlanLoader>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let state =
            Arc::new(Mutex::new(DashboardState::new(workspace, parallel.max(1), total_tasks)));
        let stop = Arc::new(AtomicBool::new(false));
        // The loader is set once at construction and never mutated, so no
        // mutex is required around it. `last_plan` does need a mutex because
        // it caches a value that's overwritten each request.
        let plan = plan_loader;
        let last_plan = Arc::new(Mutex::new(None));

        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let handle =
            thread::spawn(move || serve(listener, thread_state, plan, last_plan, thread_stop));

        Ok(Self { url, state, stop, join: Mutex::new(Some(handle)) })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn finish(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let mut guard = match self.join.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(handle) = guard.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DashboardSink {
    fn drop(&mut self) {
        self.finish();
    }
}

impl EventSink for DashboardSink {
    fn emit(&self, event: RunEvent) {
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        state.apply(&event);
    }
}

fn serve(
    listener: TcpListener,
    state: Arc<Mutex<DashboardState>>,
    plan: Option<PlanLoader>,
    last_plan: Arc<Mutex<Option<PlanSnapshot>>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => handle_client(stream, &state, plan.as_ref(), &last_plan),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

fn handle_client(
    mut stream: TcpStream,
    state: &Arc<Mutex<DashboardState>>,
    plan: Option<&PlanLoader>,
    last_plan: &Arc<Mutex<Option<PlanSnapshot>>>,
) {
    let mut buf = [0u8; 2048];
    let Ok(n) = stream.read(&mut buf) else {
        return;
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let path =
        request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/");

    match path {
        "/" => write_response(&mut stream, "text/html; charset=utf-8", DASHBOARD_HTML.as_bytes()),
        "/snapshot" => {
            let snapshot_state = match state.lock() {
                Ok(s) => s.clone(),
                Err(p) => p.into_inner().clone(),
            };
            // Refresh the plan view. If the loader is missing or returns
            // None (e.g. transient parse error during a write), reuse the
            // last good snapshot rather than blanking the Tasks tab.
            let fresh = plan.and_then(|loader| loader());
            let plan_snapshot = if let Some(s) = fresh {
                if let Ok(mut last) = last_plan.lock() {
                    *last = Some(s.clone());
                }
                Some(s)
            } else {
                last_plan.lock().ok().and_then(|g| g.clone())
            };

            let auto_links = derive_auto_links(&snapshot_state.workspace);
            let (plan_title, plan_state, tasks) = match plan_snapshot {
                Some(p) => {
                    let plan_state = derive_plan_state(&p.tasks);
                    let active_tasks: HashSet<&str> =
                        snapshot_state.slots.iter().filter_map(|s| s.task.as_deref()).collect();
                    let deferred_set: HashSet<&str> =
                        snapshot_state.deferred.iter().map(|s| s.as_str()).collect();
                    let rows = p
                        .tasks
                        .into_iter()
                        .map(|task| {
                            let in_slot =
                                snapshot_state.slots.iter().enumerate().find_map(|(i, slot)| {
                                    slot.task
                                        .as_deref()
                                        .filter(|t| *t == task.id && active_tasks.contains(t))
                                        .map(|_| i as u16)
                                });
                            let deferred_this_pass = deferred_set.contains(task.id.as_str());
                            TaskRow { task, in_slot, deferred_this_pass }
                        })
                        .collect();
                    (Some(p.title), Some(plan_state), rows)
                }
                None => (None, None, Vec::new()),
            };

            let payload = SnapshotPayload {
                state: &snapshot_state,
                plan_title,
                plan_state,
                tasks,
                auto_links,
            };
            match serde_json::to_vec(&payload) {
                Ok(body) => write_response(&mut stream, "application/json", &body),
                Err(err) => write_response(
                    &mut stream,
                    "application/json",
                    format!(r#"{{"error":"{}"}}"#, escape_json_string(&err.to_string())).as_bytes(),
                ),
            }
        }
        _ => write_not_found(&mut stream),
    }
}

fn derive_auto_links(workspace: &str) -> Vec<DashboardLink> {
    let mut out = Vec::new();
    let encoded_root = encode_url_path(workspace);
    let push = |out: &mut Vec<DashboardLink>, label: &str, suffix: &str| {
        let url = if suffix.is_empty() {
            format!("file://{encoded_root}")
        } else {
            // `suffix` is a fixed ASCII relative path; encoding is a no-op
            // but keeps the construction symmetric.
            format!("file://{encoded_root}/{}", encode_url_path(suffix))
        };
        out.push(DashboardLink { label: label.to_string(), url, source: "workspace" });
    };
    push(&mut out, "Workspace", "");
    push(&mut out, "Runtime logs", "runtime/logs");
    push(&mut out, "Runtime results", "runtime/results");
    out
}

/// Percent-encode a filesystem path for embedding in a `file://` URL.
///
/// Preserves RFC 3986 path-safe bytes (unreserved + sub-delims + `:` `@` `/`)
/// and percent-encodes everything else, including spaces, `#`, `?`, and any
/// non-ASCII byte. Operates on raw bytes so non-UTF-8 paths are still
/// representable.
fn encode_url_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        let safe = matches!(
            byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
                | b'/'
        );
        if safe {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", byte));
        }
    }
    out
}

fn write_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn write_not_found(stream: &mut TcpStream) {
    let body = b"not found";
    let _ = write!(
        stream,
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn now_ms() -> u128 {
    system_time_ms(SystemTime::now())
}

fn system_time_ms(value: SystemTime) -> u128 {
    value.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests;
