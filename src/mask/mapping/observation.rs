use std::collections::HashMap;

use bevy::{
    ecs::{
        resource::Resource,
        system::{Commands, Local, Res, ResMut, Single},
    },
    math::Vec2,
    state::state::State,
    window::Window,
};
use bevy_ineffable::prelude::{ContinuousBinding, Ineffable, InputBinding};
use serde::{Deserialize, Serialize};

use crate::{
    input_effects::InputEffects,
    mask::{
        mapping::{
            MappingState,
            binding::{ButtonBinding, ValidateMappingConfig},
            config::ActiveMappingConfig,
            free_look,
            cursor::{CursorPosition, CursorState, NormalCursorCapture},
            executor::{
                MappingLifecycleStart, MappingLifecycleState, make_mapping_execution_context,
                run_script_hook,
            },
            script::{BindMappingScriptHooks, MappingScriptHooks},
            script_helper::{ScriptRuntimeCommandSender, ScriptSharedState},
            utils::{ControlMsgHelper, Position, default_random_offset, random_offset_vec2},
            view_motion::ViewMotion,
        },
        mask_command::MaskSize,
    },
    scrcpy::constant::MotionEventAction,
    tokio_tasks::TokioTasksRuntime,
    utils::ChannelSenderCS,
};

pub fn init_observation(mut commands: Commands) {
    commands.insert_resource(ActiveObservationMap::default());
    commands.insert_resource(ObservationLifecycleState::default());
}

pub fn cleanup_observation_on_stop(
    active_mapping: Res<ActiveMappingConfig>,
    cs_tx_res: Res<ChannelSenderCS>,
    cursor_pos: Res<CursorPosition>,
    mask_size: Res<MaskSize>,
    mut active_map: ResMut<ActiveObservationMap>,
    mut lifecycle_state: ResMut<ObservationLifecycleState>,
    mut normal_cursor_capture: ResMut<NormalCursorCapture>,
) {
    for (action, item) in active_map.0.drain() {
        let delta = item.view_delta(cursor_pos.0);
        ControlMsgHelper::send_touch(
            &cs_tx_res.0,
            MotionEventAction::Up,
            item.pointer_id,
            mask_size.0,
            item.mask_pos + delta,
        );
        normal_cursor_capture.release(&observation_capture_owner(&action));
    }

    if let Some(active_mapping) = &active_mapping.0 {
        for action in active_mapping.mappings.keys() {
            if action.as_ref().starts_with("Observation") {
                normal_cursor_capture.release(&observation_capture_owner(action.as_ref()));
            }
        }
    }
    lifecycle_state.0.clear_all();
}

#[derive(Debug, Clone)]
pub struct BindMappingObservation {
    pub id: String,
    pub note: String,
    pub pointer_id: u64,
    pub position: Position,
    pub sensitivity_x: f32,
    pub sensitivity_y: f32,
    pub bind: ButtonBinding,
    pub input_binding: InputBinding,
    pub random_offset_x: f32,
    pub random_offset_y: f32,
    pub max_radius: f32,
    pub effects: InputEffects,
    pub script_hooks: BindMappingScriptHooks,
}

impl From<MappingObservation> for BindMappingObservation {
    fn from(value: MappingObservation) -> Self {
        Self {
            id: value.id,
            note: value.note,
            pointer_id: value.pointer_id,
            position: value.position,
            sensitivity_x: value.sensitivity_x,
            sensitivity_y: value.sensitivity_y,
            bind: value.bind.clone(),
            input_binding: ContinuousBinding::hold(value.bind).0,
            random_offset_x: value.random_offset_x,
            random_offset_y: value.random_offset_y,
            max_radius: value.max_radius,
            effects: value.effects,
            script_hooks: value.script_hooks.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MappingObservation {
    #[serde(default = "crate::mask::mapping::config::default_mapping_id")]
    pub id: String,
    pub note: String,
    pub pointer_id: u64,
    pub position: Position,
    #[serde(serialize_with = "crate::mask::mapping::serde_float::serialize_f32_3dp")]
    pub sensitivity_x: f32,
    #[serde(serialize_with = "crate::mask::mapping::serde_float::serialize_f32_3dp")]
    pub sensitivity_y: f32,
    pub bind: ButtonBinding,
    #[serde(
        default = "default_random_offset",
        serialize_with = "crate::mask::mapping::serde_float::serialize_f32_3dp"
    )]
    pub random_offset_x: f32,
    #[serde(
        default = "default_random_offset",
        serialize_with = "crate::mask::mapping::serde_float::serialize_f32_3dp"
    )]
    pub random_offset_y: f32,
    #[serde(
        default,
        serialize_with = "crate::mask::mapping::serde_float::serialize_f32_3dp"
    )]
    pub max_radius: f32,
    /// 输入效果（翻转/增益）。缺省即旧行为。
    #[serde(default)]
    pub effects: InputEffects,
    #[serde(default)]
    pub script_hooks: MappingScriptHooks,
}

impl ValidateMappingConfig for MappingObservation {
    fn validate(&self) -> Result<(), String> {
        self.script_hooks.validate()
    }
}

#[derive(Resource, Default)]
pub struct ActiveObservationMap(HashMap<String, ObservationItem>);

impl ActiveObservationMap {
    /// free look 用它判断"当前是否有观察映射按着"，避免两边同时驱动视角。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Resource, Default)]
pub struct ObservationLifecycleState(MappingLifecycleState<ObservationReleaseContext>);

#[derive(Clone)]
struct ObservationReleaseContext {
    mask_size: Vec2,
    cursor_pos: Vec2,
    raw_input_flag: bool,
    fps_mode_flag: bool,
}

fn observation_capture_owner(action: &str) -> String {
    format!("Observation:{action}")
}

struct ObservationItem {
    start_cursor_pos: Vec2,
    mask_pos: Vec2,
    pointer_id: u64,
    /// 位移换算（输入效果 → 灵敏度 → 最大半径）统一由 `ViewMotion` 负责。
    motion: ViewMotion,
}

impl ObservationItem {
    /// 视角位移：起点到当前光标位置，经 `ViewMotion` 换算。
    fn view_delta(&self, cursor_pos: Vec2) -> Vec2 {
        self.motion.delta_since(self.start_cursor_pos, cursor_pos)
    }
}

fn observation_has_before_hook(mapping: &BindMappingObservation) -> bool {
    !mapping.script_hooks.before_script_ast.empty
}

fn observation_has_after_hook(mapping: &BindMappingObservation) -> bool {
    !mapping.script_hooks.after_script_ast.empty
}

fn apply_observation_down(
    cs_tx: &ChannelSenderCS,
    active_map: &mut ActiveObservationMap,
    action: String,
    mapping: &BindMappingObservation,
    original_size: Vec2,
    mask_size: Vec2,
    cursor_pos: Vec2,
) {
    let original_pos: Vec2 = mapping.position.into();
    let original_pos = random_offset_vec2(
        original_pos,
        Vec2::new(mapping.random_offset_x, mapping.random_offset_y),
    );
    let sensitivity: Vec2 = (mapping.sensitivity_x, mapping.sensitivity_y).into();
    let pointer_id = mapping.pointer_id;
    let mask_pos = original_pos / original_size * mask_size;

    ControlMsgHelper::send_touch(
        &cs_tx.0,
        MotionEventAction::Down,
        pointer_id,
        mask_size,
        mask_pos,
    );
    active_map.0.insert(
        action,
        ObservationItem {
            start_cursor_pos: cursor_pos,
            mask_pos,
            pointer_id,
            motion: ViewMotion::new(mapping.effects, sensitivity, mapping.max_radius),
        },
    );
}

fn apply_observation_up(
    cs_tx: &ChannelSenderCS,
    active_map: &mut ActiveObservationMap,
    action: &str,
    mask_size: Vec2,
    cursor_pos: Vec2,
) -> bool {
    if let Some(item) = active_map.0.remove(action) {
        let delta = item.view_delta(cursor_pos);
        ControlMsgHelper::send_touch(
            &cs_tx.0,
            MotionEventAction::Up,
            item.pointer_id,
            mask_size,
            item.mask_pos + delta,
        );
        true
    } else {
        false
    }
}

pub fn handle_observation_trigger(
    cs_tx_res: Res<ChannelSenderCS>,
    mask_size: Res<MaskSize>,
    cursor_pos: Res<CursorPosition>,
    mut active_map: ResMut<ActiveObservationMap>,
) {
    for (_, item) in active_map.0.iter_mut() {
        let delta = item.view_delta(cursor_pos.0);
        ControlMsgHelper::send_touch(
            &cs_tx_res.0,
            MotionEventAction::Move,
            item.pointer_id,
            mask_size.0,
            item.mask_pos + delta,
        );
    }
}

pub fn handle_observation(
    ineffable: Res<Ineffable>,
    active_mapping: Res<ActiveMappingConfig>,
    cs_tx_res: Res<ChannelSenderCS>,
    script_command_tx: Res<ScriptRuntimeCommandSender>,
    shared_state: Res<ScriptSharedState>,
    mask_size: Res<MaskSize>,
    cursor_pos: Res<CursorPosition>,
    mapping_state: Res<State<MappingState>>,
    cursor_state: Res<State<CursorState>>,
    runtime: ResMut<TokioTasksRuntime>,
    mut active_map: ResMut<ActiveObservationMap>,
    mut lifecycle_state: ResMut<ObservationLifecycleState>,
    mut normal_cursor_capture: ResMut<NormalCursorCapture>,
    free_look: Res<free_look::FreeLookState>,
) {
    if let Some(active_mapping) = &active_mapping.0 {
        for (action, mapping) in &active_mapping.mappings {
            if action.as_ref().starts_with("Observation") {
                let mapping = mapping.as_ref_observation();
                if ineffable.just_activated(action.ineff_continuous())
                    && !free_look::is_blocked_in_free_look(
                        &mapping.bind,
                        free_look.is_active(),
                    )
                {
                    let original_size: Vec2 = active_mapping.original_size.into();
                    let capture_owner = observation_capture_owner(action.as_ref());
                    let capture_requested = cursor_state.get() != &CursorState::Fps;
                    if capture_requested {
                        normal_cursor_capture.request(capture_owner.clone());
                    }
                    if observation_has_before_hook(mapping) {
                        let action = action.to_string();
                        let version = lifecycle_state.0.begin_start(&action);
                        let mapping = mapping.clone();
                        let before_script_ast = mapping.script_hooks.before_script_ast.clone();
                        let after_script_ast = mapping.script_hooks.after_script_ast.clone();
                        let exec_ctx = make_mapping_execution_context(
                            &cs_tx_res,
                            &script_command_tx,
                            &shared_state,
                            mapping.id.clone(),
                            original_size,
                            cursor_pos.0,
                            mask_size.0,
                            mapping_state.get() == &MappingState::RawInput,
                            cursor_state.get() == &CursorState::Fps,
                        );
                        let cs_tx = cs_tx_res.0.clone();
                        let error_capture_owner = capture_owner.clone();
                        runtime.spawn_background_task(move |mut task_ctx| async move {
                            if let Err(e) = run_script_hook(&before_script_ast, &exec_ctx).await {
                                task_ctx
                                    .run_on_main_thread(move |main_ctx| {
                                        main_ctx
                                            .world
                                            .resource_mut::<ObservationLifecycleState>()
                                            .0
                                            .cancel_start(&action, version);
                                        if capture_requested {
                                            main_ctx
                                                .world
                                                .resource_mut::<NormalCursorCapture>()
                                                .release(&error_capture_owner);
                                        }
                                    })
                                    .await;
                                log::error!("[Observation] script hook runtime error: {:?}", e);
                                return;
                            }

                            let pending_release = task_ctx
                                .run_on_main_thread(move |main_ctx| {
                                    let start = main_ctx
                                        .world
                                        .resource_mut::<ObservationLifecycleState>()
                                        .0
                                        .finish_start(&action, version);
                                    let pending_release = match start {
                                        MappingLifecycleStart::Stale => return None,
                                        MappingLifecycleStart::Ready { pending_release } => {
                                            pending_release
                                        }
                                    };

                                    let mask_size = pending_release
                                        .as_ref()
                                        .map(|release| release.mask_size)
                                        .unwrap_or_else(|| main_ctx.world.resource::<MaskSize>().0);
                                    let cursor_pos = pending_release
                                        .as_ref()
                                        .map(|release| release.cursor_pos)
                                        .unwrap_or_else(|| {
                                            main_ctx.world.resource::<CursorPosition>().0
                                        });
                                    let mut active_map =
                                        main_ctx.world.resource_mut::<ActiveObservationMap>();
                                    apply_observation_down(
                                        &ChannelSenderCS(cs_tx.clone()),
                                        &mut active_map,
                                        action.clone(),
                                        &mapping,
                                        original_size,
                                        mask_size,
                                        cursor_pos,
                                    );

                                    if let Some(release) = &pending_release {
                                        apply_observation_up(
                                            &ChannelSenderCS(cs_tx),
                                            &mut active_map,
                                            &action,
                                            release.mask_size,
                                            release.cursor_pos,
                                        );
                                        main_ctx
                                            .world
                                            .resource_mut::<NormalCursorCapture>()
                                            .release(&observation_capture_owner(&action));
                                    }

                                    pending_release
                                })
                                .await;

                            if let Some(release) = pending_release {
                                if !after_script_ast.empty {
                                    let mut after_exec_ctx = exec_ctx.clone();
                                    after_exec_ctx.cursor_pos = release.cursor_pos;
                                    after_exec_ctx.mask_size = release.mask_size;
                                    after_exec_ctx.raw_input_flag = release.raw_input_flag;
                                    after_exec_ctx.fps_mode_flag = release.fps_mode_flag;
                                    if let Err(e) =
                                        run_script_hook(&after_script_ast, &after_exec_ctx).await
                                    {
                                        log::error!(
                                            "[Observation] script hook runtime error: {:?}",
                                            e
                                        );
                                    }
                                }
                            }
                        });
                    } else {
                        apply_observation_down(
                            &cs_tx_res,
                            &mut active_map,
                            action.to_string(),
                            mapping,
                            original_size,
                            mask_size.0,
                            cursor_pos.0,
                        );
                    }
                } else if ineffable.just_deactivated(action.ineff_continuous()) {
                    let released = apply_observation_up(
                        &cs_tx_res,
                        &mut active_map,
                        action.as_ref(),
                        mask_size.0,
                        cursor_pos.0,
                    );

                    if released {
                        lifecycle_state.0.clear_pending(action.as_ref());
                        normal_cursor_capture.release(&observation_capture_owner(action.as_ref()));
                    } else if observation_has_before_hook(mapping) {
                        lifecycle_state.0.record_early_release(
                            action.as_ref(),
                            ObservationReleaseContext {
                                mask_size: mask_size.0,
                                cursor_pos: cursor_pos.0,
                                raw_input_flag: mapping_state.get() == &MappingState::RawInput,
                                fps_mode_flag: cursor_state.get() == &CursorState::Fps,
                            },
                        );
                        normal_cursor_capture.release(&observation_capture_owner(action.as_ref()));
                    }

                    if released && observation_has_after_hook(mapping) {
                        let after_script_ast = mapping.script_hooks.after_script_ast.clone();
                        let exec_ctx = make_mapping_execution_context(
                            &cs_tx_res,
                            &script_command_tx,
                            &shared_state,
                            mapping.id.clone(),
                            active_mapping.original_size.into(),
                            cursor_pos.0,
                            mask_size.0,
                            mapping_state.get() == &MappingState::RawInput,
                            cursor_state.get() == &CursorState::Fps,
                        );
                        runtime.spawn_background_task(move |_task_ctx| async move {
                            let result = run_script_hook(&after_script_ast, &exec_ctx).await;
                            if let Err(e) = result {
                                log::error!("[Observation] script hook runtime error: {:?}", e);
                            }
                        });
                    }
                }
            }
        }
    }
}

pub fn handle_observation_focus_lost(
    window: Single<&Window>,
    mut was_focused: Local<bool>,
    active_mapping: Res<ActiveMappingConfig>,
    cs_tx_res: Res<ChannelSenderCS>,
    script_command_tx: Res<ScriptRuntimeCommandSender>,
    shared_state: Res<ScriptSharedState>,
    mask_size: Res<MaskSize>,
    cursor_pos: Res<CursorPosition>,
    mapping_state: Res<State<MappingState>>,
    cursor_state: Res<State<CursorState>>,
    runtime: ResMut<TokioTasksRuntime>,
    mut active_map: ResMut<ActiveObservationMap>,
    mut lifecycle_state: ResMut<ObservationLifecycleState>,
    mut normal_cursor_capture: ResMut<NormalCursorCapture>,
) {
    let lost_focus = *was_focused && !window.focused;
    *was_focused = window.focused;
    if !lost_focus {
        return;
    }

    let Some(active_mapping) = &active_mapping.0 else {
        return;
    };

    let original_size: Vec2 = active_mapping.original_size.into();
    let raw_input_flag = mapping_state.get() == &MappingState::RawInput;
    let fps_mode_flag = cursor_state.get() == &CursorState::Fps;

    let active_actions: Vec<String> = active_map.0.keys().cloned().collect();
    for action in active_actions {
        let released = apply_observation_up(
            &cs_tx_res,
            &mut active_map,
            &action,
            mask_size.0,
            cursor_pos.0,
        );
        normal_cursor_capture.release(&observation_capture_owner(&action));
        lifecycle_state.0.clear_pending(&action);

        if !released {
            continue;
        }

        let Some((_, mapping)) = active_mapping
            .mappings
            .iter()
            .find(|(mapping_action, _)| mapping_action.as_ref() == action)
        else {
            continue;
        };

        let mapping = mapping.as_ref_observation();
        if !observation_has_after_hook(mapping) {
            continue;
        }

        let after_script_ast = mapping.script_hooks.after_script_ast.clone();
        let exec_ctx = make_mapping_execution_context(
            &cs_tx_res,
            &script_command_tx,
            &shared_state,
            mapping.id.clone(),
            original_size,
            cursor_pos.0,
            mask_size.0,
            raw_input_flag,
            fps_mode_flag,
        );
        runtime.spawn_background_task(move |_task_ctx| async move {
            if let Err(e) = run_script_hook(&after_script_ast, &exec_ctx).await {
                log::error!("[Observation] script hook runtime error: {:?}", e);
            }
        });
    }

    for action in active_mapping.mappings.keys() {
        if action.as_ref().starts_with("Observation") {
            lifecycle_state.0.cancel_pending(action.as_ref());
            normal_cursor_capture.release(&observation_capture_owner(action.as_ref()));
        }
    }
}
