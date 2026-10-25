//! Tauri shell. Commands are thin wrappers over the shared
//! [`AppService`]; no business logic belongs in this crate.

use std::sync::Mutex;

use settlement_core::AppInfo;
use settlement_core::content::{Condition, EffectDef, Modifier};
use settlement_core::content::{Table, Tag};
use settlement_core::dice::RandomFaces;
use settlement_core::dice_assist::{DiceAdvice, DiceFit};
use settlement_core::editor::{self, EffectKindSchema};
use settlement_core::engine::{GenerationError, Selections};
use settlement_core::explain::Explanation;
use settlement_core::framework::SettingDef;
use settlement_core::framework::describe::FlowContext;
use settlement_core::framework::describe::SettingType;
use settlement_core::framework::edit::FlowEdit;
use settlement_core::framework::layers::ItemKind;
use settlement_core::framework::{Flow, SettingValue};
use settlement_core::ids::{FlowId, ResultId, SettingId, TableId, TagId};
use settlement_core::layers::new_user_id;
use settlement_core::roadmap::RoadmapEntry;
use settlement_core::service::{
    ActiveGeneration, AppService, ContentOverview, ContentSettingView, DraftCheck, ExportScope,
    FlowCheck, FrameworkItemListing, GenerationResponse, PreviewRoll, SavedSettlementView,
    ServiceResult, SettingDetail, TableDetail, TagDetail, TagInsight,
};
use settlement_core::session::StageChange;
use settlement_core::settings::Settings;
use settlement_core::settlements::{SettlementId, SettlementSummary};
use settlement_core::simulate::{self, FlowPreview, FlowSimulation, TableSimulation};
use settlement_core::table_editor::Usage as ItemUsage;
use settlement_core::table_editor::Usage;
use settlement_core::transfer::{Collision, ContentPack, ImportPlan, PackItem, read_pack};
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
fn content_dice_advice(weights: Vec<u32>) -> DiceAdvice {
    AppService::dice_advice(&weights)
}

#[tauri::command]
fn content_dice_fit(weights: Vec<u32>, dice: String) -> ServiceResult<DiceFit> {
    AppService::dice_fit(&weights, &dice)
}

#[tauri::command]
fn content_apply_dice_fit(
    table: Table,
    entries: Vec<ResultId>,
    fit: DiceFit,
) -> ServiceResult<Table> {
    AppService::apply_dice_fit(&table, &entries, &fit)
}

#[tauri::command]
fn content_preview_roll(state: State<'_, AppState>, table: Table) -> ServiceResult<PreviewRoll> {
    state.with(|s| s.preview_roll(table))
}

#[tauri::command]
fn content_copy_table(state: State<'_, AppState>, id: String) -> ServiceResult<Table> {
    state.with(|s| s.copy_table(&TableId::from(id.as_str())))
}

#[tauri::command]
fn content_result_usages(
    state: State<'_, AppState>,
    table: Table,
    result: String,
) -> ServiceResult<Vec<Usage>> {
    state.with(|s| s.result_usages(&table, &ResultId::from(result.as_str())))
}

#[tauri::command]
fn content_save_table_draft(state: State<'_, AppState>, table: Table) -> ServiceResult<()> {
    state.with(|s| s.save_table_draft(&table))
}

#[tauri::command]
fn content_table_draft(state: State<'_, AppState>, id: String) -> ServiceResult<Option<Table>> {
    state.with(|s| s.table_draft(&TableId::from(id.as_str())))
}

#[tauri::command]
fn content_discard_table_draft(state: State<'_, AppState>, id: String) -> ServiceResult<()> {
    state.with(|s| s.discard_table_draft(&TableId::from(id.as_str())))
}

#[tauri::command]
fn content_settings(state: State<'_, AppState>) -> ServiceResult<Vec<ContentSettingView>> {
    state.with(|s| s.content_settings())
}

#[tauri::command]
fn content_set_setting(
    state: State<'_, AppState>,
    id: String,
    value: SettingValue,
) -> ServiceResult<Vec<ContentSettingView>> {
    state.with(|s| s.set_content_setting(&id.as_str().into(), value))
}

#[tauri::command]
fn content_toggle_settings(
    state: State<'_, AppState>,
) -> ServiceResult<Vec<settlement_core::engine::Named<settlement_core::ids::SettingId>>> {
    state.with(|s| s.toggle_settings())
}

#[tauri::command]
fn framework_items(state: State<'_, AppState>) -> ServiceResult<Vec<FrameworkItemListing>> {
    state.with(|s| s.framework_items())
}

#[tauri::command]
fn framework_item(
    state: State<'_, AppState>,
    kind: ItemKind,
    id: String,
) -> ServiceResult<serde_json::Value> {
    state.with(|s| s.framework_item(kind, &id))
}

#[tauri::command]
fn framework_save_item(
    state: State<'_, AppState>,
    kind: ItemKind,
    item: serde_json::Value,
) -> ServiceResult<Vec<FrameworkItemListing>> {
    state.with(|s| s.save_framework_item(kind, item))
}

#[tauri::command]
fn framework_delete_item(
    state: State<'_, AppState>,
    kind: ItemKind,
    id: String,
) -> ServiceResult<Vec<FrameworkItemListing>> {
    state.with(|s| s.delete_framework_item(kind, &id))
}

#[tauri::command]
fn framework_reset_item(
    state: State<'_, AppState>,
    kind: ItemKind,
    id: String,
) -> ServiceResult<Vec<FrameworkItemListing>> {
    state.with(|s| s.reset_framework_item(kind, &id))
}

#[tauri::command]
fn flow_get(state: State<'_, AppState>, id: String) -> ServiceResult<Flow> {
    state.with(|s| s.flow(&FlowId::from(id.as_str())))
}

#[tauri::command]
fn flow_new() -> Flow {
    AppService::new_flow()
}

#[tauri::command]
fn flow_duplicate(state: State<'_, AppState>, id: String) -> ServiceResult<Flow> {
    state.with(|s| s.duplicate_flow(&FlowId::from(id.as_str())))
}

#[tauri::command]
fn flow_edit(flow: Flow, edit: FlowEdit) -> ServiceResult<Flow> {
    AppService::edit_flow(&flow, edit)
}

#[tauri::command]
fn flow_check(state: State<'_, AppState>, flow: Flow) -> ServiceResult<FlowCheck> {
    state.with(|s| s.check_flow(&flow))
}

#[tauri::command]
fn flow_context(state: State<'_, AppState>, flow: Flow) -> ServiceResult<FlowContext> {
    state.with(|s| s.flow_context(&flow))
}

#[tauri::command]
fn flow_describe_condition(
    state: State<'_, AppState>,
    flow: Flow,
    condition: Condition,
) -> ServiceResult<String> {
    state.with(|s| s.describe_condition(&flow, &condition))
}

#[tauri::command]
fn flow_save(state: State<'_, AppState>, flow: Flow) -> ServiceResult<Flow> {
    state.with(|s| s.save_flow(flow))
}

#[tauri::command]
fn flow_save_draft(state: State<'_, AppState>, flow: Flow) -> ServiceResult<()> {
    state.with(|s| s.save_flow_draft(&flow))
}

#[tauri::command]
fn flow_draft(state: State<'_, AppState>, id: String) -> ServiceResult<Option<Flow>> {
    state.with(|s| s.flow_draft(&FlowId::from(id.as_str())))
}

#[tauri::command]
fn flow_discard_draft(state: State<'_, AppState>, id: String) -> ServiceResult<()> {
    state.with(|s| s.discard_flow_draft(&FlowId::from(id.as_str())))
}

#[tauri::command]
fn tag_explain(state: State<'_, AppState>, tag: Tag) -> ServiceResult<TagInsight> {
    state.with(|s| s.explain_tag(&tag))
}

#[tauri::command]
fn effect_explain(
    state: State<'_, AppState>,
    tag: Option<Tag>,
    effect: EffectDef,
) -> ServiceResult<Explanation> {
    state.with(|s| s.explain_effect(tag.as_ref(), &effect))
}

#[tauri::command]
fn modifier_explain(
    state: State<'_, AppState>,
    table: String,
    modifier: Modifier,
) -> ServiceResult<Explanation> {
    state.with(|s| s.explain_modifier(&TableId::from(table.as_str()), modifier))
}

#[tauri::command]
fn setting_detail(state: State<'_, AppState>, id: String) -> ServiceResult<SettingDetail> {
    state.with(|s| s.setting_detail(&SettingId::from(id.as_str())))
}

#[tauri::command]
fn setting_new(kind: SettingType) -> SettingDef {
    AppService::new_setting(kind)
}

#[tauri::command]
fn setting_check(state: State<'_, AppState>, setting: SettingDef) -> ServiceResult<DraftCheck> {
    state.with(|s| s.check_setting(&setting))
}

#[tauri::command]
fn setting_save(state: State<'_, AppState>, setting: SettingDef) -> ServiceResult<SettingDetail> {
    state.with(|s| s.save_setting(setting))
}

#[tauri::command]
fn setting_delete(
    state: State<'_, AppState>,
    id: String,
) -> ServiceResult<Vec<FrameworkItemListing>> {
    state.with(|s| s.delete_setting(&SettingId::from(id.as_str())))
}

/// Runs `job` off the interface thread (simulations can take seconds).
async fn in_background<T: Send + 'static>(
    job: impl FnOnce() -> ServiceResult<T> + Send + 'static,
) -> ServiceResult<T> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn content_simulate_table(
    state: State<'_, AppState>,
    table: Table,
    runs: u32,
    tags: Vec<String>,
) -> ServiceResult<TableSimulation> {
    let content = state.with(|s| s.content_with_table(&table))?;
    let tags: Vec<TagId> = tags.iter().map(|t| TagId::from(t.as_str())).collect();
    in_background(move || simulate::simulate_table(&content, &table.id, runs, &tags, None)).await
}

#[tauri::command]
async fn flow_simulate(
    state: State<'_, AppState>,
    flow: Flow,
    runs: u32,
) -> ServiceResult<FlowSimulation> {
    let content = state.with(|s| s.content_with_flow(&flow))?;
    in_background(move || simulate::simulate_flow(&content, &flow, runs, None)).await
}

#[tauri::command]
fn flow_preview(state: State<'_, AppState>, flow: Flow) -> ServiceResult<FlowPreview> {
    state.with(|s| s.preview_flow(&flow))
}

#[tauri::command]
fn flow_usages(state: State<'_, AppState>, id: String) -> ServiceResult<Vec<ItemUsage>> {
    state.with(|s| s.flow_usages(&FlowId::from(id.as_str())))
}

#[tauri::command]
fn flow_delete(state: State<'_, AppState>, id: String) -> ServiceResult<Vec<FrameworkItemListing>> {
    state.with(|s| s.delete_flow(&FlowId::from(id.as_str())))
}

#[tauri::command]
fn content_export_items(
    state: State<'_, AppState>,
    items: Vec<PackItem>,
    with_dependencies: bool,
    name: String,
) -> ServiceResult<ContentPack> {
    state.with(|s| s.export_items(&items, with_dependencies, &name))
}

#[tauri::command]
fn content_export_everything(
    state: State<'_, AppState>,
    scope: ExportScope,
) -> ServiceResult<ContentPack> {
    state.with(|s| s.export_everything(scope))
}

/// Writes a pack to the exports folder and returns where.
#[tauri::command]
fn content_write_pack(pack: ContentPack, file_name: String) -> ServiceResult<String> {
    AppService::write_pack(&pack, &file_name, None).map(|p| p.display().to_string())
}

#[tauri::command]
fn content_exports_folder() -> Option<String> {
    settlement_core::storage::exports_dir().map(|p| p.display().to_string())
}

#[tauri::command]
fn content_plan_import(
    state: State<'_, AppState>,
    text: String,
    policy: Collision,
) -> ServiceResult<ImportPlan> {
    let pack = read_pack(&text)?;
    state.with(|s| s.plan_import(&pack, policy))
}

#[tauri::command]
fn content_import(
    state: State<'_, AppState>,
    text: String,
    policy: Collision,
) -> ServiceResult<ImportPlan> {
    let pack = read_pack(&text)?;
    state.with(|s| s.import_pack(&pack, policy))
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
            content_dice_advice,
            content_dice_fit,
            content_apply_dice_fit,
            content_preview_roll,
            content_copy_table,
            content_result_usages,
            content_save_table_draft,
            content_table_draft,
            content_discard_table_draft,
            content_settings,
            content_set_setting,
            content_toggle_settings,
            framework_items,
            framework_item,
            framework_save_item,
            framework_delete_item,
            framework_reset_item,
            flow_get,
            tag_explain,
            content_simulate_table,
            flow_simulate,
            flow_preview,
            flow_usages,
            flow_delete,
            content_export_items,
            content_export_everything,
            content_write_pack,
            content_exports_folder,
            content_plan_import,
            content_import,
            effect_explain,
            modifier_explain,
            setting_detail,
            setting_new,
            setting_check,
            setting_save,
            setting_delete,
            flow_new,
            flow_duplicate,
            flow_edit,
            flow_check,
            flow_context,
            flow_describe_condition,
            flow_save,
            flow_save_draft,
            flow_draft,
            flow_discard_draft,
            effect_schema,
            exit_app,
            updates::update_check,
            updates::update_install
        ])
        .run(tauri::generate_context!())
        .expect("error while running Settlement Generator");
}
