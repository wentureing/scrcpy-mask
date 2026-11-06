//! L1 统一触发守卫（`docs/26` §26.5.2）。
//!
//! 目标：把"某个映射此刻是否允许触发"收口到一处，避免 12 个 `handle_*` 各自硬编码。
//! 这里只做三件事：
//! 1. 前缀匹配（`action` 是否属于该映射类型）；
//! 2. 访问规则（`docs/26` 的 `mapping_id + 单一状态 → allow/deny`）；
//! 3. 触发方式（continuous / pulse）仍由调用方判断，保持原有语义不变。
//!
//! 状态由 [`update_access_state`] 每帧算一次（`PreUpdate`），按优先级取**单一**状态：
//! `raw_input > fps > observation > free_look_active > normal`。

use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    access_rules::{AccessRule, CompiledAccessRules, STATE_FPS, STATE_FREE_LOOK_ACTIVE, STATE_NORMAL, STATE_OBSERVATION, STATE_RAW_INPUT},
    config::LocalConfig,
    mask::mapping::{
        MappingState,
        cursor::CursorState,
        free_look::FreeLookState,
        observation::ActiveObservationMap,
    },
};

/// 每帧计算出的"当前单一状态"。
#[derive(Resource)]
pub struct AccessState(pub &'static str);

impl Default for AccessState {
    fn default() -> Self {
        Self(STATE_NORMAL)
    }
}

/// 规则表 + 原始配置（用于检测配置变化后重建）。
#[derive(Resource, Default)]
pub struct AccessRulesResource {
    source: Vec<AccessRule>,
    compiled: CompiledAccessRules,
}

impl AccessRulesResource {
    pub fn compiled(&self) -> &CompiledAccessRules {
        &self.compiled
    }
}

/// 按优先级选出当前状态（只选一个，不做集合）。
pub fn current_state(
    raw_input: bool,
    fps: bool,
    observation_active: bool,
    free_look_active: bool,
) -> &'static str {
    if raw_input {
        STATE_RAW_INPUT
    } else if fps {
        STATE_FPS
    } else if observation_active {
        STATE_OBSERVATION
    } else if free_look_active {
        STATE_FREE_LOOK_ACTIVE
    } else {
        STATE_NORMAL
    }
}

/// 每帧刷新状态与规则表（`PreUpdate`，早于所有 `handle_*`）。
pub fn update_access_state(
    mapping_state: Res<State<MappingState>>,
    cursor_state: Res<State<CursorState>>,
    observation: Res<ActiveObservationMap>,
    free_look: Res<FreeLookState>,
    mut access: ResMut<AccessState>,
    mut rules: ResMut<AccessRulesResource>,
) {
    access.0 = current_state(
        mapping_state.get() == &MappingState::RawInput,
        cursor_state.get() == &CursorState::Fps,
        !observation.is_empty(),
        free_look.is_active(),
    );

    // 规则通常 0–5 条，每帧比较一次即可；变化时才重建（避免每帧哈希构建）。
    let configured = LocalConfig::access_rules();
    if configured != rules.source {
        rules.source = configured;
        rules.compiled = CompiledAccessRules::compile(&rules.source);
    }
}

/// 统一判定：该映射在当前状态下是否允许触发。
///
/// 返回 `true` 表示放行；调用方仍需自行判断触发方式（`just_activated` / `just_pulsed`）。
pub fn allowed(rules: &AccessRulesResource, mapping_id: &str, state: &str) -> bool {
    rules.compiled.allowed(mapping_id, state)
}

/// 把"当前状态 + 规则表"合成一个系统参数。
///
/// 目的：`handle_*` 的参数个数受 Bevy 上限（16）约束；直接加两个参数会让
/// `handle_pad_cast_spell` / `handle_direction_pad` 超限。合成一个后每处只多 1 个参数。
#[derive(SystemParam)]
pub struct AccessContext<'w> {
    state: Res<'w, AccessState>,
    rules: Res<'w, AccessRulesResource>,
}

impl AccessContext<'_> {
    /// 该映射在当前状态下是否允许触发。
    pub fn allowed(&self, mapping_id: &str) -> bool {
        allowed(&self.rules, mapping_id, self.state.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_priority_is_fixed() {
        assert_eq!(current_state(true, true, true, true), STATE_RAW_INPUT);
        assert_eq!(current_state(false, true, true, true), STATE_FPS);
        assert_eq!(current_state(false, false, true, true), STATE_OBSERVATION);
        assert_eq!(
            current_state(false, false, false, true),
            STATE_FREE_LOOK_ACTIVE
        );
        assert_eq!(current_state(false, false, false, false), STATE_NORMAL);
    }

    #[test]
    fn allowed_delegates_to_compiled_rules() {
        let rules = AccessRulesResource {
            source: vec![AccessRule {
                mapping_id: "Fire-0".to_string(),
                when: STATE_FREE_LOOK_ACTIVE.to_string(),
                allow: false,
            }],
            compiled: CompiledAccessRules::compile(&[AccessRule {
                mapping_id: "Fire-0".to_string(),
                when: STATE_FREE_LOOK_ACTIVE.to_string(),
                allow: false,
            }]),
        };
        assert!(!allowed(&rules, "Fire-0", STATE_FREE_LOOK_ACTIVE));
        assert!(allowed(&rules, "Fire-0", STATE_NORMAL));
        assert!(allowed(&rules, "Other", STATE_FREE_LOOK_ACTIVE));
    }
}
