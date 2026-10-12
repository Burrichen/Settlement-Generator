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
  { has_tag: string } | { all: Condition[] } | { any: Condition[] } | { not: Condition };

export interface TableResult {
  id: string;
  name: string;
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

export interface TableDetail {
  table: Table;
  origin: Origin | null;
  built_in: boolean;
  coverage: Coverage[];
  issues: ValidationIssue[];
}

export interface TagUsage {
  kind: "emitted_by" | "checked_by" | "added_by";
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

export interface EffectKindSchema {
  kind: string;
  label: string;
  description: string;
  executable: boolean;
  fields: EffectField[];
}

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
  effectSchema: () => invoke<EffectKindSchema[]>("effect_schema"),
};

/** `[min, max]` for any face range. */
export function toPair(range: FaceRange): [number, number] {
  return typeof range === "number" ? [range, range] : range;
}
