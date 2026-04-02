use crossterm::event::{KeyEvent, MouseEvent};

/// Events the TUI processes
pub enum AppEvent {
    /// Keyboard input
    Key(KeyEvent),
    /// Mouse input
    Mouse(MouseEvent),
    /// Terminal resized
    Resize(u16, u16),
    /// Query event from the engine
    Query(crate::query::QueryEvent),
    /// Tick for periodic updates
    Tick,
}
