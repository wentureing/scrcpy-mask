//! 手势脉冲工具（`TouchPulse`）：把"一次滑动"做成可分帧推进的状态机。
//!
//! 一次脉冲 = `Down(anchor)` → `Move(anchor+delta)` → `Up(...)`，每帧推进一步。
//! 之所以分帧、而不是同一帧连发三条：同帧的 Down/Move/Up 容易被设备端当成 tap 丢掉。
//!
//! 原本这段状态机写在 `free_look.rs` 里，现抽成独立工具（`docs/21` P3 / S3），
//! 任何"需要脉冲式滑动"的映射都可以复用它：
//! - `step_px`：累计位移达到多少才发一次脉冲（避免每帧一个触点事件）；
//! - `max_radius_ratio`：单次脉冲最大位移 = `mask 短边 × 该比例`，防止触点被甩出屏幕；
//! - `pointer_id`：由调用方决定，避免与其它映射的触点冲突。

use bevy::math::Vec2;

use crate::{
    mask::mapping::{
        pipeline::{Intent, IntentSender},
        utils::ControlMsgHelper,
    },
    scrcpy::constant::MotionEventAction,
    utils::ChannelSenderCS,
};

/// 一次脉冲的三个阶段，每帧最多推进一步。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum PulsePhase {
    #[default]
    Idle,
    /// 已发 Down，等下一帧发 Move。
    Pressed { end: Vec2 },
    /// 已发 Move，等下一帧发 Up。
    Released { end: Vec2 },
}

/// 手感参数（由调用方固定或配置）。
#[derive(Debug, Clone, Copy)]
pub struct TouchPulseConfig {
    pub pointer_id: u64,
    pub step_px: f32,
    pub max_radius_ratio: f32,
}

/// 脉冲状态机。
#[derive(Debug, Clone)]
pub struct TouchPulse {
    config: TouchPulseConfig,
    pending: Vec2,
    phase: PulsePhase,
}

impl TouchPulse {
    pub fn new(config: TouchPulseConfig) -> Self {
        Self {
            config,
            pending: Vec2::ZERO,
            phase: PulsePhase::Idle,
        }
    }

    /// 累计位移（已做过输入效果换算）。
    pub fn accumulate(&mut self, delta: Vec2) {
        self.pending += delta;
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.phase, PulsePhase::Idle)
    }

    pub fn has_pending(&self) -> bool {
        self.pending != Vec2::ZERO
    }

    /// 每帧推进一次。`anchor` 是每次脉冲的起点（通常是屏幕中心）。
    pub fn tick(&mut self, cs_tx: &ChannelSenderCS, size: Vec2, anchor: Vec2) {
        match self.phase {
            PulsePhase::Idle => {}
            PulsePhase::Pressed { end } => {
                IntentSender::new(cs_tx.0.clone(), size)
            .send(Intent::Move {
                pointer_id: self.config.pointer_id,
                pos: end,
            });
                self.phase = PulsePhase::Released { end };
                return;
            }
            PulsePhase::Released { end } => {
                IntentSender::new(cs_tx.0.clone(), size)
            .send(Intent::Up {
                pointer_id: self.config.pointer_id,
                pos: end,
            });
                self.phase = PulsePhase::Idle;
                return;
            }
        }

        if self.pending.length() < self.config.step_px {
            return;
        }

        let mut delta = self.pending;
        self.pending = Vec2::ZERO;
        let max_radius = size.min_element() * self.config.max_radius_ratio;
        if max_radius > 0.0 {
            let len = delta.length();
            if len > max_radius {
                delta = delta / len * max_radius;
            }
        }

        let end = anchor + delta;
        IntentSender::new(cs_tx.0.clone(), size)
            .send(Intent::Down {
                pointer_id: self.config.pointer_id,
                pos: anchor,
            });
        self.phase = PulsePhase::Pressed { end };
    }

    /// 强制收尾：若正按下则补一个 Up，并清空累计（开关关闭、失焦、停止映射时调用）。
    pub fn release(&mut self, cs_tx: &ChannelSenderCS, size: Vec2) {
        let pos = match self.phase {
            PulsePhase::Idle => None,
            PulsePhase::Pressed { end } | PulsePhase::Released { end } => Some(end),
        };
        if let Some(pos) = pos {
            IntentSender::new(cs_tx.0.clone(), size)
            .send(Intent::Up {
                pointer_id: self.config.pointer_id,
                pos: pos,
            });
        }
        self.phase = PulsePhase::Idle;
        self.pending = Vec2::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scrcpy::control_msg::ScrcpyControlMsg;

    fn pulse() -> TouchPulse {
        TouchPulse::new(TouchPulseConfig {
            pointer_id: 15,
            step_px: 8.0,
            max_radius_ratio: 0.28,
        })
    }

    fn drain(rx: &mut tokio::sync::broadcast::Receiver<ScrcpyControlMsg>) -> Vec<(MotionEventAction, i32, i32)> {
        let mut events = Vec::new();
        while let Ok(ScrcpyControlMsg::InjectTouchEvent { action, x, y, .. }) = rx.try_recv() {
            events.push((action, x, y));
        }
        events
    }

    #[test]
    fn idle_release_sends_nothing() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let mut p = pulse();
        p.release(&ChannelSenderCS(tx), Vec2::new(100.0, 100.0));
        assert!(p.is_idle());
        assert!(!p.has_pending());
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn pressed_release_sends_up() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let mut p = pulse();
        p.accumulate(Vec2::new(10.0, 0.0));
        p.tick(&ChannelSenderCS(tx.clone()), Vec2::new(100.0, 100.0), Vec2::new(50.0, 50.0));
        p.release(&ChannelSenderCS(tx), Vec2::new(100.0, 100.0));

        let events = drain(&mut rx);
        assert_eq!(events.len(), 2, "expected Down + Up, got {events:?}");
        assert_eq!(events[0].0, MotionEventAction::Down);
        assert_eq!(events[1].0, MotionEventAction::Up);
    }

    #[test]
    fn below_step_does_not_fire() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let mut p = pulse();
        p.accumulate(Vec2::new(3.0, 0.0));
        p.tick(&ChannelSenderCS(tx), Vec2::new(100.0, 100.0), Vec2::new(50.0, 50.0));
        assert!(drain(&mut rx).is_empty());
        assert!(p.has_pending());
    }

    #[test]
    fn three_ticks_complete_one_pulse() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let mut p = pulse();
        let size = Vec2::new(100.0, 100.0);
        let anchor = Vec2::new(50.0, 50.0);

        p.accumulate(Vec2::new(10.0, 0.0));
        p.tick(&ChannelSenderCS(tx.clone()), size, anchor);
        p.tick(&ChannelSenderCS(tx.clone()), size, anchor);
        p.tick(&ChannelSenderCS(tx.clone()), size, anchor);

        let events = drain(&mut rx);
        let actions: Vec<_> = events.iter().map(|e| e.0).collect();
        assert_eq!(
            actions,
            vec![
                MotionEventAction::Down,
                MotionEventAction::Move,
                MotionEventAction::Up
            ]
        );
        assert!(p.is_idle());
    }

    #[test]
    fn clamps_single_pulse_to_max_radius() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);
        let mut p = pulse();
        let size = Vec2::new(100.0, 100.0);
        let anchor = Vec2::new(50.0, 50.0);

        // 短边 100 × 0.28 = 28 上限
        p.accumulate(Vec2::new(1000.0, 0.0));
        p.tick(&ChannelSenderCS(tx.clone()), size, anchor);
        p.tick(&ChannelSenderCS(tx), size, anchor);

        let events = drain(&mut rx);
        assert_eq!(events[0].0, MotionEventAction::Down);
        assert_eq!(events[1].0, MotionEventAction::Move);
        let (x, _y) = (events[1].1, events[1].2);
        assert!(x <= 50 + 29 && x >= 50 + 27, "move x should be clamped, got {x}");
    }
}
