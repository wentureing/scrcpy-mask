//! 指令发送器层（层④）：把"意图"翻译成 scrcpy 控制消息。
//!
//! 分层约定（见 docs/12）：
//! - 层③ 监听器只负责产出 [`Intent`]（按下了哪里、移动到哪里）；
//! - 层② 每按钮配置（`behavior`）决定坐标后处理与轨迹策略；
//! - 层④（本模块）是**唯一**把坐标交给 `ScrcpyControlMsg` 的地方，
//!   便于以后统一加随机化、缓动、插值，而不必逐个映射去改。
//!
//! 本次迁移是渐进的：摇杆（direction_pad）已接入；其余映射仍在直接调用
//! `ControlMsgHelper::send_touch`，行为完全一致，后续逐个替换即可。

use bevy::math::Vec2;
use tokio::sync::broadcast;

use crate::{
    scrcpy::{constant::MotionEventAction, control_msg::ScrcpyControlMsg},
    mask::mapping::utils::ControlMsgHelper,
};

/// 层③ → 层④ 的输入意图。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Intent {
    /// 按下触点
    Down { pointer_id: u64, pos: Vec2 },
    /// 移动触点
    Move { pointer_id: u64, pos: Vec2 },
    /// 抬起触点
    Up { pointer_id: u64, pos: Vec2 },
}

impl Intent {
    pub fn pointer_id(&self) -> u64 {
        match self {
            Intent::Down { pointer_id, .. }
            | Intent::Move { pointer_id, .. }
            | Intent::Up { pointer_id, .. } => *pointer_id,
        }
    }

    fn split(self) -> (MotionEventAction, u64, Vec2) {
        match self {
            Intent::Down { pointer_id, pos } => (MotionEventAction::Down, pointer_id, pos),
            Intent::Move { pointer_id, pos } => (MotionEventAction::Move, pointer_id, pos),
            Intent::Up { pointer_id, pos } => (MotionEventAction::Up, pointer_id, pos),
        }
    }
}

/// 指令发送器。轻量句柄（内部是 `broadcast::Sender` 的 clone），可每帧构造。
#[derive(Clone)]
pub struct IntentSender {
    cs_tx: broadcast::Sender<ScrcpyControlMsg>,
    original_size: Vec2,
}

impl IntentSender {
    pub fn new(cs_tx: broadcast::Sender<ScrcpyControlMsg>, original_size: Vec2) -> Self {
        Self {
            cs_tx,
            original_size,
        }
    }

    /// 发送一条意图。坐标已是 `original_size` 坐标系（与映射文件一致）。
    pub fn send(&self, intent: Intent) {
        let (action, pointer_id, pos) = intent.split();
        ControlMsgHelper::send_touch(
            &self.cs_tx,
            action,
            pointer_id,
            self.original_size,
            pos,
        );
    }
}
