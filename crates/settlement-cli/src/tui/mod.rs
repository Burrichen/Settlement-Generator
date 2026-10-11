//! The terminal interface: a stack of screens over the shared core.
//!
//! Every screen reads and writes through [`AppService`], the same service the
//! GUI uses, so settings, content customisations and saved settlements are
//! shared between the two interfaces.

mod screens;
pub mod theme;
pub mod widgets;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};
use settlement_core::AppInfo;
use settlement_core::dice::{FaceSource, RandomFaces};
use settlement_core::service::AppService;
use settlement_core::settings::Settings;

use theme::Styles;
use widgets::hint_line;

/// Smallest usable window; below this a notice is shown instead.
pub const MIN_WIDTH: u16 = 48;
pub const MIN_HEIGHT: u16 = 14;

/// Shared state every screen can use.
pub struct Ctx {
    pub service: AppService,
    pub settings: Settings,
    pub styles: Styles,
    pub info: AppInfo,
    /// Physical dice: real randomness in normal use, seeded in tests.
    pub dice: Box<dyn FaceSource>,
    /// A short message shown in the footer until the next key.
    pub flash: Option<String>,
}

impl Ctx {
    pub fn new(service: AppService, dice: Box<dyn FaceSource>) -> Self {
        let (settings, flash) = match service.settings() {
            Ok(settings) => (settings, None),
            Err(e) => (Settings::default(), Some(e)),
        };
        Ctx {
            styles: Styles::for_theme(settings.theme),
            settings,
            service,
            info: settlement_core::app_info(),
            dice,
            flash,
        }
    }

    /// Saves settings through the core and applies the theme.
    pub fn save_settings(&mut self, next: Settings) -> Result<(), String> {
        let stored = self.service.change_settings(&self.settings, &next)?;
        self.styles = Styles::for_theme(stored.theme);
        self.settings = stored;
        Ok(())
    }
}

/// What a screen asks the app to do after handling a key.
pub enum Nav {
    Stay,
    Push(Box<dyn Screen>),
    Pop,
    /// Back to the Main Menu.
    Home,
    Quit,
}

pub trait Screen {
    /// Shown in the header.
    fn title(&self) -> String;
    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx);
    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav;
    /// Key hints for the footer.
    fn hints(&self) -> Vec<(&'static str, &'static str)>;
    /// Called when the screen becomes visible again (refresh data here).
    fn resume(&mut self, _ctx: &mut Ctx) {}
}

pub struct App {
    pub ctx: Ctx,
    stack: Vec<Box<dyn Screen>>,
    pub quit: bool,
    /// Set while the last frame was too small to show the screen. Keys are
    /// then ignored, so nothing happens that the user cannot see.
    too_small: bool,
}

impl App {
    pub fn new(ctx: Ctx) -> Self {
        App {
            ctx,
            stack: vec![Box::new(screens::menu::MainMenu::default())],
            quit: false,
            too_small: false,
        }
    }

    pub fn screen_title(&self) -> String {
        self.stack.last().map(|s| s.title()).unwrap_or_default()
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return; // Windows also reports releases.
        }
        if self.too_small {
            return;
        }
        self.ctx.flash = None;
        let Some(screen) = self.stack.last_mut() else {
            self.quit = true;
            return;
        };
        match screen.handle(key, &mut self.ctx) {
            Nav::Stay => {}
            Nav::Push(next) => self.stack.push(next),
            Nav::Pop => {
                self.stack.pop();
                if self.stack.is_empty() {
                    self.quit = true;
                } else if let Some(top) = self.stack.last_mut() {
                    top.resume(&mut self.ctx);
                }
            }
            Nav::Home => {
                self.stack.truncate(1);
                if let Some(top) = self.stack.last_mut() {
                    top.resume(&mut self.ctx);
                }
            }
            Nav::Quit => self.quit = true,
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        let styles = self.ctx.styles;
        self.too_small = area.width < MIN_WIDTH || area.height < MIN_HEIGHT;
        if self.too_small {
            let lines = vec![
                Line::styled("Window too small", styles.title()),
                Line::raw(format!(
                    "Needs at least {MIN_WIDTH}×{MIN_HEIGHT}; now {}×{}.",
                    area.width, area.height
                )),
                Line::styled(
                    "Enlarge the terminal to continue (keys are paused), or press Ctrl+C to quit.",
                    styles.dim(),
                ),
            ];
            f.render_widget(
                Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }),
                area,
            );
            return;
        }

        let [header, rule_top, body, rule_bottom, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        let title = self.screen_title();
        let left = Line::from(vec![
            Span::styled(self.ctx.info.name.clone(), styles.title()),
            Span::styled(if title.is_empty() { "" } else { "  ›  " }, styles.dim()),
            Span::raw(title),
        ]);
        f.render_widget(Paragraph::new(left), header);
        f.render_widget(
            Paragraph::new(Line::styled(self.ctx.info.display_version(), styles.dim()))
                .alignment(ratatui::layout::Alignment::Right),
            header,
        );
        f.render_widget(Paragraph::new(widgets::rule(area.width, &styles)), rule_top);
        f.render_widget(
            Paragraph::new(widgets::rule(area.width, &styles)),
            rule_bottom,
        );

        let inner = Rect::new(
            body.x + 1,
            body.y,
            body.width.saturating_sub(2),
            body.height,
        );
        let hints = if let Some(screen) = self.stack.last_mut() {
            screen.render(f, inner, &self.ctx);
            screen.hints()
        } else {
            Vec::new()
        };
        let footer_line = match &self.ctx.flash {
            Some(message) => Line::styled(message.clone(), styles.good()),
            None => hint_line(&hints, &styles),
        };
        f.render_widget(Paragraph::new(footer_line), footer);
    }
}

/// Runs the full-screen interface until the user exits.
pub fn run() -> anyhow::Result<()> {
    let service = AppService::open_default().map_err(anyhow::Error::msg)?;
    let mut app = App::new(Ctx::new(service, Box::new(RandomFaces::from_entropy())));
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> anyhow::Result<()> {
    while !app.quit {
        terminal.draw(|f| app.draw(f))?;
        match event::read()? {
            Event::Key(key) => {
                // Ctrl+C always quits, even when the window is too small.
                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(event::KeyModifiers::CONTROL)
                {
                    break;
                }
                app.handle_key(key);
            }
            // Resizes redraw on the next loop.
            Event::Resize(..) => {}
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
