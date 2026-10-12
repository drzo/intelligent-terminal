//! Completed-response batch Markdown cache. Active replies never enter this module.
//!
//! Only `poll` installs worker results; `render` borrows the current UI-owned snapshot.
//! Unseen viewport/overscan entries are evicted each frame, so reopening evicted history rebuilds
//! from its original source. There is no separate retained-history or source-size budget here.
//! The registry renderer's heading markers and code fences are deliberately retained.
//!
//! Projection uses Ratatui's wrapping, including its limitations: graphemes wider than the body
//! are omitted, a wide grapheme can occupy the last cell and exceed the nominal row width, and
//! scroll/height coordinates address at most `2 * u16::MAX` rows per logical line. These cases
//! and widths exceeding `u16` are logged, never applied to source, and reconsidered on resize.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{mpsc, Arc};
use std::time::Instant;

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use tui_markdown::{from_str_with_options, AlertKind, Options, StyleSheet};
use unicode_width::UnicodeWidthStr;

use crate::theme;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct MarkdownKey {
    pub turn: usize,
    pub message: usize,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CacheStats {
    pub batch_renders: u64,
    pub processed_source_bytes: u64,
    pub projected_rows: u64,
}

#[derive(Default)]
pub(crate) struct BatchMarkdownCache {
    entries: HashMap<MarkdownKey, Document>,
    worker: Option<Worker>,
    in_flight: Option<(MarkdownKey, u64)>,
    generation: u64,
    worker_failed: bool,
    #[cfg(test)]
    stats: CacheStats,
}

struct Document {
    source: Arc<str>,
    width: usize,
    generation: u64,
    reason: &'static str,
    queued: bool,
    rows: Option<Vec<Line<'static>>>,
    published: bool,
    seen: bool,
}

struct BatchJob {
    key: MarkdownKey,
    source: Arc<str>,
    width: usize,
    generation: u64,
    reason: &'static str,
}

struct BatchResult {
    key: MarkdownKey,
    generation: u64,
    source_bytes: usize,
    rows: Vec<Line<'static>>,
}

struct Worker {
    requests: mpsc::Sender<BatchJob>,
    results: mpsc::Receiver<BatchResult>,
}

impl Worker {
    fn start() -> std::io::Result<Self> {
        let (requests, request_rx) = mpsc::channel::<BatchJob>();
        let (result_tx, results) = mpsc::channel();
        let dispatcher = tracing::dispatcher::get_default(Clone::clone);
        // Dropping the handle detaches, not joins. Closing the cache's channels ends the worker
        // after its current immutable job, without blocking UI teardown or catching parser panics.
        std::thread::Builder::new()
            .name("completed-markdown".into())
            .spawn(move || {
                tracing::dispatcher::with_default(&dispatcher, || {
                    while let Ok(job) = request_rx.recv() {
                        let result = render_batch(job);
                        if result_tx.send(result).is_err() {
                            // The cache was dropped. No later request or result can be published.
                            break;
                        }
                    }
                });
            })?;
        Ok(Self { requests, results })
    }
}

impl BatchMarkdownCache {
    pub(crate) fn snapshot(
        &self,
        key: MarkdownKey,
        source: &str,
        width: usize,
        allow_transition: bool,
    ) -> Option<&[Line<'static>]> {
        self.entries
            .get(&key)
            .filter(|document| {
                document.source.as_ref() == source
                    && document.width == width
                    && (document.published || allow_transition)
            })
            .and_then(|document| document.rows.as_deref())
    }

    pub(crate) fn render(
        &mut self,
        key: MarkdownKey,
        source: &str,
        width: usize,
        allow_transition: bool,
    ) -> Option<&[Line<'static>]> {
        let next_generation = self.generation.wrapping_add(1);
        let changed = match self.entries.entry(key) {
            Entry::Occupied(mut entry) => {
                let document = entry.get_mut();
                document.seen = true;
                let source_changed = document.source.as_ref() != source;
                let width_changed = document.width != width;
                if source_changed || width_changed {
                    if source_changed {
                        document.source = Arc::from(source);
                    }
                    document.width = width;
                    document.generation = next_generation;
                    document.reason = match (source_changed, width_changed) {
                        (true, true) => "source_and_width",
                        (true, false) => "source",
                        _ => "width",
                    };
                    document.queued = true;
                    document.rows = None;
                    document.published = false;
                    true
                } else {
                    false
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(Document {
                    source: Arc::from(source),
                    width,
                    generation: next_generation,
                    reason: "initial",
                    queued: true,
                    rows: None,
                    published: false,
                    seen: true,
                });
                true
            }
        };
        if changed {
            self.generation = next_generation;
        }
        self.dispatch();
        let document = self.entries.get_mut(&key)?;
        if document.rows.is_some() && (document.published || allow_transition) {
            document.published = true;
            document.rows.as_deref()
        } else {
            None
        }
    }

    pub(crate) fn poll(&mut self) -> bool {
        let received = self.worker.as_ref().map(|worker| worker.results.try_recv());
        let ready = match received {
            Some(Ok(result)) => self.accept_result(result),
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.disable_worker("result_disconnected");
                false
            }
            _ => false,
        };
        self.dispatch();
        ready
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.worker_failed
            && (self.in_flight.is_some() || self.entries.values().any(|entry| entry.queued))
    }

    pub(crate) fn begin_frame(&mut self) {
        for document in self.entries.values_mut() {
            document.seen = false;
        }
    }

    /// Retain cached segments of a visible collapsed turn without scheduling work.
    pub(crate) fn touch_turn(&mut self, turn: usize) {
        for (key, document) in &mut self.entries {
            if key.turn == turn {
                document.seen = true;
            }
        }
    }

    pub(crate) fn end_frame(&mut self) -> Vec<usize> {
        let mut evicted_turns = Vec::new();
        self.entries.retain(|key, document| {
            if !document.seen {
                evicted_turns.push(key.turn);
            }
            document.seen
        });
        evicted_turns.sort_unstable();
        evicted_turns.dedup();
        evicted_turns
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        // Keep the generation and in-flight slot: the old worker may still return a result.
        // Reusing the same turn/message indices must neither adopt it nor start a second job.
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> CacheStats {
        self.stats
    }

    #[cfg(test)]
    pub(crate) fn source(&self, key: MarkdownKey) -> Option<&str> {
        self.entries
            .get(&key)
            .map(|document| document.source.as_ref())
    }

    fn dispatch(&mut self) {
        if self.worker_failed
            || self.in_flight.is_some()
            || !self.entries.values().any(|entry| entry.queued)
        {
            return;
        }
        if self.worker.is_none() {
            match Worker::start() {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    tracing::error!(
                        target: "completed_markdown",
                        %error,
                        reason = "worker_start",
                        "Cannot start the Markdown worker; retaining raw display"
                    );
                    self.worker_failed = true;
                    return;
                }
            }
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        let Some((&key, document)) = self
            .entries
            .iter_mut()
            .filter(|(_, document)| document.queued)
            .min_by_key(|(_, document)| document.generation)
        else {
            return;
        };
        let generation = document.generation;
        let job = BatchJob {
            key,
            source: Arc::clone(&document.source),
            width: document.width,
            generation,
            reason: document.reason,
        };
        if worker.requests.send(job).is_err() {
            self.disable_worker("request_disconnected");
        } else {
            document.queued = false;
            self.in_flight = Some((key, generation));
        }
    }

    fn accept_result(&mut self, result: BatchResult) -> bool {
        #[cfg(test)]
        {
            // Count actual completed jobs, including results invalidated while they were running.
            self.stats.batch_renders += 1;
            self.stats.processed_source_bytes += result.source_bytes as u64;
            self.stats.projected_rows += result.rows.len() as u64;
        }
        if self.in_flight.take() != Some((result.key, result.generation)) {
            self.disable_worker("unexpected_result");
            return false;
        }
        if let Some(document) = self
            .entries
            .get_mut(&result.key)
            .filter(|document| document.generation == result.generation)
        {
            document.rows = Some(result.rows);
            true
        } else {
            tracing::debug!(
                target: "completed_markdown",
                key = ?result.key,
                source_bytes = result.source_bytes,
                reason = "superseded",
                "Discarded stale Markdown result"
            );
            false
        }
    }

    fn disable_worker(&mut self, reason: &'static str) {
        tracing::error!(
            target: "completed_markdown",
            reason,
            "Markdown worker disconnected; retaining raw display without retrying"
        );
        self.worker = None;
        self.in_flight = None;
        self.worker_failed = true;
    }

    #[cfg(test)]
    fn wait_until_idle(&mut self) {
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while self.has_pending() {
            self.dispatch();
            let result = self
                .worker
                .as_ref()
                .expect("pending work must have a worker")
                .results
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("batch worker must finish within the test deadline");
            self.accept_result(result);
            self.dispatch();
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PaneStyles;

impl StyleSheet for PaneStyles {
    fn heading(&self, level: u8) -> Style {
        Style::new().add_modifier(match level {
            1 => Modifier::BOLD | Modifier::UNDERLINED,
            2 => Modifier::BOLD,
            3 => Modifier::BOLD | Modifier::ITALIC,
            _ => Modifier::ITALIC,
        })
    }

    fn code(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    fn link(&self) -> Style {
        Style::new().add_modifier(Modifier::UNDERLINED)
    }

    fn blockquote(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM | Modifier::ITALIC)
    }

    fn metadata_block(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    fn math_inline(&self) -> Style {
        Style::new().add_modifier(Modifier::ITALIC)
    }

    fn math_display(&self) -> Style {
        Style::new().add_modifier(Modifier::ITALIC)
    }

    fn alert(&self, _kind: AlertKind) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    fn table_header(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    fn table_border(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    fn list_marker(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }
}

fn body_width(width: usize) -> u16 {
    let width = width.min(usize::from(u16::MAX)) as u16;
    if width > 2 {
        width - 2
    } else {
        width
    }
}

fn options(width: usize) -> Options<PaneStyles> {
    // Keep the registry renderer's heading markers, including headings before task lists.
    Options::new(PaneStyles).table_width(body_width(width))
}

fn render_batch(job: BatchJob) -> BatchResult {
    let start = Instant::now();
    if job.width > usize::from(u16::MAX) {
        tracing::warn!(
            target: "completed_markdown",
            key = ?job.key,
            width = job.width,
            supported_width = u16::MAX,
            "Chat width exceeds Ratatui coordinates; source is retained"
        );
    }
    let text = from_str_with_options(&job.source, &options(job.width));
    let rows = project_text(job.key, &text, job.width);
    tracing::debug!(
        target: "completed_markdown",
        key = ?job.key,
        source_bytes = job.source.len(),
        width = job.width,
        elapsed_us = start.elapsed().as_micros(),
        projected_rows = rows.len(),
        reason = job.reason,
        "Completed Markdown batch render"
    );
    BatchResult {
        key: job.key,
        generation: job.generation,
        source_bytes: job.source.len(),
        rows,
    }
}

fn project_text(key: MarkdownKey, text: &Text<'_>, width: usize) -> Vec<Line<'static>> {
    let body = body_width(width);
    let mut rows = Vec::new();
    if body == 0 {
        return rows;
    }
    let mut marked = false;
    for (logical_row, line) in text.lines.iter().enumerate() {
        let line = prepare_line(line, text.style, text.alignment);
        for mut line in project_line(key, logical_row, line, body) {
            if line
                .spans
                .iter()
                .any(|span| !span.content.trim().is_empty())
            {
                if !marked {
                    marked = true;
                    if width <= 2 {
                        rows.push(Line::from(Span::styled("●", theme::DOT_AGENT)));
                    } else {
                        line.spans.insert(0, Span::styled("● ", theme::DOT_AGENT));
                    }
                } else if width > 2 {
                    line.spans.insert(0, Span::raw("  "));
                }
            }
            rows.push(line);
        }
    }
    rows
}

fn push_symbol(spans: &mut Vec<Span<'static>>, symbol: &str, style: Style) {
    if let Some(last) = spans.last_mut().filter(|span| span.style == style) {
        last.content.to_mut().push_str(symbol);
    } else {
        spans.push(Span::styled(symbol.to_owned(), style));
    }
}

fn prepare_line<'a>(
    line: &'a Line<'_>,
    text_style: Style,
    alignment: Option<Alignment>,
) -> Line<'a> {
    let base = text_style.patch(line.style);
    if line.spans.len() <= 1 {
        return Line {
            style: base,
            alignment: line.alignment.or(alignment),
            spans: line
                .spans
                .iter()
                .map(|span| Span::styled(span.content.as_ref(), span.style))
                .collect(),
        };
    }

    // Ratatui segments spans separately. Segment the joined line through Ratatui so an inline
    // style boundary cannot split a grapheme; its leading byte determines its cell's style.
    let joined = Span::raw(
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>(),
    );
    let mut spans = Vec::new();
    let mut index = 0;
    let mut end = line.spans[0].content.len();
    for grapheme in joined.styled_graphemes(base) {
        let byte = grapheme.symbol.as_ptr().addr() - joined.content.as_ptr().addr();
        while byte >= end && index + 1 < line.spans.len() {
            index += 1;
            end += line.spans[index].content.len();
        }
        push_symbol(
            &mut spans,
            grapheme.symbol,
            base.patch(line.spans[index].style),
        );
    }
    Line {
        spans,
        style: Style::default(),
        alignment: line.alignment.or(alignment),
    }
}

fn project_line(
    key: MarkdownKey,
    logical_row: usize,
    line: Line<'_>,
    width: u16,
) -> Vec<Line<'static>> {
    let oversized = line
        .styled_graphemes(Style::default())
        .filter(|grapheme| grapheme.symbol.width() > usize::from(width))
        .count();
    if oversized != 0 {
        tracing::warn!(
            target: "completed_markdown",
            ?key,
            logical_row,
            body_width = width,
            omitted_graphemes = oversized,
            "Ratatui cannot fit these graphemes at this width; source is retained"
        );
    }
    let mut paragraph = Paragraph::new(line)
        .style(theme::AGENT_TEXT)
        .wrap(Wrap { trim: false });
    let height = paragraph.line_count(width);
    let addressable = usize::from(u16::MAX) * 2;
    if height > addressable {
        tracing::warn!(
            target: "completed_markdown",
            ?key,
            logical_row,
            required_rows = height,
            addressable_rows = addressable,
            "Logical row exceeds Ratatui scroll/height coordinates; source is retained"
        );
    }
    let height = height.min(addressable);
    let mut rows = Vec::with_capacity(height);
    while rows.len() < height {
        let page_height = (height - rows.len()).min(usize::from(u16::MAX)) as u16;
        let area = Rect::new(0, 0, width, page_height);
        // Empty symbols distinguish unpainted padding from meaningful trailing spaces.
        let mut buffer = Buffer::filled(area, Cell::new(""));
        paragraph = paragraph.scroll((rows.len() as u16, 0));
        (&paragraph).render(area, &mut buffer);
        for y in 0..page_height {
            let mut spans = Vec::new();
            let start = usize::from(y) * usize::from(width);
            let cells = &buffer.content[start..start + usize::from(width)];
            if let Some(last) = cells.iter().rposition(|cell| !cell.symbol().is_empty()) {
                let mut x = 0;
                while x <= last {
                    let cell = &cells[x];
                    let symbol = if cell.symbol().is_empty() {
                        " "
                    } else {
                        cell.symbol()
                    };
                    push_symbol(
                        &mut spans,
                        symbol,
                        Style::new().fg(cell.fg).add_modifier(cell.modifier),
                    );
                    x += symbol.width().max(1);
                }
            }
            rows.push(Line::from(spans));
        }
    }
    let overwide = rows
        .iter()
        .filter(|row| row.width() > usize::from(width))
        .count();
    if overwide != 0 {
        tracing::warn!(
            target: "completed_markdown",
            ?key,
            logical_row,
            body_width = width,
            overwide_rows = overwide,
            "Ratatui word wrapping exceeded the body width; source is retained"
        );
    }
    rows
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::sync::Mutex;
    use std::time::Duration;
    use std::time::Instant;

    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::widgets::{Paragraph, Widget, Wrap};
    use tui_markdown::from_str_with_options;

    use super::*;
    use crate::theme;

    const KEY: MarkdownKey = MarkdownKey {
        turn: 7,
        message: 0,
    };
    const OTHER: MarkdownKey = MarkdownKey {
        turn: 8,
        message: 0,
    };

    fn completed(
        cache: &mut BatchMarkdownCache,
        key: MarkdownKey,
        source: &str,
        width: usize,
    ) -> Vec<Line<'static>> {
        cache.render(key, source, width, true);
        cache.wait_until_idle();
        cache
            .render(key, source, width, true)
            .expect("a completed batch must publish a Markdown snapshot")
            .to_vec()
    }

    fn visible(rows: &[Line<'_>]) -> String {
        rows.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assert_batch_parity(rows: &[Line<'static>], source: &str, width: usize) {
        let paragraph = Paragraph::new(from_str_with_options(source, &options(width)))
            .style(theme::AGENT_TEXT)
            .wrap(Wrap { trim: false });
        let height = paragraph.line_count(body_width(width));
        assert_eq!(rows.len(), height);
        let mut expected = Buffer::empty(Rect::new(0, 0, body_width(width), height as u16));
        paragraph.render(expected.area, &mut expected);
        let mut actual = Buffer::empty(Rect::new(0, 0, width as u16, height as u16));
        Paragraph::new(rows.to_vec())
            .style(theme::AGENT_TEXT)
            .render(actual.area, &mut actual);
        for y in 0..height as u16 {
            for x in 0..body_width(width) {
                assert_eq!(
                    actual[(x + 2, y)],
                    expected[(x, y)],
                    "batch cells and styles at width={width}, x={x}, y={y}"
                );
            }
        }
    }

    #[test]
    fn completed_batch_matches_registry_paragraph_cells_and_styles() {
        let mut cache = BatchMarkdownCache::default();
        for source in [
            "# Heading\n\n- [x] Task\n- [ ] Another\n\n**bold** and *italic*",
            "> quoted `code` and [link](https://example.invalid)\n\n~~gone~~",
            "```rust\nlet text = \"**literal**\";\n```\n\nA soft\nbreak",
            "| Header | Value |\n| --- | --- |\n| **strong** | CJK 中 |\n| last | cell |",
        ] {
            let rows = completed(&mut cache, KEY, source, 56);
            assert_batch_parity(&rows, source, 56);
            assert_eq!(visible(&rows).matches('●').count(), 1);
            assert_eq!(cache.source(KEY), Some(source));
        }
        assert_eq!(cache.stats().batch_renders, 4);
    }

    #[test]
    fn empty_source_finishes_once_and_stays_cached() {
        let mut cache = BatchMarkdownCache::default();
        let rows = completed(&mut cache, KEY, "", 40);
        assert!(visible(&rows).is_empty());
        assert_eq!(cache.source(KEY), Some(""));
        assert_eq!(cache.stats().batch_renders, 1);
        assert_eq!(cache.stats().processed_source_bytes, 0);
        assert_eq!(completed(&mut cache, KEY, "", 40), rows);
        assert_eq!(cache.stats().batch_renders, 1);
    }

    #[test]
    fn height_snapshots_do_not_schedule_publish_or_retain_offscreen_documents() {
        let mut cache = BatchMarkdownCache::default();
        let source = "**snapshot**";
        assert!(cache.snapshot(KEY, source, 40, true).is_none());
        assert!(cache.entries.is_empty());
        assert!(cache.worker.is_none());
        assert!(cache.render(KEY, source, 40, true).is_none());
        cache.wait_until_idle();
        let stats = cache.stats();
        let pointer = cache.snapshot(KEY, source, 40, true).unwrap().as_ptr();
        assert!(cache.snapshot(KEY, source, 40, false).is_none());
        assert_eq!(
            cache.render(KEY, source, 40, true).unwrap().as_ptr(),
            pointer
        );
        assert_eq!(
            cache.snapshot(KEY, source, 40, false).unwrap().as_ptr(),
            pointer
        );
        assert!(cache.snapshot(KEY, "**replaced**", 40, true).is_none());
        assert!(cache.snapshot(KEY, source, 41, true).is_none());
        assert!(!cache.has_pending());
        cache.begin_frame();
        assert!(cache.snapshot(KEY, source, 40, false).is_some());
        assert_eq!(cache.end_frame(), vec![KEY.turn]);
        assert!(cache.snapshot(KEY, source, 40, true).is_none());
        assert!(cache.entries.is_empty());
        assert!(!cache.has_pending());
        assert_eq!(cache.stats(), stats);
    }

    #[test]
    fn unchanged_frames_borrow_the_same_snapshot_without_more_work() {
        let mut cache = BatchMarkdownCache::default();
        let source = "**cached**";
        let rows = completed(&mut cache, KEY, source, 30);
        assert!(visible(&rows).contains("cached"));
        assert!(!visible(&rows).contains("**"));
        let pointer = cache.render(KEY, source, 30, true).unwrap().as_ptr();
        let raw_pointer = cache.source(KEY).unwrap().as_ptr();
        let stats = cache.stats();
        for _ in 0..5 {
            cache.begin_frame();
            assert!(!cache.poll());
            assert_eq!(
                cache.render(KEY, source, 30, true).unwrap().as_ptr(),
                pointer
            );
            assert!(cache.end_frame().is_empty());
            assert_eq!(cache.source(KEY).unwrap().as_ptr(), raw_pointer);
            assert_eq!(cache.stats(), stats);
            assert!(!cache.has_pending());
        }
    }

    #[test]
    fn equal_prefix_replacements_and_other_turns_are_isolated() {
        let mut cache = BatchMarkdownCache::default();
        let initial = "**prefix** first";
        let old = completed(&mut cache, KEY, initial, 40);
        assert_eq!(completed(&mut cache, OTHER, initial, 40), old);
        for replacement in [
            "**prefix** other",
            "**prefix** other extended",
            "**prefix**",
        ] {
            let rows = completed(&mut cache, KEY, replacement, 40);
            assert_batch_parity(&rows, replacement, 40);
            assert_eq!(cache.source(KEY), Some(replacement));
            assert_eq!(cache.source(OTHER), Some(initial));
            assert_eq!(cache.render(OTHER, initial, 40, true).unwrap(), old);
        }
        assert_eq!(cache.stats().batch_renders, 5);
    }

    #[test]
    fn source_and_line_endings_are_preserved_byte_for_byte() {
        let mut cache = BatchMarkdownCache::default();
        let source = "# Original\r\n\r\n**bold**\r\nline\rfinal  \r\n";
        let rows = completed(&mut cache, KEY, source, 48);
        assert_batch_parity(&rows, source, 48);
        assert_eq!(cache.source(KEY).unwrap().as_bytes(), source.as_bytes());
        assert_eq!(cache.stats().processed_source_bytes, source.len() as u64);
    }

    #[test]
    fn selection_defers_new_markdown_without_toggling_formatted_history() {
        let mut cache = BatchMarkdownCache::default();
        let old = completed(&mut cache, KEY, "**history**", 30);
        assert!(cache.render(OTHER, "**selected raw**", 30, false).is_none());
        cache.wait_until_idle();
        assert!(!cache.has_pending());
        assert!(cache.render(OTHER, "**selected raw**", 30, false).is_none());
        assert_eq!(cache.render(KEY, "**history**", 30, false).unwrap(), old);
        assert_eq!(cache.stats().batch_renders, 2);
        let rows = cache.render(OTHER, "**selected raw**", 30, true).unwrap();
        assert!(visible(rows).contains("selected raw"));
        assert!(!visible(rows).contains("**"));
        assert_eq!(cache.stats().batch_renders, 2);
    }

    #[test]
    fn width_and_source_updates_coalesce_behind_one_inflight_job() {
        let mut cache = BatchMarkdownCache::default();
        assert!(cache.render(KEY, "**first**", 24, true).is_none());
        assert!(cache.has_pending());
        for width in [12, 20, 32, 48] {
            assert!(cache.render(KEY, "**latest**", width, true).is_none());
        }
        assert_eq!(cache.stats().batch_renders, 0);
        let stale = cache
            .worker
            .as_ref()
            .unwrap()
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(!cache.accept_result(stale));
        assert!(cache.render(KEY, "**latest**", 48, true).is_none());
        cache.wait_until_idle();
        let rows = cache.render(KEY, "**latest**", 48, true).unwrap();
        assert_batch_parity(rows, "**latest**", 48);
        assert!(!visible(rows).contains("first"));
        assert_eq!(cache.stats().batch_renders, 2);
        assert_eq!(
            cache.stats().processed_source_bytes,
            ("**first**".len() + "**latest**".len()) as u64
        );
    }

    #[test]
    fn clear_rejects_inflight_results_for_reused_keys() {
        let mut cache = BatchMarkdownCache::default();
        cache.render(KEY, "**old session**", 40, true);
        cache.clear();
        assert_eq!(cache.source(KEY), None);
        assert!(cache.render(KEY, "**new session**", 40, true).is_none());
        let stale = cache
            .worker
            .as_ref()
            .unwrap()
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(!cache.accept_result(stale));
        assert!(cache.render(KEY, "**new session**", 40, true).is_none());
        cache.wait_until_idle();
        let rows = cache.render(KEY, "**new session**", 40, true).unwrap();
        assert!(visible(rows).contains("new session"));
        assert!(!visible(rows).contains("old session"));
        assert_eq!(cache.source(KEY), Some("**new session**"));
        assert_eq!(cache.stats().batch_renders, 2);
    }

    #[test]
    fn frame_pruning_discards_unseen_queued_entries_and_rebuilds_evicted_history() {
        let mut cache = BatchMarkdownCache::default();
        cache.render(KEY, "**kept**", 40, true);
        cache.render(OTHER, "**never dispatched**", 40, true);
        cache.begin_frame();
        cache.render(KEY, "**kept**", 40, true);
        assert_eq!(cache.end_frame(), vec![OTHER.turn]);
        assert_eq!(cache.source(OTHER), None);
        let old = completed(&mut cache, KEY, "**kept**", 40);
        assert_eq!(cache.stats().batch_renders, 1);
        cache.begin_frame();
        assert_eq!(cache.end_frame(), vec![KEY.turn]);
        assert_eq!(cache.source(KEY), None);
        assert_eq!(completed(&mut cache, KEY, "**kept**", 40), old);
        assert_eq!(cache.stats().batch_renders, 2);
    }

    #[test]
    fn touching_a_collapsed_turn_keeps_existing_segments_without_scheduling_work() {
        let mut cache = BatchMarkdownCache::default();
        cache.begin_frame();
        cache.touch_turn(KEY.turn);
        assert!(cache.end_frame().is_empty());
        assert!(cache.entries.is_empty());
        assert!(cache.worker.is_none());
        assert!(!cache.has_pending());
        assert_eq!(cache.stats(), CacheStats::default());

        let sibling = MarkdownKey { message: 2, ..KEY };
        let first = completed(&mut cache, KEY, "**first segment**", 40);
        let second = completed(&mut cache, sibling, "**second segment**", 40);
        completed(&mut cache, OTHER, "**offscreen turn**", 40);
        let stats = cache.stats();
        for frame in 0..2 {
            cache.begin_frame();
            cache.touch_turn(KEY.turn);
            assert_eq!(
                cache.end_frame(),
                if frame == 0 { vec![OTHER.turn] } else { vec![] }
            );
            assert_eq!(cache.source(KEY), Some("**first segment**"));
            assert_eq!(cache.source(sibling), Some("**second segment**"));
            assert_eq!(cache.source(OTHER), None);
            assert!(!cache.has_pending());
            assert_eq!(cache.stats(), stats);
        }
        assert_eq!(
            cache.render(KEY, "**first segment**", 40, true).unwrap(),
            first
        );
        assert_eq!(
            cache
                .render(sibling, "**second segment**", 40, true)
                .unwrap(),
            second
        );
        assert!(!cache.has_pending());
        assert_eq!(cache.stats(), stats);
    }

    #[test]
    fn unicode_graphemes_across_styles_use_the_leading_cell_style() {
        let mut cache = BatchMarkdownCache::default();
        let source = "**e**\u{301} **👩\u{200d}💻** 中";
        let rows = completed(&mut cache, KEY, source, 40);
        let text = visible(&rows);
        assert!(text.contains("e\u{301}"));
        assert!(text.contains("👩\u{200d}💻"));
        assert!(text.contains('中'));
        for symbol in ["e\u{301}", "👩\u{200d}💻"] {
            assert!(rows.iter().flat_map(|line| &line.spans).any(|span| {
                span.content.contains(symbol) && span.style.add_modifier.contains(Modifier::BOLD)
            }));
        }
        assert_eq!(cache.source(KEY), Some(source));
        let split = Text::from(Line::from(vec![
            Span::styled("👩", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw("\u{200d}💻"),
        ]));
        let projected = project_text(KEY, &split, 40);
        assert!(projected.iter().flat_map(|line| &line.spans).any(|span| {
            span.content == "👩\u{200d}💻" && span.style.add_modifier.contains(Modifier::BOLD)
        }));
    }

    #[test]
    fn narrow_tables_and_hanging_prefix_follow_paragraph_wrapping() {
        let mut cache = BatchMarkdownCache::default();
        let source = "| Heading | Other |\n| --- | --- |\n| long cell text | value |";
        for width in [10, 18, 40] {
            let rows = completed(&mut cache, KEY, source, width);
            assert_batch_parity(&rows, source, width);
            assert!(rows.iter().all(|line| line.width() <= width));
            assert_eq!(visible(&rows).matches('●').count(), 1);
            for row in rows.iter().skip(1) {
                let text = row.to_string();
                assert!(text.trim().is_empty() || text.starts_with("  "));
            }
        }
    }

    #[test]
    fn zero_tiny_and_coordinate_widths_preserve_raw_source() {
        let mut cache = BatchMarkdownCache::default();
        let source = "**abc** 中 e\u{301}";
        assert!(completed(&mut cache, KEY, source, 0).is_empty());
        for width in [1, 2, 3, 6, 40, usize::MAX] {
            let rows = completed(&mut cache, KEY, source, width);
            assert!(rows
                .iter()
                .all(|line| line.width() <= width.min(u16::MAX as usize)));
            assert_eq!(visible(&rows).matches('●').count(), 1);
            assert_eq!(cache.source(KEY), Some(source));
        }
        assert!(visible(cache.render(KEY, source, usize::MAX, true).unwrap()).contains('中'));
    }

    #[test]
    fn pane_styles_inherit_foreground_and_background_and_keep_upstream_markers() {
        let styles = PaneStyles;
        for style in [
            styles.heading(1),
            styles.heading(2),
            styles.code(),
            styles.link(),
            styles.blockquote(),
            styles.metadata_block(),
            styles.math_inline(),
            styles.math_display(),
            styles.alert(AlertKind::Warning),
            styles.table_header(),
            styles.table_cell(),
            styles.table_border(),
            styles.list_marker(),
        ] {
            assert_eq!(style.fg, None);
            assert_eq!(style.bg, None);
            let inherited = Style::new().fg(Color::Yellow).bg(Color::Blue);
            assert_eq!(inherited.patch(style).fg, Some(Color::Yellow));
            assert_eq!(inherited.patch(style).bg, Some(Color::Blue));
        }
        assert_eq!(styles.heading_marker(1), "#");
        assert_eq!(styles.code_block_fence(), "```");
        let mut cache = BatchMarkdownCache::default();
        let rows = completed(
            &mut cache,
            KEY,
            "# Head\n\n```\ncode\n```\n\n- [x] task",
            40,
        );
        assert!(visible(&rows).contains("# Head"));
        assert!(visible(&rows).contains("```"));
        for line in &rows {
            assert_eq!(line.style.bg, None);
            assert!(line.spans.iter().all(|span| span.style.bg.is_none()));
        }
    }

    #[test]
    #[ignore = "Run timing measurements in isolation with --ignored --test-threads=1"]
    fn medium_and_large_completed_jobs_report_work_without_a_latency_budget() {
        // The receive deadline still fails a stalled measurement. Parallel-suite CPU contention
        // is not an approved response-latency budget, so this measurement is run explicitly.
        let block = "## Completed response\n\nA paragraph with **strong**, *emphasis*, `code`, and Unicode 中 e\u{301} 👩\u{200d}💻.\n\n| Key | Value |\n| --- | --- |\n| item | complete |\n\n";
        for blocks in [256, 4096] {
            let source = block.repeat(blocks);
            let mut cache = BatchMarkdownCache::default();
            let start = Instant::now();
            let rows = completed(&mut cache, KEY, &source, 80);
            let elapsed = start.elapsed();
            assert_eq!(cache.stats().batch_renders, 1);
            assert_eq!(cache.stats().processed_source_bytes, source.len() as u64);
            assert_eq!(cache.stats().projected_rows, rows.len() as u64);
            let stats = cache.stats();
            cache.render(KEY, &source, 80, true);
            assert_eq!(cache.stats(), stats);
            eprintln!(
                "completed_markdown measurement: source_bytes={} width=80 projected_rows={} elapsed_us={} batch_renders=1",
                source.len(),
                rows.len(),
                elapsed.as_micros()
            );
        }
    }

    #[test]
    fn ready_results_stay_private_until_the_ui_accepts_them() {
        let mut cache = BatchMarkdownCache::default();
        let source = "**owned snapshot**";
        assert!(cache.worker.is_none());
        assert!(cache.render(KEY, source, 40, true).is_none());
        let result = cache
            .worker
            .as_ref()
            .unwrap()
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(cache.render(KEY, source, 40, true).is_none());
        assert_eq!(cache.stats(), CacheStats::default());
        assert!(cache.accept_result(result));
        let snapshot = cache.render(KEY, source, 40, true).unwrap().to_vec();
        let text = from_str_with_options(source, &options(40));
        let original = text.clone();
        assert_eq!(project_text(KEY, &text, 40), snapshot);
        assert_eq!(text, original);
        assert_eq!(cache.source(KEY), Some(source));
        assert!(!cache.poll());
    }

    #[test]
    fn ratatui_wide_grapheme_overflow_is_characterized_without_changing_source() {
        let source = "**A中e\u{301}👩\u{200d}💻B 文 end**";
        let mut cache = BatchMarkdownCache::default();
        let mut overwide = Vec::new();
        for width in [5, 7, 11, 20] {
            let rows = completed(&mut cache, KEY, source, width);
            overwide.extend(
                rows.iter()
                    .enumerate()
                    .filter(|(_, line)| line.width() > width)
                    .map(|(row, line)| (width, row, line.width())),
            );
            let content = visible(&rows);
            for grapheme in ["中", "e\u{301}", "👩\u{200d}💻", "文"] {
                assert_eq!(content.matches(grapheme).count(), 1);
            }
            assert_eq!(cache.source(KEY), Some(source));
        }
        assert_eq!(overwide, [(5, 2, 6), (7, 1, 8), (11, 0, 12)]);
    }

    #[test]
    fn one_logical_line_uses_both_addressable_ratatui_pages() {
        let mut cache = BatchMarkdownCache::default();
        let source = "x".repeat(usize::from(u16::MAX) + 1);
        let rows = completed(&mut cache, KEY, &source, 3);
        assert_eq!(rows.len(), source.len());
        assert_eq!(visible(&rows).matches('x').count(), source.len());
        assert_eq!(cache.source(KEY), Some(source.as_str()));
    }

    #[derive(Clone)]
    struct LogWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for LogWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn diagnostics_identify_worker_work_and_limits_without_source_or_retries() {
        let logs = Arc::new(Mutex::new(Vec::new()));
        let writer = LogWriter(Arc::clone(&logs));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .with_target(true)
            .with_thread_names(true)
            .with_max_level(tracing::Level::DEBUG)
            .without_time()
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            // Parallel tests can register these callsites without this scoped subscriber.
            let mut warmup = BatchMarkdownCache::default();
            completed(&mut warmup, KEY, "", 0);
            completed(&mut warmup, KEY, "中", 3);
            completed(&mut warmup, KEY, "**A中e\u{301}👩\u{200d}💻B 文 end**", 5);
            tracing::callsite::rebuild_interest_cache();
            logs.lock().unwrap().clear();
            let mut cache = BatchMarkdownCache::default();
            let source = "**DO_NOT_LOG_SOURCE** 中";
            completed(&mut cache, KEY, source, 3);
            let size = logs.lock().unwrap().len();
            cache.render(KEY, source, 3, true);
            cache.poll();
            assert_eq!(logs.lock().unwrap().len(), size);
            completed(&mut cache, KEY, source, usize::MAX);
            completed(&mut cache, KEY, "**A中e\u{301}👩\u{200d}💻B 文 end**", 5);
            let original = "x".repeat(usize::from(u16::MAX) * 2 + 1);
            let rows = completed(&mut cache, OTHER, &original, 3);
            assert_eq!(rows.len(), usize::from(u16::MAX) * 2);
            assert_eq!(cache.source(OTHER), Some(original.as_str()));

            let (requests, request_rx) = mpsc::channel();
            let (result_tx, results) = mpsc::channel();
            let mut failed = BatchMarkdownCache {
                worker: Some(Worker { requests, results }),
                ..BatchMarkdownCache::default()
            };
            failed.render(KEY, source, 20, true);
            request_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(result_tx);
            assert!(!failed.poll());
            assert!(!failed.has_pending());
            for width in [30, 40, 50] {
                failed.clear();
                assert!(failed.render(KEY, source, width, true).is_none());
                assert!(!failed.poll());
                assert!(!failed.has_pending());
            }
            assert_eq!(failed.stats(), CacheStats::default());
        });
        let logs = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("completed-markdown"));
        assert!(logs.contains("completed_markdown"));
        for metadata in [
            "source_bytes=",
            "width=",
            "elapsed_us=",
            "projected_rows=",
            "reason=\"initial\"",
            "reason=\"width\"",
            "omitted_graphemes=",
            "supported_width=",
            "addressable_rows=",
            "overwide_rows=",
        ] {
            assert!(
                logs.contains(metadata),
                "missing diagnostic {metadata}: {logs}"
            );
        }
        assert_eq!(logs.matches("result_disconnected").count(), 1);
        assert!(!logs.contains("DO_NOT_LOG_SOURCE"));
    }
}
