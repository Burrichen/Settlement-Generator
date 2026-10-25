//! Validation of the framework's content: shared results, scales, settings,
//! object types, flows, modifiers and built-in steps. Planned steps are
//! design notes and their references are not checked.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::content::{Condition, Modifier, SelectionMode, Table};
use crate::dice::DiceExpr;
use crate::ids::{FlowId, ObjectTypeId, StepId, TableId};
use crate::validation::{IdScope, IssueKind, Location, Validator};

use super::builtin::Builtin;
use super::{
    APP_SETTING_PREFIX, APP_SETTINGS, Count, FlowStep, MAX_FLOW_DEPTH, MAX_REPEAT, SettingKind,
    StepAction,
};

impl Validator<'_> {
    /// Checks every framework item and the pipeline.
    pub(crate) fn check_framework(&mut self) {
        self.check_shared_results();
        self.check_scales();
        self.check_settings();
        self.check_objects();
        self.check_flows();
        self.check_pipeline();
    }

    fn check_unique_ids<'i>(
        &mut self,
        ids: impl IntoIterator<Item = &'i str>,
        scope: IdScope,
        at: impl Fn(&str) -> Location,
    ) {
        let mut seen = HashSet::new();
        for id in ids {
            self.check_id(id, at(id));
            if !seen.insert(id) {
                self.error(
                    at(id),
                    IssueKind::DuplicateId {
                        scope,
                        id: id.to_owned(),
                    },
                );
            }
        }
    }

    fn check_shared_results(&mut self) {
        let content = self.content;
        self.check_unique_ids(
            content.shared_results.iter().map(|r| r.id.as_str()),
            IdScope::SharedResult,
            |id| Location {
                shared_result: Some(id.into()),
                ..Default::default()
            },
        );
        for shared in &content.shared_results {
            let loc = Location {
                shared_result: Some(shared.id.to_string()),
                ..Default::default()
            };
            for tag in &shared.tags {
                self.check_tag_ref(tag, &loc);
            }
            self.check_condition(shared.eligibility.as_ref(), &loc);
            if let Some(value) = shared.value.filter(|v| *v < 1) {
                self.error(loc.clone(), IssueKind::InvalidValue { value });
            }
        }
        for table in &content.tables {
            for result in &table.results {
                if let Some(id) = &result.shared
                    && content.shared_result(id).is_none()
                {
                    self.error(
                        Location::table(&table.id).with_result(&result.id),
                        IssueKind::MissingSharedResult {
                            shared: id.to_string(),
                        },
                    );
                }
            }
        }
    }

    fn check_scales(&mut self) {
        let content = self.content;
        self.check_unique_ids(
            content.scales.iter().map(|s| s.id.as_str()),
            IdScope::Scale,
            |id| Location {
                scale: Some(id.into()),
                ..Default::default()
            },
        );
        for scale in &content.scales {
            let loc = Location {
                scale: Some(scale.id.to_string()),
                ..Default::default()
            };
            let mut steps = HashSet::new();
            for step in &scale.steps {
                if !steps.insert(step) {
                    self.error(
                        loc.clone(),
                        IssueKind::DuplicateId {
                            scope: IdScope::Result,
                            id: step.to_string(),
                        },
                    );
                }
            }
        }
        for table in &content.tables {
            if let Some(id) = &table.scale
                && content.scale(id).is_none()
            {
                self.error(
                    Location::table(&table.id),
                    IssueKind::MissingScale {
                        scale: id.to_string(),
                    },
                );
            }
        }
    }

    fn check_settings(&mut self) {
        let content = self.content;
        self.check_unique_ids(
            content.settings.iter().map(|s| s.id.as_str()),
            IdScope::Setting,
            |id| Location {
                setting: Some(id.into()),
                ..Default::default()
            },
        );
        for setting in &content.settings {
            let loc = Location {
                setting: Some(setting.id.to_string()),
                ..Default::default()
            };
            if setting.id.as_str().starts_with(APP_SETTING_PREFIX) {
                self.error(loc.clone(), IssueKind::ReservedSettingId);
            }
            let options = setting.kind.options();
            let mut ids = HashSet::new();
            for option in options {
                self.check_id(&option.id, loc.clone());
                if !ids.insert(&option.id) {
                    self.error(
                        loc.clone(),
                        IssueKind::DuplicateId {
                            scope: IdScope::SettingOption,
                            id: option.id.clone(),
                        },
                    );
                }
            }
            if matches!(
                setting.kind,
                SettingKind::Choice { .. } | SettingKind::MultiSelect { .. }
            ) && options.is_empty()
            {
                self.error(loc.clone(), IssueKind::SettingWithoutOptions);
            } else if !setting.accepts(&setting.default_value()) {
                if let SettingKind::Number {
                    min: Some(a),
                    max: Some(b),
                    ..
                } = setting.kind
                    && a > b
                {
                    self.error(loc, IssueKind::InvalidSettingRange);
                } else {
                    self.error(loc, IssueKind::InvalidSettingDefault);
                }
            }
        }
    }

    fn check_objects(&mut self) {
        let content = self.content;
        self.check_unique_ids(
            content.objects.iter().map(|o| o.id.as_str()),
            IdScope::ObjectType,
            |id| Location {
                object_type: Some(id.into()),
                ..Default::default()
            },
        );
        for object in &content.objects {
            let loc = Location {
                object_type: Some(object.id.to_string()),
                ..Default::default()
            };
            let mut ids = HashSet::new();
            for field in &object.fields {
                self.check_id(&field.id, loc.clone());
                if !ids.insert(&field.id) {
                    self.error(
                        loc.clone(),
                        IssueKind::DuplicateId {
                            scope: IdScope::Field,
                            id: field.id.clone(),
                        },
                    );
                }
            }
        }
    }

    fn check_flows(&mut self) {
        let content = self.content;
        self.check_unique_ids(
            content.flows.iter().map(|f| f.id.as_str()),
            IdScope::Flow,
            |id| Location {
                flow: Some(id.into()),
                ..Default::default()
            },
        );
        if let Some(start) = &content.start
            && content.flow(start).is_none()
        {
            self.error(
                Location::default(),
                IssueKind::MissingFlow {
                    flow: start.to_string(),
                },
            );
        }
        // Object types some running step creates, for For Each.
        let created: HashSet<ObjectTypeId> = content
            .flows
            .iter()
            .flat_map(|f| f.all_steps())
            .filter(|s| s.runs())
            .filter_map(|s| match &s.action {
                StepAction::CreateObject { object, .. } => Some(object.clone()),
                _ => None,
            })
            .collect();
        let stages = content.pipeline();
        for flow in &content.flows {
            let loc = Location {
                flow: Some(flow.id.to_string()),
                ..Default::default()
            };
            let mut ids = HashSet::new();
            for step in flow.all_steps() {
                self.check_id(step.id.as_str(), loc.clone().with_step(&step.id));
                if !ids.insert(&step.id) {
                    self.error(
                        loc.clone().with_step(&step.id),
                        IssueKind::DuplicateId {
                            scope: IdScope::Step,
                            id: step.id.to_string(),
                        },
                    );
                }
            }
            let mut infos: HashMap<StepId, StepInfo> = HashMap::new();
            for step in stages.iter().chain(flow.all_steps()) {
                infos.insert(step.id.clone(), self.step_info(step));
            }
            let ctx = FlowCtx {
                flow: &loc,
                top: content.start.as_ref() == Some(&flow.id),
                infos: &infos,
                created: &created,
            };
            self.check_steps(&flow.steps, &ctx, false);
            // Steps taken out of the flow in the editor never run.
            for step in &flow.editor.detached {
                self.warning(
                    loc.clone().with_step(&step.id),
                    IssueKind::UnreachableStep {
                        reason: "it is not connected to the flow".into(),
                    },
                );
            }
        }
        self.check_flow_cycles();
    }

    /// The legacy sequence's own checks, and the pipeline's first stage.
    fn check_pipeline(&mut self) {
        let content = self.content;
        if content.start.is_none() {
            for (step, id) in content
                .sequence
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.table().map(|t| (i, t)))
            {
                if !self.tables.contains_key(id) {
                    self.error(
                        Location {
                            sequence_step: Some(step),
                            ..Default::default()
                        },
                        IssueKind::MissingTable { table: id.clone() },
                    );
                }
            }
            // A legacy sequence's built-in stages use the legacy
            // configuration, whose tables must exist.
            let loc = Location::default();
            for step in content.pipeline() {
                if let StepAction::Builtin { handler, config } = &step.action
                    && let Ok(builtin) = Builtin::parse(handler, config)
                {
                    self.check_builtin(&builtin, &loc);
                }
            }
        }
        if content
            .pipeline()
            .iter()
            .filter(|s| s.runs())
            .take(1)
            .any(|s| s.sub_stage)
        {
            self.error(
                Location {
                    sequence_step: Some(0),
                    ..Default::default()
                },
                IssueKind::SubStageWithoutParent,
            );
        }
    }

    /// What a step stores for later conditions and counts.
    fn step_info(&self, step: &FlowStep) -> StepInfo {
        let label = step.name.clone().unwrap_or_else(|| step.id.to_string());
        let valued = |t: &TableId| {
            self.tables
                .get(t)
                .is_some_and(|t| t.results.iter().any(|r| r.value.is_some()))
        };
        match &step.action {
            StepAction::Roll { table, .. } => StepInfo {
                label,
                table: Some(table.clone()),
                result: true,
                value: valued(table),
            },
            StepAction::SetResult { table, .. } => StepInfo {
                label,
                table: Some(table.clone()),
                result: true,
                value: valued(table),
            },
            StepAction::SecondaryRoll { .. } | StepAction::SetValue { .. } => StepInfo {
                label,
                table: None,
                result: false,
                value: true,
            },
            StepAction::ReadSetting { setting } => {
                let choice = self.content.setting(setting).is_some_and(|s| {
                    matches!(
                        s.kind,
                        SettingKind::Choice { .. } | SettingKind::Text { .. }
                    )
                });
                StepInfo {
                    label,
                    table: None,
                    result: choice,
                    value: !choice,
                }
            }
            _ => StepInfo {
                label,
                table: None,
                result: false,
                value: false,
            },
        }
    }

    /// One list of steps: each step, and what can never be reached.
    fn check_steps(&mut self, steps: &[FlowStep], ctx: &FlowCtx<'_>, in_object: bool) {
        let mut stopped = false;
        for step in steps.iter().filter(|s| s.runs()) {
            if stopped {
                self.warning(
                    ctx.flow.clone().with_step(&step.id),
                    IssueKind::UnreachableStep {
                        reason: "a Stop step before it always ends the flow".into(),
                    },
                );
            }
            self.check_step(step, ctx, in_object);
            if matches!(step.action, StepAction::Stop) && step.when.is_none() {
                stopped = true;
            }
        }
    }

    fn check_step(&mut self, step: &FlowStep, ctx: &FlowCtx<'_>, in_object: bool) {
        if !step.status.is_active() {
            return;
        }
        let flow = ctx.flow;
        let loc = flow.clone().with_step(&step.id);
        if let Some(when) = &step.when {
            self.check_flow_condition(when, &loc, ctx);
            if ctx.top
                && matches!(
                    step.action,
                    StepAction::Builtin { .. } | StepAction::Name { .. }
                )
            {
                self.error(loc.clone(), IssueKind::ConditionNotSupported);
            }
        }
        if step.sub_stage && !ctx.top {
            self.error(loc.clone(), IssueKind::SubStageWithoutParent);
        }
        let inner = FlowCtx { top: false, ..*ctx };
        let check_field = |v: &mut Self, field: &Option<String>| {
            if field.is_some() && !in_object {
                v.error(loc.clone(), IssueKind::FieldOutsideObject);
            }
        };
        match &step.action {
            StepAction::Roll {
                table,
                field,
                modifiers,
            } => {
                check_field(self, field);
                match self.tables.get(table).copied() {
                    None => self.error(
                        loc.clone(),
                        IssueKind::MissingTable {
                            table: table.clone(),
                        },
                    ),
                    Some(t) => {
                        for m in modifiers {
                            self.check_modifier(m.modifier, t, &loc);
                            if let Some(c) = &m.when {
                                self.check_flow_condition(c, &loc, ctx);
                            }
                        }
                    }
                }
            }
            StepAction::SecondaryRoll { dice, .. } => self.check_dice(dice, &loc),
            StepAction::AddTag { tag } | StepAction::RemoveTag { tag } => {
                self.check_tag_ref(tag, &loc);
            }
            StepAction::SetResult {
                table,
                result,
                field,
            } => {
                check_field(self, field);
                match self.tables.get(table).copied() {
                    None => self.error(
                        loc.clone(),
                        IssueKind::MissingTable {
                            table: table.clone(),
                        },
                    ),
                    Some(t) if t.result(result).is_none() => self.error(
                        loc.clone(),
                        IssueKind::BrokenResultReference {
                            table: table.clone(),
                            result: result.clone(),
                        },
                    ),
                    Some(_) => {}
                }
            }
            StepAction::SetValue { field, .. } => check_field(self, &Some(field.clone())),
            StepAction::Branch { cases, otherwise } => {
                let mut always = false;
                for (i, case) in cases.iter().enumerate() {
                    self.check_flow_condition(&case.when, &loc, ctx);
                    if always {
                        self.warning(
                            loc.clone(),
                            IssueKind::UnreachableStep {
                                reason: format!(
                                    "case {} can never be reached: a case before it always applies",
                                    i + 1
                                ),
                            },
                        );
                    }
                    if matches!(&case.when, Condition::All(c) if c.is_empty()) {
                        always = true;
                    }
                    self.check_steps(&case.steps, &inner, in_object);
                }
                if always && !otherwise.is_empty() {
                    self.warning(
                        loc.clone(),
                        IssueKind::UnreachableStep {
                            reason: "the Otherwise steps can never be reached".into(),
                        },
                    );
                }
                self.check_steps(otherwise, &inner, in_object);
            }
            StepAction::Repeat { count, steps: body } => {
                match count {
                    Count::Times(n) if *n > MAX_REPEAT => self.error(
                        loc.clone(),
                        IssueKind::InvalidCount {
                            reason: format!("a repeat can run at most {MAX_REPEAT} times"),
                        },
                    ),
                    Count::Times(0) => self.warning(
                        loc.clone(),
                        IssueKind::EmptyLoop {
                            reason: "it repeats 0 times, so its steps never run".into(),
                        },
                    ),
                    Count::Times(_) => {}
                    Count::ValueOf { step: from, .. } => match ctx.infos.get(from) {
                        None => self.error(
                            loc.clone(),
                            IssueKind::MissingStep {
                                step: from.to_string(),
                            },
                        ),
                        Some(info) if !info.value => self.error(
                            loc.clone(),
                            IssueKind::InvalidCount {
                                reason: format!(
                                    "step “{}” stores no number to count with",
                                    info.label
                                ),
                            },
                        ),
                        Some(_) => {}
                    },
                    Count::Dice(dice) => match DiceExpr::parse(dice) {
                        Err(_) => self.check_dice(dice, &loc),
                        Ok(expr) if expr.max_total() > MAX_REPEAT as i32 => self.error(
                            loc.clone(),
                            IssueKind::InvalidCount {
                                reason: format!(
                                    "{dice} can roll more than the {MAX_REPEAT} repeats allowed"
                                ),
                            },
                        ),
                        Ok(_) => {}
                    },
                }
                if body.is_empty() {
                    self.warning(
                        loc.clone(),
                        IssueKind::EmptyLoop {
                            reason: "it has no steps to repeat".into(),
                        },
                    );
                }
                self.check_steps(body, &inner, in_object);
            }
            StepAction::ForEach {
                object,
                steps: body,
                ..
            } => {
                if self.content.object_type(object).is_none() {
                    self.error(
                        loc.clone(),
                        IssueKind::MissingObjectType {
                            object: object.to_string(),
                        },
                    );
                } else if !ctx.created.contains(object) {
                    self.warning(
                        loc.clone(),
                        IssueKind::EmptyLoop {
                            reason: format!("no step creates a {object}, so it never runs"),
                        },
                    );
                }
                if body.is_empty() {
                    self.warning(
                        loc.clone(),
                        IssueKind::EmptyLoop {
                            reason: "it has no steps to run".into(),
                        },
                    );
                }
                self.check_steps(body, &inner, in_object);
            }
            StepAction::RunFlow {
                flow: target,
                max_depth,
            } => {
                if self.content.flow(target).is_none() {
                    self.error(
                        loc.clone(),
                        IssueKind::MissingFlow {
                            flow: target.to_string(),
                        },
                    );
                }
                if let Some(depth) = max_depth
                    && (*depth == 0 || *depth as usize > MAX_FLOW_DEPTH)
                {
                    self.error(loc.clone(), IssueKind::InvalidRecursionDepth);
                }
            }
            StepAction::CreateObject {
                object,
                steps: body,
                ..
            } => {
                let def = self.content.object_type(object);
                if def.is_none() {
                    self.error(
                        loc.clone(),
                        IssueKind::MissingObjectType {
                            object: object.to_string(),
                        },
                    );
                }
                self.check_steps(body, &inner, true);
                // Fields set directly inside the object must be its own.
                for child in body {
                    if let (Some(def), Some(field)) = (def, step_field(child))
                        && def.field(field).is_none()
                    {
                        self.error(
                            flow.clone().with_step(&child.id),
                            IssueKind::MissingField {
                                field: field.to_owned(),
                            },
                        );
                    }
                }
            }
            StepAction::ApplyModifier { table, modifier } => {
                match self.tables.get(table).copied() {
                    None => self.error(
                        loc.clone(),
                        IssueKind::MissingTable {
                            table: table.clone(),
                        },
                    ),
                    Some(t) => self.check_modifier(*modifier, t, &loc),
                }
            }
            StepAction::ReadSetting { setting } => {
                let known = APP_SETTINGS.contains(&setting.as_str())
                    || self.content.setting(setting).is_some();
                if !known {
                    self.error(
                        loc.clone(),
                        IssueKind::MissingSetting {
                            setting: setting.to_string(),
                        },
                    );
                }
            }
            StepAction::Name { .. } | StepAction::Stop => {}
            StepAction::Builtin { handler, config } => {
                if !ctx.top {
                    self.error(loc.clone(), IssueKind::BuiltinNotTopLevel);
                }
                match Builtin::parse(handler, config) {
                    Err(super::builtin::BuiltinError::Unknown(handler)) => {
                        self.error(loc.clone(), IssueKind::UnknownBuiltin { handler });
                    }
                    Err(e) => self.error(
                        loc.clone(),
                        IssueKind::InvalidBuiltinConfig {
                            error: e.to_string(),
                        },
                    ),
                    Ok(builtin) => self.check_builtin(&builtin, &loc),
                }
            }
        }
    }

    fn check_builtin(&mut self, builtin: &Builtin, loc: &Location) {
        for table in builtin.tables() {
            if !self.tables.contains_key(table) {
                self.error(
                    loc.clone(),
                    IssueKind::MissingTable {
                        table: table.clone(),
                    },
                );
            }
        }
        for tag in builtin.tags() {
            self.check_tag_ref(tag, loc);
        }
        for condition in builtin.conditions() {
            self.check_condition(Some(condition), loc);
        }
        // The guaranteed location's type is a result of the type table.
        if let Builtin::Locations(c) = builtin
            && let (Some(spec), Some(table)) = (&c.guaranteed, self.tables.get(&c.type_table))
            && table.result(&spec.location_type).is_none()
        {
            self.error(
                Location::table(&c.type_table),
                IssueKind::BrokenResultReference {
                    table: c.type_table.clone(),
                    result: spec.location_type.clone(),
                },
            );
        }
    }

    /// A modifier applied to `table`, by a tag effect or a flow step.
    pub(crate) fn check_modifier(&mut self, modifier: Modifier, table: &Table, loc: &Location) {
        if let Some(reason) = modifier.problem() {
            self.error(loc.clone(), IssueKind::InvalidModifier { reason });
        }
        if modifier.needs_scale() && self.content.scale_order(table).is_none() {
            self.error(loc.clone(), IssueKind::ModifierNeedsScale);
        }
        let rolled = table.selection == SelectionMode::Roll && table.dice.is_some();
        if modifier.total_delta() != 0 && !rolled {
            self.error(loc.clone(), IssueKind::ModifierNeedsDice);
        }
    }

    /// A condition inside a flow: tags and settings must exist, the steps
    /// it reads must be steps of the flow (or stages) that store what it
    /// tests, and it must be possible for it to hold.
    fn check_flow_condition(&mut self, condition: &Condition, loc: &Location, ctx: &FlowCtx<'_>) {
        for tag in condition.tags() {
            self.check_tag_ref(tag, loc);
        }
        self.check_setting_refs(condition, loc);
        let mut problems: Vec<String> = Vec::new();
        condition.visit(&mut |c| match c {
            Condition::ResultIs { step, results } => match ctx.infos.get(step) {
                None => {}
                Some(info) if !info.result => problems.push(format!(
                    "step “{}” does not give a result to test",
                    info.label
                )),
                Some(info) => {
                    if let Some(table) = info.table.as_ref().and_then(|t| self.tables.get(t)) {
                        for r in results.iter().filter(|r| table.result(r).is_none()) {
                            problems.push(format!(
                                "“{}” is not a result of {} (step “{}”)",
                                r, table.name, info.label
                            ));
                        }
                    }
                    if results.is_empty() {
                        problems.push(format!("no result of step “{}” is listed", info.label));
                    }
                }
            },
            Condition::ValueIn { step, min, max } => {
                if let Some(info) = ctx.infos.get(step)
                    && !info.value
                {
                    problems.push(format!("step “{}” stores no number", info.label));
                }
                if let (Some(a), Some(b)) = (min, max)
                    && a > b
                {
                    problems.push(format!("no number is at least {a} and at most {b}"));
                }
            }
            Condition::SettingIs { setting, value } | Condition::SettingHas { setting, value } => {
                if let Some(def) = self.content.setting(setting)
                    && !matches!(def.kind, SettingKind::Text { .. })
                    && !def.kind.options().is_empty()
                    && !def.kind.options().iter().any(|o| &o.id == value)
                {
                    problems.push(format!("setting {setting} has no option “{value}”"));
                }
            }
            Condition::SettingBetween {
                min: Some(a),
                max: Some(b),
                ..
            } if a > b => {
                problems.push(format!("no number is at least {a} and at most {b}"));
            }
            Condition::Any(items) if items.is_empty() => {
                problems.push("an empty “any of” group can never hold".into());
            }
            Condition::All(items) => {
                // A test and its opposite together can never hold.
                for item in items {
                    if let Condition::Not(inner) = item
                        && items.iter().any(|other| other == &**inner)
                    {
                        problems.push(format!(
                            "it requires {} and its opposite at once",
                            inner.describe_test().unwrap_or_else(|| "a test".into())
                        ));
                    }
                }
            }
            _ => {}
        });
        for step in condition.steps() {
            if !ctx.infos.contains_key(step) {
                self.error(
                    loc.clone(),
                    IssueKind::MissingStep {
                        step: step.to_string(),
                    },
                );
            }
        }
        for reason in problems {
            self.error(loc.clone(), IssueKind::ImpossibleCondition { reason });
        }
    }

    pub(crate) fn check_setting_refs(&mut self, condition: &Condition, loc: &Location) {
        for setting in condition.settings() {
            let known = setting.as_str() == super::GOD_MODE_SETTING
                || super::canonical_setting(setting).as_str() == super::BARE_MINIMUM_BUILDINGS
                || self.content.setting(setting).is_some();
            if !known {
                self.error(
                    loc.clone(),
                    IssueKind::MissingSetting {
                        setting: setting.to_string(),
                    },
                );
            }
        }
        // Each test must suit the kind of setting it tests.
        let mut wrong = Vec::new();
        condition.visit(&mut |c| {
            let (setting, wants) = match c {
                Condition::SettingOn(s) => (s, "an on/off setting"),
                Condition::SettingIs { setting, .. } => (setting, "a dropdown or text setting"),
                Condition::SettingBetween { setting, .. } => (setting, "a number setting"),
                Condition::SettingHas { setting, .. } => (setting, "a multi-select setting"),
                _ => return,
            };
            let Some(def) = self.content.setting(setting) else {
                return;
            };
            let fits = matches!(
                (c, &def.kind),
                (Condition::SettingOn(_), SettingKind::Toggle { .. })
                    | (
                        Condition::SettingIs { .. },
                        SettingKind::Choice { .. } | SettingKind::Text { .. }
                    )
                    | (Condition::SettingBetween { .. }, SettingKind::Number { .. })
                    | (
                        Condition::SettingHas { .. },
                        SettingKind::MultiSelect { .. }
                    )
            );
            if !fits {
                wrong.push(IssueKind::SettingKindMismatch {
                    setting: setting.to_string(),
                    expected: wants.into(),
                    actual: def.kind.label().into(),
                });
            }
        });
        for kind in wrong {
            self.error(loc.clone(), kind);
        }
    }

    /// Flows that run each other in a loop would never finish, unless every
    /// step in the loop limits its depth (deliberate, guarded recursion,
    /// which is reported as a warning).
    fn check_flow_cycles(&mut self) {
        let content = self.content;
        let index: HashMap<&FlowId, usize> = content
            .flows
            .iter()
            .enumerate()
            .rev()
            .map(|(i, f)| (&f.id, i))
            .collect();
        let mut all = vec![BTreeSet::new(); content.flows.len()];
        let mut unguarded = vec![BTreeSet::new(); content.flows.len()];
        let mut limits: HashMap<usize, u32> = HashMap::new();
        for (i, flow) in content.flows.iter().enumerate() {
            for step in flow.all_steps().into_iter().filter(|s| s.runs()) {
                if let StepAction::RunFlow {
                    flow: target,
                    max_depth,
                } = &step.action
                    && let Some(&j) = index.get(target)
                {
                    all[i].insert(j);
                    match max_depth {
                        Some(d) => {
                            let limit = limits.entry(i).or_insert(*d);
                            *limit = (*limit).max(*d);
                        }
                        None => {
                            unguarded[i].insert(j);
                        }
                    }
                }
            }
        }
        let names = |component: &[usize]| -> Vec<String> {
            component
                .iter()
                .map(|&i| content.flows[i].id.to_string())
                .collect()
        };
        let errors = crate::validation::cyclic_components(&unguarded);
        let covered: HashSet<usize> = errors.iter().flatten().copied().collect();
        for component in errors {
            let flows = names(&component);
            self.error(
                Location {
                    flow: Some(flows[0].clone()),
                    ..Default::default()
                },
                IssueKind::FlowCycle { flows },
            );
        }
        for component in crate::validation::cyclic_components(&all) {
            if component.iter().any(|i| covered.contains(i)) {
                continue;
            }
            let depth = component
                .iter()
                .filter_map(|i| limits.get(i))
                .copied()
                .max()
                .unwrap_or(1);
            let flows = names(&component);
            self.warning(
                Location {
                    flow: Some(flows[0].clone()),
                    ..Default::default()
                },
                IssueKind::LimitedRecursion { flows, depth },
            );
        }
    }
}

/// What the checks of one flow need to know.
#[derive(Clone, Copy)]
struct FlowCtx<'a> {
    flow: &'a Location,
    /// Checking the top level of the start flow (stages).
    top: bool,
    infos: &'a HashMap<StepId, StepInfo>,
    /// Object types some step creates.
    created: &'a HashSet<ObjectTypeId>,
}

/// What a step stores for later conditions and counts.
struct StepInfo {
    label: String,
    table: Option<TableId>,
    /// It stores a result (a roll, a fixed result, a choice setting).
    result: bool,
    /// It stores a number.
    value: bool,
}

/// The object field a step sets directly, if any.
fn step_field(step: &FlowStep) -> Option<&str> {
    match &step.action {
        StepAction::Roll { field, .. } | StepAction::SetResult { field, .. } => field.as_deref(),
        StepAction::SetValue { field, .. } => Some(field),
        _ => None,
    }
}
