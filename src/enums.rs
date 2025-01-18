use crossterm::event::{Event, KeyEvent, KeyEventKind};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use strum_macros::EnumIter;

/// Valid ways how `Reedline::read_line()` can return
#[derive(Debug)]
pub enum Signal {
    /// Entry succeeded with the provided content
    Success(String),
    /// Entry was aborted with `Ctrl+C`
    CtrlC, // Interrupt current editing
    /// Abort with `Ctrl+D` signalling `EOF` or abort of a whole interactive session
    CtrlD, // End terminal session
}

/// How we compute the “target index” or “destination” from the current cursor.
#[derive(Copy, Clone, Debug)]
pub enum MotionKind {
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    MoveBigWordLeft,
    MoveBigWordRight,
    MoveToLineStart,
    MoveToLineEnd,
    // Char-based search
    MoveUntilChar {
        c: char,
        inclusive: bool,  // do we include that char in the motion?
        forward: bool,    // left vs. right
    },
    // etc. Add more as needed, e.g. MoveLineUp, MoveLineDown, NextWhitespace, etc.
}

#[derive(Copy, Clone, Debug)]
pub enum MotionAction {
    /// Just move the cursor, optionally enabling selection if `select==true`.
    MoveCursor { select: bool },
    /// Copy that range into the cut/clipboard buffer (does not remove).
    Copy,
    /// Cut that range (copy + remove from line buffer).
    Cut,
    /// Delete that range (remove from line buffer but do not copy).
    Delete,
    /// Possibly more exotic actions: e.g. “SwapCaseInRange”
    // ...
}

/// For inside-pair operations:
#[derive(Copy, Clone, Debug)]
pub enum PairAction {
    CutInside,
    YankInside,
    // or “DeleteInside”, “ChangeInside”, etc.
}



/// Editing actions which can be mapped to key bindings.
///
/// Executed by `Reedline::run_edit_commands()`
/// The core editing commands recognized by the `Editor`.
#[derive(Clone, Debug)]
pub enum EditCommand {
    /// Insert the given character at the cursor (inserting, not replacing).
    InsertChar(char),
    /// Insert the given string at the cursor.
    InsertString(String),
    /// Insert a newline (accounting for CRLF vs LF if desired).
    InsertNewline,

    /// Replace one character under the cursor by `chr` (delete+insert).
    ReplaceChar(char),
    /// Replace `n_chars` characters under the cursor with the given string.
    ReplaceChars(usize, String),

    /// A parametric command that uses a `MotionKind` to compute a range
    /// and then an action to apply (move cursor, cut, copy, delete, select).
    Motion {
        motion: MotionKind,
        action: MotionAction,
    },

    /// Delete one grapheme to the left (like Backspace).
    Backspace,
    /// Delete one grapheme to the right.
    Delete,

    /// Operate inside pair of matching delimiters, e.g. "di(" or "ya{"
    OperateInsidePair {
        left_char: char,
        right_char: char,
        action: PairAction,
    },

    /// Undo or redo
    Undo,
    Redo,

    /// Word-level transformations or advanced manipulations
    SwapGraphemes,
    SwapWords,
    UppercaseWord,
    LowercaseWord,
    SwitchcaseChar,
    CapitalizeChar,

    /// Possibly more specialized commands if needed
    SelectAll,
    Clear,              // clear entire line buffer
    ClearToLineEnd,
    CutSelection,
    CopySelection,
    PasteCutBuffer,
    // etc.
}


impl Display for EditCommand {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            EditCommand::MoveToStart { .. } => write!(f, "MoveToStart Optional[select: <bool>]"),
            EditCommand::MoveToLineStart { .. } => {
                write!(f, "MoveToLineStart Optional[select: <bool>]")
            }
            EditCommand::MoveToEnd { .. } => write!(f, "MoveToEnd Optional[select: <bool>]"),
            EditCommand::MoveToLineEnd { .. } => {
                write!(f, "MoveToLineEnd Optional[select: <bool>]")
            }
            EditCommand::MoveLeft { .. } => write!(f, "MoveLeft Optional[select: <bool>]"),
            EditCommand::MoveRight { .. } => write!(f, "MoveRight Optional[select: <bool>]"),
            EditCommand::MoveWordLeft { .. } => write!(f, "MoveWordLeft Optional[select: <bool>]"),
            EditCommand::MoveBigWordLeft { .. } => {
                write!(f, "MoveBigWordLeft Optional[select: <bool>]")
            }
            EditCommand::MoveWordRight { .. } => {
                write!(f, "MoveWordRight Optional[select: <bool>]")
            }
            EditCommand::MoveWordRightEnd { .. } => {
                write!(f, "MoveWordRightEnd Optional[select: <bool>]")
            }
            EditCommand::MoveBigWordRightEnd { .. } => {
                write!(f, "MoveBigWordRightEnd Optional[select: <bool>]")
            }
            EditCommand::MoveWordRightStart { .. } => {
                write!(f, "MoveWordRightStart Optional[select: <bool>]")
            }
            EditCommand::MoveBigWordRightStart { .. } => {
                write!(f, "MoveBigWordRightStart Optional[select: <bool>]")
            }
            EditCommand::MoveToPosition { .. } => {
                write!(f, "MoveToPosition  Value: <int>, Optional[select: <bool>]")
            }
            EditCommand::MoveLeftUntil { .. } => {
                write!(f, "MoveLeftUntil Value: <char>, Optional[select: <bool>]")
            }
            EditCommand::MoveLeftBefore { .. } => {
                write!(f, "MoveLeftBefore Value: <char>, Optional[select: <bool>]")
            }
            EditCommand::InsertChar(_) => write!(f, "InsertChar  Value: <char>"),
            EditCommand::InsertString(_) => write!(f, "InsertString Value: <string>"),
            EditCommand::InsertNewline => write!(f, "InsertNewline"),
            EditCommand::ReplaceChar(_) => write!(f, "ReplaceChar <char>"),
            EditCommand::ReplaceChars(_, _) => write!(f, "ReplaceChars <int> <string>"),
            EditCommand::Backspace => write!(f, "Backspace"),
            EditCommand::Delete => write!(f, "Delete"),
            EditCommand::CutChar => write!(f, "CutChar"),
            EditCommand::BackspaceWord => write!(f, "BackspaceWord"),
            EditCommand::DeleteWord => write!(f, "DeleteWord"),
            EditCommand::Clear => write!(f, "Clear"),
            EditCommand::ClearToLineEnd => write!(f, "ClearToLineEnd"),
            EditCommand::Complete => write!(f, "Complete"),
            EditCommand::CutCurrentLine => write!(f, "CutCurrentLine"),
            EditCommand::CutFromStart => write!(f, "CutFromStart"),
            EditCommand::CutFromLineStart => write!(f, "CutFromLineStart"),
            EditCommand::CutToEnd => write!(f, "CutToEnd"),
            EditCommand::CutToLineEnd => write!(f, "CutToLineEnd"),
            EditCommand::CutWordLeft => write!(f, "CutWordLeft"),
            EditCommand::CutBigWordLeft => write!(f, "CutBigWordLeft"),
            EditCommand::CutWordRight => write!(f, "CutWordRight"),
            EditCommand::CutBigWordRight => write!(f, "CutBigWordRight"),
            EditCommand::CutWordRightToNext => write!(f, "CutWordRightToNext"),
            EditCommand::CutBigWordRightToNext => write!(f, "CutBigWordRightToNext"),
            EditCommand::PasteCutBufferBefore => write!(f, "PasteCutBufferBefore"),
            EditCommand::PasteCutBufferAfter => write!(f, "PasteCutBufferAfter"),
            EditCommand::UppercaseWord => write!(f, "UppercaseWord"),
            EditCommand::LowercaseWord => write!(f, "LowercaseWord"),
            EditCommand::SwitchcaseChar => write!(f, "SwitchcaseChar"),
            EditCommand::CapitalizeChar => write!(f, "CapitalizeChar"),
            EditCommand::SwapWords => write!(f, "SwapWords"),
            EditCommand::SwapGraphemes => write!(f, "SwapGraphemes"),
            EditCommand::Undo => write!(f, "Undo"),
            EditCommand::Redo => write!(f, "Redo"),
            EditCommand::CutRightUntil(_) => write!(f, "CutRightUntil Value: <char>"),
            EditCommand::CutRightBefore(_) => write!(f, "CutRightBefore Value: <char>"),
            EditCommand::MoveRightUntil { .. } => write!(f, "MoveRightUntil Value: <char>"),
            EditCommand::MoveRightBefore { .. } => write!(f, "MoveRightBefore Value: <char>"),
            EditCommand::CutLeftUntil(_) => write!(f, "CutLeftUntil Value: <char>"),
            EditCommand::CutLeftBefore(_) => write!(f, "CutLeftBefore Value: <char>"),
            EditCommand::SelectAll => write!(f, "SelectAll"),
            EditCommand::CutSelection => write!(f, "CutSelection"),
            EditCommand::CopySelection => write!(f, "CopySelection"),
            EditCommand::Paste => write!(f, "Paste"),
            EditCommand::CopyFromStart => write!(f, "CopyFromStart"),
            EditCommand::CopyFromLineStart => write!(f, "CopyFromLineStart"),
            EditCommand::CopyToEnd => write!(f, "CopyToEnd"),
            EditCommand::CopyToLineEnd => write!(f, "CopyToLineEnd"),
            EditCommand::CopyCurrentLine => write!(f, "CopyCurrentLine"),
            EditCommand::CopyWordLeft => write!(f, "CopyWordLeft"),
            EditCommand::CopyBigWordLeft => write!(f, "CopyBigWordLeft"),
            EditCommand::CopyWordRight => write!(f, "CopyWordRight"),
            EditCommand::CopyBigWordRight => write!(f, "CopyBigWordRight"),
            EditCommand::CopyWordRightToNext => write!(f, "CopyWordRightToNext"),
            EditCommand::CopyBigWordRightToNext => write!(f, "CopyBigWordRightToNext"),
            EditCommand::CopyLeft => write!(f, "CopyLeft"),
            EditCommand::CopyRight => write!(f, "CopyRight"),
            EditCommand::CopyRightUntil(_) => write!(f, "CopyRightUntil Value: <char>"),
            EditCommand::CopyRightBefore(_) => write!(f, "CopyRightBefore Value: <char>"),
            EditCommand::CopyLeftUntil(_) => write!(f, "CopyLeftUntil Value: <char>"),
            EditCommand::CopyLeftBefore(_) => write!(f, "CopyLeftBefore Value: <char>"),
            #[cfg(feature = "system_clipboard")]
            EditCommand::CutSelectionSystem => write!(f, "CutSelectionSystem"),
            #[cfg(feature = "system_clipboard")]
            EditCommand::CopySelectionSystem => write!(f, "CopySelectionSystem"),
            #[cfg(feature = "system_clipboard")]
            EditCommand::PasteSystem => write!(f, "PasteSystem"),
            EditCommand::CutInside { .. } => write!(f, "CutInside Value: <char> <char>"),
            EditCommand::YankInside { .. } => write!(f, "YankInside Value: <char> <char>"),
        }
    }
}

impl EditCommand {
    /// Determine if a certain operation should be undoable
    /// or if the operations should be coalesced for undoing
    pub fn edit_type(&self) -> EditType {
        match self {
            // Cursor moves
            EditCommand::MoveToStart { select, .. }
            | EditCommand::MoveToEnd { select, .. }
            | EditCommand::MoveToLineStart { select, .. }
            | EditCommand::MoveToLineEnd { select, .. }
            | EditCommand::MoveToPosition { select, .. }
            | EditCommand::MoveLeft { select, .. }
            | EditCommand::MoveRight { select, .. }
            | EditCommand::MoveWordLeft { select, .. }
            | EditCommand::MoveBigWordLeft { select, .. }
            | EditCommand::MoveWordRight { select, .. }
            | EditCommand::MoveWordRightStart { select, .. }
            | EditCommand::MoveBigWordRightStart { select, .. }
            | EditCommand::MoveWordRightEnd { select, .. }
            | EditCommand::MoveBigWordRightEnd { select, .. }
            | EditCommand::MoveRightUntil { select, .. }
            | EditCommand::MoveRightBefore { select, .. }
            | EditCommand::MoveLeftUntil { select, .. }
            | EditCommand::MoveLeftBefore { select, .. } => {
                EditType::MoveCursor { select: *select }
            }

            EditCommand::SelectAll => EditType::MoveCursor { select: true },
            // Text edits
            EditCommand::InsertChar(_)
            | EditCommand::Backspace
            | EditCommand::Delete
            | EditCommand::CutChar
            | EditCommand::InsertString(_)
            | EditCommand::InsertNewline
            | EditCommand::ReplaceChar(_)
            | EditCommand::ReplaceChars(_, _)
            | EditCommand::BackspaceWord
            | EditCommand::DeleteWord
            | EditCommand::Clear
            | EditCommand::ClearToLineEnd
            | EditCommand::Complete
            | EditCommand::CutCurrentLine
            | EditCommand::CutFromStart
            | EditCommand::CutFromLineStart
            | EditCommand::CutToLineEnd
            | EditCommand::CutToEnd
            | EditCommand::CutWordLeft
            | EditCommand::CutBigWordLeft
            | EditCommand::CutWordRight
            | EditCommand::CutBigWordRight
            | EditCommand::CutWordRightToNext
            | EditCommand::CutBigWordRightToNext
            | EditCommand::PasteCutBufferBefore
            | EditCommand::PasteCutBufferAfter
            | EditCommand::UppercaseWord
            | EditCommand::LowercaseWord
            | EditCommand::SwitchcaseChar
            | EditCommand::CapitalizeChar
            | EditCommand::SwapWords
            | EditCommand::SwapGraphemes
            | EditCommand::CutRightUntil(_)
            | EditCommand::CutRightBefore(_)
            | EditCommand::CutLeftUntil(_)
            | EditCommand::CutLeftBefore(_)
            | EditCommand::CutSelection
            | EditCommand::Paste => EditType::EditText,

            #[cfg(feature = "system_clipboard")] // Sadly cfg attributes in patterns don't work
            EditCommand::CutSelectionSystem | EditCommand::PasteSystem => EditType::EditText,

            EditCommand::Undo | EditCommand::Redo => EditType::UndoRedo,

            EditCommand::CopySelection => EditType::NoOp,
            #[cfg(feature = "system_clipboard")]
            EditCommand::CopySelectionSystem => EditType::NoOp,
            EditCommand::CutInside { .. } => EditType::EditText,
            EditCommand::YankInside { .. } => EditType::EditText,
            EditCommand::CopyFromStart
            | EditCommand::CopyFromLineStart
            | EditCommand::CopyToEnd
            | EditCommand::CopyToLineEnd
            | EditCommand::CopyCurrentLine
            | EditCommand::CopyWordLeft
            | EditCommand::CopyBigWordLeft
            | EditCommand::CopyWordRight
            | EditCommand::CopyBigWordRight
            | EditCommand::CopyWordRightToNext
            | EditCommand::CopyBigWordRightToNext
            | EditCommand::CopyLeft
            | EditCommand::CopyRight
            | EditCommand::CopyRightUntil(_)
            | EditCommand::CopyRightBefore(_)
            | EditCommand::CopyLeftUntil(_)
            | EditCommand::CopyLeftBefore(_) => EditType::NoOp,
        }
    }
}

/// Specifies the types of edit commands, used to simplify grouping edits
/// to mark undo behavior
#[derive(PartialEq, Eq)]
pub enum EditType {
    /// Cursor movement commands
    MoveCursor { select: bool },
    /// Undo/Redo commands
    UndoRedo,
    /// Text editing commands
    EditText,
    /// No effect on line buffer
    NoOp,
}

/// Every line change should come with an `UndoBehavior` tag, which can be used to
/// calculate how the change should be reflected on the undo stack
#[derive(Debug)]
pub enum UndoBehavior {
    /// Character insertion, tracking the character inserted
    InsertCharacter(char),
    /// Backspace command, tracking the deleted character (left of cursor)
    /// Warning: this does not track the whole grapheme, just the character
    Backspace(Option<char>),
    /// Delete command, tracking the deleted character (right of cursor)
    /// Warning: this does not track the whole grapheme, just the character
    Delete(Option<char>),
    /// Move the cursor position
    MoveCursor,
    /// Navigated the history using up or down arrows
    HistoryNavigation,
    /// Catch-all for actions that should always form a unique undo point and never be
    /// grouped with later edits
    CreateUndoPoint,
    /// Undo/Redo actions shouldn't be reflected on the edit stack
    UndoRedo,
}

impl UndoBehavior {
    /// Return if the current operation should start a new undo set, or be
    /// combined with the previous operation
    pub fn create_undo_point_after(&self, previous: &UndoBehavior) -> bool {
        use UndoBehavior as UB;
        match (previous, self) {
            // Never start an undo set with cursor movement
            (_, UB::MoveCursor) => false,
            (UB::HistoryNavigation, UB::HistoryNavigation) => false,
            // When inserting/deleting repeatedly, each undo set should encompass
            // inserting/deleting a complete word and the associated whitespace
            (UB::InsertCharacter(c_prev), UB::InsertCharacter(c_new)) => {
                (*c_prev == '\n' || *c_prev == '\r')
                    || (!c_prev.is_whitespace() && c_new.is_whitespace())
            }
            (UB::Backspace(Some(c_prev)), UB::Backspace(Some(c_new))) => {
                (*c_new == '\n' || *c_new == '\r')
                    || (c_prev.is_whitespace() && !c_new.is_whitespace())
            }
            (UB::Backspace(_), UB::Backspace(_)) => false,
            (UB::Delete(Some(c_prev)), UB::Delete(Some(c_new))) => {
                (*c_new == '\n' || *c_new == '\r')
                    || (c_prev.is_whitespace() && !c_new.is_whitespace())
            }
            (UB::Delete(_), UB::Delete(_)) => false,
            (_, _) => true,
        }
    }
}

/// Reedline supported actions.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, EnumIter)]
pub enum ReedlineEvent {
    /// No op event
    None,

    /// Complete history hint (default in full)
    HistoryHintComplete,

    /// Complete a single token/word of the history hint
    HistoryHintWordComplete,

    /// Handle EndOfLine event
    ///
    /// Expected Behavior:
    ///
    /// - On empty line breaks execution to exit with [`Signal::CtrlD`]
    /// - Secondary behavior [`EditCommand::Delete`]
    CtrlD,

    /// Handle SIGTERM key input
    ///
    /// Expected behavior:
    ///
    /// Abort entry
    /// Run [`EditCommand::Clear`]
    /// Clear the current undo
    /// Bubble up [`Signal::CtrlC`]
    CtrlC,

    /// Clears the screen and sets prompt to first line
    ClearScreen,

    /// Clears the screen and the scrollback buffer
    ///
    /// Sets the prompt back to the first line
    ClearScrollback,

    /// Handle enter event
    Enter,

    /// Handle unconditional submit event
    Submit,

    /// Submit at the end of the *complete* text, otherwise newline
    SubmitOrNewline,

    /// Esc event
    Esc,

    /// Mouse
    Mouse, // Fill in details later

    /// trigger terminal resize
    Resize(u16, u16),

    /// Run these commands in the editor
    Edit(Vec<EditCommand>),

    /// Trigger full repaint
    Repaint,

    /// Navigate to the previous historic buffer
    PreviousHistory,

    /// Move up to the previous line, if multiline, or up into the historic buffers
    Up,

    /// Move down to the next line, if multiline, or down through the historic buffers
    Down,

    /// Move right to the next column, completion entry, or complete hint
    Right,

    /// Move left to the next column, or completion entry
    Left,

    /// Navigate to the next historic buffer
    NextHistory,

    /// Search the history for a string
    SearchHistory,

    /// In vi mode multiple reedline events can be chained while parsing the
    /// command or movement characters
    Multiple(Vec<ReedlineEvent>),

    /// Test
    UntilFound(Vec<ReedlineEvent>),

    /// Trigger a menu event. It activates a menu with the event name
    Menu(String),

    /// Next element in the menu
    MenuNext,

    /// Previous element in the menu
    MenuPrevious,

    /// Moves up in the menu
    MenuUp,

    /// Moves down in the menu
    MenuDown,

    /// Moves left in the menu
    MenuLeft,

    /// Moves right in the menu
    MenuRight,

    /// Move to the next history page
    MenuPageNext,

    /// Move to the previous history page
    MenuPagePrevious,

    /// Way to bind the execution of a whole command (directly returning from [`crate::Reedline::read_line()`]) to a keybinding
    ExecuteHostCommand(String),

    /// Open text editor
    OpenEditor,

    /// Reset the current text selection
    ResetSelection,
}

impl Display for ReedlineEvent {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            ReedlineEvent::None => write!(f, "None"),
            ReedlineEvent::HistoryHintComplete => write!(f, "HistoryHintComplete"),
            ReedlineEvent::HistoryHintWordComplete => write!(f, "HistoryHintWordComplete"),
            ReedlineEvent::CtrlD => write!(f, "CtrlD"),
            ReedlineEvent::CtrlC => write!(f, "CtrlC"),
            ReedlineEvent::ClearScreen => write!(f, "ClearScreen"),
            ReedlineEvent::ClearScrollback => write!(f, "ClearScrollback"),
            ReedlineEvent::Enter => write!(f, "Enter"),
            ReedlineEvent::Submit => write!(f, "Submit"),
            ReedlineEvent::SubmitOrNewline => write!(f, "SubmitOrNewline"),
            ReedlineEvent::Esc => write!(f, "Esc"),
            ReedlineEvent::Mouse => write!(f, "Mouse"),
            ReedlineEvent::Resize(_, _) => write!(f, "Resize <int> <int>"),
            ReedlineEvent::Edit(_) => write!(
                f,
                "Edit: <EditCommand> or Edit: <EditCommand> value: <string>"
            ),
            ReedlineEvent::Repaint => write!(f, "Repaint"),
            ReedlineEvent::PreviousHistory => write!(f, "PreviousHistory"),
            ReedlineEvent::Up => write!(f, "Up"),
            ReedlineEvent::Down => write!(f, "Down"),
            ReedlineEvent::Right => write!(f, "Right"),
            ReedlineEvent::Left => write!(f, "Left"),
            ReedlineEvent::NextHistory => write!(f, "NextHistory"),
            ReedlineEvent::SearchHistory => write!(f, "SearchHistory"),
            ReedlineEvent::Multiple(_) => write!(f, "Multiple[ {{ ReedLineEvents, }} ]"),
            ReedlineEvent::UntilFound(_) => write!(f, "UntilFound [ {{ ReedLineEvents, }} ]"),
            ReedlineEvent::Menu(_) => write!(f, "Menu Name: <string>"),
            ReedlineEvent::MenuNext => write!(f, "MenuNext"),
            ReedlineEvent::MenuPrevious => write!(f, "MenuPrevious"),
            ReedlineEvent::MenuUp => write!(f, "MenuUp"),
            ReedlineEvent::MenuDown => write!(f, "MenuDown"),
            ReedlineEvent::MenuLeft => write!(f, "MenuLeft"),
            ReedlineEvent::MenuRight => write!(f, "MenuRight"),
            ReedlineEvent::MenuPageNext => write!(f, "MenuPageNext"),
            ReedlineEvent::MenuPagePrevious => write!(f, "MenuPagePrevious"),
            ReedlineEvent::ExecuteHostCommand(_) => write!(f, "ExecuteHostCommand"),
            ReedlineEvent::OpenEditor => write!(f, "OpenEditor"),
            ReedlineEvent::ResetSelection => write!(f, "ResetSelection"),
        }
    }
}

pub(crate) enum EventStatus {
    Handled,
    Inapplicable,
    Exits(Signal),
}

/// A wrapper for [crossterm::event::Event].
///
/// It ensures that the given event doesn't contain [KeyEventKind::Release]
/// (which is rejected) or [KeyEventKind::Repeat] (which is converted to
/// [KeyEventKind::Press]).
pub struct ReedlineRawEvent(Event);

impl TryFrom<Event> for ReedlineRawEvent {
    type Error = ();

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                kind: KeyEventKind::Release,
                ..
            }) => Err(()),
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Repeat,
                state,
            }) => Ok(Self(Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                state,
            }))),
            other => Ok(Self(other)),
        }
    }
}

impl From<ReedlineRawEvent> for Event {
    fn from(event: ReedlineRawEvent) -> Self {
        event.0
    }
}
