use crate::agent_sessions::AgentStatus;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PermissionUpdate {
    Prompt {
        message: String,
        at_ms: Option<u64>,
    },
    Requested {
        request_id: String,
        message: String,
        at_ms: u64,
    },
    Completed {
        request_id: String,
        at_ms: u64,
    },
}

pub(crate) enum PermissionActivity {
    Waiting(String),
    Working,
}

#[derive(Default)]
pub(crate) struct PermissionWaits {
    pending: HashMap<String, (u64, String)>,
    completed: HashSet<String>,
    owns_attention: bool,
    prompt_at_ms: Option<u64>,
    completed_at_ms: Option<u64>,
    stopped_at_ms: Option<u64>,
}

impl PermissionWaits {
    pub(crate) fn apply(
        &mut self,
        update: PermissionUpdate,
        status: &AgentStatus,
    ) -> Option<PermissionActivity> {
        if !matches!(
            status,
            AgentStatus::Idle | AgentStatus::Working | AgentStatus::Attention
        ) {
            return None;
        }
        match update {
            PermissionUpdate::Prompt { message, at_ms } => {
                // COM hook delivery may trail the matching file completion.
                if at_ms.is_some_and(|at| {
                    self.stopped_at_ms.is_some_and(|stopped| at <= stopped)
                        || (self.pending.is_empty()
                            && self
                                .completed_at_ms
                                .is_some_and(|completed| at <= completed))
                }) {
                    return None;
                }
                if *status == AgentStatus::Attention && !self.owns_attention {
                    return None;
                }
                self.owns_attention = true;
                self.prompt_at_ms = at_ms;
                Some(PermissionActivity::Waiting(message))
            }
            PermissionUpdate::Requested {
                request_id,
                message,
                at_ms,
            } => {
                if self.stopped_at_ms.is_some_and(|stopped| at_ms <= stopped)
                    || self.pending.contains_key(&request_id)
                    || self.completed.contains(&request_id)
                {
                    return None;
                }
                self.pending.insert(request_id, (at_ms, message.clone()));
                if *status == AgentStatus::Attention && !self.owns_attention {
                    return None;
                }
                self.owns_attention = true;
                self.prompt_at_ms = Some(self.prompt_at_ms.unwrap_or(0).max(at_ms));
                Some(PermissionActivity::Waiting(message))
            }
            PermissionUpdate::Completed { request_id, at_ms } => {
                let (requested_at, _) = self.pending.get(&request_id)?;
                if at_ms < *requested_at {
                    tracing::debug!(target: "session_permissions", "ignored permission completion before its request");
                    return None;
                }
                self.pending.remove(&request_id);
                self.completed.insert(request_id);
                let completed_at = self.completed_at_ms.unwrap_or(0).max(at_ms);
                self.completed_at_ms = Some(completed_at);
                if !self.owns_attention || *status != AgentStatus::Attention {
                    return None;
                }
                if let Some(message) = self.pending_message() {
                    return Some(PermissionActivity::Waiting(message));
                }
                if self
                    .prompt_at_ms
                    .is_some_and(|prompt| prompt > completed_at)
                {
                    return None;
                }
                self.other_attention();
                Some(PermissionActivity::Working)
            }
        }
    }

    pub(crate) fn other_attention(&mut self) {
        self.owns_attention = false;
        self.prompt_at_ms = None;
    }

    pub(crate) fn working(&mut self) -> Option<String> {
        let pending = self.pending_message();
        self.owns_attention = pending.is_some();
        if pending.is_none() {
            self.prompt_at_ms = None;
        }
        pending
    }

    pub(crate) fn stop(&mut self, at_ms: u64) {
        self.pending.clear();
        self.completed.clear();
        self.other_attention();
        self.stopped_at_ms = Some(self.stopped_at_ms.unwrap_or(0).max(at_ms));
    }

    fn pending_message(&self) -> Option<String> {
        self.pending
            .iter()
            .max_by_key(|(id, (at_ms, _))| (*at_ms, *id))
            .map(|(_, (_, message))| message.clone())
    }
}

pub(crate) fn timestamp_ms(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        let timestamp = crate::session_history::parse_iso_to_system_time(value.as_str()?)?;
        let duration = timestamp.duration_since(std::time::UNIX_EPOCH).ok()?;
        u64::try_from(duration.as_millis()).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str, at_ms: u64) -> PermissionUpdate {
        PermissionUpdate::Requested {
            request_id: id.into(),
            message: id.into(),
            at_ms,
        }
    }

    fn complete(id: &str, at_ms: u64) -> PermissionUpdate {
        PermissionUpdate::Completed {
            request_id: id.into(),
            at_ms,
        }
    }

    #[test]
    fn permission_completion_waits_for_all_pending_requests() {
        let mut waits = PermissionWaits::default();
        waits.apply(request("first", 10), &AgentStatus::Working);
        waits.apply(request("second", 20), &AgentStatus::Attention);
        assert!(matches!(
            waits.apply(complete("first", 30), &AgentStatus::Attention),
            Some(PermissionActivity::Waiting(message)) if message == "second"
        ));
        assert!(matches!(
            waits.apply(complete("second", 40), &AgentStatus::Attention),
            Some(PermissionActivity::Working)
        ));
    }

    #[test]
    fn permission_completion_does_not_clear_an_unrelated_question() {
        let mut waits = PermissionWaits::default();
        waits.apply(request("approval", 10), &AgentStatus::Working);
        waits.other_attention();
        assert!(waits
            .apply(complete("approval", 20), &AgentStatus::Attention)
            .is_none());
        assert!(waits
            .apply(request("another", 30), &AgentStatus::Attention)
            .is_none());
        assert!(waits
            .apply(complete("another", 40), &AgentStatus::Attention)
            .is_none());
    }

    #[test]
    fn permission_completion_requires_the_matching_request() {
        let mut waits = PermissionWaits::default();
        waits.apply(request("current", 20), &AgentStatus::Working);
        assert!(waits
            .apply(complete("previous", 30), &AgentStatus::Attention)
            .is_none());
        assert!(waits
            .apply(complete("current", 10), &AgentStatus::Attention)
            .is_none());
        assert!(matches!(
            waits.apply(complete("current", 40), &AgentStatus::Attention),
            Some(PermissionActivity::Working)
        ));
        assert!(waits
            .apply(request("current", 20), &AgentStatus::Working)
            .is_none());
    }

    #[test]
    fn late_permission_hook_cannot_reopen_a_completed_wait() {
        let mut waits = PermissionWaits::default();
        waits.apply(request("approval", 10), &AgentStatus::Working);
        waits.apply(complete("approval", 20), &AgentStatus::Attention);
        assert!(waits
            .apply(
                PermissionUpdate::Prompt {
                    message: "late approval".into(),
                    at_ms: Some(10)
                },
                &AgentStatus::Working,
            )
            .is_none());
        assert!(matches!(
            waits.apply(
                PermissionUpdate::Prompt {
                    message: "new approval".into(),
                    at_ms: Some(30)
                },
                &AgentStatus::Working,
            ),
            Some(PermissionActivity::Waiting(_))
        ));
        assert!(waits
            .apply(complete("approval", 40), &AgentStatus::Attention)
            .is_none());
    }

    #[test]
    fn late_permission_records_cannot_restart_a_finished_turn() {
        let mut waits = PermissionWaits::default();
        waits.apply(request("approval", 10), &AgentStatus::Working);
        waits.stop(30);
        assert!(waits
            .apply(complete("approval", 20), &AgentStatus::Idle)
            .is_none());
        assert!(waits
            .apply(request("approval", 10), &AgentStatus::Idle)
            .is_none());
        assert!(waits
            .apply(
                PermissionUpdate::Prompt {
                    message: "late approval".into(),
                    at_ms: Some(10)
                },
                &AgentStatus::Idle,
            )
            .is_none());
    }
}
