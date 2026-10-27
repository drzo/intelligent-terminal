//! Migrate an already-bound conversation only after Terminal confirms its old identity.

use super::*;
use crate::agent_sessions::{pane_key, AgentStatus, CliSource, SessionLocation, SessionOrigin};
use crate::session_registry::{SessionIdentity, SessionInfo};
use crate::session_watcher::copilot_fork::CopilotFork;

pub(super) struct PendingFork {
    operation_id: String,
    source: SessionIdentity,
    target: acp::schema::v1::SessionId,
}

fn eligible(row: &SessionInfo) -> bool {
    row.provider_id.as_deref() == Some("copilot")
        && row.cli_source == Some(CliSource::Copilot)
        && row.location == SessionLocation::Host
        && row.session_universe.is_none()
        && row.origin != Some(SessionOrigin::AgentPane)
        && row
            .pane_session_id
            .as_ref()
            .is_some_and(|pane| !pane.is_empty())
        && matches!(
            row.status,
            Some(
                AgentStatus::Idle
                    | AgentStatus::Working
                    | AgentStatus::Attention
                    | AgentStatus::Error
            )
        )
}

pub(super) async fn observe(state: Arc<MasterStateInner>, fork: CopilotFork) {
    for attempt in 0..4 {
        if attempt != 0 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        match request(&state, &fork).await {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(target: "copilot_fork", %error, "could not publish fork binding");
                return;
            }
        }
    }
    tracing::debug!(target: "copilot_fork", "fork source has no live binding in this Terminal");
}

pub(super) async fn request(state: &Arc<MasterStateInner>, fork: &CopilotFork) -> Result<bool> {
    let sid = acp::schema::v1::SessionId::new(fork.source.clone());
    let Some(row) = state.registry.lookup(&sid).await else {
        return Ok(false);
    };
    if !eligible(&row) {
        return Ok(true);
    }
    let target = acp::schema::v1::SessionId::new(fork.target.clone());
    let target_identity = SessionIdentity {
        session_id: target.clone(),
        history_key: crate::session_registry::HistoryRowKey::new(
            "copilot",
            SessionLocation::Host,
            &fork.target,
            None,
        ),
    };
    if state
        .registry
        .lookup_identity(&target_identity)
        .await
        .is_some_and(|target| {
            target.origin == Some(SessionOrigin::AgentPane)
                || (target.pane_session_id.is_some()
                    && matches!(
                        target.status,
                        Some(
                            AgentStatus::Idle
                                | AgentStatus::Working
                                | AgentStatus::Attention
                                | AgentStatus::Error
                        )
                    ))
        })
    {
        tracing::debug!(target: "copilot_fork", "fork target already has a live owner");
        return Ok(true);
    }
    let pane = pane_key(
        row.pane_session_id
            .as_deref()
            .context("fork source pane missing")?,
    );
    let Some(wt) = state.wt.as_ref().filter(|wt| wt.is_available()) else {
        return Ok(true);
    };
    let source = SessionIdentity::from_info(&row);
    let mut pending = state.pending_copilot_forks.lock().await;
    if pending
        .get(&pane)
        .is_some_and(|plan| plan.source == source && plan.target.to_string() == fork.target)
    {
        return Ok(true);
    }
    let operation_id = uuid::Uuid::new_v4().to_string();
    pending.insert(
        pane.clone(),
        PendingFork {
            operation_id: operation_id.clone(),
            source,
            target,
        },
    );
    drop(pending);
    let result = wt
        .request(
            "publish",
            serde_json::json!({
                "type": "event",
                "method": "pane_agent_session_changed",
                "params": {
                    "event": "agent.session.fork",
                    "agent": "copilot",
                    "pane_id": pane,
                    "previous_agent_session_id": fork.source,
                    "agent_session_id": fork.target,
                    "operation_id": operation_id,
                },
            }),
        )
        .await;
    if let Err(error) = result {
        let mut pending = state.pending_copilot_forks.lock().await;
        if pending
            .get(&pane)
            .is_some_and(|plan| plan.operation_id == operation_id)
        {
            pending.remove(&pane);
        }
        return Err(error).context("publish verified fork binding");
    }
    Ok(true)
}

pub(super) async fn confirm(state: &Arc<MasterStateInner>, params: &serde_json::Value) {
    let pane = pane_key(params["pane_id"].as_str().unwrap_or(""));
    let plan = {
        let mut pending = state.pending_copilot_forks.lock().await;
        let Some(plan) = pending.get(&pane) else {
            return;
        };
        if params["operation_id"].as_str() != Some(plan.operation_id.as_str())
            || params["previous_agent_session_id"].as_str()
                != Some(plan.source.session_id.0.as_ref())
            || params["agent_session_id"].as_str() != Some(plan.target.0.as_ref())
        {
            return;
        }
        pending.remove(&pane)
    };
    let Some(plan) = plan else { return };
    let (first, second) = if plan.source.session_id.0 < plan.target.0 {
        (&plan.source.session_id, &plan.target)
    } else {
        (&plan.target, &plan.source.session_id)
    };
    let first_gate = session_lifecycle_gate(state, first).await;
    let second_gate = session_lifecycle_gate(state, second).await;
    let first_guard = first_gate.lock().await;
    let second_guard = second_gate.lock().await;
    let applied = state
        .registry
        .rebind_copilot_fork(&plan.source, &pane, &plan.target)
        .await;
    if applied {
        let mut hooked = state.hook_owned.lock().await;
        hooked.remove(&plan.source.session_id);
        let mut born = state.born_bound.lock().await;
        born.remove(&plan.source.session_id);
        if !hooked.contains(&plan.target) {
            born.insert(plan.target.clone());
        }
    }
    drop(second_guard);
    drop(first_guard);
    if applied {
        tracing::info!(target: "copilot_fork", pane, "confirmed foreground Copilot fork binding");
        broadcast_session_status_change(state, None).await;
        try_refresh_title_via_acp(state, &plan.target).await;
    } else {
        tracing::debug!(target: "copilot_fork", pane, "fork source binding was superseded before confirmation");
    }
}
