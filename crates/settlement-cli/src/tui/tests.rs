//! End-to-end tests of the terminal interface: real screens and the real
//! core over an in-memory database, driven by key presses and rendered to a
//! test backend.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Modifier;
use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::ids::{ResultId, TagId};
use settlement_core::service::AppService;
use settlement_core::settings::{GenerationMethod, GenerationPresentation};

use super::{App, Ctx, MIN_HEIGHT, MIN_WIDTH};

fn app() -> App {
    let service = AppService::new(Database::open_in_memory().unwrap());
    App::new(Ctx::new(service, Box::new(RandomFaces::seeded(7))))
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn keys(app: &mut App, codes: &[KeyCode]) {
    for &c in codes {
        key(app, c);
    }
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        key(app, KeyCode::Char(c));
    }
}

fn render(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| app.draw(f)).unwrap();
    terminal
}

fn text(app: &mut App) -> String {
    text_at(app, 120, 40)
}

fn text_at(app: &mut App, width: u16, height: u16) -> String {
    let terminal = render(app, width, height);
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// Saves a finished Kurovia settlement directly through the service.
fn saved_settlement(
    app: &mut App,
    name: &str,
    size: &str,
) -> settlement_core::settlements::SettlementId {
    let mut active = app.ctx.service.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut sel = Selections::new();
    sel.insert("table.size".into(), size.into());
    active
        .session
        .run_automatic(&sel, &mut RandomFaces::seeded(2))
        .unwrap();
    app.ctx.service.save_new(&active, name).unwrap().id
}

fn set_manual(app: &mut App, presentation: GenerationPresentation) {
    let mut s = app.ctx.settings;
    s.generation_method = GenerationMethod::Manual;
    app.ctx.save_settings(s).unwrap();
    let mut s = app.ctx.settings;
    s.generation_presentation = presentation;
    app.ctx.save_settings(s).unwrap();
}

// ----- main menu and frame ------------------------------------------------------

#[test]
fn main_menu_lists_destinations_with_dlc_disabled() {
    let mut app = app();
    let screen = text(&mut app);
    for item in [
        "Start",
        "Settings",
        "View",
        "Roadmap",
        "DLC Content",
        "Exit",
    ] {
        assert!(screen.contains(item), "missing {item}");
    }
    assert!(screen.contains("v0.1.0"));
    // DLC Content is struck through and dimmed.
    let terminal = render(&mut app, 120, 40);
    let buffer = terminal.backend().buffer();
    let (x, y) = (0..40u16)
        .flat_map(|y| (0..120u16).map(move |x| (x, y)))
        .find(|&(x, y)| buffer[(x, y)].symbol() == "D" && buffer[(x + 1, y)].symbol() == "L")
        .unwrap();
    let style = buffer[(x, y)].style();
    assert!(style.add_modifier.contains(Modifier::CROSSED_OUT));
    assert!(style.add_modifier.contains(Modifier::DIM));

    // Down from Roadmap skips DLC Content to Exit.
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Down],
    );
    key(&mut app, KeyCode::Enter);
    assert!(app.quit, "Exit was selected");
}

#[test]
fn tiny_windows_show_a_notice_instead_of_breaking() {
    let mut app = app();
    let screen = text_at(&mut app, 30, 8);
    assert!(screen.contains("Window too small"));
    let screen = text_at(&mut app, MIN_WIDTH, MIN_HEIGHT);
    assert!(screen.contains("Start"));
}

#[test]
fn every_screen_survives_resizing() {
    let mut app = app();
    saved_settlement(&mut app, "Riverside", "town");
    let sizes = [(20, 6), (48, 14), (60, 18), (80, 24), (120, 40), (200, 60)];
    let check = |app: &mut App| {
        for (w, h) in sizes {
            render(app, w, h);
        }
    };
    check(&mut app);
    // Settings, Tags, Tables, a table editor, a tag editor.
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    check(&mut app);
    for _ in 0..3 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter); // View Tags
    check(&mut app);
    key(&mut app, KeyCode::Enter);
    check(&mut app);
    keys(
        &mut app,
        &[KeyCode::Esc, KeyCode::Esc, KeyCode::Down, KeyCode::Enter],
    ); // View Tables
    check(&mut app);
    keys(
        &mut app,
        &[KeyCode::End, KeyCode::Up, KeyCode::Up, KeyCode::Enter],
    ); // a table
    check(&mut app);
    keys(&mut app, &[KeyCode::Esc, KeyCode::Esc, KeyCode::Esc]);
    // View and Start.
    keys(
        &mut app,
        &[KeyCode::Home, KeyCode::Down, KeyCode::Down, KeyCode::Enter],
    );
    check(&mut app);
    keys(&mut app, &[KeyCode::Esc, KeyCode::Home, KeyCode::Enter]);
    check(&mut app);
}

// ----- settings -------------------------------------------------------------------

#[test]
fn settings_save_immediately_and_lock_presentation_while_automatic() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]);
    let screen = text(&mut app);
    assert!(screen.contains("(Manual only)"));
    assert!(screen.contains("v0.1.0"));
    // Method → Manual, saved through the core.
    key(&mut app, KeyCode::Right);
    assert_eq!(
        app.ctx.service.settings().unwrap().generation_method,
        GenerationMethod::Manual
    );
    assert!(!text(&mut app).contains("(Manual only)"));
    // Presentation is now selectable.
    keys(&mut app, &[KeyCode::Down, KeyCode::Right]);
    assert_eq!(
        app.ctx.settings.generation_presentation,
        GenerationPresentation::OneTable
    );
    // Back to Automatic: presentation keeps its value but is skipped.
    keys(
        &mut app,
        &[KeyCode::Up, KeyCode::Left, KeyCode::Down, KeyCode::Enter],
    );
    assert!(
        app.ctx.settings.god_mode,
        "Down from Method lands on God Mode"
    );
    assert_eq!(
        app.ctx.service.settings().unwrap().generation_presentation,
        GenerationPresentation::OneTable
    );
}

#[test]
fn theme_and_tag_view_are_shared_settings() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]);
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]); // Tag View
    assert!(app.ctx.service.settings().unwrap().tag_view);
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Enter],
    ); // Theme
    assert_eq!(
        app.ctx.service.settings().unwrap().theme,
        settlement_core::settings::Theme::OldTimeyDark
    );
}

// ----- generation -----------------------------------------------------------------

#[test]
fn automatic_generation_end_to_end() {
    let mut app = app();
    key(&mut app, KeyCode::Enter); // Start
    assert!(text(&mut app).contains("Choose a Setting Profile"));
    key(&mut app, KeyCode::Enter); // Kurovia (first)
    // Automatic stops at Size, which has no approved distribution.
    let screen = text(&mut app);
    assert!(screen.contains("Size needs a choice"), "{screen}");
    key(&mut app, KeyCode::Enter); // first size
    let screen = text(&mut app);
    assert!(screen.contains("SETTING PROFILE"), "{screen}");
    assert!(screen.contains("Kurovia"));
    assert!(screen.contains("Not saved yet"));

    // Save: an empty name is refused, then a real one is accepted.
    key(&mut app, KeyCode::Char('s'));
    key(&mut app, KeyCode::Enter);
    assert!(text(&mut app).contains("cannot be empty"));
    type_text(&mut app, "Riverside");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.screen_title(), "Main Menu");
    assert!(text(&mut app).contains("Saved “Riverside”"));
    assert_eq!(app.ctx.service.saved_list().unwrap()[0].name, "Riverside");
}

#[test]
fn completed_settlement_can_be_rerolled_or_deleted() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Enter]);
    assert!(text(&mut app).contains("Re-roll"));
    key(&mut app, KeyCode::Char('r'));
    assert!(text(&mut app).contains("Not saved yet"), "still complete");
    key(&mut app, KeyCode::Char('d'));
    assert!(text(&mut app).contains("Delete this settlement?"));
    keys(&mut app, &[KeyCode::Right, KeyCode::Enter]);
    assert_eq!(app.screen_title(), "Main Menu");
    assert!(app.ctx.service.saved_list().unwrap().is_empty());
}

#[test]
fn incremental_generation_one_stage_at_a_time() {
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::Incremental);
    key(&mut app, KeyCode::Enter);
    let screen = text(&mut app);
    assert!(screen.contains("STAGE 1 OF 8"), "{screen}");
    assert!(screen.contains("Manual · Incremental"));
    // Next is refused until the stage is resolved.
    key(&mut app, KeyCode::Char('n'));
    assert!(text(&mut app).contains("must be rolled or chosen"));
    key(&mut app, KeyCode::Enter); // Kurovia
    key(&mut app, KeyCode::Char('n'));
    assert!(text(&mut app).contains("STAGE 2 OF 8"));
    for stage in 2..=8 {
        if stage == 5 {
            key(&mut app, KeyCode::Enter); // Size must be chosen
        } else {
            key(&mut app, KeyCode::Char('r'));
            assert!(text(&mut app).contains("Rolled"), "stage {stage}");
        }
        key(&mut app, KeyCode::Char('n'));
    }
    assert!(text(&mut app).contains("Not saved yet"));
}

#[test]
fn one_table_generation_with_choices_and_rolls() {
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    assert!(text(&mut app).contains("Manual · One-table"));
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a')); // rolls until Size stops it
    assert!(text(&mut app).contains("Size cannot be generated yet"));
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Down],
    );
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // choose a size
    key(&mut app, KeyCode::Char('a'));
    key(&mut app, KeyCode::Char('f'));
    assert!(text(&mut app).contains("Not saved yet"));
}

#[test]
fn tag_view_shows_tags_only_when_enabled() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Enter]);
    assert!(!text(&mut app).contains("tags:"));
    let mut app = app_with_tag_view();
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Enter]);
    assert!(text(&mut app).contains("tags: Kurovia"));
}

fn app_with_tag_view() -> App {
    let mut app = app();
    let mut s = app.ctx.settings;
    s.tag_view = true;
    app.ctx.save_settings(s).unwrap();
    app
}

#[test]
fn leaving_generation_asks_before_discarding() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // Kurovia chosen
    key(&mut app, KeyCode::Esc);
    assert!(text(&mut app).contains("Leave and discard"));
    key(&mut app, KeyCode::Esc); // keep it
    assert_ne!(app.screen_title(), "Main Menu");
    keys(&mut app, &[KeyCode::Esc, KeyCode::Char('y')]);
    assert_eq!(app.screen_title(), "Main Menu");
}

// ----- saved settlements ------------------------------------------------------------

#[test]
fn view_lists_settlements_with_favourite_stars_and_actions() {
    let mut app = app();
    let a = saved_settlement(&mut app, "Ashford", "town");
    saved_settlement(&mut app, "Brightwater", "village");
    app.ctx.service.saved_set_favourite(&a, true).unwrap();
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let screen = text(&mut app);
    assert!(screen.contains("★ Ashford"), "{screen}");
    assert!(screen.contains("Generated with v0.1.0"));

    // Select the favourite: no Delete offered.
    keys(&mut app, &[KeyCode::End]);
    let screen = text(&mut app);
    assert!(
        screen.contains("Unfavourite") && !screen.contains("Delete"),
        "{screen}"
    );

    // The other one: Edit, Favourite, Delete.
    key(&mut app, KeyCode::Home);
    let screen = text(&mut app);
    assert!(screen.contains("Favourite") && screen.contains("Delete"));
    // Right → actions, Right → Favourite, Right → Delete; confirm.
    keys(
        &mut app,
        &[
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Enter,
        ],
    );
    assert!(text(&mut app).contains("cannot be undone"));
    keys(&mut app, &[KeyCode::Char('y')]);
    let list = app.ctx.service.saved_list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Ashford");
}

#[test]
fn split_pane_on_wide_terminals_and_stacked_on_narrow_ones() {
    let mut app = app();
    saved_settlement(&mut app, "Ashford", "town");
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    assert!(text_at(&mut app, 120, 40).contains("│"));
    assert!(!text_at(&mut app, 60, 30).contains(" │"));
}

#[test]
fn editing_a_saved_settlement_saves_a_revision() {
    let mut app = app();
    saved_settlement(&mut app, "Ashford", "town");
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    keys(&mut app, &[KeyCode::Right, KeyCode::Enter]); // Edit
    assert!(app.screen_title().starts_with("Edit"));
    key(&mut app, KeyCode::Char('s'));
    for _ in 0.."Ashford".len() {
        key(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "Ashford Revisited");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.screen_title(), "Saved Settlements");
    assert_eq!(
        app.ctx.service.saved_list().unwrap()[0].name,
        "Ashford Revisited"
    );
}

// ----- content editors -----------------------------------------------------------------

#[test]
fn tag_editor_renames_and_saves_through_the_core() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    for _ in 0..3 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter); // View Tags
    assert!(text(&mut app).contains("world.kurovia"));
    key(&mut app, KeyCode::Enter); // first tag
    assert!(text(&mut app).contains("ID world.kurovia"));
    // Name row is the first selectable row.
    key(&mut app, KeyCode::Enter);
    keys(&mut app, &[KeyCode::End]);
    type_text(&mut app, " (renamed)");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('s'));
    let tag = app
        .ctx
        .service
        .tag_detail(&TagId::new("world.kurovia"))
        .unwrap()
        .tag;
    assert_eq!(tag.name, "Kurovia (renamed)");
}

#[test]
fn table_editor_shows_uncovered_faces_and_refuses_to_save() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    for _ in 0..4 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter); // View Tables
    key(&mut app, KeyCode::End); // Age is last
    key(&mut app, KeyCode::Enter);
    assert!(text(&mut app).contains("ID table.age"));
    // Name, Description, Dice.
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    for _ in 0..3 {
        key(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "1d20");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("uncovered 9-20"), "{screen}");
    assert!(screen.contains("fix the problems above"));
    key(&mut app, KeyCode::Char('s'));
    assert_eq!(
        app.ctx
            .service
            .table_detail(&"table.age".into())
            .unwrap()
            .table
            .dice
            .as_deref(),
        Some("1d8"),
        "nothing saved"
    );
}

// ----- release hardening ---------------------------------------------------------------

#[test]
fn keys_are_paused_while_the_window_is_too_small() {
    let mut app = app();
    render(&mut app, 30, 8); // shows the notice
    key(&mut app, KeyCode::Enter); // would open Start
    assert_eq!(app.screen_title(), "Main Menu");
    render(&mut app, 120, 40);
    key(&mut app, KeyCode::Enter);
    assert_ne!(app.screen_title(), "Main Menu");
}

#[test]
fn roadmap_screen_shows_the_core_roadmap() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Down]);
    key(&mut app, KeyCode::Enter);
    let screen = text(&mut app);
    for line in settlement_core::roadmap::roadmap() {
        assert!(screen.contains(&line.summary), "{screen}");
    }
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.screen_title(), "Main Menu");
}

#[test]
fn selection_and_choices_are_legible_without_colour() {
    let mut app = app();
    // The selected menu item carries a marker, not just a colour.
    let screen = text(&mut app);
    assert!(screen.contains("▸ Start"), "{screen}");
    key(&mut app, KeyCode::Down);
    assert!(text(&mut app).contains("▸ Settings"));
    // Current choices are bracketed; the disabled setting says why.
    key(&mut app, KeyCode::Enter);
    let screen = text(&mut app);
    assert!(screen.contains("[ Automatic ]"), "{screen}");
    assert!(screen.contains("(Manual only)"), "{screen}");
    assert!(screen.contains("[ Off ]"), "{screen}");
}

#[test]
fn god_mode_offers_otherwise_ineligible_results_during_generation() {
    use settlement_core::dice::ScriptedFaces;
    for god_mode in [false, true] {
        let mut app = app();
        set_manual(&mut app, GenerationPresentation::OneTable);
        let mut s = app.ctx.settings;
        s.god_mode = god_mode;
        app.ctx.save_settings(s).unwrap();
        key(&mut app, KeyCode::Enter); // Start
        keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // Kurovia
        app.ctx.dice = Box::new(ScriptedFaces::new([8])); // Region: Targoth
        keys(&mut app, &[KeyCode::Down, KeyCode::Char('r')]);
        assert!(text(&mut app).contains("Targoth"));
        // Connections: Targoth rules out a Major Route unless in God Mode.
        keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        let picker = text(&mut app);
        assert!(picker.contains("Choose Connections"), "{picker}");
        assert_eq!(picker.contains("Major Route"), god_mode, "{picker}");
    }
}

#[test]
fn favourite_and_unfavourite_by_keypress() {
    let mut app = app();
    let id = saved_settlement(&mut app, "Ashford", "town");
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    keys(&mut app, &[KeyCode::Right, KeyCode::Right, KeyCode::Enter]); // Favourite
    assert!(app.ctx.service.saved_get(&id).unwrap().summary.favourite);
    let screen = text(&mut app);
    assert!(
        screen.contains("★ Ashford") && !screen.contains("Delete"),
        "{screen}"
    );
    key(&mut app, KeyCode::Enter); // Unfavourite (focus stays on it)
    assert!(!app.ctx.service.saved_get(&id).unwrap().summary.favourite);
    assert!(text(&mut app).contains("Delete"));
}

#[test]
fn edit_names_are_checked_and_unchanged_edits_are_not_saved() {
    let mut app = app();
    saved_settlement(&mut app, "Ashford", "town");
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    keys(&mut app, &[KeyCode::Right, KeyCode::Enter]); // Edit

    // Nothing changed: refused by the core, with a reason.
    keys(&mut app, &[KeyCode::Char('s'), KeyCode::Enter]);
    assert!(text(&mut app).contains("Nothing has changed"));
    assert!(app.screen_title().starts_with("Edit"));

    // Too long: refused, nothing saved.
    key(&mut app, KeyCode::Char('s'));
    type_text(&mut app, &"x".repeat(80));
    key(&mut app, KeyCode::Enter);
    assert!(app.screen_title().starts_with("Edit"));
    assert_eq!(app.ctx.service.saved_list().unwrap()[0].name, "Ashford");
    assert!(text(&mut app).contains("80"));
}
