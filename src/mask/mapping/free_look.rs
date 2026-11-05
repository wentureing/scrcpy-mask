//! free look：无点击的"鼠标轨迹 = 角色转头"。
//!
//! 窗口聚焦且有设备在映射时，鼠标位移直接合成一次"落下 → 移动 → 松手"的滑动手势，
//! 不需要按住任何键或鼠标键。手势按位移累计触发（默认 8 mask 像素一次），
//! 并且分帧发送 Down / Move / Up，避免同一帧三条事件被设备端当成 tap 丢弃。
//!
//! 灵敏度与翻转走通用的 `InputEffects`（与按钮级 `effects` 同一套结构），
//! 所以"翻转 X 轴"是映射层能力，不是这个模块的特例。
//!
//! 设计取舍见 `docs/19` §19.2。

use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*, window::Window};

use crate::{
    config::LocalConfig,
    mask::{
        mapping::{
            config::ActiveMappingConfig, cursor::CursorState, observation::ActiveObservationMap,
            utils::ControlMsgHelper,
        },
        mask_command::MaskSize,
    },
    scrcpy::constant::MotionEventAction,
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

/// 一次滑动手势的三个阶段，每帧最多推进一步。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum FreeLookPhase {
    #[default]
    Idle,
    /// 已发 Down，等下一帧发 Move。
    Pressed { end: Vec2 },
    /// 已发 Move，等下一帧发 Up。
    Released { end: Vec2 },
}

#[derive(Resource, Default)]
pub struct FreeLookState {
    /// 尚未达到阈值、还没发出去的累计位移（mask 像素）。
    pending: Vec2,
    phase: FreeLookPhase,
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
    release_touch(&mut state, &cs_tx_res, mask_size.0);
}

fn release_touch(state: &mut FreeLookState, cs_tx: &ChannelSenderCS, size: Vec2) {
    let pos = match state.phase {
        FreeLookPhase::Idle => None,
        FreeLookPhase::Pressed { end } | FreeLookPhase::Released { end } => Some(end),
    };
    if let Some(pos) = pos {
        ControlMsgHelper::send_touch(
            &cs_tx.0,
            MotionEventAction::Up,
            FREE_LOOK_POINTER_ID,
            size,
            pos,
        );
    }
    state.phase = FreeLookPhase::Idle;
    state.pending = Vec2::ZERO;
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
        if !matches!(state.phase, FreeLookPhase::Idle) || state.pending != Vec2::ZERO {
            release_touch(&mut state, &cs_tx_res, size);
        }
        return;
    }

    let anchor = size * 0.5;
    let max_radius = size.min_element() * FREE_LOOK_MAX_RADIUS_RATIO;
    // 输入效果统一处理灵敏度与翻转：翻转作用在"输入轴"上，
    // 所以 invert_x 就是"鼠标右移 → 视角左转"。
    state.pending += LocalConfig::free_look_effects().apply(accumulated_motion.delta);

    // 未完成的手势优先推进：每帧一步，让设备端看到真实的手势时序，
    // 而不是同一帧挤进 Down/Move/Up（那样容易被识别成 tap）。
    match state.phase {
        FreeLookPhase::Idle => {}
        FreeLookPhase::Pressed { end } => {
            ControlMsgHelper::send_touch(
                &cs_tx_res.0,
                MotionEventAction::Move,
                FREE_LOOK_POINTER_ID,
                size,
                end,
            );
            state.phase = FreeLookPhase::Released { end };
            return;
        }
        FreeLookPhase::Released { end } => {
            ControlMsgHelper::send_touch(
                &cs_tx_res.0,
                MotionEventAction::Up,
                FREE_LOOK_POINTER_ID,
                size,
                end,
            );
            state.phase = FreeLookPhase::Idle;
            return;
        }
    }

    if state.pending.length() < FREE_LOOK_STEP_PX {
        return;
    }

    let mut delta = state.pending;
    state.pending = Vec2::ZERO;
    if max_radius > 0.0 {
        let len = delta.length();
        if len > max_radius {
            delta = delta / len * max_radius;
        }
    }

    let end = anchor + delta;
    ControlMsgHelper::send_touch(
        &cs_tx_res.0,
        MotionEventAction::Down,
        FREE_LOOK_POINTER_ID,
        size,
        anchor,
    );
    state.phase = FreeLookPhase::Pressed { end };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(phase: FreeLookPhase) -> FreeLookState {
        FreeLookState {
            pending: Vec2::new(4.0, -4.0),
            phase,
        }
    }

    #[test]
    fn idle_state_has_nothing_to_release() {
        let mut state = make_state(FreeLookPhase::Idle);
        let (tx, _rx) = tokio::sync::broadcast::channel(4);
        release_touch(&mut state, &ChannelSenderCS(tx), Vec2::new(100.0, 100.0));
        assert_eq!(state.phase, FreeLookPhase::Idle);
        assert_eq!(state.pending, Vec2::ZERO);
    }

    #[test]
    fn pressed_state_emits_up_on_release() {
        let mut state = make_state(FreeLookPhase::Pressed {
            end: Vec2::new(60.0, 40.0),
        });
        let (tx, mut rx) = tokio::sync::broadcast::channel(4);
        release_touch(&mut state, &ChannelSenderCS(tx), Vec2::new(100.0, 100.0));
        assert_eq!(state.phase, FreeLookPhase::Idle);
        assert_eq!(state.pending, Vec2::ZERO);

        let msg = rx.try_recv().expect("expected an injected touch event");
        match msg {
            crate::scrcpy::control_msg::ScrcpyControlMsg::InjectTouchEvent {
                action, x, y, ..
            } => {
                assert_eq!(action, MotionEventAction::Up);
                assert_eq!((x, y), (60, 40));
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }
}
