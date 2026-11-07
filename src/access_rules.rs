//! 访问规则（docs/29、docs/31）：目标（映射 + 映射内操作）+ 条件（谓词 + 条件）+ 操作。
//!
//! 语义（用户定稿）：
//! - 条件 = **谓词（肯定/否定）+ 中性条件**；
//! - 操作 = **禁用/启用**（状态型，过滤器）或 **打开/关闭**（一次型）；
//! - **禁用优先**：状态型“禁用”压过一次型动作；
//! - 每条规则最左有独立开关（`enabled`），关闭的规则不参与判定；
//! - 默认可用：没有规则命中就放行；
//! - 条件/操作的冲突由用户负责（保存时警告，属未定义行为）。

use serde::{Deserialize, Serialize};

/// 运行条件：自由视角处于激活。
pub const CONDITION_FREE_LOOK_ACTIVE: &str = "free_look_active";
/// 运行条件：处于 FPS 视角。
pub const CONDITION_FPS: &str = "fps";
/// 运行条件：观察键被按住。
pub const CONDITION_OBSERVATION: &str = "observation";
/// 运行条件：处于鼠标直控。
pub const CONDITION_RAW_INPUT: &str = "raw_input";
/// 运行条件：其它情况（兜底）。
pub const CONDITION_NORMAL: &str = "normal";

/// 谓词：肯定 / 否定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    Is,
    IsNot,
}

fn default_predicate() -> Predicate {
    Predicate::Is
}

fn default_enabled() -> bool {
    true
}

/// 操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    /// 状态型：禁用（过滤器，优先于一次型）。
    Disable,
    /// 状态型：启用。
    Enable,
    /// 一次型：打开（设为可用）。
    Open,
    /// 一次型：关闭（设为不可用）。
    Close,
}

/// 打开/关闭的目标：有模式的对象或指定映射。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationTarget {
    /// `mode` = 有模式的对象（如自由视角）；`mapping` = 指定映射。
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mapping_id: Option<String>,
}

/// 操作：类型 + 可选目标（打开/关闭需要目标；禁用/启用作用于自身）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub kind: OperationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<OperationTarget>,
}

/// 一条访问规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRule {
    /// 规则目标映射（禁用/启用对象；也是打开/关闭的默认目标）。
    pub mapping_id: String,
    /// 映射内操作（如 DirectionPad 的 `up`）；`None` = 整个映射。
    #[serde(default)]
    pub action: Option<String>,
    /// 谓词：肯定 / 否定。
    #[serde(default = "default_predicate")]
    pub predicate: Predicate,
    /// 条件（中性）。
    pub condition: String,
    /// 操作。
    pub operation: Operation,
    /// 规则自身是否生效（最左开关）。
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// 把条件字符串归一化。
pub fn normalize_condition(condition: &str) -> Option<&'static str> {
    match condition {
        CONDITION_FREE_LOOK_ACTIVE => Some(CONDITION_FREE_LOOK_ACTIVE),
        CONDITION_FPS => Some(CONDITION_FPS),
        CONDITION_OBSERVATION => Some(CONDITION_OBSERVATION),
        CONDITION_RAW_INPUT => Some(CONDITION_RAW_INPUT),
        CONDITION_NORMAL => Some(CONDITION_NORMAL),
        _ => None,
    }
}

/// 谓词是否满足当前状态。
pub fn predicate_matches(predicate: Predicate, condition: &str, state: &str) -> bool {
    let met = condition == state;
    match predicate {
        Predicate::Is => met,
        Predicate::IsNot => !met,
    }
}

/// 编译后的规则：只保留可解析的条件。
#[derive(Debug, Default)]
pub struct CompiledAccessRules {
    rules: Vec<AccessRule>,
}

impl CompiledAccessRules {
    pub fn compile(rules: &[AccessRule]) -> Self {
        let mut kept = Vec::new();
        for rule in rules {
            if normalize_condition(&rule.condition).is_none() {
                log::warn!(
                    "忽略未知的访问规则条件 `{}`（映射 {}），可选值见 docs/31",
                    rule.condition,
                    rule.mapping_id
                );
                continue;
            }
            kept.push(rule.clone());
        }
        Self { rules: kept }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// 判定某映射（或映射内操作）在当前条件下是否允许。
    ///
    /// 语义：禁用优先。返回 `false` 表示被过滤器拦截。
    pub fn allowed(&self, mapping_id: &str, action: Option<&str>, state: &str) -> bool {
        let mut disabled = false;
        let mut enabled = false;
        let mut closed = false;
        let mut opened = false;

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            if !predicate_matches(rule.predicate, &rule.condition, state) {
                continue;
            }
            let hits_self = rule.mapping_id == mapping_id
                && (rule.action.is_none() || rule.action.as_deref() == action);
            if !hits_self {
                continue;
            }
            match rule.operation.kind {
                OperationKind::Disable => disabled = true,
                OperationKind::Enable => enabled = true,
                OperationKind::Close => closed = true,
                OperationKind::Open => opened = true,
            }
        }

        // 禁用优先：状态型禁用压过启用的动作；其余按“启用/打开 > 关闭”保守处理。
        if disabled {
            return false;
        }
        if enabled || opened {
            return true;
        }
        if closed {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(
        mapping_id: &str,
        predicate: Predicate,
        condition: &str,
        kind: OperationKind,
    ) -> AccessRule {
        AccessRule {
            mapping_id: mapping_id.to_string(),
            action: None,
            predicate,
            condition: condition.to_string(),
            operation: Operation {
                kind,
                target: None,
            },
            enabled: true,
        }
    }

    #[test]
    fn empty_rules_allow() {
        let rules = CompiledAccessRules::compile(&[]);
        assert!(rules.is_empty());
        assert!(rules.allowed("a62a9ce3", None, CONDITION_NORMAL));
    }

    #[test]
    fn disable_filters_when_condition_matches() {
        let rules = CompiledAccessRules::compile(&[rule(
            "a62a9ce3",
            Predicate::IsNot,
            CONDITION_FREE_LOOK_ACTIVE,
            OperationKind::Disable,
        )]);
        // 自由视角未激活（normal）→ 否定谓词成立 → 禁用
        assert!(!rules.allowed("a62a9ce3", None, CONDITION_NORMAL));
        // 自由视角激活 → 否定不成立 → 不禁用
        assert!(rules.allowed("a62a9ce3", None, CONDITION_FREE_LOOK_ACTIVE));
    }

    #[test]
    fn disable_wins_over_open() {
        let rules = CompiledAccessRules::compile(&[
            rule(
                "a62a9ce3",
                Predicate::Is,
                CONDITION_NORMAL,
                OperationKind::Disable,
            ),
            rule(
                "a62a9ce3",
                Predicate::Is,
                CONDITION_NORMAL,
                OperationKind::Open,
            ),
        ]);
        assert!(!rules.allowed("a62a9ce3", None, CONDITION_NORMAL));
    }

    #[test]
    fn disabled_rule_is_skipped() {
        let mut r = rule(
            "a62a9ce3",
            Predicate::Is,
            CONDITION_NORMAL,
            OperationKind::Disable,
        );
        r.enabled = false;
        let rules = CompiledAccessRules::compile(&[r]);
        assert!(rules.allowed("a62a9ce3", None, CONDITION_NORMAL));
    }

    #[test]
    fn action_specific_rule_matches() {
        let mut r = rule(
            "ea1af84d",
            Predicate::Is,
            CONDITION_NORMAL,
            OperationKind::Disable,
        );
        r.action = Some("up".to_string());
        let rules = CompiledAccessRules::compile(&[r]);
        assert!(!rules.allowed("ea1af84d", Some("up"), CONDITION_NORMAL));
        assert!(rules.allowed("ea1af84d", Some("down"), CONDITION_NORMAL));
        assert!(rules.allowed("ea1af84d", None, CONDITION_NORMAL));
    }

    #[test]
    fn unknown_condition_is_ignored() {
        let rules = CompiledAccessRules::compile(&[rule(
            "a62a9ce3",
            Predicate::Is,
            "some_future_condition",
            OperationKind::Disable,
        )]);
        assert!(rules.is_empty());
        assert!(rules.allowed("a62a9ce3", None, CONDITION_NORMAL));
    }
}
