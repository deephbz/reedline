use super::{edit_stack::EditStack, Clipboard, ClipboardMode, LineBuffer};
#[cfg(feature = "system_clipboard")]
use crate::core_editor::get_system_clipboard;
use crate::enums::{EditType, UndoBehavior, MotionKind, MotionAction, PairAction};
use crate::{core_editor::get_local_clipboard, EditCommand};
use std::ops::DerefMut;

/// Stateful editor executing changes to the underlying [`LineBuffer`]
///
/// In comparison to the state-less [`LineBuffer`] the [`Editor`] keeps track of
/// the undo/redo history and has facilities for cut/copy/yank/paste
pub struct Editor {
    line_buffer: LineBuffer,
    cut_buffer: Box<dyn Clipboard>,
    #[cfg(feature = "system_clipboard")]
    system_clipboard: Box<dyn Clipboard>,
    edit_stack: EditStack<LineBuffer>,
    last_undo_behavior: UndoBehavior,
    selection_anchor: Option<usize>,
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            line_buffer: LineBuffer::new(),
            cut_buffer: get_local_clipboard(),

            #[cfg(feature = "system_clipboard")]
            system_clipboard: get_system_clipboard(),
            edit_stack: EditStack::new(),
            last_undo_behavior: UndoBehavior::CreateUndoPoint,
            selection_anchor: None,
        }
    }
}

impl Editor {
    /// Get the current [`LineBuffer`]
    pub const fn line_buffer(&self) -> &LineBuffer {
        &self.line_buffer
    }

    /// Set the current [`LineBuffer`].
    /// [`UndoBehavior`] specifies how this change should be reflected on the undo stack.
    pub(crate) fn set_line_buffer(&mut self, line_buffer: LineBuffer, undo_behavior: UndoBehavior) {
        self.line_buffer = line_buffer;
        self.update_undo_state(undo_behavior);
    }

    pub(crate) fn run_edit_command(&mut self, command: &EditCommand) {
        match command {
            // Insertion
            EditCommand::InsertChar(c) => self.insert_char(*c),
            EditCommand::InsertString(s) => self.insert_str(s),
            EditCommand::InsertNewline => self.insert_newline(),

            // Replacements
            EditCommand::ReplaceChar(chr) => self.replace_char(*chr),
            EditCommand::ReplaceChars(n, s) => self.replace_chars(*n, s),

            // Motion-based commands
            EditCommand::Motion { motion, action } => {
                self.handle_motion(*motion, *action);
            }

            // Single grapheme removal
            EditCommand::Backspace => self.backspace(),
            EditCommand::Delete => self.delete(),

            // Inside-pair commands
            EditCommand::OperateInsidePair { left_char, right_char, action } => {
                match action {
                    PairAction::CutInside => self.cut_inside(*left_char, *right_char),
                    PairAction::YankInside => self.yank_inside(*left_char, *right_char),
                }
            }

            // Undo / redo
            EditCommand::Undo => self.undo(),
            EditCommand::Redo => self.redo(),

            // Word transformations
            EditCommand::SwapGraphemes => self.line_buffer.swap_graphemes(),
            EditCommand::SwapWords => self.line_buffer.swap_words(),
            EditCommand::UppercaseWord => self.line_buffer.uppercase_word(),
            EditCommand::LowercaseWord => self.line_buffer.lowercase_word(),
            EditCommand::SwitchcaseChar => self.line_buffer.switchcase_char(),
            EditCommand::CapitalizeChar => self.line_buffer.capitalize_char(),

            // Clipboard operations
            EditCommand::CutSelection => self.cut_selection_to_cut_buffer(),
            EditCommand::CopySelection => self.copy_selection_to_cut_buffer(),
            EditCommand::PasteCutBuffer => self.paste_cut_buffer(),

            // Add missing arm:
            EditCommand::SelectAll => self.select_all(),

            // If you have system clipboard arms:
            #[cfg(feature = "system_clipboard")]
            EditCommand::CutSelectionSystem => self.cut_selection_to_system(),
            #[cfg(feature = "system_clipboard")]
            EditCommand::CopySelectionSystem => self.copy_selection_to_system(),
            #[cfg(feature = "system_clipboard")]
            EditCommand::PasteSystem => self.paste_from_system(),
        }

        // Update selection anchor based on command type
        if !matches!(command.edit_type(), EditType::MoveCursor { select: true }) {
            self.selection_anchor = None;
        }

        // Update undo state
        let new_undo_behavior = match (command, command.edit_type()) {
            (_, EditType::MoveCursor { .. }) => UndoBehavior::MoveCursor,
            (EditCommand::InsertChar(c), EditType::EditText) => UndoBehavior::InsertCharacter(*c),
            (EditCommand::Delete, EditType::EditText) => {
                let deleted_char = self.edit_stack.current().grapheme_right().chars().next();
                UndoBehavior::Delete(deleted_char)
            }
            (EditCommand::Backspace, EditType::EditText) => {
                let deleted_char = self.edit_stack.current().grapheme_left().chars().next();
                UndoBehavior::Backspace(deleted_char)
            }
            (_, EditType::UndoRedo) => UndoBehavior::UndoRedo,
            (_, _) => UndoBehavior::CreateUndoPoint,
        };

        self.update_undo_state(new_undo_behavior);
    }

    fn handle_motion(&mut self, motion: MotionKind, action: MotionAction) {
        let start_idx = self.line_buffer.insertion_point();
        let end_idx = self.motion_destination(motion);

        // Compute the range to operate on
        let (range_start, range_end) = if end_idx > start_idx {
            (start_idx, end_idx)
        } else {
            (end_idx, start_idx)
        };

        // Apply the action
        match action {
            MotionAction::MoveCursor { select } => {
                // If we want to maintain selection, set the anchor if not already set:
                if select {
                    // If there's no anchor yet, set anchor to the old cursor
                    self.selection_anchor = self.selection_anchor.or(Some(start_idx));
                } else {
                    self.selection_anchor = None;
                }
                // Move insertion point
                self.line_buffer.set_insertion_point(end_idx);
            }
            MotionAction::Copy => {
                let content = &self.line_buffer.get_buffer()[range_start..range_end];
                self.cut_buffer.set(content, ClipboardMode::Normal);
            }
            MotionAction::Cut => {
                let content = &self.line_buffer.get_buffer()[range_start..range_end];
                self.cut_buffer.set(content, ClipboardMode::Normal);
                self.line_buffer.clear_range(range_start..range_end);
                self.line_buffer.set_insertion_point(range_start);
            }
            MotionAction::Delete => {
                self.line_buffer.clear_range(range_start..range_end);
                self.line_buffer.set_insertion_point(range_start);
            }
        }
    }

    fn motion_destination(&self, motion: MotionKind) -> usize {
        match motion {
            MotionKind::MoveLeft => self.line_buffer.grapheme_left_index(),
            MotionKind::MoveRight => self.line_buffer.grapheme_right_index(),
            MotionKind::MoveWordLeft => self.line_buffer.word_left_index(),
            MotionKind::MoveWordRight => self.line_buffer.word_right_start_index(),
            MotionKind::MoveBigWordLeft => self.line_buffer.big_word_left_index(),
            MotionKind::MoveBigWordRight => self.line_buffer.big_word_right_start_index(),
            MotionKind::MoveToLineStart => self.line_buffer.find_current_line_start(),
            MotionKind::MoveToLineEnd => self.line_buffer.find_current_line_end(),
            MotionKind::MoveToStart => 0,
            MotionKind::MoveToEnd => self.line_buffer.get_buffer().len(),
            MotionKind::MoveWordRightEnd => self.line_buffer.word_right_end_index(),
            MotionKind::MoveBigWordRightEnd => self.line_buffer.big_word_right_end_index(),
            MotionKind::MoveLineUp => {
                let current_pos = self.line_buffer.insertion_point();
                let start_of_line = self.line_buffer.find_current_line_start();
                let relative_pos = current_pos - start_of_line;
                
                // Find start of previous line
                if start_of_line > 0 {
                    let prev_line_start = self.line_buffer.find_line_start_before(start_of_line);
                    let prev_line_end = start_of_line;
                    let prev_line_len = prev_line_end - prev_line_start;
                    
                    // Keep same relative position if possible
                    prev_line_start + relative_pos.min(prev_line_len)
                } else {
                    current_pos // Already at first line
                }
            }
            MotionKind::MoveLineDown => {
                let current_pos = self.line_buffer.insertion_point();
                let start_of_line = self.line_buffer.find_current_line_start();
                let relative_pos = current_pos - start_of_line;
                
                let end_of_line = self.line_buffer.find_current_line_end();
                if end_of_line < self.line_buffer.get_buffer().len() {
                    let next_line_start = end_of_line + 1; // Skip newline
                    let next_line_end = self.line_buffer.find_line_end_after(next_line_start);
                    let next_line_len = next_line_end - next_line_start;
                    
                    // Keep same relative position if possible
                    next_line_start + relative_pos.min(next_line_len)
                } else {
                    current_pos // Already at last line
                }
            }
            MotionKind::MoveUntilChar { c, inclusive, forward } => {
                if forward {
                    // Instead of find_next_char, use find_char_right
                    if let Some(index) = self.line_buffer.find_char_right(c, /* e.g. false for entire line? */ false) {
                        // If inclusive is false => we want to move up until just before char?
                        let delta = if inclusive { c.len_utf8() } else { 0 };
                        (index + delta).min(self.line_buffer.get_buffer().len())
                    } else {
                        self.line_buffer.get_buffer().len()
                    }
                } else {
                    // Instead of find_previous_char, use find_char_left
                    if let Some(index) = self.line_buffer.find_char_left(c, /* e.g. false? */ false) {
                        // If inclusive, move behind that char
                        let offset = if inclusive {
                            index
                        } else {
                            index + c.len_utf8()
                        };
                        offset.max(0)
                    } else {
                        0
                    }
                }
            }
            MotionKind::MoveToPosition(pos) => pos,
        }
    }

    pub(crate) fn move_line_up(&mut self) {
        self.handle_motion(MotionKind::MoveLineUp, MotionAction::MoveCursor { select: false });
    }

    pub(crate) fn move_line_down(&mut self) {
        self.handle_motion(MotionKind::MoveLineDown, MotionAction::MoveCursor { select: false });
    }

    fn move_cursor_to(&mut self, new_offset: usize, select: bool) {
        self.handle_motion(
            MotionKind::MoveToPosition(new_offset),
            MotionAction::MoveCursor { select },
        );
    }

    fn cut_range(&mut self, start: usize, end: usize) {
        let (s, e) = if start <= end { (start, end) } else { (end, start) };
        let slice = &self.line_buffer.get_buffer()[s..e];
        self.cut_buffer.set(slice, ClipboardMode::Normal);
        self.line_buffer.clear_range(s..e);
        // set insertion_point to s
        self.line_buffer.set_insertion_point(s);
    }

    fn copy_range(&mut self, start: usize, end: usize) {
        let (s, e) = if start <= end { (start, end) } else { (end, start) };
        let slice = &self.line_buffer.get_buffer()[s..e];
        self.cut_buffer.set(slice, ClipboardMode::Normal);
        // do not remove text from buffer
    }

    fn delete_range(&mut self, start: usize, end: usize) {
        let (s, e) = if start <= end { (start, end) } else { (end, start) };
        self.line_buffer.clear_range(s..e);
        self.line_buffer.set_insertion_point(s);
    }

    fn update_selection_anchor(&mut self, select: bool) {
        self.selection_anchor = if select {
            self.selection_anchor
                .or_else(|| Some(self.insertion_point()))
        } else {
            None
        };
    }
    fn move_to_position(&mut self, position: usize, select: bool) {
        self.update_selection_anchor(select);
        self.line_buffer.set_insertion_point(position)
    }

    /// Get the text of the current [`LineBuffer`]
    pub fn get_buffer(&self) -> &str {
        self.line_buffer.get_buffer()
    }

    /// Edit the [`LineBuffer`] in an undo-safe manner.
    pub fn edit_buffer<F>(&mut self, func: F, undo_behavior: UndoBehavior)
    where
        F: FnOnce(&mut LineBuffer),
    {
        self.update_undo_state(undo_behavior);
        func(&mut self.line_buffer);
    }

    /// Set the text of the current [`LineBuffer`] given the specified [`UndoBehavior`]
    /// Insertion point update to the end of the buffer.
    pub(crate) fn set_buffer(&mut self, buffer: String, undo_behavior: UndoBehavior) {
        self.line_buffer.set_buffer(buffer);
        self.update_undo_state(undo_behavior);
    }

    pub(crate) fn insertion_point(&self) -> usize {
        self.line_buffer.insertion_point()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.line_buffer.is_empty()
    }

    pub(crate) fn is_cursor_at_first_line(&self) -> bool {
        self.line_buffer.is_cursor_at_first_line()
    }

    pub(crate) fn is_cursor_at_last_line(&self) -> bool {
        self.line_buffer.is_cursor_at_last_line()
    }

    pub(crate) fn is_cursor_at_buffer_end(&self) -> bool {
        self.line_buffer.insertion_point() == self.get_buffer().len()
    }

    pub(crate) fn reset_undo_stack(&mut self) {
        self.edit_stack.reset();
    }

    pub(crate) fn move_to_start(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveToStart, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_to_end(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveToEnd, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_to_line_start(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveToLineStart, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_to_line_end(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveToLineEnd, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_left(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveLeft, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_right(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveRight, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_word_left(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveWordLeft, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_big_word_left(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveBigWordLeft, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_word_right(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveWordRight, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_word_right_end(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveWordRightEnd, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_big_word_right_end(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveBigWordRightEnd, MotionAction::MoveCursor { select });
    }

    pub(crate) fn move_right_until_char(
        &mut self,
        c: char,
        before_char: bool,
        current_line: bool,
        select: bool,
    ) {
        self.handle_motion(
            MotionKind::MoveUntilChar {
                c,
                inclusive: !before_char,
                forward: true,
            },
            MotionAction::MoveCursor { select },
        );
    }

    pub(crate) fn move_left_until_char(
        &mut self,
        c: char,
        before_char: bool,
        current_line: bool,
        select: bool,
    ) {
        self.handle_motion(
            MotionKind::MoveUntilChar {
                c,
                inclusive: !before_char,
                forward: false,
            },
            MotionAction::MoveCursor { select },
        );
    }

    fn undo(&mut self) {
        let val = self.edit_stack.undo();
        self.line_buffer = val.clone();
    }

    fn redo(&mut self) {
        let val = self.edit_stack.redo();
        self.line_buffer = val.clone();
    }

    pub(crate) fn update_undo_state(&mut self, undo_behavior: UndoBehavior) {
        if matches!(undo_behavior, UndoBehavior::UndoRedo) {
            self.last_undo_behavior = UndoBehavior::UndoRedo;
            return;
        }
        if !undo_behavior.create_undo_point_after(&self.last_undo_behavior) {
            self.edit_stack.undo();
        }
        self.edit_stack.insert(self.line_buffer.clone());
        self.last_undo_behavior = undo_behavior;
    }

    fn cut_current_line(&mut self) {
        self.handle_motion(MotionKind::MoveToLineEnd, MotionAction::Cut);
    }

    fn cut_from_start(&mut self) {
        self.handle_motion(MotionKind::MoveToStart, MotionAction::Cut);
    }

    fn cut_from_line_start(&mut self) {
        self.handle_motion(MotionKind::MoveToLineStart, MotionAction::Cut);
    }

    fn cut_from_end(&mut self) {
        self.handle_motion(MotionKind::MoveToEnd, MotionAction::Cut);
    }

    fn cut_to_line_end(&mut self) {
        self.handle_motion(MotionKind::MoveToLineEnd, MotionAction::Cut);
    }

    pub(crate) fn copy_from_start(&mut self) {
        self.handle_motion(MotionKind::MoveToStart, MotionAction::Copy);
    }

    pub(crate) fn copy_from_line_start(&mut self) {
        self.handle_motion(MotionKind::MoveToLineStart, MotionAction::Copy);
    }

    pub(crate) fn copy_from_end(&mut self) {
        self.handle_motion(MotionKind::MoveToEnd, MotionAction::Copy);
    }

    pub(crate) fn copy_to_line_end(&mut self) {
        self.handle_motion(MotionKind::MoveToLineEnd, MotionAction::Copy);
    }

    fn replace_char(&mut self, character: char) {
        self.line_buffer.delete_right_grapheme();

        self.line_buffer.insert_char(character);
    }

    fn replace_chars(&mut self, n_chars: usize, string: &str) {
        for _ in 0..n_chars {
            self.line_buffer.delete_right_grapheme();
        }

        self.line_buffer.insert_str(string);
    }

    fn select_all(&mut self) {
        self.handle_motion(MotionKind::MoveToStart, MotionAction::MoveCursor { select: false });
        self.handle_motion(MotionKind::MoveToEnd, MotionAction::MoveCursor { select: true });
    }

    #[cfg(feature = "system_clipboard")]
    fn cut_selection_to_system(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.system_clipboard.set(cut_slice, ClipboardMode::Normal);
            self.line_buffer.clear_range_safe(start, end);
            self.selection_anchor = None;
        }
    }

    fn cut_selection_to_cut_buffer(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
            self.line_buffer.clear_range_safe(start, end);
            self.selection_anchor = None;
        }
    }

    #[cfg(feature = "system_clipboard")]
    fn copy_selection_to_system(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.system_clipboard.set(cut_slice, ClipboardMode::Normal);
        }
    }

    fn copy_selection_to_cut_buffer(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            let cut_slice = &self.line_buffer.get_buffer()[start..end];
            self.cut_buffer.set(cut_slice, ClipboardMode::Normal);
        }
    }

    /// If a selection is active returns the selected range, otherwise None.
    /// The range is guaranteed to be ascending.
    pub fn get_selection(&self) -> Option<(usize, usize)> {
        self.selection_anchor.map(|selection_anchor| {
            let buffer_len = self.line_buffer.len();
            if self.insertion_point() > selection_anchor {
                (
                    selection_anchor,
                    self.line_buffer.grapheme_right_index().min(buffer_len),
                )
            } else {
                (
                    self.insertion_point(),
                    self.line_buffer
                        .grapheme_right_index_from_pos(selection_anchor)
                        .min(buffer_len),
                )
            }
        })
    }

    fn delete_selection(&mut self) {
        if let Some((start, end)) = self.get_selection() {
            self.line_buffer.clear_range_safe(start, end);
            self.selection_anchor = None;
        }
    }

    fn backspace(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
        } else {
            self.line_buffer.delete_left_grapheme();
        }
    }

    fn delete(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
        } else {
            self.line_buffer.delete_right_grapheme();
        }
    }

    fn move_word_right_start(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveWordRight, MotionAction::MoveCursor { select });
    }

    fn move_big_word_right_start(&mut self, select: bool) {
        self.handle_motion(MotionKind::MoveBigWordRight, MotionAction::MoveCursor { select });
    }

    fn insert_char(&mut self, c: char) {
        self.delete_selection();
        self.line_buffer.insert_char(c);
    }

    fn insert_str(&mut self, str: &str) {
        self.delete_selection();
        self.line_buffer.insert_str(str);
    }

    fn insert_newline(&mut self) {
        self.delete_selection();
        self.line_buffer.insert_newline();
    }

    #[cfg(feature = "system_clipboard")]
    fn paste_from_system(&mut self) {
        self.delete_selection();
        insert_clipboard_content_before(&mut self.line_buffer, self.system_clipboard.deref_mut());
    }

    fn paste_cut_buffer(&mut self) {
        self.delete_selection();
        insert_clipboard_content_before(&mut self.line_buffer, self.cut_buffer.deref_mut());
    }

    pub(crate) fn reset_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// Delete text strictly between matching `left_char` and `right_char`.
    /// Places deleted text into the cut buffer.
    /// Leaves the parentheses/quotes/etc. themselves.
    /// On success, move the cursor just after the `left_char`.
    /// If matching chars can't be found, restore the original cursor.
    pub(crate) fn cut_inside(&mut self, left_char: char, right_char: char) {
        let old_pos = self.insertion_point();
        let buffer_len = self.line_buffer.len();

        if let Some((lp, rp)) =
            self.line_buffer
                .find_matching_pair(left_char, right_char, self.insertion_point())
        {
            let inside_start = lp + left_char.len_utf8();
            if inside_start < rp && rp <= buffer_len {
                let inside_slice = &self.line_buffer.get_buffer()[inside_start..rp];
                if !inside_slice.is_empty() {
                    self.cut_buffer.set(inside_slice, ClipboardMode::Normal);
                    self.line_buffer.clear_range_safe(inside_start, rp);
                }
                self.line_buffer
                    .set_insertion_point(lp + left_char.len_utf8());
                return;
            }
        }
        // If no valid pair was found, restore original cursor
        self.line_buffer.set_insertion_point(old_pos);
    }

    /// Yank text strictly between matching `left_char` and `right_char`.
    /// Copies it into the cut buffer without removing anything.
    /// Leaves the buffer unchanged and restores the original cursor.
    pub(crate) fn yank_inside(&mut self, left_char: char, right_char: char) {
        let old_pos = self.insertion_point();
        let buffer_len = self.line_buffer.len();

        if let Some((lp, rp)) =
            self.line_buffer
                .find_matching_pair(left_char, right_char, self.insertion_point())
        {
            let inside_start = lp + left_char.len_utf8();
            if inside_start < rp && rp <= buffer_len {
                let inside_slice = &self.line_buffer.get_buffer()[inside_start..rp];
                if !inside_slice.is_empty() {
                    self.cut_buffer.set(inside_slice, ClipboardMode::Normal);
            }
        }
        }

        // Always restore the cursor position
        self.line_buffer.set_insertion_point(old_pos);
    }
}

fn insert_clipboard_content_before(line_buffer: &mut LineBuffer, clipboard: &mut dyn Clipboard) {
    match clipboard.get() {
        (content, ClipboardMode::Normal) => {
            line_buffer.insert_str(&content);
        }
        (mut content, ClipboardMode::Lines) => {
            // TODO: Simplify that?
            line_buffer.move_to_line_start();
            line_buffer.move_line_up();
            if !content.ends_with('\n') {
                // TODO: Make sure platform requirements are met
                content.push('\n');
            }
            line_buffer.insert_str(&content);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn editor_with(buffer: &str) -> Editor {
        let mut editor = Editor::default();
        editor.set_buffer(buffer.to_string(), UndoBehavior::CreateUndoPoint);
        editor
    }

    #[rstest]
    #[case("abc def ghi", 11, "abc def ")]
    #[case("abc def-ghi", 11, "abc def-")]
    #[case("abc def.ghi", 11, "abc ")]
    fn test_cut_word_left(#[case] input: &str, #[case] position: usize, #[case] expected: &str) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(position);

        editor.cut_word_left();

        assert_eq!(editor.get_buffer(), expected);
    }

    #[rstest]
    #[case("abc def ghi", 11, "abc def ")]
    #[case("abc def-ghi", 11, "abc ")]
    #[case("abc def.ghi", 11, "abc ")]
    #[case("abc def gh ", 11, "abc def ")]
    fn test_cut_big_word_left(
        #[case] input: &str,
        #[case] position: usize,
        #[case] expected: &str,
    ) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(position);

        editor.cut_big_word_left();

        assert_eq!(editor.get_buffer(), expected);
    }

    #[rstest]
    #[case("hello world", 0, 'l', 1, false, "lo world")]
    #[case("hello world", 0, 'l', 1, true, "llo world")]
    #[ignore = "Deleting two consecutive chars is not implemented correctly and needs the multiplier explicitly."]
    #[case("hello world", 0, 'l', 2, false, "o world")]
    #[case("hello world", 0, 'h', 1, false, "hello world")]
    #[case("hello world", 0, 'l', 3, true, "ld")]
    #[case("hello world", 4, 'o', 1, true, "hellorld")]
    #[case("hello world", 4, 'w', 1, false, "hellorld")]
    #[case("hello world", 4, 'o', 1, false, "hellrld")]
    fn test_cut_right_until_char(
        #[case] input: &str,
        #[case] position: usize,
        #[case] search_char: char,
        #[case] repeat: usize,
        #[case] before_char: bool,
        #[case] expected: &str,
    ) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(position);
        for _ in 0..repeat {
            editor.cut_right_until_char(search_char, before_char, true);
        }
        assert_eq!(editor.get_buffer(), expected);
    }

    #[rstest]
    #[case("abc", 1, 'X', "aXc")]
    #[case("abc", 1, '🔄', "a🔄c")]
    #[case("a🔄c", 1, 'X', "aXc")]
    #[case("a🔄c", 1, '🔀', "a🔀c")]
    fn test_replace_char(
        #[case] input: &str,
        #[case] position: usize,
        #[case] replacement: char,
        #[case] expected: &str,
    ) {
        let mut editor = editor_with(input);
        editor.line_buffer.set_insertion_point(position);

        editor.replace_char(replacement);

        assert_eq!(editor.get_buffer(), expected);
    }

    fn str_to_edit_commands(s: &str) -> Vec<EditCommand> {
        s.chars().map(EditCommand::InsertChar).collect()
    }

    #[test]
    fn test_undo_insert_works_on_work_boundaries() {
        let mut editor = editor_with("This is  a");
        for cmd in str_to_edit_commands(" test") {
            editor.run_edit_command(&cmd);
        }
        assert_eq!(editor.get_buffer(), "This is  a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is  a");
        editor.run_edit_command(&EditCommand::Redo);
        assert_eq!(editor.get_buffer(), "This is  a test");
    }

    #[test]
    fn test_undo_backspace_works_on_word_boundaries() {
        let mut editor = editor_with("This is  a test");
        for _ in 0..6 {
            editor.run_edit_command(&EditCommand::Backspace);
        }
        assert_eq!(editor.get_buffer(), "This is  ");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is  a");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is  a test");
    }

    #[test]
    fn test_undo_delete_works_on_word_boundaries() {
        let mut editor = editor_with("This  is a test");
        editor.line_buffer.set_insertion_point(0);
        for _ in 0..7 {
            editor.run_edit_command(&EditCommand::Delete);
        }
        assert_eq!(editor.get_buffer(), "s a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This  is a test");
    }

    #[test]
    fn test_undo_insert_with_newline() {
        let mut editor = editor_with("This is a");
        for cmd in str_to_edit_commands(" \n test") {
            editor.run_edit_command(&cmd);
        }
        assert_eq!(editor.get_buffer(), "This is a \n test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \n");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a");
    }

    #[test]
    fn test_undo_backspace_with_newline() {
        let mut editor = editor_with("This is a \n test");
        for _ in 0..8 {
            editor.run_edit_command(&EditCommand::Backspace);
        }
        assert_eq!(editor.get_buffer(), "This is ");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \n");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \n test");
    }

    #[test]
    fn test_undo_backspace_with_crlf() {
        let mut editor = editor_with("This is a \r\n test");
        for _ in 0..8 {
            editor.run_edit_command(&EditCommand::Backspace);
        }
        assert_eq!(editor.get_buffer(), "This is ");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \r\n");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This is a \r\n test");
    }

    #[test]
    fn test_undo_delete_with_newline() {
        let mut editor = editor_with("This \n is a test");
        editor.line_buffer.set_insertion_point(0);
        for _ in 0..8 {
            editor.run_edit_command(&EditCommand::Delete);
        }
        assert_eq!(editor.get_buffer(), "s a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "\n is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This \n is a test");
    }

    #[test]
    fn test_undo_delete_with_crlf() {
        // CLRF delete is a special case, since the first character of the
        // grapheme is \r rather than \n
        let mut editor = editor_with("This \r\n is a test");
        editor.line_buffer.set_insertion_point(0);
        for _ in 0..8 {
            editor.run_edit_command(&EditCommand::Delete);
        }
        assert_eq!(editor.get_buffer(), "s a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "\r\n is a test");
        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(editor.get_buffer(), "This \r\n is a test");
    }
    #[cfg(feature = "system_clipboard")]
    mod without_system_clipboard {
        use super::*;
        #[test]
        fn test_cut_selection_system() {
            let mut editor = editor_with("This is a test!");
            editor.selection_anchor = Some(editor.line_buffer.len());
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::CutSelectionSystem);
            assert!(editor.line_buffer.get_buffer().is_empty());
        }
        #[test]
        fn test_copypaste_selection_system() {
            let s = "This is a test!";
            let mut editor = editor_with(s);
            editor.selection_anchor = Some(editor.line_buffer.len());
            editor.line_buffer.set_insertion_point(0);
            editor.run_edit_command(&EditCommand::CopySelectionSystem);
            editor.run_edit_command(&EditCommand::PasteSystem);
            pretty_assertions::assert_eq!(editor.line_buffer.len(), s.len() * 2);
        }
    }

    #[test]
    fn test_cut_inside_brackets() {
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(5, false); // Move inside brackets
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo()baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with cursor outside brackets
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(0, false);
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar)baz");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, "");

        // Test with no matching brackets
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "");
    }

    #[test]
    fn test_cut_inside_quotes() {
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(5, false); // Move inside quotes
        editor.cut_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"\"baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with cursor outside quotes
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(0, false);
        editor.cut_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"bar\"baz");
        assert_eq!(editor.insertion_point(), 0);
        assert_eq!(editor.cut_buffer.get().0, "");

        // Test with no matching quotes
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.cut_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
    }

    #[test]
    fn test_cut_inside_nested() {
        let mut editor = editor_with("foo(bar(baz)qux)quux");
        editor.move_to_position(8, false); // Move inside inner brackets
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar()qux)quux");
        assert_eq!(editor.insertion_point(), 8);
        assert_eq!(editor.cut_buffer.get().0, "baz");

        editor.move_to_position(4, false); // Move inside outer brackets
        editor.cut_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo()quux");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar()qux");
    }

    #[test]
    fn test_yank_inside_brackets() {
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(5, false); // Move inside brackets
        editor.yank_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar)baz"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 5); // Cursor should return to original position

        // Test yanked content by pasting
        editor.paste_cut_buffer();
        assert_eq!(editor.get_buffer(), "foo(bbarar)baz");

        // Test with cursor outside brackets
        let mut editor = editor_with("foo(bar)baz");
        editor.move_to_position(0, false);
        editor.yank_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar)baz");
        assert_eq!(editor.insertion_point(), 0);
    }

    #[test]
    fn test_yank_inside_quotes() {
        let mut editor = editor_with("foo\"bar\"baz");
        editor.move_to_position(5, false); // Move inside quotes
        editor.yank_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo\"bar\"baz"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 5); // Cursor should return to original position
        assert_eq!(editor.cut_buffer.get().0, "bar");

        // Test with no matching quotes
        let mut editor = editor_with("foo bar baz");
        editor.move_to_position(4, false);
        editor.yank_inside('"', '"');
        assert_eq!(editor.get_buffer(), "foo bar baz");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "");
    }

    #[test]
    fn test_yank_inside_nested() {
        let mut editor = editor_with("foo(bar(baz)qux)quux");
        editor.move_to_position(8, false); // Move inside inner brackets
        editor.yank_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar(baz)qux)quux"); // Buffer shouldn't change
        assert_eq!(editor.insertion_point(), 8);
        assert_eq!(editor.cut_buffer.get().0, "baz");

        // Test yanked content by pasting
        editor.paste_cut_buffer();
        assert_eq!(editor.get_buffer(), "foo(bar(bazbaz)qux)quux");

        editor.move_to_position(4, false); // Move inside outer brackets
        editor.yank_inside('(', ')');
        assert_eq!(editor.get_buffer(), "foo(bar(bazbaz)qux)quux");
        assert_eq!(editor.insertion_point(), 4);
        assert_eq!(editor.cut_buffer.get().0, "bar(bazbaz)qux");
    }
}
