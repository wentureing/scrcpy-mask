//! 输入效果（`InputEffects`）：对"输入轴"做通用后处理。
//!
//! 挂在映射配置的公共 `effects` 字段上（`#[serde(default)]`，缺省即完全不改变行为），
//! 是**映射层能力**而不是某个映射的特例，所以任何按钮都能复用：
//!
//! - 位移类（Observation / Fps / free_look）：作用在每帧鼠标位移上，
//!   `invert_x` 让"鼠标右移"变成"视角左转"；
//! - 方向类（DirectionPad / 各类施法摇杆）：作用在方向向量上，
//!   `invert_y` 让"W 键"变成"摇杆向下"。
//!
//! `scale_x/scale_y` 预留为"每按钮增益"，目前只有 free_look 用它承载灵敏度，
//! 其它映射暂时保持各自的 `sensitivity` 字段不变（避免一次改动影响所有手感）。

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

fn default_scale() -> f32 {
    1.0
}

/// 输入轴效果：按轴缩放 + 按轴翻转。
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct InputEffects {
    /// 反转 X 轴（左右互换）。
    #[serde(default)]
    pub invert_x: bool,
    /// 反转 Y 轴（上下互换）。
    #[serde(default)]
    pub invert_y: bool,
    /// X 轴增益。
    #[serde(default = "default_scale")]
    pub scale_x: f32,
    /// Y 轴增益。
    #[serde(default = "default_scale")]
    pub scale_y: f32,
}

impl Default for InputEffects {
    fn default() -> Self {
        Self {
            invert_x: false,
            invert_y: false,
            scale_x: default_scale(),
            scale_y: default_scale(),
        }
    }
}

impl InputEffects {
    /// 是否等价于"什么都不做"（用于跳过不必要的计算）。
    pub fn is_identity(&self) -> bool {
        !self.invert_x && !self.invert_y && self.scale_x == 1.0 && self.scale_y == 1.0
    }

    /// 输入轴换算：先按轴缩放，再按轴翻转。
    pub fn apply(&self, input: Vec2) -> Vec2 {
        if self.is_identity() {
            return input;
        }
        let mut out = Vec2::new(input.x * self.scale_x, input.y * self.scale_y);
        if self.invert_x {
            out.x = -out.x;
        }
        if self.invert_y {
            out.y = -out.y;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_identity() {
        let effects = InputEffects::default();
        assert!(effects.is_identity());
        assert_eq!(effects.apply(Vec2::new(3.0, -4.0)), Vec2::new(3.0, -4.0));
    }

    #[test]
    fn inverts_each_axis_independently() {
        let only_x = InputEffects {
            invert_x: true,
            ..Default::default()
        };
        assert_eq!(only_x.apply(Vec2::new(3.0, -4.0)), Vec2::new(-3.0, -4.0));

        let only_y = InputEffects {
            invert_y: true,
            ..Default::default()
        };
        assert_eq!(only_y.apply(Vec2::new(3.0, -4.0)), Vec2::new(3.0, 4.0));

        let both = InputEffects {
            invert_x: true,
            invert_y: true,
            ..Default::default()
        };
        assert_eq!(both.apply(Vec2::new(3.0, -4.0)), Vec2::new(-3.0, 4.0));
    }

    #[test]
    fn scale_and_invert_compose() {
        let effects = InputEffects {
            invert_x: true,
            invert_y: false,
            scale_x: 2.0,
            scale_y: 0.5,
        };
        assert_eq!(effects.apply(Vec2::new(3.0, -4.0)), Vec2::new(-6.0, -2.0));
    }

    #[test]
    fn old_config_without_effects_deserializes_to_identity() {
        let effects: InputEffects = serde_json::from_str("{}").unwrap();
        assert!(effects.is_identity());
    }
}
