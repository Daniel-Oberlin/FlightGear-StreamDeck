mod config;
mod installs;
mod process;

use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use config::{Config, State};
use installs::Install;
use process::Flight;

const TICK: Duration = Duration::from_millis(250);

enum Mode {
    Menu,
    Flying(Flight),
}

struct App {
    cfg: Config,
    state: State,
    installs: Vec<Install>,
    list: ListState,
    mode: Mode,
    /// Last outcome to show under the menu, and whether it is an error.
    message: Option<(String, bool)>,
    /// Asked "stop FlightGear?" and waiting for y/n.
    confirm_stop: bool,
    log_warning: bool,
    quit: bool,
}

impl App {
    fn new(cfg: Config) -> App {
        let state = State::load();
        let mut app = App {
            cfg,
            state,
            installs: Vec::new(),
            list: ListState::default(),
            mode: Mode::Menu,
            message: None,
            confirm_stop: false,
            log_warning: false,
            quit: false,
        };
        app.rescan();
        let last = app.state.last_flown.clone();
        let index = last.and_then(|name| app.installs.iter().position(|i| i.name == name));
        app.list.select(index.or(if app.installs.is_empty() {
            None
        } else {
            Some(0)
        }));
        app
    }

    fn rescan(&mut self) {
        self.installs = installs::scan(&self.cfg.installs_dir());
        if let Some(i) = self.list.selected()
            && i >= self.installs.len()
        {
            self.list.select(self.installs.len().checked_sub(1));
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        match &self.mode {
            Mode::Menu => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
                _ if ctrl_c => self.quit = true,
                KeyCode::Up | KeyCode::Char('k') => self.list.select_previous(),
                KeyCode::Down | KeyCode::Char('j')
                    if self
                        .list
                        .selected()
                        .is_some_and(|i| i + 1 < self.installs.len()) =>
                {
                    self.list.select_next();
                }
                KeyCode::Char('v') => {
                    self.state.vr = !self.state.vr;
                    self.state.save();
                }
                KeyCode::Enter => self.launch(),
                _ => {}
            },
            Mode::Flying(_) if self.confirm_stop => match key.code {
                KeyCode::Char('y') => {
                    self.confirm_stop = false;
                    if let Mode::Flying(flight) = &mut self.mode {
                        flight.stop_requested = true;
                        process::signal_group(&flight.child, libc::SIGTERM);
                    }
                }
                _ => self.confirm_stop = false,
            },
            Mode::Flying(_) => {
                if key.code == KeyCode::Char('s') || ctrl_c {
                    self.confirm_stop = true;
                }
            }
        }
    }

    fn launch(&mut self) {
        let Some(install) = self
            .list
            .selected()
            .and_then(|i| self.installs.get(i))
            .cloned()
        else {
            return;
        };
        if process::flightgear_running() {
            self.message = Some((
                "FlightGear is already running; close it first.".into(),
                true,
            ));
            return;
        }
        let opendeck = match &self.cfg.opendeck {
            Some(path) if !process::opendeck_running() => match process::start_opendeck(path) {
                Ok(child) => Some(child),
                Err(err) => {
                    self.message = Some((err, true));
                    None
                }
            },
            _ => None,
        };
        match process::start_flightgear(&self.cfg, &install, self.state.vr) {
            Ok(child) => {
                self.state.last_flown = Some(install.name.clone());
                self.state.save();
                self.message = None;
                self.log_warning = false;
                self.mode = Mode::Flying(Flight {
                    child,
                    install: install.name.clone(),
                    vr: self.state.vr,
                    started: Instant::now(),
                    log: install.home().join("fgfs.log"),
                    console_log: install.home().join("launch.log"),
                    opendeck,
                    stop_requested: false,
                });
            }
            Err(err) => {
                if let Some(child) = opendeck {
                    process::stop_group_in_background(child);
                }
                self.message = Some((err, true));
            }
        }
    }

    /// Called every tick: notice FlightGear exiting and watch its log size.
    fn on_tick(&mut self) {
        let Mode::Flying(flight) = &mut self.mode else {
            return;
        };
        let limit = self.cfg.log_limit_mb * 1024 * 1024;
        if !self.log_warning && fs::metadata(&flight.log).is_ok_and(|m| m.len() > limit) {
            self.log_warning = true;
        }
        let status = match flight.child.try_wait() {
            Ok(Some(status)) => status,
            Ok(None) => return,
            Err(err) => {
                self.message = Some((format!("lost track of FlightGear: {err}"), true));
                return;
            }
        };
        let Mode::Flying(mut flight) = std::mem::replace(&mut self.mode, Mode::Menu) else {
            unreachable!()
        };
        if let Some(opendeck) = flight.opendeck.take() {
            process::stop_group_in_background(opendeck);
        }
        self.confirm_stop = false;
        self.log_warning = false;
        self.message = Some(exit_message(&flight, status));
        self.rescan();
    }
}

fn exit_message(flight: &Flight, status: ExitStatus) -> (String, bool) {
    let minutes = flight.started.elapsed().as_secs() / 60;
    let ran = format!("{} ran {minutes} min", flight.install);
    if status.success() {
        return (format!("{ran} and exited normally."), false);
    }
    if flight.stop_requested {
        return (format!("{ran} and was stopped from the menu."), false);
    }
    let how = match (status.code(), status.signal()) {
        (Some(code), _) => format!("exited with code {code}"),
        (None, Some(signal)) => format!("was stopped by signal {signal}"),
        _ => "exited abnormally".to_string(),
    };
    (
        format!(
            "{ran} and {how}. Logs: {} and {}",
            flight.log.display(),
            flight.console_log.display()
        ),
        true,
    )
}

fn draw(frame: &mut Frame, app: &mut App) {
    let [header, body, status, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);
    match &app.mode {
        Mode::Menu => draw_menu(frame, body, app),
        Mode::Flying(flight) => draw_flying(frame, body, flight),
    }
    draw_status(frame, status, app);
    draw_footer(frame, footer, app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let vr = if app.state.vr {
        Span::styled(
            " VR ● ON ",
            Style::new().fg(Color::Black).bg(Color::Green).bold(),
        )
    } else {
        Span::styled(
            " VR ○ OFF ",
            Style::new().fg(Color::Gray).bg(Color::DarkGray),
        )
    };
    let [title, badge] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(vr.width() as u16)]).areas(area);
    frame.render_widget(Line::from(" FlightGear").bold(), title);
    frame.render_widget(Line::from(vr), badge);
}

fn draw_menu(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::new().borders(Borders::TOP).title(" installed ");
    if app.installs.is_empty() {
        let text = format!(
            "Nothing installed yet.\nInstalls are looked for in {}",
            app.cfg.installs_dir().display()
        );
        frame.render_widget(Paragraph::new(text).block(block).dim(), area);
        return;
    }
    let items: Vec<ListItem> = app
        .installs
        .iter()
        .map(|install| {
            let mut spans = vec![
                Span::raw(format!("{:<16}", install.name)),
                Span::styled(
                    format!("{:<9}", install.kind.label()),
                    Style::new().fg(Color::Cyan),
                ),
            ];
            if let Some(version) = install
                .data_version
                .as_ref()
                .filter(|v| **v != install.name)
            {
                spans.push(Span::styled(
                    format!("data {version}  "),
                    Style::new().dim(),
                ));
            }
            if app.state.last_flown.as_deref() == Some(install.name.as_str()) {
                spans.push(Span::styled("last flown", Style::new().fg(Color::Yellow)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_symbol("▶ ")
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut app.list);
}

fn draw_flying(frame: &mut Frame, area: Rect, flight: &Flight) {
    let elapsed = flight.started.elapsed().as_secs();
    let mode = if flight.vr { " in VR" } else { "" };
    let lines = vec![
        Line::from(vec![
            Span::raw("Flying "),
            Span::styled(&flight.install, Style::new().bold()),
            Span::raw(format!(
                "{mode}   {:02}:{:02}:{:02}",
                elapsed / 3600,
                elapsed / 60 % 60,
                elapsed % 60
            )),
        ]),
        Line::from(""),
        Line::from(format!("Log: {}", flight.log.display())).dim(),
        Line::from("The menu comes back when FlightGear exits.").dim(),
    ];
    let block = Block::new().borders(Borders::TOP).title(" running ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    if app.log_warning {
        lines.push(Line::styled(
            format!(
                "fgfs.log is over {} MB - FlightGear may be stuck in a crash loop. Press s to stop it.",
                app.cfg.log_limit_mb
            ),
            Style::new().fg(Color::Red).bold(),
        ));
    }
    if app.confirm_stop {
        lines.push(Line::styled(
            "Stop FlightGear? y/n",
            Style::new().fg(Color::Yellow).bold(),
        ));
    }
    if let Some((text, is_error)) = &app.message {
        let color = if *is_error { Color::Red } else { Color::Green };
        lines.push(Line::styled(text.as_str(), Style::new().fg(color)));
    }
    let deck = match (&app.mode, &app.cfg.opendeck) {
        (
            Mode::Flying(Flight {
                opendeck: Some(_), ..
            }),
            _,
        ) => "Stream Deck: started for this flight",
        (_, None) => "Stream Deck: OpenDeck AppImage not found",
        _ if process::opendeck_running() => "Stream Deck: running",
        _ => "Stream Deck: starts with FlightGear",
    };
    lines.push(Line::styled(deck, Style::new().dim()));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let launch = if app.state.vr {
        "launch in VR"
    } else {
        "launch"
    };
    let keys: &[(&str, &str)] = match app.mode {
        Mode::Menu => &[
            ("↑↓", "select"),
            ("⏎", launch),
            ("v", "VR on/off"),
            ("q", "quit"),
        ],
        Mode::Flying(_) => &[("s", "stop FlightGear")],
    };
    let spans: Vec<Span> = keys
        .iter()
        .flat_map(|(key, what)| {
            [
                Span::styled(format!(" {key} "), Style::new().reversed()),
                Span::raw(format!(" {what}  ")),
            ]
        })
        .collect();
    frame.render_widget(Line::from(spans), area);
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    let mut last_tick = Instant::now();
    while !app.quit {
        terminal.draw(|frame| draw(frame, app))?;
        let timeout = TICK.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }
        if last_tick.elapsed() >= TICK {
            app.on_tick();
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn main() {
    let cfg = match Config::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("fglaunch: {err}");
            std::process::exit(2);
        }
    };
    let mut app = App::new(cfg);
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    if let Err(err) = result {
        eprintln!("fglaunch: {err}");
        std::process::exit(1);
    }
}
