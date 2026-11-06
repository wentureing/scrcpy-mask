//! 人类手势路径模仿器（`HumanGestureImitator`）：描述"一次操作该怎么像人手一样动"。
//!
//! 定位（`docs/22` §22.14）：映射的**可选配置/工具**，不是必用核心层。
//! 启用后，一般点击/按键改写成 `<起点, 终点>` 描述，由模仿器生成：
//! - **过渡段**：起点 → 终点的插值（时长可调）；
//! - **稳态**：到达终点后的随机抖动；
//! - **结束段**：可选"过渡回中性"（默认关闭，强化跟手）。
//!
//! 例：按下 W → 虚拟摇杆从中心过渡到前进位（过渡段），到位后轻微抖动（稳态）。
//! 瞬时操作（单击）不需要 `<起点,终点>`，只做抖动。
//!
//! 当前状态：**只定义了数据结构与默认值，尚未接入执行路径**（`#![allow(dead_code)]`）。
//! 接线计划见 `docs/21` S8b（`P8`）；与 `pipeline.rs`（意图发送器）的分工见 `docs/22` §22.19.9：
//! **本模块决定"怎么动"，`pipeline` 决定"怎么发"**。
//!
//! 说明：原文件名为 `behavior.rs`、类型为 `ButtonBehavior`，按用户定义改名
//! （`docs/21` P5 / `docs/22` §22.19 D22），纯改名，不动逻辑。

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

/// 轨迹配置：过渡段（起点 → 终点）怎么走
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionProfile {
    pub mode: MotionMode,
    /// 所有方向键松开后的抬指宽限期（ms）。0 = 立即抬指（等价于旧行为）。
    ///
    /// 注意：`direction_pad.rs` 目前用的是同值常量 `RELEASE_GRACE_MS = 40`，
    /// 接线时必须确认默认值契约（`docs/21` §21.6.2）。
    pub release_grace_ms: u64,
    /// 过渡段时长（ms），对应现有 `initial_duration`；用户可改、有默认值。
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

/// 人类化配置：位置/时序抖动与缓慢漂移（稳态用）
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

/// 一个按钮的全部"像人手"配置。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HumanGestureImitator {
    pub motion: MotionProfile,
    pub humanize: HumanizeConfig,
}
