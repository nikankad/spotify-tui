mod app;
mod auth;
mod banner;
mod cli;
mod config;
mod event;
mod handlers;
mod http;
mod media;
mod network;
mod paging;
mod spotify;
mod ui;
mod user_config;
mod yaml;

use crate::app::RouteId;
use crate::event::Key;
use crate::spotify::Spotify;
use anyhow::{Result, anyhow};
use app::{ActiveBlock, App};
use banner::BANNER;
use clap::{Arg, Command as ClapApp};
use clap_complete::Shell;
use config::ClientConfig;
use crossterm::{
  ExecutableCommand,
  cursor::MoveTo,
  event::{DisableMouseCapture, EnableMouseCapture},
  execute,
  terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
  },
};
use network::{IoEvent, Network};
use std::{
  cmp::{max, min},
  io::{self, stdout},
  panic::{self, PanicHookInfo},
  path::PathBuf,
  sync::Arc,
};
use tokio::sync::Mutex;
use tui::{
  Terminal,
  backend::{Backend, CrosstermBackend},
};
use user_config::{UserConfig, UserConfigPaths};

fn close_application() -> Result<()> {
  disable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
  Ok(())
}

fn panic_hook(info: &PanicHookInfo<'_>) {
  if crossterm::terminal::is_raw_mode_enabled().unwrap_or(false) {
    let _ = close_application();
  }
  eprintln!("{info}\n{}", std::backtrace::Backtrace::force_capture());
}

struct TerminalGuard;
impl Drop for TerminalGuard {
  fn drop(&mut self) {
    let _ = close_application();
  }
}

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
  panic::set_hook(Box::new(|info| {
    panic_hook(info);
  }));

  let mut clap_app = ClapApp::new(env!("CARGO_PKG_NAME"))
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about(env!("CARGO_PKG_DESCRIPTION"))
    .before_help(BANNER)
    .after_help("Your Spotify Client ID is stored in $HOME/.config/spotify-tui/client.yml")
    .arg(cli::development_mode_arg())
    .arg(
      Arg::new("tick-rate")
        .short('t')
        .long("tick-rate")
        .help("Set the tick rate (milliseconds): the lower the number the higher the FPS.")
        .long_help("Set the tick rate in milliseconds. Lower values use more CPU.")
        .num_args(1),
    )
    .arg(
      Arg::new("config")
        .short('c')
        .long("config")
        .help("Specify configuration file path.")
        .num_args(1),
    )
    .arg(
      Arg::new("completions")
        .long("completions")
        .help("Generates completions for your preferred shell")
        .num_args(1)
        .value_parser(["bash", "zsh", "fish", "power-shell", "elvish"])
        .value_name("SHELL"),
    )
    // Control spotify from the command line
    .subcommand(cli::playback_subcommand())
    .subcommand(cli::play_subcommand())
    .subcommand(cli::list_subcommand())
    .subcommand(cli::search_subcommand())
    .subcommands(cli::media::commands());

  let matches = clap_app.clone().get_matches();

  // Shell completions don't need any spotify work
  if let Some(s) = matches.get_one::<String>("completions").map(String::as_str) {
    let shell = match s {
      "fish" => Shell::Fish,
      "bash" => Shell::Bash,
      "zsh" => Shell::Zsh,
      "power-shell" => Shell::PowerShell,
      "elvish" => Shell::Elvish,
      _ => return Err(anyhow!("no completions avaible for '{}'", s)),
    };
    clap_complete::generate(shell, &mut clap_app, "spt", &mut io::stdout());
    return Ok(());
  }

  let development = matches.get_flag("dev");
  let mut user_config = UserConfig::new();
  if let Some(config_file_path) = matches.get_one::<String>("config").map(String::as_str) {
    let config_file_path = PathBuf::from(config_file_path);
    let path = UserConfigPaths { config_file_path };
    user_config.path_to_config.replace(path);
  }
  if !development || user_config.path_to_config.is_some() {
    user_config.load_config()?;
  }

  if let Some(tick_rate) = matches
    .get_one::<String>("tick-rate")
    .map(String::as_str)
    .and_then(|tick_rate| tick_rate.parse().ok())
  {
    if !(1..1000).contains(&tick_rate) {
      return Err(anyhow!("Tick rate must be between 1 and 999 milliseconds"));
    } else {
      user_config.behavior.tick_rate_milliseconds = tick_rate;
    }
  }

  let mut client_config = ClientConfig::new();
  let spotify = if development {
    Spotify::development()
  } else {
    client_config.load_config()?;
    Spotify::new(auth::Auth::authenticate(&client_config).await?)?
  };
  if let Some((name @ ("queue" | "episodes" | "audiobooks"), matches)) = matches.subcommand() {
    println!(
      "{}",
      cli::media::handle(name, matches, &spotify, &client_config).await?
    );
    return Ok(());
  }
  let (sync_io_tx, mut sync_io_rx) = tokio::sync::mpsc::channel::<IoEvent>(64);
  let mut initial_app = App::new(sync_io_tx, user_config.clone());
  initial_app.development = development;
  let app = Arc::new(Mutex::new(initial_app));
  if let Some((cmd, matches)) = matches.subcommand() {
    let network = Network::new(spotify, client_config, &app);
    println!(
      "{}",
      cli::handle_matches(matches, cmd.to_string(), network, user_config).await?
    );
  } else {
    if development {
      Network::new(spotify.clone(), client_config.clone(), &app)
        .populate_development()
        .await;
    }
    let worker_app = Arc::clone(&app);
    let worker = tokio::spawn(async move {
      let mut network = Network::new(spotify, client_config, &worker_app);
      while let Some(event) = sync_io_rx.recv().await {
        network.handle_network_event(event).await;
      }
    });
    let result = start_ui(user_config, &app).await;
    worker.abort();
    result?;
  }

  Ok(())
}

async fn start_ui(user_config: UserConfig, shared_app: &Arc<Mutex<App>>) -> Result<()> {
  // Terminal initialization
  let mut stdout = stdout();
  execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
  let _terminal_guard = TerminalGuard;
  enable_raw_mode()?;

  let mut backend = CrosstermBackend::new(stdout);

  if user_config.behavior.set_window_title {
    backend.execute(SetTitle("spt - Spotify TUI"))?;
  }

  let mut terminal = Terminal::new(backend)?;
  terminal.hide_cursor()?;

  let events = event::Events::new(user_config.behavior.tick_rate_milliseconds);

  // play music on, if not send them to the device selection view

  let mut is_first_render = true;

  loop {
    let mut app = shared_app.lock().await;
    // Get the size of the screen on each loop to account for resize event
    if let Ok(size) = terminal.backend().size() {
      // Reset the help menu is the terminal was resized
      if is_first_render || app.size != size.into() {
        app.help_menu_max_lines = 0;
        app.help_menu_offset = 0;
        app.help_menu_page = 0;

        app.size = size.into();

        // Based on the size of the terminal, adjust the search limit.
        let potential_limit = max((app.size.height as i32) - 13, 0) as u32;
        let max_limit = potential_limit.clamp(1, 50);
        let large_search_limit = min((f32::from(size.height) / 1.4) as u32, max_limit).max(1);
        let small_search_limit =
          min((f32::from(size.height) / 2.85) as u32, max_limit / 2).clamp(1, 10);

        app.dispatch(IoEvent::UpdateSearchLimits(
          large_search_limit,
          small_search_limit,
        ));

        // Based on the size of the terminal, adjust how many lines are
        // displayed in the help menu
        if app.size.height > 8 {
          app.help_menu_max_lines = (app.size.height as u32) - 8;
        } else {
          app.help_menu_max_lines = 0;
        }
      }
    };

    let current_route = app.get_current_route();
    terminal.draw(|f| match current_route.active_block {
      ActiveBlock::HelpMenu => {
        ui::draw_help_menu(f, &app);
      }
      ActiveBlock::Error => {
        ui::draw_error_screen(f, &app);
      }
      ActiveBlock::SelectDevice => {
        ui::draw_device_list(f, &app);
      }

      ActiveBlock::BasicView => {
        ui::draw_basic_view(f, &app);
      }
      _ => {
        ui::draw_main_layout(f, &app);
      }
    })?;

    if current_route.active_block == ActiveBlock::Input {
      terminal.show_cursor()?;
    } else {
      terminal.hide_cursor()?;
    }

    let cursor_offset = if app.size.height > ui::util::SMALL_TERMINAL_HEIGHT {
      2
    } else {
      1
    };

    // Put the cursor back inside the input box
    terminal.backend_mut().execute(MoveTo(
      cursor_offset + app.input_cursor_position,
      cursor_offset,
    ))?;

    // Waiting for terminal input must not block network updates to App.
    drop(app);
    let event = events.next()?;
    let mut app = shared_app.lock().await;
    match event {
      event::Event::Input(key) => {
        if key == Key::Ctrl('c') {
          break;
        }

        let current_active_block = app.get_current_route().active_block;

        // To avoid swallowing the global key presses `q` and `-` make a special
        // case for the input handler
        if current_active_block == ActiveBlock::Input {
          handlers::input_handler(key, &mut app);
        } else if key == app.user_config.keys.back {
          if app.get_current_route().active_block != ActiveBlock::Input {
            // Go back through navigation stack when not in search input mode and exit the app if there are no more places to back to

            let pop_result = match app.pop_navigation_stack() {
              Some(ref x) if x.id == RouteId::Search => app.pop_navigation_stack(),
              Some(x) => Some(x),
              None => None,
            };
            if pop_result.is_none() {
              break; // Exit application
            }
          }
        } else {
          handlers::handle_app(key, &mut app);
        }
      }
      event::Event::Tick => {
        app.update_on_tick();
      }
    }

    // Delay spotify request until first render, will have the effect of improving
    // startup speed
    if is_first_render {
      app.dispatch(IoEvent::GetPlaylists);
      app.dispatch(IoEvent::GetUser);
      app.dispatch(IoEvent::GetCurrentPlayback);
      app.help_docs_size = ui::help::get_help_docs(&app.user_config.keys).len() as u32;

      is_first_render = false;
    }
  }

  terminal.show_cursor()?;
  close_application()?;

  Ok(())
}
