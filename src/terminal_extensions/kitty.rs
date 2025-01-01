use crossterm::{event, execute};

/// Helper managing proper setup and teardown of the kitty keyboard enhancement protocol
///
/// Note that, currently, only the following support this protocol:
/// * [kitty terminal](https://sw.kovidgoyal.net/kitty/)
/// * [foot terminal](https://codeberg.org/dnkl/foot/issues/319)
/// * [WezTerm terminal](https://wezfurlong.org/wezterm/config/lua/config/enable_kitty_keyboard.html)
/// * [notcurses library](https://github.com/dankamongmen/notcurses/issues/2131)
/// * [neovim text editor](https://github.com/neovim/neovim/pull/18181)
/// * [kakoune text editor](https://github.com/mawww/kakoune/issues/4103)
/// * [dte text editor](https://gitlab.com/craigbarnes/dte/-/issues/138)
///
/// Refer to <https://sw.kovidgoyal.net/kitty/keyboard-protocol/> if you're curious.
#[derive(Default)]
pub(crate) struct KittyProtocolGuard {
    enabled: bool,
    active: bool,
}

impl KittyProtocolGuard {
    pub fn set(&mut self, enable: bool) {
        let available = super::kitty_protocol_available();
        eprintln!("[DEBUG] Kitty protocol available: {}", available);
        self.enabled = enable && available;
        eprintln!("[DEBUG] Kitty protocol enabled: {}", self.enabled);
    }
    pub fn enter(&mut self) {
        if self.enabled && !self.active {
            eprintln!("[DEBUG] Activating Kitty keyboard protocol with all enhancement flags");
            let _ = execute!(
                std::io::stdout(),
                event::PushKeyboardEnhancementFlags(
                    event::KeyboardEnhancementFlags::all()
                )
            );

            self.active = true;
            eprintln!("[DEBUG] Kitty keyboard protocol activated: {}", self.active);
        }
    }
    pub fn exit(&mut self) {
        if self.active {
            eprintln!("[DEBUG] Deactivating Kitty keyboard protocol");
            let _ = execute!(std::io::stdout(), event::PopKeyboardEnhancementFlags);
            self.active = false;
            eprintln!("[DEBUG] Kitty keyboard protocol deactivated");
        }
    }
}

impl Drop for KittyProtocolGuard {
    fn drop(&mut self) {
        if self.active {
            eprintln!("[DEBUG] Cleaning up Kitty keyboard protocol on drop");
            let _ = execute!(std::io::stdout(), event::PopKeyboardEnhancementFlags);
            eprintln!("[DEBUG] Kitty keyboard protocol cleanup complete");
        }
    }
}
