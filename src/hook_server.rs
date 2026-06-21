use std::fs;
use std::net::TcpListener;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

pub struct HookEvent {
    pub terminal_id: usize,
    pub event_type: String,
}

pub fn start_hook_server(tx: Sender<HookEvent>) -> Option<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = listener.local_addr().ok()?.port();
    
    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let mut buffer = vec![0; 8192];
                if let Ok(bytes_read) = stream.read(&mut buffer) {
                    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                    
                    let mut terminal_id = None;
                    let mut event_type = None;
                    
                    // Parse path: POST /hook/<terminal_id>/<event_type> HTTP/1.1
                    // or POST /<terminal_id>/<event_type>
                    let first_line = request.lines().next().unwrap_or("");
                    let parts: Vec<&str> = first_line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let path = parts[1];
                        let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
                        
                        if path_segments.len() >= 3 && path_segments[0] == "hook" {
                            if let Ok(id) = path_segments[1].parse::<usize>() {
                                terminal_id = Some(id);
                                event_type = Some(path_segments[2].to_string());
                            }
                        } else if path_segments.len() == 2 && path_segments[0] == "hook" {
                            if let Ok(id) = path_segments[1].parse::<usize>() {
                                terminal_id = Some(id);
                            }
                        } else if path_segments.len() >= 2 {
                            if let Ok(id) = path_segments[0].parse::<usize>() {
                                terminal_id = Some(id);
                                event_type = Some(path_segments[1].to_string());
                            }
                        }
                    }
                    
                    // Fallback/Supplement from JSON body
                    if let Some(body_start) = request.find("\r\n\r\n") {
                        let body = &request[body_start + 4..];
                        if !body.trim().is_empty() {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
                                let (t_id_str, ev_str) = extract_from_json(&v);
                                if terminal_id.is_none() {
                                    if let Some(t_id) = t_id_str.and_then(|s| s.parse::<usize>().ok()) {
                                        terminal_id = Some(t_id);
                                    }
                                }
                                if event_type.is_none() {
                                    if let Some(ev) = ev_str {
                                        event_type = Some(ev);
                                    }
                                }
                            }
                        }
                    }
                    
                    if let (Some(tid), Some(ev)) = (terminal_id, event_type) {
                        let normalized = normalize_event(&ev).to_string();
                        let _ = tx.send(HookEvent {
                            terminal_id: tid,
                            event_type: normalized,
                        });
                    }
                    
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        }
    });
    
    Some(port)
}

fn extract_from_json(v: &serde_json::Value) -> (Option<String>, Option<String>) {
    if let Some(json_obj) = v.get("json") {
        let term_id = json_obj.get("terminalId").and_then(|x| x.as_str().map(|s| s.to_string()));
        let ev_type = json_obj.get("eventType").and_then(|x| x.as_str().map(|s| s.to_string()));
        if term_id.is_some() || ev_type.is_some() {
            return (term_id, ev_type);
        }
    }
    
    let term_id = v.get("terminalId").and_then(|x| x.as_str().map(|s| s.to_string()));
    let ev_type = v.get("eventType").and_then(|x| x.as_str().map(|s| s.to_string()));
    (term_id, ev_type)
}

fn normalize_event(raw: &str) -> &'static str {
    match raw.to_lowercase().as_str() {
        "start" | "started" | "sessionstart" | "session_start" | "task_started" | "userpromptsubmit" => "Start",
        "stop" | "completed" | "sessionend" | "session_end" | "agent-turn-complete" | "task_complete" => "Stop",
        "permissionrequest" | "waiting-for-input" | "exec_approval_request" | "apply_patch_approval_request" | "request_user_input" => "PermissionRequest",
        _ => "Unknown",
    }
}

pub fn setup_agent_hooks(agents: &[crate::settings::AgentConfig]) -> anyhow::Result<()> {
    let home = get_home_dir();
    let hook_dir = home.join(".ghost-mux").join("hooks");
    fs::create_dir_all(&hook_dir)?;
    
    let notify_path = hook_dir.join("notify.sh");
    let notify_content = r#"#!/bin/bash
# Ghost-mux CLI agent lifecycle hook

if [ -n "$1" ]; then
  INPUT="$1"
else
  INPUT=$(cat)
fi

EVENT_TYPE=$(echo "$INPUT" | grep -oE '"hook_event_name"[[:space:]]*:[[:space:]]*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
if [ -z "$EVENT_TYPE" ]; then
  EVENT_TYPE=$(echo "$INPUT" | grep -oE '"type"[[:space:]]*:[[:space:]]*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
fi

if [ -z "$EVENT_TYPE" ] && [ -n "$1" ]; then
  EVENT_TYPE="$1"
fi

if [ -z "$EVENT_TYPE" ]; then
  EVENT_TYPE="Stop"
fi

case "$EVENT_TYPE" in
  agent-turn-complete|task_complete|SessionEnd|sessionEnd|session_end|Stop|stop)
    EVENT_TYPE="Stop"
    ;;
  task_started|SessionStart|sessionStart|session_start|Start|start|UserPromptSubmit)
    EVENT_TYPE="Start"
    ;;
  exec_approval_request|apply_patch_approval_request|request_user_input|PermissionRequest|permissionrequest)
    EVENT_TYPE="PermissionRequest"
    ;;
esac

if [ "$EVENT_TYPE" = "PermissionRequest" ]; then
  printf '{"continue":true}\n'
fi

json_escape() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

HOOK_URL="${GHOST_MUX_HOOK_URL:-$SUPERSET_HOST_AGENT_HOOK_URL}"
if [ -n "$HOOK_URL" ]; then
  curl -s -X POST "$HOOK_URL/$EVENT_TYPE" \
    -H "Content-Type: application/json" \
    -d "{\"terminalId\":\"$GHOST_MUX_TERMINAL_ID\",\"eventType\":\"$EVENT_TYPE\"}" \
    > /dev/null 2>&1
fi

exit 0
"#;
    
    fs::write(&notify_path, notify_content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&notify_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&notify_path, perms)?;
    }
    
    let hook_cmd = "[ -n \"$SUPERSET_HOME_DIR\" ] && [ -x \"$SUPERSET_HOME_DIR/hooks/notify.sh\" ] && \"$SUPERSET_HOME_DIR/hooks/notify.sh\"";
    
    for agent in agents {
        if !agent.enabled {
            continue;
        }
        let config_path = resolve_home_path(&agent.config_file);
        if let Some(parent) = config_path.parent() {
            if parent.exists() {
                let mut existing = if config_path.exists() {
                    fs::read_to_string(&config_path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .unwrap_or_else(|| serde_json::json!({}))
                } else {
                    serde_json::json!({})
                };
                
                if agent.hook_type == "flat" {
                    merge_flat_hooks(&mut existing, hook_cmd);
                } else if agent.hook_type == "nested" {
                    merge_nested_hooks(&mut existing, hook_cmd);
                }
                
                if let Ok(content) = serde_json::to_string_pretty(&existing) {
                    let _ = fs::write(&config_path, content);
                }
            }
        }
    }
    
    // Pi extension setup
    let pi_dir = home.join(".pi").join("agent").join("extensions");
    if pi_dir.exists() {
        let pi_content = r#"// Ghost-mux pi extension v1
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export default function (pi: ExtensionAPI) {
	if (!process.env.SUPERSET_TERMINAL_ID) return;

	const supersetHome =
		process.env.SUPERSET_HOME_DIR || join(homedir(), ".ghost-mux");
	const notifyScript = join(supersetHome, "hooks", "notify.sh");
	if (!existsSync(notifyScript)) return;

	const fire = (eventName: string) => {
		try {
			const child = spawn(notifyScript, [], {
				stdio: ["pipe", "ignore", "ignore"],
				detached: true,
				env: { ...process.env, SUPERSET_AGENT_ID: "pi" },
			});
			child.on("error", () => {});
			child.stdin?.on("error", () => {});
			child.stdin?.end(JSON.stringify({ hook_event_name: eventName }));
			child.unref();
		} catch {}
	};

	const skip = (ctx: { hasUI?: boolean }) => ctx.hasUI === false;

	pi.on("session_start", (_event, ctx) => {
		if (skip(ctx)) return;
		fire("SessionStart");
	});

	pi.on("session_end", (_event, ctx) => {
		if (skip(ctx)) return;
		fire("SessionEnd");
	});

	pi.on("before_agent_start", (_event, ctx) => {
		if (skip(ctx)) return;
		fire("UserPromptSubmit");
	});

	pi.on("tool_execution_end", (_event, ctx) => {
		if (skip(ctx)) return;
		fire("PostToolUse");
	});

	pi.on("agent_end", (_event, ctx) => {
		if (skip(ctx)) return;
		fire("Stop");
	});

	pi.on("session_shutdown", (_event, ctx) => {
		if (skip(ctx)) return;
		fire("Stop");
	});
}
"#;
        let _ = fs::write(pi_dir.join("superset-hooks.ts"), pi_content);
        let _ = fs::write(pi_dir.join("ghost-mux-hooks.ts"), pi_content);
    }

    // OpenCode plugin setup
    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".config"));
    let opencode_dir = xdg_config.join("opencode").join("plugin");
    if opencode_dir.exists() {
        let opencode_content = r#"// Ghost-mux opencode plugin v8
export const SupersetNotifyPlugin = async ({ $, client }) => {
  if (globalThis.__ghostMuxOpencodeNotifyPluginV8) return {};
  globalThis.__ghostMuxOpencodeNotifyPluginV8 = true;

  if (!process?.env?.SUPERSET_TERMINAL_ID) return {};

  const notifyPath = "{{NOTIFY_PATH}}";
  const debug = process?.env?.SUPERSET_DEBUG === '1';

  let currentState = 'idle';
  let rootSessionID = null;
  let stopSent = false;

  const log = (...args) => {
    if (debug) console.log('[ghost-mux-plugin]', ...args);
  };

  const notify = async (hookEventName) => {
    const payload = JSON.stringify({ hook_event_name: hookEventName });
    try {
      await $`bash ${notifyPath} ${payload}`;
    } catch (err) {}
  };

  const childSessionCache = new Map();
  const isChildSession = async (sessionID) => {
    if (!sessionID) return true;
    if (!client?.session?.list) return true;

    if (childSessionCache.has(sessionID)) {
      return childSessionCache.get(sessionID);
    }

    try {
      const sessions = await client.session.list();
      const session = sessions.data?.find((s) => s.id === sessionID);
      const isChild = !!session?.parentID;
      childSessionCache.set(sessionID, isChild);
      return isChild;
    } catch (err) {
      return true;
    }
  };

  const handleBusy = async (sessionID) => {
    if (!rootSessionID) {
      rootSessionID = sessionID;
    }

    if (sessionID !== rootSessionID) {
      return;
    }

    if (currentState === 'idle') {
      currentState = 'busy';
      stopSent = false;
      await notify('Start');
    }
  };

  const handleStop = async (sessionID, reason) => {
    if (rootSessionID && sessionID !== rootSessionID) {
      return;
    }

    if (currentState === 'busy' && !stopSent) {
      currentState = 'idle';
      stopSent = true;
      await notify('Stop');
      rootSessionID = null;
    }
  };

  return {
    event: async ({ event }) => {
      const sessionID =
        event.properties?.sessionID ??
        event.properties?.info?.id ??
        null;

      if (event.type === "session.created") {
        const isChild = Boolean(event.properties?.info?.parentID);
        if (sessionID) childSessionCache.set(sessionID, isChild);
        if (!isChild) {
          await notify("SessionStart");
        }
        return;
      }
      if (event.type === "session.deleted") {
        const cachedIsChild =
          sessionID != null ? childSessionCache.get(sessionID) : undefined;
        const isChild =
          cachedIsChild !== undefined
            ? cachedIsChild
            : await isChildSession(sessionID);
        if (!isChild) {
          await notify("SessionEnd");
        }
        if (sessionID) childSessionCache.delete(sessionID);
        return;
      }

      if (await isChildSession(sessionID)) {
        return;
      }

      if (event.type === "session.status") {
        const status = event.properties?.status;
        if (status?.type === "busy") {
          await handleBusy(sessionID);
        } else if (status?.type === "idle") {
          await handleStop(sessionID, 'session.status.idle');
        }
      }

      if (event.type === "session.busy") {
        await handleBusy(sessionID);
      }
      if (event.type === "session.idle") {
        await handleStop(sessionID, 'session.idle');
      }

      if (event.type === "session.error") {
        await handleStop(sessionID, 'session.error');
      }
    },
    "permission.ask": async (_permission, output) => {
      if (output.status === "ask") {
        await notify("PermissionRequest");
      }
    },
  };
};
"#.replace("{{NOTIFY_PATH}}", &notify_path.to_string_lossy());
        
        let _ = fs::write(opencode_dir.join("superset-notify.js"), &opencode_content);
        let _ = fs::write(opencode_dir.join("ghost-mux-notify.js"), &opencode_content);
    }
    
    Ok(())
}

fn get_home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Users/saranyadamo"))
}

fn resolve_home_path(path_str: &str) -> PathBuf {
    if path_str.starts_with("~/") {
        get_home_dir().join(&path_str[2..])
    } else {
        PathBuf::from(path_str)
    }
}

fn merge_flat_hooks(existing: &mut serde_json::Value, hook_cmd: &str) {
    if !existing.is_object() {
        *existing = serde_json::json!({});
    }
    let obj = existing.as_object_mut().unwrap();
    if !obj.contains_key("version") {
        obj.insert("version".to_string(), serde_json::json!(1));
    }
    
    let hooks_val = obj.entry("hooks".to_string()).or_insert_with(|| serde_json::json!({}));
    if !hooks_val.is_object() {
        *hooks_val = serde_json::json!({});
    }
    let hooks_obj = hooks_val.as_object_mut().unwrap();
    
    let events = vec![
        ("sessionStart", "SessionStart"),
        ("sessionEnd", "SessionEnd"),
        ("beforeSubmitPrompt", "Start"),
        ("stop", "Stop"),
        ("beforeShellExecution", "PermissionRequest"),
        ("beforeMCPExecution", "PermissionRequest"),
    ];
    
    for (ev_name, ev_arg) in events {
        let entry_cmd = format!("{} {}", hook_cmd, ev_arg);
        let list_val = hooks_obj.entry(ev_name.to_string()).or_insert_with(|| serde_json::json!([]));
        if !list_val.is_array() {
            *list_val = serde_json::json!([]);
        }
        let list = list_val.as_array_mut().unwrap();
        
        let mut exists = false;
        for item in list.iter() {
            if let Some(cmd) = item.get("command").and_then(|x| x.as_str()) {
                if cmd.contains("notify.sh") {
                    exists = true;
                    break;
                }
            }
        }
        if !exists {
            list.push(serde_json::json!({ "command": entry_cmd }));
        }
    }
}

fn merge_nested_hooks(existing: &mut serde_json::Value, hook_cmd: &str) {
    if !existing.is_object() {
        *existing = serde_json::json!({});
    }
    let obj = existing.as_object_mut().unwrap();
    
    let hooks_val = obj.entry("hooks".to_string()).or_insert_with(|| serde_json::json!({}));
    if !hooks_val.is_object() {
        *hooks_val = serde_json::json!({});
    }
    let hooks_obj = hooks_val.as_object_mut().unwrap();
    
    let events = vec![
        ("SessionStart", None),
        ("SessionEnd", None),
        ("UserPromptSubmit", None),
        ("Stop", None),
        ("PostToolUse", Some("*")),
        ("PostToolUseFailure", Some("*")),
        ("PermissionRequest", Some("*")),
    ];
    
    for (ev_name, matcher) in events {
        let list_val = hooks_obj.entry(ev_name.to_string()).or_insert_with(|| serde_json::json!([]));
        if !list_val.is_array() {
            *list_val = serde_json::json!([]);
        }
        let list = list_val.as_array_mut().unwrap();
        
        let mut exists = false;
        for def in list.iter() {
            if let Some(hooks_list) = def.get("hooks").and_then(|x| x.as_array()) {
                for hook in hooks_list {
                    if let Some(cmd) = hook.get("command").and_then(|x| x.as_str()) {
                        if cmd.contains("notify.sh") {
                            exists = true;
                            break;
                        }
                    }
                }
            }
            if exists { break; }
        }
        
        if !exists {
            let hook_entry = serde_json::json!({
                "type": "command",
                "command": format!("{} || true", hook_cmd)
            });
            let mut def = serde_json::json!({
                "hooks": [hook_entry]
            });
            if let Some(m) = matcher {
                def.as_object_mut().unwrap().insert("matcher".to_string(), serde_json::json!(m));
            }
            list.push(def);
        }
    }
}
