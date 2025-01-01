use crossterm::event::{read, Event, KeyEvent, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags};
use crossterm::{terminal, execute};
use std::io::{self, Write};

fn main() -> std::io::Result<()> {
    terminal::enable_raw_mode()?;
    
    // Enable all keyboard enhancement flags
    execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::all())
    )?;
    
    println!("Press keys to see raw events (Ctrl+C to exit)");
    println!("Keyboard enhancements enabled - you'll see press/release/repeat events");
    std::io::stdout().flush()?;

    let result = (|| -> std::io::Result<()> {
        loop {
            match read()? {
                Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind,
                    state,
                }) => {
                    println!(
                        "\r\nKey event:\r\n code: {:?}\r\n modifiers: {:?}\r\n kind: {:?}\r\n state: {:?}\r\n",
                        code, modifiers, kind, state
                    );
                    std::io::stdout().flush()?;

                    if matches!(code, crossterm::event::KeyCode::Char('c')) && modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                        break;
                    }
                }
                Event::Mouse(event) => {
                    println!("\r\nMouse event: {:?}\r\n", event);
                    std::io::stdout().flush()?;
                }
                Event::Resize(w, h) => {
                    println!("\r\nResized to {}x{}\r\n", w, h);
                    std::io::stdout().flush()?;
                }
                Event::FocusGained => {
                    println!("\r\nFocus gained\r\n");
                    std::io::stdout().flush()?;
                }
                Event::FocusLost => {
                    println!("\r\nFocus lost\r\n");
                    std::io::stdout().flush()?;
                }
                Event::Paste(data) => {
                    println!("\r\nPaste: {}\r\n", data);
                    std::io::stdout().flush()?;
                }
            }
        }
        Ok(())
    })();

    // Make sure we clean up the keyboard flags before exiting
    execute!(io::stdout(), PopKeyboardEnhancementFlags)?;
    terminal::disable_raw_mode()?;
    result
}
