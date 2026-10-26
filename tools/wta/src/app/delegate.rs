use super::*;
use crate::native_profile::{DelegateInput, LaunchRequest, NativeProfile, Placement};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DelegateDraft {
    pub profile_id: String,
    pub profile_name: String,
    pub placement: Placement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DelegateControl {
    Profile,
    Placement,
    Pick(usize),
}

pub(crate) struct DelegatePicker {
    pub control: DelegateControl,
    pub query: String,
    pub selected: usize,
}

pub(crate) struct PendingDelegate {
    pub id: uuid::Uuid,
    pub origin_window_id: Option<String>,
    pub origin_tab_id: String,
    pub input: String,
    pub draft: Option<DelegateDraft>,
    pub submitted_revision: u64,
    pub released_revision: Option<u64>,
}

pub(crate) struct DelegateFailure {
    pub error: String,
    pub outcome_unknown: bool,
}

#[derive(Default)]
pub(crate) struct DelegateState {
    pub draft: Option<DelegateDraft>,
    pub picker: Option<DelegatePicker>,
    pub pending: Option<PendingDelegate>,
}

impl App {
    pub(super) fn delegate_owner_mut(
        &mut self,
        window_id: &Option<String>,
        tab_id: &str,
        request_id: uuid::Uuid,
    ) -> Option<&mut TabSession> {
        self.tab_sessions.values_mut().find(|tab| {
            tab.delegate.pending.as_ref().is_some_and(|pending| {
                pending.id == request_id
                    && &pending.origin_window_id == window_id
                    && pending.origin_tab_id == tab_id
            })
        })
    }

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

    pub(crate) fn delegate_profile_candidates(&self) -> Vec<&NativeProfile> {
        if self.delegate_profiles_requested {
            return Vec::new();
        }
        let query = self
            .current_tab()
            .delegate
            .picker
            .as_ref()
            .map(|picker| picker.query.to_lowercase())
            .unwrap_or_default();
        self.delegate_profiles
            .iter()
            .filter(|profile| {
                profile.name.to_lowercase().contains(&query)
                    || profile.agent_id.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub(super) fn open_delegate_picker(&mut self, control: DelegateControl) {
        if self.current_tab().delegate.pending.is_some() {
            self.current_tab_mut()
                .messages
                .push(ChatMessage::warning(t!("agents.loading").into_owned()));
            return;
        }
        self.delegate_control_hits.clear();
        let selected = match control {
            DelegateControl::Profile => {
                let selected_id = self
                    .current_tab()
                    .delegate
                    .draft
                    .as_ref()
                    .map(|draft| draft.profile_id.as_str());
                self.delegate_profiles
                    .iter()
                    .position(|profile| {
                        selected_id.map_or(profile.default, |id| profile.guid == id)
                    })
                    .unwrap_or_default()
            }
            DelegateControl::Placement => usize::from(
                self.current_tab()
                    .delegate
                    .draft
                    .as_ref()
                    .is_some_and(|draft| draft.placement == Placement::Pane),
            ),
            DelegateControl::Pick(_) => return,
        };
        let tab = self.current_tab_mut();
        tab.clear_completed_turn_selection();
        tab.recommendation_focus = RecommendationFocus::Input;
        tab.delegate.picker = Some(DelegatePicker {
            control,
            query: String::new(),
            selected,
        });
        if control == DelegateControl::Profile {
            self.request_delegate_profiles();
        }
    }

    pub(super) fn commit_delegate_pick(&mut self) {
        self.delegate_control_hits.clear();
        let Some(picker) = &self.current_tab().delegate.picker else {
            return;
        };
        let control = picker.control;
        let selected = picker.selected;
        match control {
            DelegateControl::Profile => {
                let Some(profile) = self.delegate_profile_candidates().get(selected).copied()
                else {
                    return;
                };
                let profile_id = profile.guid.clone();
                let profile_name = profile.name.clone();
                let tab = self.current_tab_mut();
                let placement = tab
                    .delegate
                    .draft
                    .as_ref()
                    .map(|draft| draft.placement)
                    .unwrap_or_default();
                if tab.delegate.draft.is_none() {
                    tab.discard_input();
                }
                tab.delegate.draft = Some(DelegateDraft {
                    profile_id,
                    profile_name,
                    placement,
                });
            }
            DelegateControl::Placement => {
                if let Some(draft) = &mut self.current_tab_mut().delegate.draft {
                    draft.placement = if selected == 0 {
                        Placement::Tab
                    } else {
                        Placement::Pane
                    };
                }
            }
            DelegateControl::Pick(_) => return,
        }
        self.current_tab_mut().mark_composer_changed();
        self.current_tab_mut().delegate.picker = None;
    }

    pub(super) fn handle_delegate_key(&mut self, key: KeyEvent) -> bool {
        if let Some(picker) = &self.current_tab().delegate.picker {
            let count = if picker.control == DelegateControl::Profile {
                self.delegate_profile_candidates().len()
            } else {
                2
            };
            match key.code {
                KeyCode::Enter if key.modifiers.is_empty() => self.commit_delegate_pick(),
                KeyCode::Esc => self.current_tab_mut().delegate.picker = None,
                KeyCode::Up | KeyCode::Down => {
                    if let Some(picker) = &mut self.current_tab_mut().delegate.picker {
                        picker.selected = if key.code == KeyCode::Up {
                            picker.selected.saturating_sub(1)
                        } else {
                            (picker.selected + 1).min(count.saturating_sub(1))
                        };
                    }
                }
                KeyCode::Backspace | KeyCode::Char(_)
                    if picker.control == DelegateControl::Profile
                        && (key.modifiers.is_empty()
                            || key.modifiers == KeyModifiers::SHIFT
                            || key.modifiers == (KeyModifiers::CONTROL | KeyModifiers::ALT)) =>
                {
                    if let Some(picker) = &mut self.current_tab_mut().delegate.picker {
                        if let KeyCode::Char(character) = key.code {
                            picker.query.push(character);
                        } else {
                            picker.query.pop();
                        }
                        picker.selected = 0;
                    }
                }
                _ => {}
            }
            return true;
        }
        let tab = self.current_tab();
        if tab.delegate.pending.as_ref().is_some_and(|pending| {
            pending.released_revision.is_none()
                && pending.submitted_revision == tab.composer_revision
                && pending.input == tab.input
                && pending.draft == tab.delegate.draft
        }) && key.code == KeyCode::Enter
            && !key.modifiers.contains(KeyModifiers::SHIFT)
        {
            return true;
        }
        if self.current_tab().delegate.draft.is_none() {
            // Tab only completes the command name; Enter opens its picker.
            if key.code == KeyCode::Tab
                && key.modifiers.is_empty()
                && self
                    .selected_slash_command_candidate()
                    .is_some_and(|candidate| {
                        matches!(candidate, crate::ui::CommandCandidate::Client(spec)
                        if spec.kind == CommandKind::Delegate)
                    })
            {
                self.current_tab_mut().replace_input("/new-agent ".into());
                return true;
            }
            return false;
        }
        if !self.current_tab().input_has_nav_focus() {
            if key.code == KeyCode::Enter
                && self.current_tab().selected_completed_turn_idx.is_some()
            {
                if !key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.current_tab_mut().toggle_selected_completed_turn();
                }
                return true;
            }
            return false;
        }
        match key.code {
            KeyCode::Tab => self.open_delegate_picker(DelegateControl::Profile),
            KeyCode::BackTab => self.open_delegate_picker(DelegateControl::Placement),
            KeyCode::Esc => {
                self.current_tab_mut().mark_composer_changed();
                self.current_tab_mut().delegate.draft = None;
                self.current_tab_mut().refresh_command_popup();
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                let tab = self.current_tab();
                let draft = tab
                    .delegate
                    .draft
                    .as_ref()
                    .expect("delegate draft is active");
                if !tab.input.trim().is_empty() {
                    self.launch_delegate(DelegateInput {
                        profile: Some(draft.profile_id.clone()),
                        placement: draft.placement,
                        task: tab.input.clone(),
                    });
                }
            }
            _ => return false,
        }
        true
    }

    pub(super) fn handle_delegate_mouse(&mut self, mouse: crossterm::event::MouseEvent) -> bool {
        use crossterm::event::{MouseButton, MouseEventKind};
        if self.mode != AppMode::Chat
            || self.current_tab().current_view != View::Chat
            || self.help_overlay_visible
            || !self.current_tab().permission.is_empty()
            || !self.current_tab().user_input.is_empty()
            || self.current_tab().agent_picker_open
            || self.current_tab().model_picker_open
            || self.current_tab().config_picker.is_open()
        {
            return false;
        }
        let picker_open = self.current_tab().delegate.picker.is_some();
        let current_owner =
            self.delegate_control_owner
                .as_ref()
                .is_some_and(|(window_id, tab_id)| {
                    window_id == &self.window_id && tab_id == self.active_tab_key()
                });
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            let control = self
                .delegate_control_hits
                .iter()
                .filter(|(_, control)| {
                    current_owner
                        && match control {
                            DelegateControl::Pick(_) => picker_open,
                            _ => self.current_tab().delegate.draft.is_some(),
                        }
                })
                .find(|(area, _)| {
                    area.contains(ratatui::layout::Position::new(mouse.column, mouse.row))
                })
                .map(|(_, control)| *control);
            if let Some(control) = control {
                self.cancel_completed_turn_click();
                self.text_selection.clear();
                match control {
                    DelegateControl::Pick(index) => {
                        if let Some(picker) = &mut self.current_tab_mut().delegate.picker {
                            picker.selected = index;
                        }
                        self.commit_delegate_pick();
                    }
                    control => self.open_delegate_picker(control),
                }
                return true;
            }
        }
        if picker_open
            && matches!(
                mouse.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            )
        {
            let code = if mouse.kind == MouseEventKind::ScrollUp {
                KeyCode::Up
            } else {
                KeyCode::Down
            };
            self.handle_delegate_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
        picker_open
    }

    pub(super) fn cmd_delegate(&mut self, input: String) {
        if input.trim().is_empty() {
            self.open_delegate_picker(DelegateControl::Profile);
            return;
        }
        let parsed = match crate::native_profile::parse(&input) {
            Ok(parsed) => parsed,
            Err(_) => {
                self.current_tab_mut()
                    .messages
                    .push(ChatMessage::warning(t!("delegate.usage").into_owned()));
                return;
            }
        };
        self.launch_delegate(parsed);
    }

    fn launch_delegate(&mut self, parsed: DelegateInput) {
        if self.current_tab().delegate.pending.is_some() {
            self.current_tab_mut()
                .messages
                .push(ChatMessage::warning(t!("agents.loading").into_owned()));
            return;
        }
        if !self.current_tab().attachments.is_empty() {
            self.current_tab_mut().messages.push(ChatMessage::warning(
                t!("system.image_not_supported").into_owned(),
            ));
            return;
        }
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
        let window_id = self.window_id.clone();
        let window = window_id.as_deref().and_then(|id| id.parse().ok());
        let id = uuid::Uuid::new_v4();
        let tab = self.current_tab_mut();
        let retry_input = if let Some(draft) = &tab.delegate.draft {
            format!(
                "/new-agent --profile {} --target {} -- {}",
                draft.profile_id,
                if parsed.placement == Placement::Pane {
                    "pane"
                } else {
                    "tab"
                },
                parsed.task,
            )
        } else {
            tab.input.clone()
        };
        tab.reset_input_history_navigation();
        tab.remember_input_history(&retry_input);
        let history = if tab.delegate.draft.is_none() {
            tab.input.clone()
        } else if parsed.task.trim_start().starts_with("--") {
            format!("/new-agent -- {}", parsed.task)
        } else {
            format!("/new-agent {}", parsed.task)
        };
        tab.messages.push(ChatMessage::User(history));
        tab.scroll_to_bottom();
        tab.delegate.pending = Some(PendingDelegate {
            id,
            origin_window_id: window_id.clone(),
            origin_tab_id: tab_id.clone(),
            input: tab.input.clone(),
            draft: tab.delegate.draft.clone(),
            submitted_revision: tab.composer_revision,
            released_revision: None,
        });
        let span = tracing::info_span!(target: "delegate", "delegate_launch", request_id = %id);
        tokio::task::spawn_local(tracing::Instrument::instrument(
            async move {
                let started = std::time::Instant::now();
                let result = async {
                    let request = LaunchRequest {
                        profile: parsed.profile.as_deref(),
                        provider: None,
                        source_pane: source.as_deref(),
                        window_id: window,
                        cwd: None,
                        task: Some(&parsed.task),
                        include_context: true,
                        placement: parsed.placement,
                        direction: "right",
                        size: 0.5,
                        background: false,
                        title: None,
                    };
                    let snapshot =
                        crate::native_profile::capture_source_context(&shell, &request).await?;
                    tx.send(AppEvent::DelegateAccepted {
                        window_id: window_id.clone(),
                        tab_id: tab_id.clone(),
                        request_id: id,
                    })
                    .map_err(|_| anyhow::anyhow!("delegation owner is no longer available"))?;
                    crate::native_profile::launch_with_snapshot(&shell, request, snapshot).await
                }
                .await
                .map_err(|error| DelegateFailure {
                    outcome_unknown: error
                        .is::<crate::native_profile::PaneCreationOutcomeUnknown>(),
                    error: format!("{error:#}"),
                });
                tracing::info!(target: "delegate", stage = "launch_completed",
                elapsed_ms = started.elapsed().as_millis() as u64, success = result.is_ok());
                let created = result.as_ref().ok().cloned();
                if tx
                    .send(AppEvent::DelegateCompleted {
                        window_id,
                        tab_id,
                        request_id: id,
                        result,
                    })
                    .is_err()
                {
                    tracing::warn!(target: "delegate", "delegation completion owner is no longer available");
                }
                if let Some(created) = created {
                    crate::native_profile::finish_launch(&shell, &created, false).await;
                }
            },
            span,
        ));
    }
}
