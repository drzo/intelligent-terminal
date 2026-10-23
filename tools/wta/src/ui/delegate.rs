use ratatui::prelude::*;
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::popup;
use crate::app::delegate::DelegateControl;
use crate::app::App;
use crate::native_profile::Placement;
use crate::theme;

pub(super) fn render_controls(frame: &mut Frame, app: &mut App, area: Rect) {
    let tab = app.current_tab();
    let Some(draft) = &tab.delegate.draft else {
        return;
    };
    if area.is_empty() {
        return;
    }
    let profile = format!(
        "[{} ▾]",
        t!(
            "delegate.profile_label",
            profile = draft.profile_name.as_str()
        )
    );
    let placement = format!("[{} ▾]", placement_label(draft.placement));
    let profile_width =
        (UnicodeWidthStr::width(profile.as_str()) as u16).min(area.width.saturating_mul(3) / 5);
    let profile_area = Rect::new(area.x, area.y, profile_width, 1);
    let placement_area = Rect::new(
        area.x.saturating_add(profile_width).saturating_add(1),
        area.y,
        area.width.saturating_sub(profile_width).saturating_sub(1),
        1,
    );
    let context = if tab.delegate.pending.is_some() {
        t!("agents.loading")
    } else {
        t!("delegate.context_hint")
    };
    frame.render_widget(
        Paragraph::new(profile).style(theme::INPUT_TEXT),
        profile_area,
    );
    frame.render_widget(
        Paragraph::new(placement).style(theme::INPUT_TEXT),
        placement_area,
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(context).style(theme::DIM),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
    app.delegate_control_hits.extend([
        (profile_area, DelegateControl::Profile),
        (placement_area, DelegateControl::Placement),
    ]);
}

fn placement_label(placement: Placement) -> std::borrow::Cow<'static, str> {
    match placement {
        Placement::Tab => t!("recommendations.button_open_in_new_tab"),
        Placement::Pane => t!("recommendations.button_open_in_new_panel"),
    }
}

pub(super) fn render_picker(frame: &mut Frame, app: &mut App, input_area: Rect) -> bool {
    let tab = app.current_tab();
    if !tab.permission.is_empty()
        || !tab.user_input.is_empty()
        || tab.model_picker_open
        || tab.agent_picker_open
        || tab.config_picker.is_open()
        || app.help_overlay_visible
    {
        return false;
    }
    let Some(picker) = &tab.delegate.picker else {
        return false;
    };
    let profiles = app.delegate_profile_candidates();
    let (title, items): (String, Vec<ListItem<'_>>) = match picker.control {
        DelegateControl::Profile => (
            t!("delegate.profile_picker", query = picker.query.as_str()).into_owned(),
            profiles
                .iter()
                .map(|profile| {
                    let duplicate_name = profiles
                        .iter()
                        .filter(|other| other.name == profile.name)
                        .take(2)
                        .count()
                        > 1;
                    let description = if duplicate_name {
                        format!("  {} · {}", profile.agent_id, profile.guid)
                    } else {
                        format!("  {}", profile.agent_id)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!(" {} ", profile.name), theme::INPUT_TEXT),
                        Span::styled(description, theme::DIM),
                    ]))
                })
                .collect(),
        ),
        DelegateControl::Placement => (
            t!("delegate.placement_picker").into_owned(),
            [Placement::Tab, Placement::Pane]
                .into_iter()
                .map(|placement| ListItem::new(placement_label(placement).into_owned()))
                .collect(),
        ),
        DelegateControl::Pick(_) => return false,
    };
    let count = items.len();
    let area = popup::anchored_above(frame, input_area, count.clamp(1, 8) as u16);
    frame.render_widget(Clear, area);
    let selected = picker.selected.min(count.saturating_sub(1));
    let inner = area.inner(Margin::new(1, 1));
    let offset = selected.saturating_sub(inner.height.saturating_sub(1) as usize);
    app.delegate_control_hits.clear();
    if count == 0 {
        let message = if app.delegate_profiles_requested {
            t!("agents.loading")
        } else {
            t!("delegate.no_profiles")
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(theme::DIM)
                .block(popup::block(title)),
            area,
        );
    } else {
        let list = List::new(items)
            .block(popup::block(title))
            .highlight_symbol("> ")
            .highlight_style(if app.pane_focused {
                theme::SELECTED
            } else {
                theme::SELECTED_INACTIVE
            });
        let mut state = ListState::default();
        state.select(Some(selected));
        *state.offset_mut() = offset;
        frame.render_stateful_widget(list, area, &mut state);
        for (row, index) in (state.offset()..count)
            .take(inner.height as usize)
            .enumerate()
        {
            app.delegate_control_hits.push((
                Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                DelegateControl::Pick(index),
            ));
        }
    }
    true
}
