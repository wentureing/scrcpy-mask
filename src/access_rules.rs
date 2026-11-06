//! 访问规则：在某个条件下，允许/不允许某个映射（或映射内的某个操作）触发（`docs/26`、`docs/29`）。
//!
//! 设计要点（用户定稿）：
//! - 目标**只用映射 id**（可选 `action` 细化到映射内操作）；不做全局规则（同时只有一个映射文件生效）；
//! - 条件是**单一字符串**（不是集合/bitmask）；条件组合接口预留但暂不实现；
//! - **默认允许**：没有规则命中就放行，规则表是"例外表"；
//! - 同一目标同一条件多条命中时**拒绝优先**；
//! - 未知条件在编译时忽略并告警（向前兼容）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 运行状态：鼠标直控模式。
pub const STATE_RAW_INPUT: &str = "raw_input";
/// 运行状态：FPS 视角。
pub const STATE_FPS: &str = "fps";
/// 运行状态：按住观察。
pub const STATE_OBSERVATION: &str = "observation";
/// 运行状态：自由视角已激活。
pub const STATE_FREE_LOOK_ACTIVE: &str = "free_look_active";
/// 运行状态：自由视角未激活（与 `normal` 解耦）。
pub const STATE_FREE_LOOK_INACTIVE: &str = "free_look_inactive";
/// 其它情况（兜底）。
pub const STATE_NORMAL: &str = "normal";
/// 配置开关：自由视角总开关为开。
pub const STATE_FREE_LOOK_ENABLED_ON: &str = "free_look_enabled";
/// 配置开关：自由视角总开关为关。
pub const STATE_FREE_LOOK_ENABLED_OFF: &str = "free_look_disabled";

fn default_true() -> bool {
    true
}

/// 一条访问规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRule {
    /// 目标映射的 id（映射文件里的 `id`）。
    pub mapping_id: String,
    /// 可选：映射内的操作（如 DirectionPad 的 `up`、Script 的 `press`）；
    /// `None` 表示整个映射。
    #[serde(default)]
    pub action: Option<String>,
    /// 单一条件字符串，见本模块常量。
    pub when: String,
    /// `true` 允许，`false` 不允许；缺省为允许。
    #[serde(default = "default_true")]
    pub allow: bool,
}

/// 规则目标：映射 id + 可选操作。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TargetKey {
    mapping_id: String,
    action: Option<String>,
}

/// 把配置里的条件字符串归一化。
pub fn normalize_state(state: &str) -> Option<&'static str> {
    match state {
        STATE_RAW_INPUT => Some(STATE_RAW_INPUT),
        STATE_FPS => Some(STATE_FPS),
        STATE_OBSERVATION => Some(STATE_OBSERVATION),
        STATE_FREE_LOOK_ACTIVE => Some(STATE_FREE_LOOK_ACTIVE),
        STATE_FREE_LOOK_INACTIVE => Some(STATE_FREE_LOOK_INACTIVE),
        STATE_NORMAL => Some(STATE_NORMAL),
        STATE_FREE_LOOK_ENABLED_ON => Some(STATE_FREE_LOOK_ENABLED_ON),
        STATE_FREE_LOOK_ENABLED_OFF => Some(STATE_FREE_LOOK_ENABLED_OFF),
        _ => None,
    }
}

/// 预编译后的规则表：`(mapping_id, action) -> state -> allow`。
#[derive(Debug, Default)]
pub struct CompiledAccessRules {
    rules: HashMap<TargetKey, HashMap<&'static str, bool>>,
}

impl CompiledAccessRules {
    /// 编译规则：忽略未知条件；同一目标同一条件多条命中时拒绝优先。
    pub fn compile(rules: &[AccessRule]) -> Self {
        let mut out: HashMap<TargetKey, HashMap<&'static str, bool>> = HashMap::new();
        for rule in rules {
            let Some(state) = normalize_state(&rule.when) else {
                log::warn!(
                    "忽略未知的访问规则条件 `{}`（映射 {}），可选值见 docs/29",
                    rule.when,
                    rule.mapping_id
                );
                continue;
            };
            let key = TargetKey {
                mapping_id: rule.mapping_id.clone(),
                action: rule.action.clone().filter(|action| !action.is_empty()),
            };
            let entry = out.entry(key).or_default();
            let slot = entry.entry(state).or_insert(true);
            // 与运算即"拒绝优先"：任一 allow=false 就保持 false。
            *slot = *slot && rule.allow;
        }
        Self { rules: out }
    }

    /// 判定某映射（或其中某个操作）在当前条件下是否允许触发。默认允许。
    ///
    /// - `action`：`None` 表示整个映射；`Some("up")` 等表示映射内操作，具体优先于整映射；
    /// - `free_look_active` / `free_look_enabled`：分别对应运行时激活与配置总开关。
    pub fn allowed(
        &self,
        mapping_id: &str,
        action: Option<&str>,
        state: &str,
        free_look_active: bool,
        free_look_enabled: bool,
    ) -> bool {
        let Some(state) = normalize_state(state) else {
            // 调用方传入未知条件时按"默认允许"处理，不 panic。
            return true;
        };
        let mut allow = self.lookup(mapping_id, action, state);
        if !free_look_active {
            allow = allow && self.lookup(mapping_id, action, STATE_FREE_LOOK_INACTIVE);
        }
        let switch_state = if free_look_enabled {
            STATE_FREE_LOOK_ENABLED_ON
        } else {
            STATE_FREE_LOOK_ENABLED_OFF
        };
        allow && self.lookup(mapping_id, action, switch_state)
    }

    /// 具体优先：先查 `(mapping, action)`，没有再查 `(mapping, None)`。
    fn lookup(&self, mapping_id: &str, action: Option<&str>, state: &'static str) -> bool {
        if let Some(value) = self.rule(mapping_id, action, state) {
            return value;
        }
        self.rule(mapping_id, None, state).unwrap_or(true)
    }

    fn rule(
        &self,
        mapping_id: &str,
        action: Option<&str>,
        state: &'static str,
    ) -> Option<bool> {
        let key = TargetKey {
            mapping_id: mapping_id.to_string(),
            action: action.map(str::to_string),
        };
        self.rules
            .get(&key)
            .and_then(|states| states.get(state))
            .copied()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(mapping_id: &str, when: &str, allow: bool) -> AccessRule {
        AccessRule {
            mapping_id: mapping_id.to_string(),
            action: None,
            when: when.to_string(),
            allow,
        }
    }

    fn rule_action(mapping_id: &str, action: &str, when: &str, allow: bool) -> AccessRule {
        AccessRule {
            mapping_id: mapping_id.to_string(),
            action: Some(action.to_string()),
            when: when.to_string(),
            allow,
        }
    }

    #[test]
    fn empty_rules_allow_everything() {
        let rules = CompiledAccessRules::compile(&[]);
        assert!(rules.is_empty());
        assert!(rules.allowed("Fire-0", None, STATE_FREE_LOOK_ACTIVE, true, true));
        assert!(rules.allowed("Fire-0", None, STATE_NORMAL, false, true));
    }

    #[test]
    fn deny_rule_blocks_only_its_mapping_and_state() {
        let rules = CompiledAccessRules::compile(&[rule(
            "Fire-0",
            STATE_FREE_LOOK_ACTIVE,
            false,
        )]);
        assert!(!rules.allowed("Fire-0", None, STATE_FREE_LOOK_ACTIVE, true, true));
        assert!(rules.allowed("Fire-0", None, STATE_NORMAL, false, true));
        assert!(rules.allowed("Fire-1", None, STATE_FREE_LOOK_ACTIVE, true, true));
    }

    #[test]
    fn action_rule_takes_precedence_over_mapping_rule() {
        let rules = CompiledAccessRules::compile(&[
            rule("Pad-0", STATE_FREE_LOOK_INACTIVE, false),
            rule_action("Pad-0", "up", STATE_FREE_LOOK_INACTIVE, true),
        ]);
        // 整映射被禁用，但 up 操作被显式允许
        assert!(!rules.allowed("Pad-0", None, STATE_NORMAL, false, true));
        assert!(rules.allowed("Pad-0", Some("up"), STATE_NORMAL, false, true));
        // 未显式允许的 down 仍走整映射规则
        assert!(!rules.allowed("Pad-0", Some("down"), STATE_NORMAL, false, true));
    }

    #[test]
    fn free_look_inactive_is_independent_of_normal() {
        let rules = CompiledAccessRules::compile(&[rule(
            "Fire-0",
            STATE_FREE_LOOK_INACTIVE,
            false,
        )]);
        assert!(!rules.allowed("Fire-0", None, STATE_NORMAL, false, true));
        assert!(!rules.allowed("Fire-0", None, STATE_FPS, false, true));
        assert!(rules.allowed("Fire-0", None, STATE_FREE_LOOK_ACTIVE, true, true));

        let normal_only = CompiledAccessRules::compile(&[rule("Fire-0", STATE_NORMAL, false)]);
        assert!(!normal_only.allowed("Fire-0", None, STATE_NORMAL, false, true));
        assert!(normal_only.allowed("Fire-0", None, STATE_FPS, false, true));
    }

    #[test]
    fn free_look_enabled_condition_uses_config_switch() {
        let rules = CompiledAccessRules::compile(&[rule(
            "Fire-0",
            STATE_FREE_LOOK_ENABLED_OFF,
            false,
        )]);
        // 总开关关 → 命中
        assert!(!rules.allowed("Fire-0", None, STATE_NORMAL, false, false));
        // 总开关开 → 不命中
        assert!(rules.allowed("Fire-0", None, STATE_NORMAL, false, true));
    }

    #[test]
    fn unknown_state_is_ignored_and_allows() {
        let rules = CompiledAccessRules::compile(&[rule("Fire-0", "some_future_state", false)]);
        assert!(rules.is_empty());
        assert!(rules.allowed("Fire-0", None, STATE_NORMAL, false, true));
    }

    #[test]
    fn allowed_defaults_to_true_for_unknown_mapping() {
        let rules = CompiledAccessRules::compile(&[rule("Fire-0", STATE_NORMAL, false)]);
        assert!(rules.allowed("Unknown-1", None, STATE_NORMAL, false, true));
    }
}
