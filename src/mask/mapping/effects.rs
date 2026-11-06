//! 兼容垫片：`InputEffects` 已迁到 crate 顶层 [`crate::input_effects`]。
//!
//! 迁移原因见 `docs/21` P1 / `docs/22` §22.19 A3：`config.rs` 需要引用它，
//! 若仍放在 `mask::mapping` 会形成"配置层 → 业务层"的反向依赖。
//! 这里保留 `mask::mapping::effects::InputEffects` 路径，其余调用点可渐进迁移。

pub use crate::input_effects::*;
