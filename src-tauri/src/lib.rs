//! Tauri shell. Commands are thin wrappers over the shared
//! [`AppService`]; no business logic belongs in this crate.

use std::sync::Mutex;

use settlement_core::AppInfo;
use settlement_core::content::{Table, Tag};
use settlement_core::dice::RandomFaces;
use settlement_core::editor::{self, EffectKindSchema};
use settlement_core::engine::{GenerationError, Selections};
use settlement_core::ids::{ResultId, TableId, TagId};
use settlement_core::layers::new_user_id;
use settlement_core::roadmap::RoadmapEntry;
use settlement_core::service::{
    ActiveGeneration, AppService, ContentOverview, DraftCheck, GenerationResponse,
    SavedSettlementView, ServiceResult, TableDetail, TagDetail,
};
use settlement_core::session::StageChange;
use settlement_core::settings::Settings;
use settlement_core::settlements::{SettlementId, SettlementSummary};
use tauri::{AppHandle, State};

mod updates;

/// Shared state: the service (or why it could not open) and the settlement
/// currently being generated or edited.
struct AppState {
    service: Mutex<Result<AppService, String>>,
    generation: Mutex<Option<ActiveGeneration>>,
}

impl AppState {
    fn with<T>(&self, f: impl FnOnce(&mut AppService) -> ServiceResult<T>) -> ServiceResult<T> {
        let mut guard = self.service.lock().map_err(|_| "service lock poisoned")?;
        let service = guard.as_mut().map_err(|e| e.clone())?;
        f(service)
    }

    /// Runs `action` on the active generation and returns the new view.
    fn act<T>(
        &self,
        action: impl FnOnce(&mut ActiveGeneration) -> Result<T, GenerationError>,
    ) -> ServiceResult<GenerationResponse> {
        let mut guard = self
            .generation
            .lock()
            .map_err(|_| "generation lock poisoned")?;
        let active = guard.as_mut().ok_or("No settlement is being generated.")?;
        let outcome = action(active);
        active.respond(outcome)
    }

    fn begin(&self, active: ActiveGeneration) -> ServiceResult<GenerationResponse> {
        let response = active.respond(Ok::<(), GenerationError>(()))?;
        *self
            .generation
            .lock()
            .map_err(|_| "generation lock poisoned")? = Some(active);
        Ok(response)
    }
}

#[tauri::command]
fn app_info() -> AppInfo {
    settlement_core::app_info()
}

#[tauri::command]
fn roadmap() -> Vec<RoadmapEntry> {
    settlement_core::roadmap::roadmap()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> ServiceResult<Settings> {
    state.with(|s| s.settings())
}

#[tauri::command]
fn save_settings(
    state: State<'_, AppState>,
    previous: Settings,
    settings: Settings,
) -> ServiceResult<Settings> {
    state.with(|s| s.change_settings(&previous, &settings))
}

// ----- generation ------------------------------------------------------------

#[tauri::command]
fn generation_start(state: State<'_, AppState>) -> ServiceResult<GenerationResponse> {
    let active = state.with(|s| s.start_generation())?;
    state.begin(active)
}

#[tauri::command]
fn generation_roll(state: State<'_, AppState>, stage: usize) -> ServiceResult<GenerationResponse> {
    state.act(|a| a.session.roll(stage, &mut RandomFaces::from_entropy()))
}

#[tauri::command]
fn generation_choose(
    state: State<'_, AppState>,
    stage: usize,
    result: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session.choose(
            stage,
            ResultId::new(result),
            &mut RandomFaces::from_entropy(),
        )
    })
}

/// Names the settlement with a typed name (the name stage).
#[tauri::command]
fn generation_custom_name(
    state: State<'_, AppState>,
    stage: usize,
    name: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| a.session.set_custom_name(stage, &name))
}

/// Rolls the District Count again (Cities and Capitals).
#[tauri::command]
fn generation_roll_district_count(
    state: State<'_, AppState>,
    stage: usize,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .roll_district_count(stage, &mut RandomFaces::from_entropy())
    })
}

/// Chooses the District Count.
#[tauri::command]
fn generation_choose_district_count(
    state: State<'_, AppState>,
    stage: usize,
    result: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session.choose_district_count(
            stage,
            ResultId::new(result),
            &mut RandomFaces::from_entropy(),
        )
    })
}

/// Rolls one area's Notable Location Count again.
#[tauri::command]
fn generation_roll_location_count(
    state: State<'_, AppState>,
    stage: usize,
    area: usize,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .roll_location_count(stage, area, &mut RandomFaces::from_entropy())
    })
}

/// Chooses one area's Notable Location Count.
#[tauri::command]
fn generation_choose_location_count(
    state: State<'_, AppState>,
    stage: usize,
    area: usize,
    result: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session.choose_location_count(
            stage,
            area,
            ResultId::new(result),
            &mut RandomFaces::from_entropy(),
        )
    })
}

/// Rolls one Notable Location's Type again.
#[tauri::command]
fn generation_roll_location_type(
    state: State<'_, AppState>,
    stage: usize,
    location: usize,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .roll_location_type(stage, location, &mut RandomFaces::from_entropy())
    })
}

/// Chooses one Notable Location's Type.
#[tauri::command]
fn generation_choose_location_type(
    state: State<'_, AppState>,
    stage: usize,
    location: usize,
    result: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session.choose_location_type(
            stage,
            location,
            ResultId::new(result),
            &mut RandomFaces::from_entropy(),
        )
    })
}

/// Rolls one Authority Figure's role again (by position, lead first).
#[tauri::command]
fn generation_roll_role(
    state: State<'_, AppState>,
    stage: usize,
    slot: usize,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .roll_role(stage, slot, &mut RandomFaces::from_entropy())
    })
}

/// Chooses one Authority Figure's role.
#[tauri::command]
fn generation_choose_role(
    state: State<'_, AppState>,
    stage: usize,
    slot: usize,
    result: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session.choose_role(
            stage,
            slot,
            ResultId::new(result),
            &mut RandomFaces::from_entropy(),
        )
    })
}

/// Rolls one field of one Authority Figure again.
#[tauri::command]
fn generation_roll_figure_field(
    state: State<'_, AppState>,
    stage: usize,
    figure: usize,
    field: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .roll_figure_field(stage, figure, &field, &mut RandomFaces::from_entropy())
    })
}

/// Chooses one field of one Authority Figure.
#[tauri::command]
fn generation_choose_figure_field(
    state: State<'_, AppState>,
    stage: usize,
    figure: usize,
    field: String,
    result: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session.choose_figure_field(
            stage,
            figure,
            &field,
            ResultId::new(result),
            &mut RandomFaces::from_entropy(),
        )
    })
}

/// Generates one Authority Figure's name again.
#[tauri::command]
fn generation_roll_figure_name(
    state: State<'_, AppState>,
    stage: usize,
    figure: usize,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .roll_figure_name(stage, figure, &mut RandomFaces::from_entropy())
    })
}

/// Chooses one part of an Authority Figure's generated name.
#[tauri::command]
fn generation_choose_figure_name_part(
    state: State<'_, AppState>,
    stage: usize,
    figure: usize,
    part: usize,
    result: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session.choose_figure_name_part(
            stage,
            figure,
            part,
            ResultId::new(result),
            &mut RandomFaces::from_entropy(),
        )
    })
}

/// Rolls one part of an Authority Figure's generated name again.
#[tauri::command]
fn generation_roll_figure_name_part(
    state: State<'_, AppState>,
    stage: usize,
    figure: usize,
    part: usize,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .roll_figure_name_part(stage, figure, part, &mut RandomFaces::from_entropy())
    })
}

/// Names one Authority Figure with a typed name.
#[tauri::command]
fn generation_figure_name(
    state: State<'_, AppState>,
    stage: usize,
    figure: usize,
    name: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| a.session.set_figure_name(stage, figure, &name))
}

/// Rolls one relationship between Authority Figures again.
#[tauri::command]
fn generation_roll_relationship(
    state: State<'_, AppState>,
    stage: usize,
    relationship: usize,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .roll_relationship(stage, relationship, &mut RandomFaces::from_entropy())
    })
}

/// Chooses one relationship between Authority Figures.
#[tauri::command]
fn generation_choose_relationship(
    state: State<'_, AppState>,
    stage: usize,
    relationship: usize,
    result: String,
) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session.choose_relationship(
            stage,
            relationship,
            ResultId::new(result),
            &mut RandomFaces::from_entropy(),
        )
    })
}

#[tauri::command]
fn generation_advance(state: State<'_, AppState>) -> ServiceResult<GenerationResponse> {
    state.act(|a| a.session.advance())
}

#[tauri::command]
fn generation_back(state: State<'_, AppState>) -> ServiceResult<GenerationResponse> {
    state.act(|a| Ok::<usize, GenerationError>(a.session.back()))
}

#[tauri::command]
fn generation_roll_remaining(state: State<'_, AppState>) -> ServiceResult<GenerationResponse> {
    state.act(|a| -> Result<Vec<StageChange>, GenerationError> {
        a.session.roll_remaining(&mut RandomFaces::from_entropy())
    })
}

#[tauri::command]
fn generation_run_automatic(state: State<'_, AppState>) -> ServiceResult<GenerationResponse> {
    state.act(|a| {
        a.session
            .run_automatic(&Selections::new(), &mut RandomFaces::from_entropy())
    })
}

#[tauri::command]
fn generation_reroll_all(state: State<'_, AppState>) -> ServiceResult<GenerationResponse> {
    state.act(|a| a.session.reroll_all(&mut RandomFaces::from_entropy()))
}

#[tauri::command]
fn generation_discard(state: State<'_, AppState>) -> ServiceResult<()> {
    *state
        .generation
        .lock()
        .map_err(|_| "generation lock poisoned")? = None;
    Ok(())
}

#[tauri::command]
fn generation_save(state: State<'_, AppState>, name: String) -> ServiceResult<SettlementSummary> {
    let mut guard = state
        .generation
        .lock()
        .map_err(|_| "generation lock poisoned")?;
    let active = guard.as_ref().ok_or("No settlement is being generated.")?;
    let saved = state.with(|s| s.save_new(active, &name))?;
    *guard = None;
    Ok(saved)
}

// ----- saved settlements -----------------------------------------------------

#[tauri::command]
fn saved_list(state: State<'_, AppState>) -> ServiceResult<Vec<SettlementSummary>> {
    state.with(|s| s.saved_list())
}

#[tauri::command]
fn saved_get(state: State<'_, AppState>, id: String) -> ServiceResult<SavedSettlementView> {
    state.with(|s| s.saved_get(&SettlementId::from(id.as_str())))
}

#[tauri::command]
fn saved_set_favourite(
    state: State<'_, AppState>,
    id: String,
    favourite: bool,
) -> ServiceResult<SavedSettlementView> {
    state.with(|s| s.saved_set_favourite(&SettlementId::from(id.as_str()), favourite))
}

#[tauri::command]
fn saved_delete(state: State<'_, AppState>, id: String) -> ServiceResult<()> {
    state.with(|s| s.saved_delete(&SettlementId::from(id.as_str())))
}

#[tauri::command]
fn saved_edit_start(state: State<'_, AppState>, id: String) -> ServiceResult<GenerationResponse> {
    let active = state.with(|s| s.start_edit(&SettlementId::from(id.as_str())))?;
    state.begin(active)
}

#[tauri::command]
fn saved_edit_save(state: State<'_, AppState>, name: String) -> ServiceResult<SavedSettlementView> {
    let mut guard = state
        .generation
        .lock()
        .map_err(|_| "generation lock poisoned")?;
    let active = guard.as_ref().ok_or("No settlement is being edited.")?;
    let saved = state.with(|s| s.save_edit(active, &name))?;
    *guard = None;
    Ok(saved)
}

// ----- content editors -------------------------------------------------------

#[tauri::command]
fn content_overview(state: State<'_, AppState>) -> ServiceResult<ContentOverview> {
    state.with(|s| s.overview())
}

#[tauri::command]
fn content_table(state: State<'_, AppState>, id: String) -> ServiceResult<TableDetail> {
    state.with(|s| s.table_detail(&TableId::new(id)))
}

#[tauri::command]
fn content_tag(state: State<'_, AppState>, id: String) -> ServiceResult<TagDetail> {
    state.with(|s| s.tag_detail(&TagId::new(id)))
}

#[tauri::command]
fn content_check_table(state: State<'_, AppState>, table: Table) -> ServiceResult<DraftCheck> {
    state.with(|s| s.check_table(table))
}

#[tauri::command]
fn content_check_tag(state: State<'_, AppState>, tag: Tag) -> ServiceResult<DraftCheck> {
    state.with(|s| s.check_tag(tag))
}

#[tauri::command]
fn content_save_table(state: State<'_, AppState>, table: Table) -> ServiceResult<TableDetail> {
    state.with(|s| s.save_table(table))
}

#[tauri::command]
fn content_save_tag(state: State<'_, AppState>, tag: Tag) -> ServiceResult<TagDetail> {
    state.with(|s| s.save_tag(tag))
}

#[tauri::command]
fn content_new_table() -> Table {
    AppService::new_table()
}

#[tauri::command]
fn content_new_tag() -> Tag {
    AppService::new_tag()
}

#[tauri::command]
fn content_new_id(kind: String) -> String {
    new_user_id(&kind)
}

#[tauri::command]
fn content_delete_table(state: State<'_, AppState>, id: String) -> ServiceResult<ContentOverview> {
    state.with(|s| s.delete_table(&TableId::new(id)))
}

#[tauri::command]
fn content_delete_tag(state: State<'_, AppState>, id: String) -> ServiceResult<ContentOverview> {
    state.with(|s| s.delete_tag(&TagId::new(id)))
}

#[tauri::command]
fn content_reset_table(state: State<'_, AppState>, id: String) -> ServiceResult<ContentOverview> {
    state.with(|s| s.reset_table(&TableId::new(id)))
}

#[tauri::command]
fn content_reset_tag(state: State<'_, AppState>, id: String) -> ServiceResult<ContentOverview> {
    state.with(|s| s.reset_tag(&TagId::new(id)))
}

#[tauri::command]
fn content_reset_all(state: State<'_, AppState>) -> ServiceResult<ContentOverview> {
    state.with(|s| s.reset_all())
}

#[tauri::command]
fn effect_schema() -> Vec<EffectKindSchema> {
    editor::effect_schema()
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::PendingUpdate::default())
        .manage(AppState {
            service: Mutex::new(AppService::open_default()),
            generation: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            roadmap,
            get_settings,
            save_settings,
            generation_start,
            generation_roll,
            generation_choose,
            generation_custom_name,
            generation_roll_district_count,
            generation_choose_district_count,
            generation_roll_location_count,
            generation_choose_location_count,
            generation_roll_location_type,
            generation_choose_location_type,
            generation_roll_role,
            generation_choose_role,
            generation_roll_figure_field,
            generation_choose_figure_field,
            generation_roll_figure_name,
            generation_figure_name,
            generation_choose_figure_name_part,
            generation_roll_figure_name_part,
            generation_roll_relationship,
            generation_choose_relationship,
            generation_advance,
            generation_back,
            generation_roll_remaining,
            generation_run_automatic,
            generation_reroll_all,
            generation_discard,
            generation_save,
            saved_list,
            saved_get,
            saved_set_favourite,
            saved_delete,
            saved_edit_start,
            saved_edit_save,
            content_overview,
            content_table,
            content_tag,
            content_check_table,
            content_check_tag,
            content_save_table,
            content_save_tag,
            content_new_table,
            content_new_tag,
            content_new_id,
            content_delete_table,
            content_delete_tag,
            content_reset_table,
            content_reset_tag,
            content_reset_all,
            effect_schema,
            exit_app,
            updates::update_check,
            updates::update_install
        ])
        .run(tauri::generate_context!())
        .expect("error while running Settlement Generator");
}
