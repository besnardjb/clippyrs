use anyhow::Result;
use copypasta_ext::prelude::*;
use copypasta_ext::x11_bin::ClipboardContext;
use log::{error, info};
use ollama::Ollama;
use std::io::stdout;
use std::io::{self, Write};
use termimad::crossterm::style::Color::*;
use termimad::crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode::*, KeyEvent},
    queue,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use termimad::*;
mod ollama;
use clap::Parser;
use colored::Colorize;

fn user_prompt() {
    print!("{}", "\nUser: ".bold().blue());
    io::stdout().flush().unwrap();
}

fn assistant_prompt() {
    print!("{}", "\nAssistant: ".bold().red());
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

fn prompt_unfold_vars(prompt: String) -> Result<String> {
    let mut ret = prompt;

    /* Input clipboard */
    if ret.contains("::CL::") {
        if let Ok(mut ctx) = ClipboardContext::new() {
            let clipboard_data = ctx.get_contents().unwrap_or("".to_string());
            ret = ret.replace("::CL::", &clipboard_data);
        }
    }

    Ok(ret)
}

fn store_in_clipboard(response: String) {
    if let Ok(mut ctx) = ClipboardContext::new() {
        if let Err(e) = ctx.set_contents(response) {
            error!("Failed to write content to clipboard: {}", e);
        } else {
            info!("Did set output data to clipboard");
        }
    } else {
        error!("No ctx");
    }
}

#[derive(Parser, Debug)]
struct Args {
    /// Model to be used
    #[arg(short, long)]
    model: Option<String>,
    /// Force markdown output
    #[arg(short, long, default_value_t = false)]
    force_md: bool,
    /// List available models
    #[clap(long, short, action)]
    list_models: bool,

    /// Store response to clipboard
    #[clap(long, short, action)]
    store_in_clipboard: bool,

    /// Optionnal Prompt
    #[clap(last = true, allow_hyphen_values = true)]
    prompt: Option<Vec<String>>,
}

async fn interactive(ollama: &Ollama, args: &Args, skin: &MadSkin) -> Result<()> {
    let mut chat = ollama.context_new()?;

    user_prompt();

    for line in std::io::stdin().lines() {
        let mut line = prompt_unfold_vars(line.unwrap())?;

        let domd = if line.starts_with('!') {
            line = line[1..].to_string();
            true
        } else {
            false
        };

        assistant_prompt();

        ollama.chat(line.as_str(), &mut chat).await?;

        if let Some(resp) = chat.response() {
            if domd || args.force_md {
                let _ = view_resp(skin.clone(), resp.clone());
            }

            if args.store_in_clipboard {
                store_in_clipboard(resp);
            }
        }

        user_prompt()
    }

    Ok(())
}

async fn single(
    ollama: &Ollama,
    prompt: String,
    args: &Args,
    skin: &MadSkin,
) -> Result<Option<String>> {
    let mut chat = ollama.context_new()?;
    let prompt = prompt_unfold_vars(prompt)?;
    ollama.chat(prompt.as_str(), &mut chat).await?;

    if let Some(response) = chat.response() {
        if args.force_md {
            let _ = view_resp(skin.clone(), response.clone());
        }

        if args.store_in_clipboard {
            store_in_clipboard(response);
        }
    }

    Ok(chat.response())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    env_logger::init();

    let mut skin = MadSkin::default();
    skin.table.align = Alignment::Center;
    skin.set_headers_fg(AnsiValue(178));
    skin.scrollbar.thumb.set_fg(AnsiValue(178));
    skin.code_block.align = Alignment::Center;

    let mut ollama = Ollama::default().await?;

    if args.list_models {
        ollama.print_models();
        return Ok(());
    }

    if let Some(model) = &args.model {
        ollama.set_model(model.as_str())?;
    }

    if let Some(prompt) = &args.prompt {
        let pr = prompt.join(" ");
        single(&ollama, pr, &args, &skin).await?;
        return Ok(());
    }

    interactive(&ollama, &args, &skin).await?;

    Ok(())
}
