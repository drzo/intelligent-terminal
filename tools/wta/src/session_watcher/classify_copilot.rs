//! Copilot `events.jsonl` classifier.
//!
//! Record shapes (verified 2026-06-09 against a live `events.jsonl`):
//!   * `{"type":"assistant.turn_start","data":{"turnId":"0",...}}`
//!   * `{"type":"assistant.turn_end","data":{"turnId":"0"}}`
//!   * `{"type":"tool.execution_start","data":{"toolName":"skill",...}}`
//!   * `{"type":"tool.execution_complete","data":{"success":true,...}}`
//!
//! Activity model — **turn-based**, not tool-based. One user prompt drives one
//! or more assistant *turns*; the agent is busy for the whole turn (thinking,
//! streaming text, AND running tools), so WORKING is bracketed by
//! `assistant.turn_start` → `assistant.turn_end`, not by the brief
//! `tool.execution_*` windows. Tool starts only refine the picture (the
//! current tool, or ATTENTION for a user-input tool); `tool.execution_complete`
//! is ignored because IDLE is owned by `turn_end`. A `permission.requested`
//! record starts a correlated permission wait. `permission.completed` ends that
//! wait even when tool.execution_start was written before the approval dialog.
//!
//! The session key is the session-state directory name (supplied by the
//! watcher from the file path), never the record body.

use crate::agent_sessions::{is_user_input_tool, AgentKey, SessionEvent};

/// Map one parsed Copilot `events.jsonl` record to zero or more events.
pub fn classify(record: &serde_json::Value, key: &AgentKey) -> Vec<SessionEvent> {
    let kind = record.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        // A turn spans the agent's whole working period for one prompt
        // (thinking + streaming text + tool runs). turn_start → WORKING for
        // the entire turn; turn_end → IDLE. This is what keeps the row WORKING
        // during text generation, not just the brief tool windows. WORKING is
        // surfaced by reusing `ToolStarting` with an empty tool name —
        // `current_tool` is not rendered in the session view, so the empty
        // name is invisible and only the WORKING status takes effect.
        "assistant.turn_start" => vec![SessionEvent::ToolStarting {
            key: key.clone(),
            tool_name: String::new(),
        }],
        "assistant.turn_end" => vec![match record
            .get("timestamp")
            .and_then(crate::session_permissions::timestamp_ms)
        {
            Some(at_ms) => SessionEvent::Permission {
                key: key.clone(),
                update: crate::session_permissions::PermissionUpdate::TurnEnded { at_ms },
            },
            None => SessionEvent::ToolCompleted { key: key.clone() },
        }],
        "permission.requested" | "permission.completed" => {
            let request_id = record
                .get("data")
                .and_then(|data| data.get("requestId"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty());
            let at_ms = record
                .get("timestamp")
                .and_then(crate::session_permissions::timestamp_ms);
            let reason = record
                .get("data")
                .and_then(|d| d.get("permissionRequest"))
                .and_then(|p| p.get("intention"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("permission requested")
                .to_string();
            let (Some(request_id), Some(at_ms)) = (request_id, at_ms) else {
                tracing::debug!(target: "session_watcher", kind, "Copilot permission record lacks correlation metadata");
                return if kind == "permission.requested" {
                    vec![SessionEvent::Permission {
                        key: key.clone(),
                        update: crate::session_permissions::PermissionUpdate::Uncorrelated {
                            message: reason,
                            at_ms,
                        },
                    }]
                } else {
                    Vec::new()
                };
            };
            let update = if kind == "permission.requested" {
                crate::session_permissions::PermissionUpdate::Requested {
                    request_id: request_id.to_string(),
                    message: reason,
                    at_ms,
                }
            } else {
                crate::session_permissions::PermissionUpdate::Completed {
                    request_id: request_id.to_string(),
                    at_ms,
                }
            };
            vec![SessionEvent::Permission {
                key: key.clone(),
                update,
            }]
        }
        "tool.execution_start" => {
            let tool = record
                .get("data")
                .and_then(|d| d.get("toolName"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // A user-input tool (ask_user, ...) means the agent is blocked on
            // the user → Attention; everything else keeps the turn WORKING and
            // just refreshes the current tool.
            if is_user_input_tool(&tool) {
                vec![SessionEvent::Notification {
                    key: key.clone(),
                    message: tool,
                }]
            } else {
                vec![SessionEvent::ToolStarting {
                    key: key.clone(),
                    tool_name: tool,
                }]
            }
        }
        // tool.execution_complete is intentionally NOT mapped: returning to
        // IDLE is driven by `assistant.turn_end`, so a tool finishing mid-turn
        // must not flip the row to IDLE while the agent keeps working.
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_sessions::SessionEvent;

    fn rec(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn tool_start_maps_to_tool_starting() {
        let r = rec(r#"{"type":"tool.execution_start","data":{"toolName":"skill"}}"#);
        let out = classify(&r, &"sess-1".to_string());
        assert_eq!(
            out,
            vec![SessionEvent::ToolStarting {
                key: "sess-1".to_string(),
                tool_name: "skill".to_string()
            }]
        );
    }

    #[test]
    fn ask_user_tool_maps_to_notification() {
        let r = rec(r#"{"type":"tool.execution_start","data":{"toolName":"ask_user"}}"#);
        let out = classify(&r, &"sess-1".to_string());
        assert!(matches!(
            out.as_slice(),
            [SessionEvent::Notification { .. }]
        ));
    }

    #[test]
    fn permission_requested_maps_to_attention() {
        // Copilot writes a `permission.requested` record while it waits for the
        // user to approve/deny a command → ATTENTION. The human-readable
        // `intention` becomes the (non-rendered) attention reason.
        let r = rec(
            r#"{"type":"permission.requested","data":{"permissionRequest":{"kind":"shell","intention":"run git log"}}}"#,
        );
        let out = classify(&r, &"sess-1".to_string());
        assert_eq!(
            out,
            vec![SessionEvent::Permission {
                key: "sess-1".to_string(),
                update: crate::session_permissions::PermissionUpdate::Uncorrelated {
                    message: "run git log".to_string(),
                    at_ms: None,
                }
            }]
        );
    }

    #[test]
    fn permission_requested_without_intention_uses_fallback() {
        let r =
            rec(r#"{"type":"permission.requested","data":{"permissionRequest":{"kind":"shell"}}}"#);
        let out = classify(&r, &"sess-1".to_string());
        assert!(matches!(
            out.as_slice(),
            [SessionEvent::Permission { update: crate::session_permissions::PermissionUpdate::Uncorrelated { message, .. }, .. }] if message == "permission requested"
        ));
    }

    #[test]
    fn permission_records_preserve_request_identity_and_event_time() {
        for (kind, expected) in [
            (
                "permission.requested",
                crate::session_permissions::PermissionUpdate::Requested {
                    request_id: "approval".into(),
                    message: "run sleep".into(),
                    at_ms: 1792043537256,
                },
            ),
            (
                "permission.completed",
                crate::session_permissions::PermissionUpdate::Completed {
                    request_id: "approval".into(),
                    at_ms: 1792043537256,
                },
            ),
        ] {
            let record = serde_json::json!({
                "type": kind,
                "timestamp": "2026-10-15T05:52:17.256Z",
                "data": {
                    "requestId": "approval",
                    "permissionRequest": { "intention": "run sleep" },
                },
            });
            assert_eq!(
                classify(&record, &"session".into()),
                vec![SessionEvent::Permission {
                    key: "session".into(),
                    update: expected
                }]
            );
        }
    }

    #[test]
    fn incomplete_permission_records_cannot_clear_an_approval() {
        for data in [
            serde_json::json!({}),
            serde_json::json!({ "requestId": "" }),
            serde_json::json!({ "requestId": 42 }),
        ] {
            let record = serde_json::json!({
                "type": "permission.completed",
                "timestamp": "2026-10-15T05:52:17.256Z",
                "data": data,
            });
            assert!(classify(&record, &"session".into()).is_empty());
        }
        let record = serde_json::json!({
            "type": "permission.completed",
            "timestamp": "not-a-timestamp",
            "data": { "requestId": "approval" },
        });
        assert!(classify(&record, &"session".into()).is_empty());
    }

    #[test]
    fn turn_start_maps_to_working() {
        // A turn beginning means the agent is busy for the whole turn —
        // thinking, streaming text, and running tools — not just during the
        // brief tool windows. We surface that as WORKING by reusing
        // ToolStarting with an empty tool name (current_tool isn't rendered,
        // so the empty name is invisible; only the WORKING status matters).
        let r = rec(r#"{"type":"assistant.turn_start","data":{"turnId":"0"}}"#);
        let out = classify(&r, &"sess-1".to_string());
        assert_eq!(
            out,
            vec![SessionEvent::ToolStarting {
                key: "sess-1".to_string(),
                tool_name: String::new()
            }]
        );
    }

    #[test]
    fn turn_end_maps_to_tool_completed() {
        // turn_end is the authoritative "agent is done" signal → IDLE.
        let r = rec(r#"{"type":"assistant.turn_end","data":{"turnId":"0"}}"#);
        let out = classify(&r, &"sess-1".to_string());
        assert_eq!(
            out,
            vec![SessionEvent::ToolCompleted {
                key: "sess-1".to_string()
            }]
        );
    }

    #[test]
    fn permission_turn_end_preserves_source_time() {
        let record = serde_json::json!({
            "type": "assistant.turn_end",
            "timestamp": 1000,
            "data": { "turnId": "old" },
        });
        assert_eq!(
            classify(&record, &"session".into()),
            vec![SessionEvent::Permission {
                key: "session".into(),
                update: crate::session_permissions::PermissionUpdate::TurnEnded { at_ms: 1000 },
            }]
        );
    }

    #[test]
    fn tool_complete_is_dropped_in_turn_model() {
        // Returning to IDLE is driven by turn_end, NOT by individual tool
        // completion — a tool finishing mid-turn must not flip the row to
        // IDLE while the agent keeps working. So tool.execution_complete is
        // intentionally ignored.
        let r = rec(r#"{"type":"tool.execution_complete","data":{"success":true}}"#);
        assert!(classify(&r, &"sess-1".to_string()).is_empty());
    }

    #[test]
    fn unrelated_record_yields_nothing() {
        let r = rec(r#"{"type":"assistant.message","data":{"content":"hi"}}"#);
        assert!(classify(&r, &"sess-1".to_string()).is_empty());
    }
}
