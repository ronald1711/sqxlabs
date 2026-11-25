//! Local process observations use held OS file locks, never a leftover PID alone.
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
pub struct Lease {
    file: File,
}
impl Lease {
    pub fn acquire(directory: &Path) -> Result<Self, String> {
        let dir = directory.join("processes");
        fs::create_dir_all(&dir).map_err(|_| "Cannot create MCP process status directory")?;
        // Stale files carry no historical evidence (the event log does). Remove only
        // our numeric status files while holding their exclusive lock.
        for path in candidates(directory)? {
            let mut opts = OpenOptions::new();
            opts.read(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.custom_flags(libc::O_NOFOLLOW);
            }
            if let Ok(file) = opts.open(&path) {
                if file.try_lock().is_ok() {
                    let _ = fs::remove_file(path);
                }
            }
        }
        let path = dir.join(format!("{}.json", std::process::id()));
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options
            .open(path)
            .map_err(|_| "Cannot open MCP process status")?;
        file.try_lock()
            .map_err(|_| "Cannot lock MCP process status")?;
        Ok(Self { file })
    }
    pub fn update(
        &mut self,
        configuration_id: Option<&str>,
        session: &str,
        client_kind: &str,
        initialized: bool,
        logging_degraded: bool,
    ) -> Result<(), String> {
        let value = json!({"schema":1,"pid":std::process::id(),"configuration_id":configuration_id,"session":session,"client_kind":client_kind,"initialized":initialized,"logging_degraded":logging_degraded});
        let bytes = serde_json::to_vec(&value).map_err(|_| "Cannot encode MCP process status")?;
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.write_all(&bytes))
            .and_then(|_| self.file.set_len(bytes.len() as u64))
            .and_then(|_| self.file.sync_data())
            .map_err(|_| "Cannot update MCP process status".to_owned())
    }
}
fn candidates(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = directory.join("processes");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for (index, e) in fs::read_dir(dir)
        .map_err(|_| "Cannot inspect MCP process statuses")?
        .take(1025)
        .enumerate()
    {
        if index >= 1024 {
            return Err("MCP process status inventory exceeds limit".into());
        }
        let e = e.map_err(|_| "Cannot inspect MCP process status")?;
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        if name
            .strip_suffix(".json")
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        {
            paths.push(e.path());
        }
        if paths.len() > 1024 {
            return Err("MCP process status inventory exceeds limit".into());
        }
    }
    Ok(paths)
}
pub fn observe(directory: &Path, configuration_id: &str) -> Result<Value, String> {
    let mut active = Vec::new();
    let mut unreadable = 0;
    for path in candidates(directory)? {
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = match options.open(&path) {
            Ok(f) => f,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        match file.try_lock() {
            Ok(()) => continue,
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(_) => {
                unreadable += 1;
                continue;
            }
        }
        let mut bytes = Vec::new();
        if Read::by_ref(&mut file)
            .take(4097)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() > 4096
        {
            unreadable += 1;
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
            unreadable += 1;
            continue;
        };
        if v["configuration_id"] != configuration_id {
            continue;
        }
        let Some(session) = v["session"]
            .as_str()
            .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            unreadable += 1;
            continue;
        };
        let client = v["client_kind"]
            .as_str()
            .filter(|s| {
                [
                    "probe",
                    "claude_declared",
                    "chatgpt_declared",
                    "other",
                    "unknown",
                ]
                .contains(s)
            })
            .unwrap_or("unknown");
        active.push(json!({"pid":v["pid"].as_u64(),"session":session,"client_kind":client,"initialized":v["initialized"]==true,"logging_degraded":v["logging_degraded"]==true}));
    }
    Ok(
        json!({"process_status":if active.is_empty(){"unknown"}else{"observed_active"},"active_processes":active,"unreadable_statuses":unreadable,"scope":"Local lock observations at this instant for this exact profile. Older/unmanaged processes may be invisible. Initialized means a handshake was observed; client name is self-declared. No active record does not prove the server is stopped."}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_pid_is_not_liveness_and_status_is_allowlisted() {
        let dir = std::env::temp_dir().join(format!("sqx-runtime-{:032x}", rand::random::<u128>()));
        let id = "a".repeat(64);
        let mut lease = Lease::acquire(&dir).unwrap();
        lease
            .update(Some(&id), &"b".repeat(32), "SECRET", true, true)
            .unwrap();
        let r = observe(&dir, &id).unwrap();
        assert_eq!(r["process_status"], "observed_active");
        assert_eq!(r["active_processes"][0]["client_kind"], "unknown");
        assert!(!r.to_string().contains("SECRET"));
        assert_eq!(r["active_processes"][0]["logging_degraded"], true);
        drop(lease);
        assert_eq!(observe(&dir, &id).unwrap()["process_status"], "unknown");
        fs::remove_dir_all(dir).unwrap();
    }
}
