//! free look：无点击的"鼠标轨迹 = 角色转头"。
//!
//! 窗口聚焦且有设备在映射时，鼠标位移直接合成一次"落下 → 移动 → 松手"的滑动手势，
//! 不需要按住任何键或鼠标键。手势按位移累计触发（默认 8 mask 像素一次），
//! 并且分帧发送 Down / Move / Up（由 [`TouchPulse`] 负责），避免同一帧三条事件被设备端当成 tap。
//!
//! 灵敏度与翻转走通用的 [`crate::input_effects::InputEffects`]，
//! 所以"翻转 X 轴"是映射层能力，不是这个模块的特例。
//!
//! 设计取舍见 `docs/19` §19.2；本轮迁移见 `docs/21` P3 / S3。

use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*, window::Window};

use crate::{
    config::LocalConfig,
    mask::{
        mapping::{
            config::ActiveMappingConfig,
            cursor::CursorState,
            observation::ActiveObservationMap,
            touch_pulse::{TouchPulse, TouchPulseConfig},
        },
        mask_command::MaskSize,
    },
    utils::ChannelSenderCS,
};

/// free look 专用的触点 ID。
///
/// 取 15：避开常规映射常用的 0-9，同时仍在 Android `MAX_POINTERS`（16）以内——
/// 超出这个范围部分机器/游戏会把 id 当数组下标用而丢事件。
const FREE_LOOK_POINTER_ID: u64 = 15;
/// 累计位移达到该值（mask 像素）才合成一次滑动，避免每帧一个触点事件。
///
/// 8px 是"手感与事件量"的折中：鼠标快速移动时约每 2-3 帧就发一次，
/// 视角更新频率接近 20-30Hz，既不会碎成点击，也不会明显顿挫。
const FREE_LOOK_STEP_PX: f32 = 8.0;
/// 单次滑动相对 mask 短边的最大位移比例，防止一次甩动把触点送出屏幕。
/// 超出部分会被丢弃（甩动时损失一点位移，换取触点始终在屏幕内）。
const FREE_LOOK_MAX_RADIUS_RATIO: f32 = 0.28;

#[derive(Resource)]
pub struct FreeLookState {
    /// 手势脉冲状态机（Down → Move → Up 分帧推进），见 `touch_pulse.rs`。
    pulse: TouchPulse,
}

impl Default for FreeLookState {
    fn default() -> Self {
        Self {
            pulse: TouchPulse::new(TouchPulseConfig {
                pointer_id: FREE_LOOK_POINTER_ID,
                step_px: FREE_LOOK_STEP_PX,
                max_radius_ratio: FREE_LOOK_MAX_RADIUS_RATIO,
            }),
        }
    }
}

pub fn free_look_init(mut commands: Commands) {
    commands.insert_resource(FreeLookState::default());
}

/// 停止映射时不能留下按下状态：补一个 Up 并清空累计位移。
pub fn cleanup_free_look_on_stop(
    cs_tx_res: Res<ChannelSenderCS>,
    mask_size: Res<MaskSize>,
    mut state: ResMut<FreeLookState>,
) {
    state.pulse.release(&cs_tx_res, mask_size.0);
}

pub fn handle_free_look(
    accumulated_motion: Res<AccumulatedMouseMotion>,
    window: Single<&Window>,
    active_mapping: Res<ActiveMappingConfig>,
    active_observation: Res<ActiveObservationMap>,
    cs_tx_res: Res<ChannelSenderCS>,
    mask_size: Res<MaskSize>,
    cursor_state: Res<State<CursorState>>,
    mut state: ResMut<FreeLookState>,
) {
    let size = mask_size.0;
    let enabled = LocalConfig::free_look_enabled() && active_mapping.0.is_some();
    // fps 模式下视角由 Fps 映射驱动；按住观察键时由 Observation 映射驱动。
    // 两种情况都跳过，避免双重驱动同一个视角。
    let blocked = cursor_state.get() == &CursorState::Fps || !active_observation.is_empty();

    if !enabled || blocked || !window.focused || size.x <= 0.0 || size.y <= 0.0 {
        if !state.pulse.is_idle() || state.pulse.has_pending() {
            state.pulse.release(&cs_tx_res, size);
        }
        return;
    }

    let anchor = size * 0.5;
    // 输入效果统一处理灵敏度与翻转：翻转作用在"输入轴"上，
    // 所以 invert_x 就是"鼠标右移 → 视角左转"。
    state
        .pulse
        .accumulate(LocalConfig::free_look_effects().apply(accumulated_motion.delta));
    // 每帧推进一步：Down → Move → Up 分帧发送，避免同帧三条被当成 tap。
    state.pulse.tick(&cs_tx_res, size, anchor);
}
