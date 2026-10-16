use crate::agent_sessions::AgentStatus;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PermissionUpdate {
    TurnEnded {
        at_ms: u64,
    },
    Uncorrelated {
        message: String,
        at_ms: Option<u64>,
    },
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
    Idle,
}

#[derive(Default)]
pub(crate) struct PermissionWaits {
    pending: HashMap<String, (u64, String)>,
    completed: HashSet<String>,
    owns_attention: bool,
    prompt_at_ms: Option<u64>,
    prompt_message: Option<String>,
    uncorrelated: Option<(Option<u64>, String)>,
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
            PermissionUpdate::TurnEnded { at_ms } => {
                let unrelated_attention = *status == AgentStatus::Attention && !self.owns_attention;
                let newer_completion = self
                    .completed_at_ms
                    .is_some_and(|completed| completed > at_ms);
                let remaining = self.stop(Some(at_ms));
                if unrelated_attention && (remaining.is_some() || newer_completion) {
                    self.other_attention();
                    return None;
                }
                match remaining {
                    Some(message) => Some(PermissionActivity::Waiting(message)),
                    None if newer_completion => None,
                    None => Some(PermissionActivity::Idle),
                }
            }
            PermissionUpdate::Uncorrelated { message, at_ms } => {
                if at_ms.is_some_and(|at| self.stopped_at_ms.is_some_and(|stop| at <= stop)) {
                    return None;
                }
                if self.uncorrelated.as_ref().is_some_and(|(known, _)| {
                    known.is_some_and(|known| at_ms.is_none_or(|at| at < known))
                }) {
                    return None;
                }
                self.uncorrelated = Some((at_ms, message.clone()));
                if *status == AgentStatus::Attention && !self.owns_attention {
                    return None;
                }
                self.owns_attention = true;
                Some(PermissionActivity::Waiting(message))
            }
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
                if self
                    .prompt_at_ms
                    .is_some_and(|known| at_ms.is_none_or(|at| at < known))
                {
                    return None;
                }
                self.owns_attention = true;
                self.prompt_at_ms = at_ms;
                self.prompt_message = Some(message.clone());
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
                if self.prompt_at_ms.is_none_or(|known| at_ms >= known) {
                    self.prompt_at_ms = Some(at_ms);
                    self.prompt_message = Some(message.clone());
                }
                Some(PermissionActivity::Waiting(
                    self.uncorrelated
                        .as_ref()
                        .map(|(_, reason)| reason.clone())
                        .or_else(|| self.prompt_message.clone())
                        .unwrap_or(message),
                ))
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
                if let Some(message) = self
                    .uncorrelated
                    .as_ref()
                    .map(|(_, message)| message.clone())
                    .or_else(|| self.pending_message())
                {
                    return Some(PermissionActivity::Waiting(message));
                }
                if self
                    .prompt_at_ms
                    .is_some_and(|prompt| prompt > completed_at)
                {
                    return self.prompt_message.clone().map(PermissionActivity::Waiting);
                }
                self.other_attention();
                Some(PermissionActivity::Working)
            }
        }
    }

    pub(crate) fn other_attention(&mut self) {
        self.owns_attention = false;
        self.prompt_at_ms = None;
        self.prompt_message = None;
    }

    pub(crate) fn working(&mut self) -> Option<String> {
        let pending = self
            .uncorrelated
            .as_ref()
            .map(|(_, message)| message.clone())
            .or_else(|| self.pending_message())
            .or_else(|| self.prompt_message.clone());
        self.owns_attention = pending.is_some();
        if pending.is_none() {
            self.prompt_at_ms = None;
        }
        pending
    }

    pub(crate) fn stop(&mut self, at_ms: Option<u64>) -> Option<String> {
        // Legacy untimed boundaries clear current waits without inventing a
        // source-time cutoff from the receiver's wall clock.
        self.pending
            .retain(|_, (at, _)| at_ms.is_some_and(|stop| *at > stop));
        if at_ms.is_none_or(|stop| {
            self.completed_at_ms
                .is_none_or(|completed| completed <= stop)
        }) {
            self.completed.clear();
        }
        if !self
            .prompt_at_ms
            .is_some_and(|at| at_ms.is_some_and(|stop| at > stop))
        {
            self.prompt_at_ms = None;
            self.prompt_message = None;
        }
        if !self.uncorrelated.as_ref().is_some_and(|(at, _)| {
            at.is_some_and(|at| at_ms.is_some_and(|stop| at > stop))
                || (at.is_none() && (!self.pending.is_empty() || self.prompt_at_ms.is_some()))
        }) {
            self.uncorrelated = None;
        }
        if let Some(at) = at_ms {
            self.stopped_at_ms = Some(self.stopped_at_ms.unwrap_or(0).max(at));
        }
        let pending = self
            .uncorrelated
            .as_ref()
            .map(|(_, message)| message.clone())
            .or_else(|| self.pending_message())
            .or_else(|| self.prompt_message.clone());
        self.owns_attention = pending.is_some();
        pending
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
    fn uncorrelated_permission_evidence_cannot_regress() {
        for delayed_at in [None, Some(10)] {
            let mut waits = PermissionWaits::default();
            waits.apply(
                PermissionUpdate::Uncorrelated {
                    message: "newer approval".into(),
                    at_ms: Some(30),
                },
                &AgentStatus::Working,
            );
            assert!(waits
                .apply(
                    PermissionUpdate::Uncorrelated {
                        message: "delayed approval".into(),
                        at_ms: delayed_at,
                    },
                    &AgentStatus::Attention,
                )
                .is_none());
            assert!(matches!(
                waits.apply(
                    PermissionUpdate::TurnEnded { at_ms: 20 },
                    &AgentStatus::Attention,
                ),
                Some(PermissionActivity::Waiting(message)) if message == "newer approval"
            ));
            assert!(matches!(
                waits.apply(
                    PermissionUpdate::TurnEnded { at_ms: 40 },
                    &AgentStatus::Attention,
                ),
                Some(PermissionActivity::Idle)
            ));
        }
    }

    #[test]
    fn delayed_turn_end_preserves_unrelated_attention() {
        let mut waits = PermissionWaits::default();
        waits.apply(request("approval", 1200), &AgentStatus::Working);
        waits.other_attention();
        assert!(waits
            .apply(
                PermissionUpdate::TurnEnded { at_ms: 1000 },
                &AgentStatus::Attention,
            )
            .is_none());
        assert!(waits
            .apply(complete("approval", 1400), &AgentStatus::Attention)
            .is_none());
    }

    #[test]
    fn delayed_turn_end_preserves_completed_approval_activity() {
        let mut waits = PermissionWaits::default();
        waits.apply(request("approval", 1200), &AgentStatus::Working);
        waits.apply(complete("approval", 1400), &AgentStatus::Attention);
        assert!(waits
            .apply(
                PermissionUpdate::TurnEnded { at_ms: 1000 },
                &AgentStatus::Working,
            )
            .is_none());
        assert!(waits
            .apply(request("approval", 1200), &AgentStatus::Working)
            .is_none());
        assert!(matches!(
            waits.apply(
                PermissionUpdate::TurnEnded { at_ms: 1500 },
                &AgentStatus::Working,
            ),
            Some(PermissionActivity::Idle)
        ));
    }

    #[test]
    fn delayed_prompt_cannot_replace_a_newer_wait() {
        for delayed_at in [Some(10), None] {
            let mut waits = PermissionWaits::default();
            waits.apply(request("A", 10), &AgentStatus::Working);
            waits.apply(
                PermissionUpdate::Prompt {
                    message: "B".into(),
                    at_ms: Some(30),
                },
                &AgentStatus::Attention,
            );
            assert!(waits
                .apply(
                    PermissionUpdate::Prompt {
                        message: "A".into(),
                        at_ms: delayed_at,
                    },
                    &AgentStatus::Attention
                )
                .is_none());
            assert!(matches!(
                waits.apply(complete("A", 20), &AgentStatus::Attention),
                Some(PermissionActivity::Waiting(message)) if message == "B"
            ));
            waits.apply(request("B", 30), &AgentStatus::Attention);
            assert!(matches!(
                waits.apply(complete("B", 40), &AgentStatus::Attention),
                Some(PermissionActivity::Working)
            ));
        }
    }

    #[test]
    fn uncorrelated_wait_survives_other_completion_and_tool_start() {
        for at_ms in [Some(15), None] {
            let mut waits = PermissionWaits::default();
            waits.apply(request("A", 10), &AgentStatus::Working);
            waits.apply(
                PermissionUpdate::Uncorrelated {
                    message: "B".into(),
                    at_ms,
                },
                &AgentStatus::Attention,
            );
            assert!(matches!(
                waits.apply(complete("A", 20), &AgentStatus::Attention),
                Some(PermissionActivity::Waiting(message)) if message == "B"
            ));
            assert_eq!(waits.working().as_deref(), Some("B"));
            assert!(waits.stop(Some(30)).is_none());
        }
    }

    #[test]
    fn source_turn_boundary_keeps_newer_waits_and_legacy_has_no_cutoff() {
        let mut waits = PermissionWaits::default();
        waits.stop(Some(1000));
        assert!(matches!(
            waits.apply(request("new", 1200), &AgentStatus::Working),
            Some(PermissionActivity::Waiting(_))
        ));
        assert_eq!(waits.stop(Some(1000)).as_deref(), Some("new"));
        waits.stop(None);
        assert!(matches!(
            waits.apply(request("next", 1300), &AgentStatus::Idle),
            Some(PermissionActivity::Waiting(_))
        ));
        assert!(waits
            .apply(request("old", 900), &AgentStatus::Attention)
            .is_none());
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
        waits.stop(Some(30));
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
