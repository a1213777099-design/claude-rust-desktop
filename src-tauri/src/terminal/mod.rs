use anyhow::Result;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, PtySystem, native_pty_system};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::{broadcast, watch, Mutex};

#[derive(Debug, Clone, Serialize)]
pub struct PtySession {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub created_at: String,
    pub pid: Option<u32>,
    pub running: bool,
    pub cols: u16,
    pub rows: u16,
}

struct PtyRuntime {
    session: PtySession,
    master: Box<dyn MasterPty + Send>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    output_tx: broadcast::Sender<String>,
    exit_tx: watch::Sender<Option<u32>>,
    /// Keep at least one receiver alive so `output_tx.send()` never fails:
    /// otherwise the reader thread would break out and die before any SSE
    /// stream connects (which happens after the session is created).
    #[allow(dead_code)]
    _keepalive_rx: broadcast::Receiver<String>,
}

pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<String, PtyRuntime>>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new PTY session and spawn a shell into it.
    pub async fn create_session(
        &self,
        cwd: Option<String>,
        shell: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> Result<PtySession> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let cwd = cwd.unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });

        let shell = if cfg!(target_os = "windows") {
            shell.unwrap_or_else(|| "cmd.exe".to_string())
        } else {
            shell.unwrap_or_else(|| "/bin/bash".to_string())
        };

        let cols = cols.unwrap_or(80);
        let rows = rows.unwrap_or(24);

        // Open PTY (a fresh system instance per session keeps PtyManager Send+Sync)
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Build command
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&cwd);

        // Spawn shell into the PTY
        let child = pair.slave.spawn_command(cmd)?;
        let pid = child.process_id();
        let killer = child.clone_killer();

        // Get reader/writer
        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let writer_arc = Arc::new(StdMutex::new(writer));

        // Output broadcast channel.
        // `keepalive_rx` stays alive inside PtyRuntime so that sends always
        // succeed (messages are buffered) even before any SSE stream connects.
        let (output_tx, keepalive_rx) = broadcast::channel(1024);

        // Exit watch channel
        let (exit_tx, _) = watch::channel(None);

        // Start reader thread: reads PTY output and broadcasts it
        let output_tx_clone = output_tx.clone();
        let exit_tx_clone = exit_tx.clone();
        let writer_clone = writer_arc.clone();
        let reader_id = session_id.clone();
        std::thread::Builder::new()
            .name(format!("pty-reader-{}", crate::truncate::safe_truncate(&reader_id, 8)))
            .spawn(move || {
                let mut buf = vec![0u8; 8192];
                let mut reader = reader;
                let mut buf_acc = Vec::new();
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            buf_acc.extend_from_slice(&buf[..n]);
                            // Try to decode as UTF-8, replace invalid sequences
                            let s = String::from_utf8_lossy(&buf_acc).to_string();
                            // Detect DSR (Device Status Report) cursor position query: \x1b[6n
                            // cmd.exe sends this on startup and waits for a response.
                            // If we don't reply, it hangs waiting and won't process any input.
                            if s.contains("\x1b[6n") {
                                // Reply with default position: row 1, column 1
                                let reply = b"\x1b[1;1R";
                                if let Ok(mut w) = writer_clone.lock() {
                                    let _ = w.write_all(reply);
                                    let _ = w.flush();
                                }
                            }
                            // Never break on a failed send: it only means no SSE
                            // subscriber is connected yet (a keepalive receiver is
                            // held in PtyRuntime, so sends normally succeed).
                            let _ = output_tx_clone.send(s);
                            buf_acc.clear();
                        }
                        Err(e) => {
                            if e.kind() != std::io::ErrorKind::Interrupted {
                                break;
                            }
                        }
                    }
                }
                // PTY closed, send exit signal
                let _ = exit_tx_clone.send(Some(0));
            })?;

        // Start exit watcher: waits for child process to exit
        let exit_tx_watcher = exit_tx.clone();
        let killer_for_watcher = killer.clone_killer();
        tokio::spawn(async move {
            // Wait for child exit (blocking)
            let result = tokio::task::spawn_blocking(move || {
                // We can't call wait() on the child directly because it was
                // moved into the reader thread. Instead, we poll using the killer.
                // The reader thread will detect EOF and send exit signal.
                // For additional safety, we can also monitor the process handle.
                let _ = killer_for_watcher;
            })
            .await;
            let _ = result;
        });

        let session = PtySession {
            id: session_id.clone(),
            shell: shell.clone(),
            cwd: cwd.clone(),
            created_at: now,
            pid,
            running: true,
            cols,
            rows,
        };

        let runtime = PtyRuntime {
            session: session.clone(),
            master: pair.master,
            writer: writer_arc,
            killer,
            output_tx,
            exit_tx: exit_tx_watcher,
            _keepalive_rx: keepalive_rx,
        };

        self.sessions.lock().await.insert(session_id, runtime);

        Ok(session)
    }

    /// Write data to the PTY input.
    pub async fn write_input(&self, session_id: &str, data: &str) -> Result<()> {
        let sessions = self.sessions.lock().await;
        if let Some(rt) = sessions.get(session_id) {
            let mut writer = rt.writer.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
            writer.write_all(data.as_bytes())?;
            writer.flush()?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found: {}", session_id))
        }
    }

    /// Resize the PTY window.
    pub async fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        if let Some(rt) = sessions.get_mut(session_id) {
            rt.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            // Update session metadata
            rt.session.cols = cols;
            rt.session.rows = rows;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found: {}", session_id))
        }
    }

    /// Close a PTY session: kill the child process and remove the session.
    pub async fn close_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        if let Some(mut rt) = sessions.remove(session_id) {
            // Kill the child process
            let _ = rt.killer.kill();
            // Notify exit
            let _ = rt.exit_tx.send(Some(127));
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found: {}", session_id))
        }
    }

    /// List all active sessions.
    pub async fn list_sessions(&self) -> Vec<PtySession> {
        self.sessions
            .lock()
            .await
            .values()
            .map(|rt| {
                let mut s = rt.session.clone();
                // Check if the session has exited
                if rt.exit_tx.borrow().is_some() {
                    s.running = false;
                }
                s
            })
            .collect()
    }

    /// Subscribe to PTY output and exit for a session.
    /// Returns (output receiver, exit receiver, session info) if the session exists.
    pub async fn get_stream(
        &self,
        session_id: &str,
    ) -> Option<(broadcast::Receiver<String>, watch::Receiver<Option<u32>>, PtySession)> {
        let sessions = self.sessions.lock().await;
        let rt = sessions.get(session_id)?;
        let mut s = rt.session.clone();
        if rt.exit_tx.borrow().is_some() {
            s.running = false;
        }
        Some((rt.output_tx.subscribe(), rt.exit_tx.subscribe(), s))
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a one-shot bash command and return its output.
/// This is separate from the interactive PTY sessions.
pub async fn execute_bash_command(
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    env_vars: Option<HashMap<String, String>>,
) -> Result<String> {
    let cwd = cwd.unwrap_or(".");
    let is_win = cfg!(target_os = "windows");

    let mut cmd = if is_win {
        let mut c = tokio::process::Command::new("cmd.exe");
        c.args(["/C", command]);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        c
    } else {
        let mut c = tokio::process::Command::new("bash");
        c.args(["-c", command]);
        c
    };

    cmd.current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    if let Some(env) = env_vars {
        let envs: Vec<(String, String)> = env.into_iter().chain([
            ("LANG".to_string(), "en_US.UTF-8".to_string()),
            ("TERM".to_string(), "xterm-256color".to_string()),
        ]).collect();
        cmd.envs(envs);
    } else {
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("TERM", "xterm-256color");
    }

    let timeout = tokio::time::Duration::from_secs(timeout_secs);

    let result = tokio::time::timeout(timeout, cmd.output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut result = stdout.to_string();
            if !stderr.is_empty() {
                result.push_str(&format!("\nSTDERR: {}", stderr));
            }

            if !output.status.success() && stdout.is_empty() {
                result = format!("Command exited with code {:?}: {}", output.status.code(), stderr);
            }

            Ok(result)
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("Failed to execute command: {}", e)),
        Err(_) => Err(anyhow::anyhow!("Command timed out after {} seconds", timeout_secs)),
    }
}