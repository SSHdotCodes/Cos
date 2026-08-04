//! Multiline text editor entity (ComposerTextEditor in Swift).
//! Return submits (in composer mode); Shift-Return inserts a newline.

use gpui::*;
use smallvec::SmallVec;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

actions!(
    cos_editor,
    [
        EditorBackspace,
        EditorDelete,
        EditorLeft,
        EditorRight,
        EditorUp,
        EditorDown,
        EditorSelectLeft,
        EditorSelectRight,
        EditorSelectAll,
        EditorHome,
        EditorEnd,
        EditorPaste,
        EditorCut,
        EditorCopy,
        EditorEnter,
        EditorShiftEnter,
        EditorEscape,
        EditorTab,
        EditorShowCharacterPalette,
    ]
);

pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", EditorBackspace, Some("CosEditor")),
        KeyBinding::new("delete", EditorDelete, Some("CosEditor")),
        KeyBinding::new("left", EditorLeft, Some("CosEditor")),
        KeyBinding::new("right", EditorRight, Some("CosEditor")),
        KeyBinding::new("up", EditorUp, Some("CosEditor")),
        KeyBinding::new("down", EditorDown, Some("CosEditor")),
        KeyBinding::new("shift-left", EditorSelectLeft, Some("CosEditor")),
        KeyBinding::new("shift-right", EditorSelectRight, Some("CosEditor")),
        KeyBinding::new("cmd-a", EditorSelectAll, Some("CosEditor")),
        KeyBinding::new("cmd-v", EditorPaste, Some("CosEditor")),
        KeyBinding::new("cmd-c", EditorCopy, Some("CosEditor")),
        KeyBinding::new("cmd-x", EditorCut, Some("CosEditor")),
        KeyBinding::new("home", EditorHome, Some("CosEditor")),
        KeyBinding::new("end", EditorEnd, Some("CosEditor")),
        KeyBinding::new("enter", EditorEnter, Some("CosEditor")),
        KeyBinding::new("shift-enter", EditorShiftEnter, Some("CosEditor")),
        KeyBinding::new("escape", EditorEscape, Some("CosEditor")),
        KeyBinding::new("tab", EditorTab, Some("CosEditor")),
        KeyBinding::new("ctrl-cmd-space", EditorShowCharacterPalette, Some("CosEditor")),
    ]
}

pub enum EditorEvent {
    /// Enter pressed without shift (composer mode only).
    Submit,
    Changed,
    /// Suggestion list is active: move selection by the offset.
    SuggestionMove(i32),
    SuggestionAccept,
    SuggestionDismiss,
}

pub struct Editor {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<SmallVec<[WrappedLine; 1]>>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    /// When true, bare Enter emits Submit instead of inserting a newline.
    pub enter_submits: bool,
    /// When true, arrows/tab/escape are forwarded as suggestion events.
    pub suggestions_active: bool,
    font_size: Pixels,
    text_color: Hsla,
    placeholder_color: Hsla,
}

impl Editor {
    pub fn new(
        placeholder: impl Into<SharedString>,
        enter_submits: bool,
        font_size: f32,
        text_color: Hsla,
        placeholder_color: Hsla,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            enter_submits,
            suggestions_active: false,
            font_size: px(font_size),
            text_color,
            placeholder_color,
        }
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.content = text.to_string().into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty()
    }

    fn line_height(&self, window: &Window) -> Pixels {
        let _ = window;
        self.font_size * 1.35
    }

    fn left(&mut self, _: &EditorLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &EditorRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn up(&mut self, _: &EditorUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.suggestions_active {
            cx.emit(EditorEvent::SuggestionMove(-1));
            return;
        }
        self.move_vertical(-1.0, window, cx);
    }

    fn down(&mut self, _: &EditorDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.suggestions_active {
            cx.emit(EditorEvent::SuggestionMove(1));
            return;
        }
        self.move_vertical(1.0, window, cx);
    }

    fn move_vertical(&mut self, direction: f32, window: &Window, cx: &mut Context<Self>) {
        let line_height = self.line_height(window);
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let cursor = self.cursor_offset();
        // Walk the shaped lines to find the visual position of the cursor.
        let mut offset = 0usize;
        let mut y = px(0.0);
        for line in layout.iter() {
            let line_end = offset + line.len();
            if cursor <= line_end {
                if let Some(position) = line.position_for_index(cursor - offset, line_height) {
                    let target_y = y + position.y + direction * line_height;
                    if direction < 0.0 && target_y < px(0.0) {
                        self.move_to(0, cx);
                        return;
                    }
                    let total_height = line_height * layout.len() as f32;
                    if direction > 0.0 && target_y >= total_height {
                        self.move_to(self.content.len(), cx);
                        return;
                    }
                    let index = self
                        .index_for_visual_position(point(position.x, target_y), line_height)
                        .unwrap_or(cursor);
                    self.move_to(index, cx);
                    return;
                }
            }
            y += line.size(line_height).height;
            offset = line_end;
        }
    }

    fn index_for_visual_position(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
    ) -> Option<usize> {
        let layout = self.last_layout.as_ref()?;
        let mut offset = 0usize;
        let mut y = px(0.0);
        for line in layout.iter() {
            let height = line.size(line_height).height;
            if position.y >= y && position.y < y + height {
                let local = point(position.x, position.y - y);
                let index = line
                    .closest_index_for_position(local, line_height)
                    .unwrap_or_else(|index| index);
                return Some(offset + index);
            }
            y += height;
            offset += line.len();
        }
        Some(self.content.len())
    }

    fn select_left(&mut self, _: &EditorSelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &EditorSelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &EditorSelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &EditorHome, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &EditorEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &EditorBackspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &EditorDelete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn enter(&mut self, _: &EditorEnter, window: &mut Window, cx: &mut Context<Self>) {
        if self.suggestions_active {
            cx.emit(EditorEvent::SuggestionAccept);
        } else if self.enter_submits {
            cx.emit(EditorEvent::Submit);
        } else {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn shift_enter(&mut self, _: &EditorShiftEnter, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn escape(&mut self, _: &EditorEscape, _window: &mut Window, cx: &mut Context<Self>) {
        if self.suggestions_active {
            cx.emit(EditorEvent::SuggestionDismiss);
        }
    }

    fn tab(&mut self, _: &EditorTab, _window: &mut Window, cx: &mut Context<Self>) {
        if self.suggestions_active {
            cx.emit(EditorEvent::SuggestionAccept);
        }
    }

    fn paste(&mut self, _: &EditorPaste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let cleaned = if self.enter_submits {
                text
            } else {
                text
            };
            self.replace_text_in_range(None, &cleaned, window, cx);
        }
    }

    fn copy(&mut self, _: &EditorCopy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &EditorCut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn show_character_palette(
        &mut self,
        _: &EditorShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        let index = self.index_for_mouse_position(event.position, window);
        if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let index = self.index_for_mouse_position(event.position, window);
            self.select_to(index, cx);
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>, window: &Window) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(_)) = (self.last_bounds.as_ref(), self.last_layout.as_ref()) else {
            return 0;
        };
        let line_height = self.line_height(window);
        let local = point(position.x - bounds.left(), position.y - bounds.top());
        if local.y < px(0.0) {
            return 0;
        }
        self.index_for_visual_position(local, line_height)
            .unwrap_or(self.content.len())
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EventEmitter<EditorEvent> for Editor {}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let line_height = self.line_height(window);
        let range = self.range_from_utf16(&range_utf16);
        let mut offset = 0usize;
        let mut y = px(0.0);
        for line in layout.iter() {
            let line_end = offset + line.len();
            let height = line.size(line_height).height;
            if range.start <= line_end && range.end >= offset {
                let start_pos = line
                    .position_for_index(range.start.saturating_sub(offset), line_height)
                    .unwrap_or(point(px(0.0), px(0.0)));
                return Some(Bounds::new(
                    point(bounds.left() + start_pos.x, bounds.top() + y + start_pos.y),
                    size(px(2.0), line_height),
                ));
            }
            y += height;
            offset = line_end;
        }
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let line_height = self.line_height(window);
        let utf8_index = self.index_for_visual_position(line_point, line_height)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

pub struct EditorElement {
    editor: Entity<Editor>,
}

pub struct PrepaintState {
    lines: Option<SmallVec<[WrappedLine; 1]>>,
    cursor: Option<PaintQuad>,
    selection: Option<Vec<PaintQuad>>,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl EditorElement {
    pub fn new(editor: Entity<Editor>) -> Self {
        Self { editor }
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let editor = self.editor.read(cx);
        let font_size = editor.font_size;
        let line_height = font_size * 1.35;
        let text: SharedString = if editor.content.is_empty() {
            editor.placeholder.clone()
        } else {
            editor.content.clone()
        };
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        // Size to the number of logical lines, capped at 8.
        let line_count = text.lines().count().max(1).min(8);
        style.size.height = (line_height * line_count).into();
        let _ = window;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.editor.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), input.placeholder_color)
        } else {
            (content, input.text_color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = input.font_size;
        let line_height = font_size * 1.35;
        let lines = window
            .text_system()
            .shape_text(display_text, font_size, &runs, Some(bounds.size.width), None)
            .ok();

        // Compute cursor + selection geometry across wrapped lines.
        let mut cursor_quad = None;
        let mut selection_quads: Vec<PaintQuad> = Vec::new();
        if let Some(lines) = lines.as_ref() {
            let mut offset = 0usize;
            let mut y = px(0.0);
            for line in lines.iter() {
                let line_end = offset + line.len();
                let height = line.size(line_height).height;
                if selected_range.is_empty() {
                    if cursor >= offset && cursor <= line_end {
                        if let Some(position) =
                            line.position_for_index(cursor - offset, line_height)
                        {
                            cursor_quad = Some(fill(
                                Bounds::new(
                                    point(bounds.left() + position.x, bounds.top() + y + position.y),
                                    size(px(1.5), line_height),
                                ),
                                input.text_color,
                            ));
                        }
                    }
                } else {
                    let sel_start = selected_range.start;
                    let sel_end = selected_range.end;
                    if sel_end >= offset && sel_start <= line_end {
                        let start_in_line = sel_start.saturating_sub(offset);
                        let end_in_line = sel_end.min(line_end) - offset;
                        if let (Some(start_pos), Some(end_pos)) = (
                            line.position_for_index(start_in_line, line_height),
                            line.position_for_index(end_in_line, line_height),
                        ) {
                            selection_quads.push(fill(
                                Bounds::from_corners(
                                    point(
                                        bounds.left() + start_pos.x,
                                        bounds.top() + y + start_pos.y,
                                    ),
                                    point(bounds.left() + end_pos.x, bounds.top() + y + end_pos.y + line_height),
                                ),
                                hsla(222.0 / 360.0, 0.72, 0.60, 0.35),
                            ));
                        }
                    }
                }
                y += height;
                offset = line_end;
            }
        }
        PrepaintState {
            lines,
            cursor: cursor_quad,
            selection: Some(selection_quads),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        let font_size = self.editor.read(cx).font_size;
        let line_height = font_size * 1.35;
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            for quad in selection {
                window.paint_quad(quad);
            }
        }
        let lines = prepaint.lines.take().unwrap_or_default();
        let mut y = px(0.0);
        for line in &lines {
            line.paint(
                point(bounds.left(), bounds.top() + y),
                line_height,
                TextAlign::Left,
                Some(bounds),
                window,
                cx,
            )
            .ok();
            y += line.size(line_height).height;
        }
        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        self.editor.update(cx, |input, _cx| {
            input.last_layout = Some(lines);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("CosEditor")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::shift_enter))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .child(EditorElement::new(cx.entity()))
    }
}
