//! Behavior tests for the agent-pane slash-command system, split out of the
//! large `app.rs` / `commands.rs` test modules so all of it lives in one
//! place: the pure `commands::classify` mapping and the `App` dispatch path.
//!
//! This is a child module of `app` (declared with `#[path]` in app.rs), not
//! of the crate root, so it can reach `App`'s private dispatch methods —
//! exactly like the inline `app::tests` module it borrows `test_app` from.

use super::tests::{
    complete_autofix_capture, test_app, test_app_with_master_rx, test_app_with_new_session_rx,
};
use super::*;
use crate::protocol::acp::client::MasterExtRequest;
use std::{sync::Mutex, time::Duration};

/// Dispatch a zero-arg slash command by name through the real
/// `handle_slash_command` path, the way the Enter handler does.
fn run_slash(app: &mut App, name: &str) {
    let spec = commands::lookup(name).expect("name is a registered command");
    app.handle_slash_command(ParsedCommand {
        kind: spec.kind,
        spec,
        rest: String::new(),
    });
}

fn seed_delegate_profiles(app: &mut App) {
    app.delegate_profiles = serde_json::from_value(serde_json::json!([
        {"guid":"{claude-project}","name":"Claude Project","agent_id":"claude","default":true},
        {"guid":"{claude-review}","name":"Claude Review","agent_id":"claude"}
    ]))
    .unwrap();
}

#[test]
fn delegate_tab_completes_while_enter_opens_profile_picker_without_launching() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    seed_delegate_profiles(&mut app);
    app.delegate_profiles.swap(0, 1);
    app.current_tab_mut().replace_input("/dele".into());
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.current_tab().input, "/delegate ");
    assert!(app.current_tab().delegate.picker.is_none());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let picker = app.current_tab().delegate.picker.as_ref().unwrap();
    assert_eq!(picker.control, delegate::DelegateControl::Profile);
    assert_eq!(
        picker.selected, 1,
        "configured default, not the first profile"
    );
    assert_eq!(app.delegate_profile_candidates().len(), 2);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let draft = app.current_tab().delegate.draft.as_ref().unwrap();
    assert_eq!(draft.profile_id, "{claude-project}");
    assert_eq!(draft.placement, crate::native_profile::Placement::Tab);
    assert!(app.current_tab().input.is_empty());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.current_tab().messages.is_empty());
    assert!(app.test_prompt_rx.as_mut().unwrap().try_recv().is_err());
}

#[test]
fn delegate_reselecting_profile_and_placement_preserves_task_and_exact_identity() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    seed_delegate_profiles(&mut app);
    app.current_tab_mut().replace_input("/delegate".into());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.current_tab_mut()
        .replace_input("fix the build\nkeep --target literal".into());
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    for character in "Review".chars() {
        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        )));
    }
    assert_eq!(app.delegate_profile_candidates().len(), 1);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.current_tab()
            .delegate
            .draft
            .as_ref()
            .unwrap()
            .profile_id,
        "{claude-review}"
    );
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.current_tab().delegate.draft.as_ref().unwrap().placement,
        crate::native_profile::Placement::Pane
    );
    assert_eq!(
        app.current_tab().input,
        "fix the build\nkeep --target literal"
    );
    assert!(!app.command_popup_visible());
    assert!(app.prepared_command_range().is_none());
    assert!(app.test_prompt_rx.as_mut().unwrap().try_recv().is_err());
}

#[test]
fn delegate_escape_preserves_input_at_each_picker_level_and_never_sends_it() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    seed_delegate_profiles(&mut app);
    app.current_tab_mut().replace_input("/delegate ".into());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.current_tab().input, "/delegate ");
    assert!(app.current_tab().delegate.picker.is_none());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.current_tab_mut()
        .replace_input("keep this draft".into());
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.current_tab().delegate.draft.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.current_tab().delegate.draft.is_none());
    assert_eq!(app.current_tab().input, "keep this draft");
    assert!(app.test_prompt_rx.as_mut().unwrap().try_recv().is_err());
}

#[test]
fn delegate_missing_task_retains_draft_and_never_submits_to_current_agent() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    app.current_tab_mut()
        .replace_input("/delegate --target pane --profile {chosen} ".into());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.current_tab().input,
        "/delegate --target pane --profile {chosen} "
    );
    assert!(app.test_prompt_rx.as_mut().unwrap().try_recv().is_err());
    assert_eq!(last_notice(&app).0, NoticeKind::Warning);
}

#[test]
fn delegate_enter_on_a_completed_turn_never_submits_or_executes_the_task() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let _locale = crate::test_support::lock_locale();
    for task in ["fix the build", "/clear"] {
        let mut app = test_app();
        seed_delegate_profiles(&mut app);
        run_slash(&mut app, "delegate");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.state = ConnectionState::Connected;
        app.current_tab_mut().session_id = Some("current-session".into());
        app.current_tab_mut().replace_input(task.into());
        let draft = app.current_tab().delegate.draft.clone();
        app.current_tab_mut().completed_turns.push(CompletedTurn {
            prompt: "previous work".into(),
            details: vec![ChatMessage::Agent("previous result".into())],
            expanded: false,
            trailing_marker: None,
        });
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20)).unwrap();
        terminal
            .draw(|frame| crate::ui::render(frame, &mut app))
            .unwrap();
        let hit = app
            .completed_turn_hits
            .iter()
            .find(|hit| hit.kind == CompletedTurnHitKind::UserInput)
            .copied()
            .unwrap();
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            app.handle_event(AppEvent::Mouse(MouseEvent {
                kind,
                column: hit.start_column + 4,
                row: hit.row,
                modifiers: KeyModifiers::NONE,
            }));
        }
        assert_eq!(app.current_tab().selected_completed_turn_idx, Some(0));
        assert!(!app.current_tab().input_has_nav_focus());
        let expanded = app.current_tab().completed_turns[0].expanded;

        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert_eq!(app.current_tab().input, task);
        assert_eq!(app.current_tab().delegate.draft, draft);
        assert!(app.current_tab().delegate.pending.is_none());
        assert!(app.current_tab().messages.is_empty());
        assert!(app.current_tab().prompt_queue.entries.is_empty());
        assert!(app.test_prompt_rx.as_mut().unwrap().try_recv().is_err());
        assert_ne!(app.current_tab().completed_turns[0].expanded, expanded);
    }
}

#[test]
fn delegate_completion_does_not_recreate_a_closed_tab() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    let count = app.tab_sessions.len();
    app.handle_event(AppEvent::DelegateCompleted {
        window_id: app.window_id.clone(),
        tab_id: "closed-tab".into(),
        request_id: uuid::Uuid::new_v4(),
        result: Ok(()),
    });
    assert_eq!(app.tab_sessions.len(), count);
    assert!(!app.tab_sessions.contains_key("closed-tab"));
}

struct DelegateChannel {
    requests: Mutex<Vec<(String, serde_json::Value)>>,
    fail: AtomicBool,
}

#[async_trait::async_trait]
impl crate::shell::wt_channel::WtChannel for DelegateChannel {
    async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        use serde_json::json;
        self.requests
            .lock()
            .unwrap()
            .push((method.into(), params.clone()));
        Ok(match method {
            "get_delegate_context" => json!({
                "pane": {"session_id":"source","window_id":42,"cwd":r"C:\Source Project"},
                "content":"recent work pane output",
            }),
            "agent_profiles" => {
                if let Some(profile) = params["profile"].as_str() {
                    json!([{
                        "guid":profile,"name":"Claude Project","agent_id":"claude",
                        "commandline":r#""C:\Agent Tools\claude.exe" --model profile-model"#,
                    }])
                } else {
                    json!([
                        {"guid":"{claude-project}","name":"Claude Project","agent_id":"claude","default":true},
                        {"guid":"{claude-review}","name":"Claude Review","agent_id":"claude"},
                    ])
                }
            }
            "create_tab" | "split_pane" => {
                anyhow::ensure!(!self.fail.load(Ordering::Relaxed), "fixture launch failed");
                json!({"session_id":"created"})
            }
            "publish" => json!({}),
            _ => anyhow::bail!("unexpected delegate fixture request: {method}"),
        })
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn delegate_submission_is_one_shot_restores_failures_and_keeps_source_context() {
    let _locale = crate::test_support::lock_locale();
    tokio::task::LocalSet::new()
        .run_until(async {
            for with_picker in [true, false] {
                let mut app = test_app();
                seed_delegate_profiles(&mut app);
                app.pane_id = Some("owning-helper".into());
                app.window_id = Some("7".into());
                if with_picker {
                    app.current_tab_mut().replace_input("/delegate".into());
                    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
                    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                }
                let task = if with_picker {
                    "--target literal task\nfix the failure"
                } else {
                    "fix the failure\nkeep --target tab in the task"
                };
                let input = if with_picker {
                    task.to_string()
                } else {
                    format!("/delegate {task}")
                };
                app.current_tab_mut().replace_input(input.clone());
                let channel = Arc::new(DelegateChannel {
                    requests: Mutex::new(Vec::new()),
                    fail: AtomicBool::new(true),
                });
                app.shell_mgr =
                    Arc::new(crate::shell::ShellManager::new().with_wt_channel(channel.clone()));
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                app.event_tx = Some(tx);
                let session = app.current_tab().session_id.clone();
                app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                    .await
                    .unwrap()
                    .unwrap();
                app.handle_event(event);
                assert_eq!(app.current_tab().input, input);
                assert_eq!(app.current_tab().delegate.draft.is_some(), with_picker);
                assert!(app.current_tab().delegate.pending.is_none());
                assert!(matches!(
                    app.current_tab().messages.last(),
                    Some(ChatMessage::Error(_))
                ));
                channel.fail.store(false, Ordering::Relaxed);
                app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                let origin = app.active_tab_key().to_string();
                app.tab_mut("other").replace_input("other tab draft".into());
                app.tab_id = Some("other".into());
                let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                    .await
                    .unwrap()
                    .unwrap();
                app.handle_event(event);
                assert_eq!(app.current_tab().input, "other tab draft");
                let original = &app.tab_sessions[&origin];
                assert!(original.input.is_empty());
                assert!(original.delegate.draft.is_none());
                assert!(original.delegate.pending.is_none());
                assert_eq!(original.session_id, session);
                assert_eq!(
                    original
                        .messages
                        .iter()
                        .filter(|message| matches!(message, ChatMessage::User(_)))
                        .count(),
                    1
                );
                assert!(app.test_prompt_rx.as_mut().unwrap().try_recv().is_err());
                let requests = channel.requests.lock().unwrap();
                assert_eq!(
                    requests
                        .iter()
                        .filter(|(method, _)| method == "get_delegate_context")
                        .count(),
                    2
                );
                let (_, context) = &requests[0];
                assert_eq!(context["session_id"], "owning-helper");
                assert_eq!(context["max_lines"], 30);
                assert_eq!(context["max_chars"], 4000);
                let launches = requests
                    .iter()
                    .filter(|(method, _)| matches!(method.as_str(), "create_tab" | "split_pane"))
                    .collect::<Vec<_>>();
                assert_eq!(launches.len(), 2, "duplicate Enter must not launch twice");
                for (method, params) in launches {
                    assert_eq!(
                        method,
                        if with_picker {
                            "split_pane"
                        } else {
                            "create_tab"
                        }
                    );
                    assert_eq!(
                        params["profile"],
                        if with_picker {
                            "{claude-review}"
                        } else {
                            "{claude-project}"
                        }
                    );
                    assert_eq!(params["window_id"], 42);
                    assert_eq!(params["cwd"], r"C:\Source Project");
                    let command = params["commandline"].as_str().unwrap();
                    if !with_picker {
                        assert!(command.contains(task));
                        assert!(command.contains("recent work pane output"));
                    }
                }
            }
        })
        .await;
}

#[tokio::test]
async fn delegate_completion_follows_owner_transfer_and_allows_retry() {
    let _locale = crate::test_support::lock_locale();
    tokio::task::LocalSet::new()
        .run_until(async {
            for rename_tab in [false, true] {
                for fail in [false, true] {
                    let mut app = test_app();
                    app.switch_tab_session("source-tab".into());
                    app.owner_tab_id = Some("source-tab".into());
                    app.window_id = Some("7".into());
                    app.pane_id = Some("owning-helper".into());
                    seed_delegate_profiles(&mut app);
                    run_slash(&mut app, "delegate");
                    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                    app.current_tab_mut().replace_input("fix the build".into());
                    let draft = app.current_tab().delegate.draft.clone();
                    let channel = Arc::new(DelegateChannel {
                        requests: Mutex::new(Vec::new()),
                        fail: AtomicBool::new(fail),
                    });
                    app.shell_mgr = Arc::new(
                        crate::shell::ShellManager::new().with_wt_channel(channel.clone()),
                    );
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                    app.event_tx = Some(tx);
                    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                    let request_id = app.current_tab().delegate.pending.as_ref().unwrap().id;
                    let new_tab = if rename_tab {
                        "moved-tab"
                    } else {
                        "source-tab"
                    };
                    app.handle_event(AppEvent::TabRenamed {
                        old_tab_id: "source-tab".into(),
                        new_tab_id: new_tab.into(),
                        new_window_id: Some("8".into()),
                    });
                    assert_eq!(app.window_id.as_deref(), Some("8"));
                    assert_eq!(app.active_tab_key(), new_tab);
                    app.handle_event(AppEvent::DelegateCompleted {
                        window_id: Some("8".into()),
                        tab_id: new_tab.into(),
                        request_id,
                        result: Ok(()),
                    });
                    assert!(app.current_tab().delegate.pending.is_some());

                    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                        .await
                        .unwrap()
                        .unwrap();
                    assert!(matches!(&event, AppEvent::DelegateCompleted {
                        window_id: Some(window), tab_id, request_id: id, ..
                    } if window == "7" && tab_id == "source-tab" && *id == request_id));
                    app.handle_event(event);

                    assert!(app.current_tab().delegate.pending.is_none());
                    if fail {
                        assert_eq!(app.current_tab().input, "fix the build");
                        assert_eq!(app.current_tab().delegate.draft, draft);
                        assert!(matches!(
                            app.current_tab().messages.last(),
                            Some(ChatMessage::Error(_))
                        ));
                        channel.fail.store(false, Ordering::Relaxed);
                        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                        assert!(app.current_tab().delegate.pending.is_some());
                        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                            .await
                            .unwrap()
                            .unwrap();
                        app.handle_event(event);
                    }
                    assert!(app.current_tab().delegate.pending.is_none());
                    assert!(app.current_tab().delegate.draft.is_none());
                    assert!(app.current_tab().input.is_empty());
                    assert!(app.test_prompt_rx.as_mut().unwrap().try_recv().is_err());
                }
            }
        })
        .await;
}

#[test]
fn delegate_completion_does_not_discard_a_newer_draft_or_accept_another_window() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    app.window_id = Some("original".into());
    app.current_tab_mut().replace_input("newer draft".into());
    let id = uuid::Uuid::new_v4();
    let origin_window_id = app.window_id.clone();
    let origin_tab_id = app.active_tab_key().to_string();
    app.current_tab_mut().delegate.pending = Some(delegate::PendingDelegate {
        id,
        origin_window_id: origin_window_id.clone(),
        origin_tab_id: origin_tab_id.clone(),
        input: "old task".into(),
        draft: None,
        history: "/delegate old task".into(),
    });
    for (window_id, tab_id, request_id) in [
        (Some("other".into()), origin_tab_id.clone(), id),
        (origin_window_id.clone(), "other-tab".into(), id),
        (
            origin_window_id.clone(),
            origin_tab_id.clone(),
            uuid::Uuid::new_v4(),
        ),
    ] {
        app.handle_event(AppEvent::DelegateCompleted {
            window_id,
            tab_id,
            request_id,
            result: Ok(()),
        });
        assert!(app.current_tab().delegate.pending.is_some());
        assert!(app.current_tab().messages.is_empty());
    }
    app.handle_event(AppEvent::DelegateCompleted {
        window_id: app.window_id.clone(),
        tab_id: app.active_tab_key().into(),
        request_id: id,
        result: Ok(()),
    });
    assert_eq!(app.current_tab().input, "newer draft");
    assert!(app.current_tab().delegate.pending.is_none());
}

#[test]
fn delegate_controls_and_picker_support_mouse_without_leaking_to_chat() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    seed_delegate_profiles(&mut app);
    app.current_tab_mut().replace_input("/delegate".into());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.current_tab_mut().replace_input("keep this task".into());
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 16)).unwrap();
    terminal
        .draw(|frame| crate::ui::render(frame, &mut app))
        .unwrap();
    let profile_area = app
        .delegate_control_hits
        .iter()
        .find(|(_, control)| *control == delegate::DelegateControl::Profile)
        .unwrap()
        .0;
    let click = |area: ratatui::layout::Rect| {
        AppEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        })
    };
    let original_tab = app.tab_id.clone();
    app.tab_mut("other-tab");
    app.tab_id = Some("other-tab".into());
    app.handle_event(click(profile_area));
    assert!(
        app.current_tab().delegate.picker.is_none(),
        "stale hit belongs to the previous tab"
    );
    app.tab_id = original_tab;
    app.handle_event(click(profile_area));
    assert!(app.current_tab().delegate.picker.is_some());
    terminal
        .draw(|frame| crate::ui::render(frame, &mut app))
        .unwrap();
    let choice_area = app
        .delegate_control_hits
        .iter()
        .find(|(_, control)| *control == delegate::DelegateControl::Pick(1))
        .unwrap()
        .0;
    app.handle_event(click(choice_area));
    assert_eq!(
        app.current_tab()
            .delegate
            .draft
            .as_ref()
            .unwrap()
            .profile_id,
        "{claude-review}"
    );
    assert_eq!(app.current_tab().input, "keep this task");
    assert!(app.test_prompt_rx.as_mut().unwrap().try_recv().is_err());

    for index in 0..10 {
        let mut profile = app.delegate_profiles[0].clone();
        profile.guid = format!("{{copy-{index}}}");
        profile.name = format!("Copied profile {index}");
        profile.default = false;
        app.delegate_profiles.push(profile);
    }
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    for _ in 0..20 {
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    terminal
        .draw(|frame| crate::ui::render(frame, &mut app))
        .unwrap();
    let (first_row, delegate::DelegateControl::Pick(first_index)) = app.delegate_control_hits[0]
    else {
        panic!("expected a scrolled picker row");
    };
    assert!(first_index > 0);
    let expected_profile = app.delegate_profile_candidates()[first_index].guid.clone();
    app.handle_event(click(first_row));
    assert_eq!(
        app.current_tab()
            .delegate
            .draft
            .as_ref()
            .unwrap()
            .profile_id,
        expected_profile
    );
    assert_eq!(app.current_tab().input, "keep this task");
}

fn custom_model(selection_id: &str, model_id: &str) -> CustomModelCatalogEntry {
    CustomModelCatalogEntry {
        selection_id: selection_id.into(),
        api_contract: crate::custom_model_provider::CANONICAL_API_CONTRACT.into(),
        model_id: model_id.into(),
        ..Default::default()
    }
}

fn last_notice(app: &App) -> (NoticeKind, &str) {
    match app.current_tab().messages.last() {
        Some(ChatMessage::Notice { kind, text }) => (*kind, text),
        other => panic!("expected an inline notice, got {other:?}"),
    }
}

fn session_command(
    name: &str,
    description: &str,
    input_hint: Option<&str>,
) -> crate::app_contracts::AcpSessionCommand {
    let completion_behavior = if input_hint.is_some() {
        crate::app_contracts::CompletionBehavior::OptionalFreeText
    } else {
        crate::app_contracts::CompletionBehavior::ExecuteImmediately
    };
    session_command_with_behavior(name, description, input_hint, completion_behavior)
}

fn session_command_with_behavior(
    name: &str,
    description: &str,
    input_hint: Option<&str>,
    completion_behavior: crate::app_contracts::CompletionBehavior,
) -> crate::app_contracts::AcpSessionCommand {
    crate::app_contracts::AcpSessionCommand {
        name: name.into(),
        description: description.into(),
        input_hint: input_hint.map(str::to_string),
        completion_behavior,
    }
}

fn popup_command_names(app: &App) -> Vec<String> {
    match app.command_popup_state().expect("command popup").candidates {
        crate::ui::PopupCandidates::Commands(candidates) => candidates
            .into_iter()
            .map(|candidate| match candidate {
                crate::ui::CommandCandidate::Client(spec) => spec.name.to_string(),
                crate::ui::CommandCandidate::Agent(command) => command.name.clone(),
            })
            .collect(),
        _ => panic!("expected slash-command candidates"),
    }
}

// ---- commands::classify — the pure input → intent mapping ----

#[test]
fn classify_known_command() {
    match commands::classify("/stop") {
        ParseOutcome::Command(c) => assert_eq!(c.kind, CommandKind::Stop),
        other => panic!("expected Command, got {other:?}"),
    }
    match commands::classify("/help me please") {
        ParseOutcome::Command(c) => {
            assert_eq!(c.kind, CommandKind::Help);
            assert_eq!(c.rest, "me please");
        }
        other => panic!("expected Command, got {other:?}"),
    }
}

#[test]
fn classify_unknown_keeps_attempted_token() {
    // Token carries its leading `/`, and trailing args are dropped from it.
    assert_eq!(
        commands::classify("/nope"),
        ParseOutcome::Unknown("/nope".to_string())
    );
    assert_eq!(
        commands::classify("/nope foo bar"),
        ParseOutcome::Unknown("/nope".to_string())
    );
    // Leading whitespace is trimmed before the token is taken.
    assert_eq!(
        commands::classify("   /missing"),
        ParseOutcome::Unknown("/missing".to_string())
    );
}

#[test]
fn classify_not_a_command() {
    assert_eq!(commands::classify("hello"), ParseOutcome::NotCommand);
    // `//literal` escape is a prompt, not an unknown-command warning.
    assert_eq!(commands::classify("//etc/hosts"), ParseOutcome::NotCommand);
    // Bare slash / whitespace-only slash have no token to name.
    assert_eq!(commands::classify("/"), ParseOutcome::NotCommand);
    assert_eq!(commands::classify("/  "), ParseOutcome::NotCommand);
    // A `/` in the middle of a prompt is not an attempt.
    assert_eq!(
        commands::classify("run cmd /flag"),
        ParseOutcome::NotCommand
    );
}

#[test]
fn agent_commands_merge_after_reserved_commands_and_replace_by_session() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-1".into(),
        commands: vec![
            session_command("plan", "Build a plan", None),
            session_command("clear", "Agent collision", None),
        ],
    });
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-2".into(),
        commands: vec![session_command("research", "Research", None)],
    });
    type_input(&mut app, "/");

    let names = popup_command_names(&app);
    assert_eq!(&names[..commands::REGISTRY.len()], {
        &commands::REGISTRY
            .iter()
            .map(|spec| spec.name.to_string())
            .collect::<Vec<_>>()[..]
    });
    assert_eq!(
        names.iter().filter(|name| name.as_str() == "clear").count(),
        1,
        "an Agent command must not shadow a reserved WTA command"
    );
    assert!(names.contains(&"plan".to_string()));
    assert!(!names.contains(&"research".to_string()));

    app.current_tab_mut().clear_input();
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-1".into(),
        commands: vec![session_command("review", "Review changes", None)],
    });
    type_input(&mut app, "/");
    let names = popup_command_names(&app);
    assert!(!names.contains(&"plan".to_string()));
    assert!(names.contains(&"review".to_string()));
}

#[test]
fn agent_command_without_input_submits_as_a_normal_prompt() {
    let mut app = test_app();
    app.state = ConnectionState::Connected;
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-1".into(),
        commands: vec![session_command("plan", "Build a plan", None)],
    });
    type_input(&mut app, "/plan");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.current_tab().input.is_empty());
    assert!(app.current_tab().turn.is_in_flight());
    assert!(!app.current_tab().messages.iter().any(|message| matches!(
        message,
        ChatMessage::Notice {
            kind: NoticeKind::Warning,
            ..
        }
    )));
}

#[test]
fn provider_yolo_command_is_not_reserved_and_submits_as_a_normal_prompt() {
    assert!(
        commands::lookup("yolo").is_none(),
        "WTA must not reserve a provider-owned yolo command"
    );

    let mut app = test_app();
    app.state = ConnectionState::Connected;
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-1".into(),
        commands: vec![session_command("yolo", "Provider Yolo mode", None)],
    });
    type_input(&mut app, "/yolo");

    assert_eq!(popup_command_names(&app), vec!["yolo"]);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.current_tab().input.is_empty());
    assert!(app.current_tab().turn.is_in_flight());
}

#[test]
fn agent_command_with_optional_input_enters_prepared_mode() {
    let mut app = test_app();
    app.state = ConnectionState::Connected;
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-1".into(),
        commands: vec![session_command(
            "review",
            "Review changes",
            Some("focus area"),
        )],
    });
    type_input(&mut app, "/rev");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.current_tab().input, "/review ");
    assert_eq!(app.command_ghost_suffix(), Some("focus area"));
    assert_eq!(app.prepared_command_range(), Some(0..7));
    assert!(app.current_tab().turn.is_idle());
}

#[test]
fn required_free_text_metadata_drives_the_complete_prepared_flow() {
    let mut app = test_app();
    app.state = ConnectionState::Connected;
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-1".into(),
        commands: vec![session_command_with_behavior(
            "intent",
            "Provide an intent",
            None,
            crate::app_contracts::CompletionBehavior::RequireFreeText,
        )],
    });
    type_input(&mut app, "/int");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.current_tab().input, "/intent ");
    assert_eq!(app.prepared_command_range(), Some(0..7));
    assert!(app.current_tab().turn.is_idle());

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.current_tab().input, "/intent ");
    assert!(app.current_tab().turn.is_idle());
    assert_eq!(
        last_notice(&app),
        (NoticeKind::Warning, "Provide an intent")
    );

    type_input(&mut app, "describe the task");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.current_tab().input.is_empty());
    assert!(app.current_tab().turn.is_in_flight());
}

#[test]
fn agent_prefix_match_ranks_before_client_substring_match() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.model_picker_models = vec![AcpModelInfo {
        id: "test-model".into(),
        name: "Test model".into(),
        description: None,
    }];
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-1".into(),
        commands: vec![session_command("odyssey", "Synthetic command", None)],
    });
    type_input(&mut app, "/od");

    let names = popup_command_names(&app);

    assert_eq!(names, vec!["odyssey", "model"]);

    app.command_popup_down();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        app.current_tab().model_picker_open,
        "the selected Client candidate must dispatch from the combined ranking"
    );
}

#[test]
fn optional_fix_completion_prepares_then_second_enter_runs() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    app.state = ConnectionState::Connected;
    app.source_session_id = Some("source".into());
    let tab_id = app.active_tab_key().to_owned();
    type_input(&mut app, "/fi");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.current_tab().input, "/fix ");
    assert_eq!(app.prepared_command_range(), Some(0..4));
    assert!(app.current_tab().turn.is_idle());

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.current_tab().input.is_empty());
    assert!(app.current_tab().turn.is_idle());
    complete_autofix_capture(&mut app, &tab_id);
    assert!(!app.current_tab().turn.is_idle());
}

#[test]
fn typed_agent_command_with_arguments_has_no_unknown_warning() {
    let mut app = test_app();
    app.state = ConnectionState::Connected;
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionCommandsUpdated {
        session_id: "session-1".into(),
        commands: vec![session_command(
            "review",
            "Review changes",
            Some("focus area"),
        )],
    });
    type_input(&mut app, "/review parser");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.current_tab().turn.is_in_flight());
    assert!(!app.current_tab().messages.iter().any(|message| matches!(
        message,
        ChatMessage::Notice {
            kind: NoticeKind::Warning,
            ..
        }
    )));
}

// ---- App dispatch — state effects via handle_slash_command ----

#[test]
fn slash_help_toggles_overlay() {
    let mut app = test_app();
    assert!(!app.help_overlay_visible);
    run_slash(&mut app, "help");
    assert!(app.help_overlay_visible);
    run_slash(&mut app, "help");
    assert!(!app.help_overlay_visible);
}

#[test]
fn slash_clear_wipes_active_tab_conversation() {
    let mut app = test_app();
    app.current_tab_mut()
        .messages
        .push(ChatMessage::System("stale".into()));
    app.current_tab_mut().selected_completed_turn_idx = Some(0);

    run_slash(&mut app, "clear");

    assert!(app.current_tab().messages.is_empty());
    assert_eq!(app.current_tab().selected_completed_turn_idx, None);
}

#[test]
fn slash_stop_when_idle_notes_nothing_to_stop() {
    let mut app = test_app();
    // Fresh tab: turn is Idle, so /stop only emits the advisory message.
    assert!(!app.current_tab().turn.is_in_flight());

    run_slash(&mut app, "stop");

    assert_eq!(app.current_tab().messages.len(), 1);
    assert_eq!(last_notice(&app).0, NoticeKind::Info);
}

#[test]
fn slash_stop_while_cancelling_is_idempotent() {
    let mut app = test_app();
    let prompt = crate::protocol::acp::client::PromptSubmission::new("stop this".into(), None);
    let cancellation = prompt.cancellation_token();
    app.turn_submit_prompt_for_tab_with_cancellation(
        DEFAULT_TAB_ID,
        SubmittedPrompt {
            id: prompt.id,
            text: prompt.text,
            submitted_at_unix_s: prompt.submitted_at_unix_s,
            context: TurnContext::default(),
            autofix: None,
        },
        cancellation,
    );

    run_slash(&mut app, "stop");
    run_slash(&mut app, "stop");

    assert!(app.current_tab().turn.is_cancelling());
    assert_eq!(
        app.current_tab()
            .messages
            .iter()
            .filter(|message| matches!(
                message,
                ChatMessage::Notice {
                    kind: NoticeKind::Success,
                    ..
                }
            ))
            .count(),
        1
    );
}

#[test]
fn slash_new_when_idle_resets_session() {
    let (mut app, _new_session_rx) = test_app_with_new_session_rx();
    app.current_tab_mut().session_id = Some("sid-1".into());
    app.current_tab_mut()
        .messages
        .push(ChatMessage::System("stale".into()));

    run_slash(&mut app, "new");

    assert_eq!(app.current_tab().session_id, None);
    assert!(app.current_tab().messages.is_empty());
}

#[test]
fn runtime_global_change_reconciles_all_live_sessions() {
    let (mut app, mut master_rx) = test_app_with_master_rx();
    app.session_to_tab
        .insert("session-a".into(), DEFAULT_TAB_ID.into());
    app.session_to_tab
        .insert("session-b".into(), DEFAULT_TAB_ID.into());

    app.apply_runtime_yolo_config(Some(true), Some(false));

    let request = master_rx.try_recv().expect("reconcile request");
    let MasterExtRequest::ReconcileSessionYolo {
        sessions,
        fail_closed,
        ..
    } = request
    else {
        panic!("expected ReconcileSessionYolo");
    };
    assert!(!fail_closed);
    assert!(sessions
        .iter()
        .any(|(session, enabled)| session.0.as_ref() == "session-a" && *enabled));
    assert!(sessions
        .iter()
        .any(|(session, enabled)| session.0.as_ref() == "session-b" && *enabled));
}

#[test]
fn runtime_policy_block_forces_off_and_clears_session_override() {
    let (mut app, mut master_rx) = test_app_with_master_rx();
    app.session_to_tab
        .insert("session".into(), DEFAULT_TAB_ID.into());
    app.yolo_state.lock().unwrap().update_runtime(true, false);

    app.apply_runtime_yolo_config(Some(false), Some(true));

    assert_eq!(
        app.yolo_state
            .lock()
            .unwrap()
            .automatic_directive("session"),
        crate::app_contracts::AutomaticYoloDirective::Disable
    );
    let request = master_rx.try_recv().expect("reconcile request");
    let MasterExtRequest::ReconcileSessionYolo {
        sessions,
        fail_closed,
        ..
    } = request
    else {
        panic!("expected ReconcileSessionYolo");
    };
    assert!(fail_closed);
    assert_eq!(sessions.len(), 1);
    assert!(!sessions[0].1);

    app.apply_runtime_yolo_config(Some(false), Some(false));
    assert_eq!(
        app.yolo_state
            .lock()
            .unwrap()
            .automatic_directive("session"),
        crate::app_contracts::AutomaticYoloDirective::Disable,
        "policy removal must leave the current global default off"
    );
}

#[test]
fn session_attach_reconciles_latest_yolo_state() {
    let (mut app, mut master_rx) = test_app_with_master_rx();
    app.yolo_state.lock().unwrap().update_runtime(true, false);

    app.handle_event(AppEvent::SessionAttached {
        tab_id: DEFAULT_TAB_ID.into(),
        session_id: "new-yolo-session".into(),
        prompt_id: None,
        available_models: Vec::new(),
        current_model_id: None,
    });

    let request = master_rx.try_recv().expect("attach reconcile request");
    let MasterExtRequest::ReconcileSessionYolo {
        sessions,
        fail_closed,
        ..
    } = request
    else {
        panic!("expected ReconcileSessionYolo");
    };
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].0 .0.as_ref(), "new-yolo-session");
    assert!(sessions[0].1);
    assert!(!fail_closed);
}

#[test]
fn client_reconciled_session_attach_does_not_duplicate_native_yolo_rpc() {
    let (mut app, mut master_rx) = super::tests::test_app_with_master_rx();
    let session_id = "client-reconciled-lazy-session";
    app.yolo_state
        .lock()
        .unwrap()
        .mark_client_reconciled(session_id.to_string(), false);

    app.handle_event(AppEvent::SessionAttached {
        tab_id: DEFAULT_TAB_ID.into(),
        session_id: session_id.into(),
        prompt_id: None,
        available_models: Vec::new(),
        current_model_id: None,
    });

    assert!(
        master_rx.try_recv().is_err(),
        "the client-owned lazy reconciliation must suppress the App duplicate"
    );
}

/// Dispatch a slash command with free-form args (e.g. `/model gpt-5`) through
/// the same `handle_slash_command` path the Enter handler uses.
fn run_slash_args(app: &mut App, name: &str, rest: &str) {
    let spec = commands::lookup(name).expect("name is a registered command");
    app.handle_slash_command(ParsedCommand {
        kind: spec.kind,
        spec,
        rest: rest.to_string(),
    });
}

#[test]
fn slash_sessions_opens_agents_view() {
    let mut app = test_app();
    assert_eq!(app.current_tab().current_view, View::Chat);

    run_slash(&mut app, "sessions");

    assert_eq!(
        app.current_tab().current_view,
        View::Agents,
        "/sessions must switch the active tab to the session-management view"
    );
}

#[test]
fn slash_restart_resets_connection_and_clears_sessions() {
    let mut app = test_app();
    let shell_mgr = Arc::clone(&app.shell_mgr);
    app.set_master_pipe_acp_params(
        r"\\.\pipe\test-master".into(),
        "test-agent".into(),
        Some("claude".into()),
        None,
        None,
        crate::agent_source::AgentSource::Host,
        None,
        Some("owner-tab".into()),
        shell_mgr,
        true,
    );
    app.state = ConnectionState::Connected;
    app.session_id = "live-sid".to_string();
    app.current_tab_mut().session_id = Some("tab-sid".into());
    app.current_tab_mut()
        .messages
        .push(ChatMessage::System("stale".into()));

    run_slash(&mut app, "restart");

    assert!(
        matches!(app.state, ConnectionState::Connecting(_)),
        "/restart must move the connection into Connecting while the stack respawns"
    );
    assert!(
        app.session_id.is_empty(),
        "/restart must clear the process-level session id"
    );
    assert_eq!(
        app.current_tab().session_id,
        None,
        "/restart must drop each tab's session so the next prompt gets a fresh one"
    );
    assert!(
        app.current_tab().messages.is_empty(),
        "/restart must wipe per-tab chat history"
    );
    assert!(
        app.restart_without_acp_pending,
        "a closed lifecycle receiver must leave the helper waiting for replacement-master readiness"
    );
    app.handle_event(AppEvent::AgentTransportRetired);

    app.handle_event(AppEvent::WtEvent {
        method: "agent_master_restarted".into(),
        pane_id: String::new(),
        tab_id: None,
        params: serde_json::json!({ "operation_id": "restart-after-failed-startup" }),
    });

    assert!(
        app.pending_acp_start,
        "replacement-master readiness must reconnect when the original ACP task already exited"
    );
    assert!(!app.restart_without_acp_pending);
}

#[test]
fn slash_fix_when_idle_submits_autofix_turn() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    app.state = ConnectionState::Connected;
    app.source_session_id = Some("source".into());
    let tab_id = app.active_tab_key().to_owned();
    let gen_before = app.current_tab().autofix.generation;
    assert!(app.current_tab().turn.is_idle());

    run_slash(&mut app, "fix");
    assert!(app.current_tab().turn.is_idle());
    complete_autofix_capture(&mut app, &tab_id);

    assert!(
        !app.current_tab().turn.is_idle(),
        "/fix on an idle tab must submit an autofix turn"
    );
    assert_eq!(
        app.current_tab().autofix.generation,
        gen_before.wrapping_add(1),
        "/fix must bump the autofix generation so stale responses are dropped"
    );
}

#[test]
fn slash_fix_while_busy_queues_without_replacing_active_turn() {
    let _locale = crate::test_support::lock_locale();
    let mut app = test_app();
    app.state = ConnectionState::Connected;
    app.source_session_id = Some("source".into());
    let tab_id = app.active_tab_key().to_owned();
    run_slash(&mut app, "fix");
    complete_autofix_capture(&mut app, &tab_id);
    assert!(!app.current_tab().turn.is_idle());
    let gen_after_first = app.current_tab().autofix.generation;

    // Second /fix is queued without replacing the active turn.
    run_slash(&mut app, "fix");
    complete_autofix_capture(&mut app, &tab_id);
    assert_eq!(
        app.current_tab().autofix.generation,
        gen_after_first,
        "/fix while a turn is in flight must not bump generation / resubmit"
    );
    assert_eq!(app.current_tab().prompt_queue.entries.len(), 1);
}

#[test]
fn slash_model_without_models_notes_none() {
    let mut app = test_app();
    assert!(app.available_models.is_empty());

    run_slash(&mut app, "model");

    assert!(
        !app.current_tab().model_picker_open,
        "/model must not open the picker when no models are available"
    );
    assert_eq!(last_notice(&app).0, NoticeKind::Info);
}

fn config_option(
    id: &str,
    name: &str,
    category: &str,
    current_value: &str,
) -> crate::app_contracts::AcpSessionConfigOption {
    crate::app_contracts::AcpSessionConfigOption {
        id: id.into(),
        name: name.into(),
        description: Some(format!("Configure {name}")),
        category: Some(category.into()),
        current_value: current_value.into(),
        values: vec![
            crate::app_contracts::AcpSessionConfigValue {
                id: "ask".into(),
                name: "Ask".into(),
                description: None,
            },
            crate::app_contracts::AcpSessionConfigValue {
                id: "code".into(),
                name: "Code".into(),
                description: Some("Write code".into()),
            },
        ],
        native_yolo: false,
    }
}

#[test]
fn slash_config_without_options_notes_none() {
    let mut app = test_app();

    run_slash(&mut app, "config");

    assert!(!app.current_tab().config_picker.is_open());
    assert_eq!(last_notice(&app).0, NoticeKind::Info);
}

#[test]
fn slash_config_opens_ordered_session_options() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![
            config_option("mode", "Mode", "mode", "ask"),
            config_option("reasoning", "Reasoning", "thought_level", "code"),
        ],
    });

    run_slash(&mut app, "config");

    let state = app.config_popup_state().expect("config picker state");
    assert_eq!(state.options[0].name, "Mode");
    assert_eq!(state.options[1].name, "Reasoning");
    assert!(state.value_option.is_none());
}

#[test]
fn slash_model_opens_the_standard_model_config_option() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![config_option("agent-model", "Model", "model", "ask")],
    });

    run_slash(&mut app, "model");

    assert!(!app.current_tab().model_picker_open);
    assert!(app.current_tab().config_picker.is_open());
    assert_eq!(
        app.current_tab().config_picker.option_id(),
        Some("agent-model")
    );
}

#[test]
fn escape_from_slash_model_closes_the_config_picker() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![config_option("agent-model", "Model", "model", "ask")],
    });

    run_slash(&mut app, "model");
    app.config_picker_escape();

    assert!(!app.current_tab().config_picker.is_open());
}

#[test]
fn escape_from_config_value_picker_returns_to_option_list() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![config_option("agent-model", "Model", "model", "ask")],
    });

    run_slash(&mut app, "config");
    app.config_picker_enter();
    app.config_picker_escape();

    assert!(app.current_tab().config_picker.is_open());
    assert!(app.current_tab().config_picker.option_id().is_none());
}

#[test]
fn slash_model_selection_uses_the_generic_config_request_lifecycle() {
    let (mut app, mut master_rx) = super::tests::test_app_with_master_rx();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![config_option("agent-model", "Model", "model", "ask")],
    });

    run_slash(&mut app, "model");
    app.config_picker_down();
    app.config_picker_enter();

    match master_rx.try_recv().expect("model config update request") {
        crate::protocol::acp::client::MasterExtRequest::SetSessionConfigOption {
            session_id,
            config_id,
            value,
        } => {
            assert_eq!(session_id.to_string(), "session-1");
            assert_eq!(config_id, "agent-model");
            assert_eq!(value, "code");
        }
        other => panic!("expected SetSessionConfigOption, got {other:?}"),
    }
    assert!(!app.current_tab().config_picker.is_open());
    assert_eq!(
        app.current_tab().config_pending_id.as_deref(),
        Some("agent-model")
    );
    assert!(app.current_tab().model_override.is_none());

    app.handle_event(AppEvent::SessionConfigSetCompleted {
        session_id: "session-1".into(),
        config_id: "agent-model".into(),
        value: "code".into(),
        model_compat: true,
    });

    assert!(app.current_tab().config_pending_id.is_none());
    assert_eq!(app.current_tab().model_override.as_deref(), Some("code"));
    assert!(
        app.config_popup_state().is_none(),
        "the /model deep link remains closed after completion"
    );
}

#[test]
fn failed_model_config_selection_keeps_the_previous_model() {
    let (mut app, _master_rx) = super::tests::test_app_with_master_rx();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![config_option("agent-model", "Model", "model", "ask")],
    });

    run_slash(&mut app, "model");
    app.config_picker_down();
    app.config_picker_enter();
    app.handle_event(AppEvent::SessionConfigSetFailed {
        session_id: "session-1".into(),
        config_id: "agent-model".into(),
        message: "rejected".into(),
        restart_required: false,
    });

    assert!(app.current_tab().config_pending_id.is_none());
    assert!(app.current_tab().model_override.is_none());
    assert_eq!(last_notice(&app).0, NoticeKind::Error);
}

#[test]
fn config_picker_select_sends_session_scoped_option_request() {
    let (mut app, mut master_rx) = super::tests::test_app_with_master_rx();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![config_option("mode", "Mode", "mode", "ask")],
    });
    run_slash(&mut app, "config");

    app.config_picker_enter();
    assert_eq!(app.current_tab().config_picker.option_id(), Some("mode"));
    app.config_picker_down();
    app.config_picker_enter();

    match master_rx.try_recv().expect("config update request") {
        crate::protocol::acp::client::MasterExtRequest::SetSessionConfigOption {
            session_id,
            config_id,
            value,
        } => {
            assert_eq!(session_id.to_string(), "session-1");
            assert_eq!(config_id, "mode");
            assert_eq!(value, "code");
        }
        other => panic!("expected SetSessionConfigOption, got {other:?}"),
    }
    assert_eq!(app.current_tab().config_pending_id.as_deref(), Some("mode"));

    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![config_option("mode", "Mode", "mode", "code")],
    });
    app.handle_event(AppEvent::SessionConfigSetCompleted {
        session_id: "session-1".into(),
        config_id: "mode".into(),
        value: "code".into(),
        model_compat: false,
    });

    assert_eq!(
        app.config_popup_state().unwrap().options[0].current_value,
        "code"
    );
    assert!(app.current_tab().config_pending_id.is_none());
    assert_eq!(last_notice(&app).0, NoticeKind::Success);
    assert!(last_notice(&app).1.contains("Mode: Code"));
}

#[test]
fn failed_native_config_dispatch_clears_prompt_gate() {
    let (mut app, master_rx) = super::tests::test_app_with_master_rx();
    drop(master_rx);
    app.current_tab_mut().session_id = Some("session-1".into());
    let mut option = config_option("mode", "Mode", "mode", "ask");
    option.native_yolo = true;
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "session-1".into(),
        options: vec![option],
    });
    run_slash(&mut app, "config");
    app.config_picker_enter();
    app.config_picker_down();

    app.config_picker_enter();

    assert!(app.current_tab().config_pending_id.is_none());
    assert!(!app.current_tab().native_yolo_config_pending);
    assert!(matches!(
        app.current_tab().messages.last(),
        Some(ChatMessage::Error(message)) if message.contains("/restart")
    ));
}

#[test]
fn restart_required_native_config_failure_keeps_prompt_gate_until_reset() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("session-1".into());
    app.current_tab_mut().config_pending_id = Some("mode".into());
    app.current_tab_mut().native_yolo_config_pending = true;
    app.session_to_tab
        .insert("session-1".into(), DEFAULT_TAB_ID.into());

    app.handle_event(AppEvent::SessionConfigSetFailed {
        session_id: "session-1".into(),
        config_id: "mode".into(),
        message: "provider rejected disable".into(),
        restart_required: true,
    });

    assert_eq!(app.current_tab().config_pending_id.as_deref(), Some("mode"));
    assert!(app.current_tab().native_yolo_config_pending);
    assert!(app.prompt_reconfiguration_pending_for_tab(DEFAULT_TAB_ID));

    app.reset_agent_scoped_state();
    assert!(app.current_tab().config_pending_id.is_none());
    assert!(!app.current_tab().native_yolo_config_pending);
}

#[test]
fn unbound_background_config_update_does_not_close_current_picker() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("current-session".into());
    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "current-session".into(),
        options: vec![config_option("mode", "Mode", "mode", "ask")],
    });
    run_slash(&mut app, "config");

    app.handle_event(AppEvent::SessionConfigUpdated {
        session_id: "background-session".into(),
        options: Vec::new(),
    });

    assert!(app.current_tab().config_picker.is_open());
    assert_eq!(app.config_popup_state().unwrap().options[0].id, "mode");
}

#[test]
fn unbound_background_config_failure_does_not_pollute_current_tab() {
    let mut app = test_app();
    app.current_tab_mut().session_id = Some("current-session".into());
    let message_count = app.current_tab().messages.len();

    app.handle_event(AppEvent::SessionConfigSetFailed {
        session_id: "closed-session".into(),
        config_id: "mode".into(),
        message: "the session is no longer active".into(),
        restart_required: false,
    });

    assert_eq!(app.current_tab().messages.len(), message_count);
}

#[test]
fn slash_model_bare_opens_picker_when_models_present() {
    let mut app = test_app();
    let selected = "custom:provider:local";
    app.set_custom_model_config(vec![custom_model(selected, "local")], Some(selected.into()));

    run_slash(&mut app, "model");

    assert!(
        app.current_tab().model_picker_open,
        "bare /model must open the model picker when models are available"
    );
}

#[test]
fn slash_model_shows_cloud_models() {
    let mut app = test_app();
    app.set_cloud_models(vec![AcpModelInfo {
        id: "cloud".into(),
        name: "Cloud".into(),
        description: None,
    }]);

    run_slash(&mut app, "model");

    assert!(app.current_tab().model_picker_open);
    assert_eq!(app.model_picker_models[0].id, "cloud");
}

#[test]
fn custom_provider_models_replace_agent_duplicates_and_use_byok_labels() {
    let mut app = test_app();
    app.set_custom_model_config(
        vec![
            custom_model("custom:provider-one:qwen/qwen3.5-9b", "qwen/qwen3.5-9b"),
            custom_model(
                "custom:provider-two:deepseek/deepseek-v4-flash",
                "deepseek/deepseek-v4-flash",
            ),
        ],
        Some("custom:provider-two:deepseek/deepseek-v4-flash".into()),
    );

    let merged = app.merge_custom_models(vec![
        AcpModelInfo {
            id: "intelligent-terminal/deepseek/deepseek-v4-flash".into(),
            name: "deepseek/deepseek-v4-flash".into(),
            description: None,
        },
        AcpModelInfo {
            id: "native".into(),
            name: "Native".into(),
            description: None,
        },
    ]);

    assert_eq!(merged.len(), 3);
    assert!(merged.iter().any(|model| model.id == "native"));
    assert!(merged.iter().any(|model| {
        model.id == "custom:provider-one:qwen/qwen3.5-9b" && model.name == "qwen/qwen3.5-9b (BYOK)"
    }));
    assert!(merged.iter().any(|model| {
        model.id == "custom:provider-two:deepseek/deepseek-v4-flash"
            && model.name == "deepseek/deepseek-v4-flash (BYOK)"
    }));
    assert_eq!(
        app.current_model_id.as_deref(),
        Some("custom:provider-two:deepseek/deepseek-v4-flash")
    );
}

#[test]
fn custom_provider_models_normalize_metadata_and_drop_empty_entries() {
    let mut app = test_app();
    app.set_custom_model_config(
        vec![
            custom_model("  custom:provider:model  ", "  provider/model  "),
            custom_model("   ", "  ignored/model  "),
            custom_model("  custom:provider:ignored  ", "   "),
        ],
        Some("  custom:provider:model  ".into()),
    );

    assert_eq!(
        app.custom_model_catalog,
        vec![custom_model("custom:provider:model", "provider/model")]
    );
    assert_eq!(app.available_models.len(), 1);
    assert_eq!(app.available_models[0].id, "custom:provider:model");
    assert_eq!(app.available_models[0].name, "provider/model (BYOK)");
    assert_eq!(
        app.current_model_id.as_deref(),
        Some("custom:provider:model")
    );
}

#[test]
fn helper_status_catalog_combines_cloud_agent_and_byok_models() {
    let mut app = test_app();
    app.set_cloud_models(vec![AcpModelInfo {
        id: "shared-model".into(),
        name: "Shared cloud model".into(),
        description: None,
    }]);
    app.set_custom_model_config(
        vec![custom_model(
            "custom:provider-one:shared-model",
            "shared-model",
        )],
        None,
    );
    app.handle_event(AppEvent::AgentConnected {
        name: "Test Agent".into(),
        model: None,
        version: None,
        session_id: "session-1".into(),
        available_models: vec![AcpModelInfo {
            id: "agent-only".into(),
            name: "Agent model".into(),
            description: None,
        }],
        current_model_id: Some("agent-only".into()),
        load_session_supported: false,
        image_supported: false,
        session_capabilities_ready: true,
        telemetry_byok_binding: None,
    });

    assert_eq!(app.available_models.len(), 3);
    assert!(app
        .available_models
        .iter()
        .any(|model| model.id == "shared-model"));
    assert!(app
        .available_models
        .iter()
        .any(|model| model.id == "agent-only"));
    assert!(app
        .available_models
        .iter()
        .any(|model| model.id == "custom:provider-one:shared-model"
            && model.name == "shared-model (BYOK)"));
    assert_eq!(app.model_picker_models.len(), 2);
    assert!(app
        .model_picker_models
        .iter()
        .all(|model| !model.id.starts_with("custom:")));
}

#[test]
fn private_cloud_catalog_survives_bare_agent_model_response() {
    let mut app = test_app();
    app.set_custom_model_config(vec![custom_model("custom:provider:byok", "byok")], None);
    app.handle_event(AppEvent::CloudModelsAvailable(vec![AcpModelInfo {
        id: "cloud-native".into(),
        name: "Cloud Native".into(),
        description: None,
    }]));
    app.handle_event(AppEvent::AgentConnected {
        name: "Test Agent".into(),
        model: None,
        version: None,
        session_id: "session-1".into(),
        available_models: Vec::new(),
        current_model_id: None,
        load_session_supported: false,
        image_supported: false,
        session_capabilities_ready: true,
        telemetry_byok_binding: None,
    });

    assert_eq!(app.cloud_models.len(), 1);
    assert_eq!(app.cloud_models[0].id, "cloud-native");
    assert!(
        app.agent_models.is_empty(),
        "private cloud metadata must not be reclassified as an ACP selector"
    );
    assert!(app
        .available_models
        .iter()
        .any(|model| model.id == "cloud-native"));
    assert!(app
        .available_models
        .iter()
        .any(|model| model.id == "custom:provider:byok"));
}

#[test]
fn agent_and_model_pickers_are_mutually_exclusive() {
    let mut app = test_app();
    let selected = "custom:provider:local";
    app.set_custom_model_config(vec![custom_model(selected, "local")], Some(selected.into()));

    app.open_model_picker();
    assert!(app.current_tab().model_picker_open);
    assert!(!app.current_tab().agent_picker_open);

    app.open_agent_picker(0);
    assert!(app.current_tab().agent_picker_open);
    assert!(!app.current_tab().model_picker_open);

    app.open_model_picker();
    assert!(app.current_tab().model_picker_open);
    assert!(!app.current_tab().agent_picker_open);
}

#[test]
fn slash_model_direct_current_byok_is_a_noop() {
    let mut app = test_app();
    let selected = "custom:provider:smart";
    app.set_custom_model_config(vec![custom_model(selected, "smart")], Some(selected.into()));

    run_slash_args(&mut app, "model", selected);

    assert_eq!(
        app.current_tab().model_override.as_deref(),
        None,
        "confirming the current BYOK row must not create a pane override"
    );
    assert!(
        !app.current_tab().model_picker_open,
        "confirming the current BYOK model must not leave the picker open"
    );
}

#[test]
fn slash_model_only_shows_cloud_choices_while_cloud_is_active() {
    let mut app = test_app();
    app.set_cloud_models(vec![AcpModelInfo {
        id: "cloud".into(),
        name: "Cloud".into(),
        description: None,
    }]);
    app.set_custom_model_config(vec![custom_model("custom:provider:local", "local")], None);
    app.current_model_id = Some("cloud".into());

    let state = {
        app.open_model_picker();
        app.model_popup_state().expect("picker state")
    };
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].id, "cloud");
    assert_eq!(state.disabled, vec![false]);

    app.close_model_picker();
    run_slash_args(&mut app, "model", "custom:provider:local");
    assert_eq!(app.current_tab().model_override, None);
    assert!(!app.current_tab().model_picker_open);
    assert_eq!(last_notice(&app).0, NoticeKind::Error);
}

#[test]
fn slash_model_only_shows_selected_byok_while_byok_is_active() {
    let mut app = test_app();
    let selected = "custom:provider:local";
    app.set_cloud_models(vec![AcpModelInfo {
        id: "cloud".into(),
        name: "Cloud".into(),
        description: None,
    }]);
    app.set_custom_model_config(
        vec![
            custom_model(selected, "local"),
            custom_model("custom:provider:other", "other"),
        ],
        Some(selected.into()),
    );

    app.open_model_picker();
    let state = app.model_popup_state().expect("picker state");
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].id, selected);
    assert_eq!(state.disabled, vec![false]);

    app.close_model_picker();
    run_slash_args(&mut app, "model", "custom:provider:other");
    assert_eq!(app.current_tab().model_override, None);
    assert!(!app.current_tab().model_picker_open);
    assert_eq!(last_notice(&app).0, NoticeKind::Error);

    run_slash_args(&mut app, "model", "cloud");
    assert_eq!(app.current_tab().model_override, None);
    assert!(!app.current_tab().model_picker_open);
    assert_eq!(last_notice(&app).0, NoticeKind::Error);
}

#[test]
fn slash_move_changes_only_the_active_tab() {
    let mut app = test_app();
    app.tab_sessions
        .insert("other-tab".to_string(), TabSession::default());

    run_slash_args(&mut app, "move", "l");

    assert_eq!(
        app.current_tab().agent_pane_position,
        Some("left"),
        "/move l must normalize to the canonical left position"
    );
    assert_eq!(
        app.tab_sessions["other-tab"].agent_pane_position, None,
        "/move must not alter another tab's pane position"
    );
}

#[test]
fn slash_move_down_uses_bottom_pane_position() {
    let mut app = test_app();

    run_slash_args(&mut app, "move", "down");

    assert_eq!(
        app.current_tab().agent_pane_position,
        Some("bottom"),
        "/move down must map to the Terminal pane position named bottom"
    );
}

#[test]
fn slash_move_invalid_argument_reopens_position_completion() {
    let mut app = test_app();

    run_slash_args(&mut app, "move", "sideways");

    assert_eq!(app.current_tab().input, "/move ");
    assert_eq!(
        app.current_tab().move_position_candidates.len(),
        commands::MOVE_POSITIONS.len()
    );
    assert!(app.command_popup_state().is_some());
}

#[test]
fn move_position_popup_completes_alias_and_dispatches() {
    let mut app = test_app();
    type_input(&mut app, "/move r");

    assert_eq!(app.current_tab().move_position_candidates.len(), 1);
    assert_eq!(
        app.current_tab().selected_move_position().unwrap().name,
        "right"
    );
    assert!(app.try_handle_slash_on_enter());
    assert_eq!(app.current_tab().agent_pane_position, Some("right"));
    assert!(app.current_tab().input.is_empty());
}

#[test]
fn explicit_empty_agent_allowlist_is_fail_closed() {
    let mut app = test_app();
    app.set_allowed_agent_ids(vec![String::new()]);
    assert!(app.available_agents.is_empty());
}

#[test]
fn switch_agent_event_is_scoped_to_window_and_tab() {
    let payload = build_switch_agent_event(
        "42",
        "{tab-guid}",
        "claude",
        &crate::agent_source::AgentSource::Wsl {
            distro: "Ubuntu".to_string(),
        },
    );
    let event: serde_json::Value = serde_json::from_str(&payload).expect("valid event json");
    assert_eq!(event["method"], "switch_agent");
    assert_eq!(event["params"]["window_id"], "42");
    assert_eq!(event["params"]["tab_id"], "{tab-guid}");
    assert_eq!(event["params"]["agent_id"], "claude");
    assert_eq!(event["params"]["agent_source"], "wsl");
    assert_eq!(event["params"]["wsl_distro"], "Ubuntu");
}

fn seed_completion_agents(app: &mut App) {
    app.available_agents = vec![
        AvailableAgent {
            id: "copilot".into(),
            display_name: "GitHub Copilot".into(),
            source: crate::agent_source::AgentSource::Host,
        },
        AvailableAgent {
            id: "codex".into(),
            display_name: "Codex".into(),
            source: crate::agent_source::AgentSource::Host,
        },
        AvailableAgent {
            id: "gemini".into(),
            display_name: "Gemini".into(),
            source: crate::agent_source::AgentSource::Host,
        },
    ];
}

#[test]
fn agent_argument_completion_uses_available_agents_in_registry_order() {
    let mut app = test_app();
    seed_completion_agents(&mut app);
    type_input(&mut app, "/AGENT CO");

    let state = app.command_popup_state().expect("agent candidates");
    let crate::ui::PopupCandidates::Agents(candidates) = state.candidates else {
        panic!("expected agent candidates");
    };
    assert_eq!(
        candidates
            .iter()
            .map(|agent| agent.id.as_str())
            .collect::<Vec<_>>(),
        vec!["copilot", "codex"]
    );
    assert_eq!(app.command_ghost_suffix(), Some("pilot"));
}

#[test]
fn agent_trailing_space_opens_completion_with_all_agents() {
    let mut app = test_app();
    seed_completion_agents(&mut app);
    type_input(&mut app, "/agent ");

    let state = app.command_popup_state().expect("all agent candidates");
    let crate::ui::PopupCandidates::Agents(candidates) = state.candidates else {
        panic!("expected agent candidates");
    };
    assert_eq!(
        candidates
            .iter()
            .map(|agent| agent.id.as_str())
            .collect::<Vec<_>>(),
        vec!["copilot", "codex", "gemini"]
    );

    let highlighted = app.selected_agent_command_candidate();
    assert_eq!(highlighted.map(|agent| agent.id.as_str()), Some("copilot"));
    let command =
        agent_command_on_enter(&app.current_tab().input, highlighted).expect("agent command");
    assert_eq!(command.kind, CommandKind::Agent);
    assert_eq!(
        command.rest, "copilot",
        "Enter must dispatch the highlighted agent once the completion list is visible"
    );
}

#[test]
fn agent_argument_arrow_changes_ghost_but_tab_does_not_complete() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = test_app();
    seed_completion_agents(&mut app);
    type_input(&mut app, "/agent co");

    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.command_ghost_suffix(), Some("dex"));

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.current_tab().input, "/agent co");
    assert_eq!(app.command_ghost_suffix(), Some("dex"));
}

#[test]
fn agent_ghost_requires_cursor_at_end() {
    let mut app = test_app();
    seed_completion_agents(&mut app);
    type_input(&mut app, "/agent co");
    app.current_tab_mut().move_cursor_left();

    assert_eq!(app.command_ghost_suffix(), None);
}

#[test]
fn agent_argument_enter_dispatches_highlighted_agent() {
    let mut app = test_app();
    seed_completion_agents(&mut app);
    type_input(&mut app, "/agent co");

    let highlighted = app.selected_agent_command_candidate();
    let command =
        agent_command_on_enter(&app.current_tab().input, highlighted).expect("agent command");
    assert_eq!(command.kind, CommandKind::Agent);
    assert_eq!(command.rest, "copilot");
}

#[test]
fn unknown_agent_prefix_does_not_open_completion() {
    let mut app = test_app();
    seed_completion_agents(&mut app);
    type_input(&mut app, "/agent zzz");

    assert!(app.command_popup_state().is_none());
    assert_eq!(app.command_ghost_suffix(), None);
}

/// Type `text` char-by-char through the real input path so the command popup
/// candidates refresh exactly as they do live.
fn type_input(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.current_tab_mut().insert_input_char(ch);
    }
}

#[test]
fn connected_popup_visible_for_any_prefix() {
    let mut app = test_app();
    type_input(&mut app, "/ne");

    assert!(
        app.command_popup_visible(),
        "a matching command prefix must keep the popup visible"
    );
}

#[test]
fn connected_popup_matches_command_name_substrings() {
    let mut app = test_app();
    type_input(&mut app, "/lear");

    assert!(
        app.command_popup_visible(),
        "typing a substring of /clear must show the command popup"
    );
}
