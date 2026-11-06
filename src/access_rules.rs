//! 访问规则：在某种状态下，允许/不允许某个映射触发（`docs/26`）。
//!
//! 设计要点（用户定稿）：
//! - 目标**只用映射 id**，不做"类型/输入键"目标；
//! - 状态是**单一字符串**（不是集合/bitmask），按优先级由调用方给出；
//! - **默认允许**：没有规则命中就放行，规则表是"例外表"；
//! - 同一映射同一状态多条命中时**拒绝优先**（避免误放行）；
//! - 未知状态在编译时忽略并告警（向前兼容，不阻塞启动）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 状态字符串常量（配置里写这些值）。
pub const STATE_RAW_INPUT: &str = "raw_input";
pub const STATE_FPS: &str = "fps";
pub const STATE_OBSERVATION: &str = "observation";
pub const STATE_FREE_LOOK_ACTIVE: &str = "free_look_active";
pub const STATE_NORMAL: &str = "normal";

fn default_true() -> bool {
    true
}

/// 一条访问规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRule {
    /// 目标映射的 id（映射文件里的 `id`）。
    pub mapping_id: String,
    /// 单一状态字符串，见本模块常量。
    pub when: String,
    /// `true` 允许，`false` 不允许；缺省为允许。
    #[serde(default = "default_true")]
    pub allow: bool,
}

/// 把配置里的状态字符串归一化。
///
/// `free_look_inactive` 是 `normal` 的别名（用户用它表达"未激活"）。
pub fn normalize_state(state: &str) -> Option<&'static str> {
    match state {
        STATE_RAW_INPUT => Some(STATE_RAW_INPUT),
        STATE_FPS => Some(STATE_FPS),
        STATE_OBSERVATION => Some(STATE_OBSERVATION),
        STATE_FREE_LOOK_ACTIVE => Some(STATE_FREE_LOOK_ACTIVE),
        "normal" | "free_look_inactive" => Some(STATE_NORMAL),
        _ => None,
    }
}

/// 预编译后的规则表：`mapping_id -> state -> allow`。
#[derive(Debug, Default)]
pub struct CompiledAccessRules {
    rules: HashMap<String, HashMap<&'static str, bool>>,
}

impl CompiledAccessRules {
    /// 编译规则：忽略未知状态；同一映射同一状态多条命中时拒绝优先。
    pub fn compile(rules: &[AccessRule]) -> Self {
        let mut out: HashMap<String, HashMap<&'static str, bool>> = HashMap::new();
        for rule in rules {
            let Some(state) = normalize_state(&rule.when) else {
                log::warn!(
                    "忽略未知的访问规则状态 `{}`（映射 {}），可选值见 docs/26",
                    rule.when,
                    rule.mapping_id
                );
                continue;
            };
            let entry = out.entry(rule.mapping_id.clone()).or_default();
            let slot = entry.entry(state).or_insert(true);
            // 与运算即"拒绝优先"：任一 allow=false 就保持 false。
            *slot = *slot && rule.allow;
        }
        Self { rules: out }
    }

    /// 判定某映射在某状态下是否允许触发。默认允许。
    pub fn allowed(&self, mapping_id: &str, state: &str) -> bool {
        let Some(state) = normalize_state(state) else {
            // 调用方传入未知状态时按"默认允许"处理，不 panic。
            return true;
        };
        self.rules
            .get(mapping_id)
            .and_then(|states| states.get(state))
            .copied()
            .unwrap_or(true)
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
            when: when.to_string(),
            allow,
        }
    }

    #[test]
    fn empty_rules_allow_everything() {
        let rules = CompiledAccessRules::compile(&[]);
        assert!(rules.is_empty());
        assert!(rules.allowed("Fire-0", STATE_FREE_LOOK_ACTIVE));
        assert!(rules.allowed("Fire-0", STATE_NORMAL));
    }

    #[test]
    fn deny_rule_blocks_only_its_mapping_and_state() {
        let rules = CompiledAccessRules::compile(&[rule(
            "Fire-0",
            STATE_FREE_LOOK_ACTIVE,
            false,
        )]);
        assert!(!rules.allowed("Fire-0", STATE_FREE_LOOK_ACTIVE));
        // 其它状态/其它映射不受影响
        assert!(rules.allowed("Fire-0", STATE_NORMAL));
        assert!(rules.allowed("Fire-1", STATE_FREE_LOOK_ACTIVE));
    }

    #[test]
    fn deny_wins_over_allow_for_same_mapping_and_state() {
        let rules = CompiledAccessRules::compile(&[
            rule("Fire-0", STATE_NORMAL, true),
            rule("Fire-0", STATE_NORMAL, false),
        ]);
        assert!(!rules.allowed("Fire-0", STATE_NORMAL));
    }

    #[test]
    fn free_look_inactive_is_alias_of_normal() {
        let rules = CompiledAccessRules::compile(&[rule(
            "Fire-0",
            STATE_NORMAL,
            false,
        )]);
        assert!(!rules.allowed("Fire-0", "free_look_inactive"));
    }

    #[test]
    fn unknown_state_is_ignored_and_allows() {
        let rules = CompiledAccessRules::compile(&[rule("Fire-0", "some_future_state", false)]);
        // 未知状态被忽略 → 规则表为空 → 默认允许
        assert!(rules.is_empty());
        assert!(rules.allowed("Fire-0", STATE_NORMAL));
    }

    #[test]
    fn allowed_defaults_to_true_for_unknown_mapping() {
        let rules = CompiledAccessRules::compile(&[rule("Fire-0", STATE_NORMAL, false)]);
        assert!(rules.allowed("Unknown-1", STATE_NORMAL));
    }
}
