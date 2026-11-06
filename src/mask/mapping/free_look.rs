//! free look：无点击的"鼠标轨迹 = 角色转头"（"我的世界"式，`docs/22` §22.16）。
//!
//! 交互（D9–D14 已定）：
//! - **退出键切换**（默认 Ctrl，可配置 `free_look_exit_key`）：第一次按下进入、再次按下退出；
//! - **进入时锁光标**（`NormalCursorCapture` → `Locked` + 隐藏光标），**退出时不锁**；
//! - **锁光标期间任何映射按键都不得禁用**；只暂停"屏幕触控点击"
//!   （`cursor.rs::handle_normal_left_click` 会检查 `is_active()`），避免边转视角边误点 UI；
//! - 退出后左键点击恢复为正常触控（pointer 0，设计如此）。
//!
//! 手感：鼠标位移累计到阈值后由 [`TouchPulse`] 分帧发出 Down/Move/Up，
//! 每次从屏幕中心落下；灵敏度与翻转走 `InputEffects`。

use bevy::{
    input::{ButtonInput, keyboard::KeyCode, mouse::AccumulatedMouseMotion},
    prelude::*,
    window::Window,
};

use crate::{
    config::LocalConfig,
    mask::{
        mapping::{
            config::ActiveMappingConfig,
            cursor::{CursorState, NormalCursorCapture},
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
const FREE_LOOK_STEP_PX: f32 = 8.0;
/// 单次滑动相对 mask 短边的最大位移比例，防止一次甩动把触点送出屏幕。
const FREE_LOOK_MAX_RADIUS_RATIO: f32 = 0.28;
/// 光标捕获的所有者标识（与 Observation 的 owner 区分开）。
const FREE_LOOK_CAPTURE_OWNER: &str = "FreeLook";

#[derive(Resource)]
pub struct FreeLookState {
    /// 手势脉冲状态机（Down → Move → Up 分帧推进），见 `touch_pulse.rs`。
    pulse: TouchPulse,
    /// 退出键切换后的激活状态：激活 = 锁光标 + 转视角；未激活 = 不锁 + 点击正常触控。
    active: bool,
}

impl FreeLookState {
    /// 是否处于激活（锁光标）状态。`cursor.rs` 用它暂停屏幕触控点击。
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Default for FreeLookState {
    fn default() -> Self {
        Self {
            pulse: TouchPulse::new(TouchPulseConfig {
                pointer_id: FREE_LOOK_POINTER_ID,
                step_px: FREE_LOOK_STEP_PX,
                max_radius_ratio: FREE_LOOK_MAX_RADIUS_RATIO,
            }),
            active: false,
        }
    }
}

/// 配置里的退出键名 → Bevy `KeyCode`。
///
/// 只覆盖设置页提供的选项；未知键返回 `None`（不响应，避免误触）。
fn exit_keycode() -> Option<KeyCode> {
    let name = LocalConfig::get().free_look_exit_key;
    Some(match name.as_str() {
        "ControlLeft" => KeyCode::ControlLeft,
        "ControlRight" => KeyCode::ControlRight,
        "ShiftLeft" => KeyCode::ShiftLeft,
        "ShiftRight" => KeyCode::ShiftRight,
        "AltLeft" => KeyCode::AltLeft,
        "AltRight" => KeyCode::AltRight,
        "Space" => KeyCode::Space,
        "Tab" => KeyCode::Tab,
        "Escape" => KeyCode::Escape,
        "KeyC" => KeyCode::KeyC,
        "KeyF" => KeyCode::KeyF,
        "KeyG" => KeyCode::KeyG,
        "KeyV" => KeyCode::KeyV,
        "Backquote" => KeyCode::Backquote,
        _ => return None,
    })
}

pub fn free_look_init(mut commands: Commands) {
    commands.insert_resource(FreeLookState::default());
}

/// 停止映射时：退出激活、释放光标捕获、补一个 Up。
pub fn cleanup_free_look_on_stop(
    cs_tx_res: Res<ChannelSenderCS>,
    mask_size: Res<MaskSize>,
    mut capture: ResMut<NormalCursorCapture>,
    mut state: ResMut<FreeLookState>,
) {
    state.active = false;
    capture.release(FREE_LOOK_CAPTURE_OWNER);
    state.pulse.release(&cs_tx_res, mask_size.0);
}

pub fn handle_free_look(
    keys: Res<ButtonInput<KeyCode>>,
    accumulated_motion: Res<AccumulatedMouseMotion>,
    window: Single<&Window>,
    active_mapping: Res<ActiveMappingConfig>,
    active_observation: Res<ActiveObservationMap>,
    cs_tx_res: Res<ChannelSenderCS>,
    mask_size: Res<MaskSize>,
    cursor_state: Res<State<CursorState>>,
    mut capture: ResMut<NormalCursorCapture>,
    mut state: ResMut<FreeLookState>,
) {
    let size = mask_size.0;

    // 1) 总开关关闭 → 强制退出（不锁光标、不发触点）。
    if !LocalConfig::free_look_enabled() {
        if state.active {
            state.active = false;
            capture.release(FREE_LOOK_CAPTURE_OWNER);
        }
        if !state.pulse.is_idle() || state.pulse.has_pending() {
            state.pulse.release(&cs_tx_res, size);
        }
        return;
    }

    // 2) 退出键切换：进入需有活跃映射，退出随时可用。
    if let Some(exit_key) = exit_keycode() {
        if keys.just_pressed(exit_key) {
            if state.active {
                state.active = false;
                capture.release(FREE_LOOK_CAPTURE_OWNER);
                state.pulse.release(&cs_tx_res, size);
            } else if active_mapping.0.is_some() {
                state.active = true;
                capture.request(FREE_LOOK_CAPTURE_OWNER);
            }
        }
    }

    if !state.active {
        if !state.pulse.is_idle() || state.pulse.has_pending() {
            state.pulse.release(&cs_tx_res, size);
        }
        return;
    }

    // 3) 失焦 / 尺寸无效：收掉当前脉冲，保持激活状态等恢复。
    if !window.focused || size.x <= 0.0 || size.y <= 0.0 {
        if !state.pulse.is_idle() || state.pulse.has_pending() {
            state.pulse.release(&cs_tx_res, size);
        }
        return;
    }

    // 4) 避让：fps 模式由 Fps 映射驱动；按住观察键时由 Observation 驱动。
    //    不退出激活，只是这一帧不驱动视角。
    let blocked = cursor_state.get() == &CursorState::Fps || !active_observation.is_empty();
    if blocked {
        if !state.pulse.is_idle() || state.pulse.has_pending() {
            state.pulse.release(&cs_tx_res, size);
        }
        return;
    }

    // 5) 正常驱动：鼠标位移 → 输入效果（翻转/增益）→ 累计 → 分帧脉冲。
    let anchor = size * 0.5;
    state
        .pulse
        .accumulate(LocalConfig::free_look_effects().apply(accumulated_motion.delta));
    state.pulse.tick(&cs_tx_res, size, anchor);
}
