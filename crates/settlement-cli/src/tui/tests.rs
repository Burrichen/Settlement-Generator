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

/// Moves the One-table selection to the stage row labelled `label`.
fn select_row(app: &mut App, label: &str) {
    key(app, KeyCode::Home);
    for _ in 0..40 {
        let screen = text_at(app, 140, 60);
        if screen.lines().any(|l| {
            l.contains('▸')
                && l.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains(&format!("▸ {label} "))
        }) {
            return;
        }
        key(app, KeyCode::Down);
    }
    panic!("no row {label}");
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
    assert!(screen.contains(&format!("v{}", settlement_core::VERSION)));
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
    for _ in 0..4 {
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
    assert!(screen.contains(&format!("v{}", settlement_core::VERSION)));
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
    select_line(&mut app, "Theme");
    key(&mut app, KeyCode::Enter);
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
    // Automatic rolls every remaining stage, Size included.
    let screen = text(&mut app);
    assert!(screen.contains("SETTING PROFILE"), "{screen}");
    assert!(screen.contains("Kurovia"));
    assert!(screen.contains("SIZE"), "{screen}");
    assert!(screen.contains("ECONOMY"), "{screen}");
    assert!(screen.contains("AUTHORITY"), "{screen}");
    assert!(
        ["Failing", "Struggling", "Stable", "Prosperous", "Wealthy"]
            .iter()
            .any(|economy| screen.contains(economy)),
        "{screen}"
    );
    assert!(
        ["Trading Post", "Village", "Town", "City", "Capital"]
            .iter()
            .any(|size| screen.contains(size)),
        "{screen}"
    );
    assert!(screen.contains("Not saved yet"));

    // Save: an empty name is refused, then a real one is accepted. (Clear
    // any name the Name stage offered first.)
    key(&mut app, KeyCode::Char('s'));
    for _ in 0..40 {
        key(&mut app, KeyCode::Backspace);
    }
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
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
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
    assert!(screen.contains("STAGE 1 OF 19"), "{screen}");
    assert!(screen.contains("Manual · Incremental"));
    // Next is refused until the stage is resolved.
    key(&mut app, KeyCode::Char('n'));
    assert!(text(&mut app).contains("must be rolled or chosen"));
    key(&mut app, KeyCode::Enter); // Kurovia
    key(&mut app, KeyCode::Char('n'));
    assert!(text(&mut app).contains("STAGE 2 OF 19"));
    for stage in 2..=10 {
        if stage == 10 {
            // Economy follows Age.
            let screen = text(&mut app);
            assert!(screen.contains("STAGE 10 OF 19"), "{screen}");
            assert!(screen.contains("Economy"), "{screen}");
        }
        if stage == 3 {
            // Name: typed, so the test does not depend on the region rolled.
            key(&mut app, KeyCode::Char('t'));
            type_text(&mut app, "Riverside");
            key(&mut app, KeyCode::Enter);
            assert!(text(&mut app).contains("Typed name"), "stage {stage}");
        } else {
            key(&mut app, KeyCode::Char('r'));
            assert!(text(&mut app).contains("Rolled"), "stage {stage}");
        }
        key(&mut app, KeyCode::Char('n'));
    }
    // Authority follows Economy. Choose Local Council (the first result):
    // always a group, so Representation is passed over and Authority Size
    // comes next.
    let screen = text(&mut app);
    assert!(screen.contains("STAGE 11 OF 19"), "{screen}");
    assert!(screen.contains("Popular Assembly"), "{screen}");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('n'));
    let screen = text(&mut app);
    assert!(screen.contains("STAGE 13 OF 19"), "{screen}");
    assert!(screen.contains("Authority Size"), "{screen}");
    assert!(!screen.contains("Representation"), "skipped: {screen}");
    key(&mut app, KeyCode::Char('r'));
    assert!(text(&mut app).contains("Rolled"));
    key(&mut app, KeyCode::Char('n'));
    // Roles: one lead and the rest members, before the figures.
    let screen = text(&mut app);
    assert!(screen.contains("STAGE 14 OF 19"), "{screen}");
    assert!(screen.contains("allocate roles"), "{screen}");
    assert!(screen.contains("Figure 1 · Lead"), "{screen}");
    key(&mut app, KeyCode::Char('r'));
    let screen = text(&mut app);
    assert!(screen.contains("on 1d6"), "{screen}");
    key(&mut app, KeyCode::Char('n'));
    // Authority Figures: generate them all.
    let screen = text(&mut app);
    assert!(screen.contains("STAGE 15 OF 19"), "{screen}");
    assert!(screen.contains("generate figures"), "{screen}");
    key(&mut app, KeyCode::Char('r'));
    let screen = text_at(&mut app, 140, 80);
    // Every Kurovia species has a name table: no unnamed figures.
    assert!(!screen.contains("Figure 1"), "{screen}");
    assert!(!screen.contains("No name table"), "{screen}");
    assert!(screen.contains(" ↔ "), "{screen}");
    assert!(screen.contains("Disposition: "), "{screen}");
    assert!(screen.contains("RELATIONSHIPS"), "{screen}");
    key(&mut app, KeyCode::Char('n'));
    // Security, Crime and Stability, in that order.
    for (stage, name) in [(16, "Security"), (17, "Crime"), (18, "Stability")] {
        let screen = text(&mut app);
        assert!(screen.contains(&format!("STAGE {stage} OF 19")), "{screen}");
        assert!(screen.contains(name), "{screen}");
        key(&mut app, KeyCode::Char('r'));
        assert!(text(&mut app).contains("on 1d12"), "{name}");
        key(&mut app, KeyCode::Char('n'));
    }
    // Notable Locations last.
    let screen = text(&mut app);
    assert!(screen.contains("STAGE 19 OF 19"), "{screen}");
    key(&mut app, KeyCode::Char('r'));
    key(&mut app, KeyCode::Char('n'));
    let screen = text_at(&mut app, 120, 160);
    assert!(screen.contains("Local Council"), "{screen}");
    for label in ["SECURITY", "CRIME", "STABILITY"] {
        assert!(screen.contains(label), "{label}: {screen}");
    }
    assert!(screen.contains("Representatives"), "{screen}");
    assert!(screen.contains("Not saved yet"), "{screen}");
    assert!(text(&mut app).contains("Not saved yet"));
}

#[test]
fn a_flow_stage_added_to_the_pipeline_is_generated_and_shown() {
    use settlement_core::framework::layers::ItemKind;
    let mut app = app();
    // A user adds a stage to the pipeline: roll Motive twice.
    let service = &mut app.ctx.service;
    let mut flow = service
        .framework_item(ItemKind::Flow, "flow.settlement")
        .unwrap();
    flow["steps"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "omens", "name": "Omens", "kind": "repeat", "count": { "times": 2 },
            "steps": [{ "id": "omen", "kind": "roll", "table": "table.authorityFigure.motive" }]
        }));
    service.save_framework_item(ItemKind::Flow, flow).unwrap();
    key(&mut app, KeyCode::Enter); // Start
    key(&mut app, KeyCode::Enter); // Kurovia
    // The sheet scrolls; a tall window shows the whole settlement.
    let screen = text_at(&mut app, 120, 200);
    let omens: Vec<&str> = screen
        .lines()
        .skip_while(|l| !l.contains("OMENS"))
        .take(3)
        .collect();
    assert!(omens[0].contains("2 ×"), "{screen}");
    // Each roll on its own line below the stage's result.
    assert!(
        omens[1..]
            .iter()
            .all(|l| l.contains("Authority Figure Motive: ")),
        "{screen}"
    );
}

#[test]
fn one_table_generation_with_choices_and_rolls() {
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    assert!(text(&mut app).contains("Manual · One-table"));
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a')); // rolls every remaining stage
    let screen = text(&mut app);
    assert!(!screen.contains("cannot be generated yet"), "{screen}");
    key(&mut app, KeyCode::Char('f'));
    assert!(text(&mut app).contains("Not saved yet"));
}

#[test]
fn one_table_shows_authority_sub_stages_only_where_they_apply() {
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a')); // roll everything
    // Rows: Setting Profile … Economy, then Authority (row 10).
    key(&mut app, KeyCode::Home);
    for _ in 0..10 {
        key(&mut app, KeyCode::Down);
    }
    // The picker opens on the current result; Home goes to the first.
    keys(&mut app, &[KeyCode::Enter, KeyCode::Home, KeyCode::Enter]); // Local Council
    let screen = text_at(&mut app, 140, 50);
    assert!(screen.contains("Local Council"), "{screen}");
    assert!(!screen.contains("Representation"), "decided: {screen}");
    assert!(screen.contains("  Authority Size"), "{screen}");
    // The row under Authority is Authority Size: rolling it uses 1d10.
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char('r'));
    let screen = text_at(&mut app, 140, 50);
    assert!(screen.contains("on 1d10"), "{screen}");
    assert!(screen.contains("Representatives"), "{screen}");

    // A single ruler: Authority Size no longer applies and disappears.
    key(&mut app, KeyCode::Up);
    keys(
        &mut app,
        &[KeyCode::Enter, KeyCode::Home, KeyCode::Down, KeyCode::Enter],
    ); // Noble
    let screen = text_at(&mut app, 140, 50);
    assert!(screen.contains("Noble / Landed Ruler"), "{screen}");
    assert!(!screen.contains("Authority Size"), "{screen}");
    assert!(!screen.contains("Representatives"), "{screen}");
}

#[test]
fn view_folds_authority_size_into_authority() {
    let mut app = app();
    let mut active = app.ctx.service.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut sel = Selections::new();
    sel.insert("table.authority".into(), "council".into());
    sel.insert("table.authoritySize".into(), "five".into());
    active
        .session
        .run_automatic(&sel, &mut RandomFaces::seeded(2))
        .unwrap();
    app.ctx.service.save_new(&active, "Ashford").unwrap();
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let screen = text_at(&mut app, 160, 70);
    let authority = screen
        .lines()
        .position(|l| l.contains("AUTHORITY"))
        .unwrap_or_else(|| panic!("{screen}"));
    let lines: Vec<&str> = screen.lines().collect();
    assert!(lines[authority].contains("Local Council"), "{screen}");
    assert!(
        lines[authority + 1].contains("5 Representatives"),
        "{screen}"
    );
    assert!(!screen.contains("REPRESENTATION"), "{screen}");
    assert!(!screen.contains("AUTHORITY SIZE"), "{screen}");
    assert!(!screen.contains("Not applicable"), "{screen}");
}

#[test]
fn one_table_edits_authority_figures_through_the_picker() {
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a')); // roll everything, figures included
    select_row(&mut app, "Authority Figures");
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Authority Figure"), "{screen}");

    // Type a name for the first figure.
    key(&mut app, KeyCode::Enter);
    assert!(text_at(&mut app, 140, 60).contains("Change an Authority Figure"));
    key(&mut app, KeyCode::Enter); // "Figure 1 · Type a name"
    type_text(&mut app, "Mila Varen");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Mila Varen"), "{screen}");

    // Choose her species: the picker lists only the Kurovia species.
    key(&mut app, KeyCode::Enter);
    // Items: type name, roll name (she has a name now), then the fields.
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Choose Species"), "{screen}");
    assert!(
        screen.contains("Firbolg") && screen.contains("Yuan-Ti"),
        "{screen}"
    );
    keys(&mut app, &[KeyCode::Home, KeyCode::Enter]); // Human
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Human •"), "{screen}");
    // A typed name stands when the species changes.
    assert!(screen.contains("Mila Varen"), "{screen}");
}

#[test]
fn a_figures_given_name_is_chosen_from_its_species_pool() {
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a'));
    select_row(&mut app, "Authority Figures");
    key(&mut app, KeyCode::Enter);
    // Type a name, Roll a name, then the parts of the generated name.
    keys(&mut app, &[KeyCode::Down, KeyCode::Down]);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("· Given name"), "{screen}");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Choose Given name"), "{screen}");
    keys(&mut app, &[KeyCode::Home, KeyCode::Enter]);
    let screen = text_at(&mut app, 140, 60);
    assert!(!screen.contains("Choose Given name"), "{screen}");
    assert!(!screen.contains("error"), "{screen}");
}

#[test]
fn one_table_changes_a_role_through_the_picker() {
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a'));
    select_row(&mut app, "Roles");
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("  Roles"), "{screen}");
    assert!(screen.contains("change a role"), "{screen}");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Change a role"), "{screen}");
    key(&mut app, KeyCode::Enter); // the first figure's role
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Choose "), "{screen}");
    keys(&mut app, &[KeyCode::Home, KeyCode::Enter]);
    let screen = text_at(&mut app, 140, 60);
    assert!(!screen.contains("Change a role"), "{screen}");
    assert!(screen.contains("chosen"), "{screen}");
}

#[test]
fn view_shows_each_figures_title_above_its_name() {
    let mut app = app();
    let id = saved_settlement(&mut app, "Ashford", "town");
    let saved = app.ctx.service.saved_get(&id).unwrap();
    let figures = saved
        .stages
        .iter()
        .find_map(|st| st.figures.clone())
        .unwrap();
    let first = &figures.figures[0];
    let title = first.role.as_ref().unwrap().label.clone();
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let screen = text_at(&mut app, 160, 90);
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains(&first.label))
        .unwrap_or_else(|| panic!("{screen}"));
    assert!(lines[at - 1].contains(&title), "{screen}");
    // The Roles stage is not listed on its own.
    assert!(!screen.contains("ROLES"), "{screen}");
}

#[test]
fn a_long_completed_settlement_scrolls_and_keeps_its_actions() {
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a'));
    // The largest group: six figures and fifteen relationships.
    key(&mut app, KeyCode::Home);
    for _ in 0..10 {
        key(&mut app, KeyCode::Down);
    }
    keys(&mut app, &[KeyCode::Enter, KeyCode::Home, KeyCode::Enter]); // Local Council
    key(&mut app, KeyCode::Down); // Authority Size
    keys(&mut app, &[KeyCode::Enter, KeyCode::End, KeyCode::Enter]); // 6
    key(&mut app, KeyCode::Down); // Roles (invalidated)
    key(&mut app, KeyCode::Char('r'));
    key(&mut app, KeyCode::Down); // Authority Figures (invalidated)
    key(&mut app, KeyCode::Char('r'));
    key(&mut app, KeyCode::Char('f'));
    let screen = text_at(&mut app, 100, 40);
    assert!(screen.contains("Not saved yet"), "{screen}");
    assert!(screen.contains("Re-roll"), "{screen}");
    assert!(screen.contains("scroll the settlement"), "{screen}");
    assert!(!screen.contains("RELATIONSHIPS"), "{screen}");
    key(&mut app, KeyCode::End);
    let screen = text_at(&mut app, 100, 40);
    assert!(screen.contains("RELATIONSHIPS"), "{screen}");
    assert!(screen.contains("Re-roll"), "actions stay in view: {screen}");
}

#[test]
fn tag_view_shows_tags_only_when_enabled() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
    assert!(!text(&mut app).contains("tags:"));
    let mut app = app_with_tag_view();
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
    let screen = text(&mut app);
    assert!(screen.contains("tags: Kurovia"), "{screen}");
    // The rolled Size shows its size.* tag (named like the result).
    assert!(
        ["Trading Post", "Village", "Town", "City", "Capital"]
            .iter()
            .any(|size| screen.contains(&format!("tags: {size}"))),
        "{screen}"
    );
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
    assert!(screen.contains(&format!("Generated with v{}", settlement_core::VERSION)));

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
fn view_shows_the_stored_economy_and_its_tag_with_tag_view() {
    let mut app = app();
    let id = saved_settlement(&mut app, "Ashford", "town");
    let stored = app
        .ctx
        .service
        .saved_get(&id)
        .unwrap()
        .stages
        .into_iter()
        .find(|s| s.stage_table.as_str() == "table.economy")
        .unwrap();
    let economy = stored.result.clone().unwrap().name;
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("ECONOMY"), "{screen}");
    assert!(screen.contains(&economy), "{screen}");
    assert!(!screen.contains("tags:"), "{screen}");

    let mut app = app_with_tag_view();
    saved_settlement(&mut app, "Ashford", "town");
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains(&format!("tags: {economy}")), "{screen}");
}

#[test]
fn view_shows_the_stored_authority_and_its_tag_with_tag_view() {
    let mut app = app_with_tag_view();
    let id = saved_settlement(&mut app, "Ashford", "town");
    let stored = app
        .ctx
        .service
        .saved_get(&id)
        .unwrap()
        .stages
        .into_iter()
        .find(|s| s.stage_table.as_str() == "table.authority")
        .unwrap();
    let authority = stored.result.clone().unwrap().name;
    let tag = stored.tags[0].name.clone();
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let screen = text_at(&mut app, 160, 70);
    assert!(screen.contains("AUTHORITY"), "{screen}");
    assert!(screen.contains(&authority), "{screen}");
    assert!(screen.contains(&format!("tags: {tag}")), "{screen}");
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
    for _ in 0..4 {
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
fn table_editor_opens_size_like_any_other_table() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    for _ in 0..5 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter); // View Tables
    let screen = text_at(&mut app, 140, 60);
    let size_row = screen
        .lines()
        .find(|l| l.contains("table.size"))
        .unwrap_or_else(|| panic!("{screen}"));
    assert!(!size_row.contains("incomplete"), "{size_row}");
    key(&mut app, KeyCode::Char('/')); // filter by ID
    type_text(&mut app, "table.size");
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("ID table.size"), "{screen}");
    assert!(screen.contains("1d10"), "{screen}");
    assert!(!screen.contains("Marked incomplete"), "{screen}");
    for size in ["Trading Post", "Village", "Town", "City", "Capital"] {
        assert!(screen.contains(size), "{size}: {screen}");
    }
}

#[test]
fn table_editor_shows_unique_roles_and_their_fallbacks() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    for _ in 0..5 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter); // View Tables
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "table.authorityRole.council.member");
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
    // Move to the Treasurer result and open it.
    for _ in 0..40 {
        let screen = text_at(&mut app, 160, 70);
        if screen
            .lines()
            .any(|l| l.contains('▸') && l.trim_end().ends_with("17%  Treasurer"))
        {
            break;
        }
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 160, 70);
    assert!(
        screen.contains("RESULT · role.council.treasurer"),
        "{screen}"
    );
    let line = |label: &str| {
        screen
            .lines()
            .find(|l| l.contains(label))
            .unwrap_or_else(|| panic!("{label}: {screen}"))
            .to_owned()
    };
    assert!(line("Unique").contains("Yes"), "{screen}");
    assert!(line("Fallback").contains("Councillor"), "{screen}");
    // No tags: roles never become settlement tags.
    assert!(line("Emits tags").contains("—"), "{screen}");
}

#[test]
fn table_editor_opens_authority_like_any_other_table() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    for _ in 0..5 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter); // View Tables
    key(&mut app, KeyCode::Char('/')); // filter by ID
    type_text(&mut app, "table.authority");
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
    let screen = text_at(&mut app, 160, 70);
    assert!(screen.contains("ID table.authority"), "{screen}");
    assert!(screen.contains("1d12"), "{screen}");
    for authority in ["Local Council", "Merchant / Guild Rule", "Popular Assembly"] {
        assert!(screen.contains(authority), "{authority}: {screen}");
    }
    // The reserved generator reference is not shown as an editable field.
    assert!(!screen.contains("authorityNpcs"), "{screen}");
}

#[test]
fn table_editor_shows_uncovered_faces_and_refuses_to_save() {
    let mut app = open_table("table.age");
    // Changing the dice lays the results out on the new dice.
    select_line(&mut app, "Dice");
    key(&mut app, KeyCode::Enter);
    for _ in 0..3 {
        key(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "1d20");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 80);
    assert!(
        screen.contains("Every face is covered exactly once"),
        "{screen}"
    );
    // Faces set by hand that leave a gap are flagged and block saving.
    select_line(&mut app, "Ancient");
    key(&mut app, KeyCode::Enter);
    select_line(&mut app, "Faces");
    key(&mut app, KeyCode::Enter);
    for _ in 0..8 {
        key(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "20");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Esc);
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("No result is on"), "{screen}");
    assert!(screen.contains("fix the problems above"), "{screen}");
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
    // Undo steps back through both changes.
    keys(&mut app, &[KeyCode::Char('u'), KeyCode::Char('u')]);
    assert!(text_at(&mut app, 140, 80).contains("1d8"));
    key(&mut app, KeyCode::Char('U'));
    assert!(text_at(&mut app, 140, 80).contains("1d20"));
}

/// Opens the table editor on `id`.
fn open_table(id: &str) -> App {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    for _ in 0..5 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter); // View Tables
    key(&mut app, KeyCode::Char('/')); // filter by ID
    type_text(&mut app, id);
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
    assert!(text(&mut app).contains(&format!("ID {id}")));
    app
}

/// Moves the selection to the first selectable line containing `label`.
fn select_line(app: &mut App, label: &str) {
    key(app, KeyCode::Home);
    for _ in 0..120 {
        let screen = text_at(app, 140, 80);
        if screen
            .lines()
            .any(|l| l.trim_start().starts_with('▸') && l.contains(label))
        {
            return;
        }
        key(app, KeyCode::Down);
    }
    panic!("no line {label}:\n{}", text_at(app, 140, 80));
}

#[test]
fn the_dice_assistant_lays_out_equal_results() {
    let mut app = open_table("table.age");
    select_line(&mut app, "Chance mode");
    key(&mut app, KeyCode::Enter); // Equal
    let screen = text_at(&mut app, 140, 80);
    assert!(
        screen.contains("Equal (every result equally likely)"),
        "{screen}"
    );
    assert!(
        screen.contains("4 equal results fit 1d4 exactly"),
        "{screen}"
    );
    // On 1d8, 4 equal results take 2 faces each.
    assert!(screen.contains("    1-2"), "{screen}");
    select_line(&mut app, "Use 1d4, 1 face each");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("1d4"), "{screen}");
    // Kept as a draft until saved.
    assert!(
        app.ctx
            .service
            .table_draft(&"table.age".into())
            .unwrap()
            .is_some()
    );
    key(&mut app, KeyCode::Char('s'));
    let saved = app
        .ctx
        .service
        .table_detail(&"table.age".into())
        .unwrap()
        .table;
    assert_eq!(saved.dice.as_deref(), Some("1d4"));
    assert!(
        app.ctx
            .service
            .table_draft(&"table.age".into())
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_result_in_use_is_not_removed_and_tests_and_previews_work() {
    let mut app = open_table("table.notableLocationType");
    select_line(&mut app, "Hospitality");
    key(&mut app, KeyCode::Enter);
    select_line(&mut app, "Remove this result");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 80);
    assert!(
        screen.contains("cannot be removed while it is used by"),
        "{screen}"
    );
    key(&mut app, KeyCode::Esc);
    select_line(&mut app, "Test a dice total");
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "1");
    key(&mut app, KeyCode::Enter);
    assert!(text_at(&mut app, 140, 80).contains("1 gives"));
    select_line(&mut app, "Preview roll");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 80);
    assert!(
        screen.contains("Notable Location Type: rolled 1d"),
        "{screen}"
    );
    assert!(screen.contains("each die is rolled separately"), "{screen}");
}

#[test]
fn a_table_can_be_copied_into_a_new_one() {
    let mut app = open_table("table.age");
    select_line(&mut app, "Copy to a new table");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("Copy of Age"), "{screen}");
    assert!(screen.contains("new custom table (not saved)"), "{screen}");
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
        // Connections (after Name): Targoth rules out a Major Route unless
        // in God Mode.
        keys(
            &mut app,
            &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Enter],
        );
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

// ----- settlement names --------------------------------------------------------------

/// One-table at Kurovia › Ordon, with Tag View on, cursor on the Name row.
fn one_table_at_ordon_name() -> App {
    use settlement_core::dice::ScriptedFaces;
    let mut app = app_with_tag_view();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter); // Start
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // Kurovia
    app.ctx.dice = Box::new(ScriptedFaces::new([1])); // Region: Ordon
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Char('r'), KeyCode::Down],
    );
    app.ctx.dice = Box::new(RandomFaces::seeded(3));
    app
}

#[test]
fn names_are_rolled_chosen_rerolled_and_inspected_from_the_keyboard() {
    use settlement_core::dice::ScriptedFaces;
    let mut app = one_table_at_ordon_name();
    let screen = text(&mut app);
    assert!(screen.contains("Ordon Naming Method"), "{screen}");
    assert!(screen.contains("type name"), "hint for typing a name");

    // r: roll a name (method, then each part, with real dice).
    app.ctx.dice = Box::new(ScriptedFaces::new([1, 1, 49]));
    key(&mut app, KeyCode::Char('r'));
    let screen = text_at(&mut app, 140, 50);
    assert!(screen.contains("Alderwick"), "{screen}");
    assert!(
        screen.contains("Ordon naming · Geographic Compound"),
        "{screen}"
    );
    // Tag View: routing and every roll.
    assert!(screen.contains("chosen by Kurovia + Ordon"), "{screen}");
    assert!(
        screen.contains("Ordon Name: First Element: Alder — rolled 1 on 1d100"),
        "{screen}"
    );

    // Enter: choose a method; its parts are rolled.
    key(&mut app, KeyCode::Enter);
    let picker = text(&mut app);
    assert!(picker.contains("Historical Founder"), "{picker}");
    app.ctx.dice = Box::new(ScriptedFaces::new([74, 6]));
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]);
    assert!(text(&mut app).contains("Godwinsbury"));
    // The region is untouched by naming.
    assert!(text(&mut app).contains("Ordon"));
}

#[test]
fn a_typed_name_is_always_possible_and_offered_when_saving() {
    let mut app = one_table_at_ordon_name();
    key(&mut app, KeyCode::Char('t'));
    type_text(&mut app, "Riverside");
    key(&mut app, KeyCode::Enter);
    let screen = text(&mut app);
    assert!(
        screen.contains("Riverside") && screen.contains("Typed name"),
        "{screen}"
    );

    // Roll the rest, then save: the name is offered.
    key(&mut app, KeyCode::Char('a'));
    key(&mut app, KeyCode::Char('f'));
    key(&mut app, KeyCode::Char('s'));
    assert!(text(&mut app).contains("Riverside"));
    key(&mut app, KeyCode::Enter); // accept the offered name
    let saved = app.ctx.service.saved_list().unwrap();
    assert_eq!(saved[0].name, "Riverside");
    // The snapshot records the name and that it was typed.
    let settlement = app.ctx.service.saved_get(&saved[0].id).unwrap();
    let name = settlement
        .stages
        .iter()
        .find_map(|s| s.name.as_ref())
        .unwrap();
    assert_eq!(name.source, settlement_core::naming::NameSource::Custom);
}

#[test]
fn regions_without_naming_data_say_so_and_accept_a_typed_name() {
    use settlement_core::dice::ScriptedFaces;
    let mut app = app();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    // A Setting Profile other than Kurovia: no naming data yet.
    keys(&mut app, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
    assert!(!text(&mut app).contains("Kurovia"));
    app.ctx.dice = Box::new(ScriptedFaces::new([5]));
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Char('r'), KeyCode::Down],
    );
    let screen = text(&mut app);
    assert!(screen.contains("no naming data"), "{screen}");
    assert!(
        screen.contains("No naming data for this region yet"),
        "{screen}"
    );
    key(&mut app, KeyCode::Char('t'));
    type_text(&mut app, "Saltmarsh");
    key(&mut app, KeyCode::Enter);
    assert!(text(&mut app).contains("Saltmarsh"));
}

// ----- notable locations ---------------------------------------------------------------

#[test]
fn bare_minimum_buildings_is_a_saved_setting() {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]);
    assert!(text(&mut app).contains("Bare Minimum Buildings"));
    // Method → God Mode → Tag View → Bare Minimum Buildings.
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Enter],
    );
    assert!(app.ctx.service.settings().unwrap().bare_minimum_buildings);
    let active = app.ctx.service.start_generation().unwrap();
    assert!(active.session.options().bare_minimum_buildings);
}

#[test]
fn one_table_changes_a_location_type_but_not_the_tavern() {
    let mut app = app();
    let mut s = app.ctx.settings;
    s.bare_minimum_buildings = true;
    app.ctx.save_settings(s).unwrap();
    set_manual(&mut app, GenerationPresentation::OneTable);
    key(&mut app, KeyCode::Enter);
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a'));
    select_row(&mut app, "Notable Locations");
    let screen = text_at(&mut app, 140, 60);
    assert!(
        screen.contains(" 1. Tavern · Hospitality · 4 spare rooms"),
        "{screen}"
    );
    assert!(
        screen.contains("change a count or a location's type"),
        "{screen}"
    );
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Change Notable Locations"), "{screen}");
    assert!(screen.contains("Notable Location Count"), "{screen}");
    // The Tavern is location 1 and is not offered.
    assert!(!screen.contains("Location 1 "), "{screen}");
    assert!(screen.contains("Location 2"), "{screen}");
    key(&mut app, KeyCode::End); // the last rolled location
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Choose Notable Location Type"), "{screen}");
    assert!(screen.contains("Roll again"), "{screen}");
    assert!(screen.contains("Criminal"), "{screen}");
    assert!(!screen.contains("Residence"), "{screen}");
    keys(&mut app, &[KeyCode::Home, KeyCode::Enter]); // Roll again
    let screen = text_at(&mut app, 140, 60);
    assert!(!screen.contains("Choose Notable Location Type"), "{screen}");
    assert!(
        screen.contains(" 1. Tavern · Hospitality · 4 spare rooms"),
        "{screen}"
    );
}

#[test]
fn view_lists_notable_locations_with_their_districts() {
    let mut app = app();
    let mut s = app.ctx.settings;
    s.bare_minimum_buildings = true;
    app.ctx.save_settings(s).unwrap();
    let id = saved_settlement(&mut app, "Highmarch", "city");
    let saved = app.ctx.service.saved_get(&id).unwrap();
    let record = saved
        .stages
        .iter()
        .find_map(|st| st.locations.clone())
        .unwrap();
    keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let screen = text_at(&mut app, 160, 120);
    assert!(screen.contains("NOTABLE LOCATIONS"), "{screen}");
    assert!(
        screen.contains(" 1. Tavern · Hospitality · 4 spare rooms"),
        "{screen}"
    );
    assert!(screen.contains("· District 1"), "{screen}");
    assert!(
        screen.contains(&record.districts.unwrap().roll.result.name),
        "{screen}"
    );
    // One Tavern for the whole City, not one per district.
    assert_eq!(screen.matches("4 spare rooms").count(), 1, "{screen}");
}

// ----- flow editor ---------------------------------------------------------------------

fn open_flows() -> App {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    for _ in 0..6 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter); // View Flows
    app
}

/// Picks item `index` of an open picker.
fn pick(app: &mut App, index: usize) {
    key(app, KeyCode::Home);
    for _ in 0..index {
        key(app, KeyCode::Down);
    }
    key(app, KeyCode::Enter);
}

#[test]
fn flows_are_listed_and_the_pipeline_opens_as_a_tree() {
    let mut app = open_flows();
    let screen = text_at(&mut app, 140, 40);
    assert!(screen.contains("Settlement"), "{screen}");
    assert!(screen.contains("flow.settlement"), "{screen}");
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "flow.settlement");
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("ID flow.settlement"), "{screen}");
    assert!(
        screen.contains("Roll Table: Roll Setting Profile"),
        "{screen}"
    );
    // Pseudocode is one Tab away, problems two.
    key(&mut app, KeyCode::Tab);
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("Roll Setting Profile."), "{screen}");
    key(&mut app, KeyCode::Tab);
    assert!(text_at(&mut app, 140, 80).contains("Problems ("));
    for (w, h) in [(48, 14), (80, 24), (200, 60)] {
        render(&mut app, w, h);
    }
}

#[test]
fn a_new_flow_is_built_from_steps_with_undo_and_saved() {
    let mut app = open_flows();
    key(&mut app, KeyCode::Char('n'));
    select_line(&mut app, "+ add a step");
    key(&mut app, KeyCode::Enter);
    pick(&mut app, 4); // Repeat N Times
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("Repeat N Times: Repeat 1 time"), "{screen}");
    assert!(screen.contains("Each time:"), "{screen}");
    // Add a Stop inside the repeat: its empty body comes first.
    select_line(&mut app, "+ add a step");
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("▸         + add a step"), "{screen}");
    key(&mut app, KeyCode::Enter);
    pick(&mut app, 11); // Stop / End
    key(&mut app, KeyCode::Tab);
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("Repeat 1 time:"), "{screen}");
    assert!(screen.contains("    Stop."), "{screen}");
    // Undo takes the Stop away again; redo puts it back.
    key(&mut app, KeyCode::Char('u'));
    assert!(!text_at(&mut app, 140, 80).contains("    Stop."));
    key(&mut app, KeyCode::Char('U'));
    assert!(text_at(&mut app, 140, 80).contains("    Stop."));
    key(&mut app, KeyCode::Char('s'));
    let screen = text_at(&mut app, 140, 80);
    assert!(screen.contains("Flow saved."), "{screen}");
    keys(&mut app, &[KeyCode::Esc, KeyCode::Esc]);
    assert!(text_at(&mut app, 140, 40).contains("New flow"));
}

#[test]
fn a_condition_is_built_in_plain_words() {
    let mut app = open_flows();
    key(&mut app, KeyCode::Char('n'));
    select_line(&mut app, "+ add a step");
    key(&mut app, KeyCode::Enter);
    pick(&mut app, 2); // If / Condition
    select_line(&mut app, "If / Condition:");
    key(&mut app, KeyCode::Enter); // its settings
    select_line(&mut app, "If ");
    key(&mut app, KeyCode::Enter); // the case's condition
    // An empty case is "always": a single All of, with nothing in it.
    select_line(&mut app, "All of:");
    key(&mut app, KeyCode::Char('t'));
    pick(&mut app, 5); // Earlier result is
    pick(&mut app, 0); // the first earlier step (Setting Profile)
    keys(&mut app, &[KeyCode::Char(' '), KeyCode::Enter]); // tick its first result
    let screen = text_at(&mut app, 140, 40);
    assert!(screen.contains("WHEN Setting Profile is "), "{screen}");
    key(&mut app, KeyCode::Esc);
    let screen = text_at(&mut app, 140, 40);
    assert!(screen.contains("Setting Profile is "), "{screen}");
    // Back in the tree, the case is labelled in words too.
    key(&mut app, KeyCode::Esc);
    assert!(text_at(&mut app, 140, 40).contains("IF Setting Profile is "));
}

#[test]
fn steps_move_disconnect_and_reconnect() {
    let mut app = open_flows();
    key(&mut app, KeyCode::Char('n'));
    select_line(&mut app, "+ add a step");
    key(&mut app, KeyCode::Enter);
    pick(&mut app, 11); // Stop / End
    select_line(&mut app, "+ add a step here");
    key(&mut app, KeyCode::Enter);
    pick(&mut app, 7); // Add Tag
    select_line(&mut app, "Add Tag:");
    key(&mut app, KeyCode::Char('K')); // up, above the Stop
    let screen = text_at(&mut app, 140, 40);
    let tag = screen.find("Add Tag:").unwrap();
    let stop = screen.find("Stop / End:").unwrap();
    assert!(tag < stop, "{screen}");
    key(&mut app, KeyCode::Char('d')); // disconnect it
    let screen = text_at(&mut app, 140, 40);
    assert!(screen.contains("NOT CONNECTED"), "{screen}");
    select_line(&mut app, "Add Tag:");
    key(&mut app, KeyCode::Char('m'));
    select_line(&mut app, "↳ put it here");
    key(&mut app, KeyCode::End);
    select_line(&mut app, "↳ put it here");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 40);
    assert!(!screen.contains("NOT CONNECTED"), "{screen}");
}

// ----- tags, settings and modifiers ----------------------------------------------------

fn open_settings_row(label: &str) -> App {
    let mut app = app();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    select_line(&mut app, label);
    key(&mut app, KeyCode::Enter);
    app
}

#[test]
fn a_tag_explains_its_effects_and_lists_what_depends_on_it() {
    let mut app = app();
    let mut tag = AppService::new_tag();
    tag.name = "Lucky".into();
    tag.effects = serde_json::from_value(serde_json::json!([
        { "kind": "plus", "table": "table.crime", "amount": 2 }
    ]))
    .unwrap();
    let id = tag.id.clone();
    app.ctx.service.save_tag(tag).unwrap();
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    select_line(&mut app, "View Tags");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('/'));
    type_text(&mut app, id.as_str());
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]);
    let screen = text_at(&mut app, 140, 60);
    assert!(
        screen.contains("While Lucky is active: adds 2 to the roll of Crime"),
        "{screen}"
    );
    assert!(screen.contains("Nothing uses this tag"), "{screen}");
    // The effect's own page shows the roll step by step.
    select_line(&mut app, "1. Plus");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("IN PLAIN WORDS"), "{screen}");
    assert!(
        screen.contains("Roll 1d12 → add 2 → cap at 12 → resolve result."),
        "{screen}"
    );
}

#[test]
fn bare_minimum_buildings_shows_its_dependencies_and_cannot_be_deleted() {
    let mut app = open_settings_row("View Generation Settings");
    let screen = text_at(&mut app, 140, 40);
    assert!(screen.contains("setting.bareMinimumBuildings"), "{screen}");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("DEPENDENCIES"), "{screen}");
    assert!(screen.contains("Notable Locations"), "{screen}");
    select_line(&mut app, "Delete");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Left); // Delete, not Keep
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("still used"), "{screen}");
}

#[test]
fn a_number_setting_is_created_and_offered_in_settings() {
    let mut app = open_settings_row("View Generation Settings");
    key(&mut app, KeyCode::Char('n'));
    pick(&mut app, 1); // Number
    select_line(&mut app, "Name");
    key(&mut app, KeyCode::Enter);
    keys(&mut app, &[KeyCode::End]);
    for _ in 0.."New setting".len() {
        key(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "Districts");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('s'));
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Saved"), "{screen}");
    // Back to Settings: the new setting is offered and ←→ changes it.
    keys(&mut app, &[KeyCode::Esc, KeyCode::Esc]);
    select_line(&mut app, "Districts");
    key(&mut app, KeyCode::Right);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("[ 2 ]"), "{screen}");
}

// ----- testing, tracing, sharing and dependencies --------------------------------------

#[test]
fn a_table_runs_many_times_from_its_editor() {
    let mut app = open_table("table.crime");
    select_line(&mut app, "Test this table");
    key(&mut app, KeyCode::Enter);
    assert!(text(&mut app).contains("Test › Crime"));
    select_line(&mut app, "Run 1,000");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Crime: 1000 rolls"), "{screen}");
    assert!(screen.contains("EXPECTED"), "{screen}");
    // Step through reveals one line at a time.
    select_line(&mut app, "Step through");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("Next step"), "{screen}");
    assert!(!screen.contains("Result:"), "{screen}");
    for _ in 0..12 {
        key(&mut app, KeyCode::Enter);
    }
    assert!(text_at(&mut app, 140, 60).contains("Result:"));
}

#[test]
fn roll_details_explain_generation_when_switched_on() {
    let mut app = app();
    let mut s = app.ctx.settings;
    s.roll_details = true;
    s.generation_method = GenerationMethod::Manual;
    app.ctx.save_settings(s).unwrap();
    let mut s = app.ctx.settings;
    s.generation_presentation = GenerationPresentation::OneTable;
    app.ctx.save_settings(s).unwrap();
    key(&mut app, KeyCode::Enter); // Start
    keys(&mut app, &[KeyCode::Enter, KeyCode::Enter]); // pick Kurovia
    key(&mut app, KeyCode::Char('a')); // rolls every remaining stage
    select_row(&mut app, "Crime");
    let screen = text_at(&mut app, 140, 60);
    assert!(screen.contains("ROLL DETAILS"), "{screen}");
    assert!(screen.contains("Base roll:"), "{screen}");
    assert!(screen.contains("Result:"), "{screen}");
}

fn temp_exports(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sg-tui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn content_is_exported_and_imported_with_a_choice_for_taken_ids() {
    let mut app = app();
    let dir = temp_exports("transfer");
    app.ctx.exports = Some(dir.clone());
    let mut crime = app
        .ctx
        .service
        .table_detail(&settlement_core::ids::TableId::new("table.crime"))
        .unwrap()
        .table;
    crime.name = "Crime (shared)".into();
    app.ctx.service.save_table(crime).unwrap();

    keys(&mut app, &[KeyCode::Down, KeyCode::Enter]); // Settings
    select_line(&mut app, "Import / Export");
    key(&mut app, KeyCode::Enter);
    select_line(&mut app, "Export everything I made or changed");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 160, 40);
    assert!(screen.contains("Saved to"), "{screen}");
    assert!(dir.join("My content.json").exists());

    // Back to the defaults, then bring the change back from the file.
    app.ctx.service.reset_all().unwrap();
    select_line(&mut app, "Import My content.json");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 160, 40);
    assert!(
        screen.contains("add as a new copy") || screen.contains("Table “Crime (shared)”"),
        "{screen}"
    );
    select_line(&mut app, "If an ID is already used");
    key(&mut app, KeyCode::Enter);
    pick(&mut app, 1); // Replace mine
    select_line(&mut app, "Import");
    key(&mut app, KeyCode::Enter);
    let screen = text_at(&mut app, 160, 40);
    assert!(screen.contains("Imported:"), "{screen}");
    let name = app
        .ctx
        .service
        .table_detail(&settlement_core::ids::TableId::new("table.crime"))
        .unwrap()
        .table
        .name;
    assert_eq!(name, "Crime (shared)");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_dependency_opens_the_item_that_uses_it() {
    let mut app = open_table("table.size");
    // Size is used by the pipeline (its Advanced section lists uses).
    select_line(&mut app, "Advanced (show)");
    key(&mut app, KeyCode::Enter);
    select_line(&mut app, "Pipeline stage");
    key(&mut app, KeyCode::Enter);
    let screen = text(&mut app);
    assert!(screen.contains("Flows › Settlement"), "{screen}");
    key(&mut app, KeyCode::Esc);
    assert!(text(&mut app).contains("ID table.size"));
}
