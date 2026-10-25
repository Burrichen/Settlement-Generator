// Single typed entry point for calls into the shared Rust core.
// Types mirror the serialised structs in `settlement-core`.
import { Channel, invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  /** Raw semantic version without a `v` prefix, e.g. `0.1.0`. */
  version: string;
}

export interface RoadmapEntry {
  horizon: "next_minor" | "next_major";
  label: string;
  summary: string;
}

export type GenerationMethod = "automatic" | "manual";
export type GenerationPresentation = "incremental" | "one_table";
export type Theme = "modern_dark" | "old_timey_dark";

export interface Settings {
  generation_method: GenerationMethod;
  /** Kept while Automatic, but disabled and ignored. */
  generation_presentation: GenerationPresentation;
  god_mode: boolean;
  tag_view: boolean;
  theme: Theme;
  /** Every settlement gets one extra Notable Location: a Tavern. */
  bare_minimum_buildings: boolean;
  /** Advanced: show how every generated value was reached. */
  roll_details?: boolean;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

export function getRoadmap(): Promise<RoadmapEntry[]> {
  return invoke<RoadmapEntry[]>("roadmap");
}

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/**
 * Saves the change from `previous` (what the screen showed) to `settings` and
 * returns what is now stored. Only changed fields are written, so a change
 * made meanwhile in the terminal interface is kept.
 */
export function saveSettings(previous: Settings, settings: Settings): Promise<Settings> {
  return invoke<Settings>("save_settings", { previous, settings });
}

export function exitApp(): Promise<void> {
  return invoke<void>("exit_app");
}

// ----- updates -----------------------------------------------------------

/** A newer stable release, found on GitHub Releases. */
export interface UpdateOffer {
  current_version: string;
  version: string;
  notes: string | null;
}

export type InstallProgress =
  | { stage: "downloading"; downloaded: number; total: number | null }
  | { stage: "backing_up" }
  | { stage: "installing" };

/** Checks for a newer stable version; `null` when up to date. */
export function checkForUpdate(): Promise<UpdateOffer | null> {
  return invoke<UpdateOffer | null>("update_check");
}

/**
 * Installs the offered update: download and verify, back up user data, then
 * run the installer, which closes and restarts the app.
 */
export function installUpdate(onProgress: (progress: InstallProgress) => void): Promise<void> {
  const channel = new Channel<InstallProgress>();
  channel.onmessage = onProgress;
  return invoke<void>("update_install", { onProgress: channel });
}

/** Display form of a version, e.g. `v0.1.0`. */
export function displayVersion(info: AppInfo): string {
  return `v${info.version}`;
}

// ----- generation --------------------------------------------------------

/** An ID with the display name it had when recorded. */
export interface Named {
  id: string;
  name: string;
}

export type GenerationMode = "automatic" | "incremental" | "one_table";
export type StageStatus = "pending" | "resolved" | "skipped";
export type ChoiceReason = "disabled" | "ineligible" | "restricted" | "not_in_layout";

export interface ResultOption {
  result: Named;
  /** Faces on the current layout: one face or `[min, max]`. */
  range?: number | [number, number];
  /** Tags the result would emit (empty unless Tag View is on). */
  tags: Named[];
  legal: boolean;
  reason?: ChoiceReason;
}

export interface FollowUpOutcome {
  name: string;
  dice: string;
  roll: string;
  becomes?: string;
}

export interface StageOutcome {
  chosen: boolean;
  dice?: string;
  roll?: string;
  rerolls: number;
  follow_ups: FollowUpOutcome[];
}

export interface StageView {
  index: number;
  /** Pipeline stage, labelled e.g. "Region". */
  stage: Named;
  /** Table that applies now, e.g. "Kurovia Region". */
  table: Named;
  status: StageStatus;
  current: boolean;
  actionable: boolean;
  can_roll: boolean;
  /** Dice a roll would use now, e.g. `1d20`. */
  dice?: string;
  incomplete?: string;
  options: ResultOption[];
  result?: Named;
  previous?: Named;
  /**
   * Why the stage was skipped. On the name stage: no naming scheme applies
   * yet (`no_naming_scheme`), or the settlement was saved before naming
   * existed (`not_recorded`).
   */
  skipped?: "inactive" | "not_eligible" | "no_naming_scheme" | "not_recorded";
  outcome?: StageOutcome;
  /** Emitted tags (only with Tag View). */
  tags?: Named[];
  /**
   * Only on the name stage. There `options` are the naming methods
   * (choosing one generates a name with it), `result` is the name, and
   * rolling generates a name.
   */
  naming?: NamingView;
  /** Elaborates the stage before it; left out where it does not apply. */
  sub_stage?: boolean;
  /** Present on the Authority Figures stage only. */
  figures?: FiguresView;
  /** Present on the Role Allocation stage only. */
  roles?: RolesView;
  /** Present on the Notable Locations stage only. */
  locations?: LocationsView;
  /** Present on flow stages (repeats, branches, objects, subflows). */
  flow?: FlowStageView;
  /** Every roll of the stage explained (only with Roll details). */
  trace?: RollTrace[];
}

/** One roll explained, line by line, e.g. "Base roll: 10", "Result: High". */
export interface RollTrace {
  title: string;
  lines: string[];
}

/** A flow stage: what its step produced, once run. */
export interface FlowStageView {
  record?: StepRecord;
  /** The record as indented lines, laid out by the core. */
  outline?: OutlineLine[];
}

/**
 * What a flow step produced. Interfaces show it through `outline`, which
 * the core computes; the rest is kept for completeness.
 */
export interface StepRecord {
  step: string;
  label: string;
  kind: string;
  [field: string]: unknown;
}

/** One indented line of a flow record, as the core lays it out. */
export interface OutlineLine {
  depth: number;
  label: string;
  value?: string;
}

/** A table roll as recorded (the parts the interface shows). */
export interface RollRecord {
  table: Named;
  dice?: string;
  selected?: boolean;
  attempts?: { roll: { total: number } }[];
  result: Named;
  tags?: Named[];
}

export interface FigureField {
  field: string;
  roll: RollRecord;
}

export interface NamePartRecord {
  id: string;
  label: string;
  result: Named;
}

export type FigureName =
  | {
      source: "generated";
      name: string;
      species?: Named;
      scheme: Named;
      method: Named;
      parts?: NamePartRecord[];
      rolls: RollRecord[];
    }
  | { source: "custom"; name: string }
  | { source: "missing"; reason: string };

export type Allocation = "single" | "lead" | "member";

export interface LabelVariant {
  tag: string;
  name: string;
}

/** A figure's Role / Title. */
export interface FigureRole {
  allocation: Allocation;
  /** The stable role and its neutral title. */
  role: Named;
  /** The title as shown for this figure (e.g. "Lady"). */
  label: string;
  label_variants?: LabelVariant[];
}

export interface RoleAssignment {
  allocation: Allocation;
  roll: RollRecord;
  /** The result rolled or chosen. */
  initial: Named;
  /** The role given instead, when a unique role was already held. */
  fallback?: Named;
  role: Named;
  label_variants?: LabelVariant[];
}

export interface RoleAllocation {
  request: AuthorityFigures["request"];
  scheme: Named;
  roles: RoleAssignment[];
  app_version: string;
}

export interface RoleSlotOptions {
  allocation: Allocation;
  table: Named;
  options: ResultOption[];
}

export interface RolesView {
  record?: RoleAllocation;
  slots: RoleSlotOptions[];
}

export type LocationSource = "rolled" | "bareMinimumBuildings";

export interface CountRoll {
  roll: RollRecord;
  count: number;
}

export interface District {
  id: string;
  label: string;
}

export interface LocationArea {
  district?: District;
  count: CountRoll;
}

export interface NotableLocation {
  id: string;
  source: LocationSource;
  district?: string;
  type: Named;
  type_roll?: RollRecord;
  subtype?: Named;
  spare_rooms?: number;
}

export interface NotableLocations {
  size: Named;
  scheme: Named;
  districts?: CountRoll;
  areas: LocationArea[];
  /** The guaranteed Tavern first (if any), then the rolled locations. */
  locations: NotableLocation[];
  /** The highest rolled-location number issued (IDs are never reused). */
  issued: number;
  app_version: string;
}

export interface TableOptions {
  table: Named;
  options: ResultOption[];
}

export interface LocationsView {
  record?: NotableLocations;
  options: { districts?: TableOptions; count?: TableOptions; types?: TableOptions };
  bare_minimum_buildings: boolean;
}

export interface AuthorityFigure {
  id: string;
  /** The name, else "Figure n". */
  label: string;
  role?: FigureRole;
  fields: FigureField[];
  name: FigureName;
  /** The figure's own tags; only sent with Tag View. */
  tags?: Named[];
  app_version: string;
}

export interface Relationship {
  a: string;
  b: string;
  roll: RollRecord;
}

export interface AuthorityFigures {
  request: { authority: Named; representation: "single" | "group"; size: number };
  figures: AuthorityFigure[];
  relationships?: Relationship[];
  app_version: string;
}

export interface FigureFieldOptions {
  field: string;
  label: string;
  table: Named;
  options: ResultOption[];
}

export interface NamePartOptions {
  part: number;
  id: string;
  label: string;
  table: Named;
  options: ResultOption[];
}

export interface FiguresView {
  record?: AuthorityFigures;
  fields: FigureFieldOptions[];
  relationship_options: ResultOption[];
  /** Per figure: the parts of its generated name and their results. */
  name_parts?: NamePartOptions[][];
}

export type NameSource = "generated" | "custom";

export interface NamingView {
  /** The scheme of the current name, or the one that applies now. */
  scheme?: Named;
  name?: string;
  source?: NameSource;
  method?: Named;
  /** Each rolled part (only with Tag View). */
  parts?: NamePartView[];
  /** Active tags that selected the scheme (only with Tag View). */
  routing?: Named[];
}

export interface NamePartView {
  table: Named;
  result: Named;
  dice?: string;
  roll?: string;
}

export interface SessionView {
  mode: GenerationMode;
  god_mode: boolean;
  cursor: number;
  complete: boolean;
  next_pending?: number;
  stages: StageView[];
  /** Active tags (only with Tag View). */
  active_tags?: Named[];
  /** The name stage's name, offered when saving. */
  suggested_name?: string;
}

export interface GenerationResponse {
  view: SessionView;
  /** Why the action could not be completed, in plain language. */
  error: string | null;
}

export interface SettlementSummary {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  app_version: string;
  favourite: boolean;
}

export const generation = {
  start: () => invoke<GenerationResponse>("generation_start"),
  roll: (stage: number) => invoke<GenerationResponse>("generation_roll", { stage }),
  choose: (stage: number, result: string) =>
    invoke<GenerationResponse>("generation_choose", { stage, result }),
  /** Names the settlement with a typed name (the name stage). */
  customName: (stage: number, name: string) =>
    invoke<GenerationResponse>("generation_custom_name", { stage, name }),
  rollDistrictCount: (stage: number) =>
    invoke<GenerationResponse>("generation_roll_district_count", { stage }),
  chooseDistrictCount: (stage: number, result: string) =>
    invoke<GenerationResponse>("generation_choose_district_count", { stage, result }),
  rollLocationCount: (stage: number, area: number) =>
    invoke<GenerationResponse>("generation_roll_location_count", { stage, area }),
  chooseLocationCount: (stage: number, area: number, result: string) =>
    invoke<GenerationResponse>("generation_choose_location_count", { stage, area, result }),
  rollLocationType: (stage: number, location: number) =>
    invoke<GenerationResponse>("generation_roll_location_type", { stage, location }),
  chooseLocationType: (stage: number, location: number, result: string) =>
    invoke<GenerationResponse>("generation_choose_location_type", { stage, location, result }),
  rollRole: (stage: number, slot: number) =>
    invoke<GenerationResponse>("generation_roll_role", { stage, slot }),
  chooseRole: (stage: number, slot: number, result: string) =>
    invoke<GenerationResponse>("generation_choose_role", { stage, slot, result }),
  rollFigureField: (stage: number, figure: number, field: string) =>
    invoke<GenerationResponse>("generation_roll_figure_field", { stage, figure, field }),
  chooseFigureField: (stage: number, figure: number, field: string, result: string) =>
    invoke<GenerationResponse>("generation_choose_figure_field", { stage, figure, field, result }),
  rollFigureName: (stage: number, figure: number) =>
    invoke<GenerationResponse>("generation_roll_figure_name", { stage, figure }),
  chooseFigureNamePart: (stage: number, figure: number, part: number, result: string) =>
    invoke<GenerationResponse>("generation_choose_figure_name_part", {
      stage,
      figure,
      part,
      result,
    }),
  rollFigureNamePart: (stage: number, figure: number, part: number) =>
    invoke<GenerationResponse>("generation_roll_figure_name_part", { stage, figure, part }),
  figureName: (stage: number, figure: number, name: string) =>
    invoke<GenerationResponse>("generation_figure_name", { stage, figure, name }),
  rollRelationship: (stage: number, relationship: number) =>
    invoke<GenerationResponse>("generation_roll_relationship", { stage, relationship }),
  chooseRelationship: (stage: number, relationship: number, result: string) =>
    invoke<GenerationResponse>("generation_choose_relationship", {
      stage,
      relationship,
      result,
    }),
  advance: () => invoke<GenerationResponse>("generation_advance"),
  back: () => invoke<GenerationResponse>("generation_back"),
  rollRemaining: () => invoke<GenerationResponse>("generation_roll_remaining"),
  runAutomatic: () => invoke<GenerationResponse>("generation_run_automatic"),
  rerollAll: () => invoke<GenerationResponse>("generation_reroll_all"),
  discard: () => invoke<void>("generation_discard"),
  save: (name: string) => invoke<SettlementSummary>("generation_save", { name }),
};

/** Faces as shown at the table, e.g. `9–12` or `4`. */
export function formatRange(range: ResultOption["range"]): string {
  if (range === undefined) return "";
  return typeof range === "number" ? String(range) : `${range[0]}–${range[1]}`;
}

// ----- saved settlements ---------------------------------------------------

/** One stage of a saved snapshot, exactly as it was saved. */
export interface SnapshotStage {
  label: string;
  stage_table: string;
  table?: Named;
  result?: Named;
  tags?: Named[];
  skipped?: boolean;
  /** Elaborates the stage before it; left out where it does not apply. */
  sub_stage?: boolean;
  /** The Authority Figures stage's record (only on that stage). */
  figures?: AuthorityFigures;
  /** The Role Allocation stage's record (only on that stage). */
  roles?: RoleAllocation;
  /** The Notable Locations stage's record (only on that stage). */
  locations?: NotableLocations;
  /** The name stage's record (only on the name stage). */
  name?: NameRecord;
  /** A flow stage's record (only on flow stages). */
  flow?: StepRecord;
  /** The flow record as indented lines (only on flow stages). */
  outline?: OutlineLine[];
}

/** A saved settlement name and how it was made. */
export interface NameRecord {
  name: string;
  source: NameSource;
  scheme?: Named;
  method?: Named;
  /** Version that produced the name. */
  app_version: string;
}

export interface SavedSettlementView extends SettlementSummary {
  stages: SnapshotStage[];
}

export const saved = {
  list: () => invoke<SettlementSummary[]>("saved_list"),
  get: (id: string) => invoke<SavedSettlementView>("saved_get", { id }),
  setFavourite: (id: string, favourite: boolean) =>
    invoke<SavedSettlementView>("saved_set_favourite", { id, favourite }),
  /** Refused by the core for favourites. */
  remove: (id: string) => invoke<void>("saved_delete", { id }),
  editStart: (id: string) => invoke<GenerationResponse>("saved_edit_start", { id }),
  editSave: (name: string) => invoke<SavedSettlementView>("saved_edit_save", { name }),
};

/** Local date and time for a stored timestamp. */
export function formatWhen(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime())
    ? iso
    : date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

// ----- content editors -----------------------------------------------------

/** Faces: one face, or `[min, max]`. */
export type FaceRange = number | [number, number];

export type Condition =
  | { has_tag: string }
  | { all: Condition[] }
  | { any: Condition[] }
  | { not: Condition }
  | { setting_on: string }
  | { setting_is: { setting: string; value: string } }
  | { setting_between: { setting: string; min?: number; max?: number } }
  | { setting_has: { setting: string; value: string } }
  | { result_is: { step: string; results: string[] } }
  | { value_in: { step: string; min?: number; max?: number } };

export interface TableResult {
  id: string;
  name: string;
  /** Tables rolled next whenever this result comes up. */
  then_roll?: string[];
  /** The shared result this entry lists; it fills the fields left empty. */
  shared?: string;
  description?: string;
  range?: FaceRange;
  tags?: string[];
  eligibility?: Condition;
  enabled?: boolean;
  /** Reserved reference to a future subordinate generator; kept as is. */
  generator?: string;
  /** The number the result stands for (a positive whole number). */
  value?: number;
  /** Held once per role allocation; a repeat becomes `fallback`. */
  unique?: boolean;
  /** The repeatable result a unique result becomes when already held. */
  fallback?: string;
  /** Display labels by a tag of the holder (e.g. gender.woman → Lady). */
  label_variants?: LabelVariant[];
}

export interface RangeAssignment {
  result: string;
  range: FaceRange;
}

export interface TableVariant {
  id: string;
  name: string;
  description?: string;
  when?: Condition;
  dice?: string;
  ranges?: RangeAssignment[];
}

/** One outcome of a follow-up roll in content. */
export interface FollowUpOutcomeDef {
  range: FaceRange;
  becomes?: string;
}

export interface FollowUpRoll {
  id: string;
  name: string;
  description?: string;
  when?: Condition;
  on_results: string[];
  dice: string;
  outcomes: FollowUpOutcomeDef[];
}

export interface Table {
  id: string;
  name: string;
  description?: string;
  dice?: string;
  selection?: "roll" | "choose";
  status?: "final" | "incomplete";
  status_note?: string;
  ordered?: boolean;
  /** The scale Shift and Cap modifiers move along. */
  scale?: string;
  active?: boolean;
  eligibility?: Condition;
  results: TableResult[];
  variants?: TableVariant[];
  follow_ups?: FollowUpRoll[];
}

/** An effect: `kind`, its fields, and lifecycle metadata. */
export interface EffectDef {
  kind: string;
  status?: "active" | "planned";
  note?: string;
  [field: string]: unknown;
}

export interface Tag {
  id: string;
  name: string;
  description?: string;
  effects?: EffectDef[];
  enabled?: boolean;
}

export type Origin = "bundled" | "modified" | "custom";

export interface IssueLocation {
  table?: string;
  variant?: string;
  result?: string;
  tag?: string;
  effect?: number;
  follow_up?: string;
  sequence_step?: number;
}

export interface ValidationIssue {
  severity: "error" | "warning";
  location: IssueLocation;
  code: string;
  message: string;
}

export interface FaceClaim {
  face: number;
  chance: number;
  claimed_by: string[];
}

export interface ResultChance {
  result: string;
  range: FaceRange;
  chance: number;
}

export interface Coverage {
  label: string;
  variant?: string;
  follow_up?: string;
  dice: string;
  dice_error?: string;
  faces: FaceClaim[];
  results: ResultChance[];
  uncovered: FaceRange[];
  overlapping: FaceRange[];
  impossible: ResultChance[];
}

export interface TableListItem {
  id: string;
  name: string;
  origin: Origin | null;
  built_in: boolean;
  deleted: boolean;
  active: boolean;
  incomplete: boolean;
  errors: number;
}

export interface TagListItem {
  id: string;
  name: string;
  origin: Origin | null;
  built_in: boolean;
  deleted: boolean;
  enabled: boolean;
}

export interface ContentOverview {
  tables: TableListItem[];
  tags: TagListItem[];
  issues: ValidationIssue[];
}

/** One place that depends on a table or a result. */
export interface Usage {
  description: string;
  table?: string;
  tag?: string;
  flow?: string;
}

export type FitKind = "exact" | "curve" | "roll_again" | "approximate";

/** One way of laying results out on physical dice. */
export interface DiceFit {
  dice: string;
  kind: FitKind;
  label: string;
  explanation: string;
  ranges: FaceRange[];
  roll_again?: FaceRange;
  /** Each result's chance (0–1). */
  chances: number[];
  /** Largest difference from the wanted chance, in percentage points. */
  max_error: number;
}

export interface EntryChange {
  kind: "add" | "remove" | "reweight";
  count: number;
  dice: string;
  explanation: string;
}

export interface DiceAdvice {
  targets: number[];
  suggestions: DiceFit[];
  summary: string;
  entry_changes?: EntryChange[];
}

/** A table roll as the preview shows it. */
export interface PreviewRecord {
  table: Named;
  dice?: string;
  attempts?: {
    roll: { total: number; dice: { die: string; value: number }[] };
    face: number;
    result: Named;
    outcome: string;
  }[];
  result: Named;
  adjusted_from?: Named;
  rolled_after?: Named;
  routed_by?: Named;
}

export type PreviewEvent =
  | ({ event: "table_rolled" } & PreviewRecord)
  | { event: "table_skipped"; table: Named; reason: string }
  | { event: "secondary_roll"; tag: Named; label: string; roll: { total: number } };

export interface PreviewRoll {
  events: PreviewEvent[];
  traces: RollTrace[];
  note: string;
}

// ----- testing and simulation ---------------------------------------------------

export interface OutcomeStat {
  id: string;
  name: string;
  count: number;
  /** Share of the runs, 0–100. */
  actual: number;
  /** What the dice should give, 0–100 (tables only). */
  expected?: number;
}

export interface ModifierUse {
  label: string;
  count: number;
}

export interface ErrorCount {
  message: string;
  count: number;
}

export interface TableSimulation {
  runs: number;
  table: Named;
  results: OutcomeStat[];
  unreachable: Named[];
  rerolls: number;
  modifiers: ModifierUse[];
  errors: ErrorCount[];
  issues: ValidationIssue[];
  note: string;
}

export interface StepStat {
  step: string;
  label: string;
  reached: number;
  outcomes: OutcomeStat[];
}

export interface BranchStat {
  step: string;
  label: string;
  cases: OutcomeStat[];
}

export interface FlowSimulation {
  runs: number;
  steps: StepStat[];
  branches: BranchStat[];
  impossible: string[];
  modifiers: ModifierUse[];
  errors: ErrorCount[];
  issues: ValidationIssue[];
  note: string;
}

export interface FlowPreview {
  steps: StepRecord[];
  outline: OutlineLine[];
  traces: RollTrace[];
  error?: string;
}

// ----- import and export ------------------------------------------------------------

export type PackKind =
  "setting" | "scale" | "shared_result" | "object_type" | "table" | "tag" | "flow";

export interface PackItem {
  kind: PackKind;
  id: string;
}

/** A content file: complete items of every kind, plus format details. */
export interface ContentPack {
  format: string;
  version: number;
  app_version?: string;
  name?: string;
  [kind: string]: unknown;
}

export type Collision = "rename" | "replace" | "skip";

export type ImportAction =
  | "add"
  | { add_as: { id: string } }
  | { replace: { existing: string } }
  | { skip: { existing: string } };

export interface ImportItem {
  kind: PackKind;
  id: string;
  name: string;
  action: ImportAction;
}

export interface ImportPlan {
  name: string;
  items: ImportItem[];
  issues: ValidationIssue[];
  error?: string;
  can_import: boolean;
  summary: string;
}

export const transfer = {
  exportItems: (items: PackItem[], withDependencies: boolean, name: string) =>
    invoke<ContentPack>("content_export_items", { items, withDependencies, name }),
  exportEverything: (scope: "everything" | "customised") =>
    invoke<ContentPack>("content_export_everything", { scope }),
  /** Writes the pack to the exports folder; resolves to the file's path. */
  writePack: (pack: ContentPack, fileName: string) =>
    invoke<string>("content_write_pack", { pack, fileName }),
  exportsFolder: () => invoke<string | null>("content_exports_folder"),
  planImport: (text: string, policy: Collision) =>
    invoke<ImportPlan>("content_plan_import", { text, policy }),
  importPack: (text: string, policy: Collision) =>
    invoke<ImportPlan>("content_import", { text, policy }),
};

export interface TableDetail {
  table: Table;
  origin: Origin | null;
  built_in: boolean;
  coverage: Coverage[];
  issues: ValidationIssue[];
  /** What depends on the table; it cannot be deleted while any remain. */
  usages?: Usage[];
}

export interface TagUsage {
  kind: "emitted_by" | "checked_by" | "added_by" | "removed_by";
  location: IssueLocation;
  description: string;
}

export interface TagDetail {
  tag: Tag;
  origin: Origin | null;
  built_in: boolean;
  usages: TagUsage[];
  issues: ValidationIssue[];
}

export interface DraftCheck {
  coverage: Coverage[];
  issues: ValidationIssue[];
  error: string | null;
}

export type FieldKind =
  | { type: "table" }
  | { type: "variant"; table_field: string }
  | { type: "result"; table_field: string }
  | { type: "result_list"; table_field: string }
  | { type: "tag" }
  | { type: "flow" }
  | { type: "dice" }
  | { type: "text" }
  | { type: "integer" }
  | { type: "choice"; options: { value: string; label: string }[] };

export interface EffectField {
  name: string;
  label: string;
  kind: FieldKind;
  required: boolean;
  help: string;
}

export interface EffectPreset {
  label: string;
  values: Record<string, unknown>;
}

export interface EffectKindSchema {
  kind: string;
  label: string;
  description: string;
  executable: boolean;
  fields: EffectField[];
  /** One-click values, e.g. "Shift +1". */
  presets?: EffectPreset[];
  /** The Modifier Builder's grouping, e.g. "Change the roll". */
  group: string;
}

/** Something an effect or modifier changes. */
export interface Affected {
  kind: "table" | "result" | "tag" | "flow";
  id: string;
  name: string;
}

/** An effect or modifier in plain words, from the core. */
export interface Explanation {
  title: string;
  summary: string;
  /** What happens to a roll, step by step. */
  preview?: string[];
  planned: boolean;
  affects: Affected[];
}

export interface TablePreview {
  table: Named;
  steps: string[];
}

export interface TagInsight {
  effects: Explanation[];
  combined: TablePreview[];
  /** No active effects: the tag only labels the settlement. */
  informational: boolean;
}

// ----- generation settings and framework items -------------------------------

/** On/off, a number, an option (or text), or the options ticked. */
export type SettingValue = boolean | number | string | string[];

export type SettingType = "toggle" | "number" | "choice" | "multi_select" | "text";

export interface SettingOption {
  id: string;
  name: string;
}

export type SettingDef = {
  id: string;
  name: string;
  description?: string;
  /** Planned settings are documented but not offered; generation uses the default. */
  status?: "active" | "planned";
  note?: string;
} & (
  | { kind: "toggle"; default?: boolean }
  | { kind: "number"; default?: number; min?: number; max?: number }
  | { kind: "choice"; default: string; options: SettingOption[] }
  | { kind: "multi_select"; default?: string[]; options: SettingOption[] }
  | { kind: "text"; default?: string }
);

export interface SettingDetail {
  setting: SettingDef;
  origin: Origin | null;
  built_in: boolean;
  usages: Usage[];
  issues: ValidationIssue[];
}

export interface ContentSettingView {
  setting: SettingDef;
  value: SettingValue;
}

export type ItemKind = "shared_result" | "scale" | "setting" | "object_type" | "flow";

export interface FrameworkItemListing {
  kind: ItemKind;
  id: string;
  name: string;
  origin?: Origin;
  deleted: boolean;
}

export const framework = {
  settings: () => invoke<ContentSettingView[]>("content_settings"),
  setSetting: (id: string, value: SettingValue) =>
    invoke<ContentSettingView[]>("content_set_setting", { id, value }),
  /** On/off settings a condition can test (the app's and the content's). */
  toggleSettings: () => invoke<Named[]>("content_toggle_settings"),
  items: () => invoke<FrameworkItemListing[]>("framework_items"),
  item: (kind: ItemKind, id: string) => invoke<unknown>("framework_item", { kind, id }),
  saveItem: (kind: ItemKind, item: unknown) =>
    invoke<FrameworkItemListing[]>("framework_save_item", { kind, item }),
  deleteItem: (kind: ItemKind, id: string) =>
    invoke<FrameworkItemListing[]>("framework_delete_item", { kind, id }),
  resetItem: (kind: ItemKind, id: string) =>
    invoke<FrameworkItemListing[]>("framework_reset_item", { kind, id }),
  settingDetail: (id: string) => invoke<SettingDetail>("setting_detail", { id }),
  newSetting: (kind: SettingType) => invoke<SettingDef>("setting_new", { kind }),
  checkSetting: (setting: SettingDef) => invoke<DraftCheck>("setting_check", { setting }),
  saveSetting: (setting: SettingDef) => invoke<SettingDetail>("setting_save", { setting }),
  /** Refused, naming what depends on it, while the setting is used. */
  deleteSetting: (id: string) => invoke<FrameworkItemListing[]>("setting_delete", { id }),
};

export const content = {
  overview: () => invoke<ContentOverview>("content_overview"),
  table: (id: string) => invoke<TableDetail>("content_table", { id }),
  tag: (id: string) => invoke<TagDetail>("content_tag", { id }),
  checkTable: (table: Table) => invoke<DraftCheck>("content_check_table", { table }),
  checkTag: (tag: Tag) => invoke<DraftCheck>("content_check_tag", { tag }),
  saveTable: (table: Table) => invoke<TableDetail>("content_save_table", { table }),
  saveTag: (tag: Tag) => invoke<TagDetail>("content_save_tag", { tag }),
  newTable: () => invoke<Table>("content_new_table"),
  newTag: () => invoke<Tag>("content_new_tag"),
  newId: (kind: string) => invoke<string>("content_new_id", { kind }),
  deleteTable: (id: string) => invoke<ContentOverview>("content_delete_table", { id }),
  deleteTag: (id: string) => invoke<ContentOverview>("content_delete_tag", { id }),
  resetTable: (id: string) => invoke<ContentOverview>("content_reset_table", { id }),
  resetTag: (id: string) => invoke<ContentOverview>("content_reset_tag", { id }),
  resetAll: () => invoke<ContentOverview>("content_reset_all"),
  diceAdvice: (weights: number[]) => invoke<DiceAdvice>("content_dice_advice", { weights }),
  diceFit: (weights: number[], dice: string) =>
    invoke<DiceFit>("content_dice_fit", { weights, dice }),
  applyDiceFit: (table: Table, entries: string[], fit: DiceFit) =>
    invoke<Table>("content_apply_dice_fit", { table, entries, fit }),
  previewRoll: (table: Table) => invoke<PreviewRoll>("content_preview_roll", { table }),
  copyTable: (id: string) => invoke<Table>("content_copy_table", { id }),
  resultUsages: (table: Table, result: string) =>
    invoke<Usage[]>("content_result_usages", { table, result }),
  saveTableDraft: (table: Table) => invoke<void>("content_save_table_draft", { table }),
  tableDraft: (id: string) => invoke<Table | null>("content_table_draft", { id }),
  discardTableDraft: (id: string) => invoke<void>("content_discard_table_draft", { id }),
  effectSchema: () => invoke<EffectKindSchema[]>("effect_schema"),
  explainTag: (tag: Tag) => invoke<TagInsight>("tag_explain", { tag }),
  simulateTable: (table: Table, runs: number, tags: string[] = []) =>
    invoke<TableSimulation>("content_simulate_table", { table, runs, tags }),
  explainEffect: (tag: Tag | null, effect: EffectDef) =>
    invoke<Explanation>("effect_explain", { tag, effect }),
};

/** `[min, max]` for any face range. */
export function toPair(range: FaceRange): [number, number] {
  return typeof range === "number" ? [range, range] : range;
}

// ----- flows -------------------------------------------------------------------

export type Modifier =
  { shift: number } | { plus: number } | { minus: number } | "cap_at_highest" | "cap_at_lowest";

export type Count =
  { times: number } | { value_of: { step: string; add?: number } } | { dice: string };

export interface StepModifier {
  modifier: Modifier;
  when?: Condition;
}

export interface BranchCase {
  name?: string;
  when: Condition;
  steps: FlowStep[];
}

export type StepAction =
  | { kind: "roll"; table: string; field?: string; modifiers?: StepModifier[] }
  | { kind: "secondary_roll"; label: string; dice: string }
  | { kind: "add_tag"; tag: string }
  | { kind: "remove_tag"; tag: string }
  | { kind: "set_result"; table: string; result: string; field?: string }
  | { kind: "set_value"; field: string; value: number }
  | { kind: "branch"; cases: BranchCase[]; otherwise?: FlowStep[] }
  | { kind: "repeat"; count: Count; steps: FlowStep[] }
  | { kind: "run_flow"; flow: string; max_depth?: number }
  | { kind: "for_each"; object: string; pairs?: boolean; steps?: FlowStep[] }
  | { kind: "apply_modifier"; table: string; modifier: Modifier }
  | { kind: "read_setting"; setting: string }
  | { kind: "stop" }
  | { kind: "create_object"; object: string; steps?: FlowStep[]; share_tags?: boolean }
  | { kind: "name"; subject?: "settlement" | "person" }
  | { kind: "builtin"; handler: string; config?: unknown };

export type FlowStep = {
  id: string;
  name?: string;
  description?: string;
  /** The step only runs while this holds. */
  when?: Condition;
  active?: boolean;
  status?: "active" | "planned";
  note?: string;
  sub_stage?: boolean;
} & StepAction;

export interface Flow {
  id: string;
  name: string;
  description?: string;
  steps: FlowStep[];
  /** The editor's own layout; never affects generation. */
  editor?: { positions?: Record<string, [number, number]>; detached?: FlowStep[] };
}

/** A list of steps: the flow's own, a container's body, a case, or Otherwise. */
export type Slot =
  | { list: "top" }
  | { list: "body"; step: string }
  | { list: "case"; step: string; case: number }
  | { list: "otherwise"; step: string };

export type FlowPosition = Slot & { index: number };

export type StepKind =
  | "roll_table"
  | "run_subflow"
  | "condition"
  | "create_object"
  | "repeat_times"
  | "repeat_for_each"
  | "set_value"
  | "add_tag"
  | "remove_tag"
  | "apply_modifier"
  | "read_setting"
  | "stop";

export type FlowEdit =
  | { op: "add"; kind: StepKind; at: FlowPosition }
  | { op: "insert"; step: FlowStep; at: FlowPosition }
  | { op: "update"; step: FlowStep }
  | { op: "remove"; step: string }
  | { op: "move"; step: string; to: FlowPosition }
  | { op: "detach"; step: string }
  | { op: "attach"; step: string; to: FlowPosition }
  | { op: "duplicate"; step: string }
  | { op: "add_case"; step: string }
  | { op: "remove_case"; step: string; case: number }
  | { op: "place"; step: string; at: [number, number] };

export interface PseudoLine {
  depth: number;
  step?: string;
  text: string;
}

/** What a step's node shows, in words from the core. */
export interface NodeText {
  step: string;
  kind: string;
  text: string;
  when?: string;
  cases?: string[];
}

export interface FlowIssueLocation extends IssueLocation {
  flow?: string;
  step?: string;
}

export interface FlowCheck {
  issues: (ValidationIssue & { location: FlowIssueLocation })[];
  pseudocode: PseudoLine[];
  nodes: NodeText[];
  error: string | null;
}

/** A step a condition can test. */
export interface ConditionSubject {
  step: string;
  label: string;
  results: Named[];
  numeric: boolean;
  pipeline: boolean;
}

export interface ConditionSetting {
  id: string;
  name: string;
  kind: SettingType;
  /** A choice setting's options; empty for an on/off one. */
  options: Named[];
}

export interface FlowContext {
  subjects: ConditionSubject[];
  tables: Named[];
  tags: Named[];
  settings: ConditionSetting[];
  objects: Named[];
  flows: Named[];
  kinds: Named[];
}

export const flows = {
  get: (id: string) => invoke<Flow>("flow_get", { id }),
  create: () => invoke<Flow>("flow_new"),
  duplicate: (id: string) => invoke<Flow>("flow_duplicate", { id }),
  edit: (flow: Flow, edit: FlowEdit) => invoke<Flow>("flow_edit", { flow, edit }),
  check: (flow: Flow) => invoke<FlowCheck>("flow_check", { flow }),
  context: (flow: Flow) => invoke<FlowContext>("flow_context", { flow }),
  simulate: (flow: Flow, runs: number) => invoke<FlowSimulation>("flow_simulate", { flow, runs }),
  preview: (flow: Flow) => invoke<FlowPreview>("flow_preview", { flow }),
  usages: (id: string) => invoke<Usage[]>("flow_usages", { id }),
  /** Refused, naming what depends on it, while anything runs the flow. */
  remove: (id: string) => invoke<FrameworkItemListing[]>("flow_delete", { id }),
  explainModifier: (table: string, modifier: Modifier) =>
    invoke<Explanation>("modifier_explain", { table, modifier }),
  describeCondition: (flow: Flow, condition: Condition) =>
    invoke<string>("flow_describe_condition", { flow, condition }),
  save: (flow: Flow) => invoke<Flow>("flow_save", { flow }),
  saveDraft: (flow: Flow) => invoke<void>("flow_save_draft", { flow }),
  draft: (id: string) => invoke<Flow | null>("flow_draft", { id }),
  discardDraft: (id: string) => invoke<void>("flow_discard_draft", { id }),
};
