use super::*;
use crate::native_profile::LaunchRequest;

pub(crate) struct DelegateCandidate {
    pub label: String,
    pub description: String,
    pub input: String,
}

impl App {
    pub(super) fn request_delegate_profiles(&mut self) {
        if self.delegate_profiles_requested {
            return;
        }
        let Some(tx) = self.event_tx.clone() else {
            return;
        };
        self.delegate_profiles_requested = true;
        let shell = Arc::clone(&self.shell_mgr);
        let window_id = self.window_id.clone();
        let tab_id = self.active_tab_key().to_string();
        tokio::task::spawn_local(async move {
            let result = crate::native_profile::list(
                &shell,
                window_id.as_deref().and_then(|id| id.parse().ok()),
            )
            .await
            .map_err(|error| format!("{error:#}"));
            let _ = tx.send(AppEvent::DelegateProfilesLoaded {
                window_id,
                tab_id,
                result,
            });
        });
    }

    pub(super) fn delegate_candidates(&self) -> Vec<DelegateCandidate> {
        let tab = self.current_tab();
        if tab.delegate_popup_dismissed || tab.cursor_pos != tab.input.len() {
            return Vec::new();
        }
        let Some(completion) = crate::native_profile::completion(&tab.input) else {
            return Vec::new();
        };
        let prefix = &tab.input[..completion.start];
        let query = completion.query.to_lowercase();
        let mut candidates = Vec::new();
        if completion.profiles {
            for profile in &self.delegate_profiles {
                if profile.name.to_lowercase().contains(&query)
                    || profile.agent_id.to_lowercase().starts_with(&query)
                    || profile.guid.to_lowercase().starts_with(&query)
                    || "--profile".starts_with(&query)
                    || "--agent".starts_with(&query)
                {
                    candidates.push(DelegateCandidate {
                        label: profile.name.clone(),
                        description: profile.agent_id.clone(),
                        input: format!("{prefix}--profile {} ", profile.guid),
                    });
                }
            }
        }
        if completion.targets {
            for (target, key) in [
                ("tab", "recommendations.button_open_in_new_tab"),
                ("pane", "recommendations.button_open_in_new_panel"),
            ] {
                if query.is_empty() || target.starts_with(&query) || "--target".starts_with(&query)
                {
                    candidates.push(DelegateCandidate {
                        label: format!("--target {target}"),
                        description: t!(key).into_owned(),
                        input: format!("{prefix}--target {target} "),
                    });
                }
            }
        }
        candidates
    }

    pub(super) fn complete_delegate_candidate(&mut self) -> bool {
        let candidates = self.delegate_candidates();
        let index = self
            .current_tab()
            .command_popup_selected
            .min(candidates.len().saturating_sub(1));
        let Some(candidate) = candidates.into_iter().nth(index) else {
            return false;
        };
        self.current_tab_mut().replace_input(candidate.input);
        true
    }

    pub(super) fn cmd_delegate(&mut self, input: String) {
        let parsed = match crate::native_profile::parse(&input) {
            Ok(parsed) => parsed,
            Err(_) => {
                self.current_tab_mut()
                    .messages
                    .push(ChatMessage::warning(t!("delegate.usage").into_owned()));
                return;
            }
        };
        let Some(tx) = self.event_tx.clone() else {
            self.current_tab_mut()
                .messages
                .push(ChatMessage::Error(t!("connection.lost").into_owned()));
            return;
        };
        let shell = Arc::clone(&self.shell_mgr);
        let tab_id = self.active_tab_key().to_string();
        let source = self.pane_id.clone();
        if source.is_none() {
            self.current_tab_mut()
                .messages
                .push(ChatMessage::Error(t!("connection.lost").into_owned()));
            return;
        }
        let window = self.window_id.as_deref().and_then(|id| id.parse().ok());
        let master = self
            .deferred_acp
            .as_ref()
            .and_then(|params| params.master_pipe_name.clone());
        let draft = format!("/delegate {input}");
        let tab = self.current_tab_mut();
        tab.record_input_history(&draft);
        tab.messages.push(ChatMessage::User(draft));
        tab.discard_input();
        tokio::task::spawn_local(async move {
            let result = async {
                let created = crate::native_profile::launch(
                    &shell,
                    LaunchRequest {
                        profile: parsed.profile.as_deref(),
                        provider: None,
                        source_pane: source.as_deref(),
                        window_id: window,
                        cwd: None,
                        task: Some(&parsed.task),
                        include_context: true,
                        placement: parsed.placement,
                        direction: "auto",
                        size: 0.5,
                        background: false,
                        title: None,
                    },
                )
                .await?;
                if let (Some(session), Some(pane), Some(provider)) = (
                    created["agent_session_id"].as_str(),
                    created["session_id"].as_str(),
                    created["agent_id"].as_str(),
                ) {
                    crate::cli::sessions::register_launched(
                        master.as_deref(),
                        session,
                        pane,
                        provider,
                        created["cwd"].as_str(),
                        None,
                    )
                    .await;
                }
                anyhow::Ok(())
            }
            .await
            .map_err(|error| format!("{error:#}"));
            let _ = tx.send(AppEvent::DelegateCompleted { tab_id, result });
        });
    }
}
