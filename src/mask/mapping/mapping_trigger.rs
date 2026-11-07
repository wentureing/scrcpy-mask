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
use std::sync::atomic::{AtomicU8, Ordering};

use crate::{
    access_rules::{AccessRule, CompiledAccessRules, CONDITION_FPS, CONDITION_FREE_LOOK_ACTIVE, CONDITION_NORMAL, CONDITION_OBSERVATION, CONDITION_RAW_INPUT},
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

/// 当前状态的全局快照（供 web 诊断接口读取；Bevy Resource 不能跨线程直接读）。
static CURRENT_STATE: AtomicU8 = AtomicU8::new(4);

fn state_index(state: &'static str) -> u8 {
    match state {
        CONDITION_RAW_INPUT => 0,
        CONDITION_FPS => 1,
        CONDITION_OBSERVATION => 2,
        CONDITION_FREE_LOOK_ACTIVE => 3,
        _ => 4,
    }
}

/// 当前访问状态字符串（诊断用）。
pub fn current_access_state() -> &'static str {
    match CURRENT_STATE.load(Ordering::Relaxed) {
        0 => CONDITION_RAW_INPUT,
        1 => CONDITION_FPS,
        2 => CONDITION_OBSERVATION,
        3 => CONDITION_FREE_LOOK_ACTIVE,
        _ => CONDITION_NORMAL,
    }
}

impl Default for AccessState {
    fn default() -> Self {
        Self(CONDITION_NORMAL)
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
        CONDITION_RAW_INPUT
    } else if fps {
        CONDITION_FPS
    } else if observation_active {
        CONDITION_OBSERVATION
    } else if free_look_active {
        CONDITION_FREE_LOOK_ACTIVE
    } else {
        CONDITION_NORMAL
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
    CURRENT_STATE.store(state_index(access.0), Ordering::Relaxed);

    // 规则通常 0–5 条，每帧比较一次即可；变化时才重建（避免每帧哈希构建）。
    let configured = LocalConfig::access_rules();
    if configured != rules.source {
        rules.source = configured;
        rules.compiled = CompiledAccessRules::compile(&rules.source);
    }
}

/// 统一判定：该映射（或其中某个操作）在当前条件下是否允许触发。
///
/// 返回 `true` 表示放行；调用方仍需自行判断触发方式（`just_activated` / `just_pulsed`）。
pub fn allowed(
    rules: &AccessRulesResource,
    mapping_id: &str,
    action: Option<&str>,
    state: &str,
) -> bool {
    rules.compiled.allowed(mapping_id, action, state)
}

/// 把"当前状态 + 规则表"合成一个系统参数。
///
/// 目的：`handle_*` 的参数个数受 Bevy 上限（16）约束；合成一个后每处只多 1 个参数。
#[derive(SystemParam)]
pub struct AccessContext<'w> {
    state: Res<'w, AccessState>,
    rules: Res<'w, AccessRulesResource>,
}

impl AccessContext<'_> {
    /// 该映射（或其中某个操作）在当前条件下是否允许触发。
    ///
    /// `action`：`None` = 整个映射；`Some("up")` 等 = 映射内操作。
    pub fn allowed(&self, mapping_id: &str, action: Option<&str>) -> bool {
        allowed(&self.rules, mapping_id, action, self.state.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_priority_is_fixed() {
        assert_eq!(current_state(true, true, true, true), CONDITION_RAW_INPUT);
        assert_eq!(current_state(false, true, true, true), CONDITION_FPS);
        assert_eq!(current_state(false, false, true, true), CONDITION_OBSERVATION);
        assert_eq!(
            current_state(false, false, false, true),
            CONDITION_FREE_LOOK_ACTIVE
        );
        assert_eq!(current_state(false, false, false, false), CONDITION_NORMAL);
    }

    #[test]
    fn allowed_delegates_to_compiled_rules() {
        let rule = AccessRule {
            mapping_id: "Fire-0".to_string(),
            action: None,
            predicate: crate::access_rules::Predicate::Is,
            condition: CONDITION_FREE_LOOK_ACTIVE.to_string(),
            operation: crate::access_rules::Operation {
                kind: crate::access_rules::OperationKind::Disable,
                target: None,
            },
            enabled: true,
        };
        let rules = AccessRulesResource {
            source: vec![rule.clone()],
            compiled: CompiledAccessRules::compile(&[rule]),
        };
        assert!(!allowed(&rules, "Fire-0", None, CONDITION_FREE_LOOK_ACTIVE));
        assert!(allowed(&rules, "Fire-0", None, CONDITION_NORMAL));
        assert!(allowed(&rules, "Other", None, CONDITION_FREE_LOOK_ACTIVE));
    }
}
