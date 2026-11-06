//! 视角位移工具（`ViewMotion`）：把"原始输入位移"换算成"实际发给设备的位移"。
//!
//! 三处消费点（Observation 按住视角、Fps 视角锁定、Fire 跟随位移）原本各自写一遍
//! `effects.apply(delta) * sensitivity + 限幅`，现统一到这里（`docs/21` P2 / S2）。
//!
//! 换算顺序固定，**不要改**：
//! 1. 输入效果（翻转 / 增益，`InputEffects::apply`）
//! 2. 乘灵敏度（每映射自己的 `sensitivity_x/y`）
//! 3. 最大半径限幅（仅 Observation 用，0 表示不限）

use bevy::math::Vec2;

use crate::input_effects::InputEffects;

/// 视角位移换算器。轻量 `Copy` 结构，每帧构造无堆分配。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewMotion {
    pub effects: InputEffects,
    pub sensitivity: Vec2,
    /// 单次位移的最大长度（mask 像素），0 表示不限幅。
    pub max_radius: f32,
}

impl ViewMotion {
    pub fn new(effects: InputEffects, sensitivity: Vec2, max_radius: f32) -> Self {
        Self {
            effects,
            sensitivity,
            max_radius,
        }
    }

    /// 只有灵敏度、没有输入效果与限幅（Fire 跟随位移用，等价旧行为）。
    pub fn from_sensitivity(sensitivity: Vec2) -> Self {
        Self::new(InputEffects::default(), sensitivity, 0.0)
    }

    /// 原始位移 → 实际位移。
    pub fn delta(&self, raw: Vec2) -> Vec2 {
        let mut delta = self.effects.apply(raw) * self.sensitivity;
        if self.max_radius > 0.0 {
            let len = delta.length();
            if len > self.max_radius {
                delta = delta / len * self.max_radius;
            }
        }
        delta
    }

    /// 从起点到当前光标位置的位移（按住的映射用）。
    pub fn delta_since(&self, start: Vec2, cursor: Vec2) -> Vec2 {
        self.delta(cursor - start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_input_through() {
        let motion = ViewMotion::new(InputEffects::default(), Vec2::ONE, 0.0);
        assert_eq!(motion.delta(Vec2::new(3.0, -4.0)), Vec2::new(3.0, -4.0));
        assert_eq!(
            motion.delta_since(Vec2::new(10.0, 10.0), Vec2::new(13.0, 6.0)),
            Vec2::new(3.0, -4.0)
        );
    }

    #[test]
    fn invert_applies_before_sensitivity() {
        let effects = InputEffects {
            invert_x: true,
            ..Default::default()
        };
        let motion = ViewMotion::new(effects, Vec2::new(2.0, 0.5), 0.0);
        // 先翻转 (-3, -4)，再乘灵敏度（x2, y0.5）
        assert_eq!(motion.delta(Vec2::new(3.0, -4.0)), Vec2::new(-6.0, -2.0));
    }

    #[test]
    fn max_radius_clamps_length() {
        let motion = ViewMotion::new(InputEffects::default(), Vec2::ONE, 5.0);
        let out = motion.delta(Vec2::new(10.0, 0.0));
        assert!((out.length() - 5.0).abs() < 1e-4);
        assert!(out.x > 0.0);
    }

    #[test]
    fn from_sensitivity_is_identity_effects() {
        let motion = ViewMotion::from_sensitivity(Vec2::new(0.5, 0.5));
        assert_eq!(motion.delta(Vec2::new(4.0, 8.0)), Vec2::new(2.0, 4.0));
    }
}
