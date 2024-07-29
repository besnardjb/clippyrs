use std::clone;
use std::io::{self, Write};

use anyhow::Result;
use ollama::Ollama;
use std::io::stdout;
use termimad::crossterm::style::Color::*;
use termimad::crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode::*, KeyEvent},
    queue,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use termimad::*;
mod ollama;

fn user_prompt() {
    print!("\nUser: ");
    io::stdout().flush().unwrap();
}

fn view_area() -> Area {
    let mut area = Area::full_screen();
    area.pad_for_max_width(120); // we don't want a too wide text column
    area
}

// Stolen from scrollbar example ...
fn view_resp(skin: MadSkin, md: String) -> Result<(), Error> {
    let mut w = stdout(); // we could also have used stderr
    queue!(w, EnterAlternateScreen)?;
    terminal::enable_raw_mode()?;
    queue!(w, Hide)?; // hiding the cursor
    let mut view = MadView::from(md, view_area(), skin);
    loop {
        view.write_on(&mut w)?;
        w.flush()?;
        match event::read() {
            Ok(Event::Key(KeyEvent { code, .. })) => match code {
                Up => view.try_scroll_lines(-1),
                Down => view.try_scroll_lines(1),
                PageUp => view.try_scroll_pages(-1),
                PageDown => view.try_scroll_pages(1),
                Right => view.try_scroll_pages(1),
                Left => view.try_scroll_pages(-1),
                _ => break,
            },
            Ok(Event::Resize(..)) => {
                queue!(w, Clear(ClearType::All))?;
                view.resize(&view_area());
            }
            _ => {}
        }
    }
    terminal::disable_raw_mode()?;
    queue!(w, Show)?; // we must restore the cursor
    queue!(w, LeaveAlternateScreen)?;
    w.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let mut skin = MadSkin::default();
    skin.table.align = Alignment::Center;
    skin.set_headers_fg(AnsiValue(178));
    skin.scrollbar.thumb.set_fg(AnsiValue(178));
    skin.code_block.align = Alignment::Center;
    let ollama = Ollama::default().await?;

    let mut chat = ollama.context_new()?;

    user_prompt();

    for line in std::io::stdin().lines() {
        let mut line = line.unwrap();

        let domd = if line.starts_with('!') {
            line = line[1..].to_string();
            true
        } else {
            false
        };

        ollama.chat(line.as_str(), &mut chat).await?;

        if let Some(resp) = chat.response() {
            if domd {
                let _ = view_resp(skin.clone(), resp);
            }
        }

        user_prompt()
    }

    Ok(())
}
