use super::*;
use crate::app_contracts::PlanEntry;
use ratatui::{backend::TestBackend, Terminal};
use std::time::{Duration, Instant};

fn app_with_reply(source: &str) -> App {
    let mut app = super::tests::test_app();
    app.state = ConnectionState::Connected;
    let id = DEFAULT_TAB_ID.to_owned();
    app.session_to_tab.insert(id.clone(), id.clone());
    let tab = app.current_tab_mut();
    tab.session_id = Some(id);
    tab.turn = TurnState::Streaming {
        prompt: SubmittedPrompt {
            id: 42,
            text: "test completion".into(),
            submitted_at_unix_s: 0.0,
            context: TurnContext::default(),
            autofix: None,
        },
    };
    tab.messages.push(ChatMessage::Agent(source.into()));
    tab.reveal_chars = usize::MAX;
    app
}

fn render(app: &mut App) -> String {
    render_at(app, 90, 30)
}

fn render_at(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            crate::ui::render(frame, app);
            app.text_selection.snapshot_and_render(frame.buffer_mut());
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(width).max(1))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn select_visible_source(app: &mut App, source: &str) {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use unicode_width::UnicodeWidthStr;

    let text = render(app);
    let (row, column) = text
        .lines()
        .enumerate()
        .find_map(|(row, line)| {
            line.find(source)
                .map(|byte| (row as u16, line[..byte].width() as u16))
        })
        .unwrap();
    for (kind, offset) in [
        (MouseEventKind::Down(MouseButton::Left), 0),
        (
            MouseEventKind::Drag(MouseButton::Left),
            source.len() as u16 - 1,
        ),
        (
            MouseEventKind::Up(MouseButton::Left),
            source.len() as u16 - 1,
        ),
    ] {
        app.text_selection.handle_mouse(MouseEvent {
            kind,
            column: column + offset,
            row,
            modifiers: KeyModifiers::NONE,
        });
    }
    assert_eq!(app.text_selection.selected_text().as_deref(), Some(source));
}

fn wait_for_render(app: &mut App, width: u16, height: u16, ready: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let text = render_at(app, width, height);
        if ready(&text) {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "completed display did not become ready:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn completed_markdown_formats_only_normal_completed_turns() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "**COMPLETED_BOLD**";
    let mut app = app_with_reply(source);
    assert!(render(&mut app).contains(source));
    assert_eq!(
        app.current_tab()
            .agent_markdown
            .borrow()
            .stats()
            .batch_renders,
        0
    );
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    let final_text = wait_for_render(&mut app, 90, 30, |text| {
        text.contains("COMPLETED_BOLD") && !text.contains(source)
    });
    assert!(final_text.contains("COMPLETED_BOLD"), "{final_text}");
    assert!(!final_text.contains(source), "{final_text}");
    assert_eq!(
        app.current_tab().completed_turns[0].details,
        [ChatMessage::Agent(source.into())]
    );
    let before = app.current_tab().agent_markdown.borrow().stats();
    assert_eq!(before.batch_renders, 1);
    assert_eq!(before.processed_source_bytes, source.len() as u64);
    for _ in 0..5 {
        assert_eq!(render(&mut app), final_text);
        assert_eq!(app.current_tab().agent_markdown.borrow().stats(), before);
    }
}

#[test]
fn completed_markdown_cancelled_reply_remains_raw() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "**CANCELLED_BOLD**";
    let mut app = app_with_reply(source);
    app.turn_cancel(DEFAULT_TAB_ID);
    assert!(render(&mut app).contains(source));
    assert_eq!(
        app.current_tab()
            .agent_markdown
            .borrow()
            .stats()
            .batch_renders,
        0
    );
}

#[test]
fn completed_markdown_failed_reply_remains_raw() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "**FAILED_BOLD**";
    let mut app = app_with_reply(source);
    app.handle_event(AppEvent::PromptError {
        tab_id: DEFAULT_TAB_ID.into(),
        prompt_id: 42,
        message: "test failure".into(),
    });
    assert!(render(&mut app).contains(source));
    assert!(app.current_tab().completed_turns.is_empty());
    assert_eq!(
        app.current_tab()
            .agent_markdown
            .borrow()
            .stats()
            .batch_renders,
        0
    );
}

#[test]
fn completed_markdown_keeps_active_segments_and_tool_text_raw_until_turn_end() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let mut app = app_with_reply("**FIRST_SEGMENT**");
    app.current_tab_mut()
        .messages
        .insert(0, ChatMessage::User("**USER_LITERAL**".into()));
    app.handle_event(AppEvent::Plan {
        session_id: DEFAULT_TAB_ID.into(),
        entries: vec![PlanEntry {
            content: "**PLAN_LITERAL**".into(),
            status: PlanEntryStatus::InProgress,
        }],
    });
    app.handle_event(AppEvent::AgentMessageChunk {
        session_id: DEFAULT_TAB_ID.into(),
        text: "**LAST_SEGMENT**".into(),
    });
    app.current_tab_mut().reveal_chars = usize::MAX;
    let active = render(&mut app);
    for text in [
        "**FIRST_SEGMENT**",
        "**USER_LITERAL**",
        "**PLAN_LITERAL**",
        "**LAST_SEGMENT**",
    ] {
        assert!(active.contains(text), "{active}");
    }
    assert_eq!(
        app.current_tab()
            .agent_markdown
            .borrow()
            .stats()
            .batch_renders,
        0
    );
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    let final_text = wait_for_render(&mut app, 90, 30, |text| {
        text.contains("LAST_SEGMENT")
            && !text.contains("**FIRST_SEGMENT**")
            && !text.contains("**LAST_SEGMENT**")
    });
    assert!(final_text.contains("**PLAN_LITERAL**"));
    assert!(final_text.find("FIRST_SEGMENT") < final_text.find("PLAN_LITERAL"));
    assert!(final_text.find("PLAN_LITERAL") < final_text.find("LAST_SEGMENT"));
    assert_eq!(
        app.current_tab()
            .agent_markdown
            .borrow()
            .stats()
            .batch_renders,
        2
    );
    assert_eq!(app.current_tab().completed_turns[0].details.len(), 3);
}

#[test]
fn completed_markdown_replayed_history_is_rendered_without_rewriting_source() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "**RESTORED_BOLD**";
    let mut app = app_with_reply("");
    let tab = app.current_tab_mut();
    tab.turn = TurnState::Idle;
    tab.messages = vec![
        ChatMessage::User("restored prompt".into()),
        ChatMessage::Agent(source.into()),
    ];
    tab.pack_replayed_messages_into_turns();
    let text = wait_for_render(&mut app, 90, 30, |text| {
        text.contains("RESTORED_BOLD") && !text.contains(source)
    });
    assert!(text.contains("restored prompt"), "{text}");
    assert_eq!(
        app.current_tab().completed_turns[0].details,
        [ChatMessage::Agent(source.into())]
    );
    let json = serde_json::to_value(&app.current_tab().completed_turns[0]).unwrap();
    let object = json.as_object().unwrap();
    assert_eq!(object.len(), 4);
    assert_eq!(object["details"][0]["Agent"], source);
}

#[test]
fn completed_markdown_source_replacement_and_resize_do_not_reuse_stale_output() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "| A | B |\n| - | - |\n| LEFT_CELL | RIGHT_CELL |\n\n**FINAL_ROW**";
    let mut app = app_with_reply(source);
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    let wide = wait_for_render(&mut app, 90, 30, |text| {
        text.contains("FINAL_ROW") && !text.contains("**FINAL_ROW**")
    });
    let narrow = wait_for_render(&mut app, 40, 30, |text| {
        text.contains("FINAL_ROW") && !text.contains("**FINAL_ROW**")
    });
    for text in [&wide, &narrow] {
        assert!(
            text.contains("LEFT_CELL") && text.contains("RIGHT_CELL"),
            "{text}"
        );
    }
    assert_eq!(
        app.current_tab()
            .agent_markdown
            .borrow()
            .stats()
            .batch_renders,
        2
    );
    app.current_tab_mut().completed_turns[0].details[0] =
        ChatMessage::Agent("**REPLACED_ROW**".into());
    app.current_tab().invalidate_completed_turn_height(0);
    let replaced = wait_for_render(&mut app, 40, 30, |text| {
        text.contains("REPLACED_ROW") && !text.contains("**REPLACED_ROW**")
    });
    assert!(
        !replaced.contains("FINAL_ROW") && !replaced.contains("LEFT_CELL"),
        "{replaced}"
    );
}

#[test]
fn completed_markdown_worker_redraws_idle_completed_history() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let mut app = app_with_reply("**IDLE_RESULT**");
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    render(&mut app);
    assert!(app.event_requires_redraw(&AppEvent::Tick));
    wait_for_render(&mut app, 90, 30, |text| {
        text.contains("IDLE_RESULT") && !text.contains("**IDLE_RESULT**")
    });
    assert!(!app.current_tab().agent_markdown.borrow().has_pending());
}

#[test]
fn completed_markdown_collapsed_visible_history_reuses_its_cached_batch() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "**CACHED_HISTORY**";
    let mut app = app_with_reply(source);
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    let formatted = wait_for_render(&mut app, 90, 30, |text| {
        text.contains("CACHED_HISTORY") && !text.contains(source)
    });
    let before = app.current_tab().agent_markdown.borrow().stats();
    app.current_tab_mut().completed_turns[0].expanded = false;
    app.current_tab().invalidate_completed_turn_height(0);
    assert!(!render(&mut app).contains("CACHED_HISTORY"));
    app.current_tab_mut().completed_turns[0].expanded = true;
    app.current_tab().invalidate_completed_turn_height(0);
    assert_eq!(render(&mut app), formatted);
    assert_eq!(app.current_tab().agent_markdown.borrow().stats(), before);
}

fn formatted_history_with_newer_raw_reply(show_popup: bool) -> (App, String, usize, usize) {
    let source = "**OLD_WORD** ".repeat(80) + "\n\n**OLD_END**";
    let mut app = app_with_reply(&source);
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    wait_for_render(&mut app, 40, 18, |text| {
        text.contains("OLD_END") && !text.contains("**OLD_END**")
    });
    let wrap_width = 38;
    let formatted_height = app
        .current_tab()
        .cached_completed_turn_height(0, wrap_width)
        .expect("the rendered turn has a height at the chat's actual width");
    app.turn_submit_prompt(
        DEFAULT_TAB_ID,
        SubmittedPrompt {
            id: 43,
            text: "newer prompt".into(),
            submitted_at_unix_s: 0.0,
            context: TurnContext::default(),
            autofix: None,
        },
    );
    app.handle_event(AppEvent::AgentMessageChunk {
        session_id: DEFAULT_TAB_ID.into(),
        text: "NEWER_RAW_ROW\n".repeat(60),
    });
    app.current_tab_mut().reveal_chars = usize::MAX;
    app.turn_cancel(DEFAULT_TAB_ID);
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    assert_eq!(app.current_tab().completed_turns.len(), 2);
    if show_popup {
        let tab = app.current_tab_mut();
        tab.input = "/".into();
        tab.cursor_pos = 1;
        tab.refresh_command_popup();
        assert!(app.command_popup_state().is_some());
    }
    let frame = render_at(&mut app, 40, 18);
    assert!(!frame.contains("OLD_END"), "{frame}");
    (app, source, wrap_width, formatted_height)
}

#[test]
fn completed_markdown_evicted_history_is_pruned_without_a_command_popup() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let (app, _, width, _) = formatted_history_with_newer_raw_reply(false);
    let key = super::agent_markdown::MarkdownKey {
        turn: 0,
        message: 0,
    };
    assert_eq!(app.current_tab().agent_markdown.borrow().source(key), None);
    assert_eq!(
        app.current_tab().cached_completed_turn_height(0, width),
        None
    );
}

#[test]
fn completed_markdown_evicted_history_rebuilds_raw_geometry_before_selection() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let (mut app, source, width, formatted_height) = formatted_history_with_newer_raw_reply(true);
    let key = super::agent_markdown::MarkdownKey {
        turn: 0,
        message: 0,
    };
    assert_eq!(app.current_tab().agent_markdown.borrow().source(key), None);
    assert_eq!(
        app.current_tab().cached_completed_turn_height(0, width),
        None
    );
    app.current_tab_mut().input.clear();
    app.current_tab_mut().refresh_command_popup();

    let mut raw = app_with_reply(&source);
    raw.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    raw.current_tab_mut().completed_markdown_turns.clear();
    render_at(&mut raw, 40, 18);
    let raw_height = raw
        .current_tab()
        .cached_completed_turn_height(0, width)
        .unwrap();
    assert_ne!(raw_height, formatted_height);

    assert!(app.current_tab_mut().select_completed_turn(0));
    let revisited = render_at(&mut app, 40, 18);
    assert!(revisited.contains("**OLD_WORD**"), "{revisited}");
    assert_eq!(
        app.current_tab().cached_completed_turn_height(0, width),
        Some(raw_height)
    );
    wait_for_render(&mut app, 40, 18, |text| {
        text.contains("OLD_WORD") && !text.contains("**OLD_WORD**")
    });
    assert_eq!(
        app.current_tab().cached_completed_turn_height(0, width),
        Some(formatted_height)
    );
    let stats = app.current_tab().agent_markdown.borrow().stats();
    assert_eq!(stats.batch_renders, 2);
    for _ in 0..3 {
        render_at(&mut app, 40, 18);
        assert_eq!(app.current_tab().agent_markdown.borrow().stats(), stats);
    }
}

#[test]
fn completed_markdown_evicted_newer_history_does_not_rebuild_during_older_navigation() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let mut app = app_with_reply(&("**OLDER_WORD** ".repeat(80) + "\n\n**OLDER_END**"));
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    wait_for_render(&mut app, 40, 18, |text| {
        text.contains("OLDER_END") && !text.contains("**OLDER_END**")
    });
    app.turn_submit_prompt(
        DEFAULT_TAB_ID,
        SubmittedPrompt {
            id: 43,
            text: "newer prompt".into(),
            submitted_at_unix_s: 0.0,
            context: TurnContext::default(),
            autofix: None,
        },
    );
    app.handle_event(AppEvent::AgentMessageChunk {
        session_id: DEFAULT_TAB_ID.into(),
        text: "**NEWER_WORD** ".repeat(80) + "\n\n**NEWER_END**",
    });
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    wait_for_render(&mut app, 40, 18, |text| {
        text.contains("NEWER_END") && !text.contains("**NEWER_END**")
    });
    assert!(app.current_tab_mut().select_completed_turn(0));
    wait_for_render(&mut app, 40, 18, |text| {
        text.contains("OLDER_WORD") && !text.contains("**OLDER_WORD**")
    });
    assert!(
        !app.current_tab().agent_markdown.borrow().has_pending(),
        "measuring evicted offscreen history must not queue another Markdown job"
    );
    let stats = app.current_tab().agent_markdown.borrow().stats();
    for _ in 0..5 {
        let frame = render_at(&mut app, 40, 18);
        assert_eq!(app.current_tab().agent_markdown.borrow().stats(), stats);
        assert!(!app.current_tab().agent_markdown.borrow().has_pending());
        assert!(frame.contains("OLDER_WORD") && !frame.contains("**OLDER_WORD**"));
    }
}

#[test]
fn completed_markdown_defers_the_display_change_while_selecting_raw_text() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "**SELECT_RAW**";
    let mut app = app_with_reply(source);
    select_visible_source(&mut app, source);
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let text = render(&mut app);
        assert!(text.contains(source), "{text}");
        assert_eq!(app.text_selection.selected_text().as_deref(), Some(source));
        if !app.current_tab().agent_markdown.borrow().has_pending() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "selected raw completion did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    app.text_selection.clear();
    wait_for_render(&mut app, 90, 30, |text| {
        text.contains("SELECT_RAW") && !text.contains(source)
    });
    assert_eq!(
        app.current_tab()
            .agent_markdown
            .borrow()
            .stats()
            .batch_renders,
        1
    );
}

#[test]
fn completed_markdown_deferred_completion_releases_queued_prompts_once_in_order() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "**SELECTED_FIRST_REPLY**";
    let mut app = app_with_reply(source);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.prompt_tx = tx;
    for text in ["second prompt", "third prompt"] {
        let tab = app.current_tab_mut();
        tab.input = text.into();
        tab.cursor_pos = text.len();
        tab.refresh_command_popup();
        app.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
    }
    assert_eq!(app.current_tab().prompt_queue.entries.len(), 2);
    assert!(rx.try_recv().is_err());
    select_visible_source(&mut app, source);
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    for _ in 0..3 {
        app.handle_event(AppEvent::Tick);
        assert!(render(&mut app).contains(source));
        assert_eq!(app.text_selection.selected_text().as_deref(), Some(source));
        assert!(app.current_tab().completed_turns.is_empty());
        assert!(rx.try_recv().is_err());
    }
    assert_eq!(
        app.current_tab()
            .agent_markdown
            .borrow()
            .stats()
            .batch_renders,
        0
    );

    app.handle_event(AppEvent::Key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE,
    )));
    let second = rx.try_recv().unwrap();
    assert_eq!(second.text, "second prompt");
    assert_eq!(app.current_tab().turn.prompt_id(), Some(second.id));
    assert_eq!(app.current_tab().prompt_queue.entries.len(), 1);
    assert!(app.current_tab().deferred_agent_end.is_none());
    assert_eq!(app.current_tab().completed_turns.len(), 1);
    assert_eq!(
        app.current_tab().completed_turns[0].details[0],
        ChatMessage::Agent(source.into())
    );
    wait_for_render(&mut app, 90, 30, |text| {
        text.contains("SELECTED_FIRST_REPLY") && !text.contains(source)
    });
    for _ in 0..3 {
        app.handle_event(AppEvent::Tick);
        assert!(rx.try_recv().is_err());
        assert_eq!(app.current_tab().completed_turns.len(), 1);
    }
    app.handle_event(AppEvent::AgentMessageChunk {
        session_id: DEFAULT_TAB_ID.into(),
        text: "**SECOND_REPLY**".into(),
    });
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    let third = rx.try_recv().unwrap();
    assert_eq!(third.text, "third prompt");
    assert_eq!(app.current_tab().turn.prompt_id(), Some(third.id));
    assert!(app.current_tab().prompt_queue.entries.is_empty());
    assert_eq!(app.current_tab().completed_turns.len(), 2);
    assert!(rx.try_recv().is_err());
}

#[test]
fn completed_markdown_clear_discards_inflight_and_completed_derived_state() {
    let _locale = crate::test_support::lock_locale();
    rust_i18n::set_locale("en-US");
    let source = "**OLD_CONTENT**\n\n".repeat(300);
    let mut app = app_with_reply(&source);
    app.handle_event(AppEvent::AgentMessageEnd {
        session_id: DEFAULT_TAB_ID.into(),
    });
    render(&mut app);
    app.current_tab_mut().clear_chat_history();
    app.current_tab_mut().clear_completed_turns();
    app.current_tab_mut()
        .messages
        .push(ChatMessage::Agent("new raw reply".into()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.current_tab().agent_markdown.borrow().has_pending() {
        assert!(!render(&mut app).contains("OLD_CONTENT"));
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!render(&mut app).contains("OLD_CONTENT"));
    assert!(app.current_tab().completed_markdown_turns.is_empty());
}
