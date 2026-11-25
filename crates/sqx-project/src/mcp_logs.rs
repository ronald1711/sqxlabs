//! Bounded, allowlisted diagnostic export. Never return raw log lines.
use crate::mcp_config::Config;
use serde_json::{json, Value};
use std::{fs, io::Read};
const TOOLS: &[&str] = &[
    "generation_resume",
    "generation_trial",
    "generation_import",
    "generation_export",
    "generation_history",
    "list_indicators",
    "list_symbols",
    "questdb_symbols",
    "questdb_bars",
    "backtest",
    "generate",
    "portfolio",
    "robustness",
    "research_history",
    "research_resume",
    "research_export",
    "research_import",
];
fn clean(v: &Value) -> Option<Value> {
    let session = v["session"]
        .as_str()
        .filter(|s| s.len() == 32 && s.bytes().all(|c| c.is_ascii_hexdigit()))?;
    let method = v["method"].as_str().filter(|s| {
        [
            "process",
            "parse",
            "initialize",
            "notifications/initialized",
            "tools/list",
            "tools/call",
            "unknown",
        ]
        .contains(s)
    })?;
    let status = v["status"].as_str().filter(|s| {
        [
            "started",
            "completed",
            "tool_error",
            "parse_error",
            "unsupported_method",
            "stdin_error",
            "stopped",
            "unknown",
        ]
        .contains(s)
    })?;
    let tool = v["tool"].as_str().filter(|s| TOOLS.contains(s));
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
    let config_id = v["configuration_id"]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()));
    Some(
        json!({"configuration_id":config_id,"utc_unix_ms":v["utc_unix_ms"].as_u64().filter(|t|*t<=8_640_000_000_000_000)?,"pid":v["pid"].as_u64()?,"session":session,"request_sequence":v["request_sequence"].as_u64()?,"method":method,"tool":tool,"status":status,"duration_ms":v["duration_ms"].as_u64()?,"client_kind":client}),
    )
}
pub fn report(config: &Config, filter: &Value) -> Result<Value, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let cutoff = now.saturating_sub(config.log_retention_days * 86_400_000);
    let since = filter
        .get("since_utc_ms")
        .map(|v| v.as_u64().ok_or("Invalid log time filter"))
        .transpose()?
        .unwrap_or(cutoff)
        .max(cutoff);
    let tool = filter
        .get("tool")
        .map(|v| {
            v.as_str()
                .filter(|v| TOOLS.contains(v))
                .ok_or("Invalid log tool filter")
        })
        .transpose()?;
    let session = filter
        .get("session")
        .map(|v| {
            v.as_str()
                .filter(|v| v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or("Invalid log session filter")
        })
        .transpose()?;
    let status = filter
        .get("status")
        .map(|v| {
            v.as_str()
                .filter(|v| {
                    [
                        "started",
                        "completed",
                        "tool_error",
                        "parse_error",
                        "unsupported_method",
                        "stdin_error",
                        "stopped",
                    ]
                    .contains(v)
                })
                .ok_or("Invalid log status filter")
        })
        .transpose()?;
    let mut events = std::collections::BTreeMap::new();
    let mut ordinal = 0_u64;
    let mut invalid = 0;
    let mut clipped = false;
    let mut bytes_left = 20 * 1024 * 1024_u64;
    let mut missing = 0;
    if !config.log_directory.is_dir() {
        return Err(
            "MCP log directory is unavailable; logging may not have started or may have failed"
                .into(),
        );
    }
    for n in 0..config.log_max_files {
        let path = config.log_directory.join(if n == 0 {
            "mcp.jsonl".to_owned()
        } else {
            format!("mcp.{n}.jsonl")
        });
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing += 1;
                continue;
            }
            Err(_) => return Err("Cannot inspect MCP log file".into()),
        };
        if !meta.file_type().is_file() {
            return Err("MCP log path is not a regular file".into());
        }
        if meta.len() > 10_485_760 || meta.len() > bytes_left {
            clipped = true;
            continue;
        }
        bytes_left -= meta.len();
        let mut raw = String::new();
        fs::File::open(&path)
            .map_err(|_| "Cannot open MCP log")?
            .take(meta.len() + 1)
            .read_to_string(&mut raw)
            .map_err(|_| "Cannot read MCP log")?;
        if raw.len() as u64 > meta.len() {
            clipped = true;
            continue;
        }
        for line in raw.lines() {
            let entry = serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|v| clean(&v));
            let Some(entry) = entry else {
                invalid += 1;
                continue;
            };
            if entry["utc_unix_ms"].as_u64().unwrap() < since
                || tool.is_some_and(|t| entry["tool"] != t)
                || session.is_some_and(|s| entry["session"] != s)
                || status.is_some_and(|s| entry["status"] != s)
            {
                continue;
            }
            ordinal += 1;
            events.insert((entry["utc_unix_ms"].as_u64().unwrap(), ordinal), entry);
            if events.len() > 2000 {
                events.pop_first();
                clipped = true;
            }
        }
    }
    let mut events: Vec<_> = events.into_values().collect();
    events.sort_by_key(|v| v["utc_unix_ms"].as_u64().unwrap());
    let mut initialized = std::collections::HashSet::new();
    let mut observed = Vec::new();
    for e in &events {
        let key = (
            e["session"].as_str().unwrap(),
            e["configuration_id"].as_str(),
        );
        if e["method"] == "initialize" && e["status"] == "completed" {
            initialized.insert(key);
        }
        if e["method"] == "notifications/initialized"
            && e["status"] == "completed"
            && initialized.contains(&key)
        {
            observed.push(json!({"session":e["session"],"configuration_id":e["configuration_id"],"client_kind":e["client_kind"],"observed_at_utc_ms":e["utc_unix_ms"]}));
        }
    }
    Ok(
        json!({"schema":1,"scope":"Sanitized MCP event metadata only. No request arguments, raw errors, prompts, account values or credentials. Logs are not independent execution evidence. Concurrent writes or rotation can make this snapshot incomplete.","configuration_id":config.identity(),"event_scope":"Log directory may be shared by multiple profiles. Each event carries its own configuration_id when available; null means unknown.","observed_handshakes":observed,"connection_scope":"Historical protocol handshakes in this filtered snapshot only. Client kind is self-declared, not authenticated. A probe is not an external client. This does not prove a process or connection is still active.","events":events,"invalid_records":invalid,"truncated":clipped,"files_not_present":missing,"external_client_connection":"unknown","process_status":"unknown"}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_allowlisted_fields_are_exported() {
        let v = json!({"utc_unix_ms":1,"pid":2,"session":"a".repeat(32),"request_sequence":3,"method":"tools/call","tool":"SECRET","status":"tool_error","duration_ms":4,"client_kind":"SECRET","api_key":"SECRET","message":"SECRET"});
        let cleaned = clean(&v).unwrap();
        assert!(!cleaned.to_string().contains("SECRET"));
        assert!(cleaned["tool"].is_null());
        assert_eq!(cleaned["client_kind"], "unknown");
        let mut bad = v;
        bad["session"] = json!("../private");
        assert!(clean(&bad).is_none());
    }
    #[test]
    fn filters_retention_and_recent_record_limit_are_explicit() {
        let dir =
            std::env::temp_dir().join(format!("sqx-log-report-{:032x}", rand::random::<u128>()));
        fs::create_dir_all(&dir).unwrap();
        let c = Config {
            schema: 1,
            executable: std::env::current_exe().unwrap(),
            working_directory: dir.clone(),
            research_directory: dir.join("archive"),
            log_directory: dir.clone(),
            log_max_bytes: 1_048_576,
            log_max_files: 2,
            log_retention_days: 1,
            source: crate::mcp_config::Source::Request,
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let mut lines = String::new();
        for n in 0..2010 {
            lines.push_str(&format!("{}\n",json!({"utc_unix_ms":now-2010+n,"pid":1,"session":"a".repeat(32),"request_sequence":n,"method":"tools/call","tool":"backtest","status":"completed","duration_ms":1,"secret":"SECRET"})));
        }
        lines.push_str("broken tail");
        fs::write(dir.join("mcp.jsonl"), lines).unwrap();
        let r = report(&c, &json!({})).unwrap();
        assert_eq!(r["events"].as_array().unwrap().len(), 2000);
        assert_eq!(r["truncated"], true);
        assert_eq!(r["invalid_records"], 1);
        assert!(!r.to_string().contains("SECRET"));
        assert_eq!(r["events"][0]["request_sequence"], 10);
        assert!(report(&c, &json!({"tool":"generate"})).unwrap()["events"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            report(&c, &json!({"since_utc_ms":now-1})).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(report(&c, &json!({"session":"../secret"})).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
