mod config;
mod install;
mod installs;
mod process;
mod updates;

use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::{DefaultTerminal, Frame};

use config::{Config, State};
use install::{Cancel, Job, Progress};
use installs::Install;
use process::Flight;
use updates::{Found, Offer, OfferKind};

const TICK: Duration = Duration::from_millis(250);
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

enum Mode {
    Menu,
    Flying(Flight),
}

/// One line of the menu. Only installs and offers can be selected.
#[derive(Clone)]
enum Row {
    Installed(Install),
    Offer(Offer),
    Heading(&'static str),
    Note(String),
}

impl Row {
    fn selectable(&self) -> bool {
        matches!(self, Row::Installed(_) | Row::Offer(_))
    }

    fn name(&self) -> Option<&str> {
        match self {
            Row::Installed(i) => Some(&i.name),
            Row::Offer(o) => Some(&o.name),
            _ => None,
        }
    }
}

/// An install or removal running in the background. One at a time.
struct Task {
    what: String,
    step: String,
    bytes: Option<(u64, u64)>,
    progress: Receiver<Progress>,
    cancel: Cancel,
}

/// A question waiting for y/n.
enum Confirm {
    StopFlightGear,
    Remove(Install),
    CancelTaskAndQuit,
}

struct App {
    cfg: Config,
    state: State,
    installs: Vec<Install>,
    rows: Vec<Row>,
    list: ListState,
    mode: Mode,
    /// Last update check, possibly from the cache.
    found: Option<Found>,
    /// A background update check in progress.
    checking: Option<Receiver<Found>>,
    spinner: usize,
    task: Option<Task>,
    /// Last outcome to show under the menu, and whether it is an error.
    message: Option<(String, bool)>,
    confirm: Option<Confirm>,
    log_warning: bool,
    /// Quit once the cancelled task has cleaned up.
    quit_after_task: bool,
    quit: bool,
}

impl App {
    fn new(cfg: Config) -> App {
        let state = State::load();
        let found = Found::load_cached();
        let mut app = App {
            cfg,
            state,
            installs: Vec::new(),
            rows: Vec::new(),
            list: ListState::default(),
            mode: Mode::Menu,
            found,
            checking: None,
            spinner: 0,
            task: None,
            message: None,
            confirm: None,
            log_warning: false,
            quit_after_task: false,
            quit: false,
        };
        app.rescan();
        if let Some(last) = app.state.last_flown.clone() {
            app.select_name(&last);
        }
        if app.found.as_ref().is_none_or(Found::is_stale) {
            app.start_check();
        }
        app
    }

    /// Re-read installs and rebuild the rows, keeping the same item selected if it still exists.
    fn rescan(&mut self) {
        let selected = self.selected().and_then(|r| r.name().map(str::to_string));
        self.installs = installs::scan(&self.cfg.installs_dir());

        let mut rows: Vec<Row> = self.installs.iter().cloned().map(Row::Installed).collect();
        if rows.is_empty() {
            rows.push(Row::Note("Nothing installed yet.".into()));
        }
        let offers = self
            .found
            .as_ref()
            .map(|f| f.offers_for(&self.installs))
            .unwrap_or_default();
        if !offers.is_empty() {
            rows.push(Row::Heading("available"));
            rows.extend(offers.into_iter().map(Row::Offer));
        }
        self.rows = rows;

        self.list.select(None);
        if let Some(name) = selected {
            self.select_name(&name);
        }
        if self.list.selected().is_none() {
            self.list.select(self.rows.iter().position(Row::selectable));
        }
    }

    fn select_name(&mut self, name: &str) {
        if let Some(i) = self.rows.iter().position(|r| r.name() == Some(name)) {
            self.list.select(Some(i));
        }
    }

    fn selected(&self) -> Option<&Row> {
        self.list.selected().and_then(|i| self.rows.get(i))
    }

    /// Move to the next selectable row up (-1) or down (+1), skipping headings.
    fn step(&mut self, delta: isize) {
        let Some(mut i) = self.list.selected() else {
            return;
        };
        loop {
            match i.checked_add_signed(delta) {
                Some(next) if next < self.rows.len() => i = next,
                _ => return,
            }
            if self.rows[i].selectable() {
                self.list.select(Some(i));
                return;
            }
        }
    }

    fn start_check(&mut self) {
        if self.checking.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let previous = self.found.clone();
        thread::spawn(move || {
            let _ = tx.send(updates::check(previous.as_ref()));
        });
        self.checking = Some(rx);
    }

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        if let Some(confirm) = self.confirm.take() {
            if key.code == KeyCode::Char('y') {
                self.confirmed(confirm);
            }
            return;
        }
        let quit_key = matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) || ctrl_c;
        match &self.mode {
            Mode::Menu => match key.code {
                _ if quit_key && self.task.is_some() => {
                    self.confirm = Some(Confirm::CancelTaskAndQuit);
                }
                _ if quit_key => self.quit = true,
                KeyCode::Up | KeyCode::Char('k') => self.step(-1),
                KeyCode::Down | KeyCode::Char('j') => self.step(1),
                KeyCode::Char('v') => {
                    self.state.vr = !self.state.vr;
                    self.state.save();
                }
                KeyCode::Char('r') => self.start_check(),
                KeyCode::Char('d') => {
                    if let Some(Row::Installed(install)) = self.selected().cloned()
                        && self.task_slot_free()
                    {
                        self.confirm = Some(Confirm::Remove(install));
                    }
                }
                KeyCode::Enter => match self.selected().cloned() {
                    Some(Row::Installed(install)) => self.launch(install),
                    Some(Row::Offer(offer)) => self.start_install(offer),
                    _ => {}
                },
                _ => {}
            },
            Mode::Flying(_) => {
                if key.code == KeyCode::Char('s') || ctrl_c {
                    self.confirm = Some(Confirm::StopFlightGear);
                }
            }
        }
    }

    fn confirmed(&mut self, confirm: Confirm) {
        match confirm {
            Confirm::StopFlightGear => {
                if let Mode::Flying(flight) = &mut self.mode {
                    flight.stop_requested = true;
                    process::signal_group(&flight.child, libc::SIGTERM);
                }
            }
            Confirm::Remove(install) => {
                if process::flightgear_running() {
                    self.message =
                        Some(("Close FlightGear before removing an install.".into(), true));
                    return;
                }
                let what = format!("Removing {}", install.name);
                self.start_task(what, move |job| install::remove(job, install));
            }
            Confirm::CancelTaskAndQuit => {
                if let Some(task) = &mut self.task {
                    task.cancel.cancel();
                    task.step = "cancelling and cleaning up".into();
                    task.bytes = None;
                    self.quit_after_task = true;
                } else {
                    self.quit = true;
                }
            }
        }
    }

    /// Only one install or removal at a time; says so if one is running.
    fn task_slot_free(&mut self) -> bool {
        if let Some(task) = &self.task {
            self.message = Some((format!("Wait for: {}", task.what), true));
            return false;
        }
        true
    }

    fn start_install(&mut self, offer: Offer) {
        if !self.task_slot_free() {
            return;
        }
        let what = format!("Installing {}", offer.name);
        self.start_task(what, move |job| install::install(job, offer));
    }

    fn start_task(&mut self, what: String, work: impl FnOnce(Job) + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        let job = Job::new(&self.cfg, &self.installs, self.state.last_flown.clone(), tx);
        let cancel = job.cancel.clone();
        thread::spawn(move || work(job));
        self.message = None;
        self.task = Some(Task {
            what,
            step: "starting".into(),
            bytes: None,
            progress: rx,
            cancel,
        });
    }

    fn launch(&mut self, install: Install) {
        if process::flightgear_running() {
            self.message = Some((
                "FlightGear is already running; close it first.".into(),
                true,
            ));
            return;
        }
        // Problems with the Stream Deck are shown, but don't stop the flight.
        let mut deck_warning = None;
        let opendeck = match &self.cfg.opendeck {
            Some(path) if !process::opendeck_running() => {
                if let Some(profile) = &self.cfg.opendeck_profile
                    && let Err(err) =
                        process::select_opendeck_profile(&self.cfg.opendeck_config, profile)
                {
                    deck_warning = Some(format!("Stream Deck page: {err}"));
                }
                match process::start_opendeck(path) {
                    Ok(child) => Some(child),
                    Err(err) => {
                        deck_warning = Some(err);
                        None
                    }
                }
            }
            _ => None,
        };
        match process::start_flightgear(&self.cfg, &install, self.state.vr) {
            Ok(child) => {
                self.state.last_flown = Some(install.name.clone());
                self.state.save();
                self.message = deck_warning.map(|w| (w, true));
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

    /// Apply progress from the running install or removal.
    fn poll_task(&mut self) {
        let Some(task) = &mut self.task else {
            return;
        };
        loop {
            match task.progress.try_recv() {
                Ok(Progress::Step(step)) => {
                    if !self.quit_after_task {
                        task.step = step;
                        task.bytes = None;
                    }
                }
                Ok(Progress::Bytes(done, total)) => task.bytes = Some((done, total)),
                Ok(Progress::Done(result)) => {
                    self.message = Some(match result {
                        Ok(text) => (text, false),
                        Err(err) => (format!("{} failed: {err}", task.what), true),
                    });
                    break;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.message = Some((format!("{} stopped unexpectedly.", task.what), true));
                    break;
                }
            }
        }
        self.task = None;
        if self.quit_after_task {
            self.quit = true;
        }
        self.rescan();
    }

    /// Called every tick: collect update results, notice FlightGear exiting, watch its log.
    fn on_tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
        if let Some(rx) = &self.checking {
            match rx.try_recv() {
                Ok(found) => {
                    found.save();
                    self.found = Some(found);
                    self.checking = None;
                    self.rescan();
                }
                Err(TryRecvError::Disconnected) => self.checking = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        self.poll_task();

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
        if matches!(self.confirm, Some(Confirm::StopFlightGear)) {
            self.confirm = None;
        }
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
        Constraint::Length(5),
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
    let last_flown = app.state.last_flown.as_deref();
    let items: Vec<ListItem> = app
        .rows
        .iter()
        .map(|row| ListItem::new(row_line(row, last_flown)))
        .collect();
    let list = List::new(items)
        .block(Block::new().borders(Borders::TOP).title(" installed "))
        // An ASCII marker, with its space kept on every row: some terminals draw
        // symbols like ▶ two cells wide, which shifted the selected row.
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut app.list);
}

fn row_line<'a>(row: &'a Row, last_flown: Option<&str>) -> Line<'a> {
    let kind = |label: &str| Span::styled(format!("{label:<9}"), Style::new().fg(Color::Cyan));
    match row {
        Row::Installed(install) => {
            let mut spans = vec![
                Span::raw(format!("{:<16}", install.name)),
                kind(install.kind.label()),
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
            if last_flown == Some(install.name.as_str()) {
                spans.push(Span::styled("last flown", Style::new().fg(Color::Yellow)));
            }
            Line::from(spans)
        }
        Row::Offer(offer) => {
            let label = match offer.kind {
                OfferKind::Stable { .. } => "stable",
                OfferKind::Nightly { .. } => "nightly",
            };
            let mut spans = vec![
                Span::raw(format!("{:<16}", offer.name)),
                kind(label),
                Span::styled("install", Style::new().fg(Color::Green)),
            ];
            if let Some(size) = offer.size {
                spans.push(Span::styled(
                    format!("  {:.1} GB", size as f64 / 1e9),
                    Style::new().dim(),
                ));
            }
            if let OfferKind::Nightly { .. } = offer.kind {
                // The size is the AppImage alone; its data comes from the fgdata git repository.
                spans.push(Span::styled(" + fgdata", Style::new().dim()));
                spans.push(Span::styled(
                    "  experimental",
                    Style::new().fg(Color::Yellow),
                ));
            }
            Line::from(spans)
        }
        Row::Heading(title) => Line::styled(format!("-- {title} "), Style::new().dim()),
        Row::Note(text) => Line::styled(text.as_str(), Style::new().dim()),
    }
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
    if let Some(confirm) = &app.confirm {
        let question = match confirm {
            Confirm::StopFlightGear => "Stop FlightGear? y/n".to_string(),
            Confirm::Remove(install) => format!(
                "Remove {} with its settings and saved aircraft state? Shared files are kept. y/n",
                install.name
            ),
            Confirm::CancelTaskAndQuit => "Cancel the running task and quit? y/n".to_string(),
        };
        lines.push(Line::styled(
            question,
            Style::new().fg(Color::Yellow).bold(),
        ));
    }
    if let Some(task) = &app.task {
        lines.push(task_line(task, app.spinner));
    }
    if let Some((text, is_error)) = &app.message {
        let color = if *is_error { Color::Red } else { Color::Green };
        lines.push(Line::styled(text.as_str(), Style::new().fg(color)));
    }
    lines.push(Line::styled(update_status(app), Style::new().dim()));
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

/// "Installing 2024.1.8: downloading data [██████░░░░░░] 52%  1.0/1.9 GB"
fn task_line(task: &Task, spinner: usize) -> Line<'_> {
    let mut spans = vec![Span::styled(
        format!("{}: {}", task.what, task.step),
        Style::new().fg(Color::Cyan),
    )];
    match task.bytes {
        Some((done, total)) if total > 0 => {
            const WIDTH: usize = 20;
            let filled = (done as f64 / total as f64 * WIDTH as f64) as usize;
            spans.push(Span::raw(format!(
                " [{}{}] {:>3}%  {:.1}/{:.1} GB",
                "█".repeat(filled.min(WIDTH)),
                "░".repeat(WIDTH - filled.min(WIDTH)),
                done * 100 / total,
                done as f64 / 1e9,
                total as f64 / 1e9
            )));
        }
        Some((done, _)) => spans.push(Span::raw(format!(" {:.1} GB", done as f64 / 1e9))),
        None => spans.push(Span::raw(format!(" {}", SPINNER[spinner % SPINNER.len()]))),
    }
    Line::from(spans)
}

fn update_status(app: &App) -> String {
    if app.checking.is_some() {
        return format!(
            "{} checking for new versions...",
            SPINNER[app.spinner % SPINNER.len()]
        );
    }
    let Some(found) = &app.found else {
        return "Not checked for new versions yet.".into();
    };
    let when = local_time(found.checked_at);
    match found.errors.len() {
        0 => format!("checked {when}"),
        // Both sources failed; usually offline.
        2 => format!(
            "couldn't check for new versions ({}), showing results from {when}",
            found.errors[0].split(": ").last().unwrap_or("unreachable")
        ),
        _ => format!("checked {when}; couldn't reach {}", found.errors.join("; ")),
    }
}

/// "14:05" today, "Sep 30 14:05" otherwise.
fn local_time(unix: u64) -> String {
    let format = |secs: u64| -> Option<libc::tm> {
        let t = secs as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
        ok.then_some(tm)
    };
    let (Some(then), Some(today)) = (format(unix), format(updates::now())) else {
        return "?".into();
    };
    let time = format!("{:02}:{:02}", then.tm_hour, then.tm_min);
    if (then.tm_year, then.tm_yday) == (today.tm_year, today.tm_yday) {
        return time;
    }
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS.get(then.tm_mon as usize).unwrap_or(&"?");
    format!("{month} {} {time}", then.tm_mday)
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let enter = match app.selected() {
        Some(Row::Offer(_)) => "install",
        _ if app.state.vr => "launch in VR",
        _ => "launch",
    };
    let keys: &[(&str, &str)] = match app.mode {
        _ if app.confirm.is_some() => &[("y", "yes"), ("any other key", "no")],
        Mode::Menu if matches!(app.selected(), Some(Row::Installed(_))) => &[
            ("↑↓", "select"),
            ("⏎", enter),
            ("v", "VR on/off"),
            ("d", "remove"),
            ("r", "check now"),
            ("q", "quit"),
        ],
        Mode::Menu => &[
            ("↑↓", "select"),
            ("⏎", enter),
            ("v", "VR on/off"),
            ("r", "check now"),
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
