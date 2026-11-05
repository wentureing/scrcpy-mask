//! 每按钮专属配置层（层②）：描述"这只按钮该怎么动"。
//!
//! 目标（见 docs/12）：把散落在各 `BindMapping*` 里的灵敏度、随机化、脚本、
//! 初始缓动等，逐步收敛成统一的 `ButtonBehavior`，并且**缺省即旧行为**。
//!
//! 当前状态：只定义数据结构与默认值，尚未接入执行路径（`#[allow(dead_code)]`）。
//! 接入顺序：S3 先接 `motion.sequence`，S4 再接 `humanize`。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// 缓动类型（轨迹插值用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EaseKind {
    /// 线性
    #[default]
    Linear,
    /// 加速（起步慢、末端快）
    Accelerate,
    /// 减速（起步快、末端慢）
    Decelerate,
    /// 先加速后减速
    EaseInOut,
}

/// 轨迹模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MotionMode {
    /// 一步到位（最快，当前摇杆方向切换用的就是这个语义）
    #[default]
    Direct,
    /// 单段缓动
    Ease,
    /// 多段序列（加速 → 匀速 → 减速）
    Sequence,
}

/// 序列的一段：从 `ms` 起、占整体位移的 `weight` 比例，以 `ease` 方式推进
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceStep {
    pub ms: u64,
    pub weight: f32,
    #[serde(default)]
    pub ease: EaseKind,
}

/// 轨迹配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionProfile {
    pub mode: MotionMode,
    /// 所有方向键松开后的抬指宽限期（ms）。0 = 立即抬指（等价于旧行为）
    pub release_grace_ms: u64,
    /// 初始按下时的缓动时长（ms），对应现有 `initial_duration`
    pub initial_duration_ms: u64,
    /// `mode = sequence` 时的分段
    pub sequence: Vec<SequenceStep>,
}

impl Default for MotionProfile {
    fn default() -> Self {
        Self {
            mode: MotionMode::Direct,
            release_grace_ms: 40,
            initial_duration_ms: 0,
            sequence: Vec::new(),
        }
    }
}

/// 人类化配置：位置/时序抖动与缓慢漂移
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HumanizeConfig {
    pub enabled: bool,
    /// 落点随机偏移半径（设备像素）
    pub position_jitter_px: f32,
    /// 时序抖动（ms）
    pub time_jitter_ms: u64,
    /// 长按期间缓慢漂移速度（设备像素/秒）
    pub drift_px_per_s: f32,
}

impl Default for HumanizeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position_jitter_px: 0.0,
            time_jitter_ms: 0,
            drift_px_per_s: 0.0,
        }
    }
}

/// 层②的总入口：一个按钮的全部行为配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ButtonBehavior {
    pub motion: MotionProfile,
    pub humanize: HumanizeConfig,
}
