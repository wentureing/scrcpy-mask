use std::{
    fs::{File, create_dir_all},
    io::Write,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::RwLock,
};

use crate::{
    DEFAULT_LANGUAGE,
    access_rules::AccessRule,
    input_effects::InputEffects,
    scrcpy::media::{AudioCodec, AudioSource, VideoCodec},
    utils::{relate_to_data_path, relate_to_root_path},
};
use once_cell::sync::Lazy;
use paste::paste;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;

static CONFIG: Lazy<RwLock<LocalConfig>> = Lazy::new(|| RwLock::default());

pub const AUDIO_BIT_RATE_MIN: u32 = 16_000;

// Old configuration files retain SDK injection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchBackend {
    #[default]
    Sdk,
    Uhid,
}

/// 退出时是否清理 adb server（`docs/22` §22.19 D26/D27）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdbCleanupPolicy {
    /// 只清理"本程序启动的 adb"；外部 adb 不动（默认，最安全）
    #[default]
    Auto,
    /// 退出时一律 `kill-server`
    Always,
    /// 从不清理 adb server
    Never,
}

#[cfg(test)]
mod uhid_config_tests {
    use super::*;

    #[test]
    fn old_config_retains_sdk_backend() {
        let config: LocalConfig =
            serde_json::from_str(r#"{"active_mapping_file":"existing.json"}"#).unwrap();
        assert_eq!(config.touch_backend, TouchBackend::Sdk);
        assert_eq!(config.active_mapping_file, "existing.json");
    }

    #[test]
    fn uhid_setting_roundtrips_and_rejects_unknown_backend() {
        let config: LocalConfig = serde_json::from_str(r#"{"touch_backend":"uhid"}"#).unwrap();
        assert_eq!(config.touch_backend, TouchBackend::Uhid);
        assert_eq!(
            serde_json::to_value(config).unwrap()["touch_backend"],
            "uhid"
        );
        assert!(serde_json::from_str::<LocalConfig>(r#"{"touch_backend":"unknown"}"#).is_err());
    }

    #[test]
    fn migrates_v0_single_exit_key_to_array() {
        let mut value = serde_json::json!({ "free_look_exit_key": "KeyF" });
        assert!(migrate_config(&mut value, &Default::default()));

        assert_eq!(value["free_look_exit_keys"], serde_json::json!(["KeyF"]));
        assert_eq!(value["free_look_enter_keys"], serde_json::json!([]));
        assert_eq!(value["virtual_cursor_visible"], serde_json::json!(false));
        assert_eq!(value["config_version"], serde_json::json!(2));
        assert!(value.get("free_look_exit_key").is_none());
    }

    #[test]
    fn migration_falls_back_to_default_key_and_is_idempotent() {
        let mut value = serde_json::json!({});
        assert!(migrate_config(&mut value, &Default::default()));
        assert_eq!(
            value["free_look_exit_keys"],
            serde_json::json!(["ControlLeft"])
        );

        // 第二次迁移：版本已是 2，不再改动。
        let snapshot = value.clone();
        assert!(!migrate_config(&mut value, &Default::default()));
        assert_eq!(value, snapshot);
    }

    #[test]
    fn future_config_version_is_not_migrated() {
        let mut value = serde_json::json!({ "config_version": 99, "free_look_exit_key": "KeyF" });
        assert!(!migrate_config(&mut value, &Default::default()));
        // 不迁移、也不改版本号（避免降级时破坏用户配置）。
        assert_eq!(value["config_version"], serde_json::json!(99));
        assert_eq!(value["free_look_exit_key"], serde_json::json!("KeyF"));
        assert!(value.get("free_look_exit_keys").is_none());
    }

    #[test]
    fn migrates_blocked_mouse_buttons_to_access_rules() {
        let mut value = serde_json::json!({
            "free_look_blocked_mouse_buttons": ["M-Left"],
        });
        let mut binds = std::collections::HashMap::new();
        binds.insert("Fire-0".to_string(), vec!["M-Left".to_string()]);
        binds.insert("Tap-1".to_string(), vec!["KeyF".to_string()]);

        assert!(migrate_config(&mut value, &binds));

        let rules = value["access_rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["mapping_id"], "Fire-0");
        assert_eq!(rules[0]["when"], "free_look_active");
        assert_eq!(rules[0]["allow"], false);
        assert_eq!(value["config_version"], serde_json::json!(2));
    }

    #[test]
    fn keeps_blocked_field_when_mapping_file_unavailable() {
        let mut value = serde_json::json!({
            "free_look_blocked_mouse_buttons": ["M-Left"],
        });
        // 映射文件不可用 → 传空表
        assert!(migrate_config(&mut value, &Default::default()));

        // 旧字段保留，规则不生成，版本仍然升级（下次启动会再尝试读映射文件）。
        assert_eq!(
            value["free_look_blocked_mouse_buttons"],
            serde_json::json!(["M-Left"])
        );
        assert_eq!(value["access_rules"], serde_json::json!([]));
    }

    #[test]
    fn prune_access_rules_drops_unknown_mapping_ids() {
        let rules = vec![
            AccessRule {
                mapping_id: "a62a9ce3".to_string(),
                when: "free_look_inactive".to_string(),
                allow: false,
            },
            AccessRule {
                mapping_id: "9".to_string(),
                when: "normal".to_string(),
                allow: false,
            },
        ];
        let valid: std::collections::HashSet<String> =
            ["a62a9ce3".to_string()].into_iter().collect();
        let pruned = prune_access_rules(rules, &valid);
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].mapping_id, "a62a9ce3");
    }
}

fn default_web_bind_addr() -> Ipv4Addr {
    Ipv4Addr::new(127, 0, 0, 1)
}

fn default_free_look_exit_key() -> String {
    "ControlLeft".to_string()
}

/// 当前配置版本。结构发生不兼容变化时 +1，并在 [`migrate_config`] 里加对应迁移器。
pub const CURRENT_CONFIG_VERSION: u32 = 2;

fn default_config_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

fn default_free_look_exit_keys() -> Vec<String> {
    vec![default_free_look_exit_key()]
}

fn bundled_adb_path() -> PathBuf {
    let adb_name = if cfg!(target_os = "windows") {
        "adb.exe"
    } else {
        "adb"
    };
    relate_to_root_path(["assets", "platform-tools", adb_name])
}

fn default_adb_path() -> String {
    let path = bundled_adb_path();
    if path.is_file() {
        path.to_string_lossy().into_owned()
    } else {
        "adb".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalConfig {
    // port
    pub web_port: u16,
    #[serde(default = "default_web_bind_addr")]
    pub web_bind_addr: Ipv4Addr,
    pub controller_port: u16,
    // connect
    /// 等待设备端 scrcpy server 连回来的超时（秒）。
    /// 慢设备上 `adb push` + `app_process` 冷启动常要 2-4 秒，默认 10 秒留足余量。
    pub connect_timeout_secs: u64,
    // adb
    pub adb_path: String,
    pub adb_connect_address: String,
    /// 退出时是否清理 adb server（Auto = 只清理本程序启动的）
    pub adb_cleanup_policy: AdbCleanupPolicy,
    // mask
    pub always_on_top: bool,
    pub titlebar_visible: bool,
    pub vertical_mask_height: u32,
    pub horizontal_mask_width: u32,
    pub vertical_position: (i32, i32),
    pub horizontal_position: (i32, i32),
    // mapping
    pub active_mapping_file: String,
    pub mapping_label_opacity: f32,
    // language
    pub language: String,
    // clipboard sync
    pub clipboard_sync: bool,
    pub touch_backend: TouchBackend,
    // video config
    pub video_codec: VideoCodec,
    pub video_bit_rate: u32,
    pub video_max_size: u32,
    pub video_max_fps: u32,
    pub display_id: i32,
    pub new_display_enabled: bool,
    pub new_display_use_main_size: bool,
    pub new_display_width: u32,
    pub new_display_height: u32,
    pub new_display_dpi: u32,
    // audio config
    pub audio_codec: AudioCodec,
    pub audio_bit_rate: u32,
    pub audio_source: AudioSource,
    pub audio_dup: bool,
    // device behavior
    pub stay_awake: bool,
    pub screen_off_timeout: i32,
    pub power_off_on_close: bool,
    // mouse behavior
    /// 只把鼠标位移的横向分量传给设备（纵向位移丢弃）。
    /// 现在通过 `InputEffects::drop_y` 表达（消费点不再现场清零 y 分量）。默认关闭，保持原有手感。
    pub mouse_horizontal_only: bool,
    // free look
    /// 无点击鼠标轨迹转视角：窗口聚焦时不需要按住任何键/鼠标键，
    /// 鼠标位移直接合成"落下 → 移动 → 松手"的滑动手势。默认关闭。
    pub free_look_enabled: bool,
    /// free look 的输入效果：`scale` 承载灵敏度，`invert_x/invert_y` 承载翻转。
    /// 与按钮级 `effects` 是同一套结构，只是这里挂在全局配置上。
    #[serde(default)]
    pub free_look_effects: InputEffects,
    /// free look 的退出键（Bevy `KeyCode` 名称，默认 `ControlLeft`）。
    ///
    /// **仅用于配置迁移**：E0 的 `v0→v1` 迁移器会把它转成 `free_look_exit_keys` 数组；
    /// 代码不再读取该字段（保留字段是为了旧 JSON 能被 serde 接受并交给迁移器）。
    #[serde(default = "default_free_look_exit_key")]
    pub free_look_exit_key: String,
    /// free look 退出键（可多个，任一按下退出；为空时沿用旧字段迁移出的值）。
    #[serde(default = "default_free_look_exit_keys")]
    pub free_look_exit_keys: Vec<String>,
    /// free look 恢复键（可多个，任一按下进入；为空则不单独提供恢复键）。
    #[serde(default)]
    pub free_look_enter_keys: Vec<String>,
    /// 自由视角激活期间屏蔽的鼠标键（字符串形式，如 `"M-Left"`）。
    /// **默认空列表**：默认不屏蔽任何键，由用户自行配置。
    /// 仅用于 v0→v2 迁移；新逻辑请使用 `access_rules`（`docs/26`）。
    #[serde(default)]
    pub free_look_blocked_mouse_buttons: Vec<String>,
    /// 访问规则：在某种状态下允许/不允许某个映射触发（`docs/26`）。
    /// **默认空表**：没有规则时全部允许。
    #[serde(default)]
    pub access_rules: Vec<AccessRule>,
    /// 虚拟光标：默认不显示；启用后在自由视角激活时显示（普通模式不显示）。
    #[serde(default)]
    pub virtual_cursor_visible: bool,
    /// 配置版本号，供兼容器使用；旧文件没有该字段时视为 0。
    #[serde(default = "default_config_version")]
    pub config_version: u32,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            adb_path: default_adb_path(),
            adb_connect_address: String::new(),
            adb_cleanup_policy: AdbCleanupPolicy::default(),
            web_port: 27799,
            web_bind_addr: default_web_bind_addr(),
            controller_port: 27798,
            connect_timeout_secs: 10,
            always_on_top: true,
            titlebar_visible: true,
            vertical_mask_height: 720,
            horizontal_mask_width: 1280,
            vertical_position: (100, 100),
            horizontal_position: (100, 100),
            active_mapping_file: "default.json".to_string(),
            mapping_label_opacity: 0.3,
            language: DEFAULT_LANGUAGE.to_string(),
            clipboard_sync: true,
            touch_backend: TouchBackend::Sdk,
            video_codec: VideoCodec::H264,
            video_bit_rate: 8_000000, // 8M
            video_max_size: 0,        // default no limit
            video_max_fps: 0,         // default no limit
            display_id: 0,
            new_display_enabled: false,
            new_display_use_main_size: true,
            new_display_width: 1280,
            new_display_height: 720,
            new_display_dpi: 240,
            audio_codec: AudioCodec::Opus,
            audio_bit_rate: 128_000,
            audio_source: AudioSource::Output,
            audio_dup: false,
            stay_awake: false,
            screen_off_timeout: -1, // default keep device setting
            power_off_on_close: false,
            mouse_horizontal_only: false,
            free_look_enabled: false,
            free_look_effects: InputEffects::default(),
            free_look_exit_key: default_free_look_exit_key(),
            free_look_exit_keys: default_free_look_exit_keys(),
            free_look_enter_keys: Vec::new(),
            free_look_blocked_mouse_buttons: Vec::new(),
            access_rules: Vec::new(),
            virtual_cursor_visible: false,
            config_version: CURRENT_CONFIG_VERSION,
        }
    }
}

macro_rules! define_setter {
    ($(($field:ident, $typ:ty)),* $(,)?) => {
        paste! {
            $(
                pub fn [<set_ $field>] (value: $typ) {
                    CONFIG.write().unwrap().$field = value;
                    Self::save().unwrap();
                }
            )*
        }
    };
}

impl LocalConfig {
    /// 选择可用的 adb：配置路径失效时自动回退到内置 adb，再回退到 PATH 中的 adb。
    ///
    /// 修复点：原实现只在 `adb_path == "adb"` 时替换。一旦某次运行把内置 adb 的**绝对路径**
    /// 写进 config.json，之后移动/更新程序目录，该路径就永久失效（日志表现为
    /// `error while starting adb server: 系统找不到指定的路径 (os error 3)`，设备列表为空）。
    pub fn prefer_bundled_adb() {
        let bundled = bundled_adb_path();
        let mut config = CONFIG.write().unwrap();
        let current = config.adb_path.clone();
        let current_valid = !current.is_empty() && Path::new(&current).is_file();

        if current_valid {
            return;
        }

        if bundled.is_file() {
            let bundled_str = bundled.to_string_lossy().into_owned();
            log::warn!(
                "[Config] configured adb path is invalid: {} -> bundled adb: {}",
                if current.is_empty() { "<empty>" } else { current.as_str() },
                bundled_str
            );
            config.adb_path = bundled_str;
        } else if !current.is_empty() && current != "adb" {
            log::warn!(
                "[Config] configured adb path is invalid: {} -> adb (PATH)",
                current
            );
            config.adb_path = "adb".to_string();
        }
    }

    pub fn save() -> Result<(), String> {
        let config_json = to_string_pretty(&Self::get())
            .map_err(|e| format!("{}: {}", t!("localConfig.serializeConfigError"), e))?;

        let path = relate_to_data_path(["config.json"]);
        if let Some(parent) = path.parent() {
            create_dir_all(parent)
                .map_err(|e| format!("{}: {}", t!("localConfig.createConfigDirError"), e))?;
        }
        let mut file = File::create(path)
            .map_err(|e| format!("{}: {}", t!("localConfig.createConfigError"), e))?;
        file.write_all(config_json.as_bytes())
            .map_err(|e| format!("{}: {}", t!("localConfig.writeConfigError"), e))?;
        Ok(())
    }

    pub fn load() -> Result<(), String> {
        let path = relate_to_data_path(["config.json"]);
        let config_string = std::fs::read_to_string(&path).map_err(|e| {
            format!(
                "{} {}: {}",
                t!("localConfig.readConfigError"),
                path.to_str().unwrap(),
                e
            )
        })?;
        let mut value: serde_json::Value = serde_json::from_str(&config_string)
            .map_err(|e| format!("{}: {}", t!("localConfig.serializeConfigError"), e))?;
        let original_version = value
            .get("config_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        // 迁移需要知道"哪些映射绑定了哪些键"（把旧屏蔽列表转成规则）；读不到就传空表。
        let mapping_binds = read_active_mapping_binds();
        let migrated = migrate_config(&mut value, &mapping_binds);
        let mut config: LocalConfig = serde_json::from_value(value)
            .map_err(|e| format!("{}: {}", t!("localConfig.serializeConfigError"), e))?;
        // D：清理指向"当前映射文件里不存在"的规则（例如旧前端误存的 pointer 编号）。
        let valid_ids: std::collections::HashSet<String> =
            mapping_binds.keys().cloned().collect();
        let before_rules = config.access_rules.len();
        if !valid_ids.is_empty() {
            config.access_rules = prune_access_rules(config.access_rules, &valid_ids);
        }
        let pruned = config.access_rules.len() != before_rules;
        if pruned {
            log::warn!(
                "清理了 {} 条指向不存在映射的访问规则",
                before_rules - config.access_rules.len()
            );
        }
        *CONFIG.write().unwrap() = config;
        // 老配置升级 / 规则被清理后立即写回，避免每次启动重复处理。
        if pruned || (migrated && original_version < CURRENT_CONFIG_VERSION as u64) {
            Self::save()?;
        }
        Ok(())
    }

    pub fn get() -> LocalConfig {
        CONFIG.read().unwrap().clone()
    }

    pub fn get_clipboard_sync() -> bool {
        CONFIG.read().unwrap().clipboard_sync
    }

    /// 只传输鼠标横向分量（纵向位移丢弃）。每帧读取，避免 clone 整个配置。
    pub fn mouse_horizontal_only() -> bool {
        CONFIG.read().unwrap().mouse_horizontal_only
    }

    /// free look 开关。每帧读取，避免 clone 整个配置。
    pub fn free_look_enabled() -> bool {
        CONFIG.read().unwrap().free_look_enabled
    }

    /// free look 的输入效果（灵敏度 + 翻转）。每帧读取。
    pub fn free_look_effects() -> InputEffects {
        CONFIG.read().unwrap().free_look_effects
    }

    /// free look 退出键集合（可能多个）。每帧读取。
    pub fn free_look_exit_keys() -> Vec<String> {
        CONFIG.read().unwrap().free_look_exit_keys.clone()
    }

    /// free look 恢复键集合（可能多个）。每帧读取。
    pub fn free_look_enter_keys() -> Vec<String> {
        CONFIG.read().unwrap().free_look_enter_keys.clone()
    }

    /// 自由视角激活期间屏蔽的鼠标键（字符串集合）。每帧读取。
    pub fn free_look_blocked_mouse_buttons() -> Vec<String> {
        CONFIG.read().unwrap().free_look_blocked_mouse_buttons.clone()
    }

    /// 虚拟光标是否启用（默认不显示）。每帧读取。
    pub fn virtual_cursor_visible() -> bool {
        CONFIG.read().unwrap().virtual_cursor_visible
    }

    /// 访问规则（默认空表）。配置变更时重建。
    pub fn access_rules() -> Vec<AccessRule> {
        CONFIG.read().unwrap().access_rules.clone()
    }

    /// 灵敏度同时写两个轴的 scale（前端只有一个滑块）。
    pub fn set_free_look_sensitivity(value: f32) {
        let mut config = CONFIG.write().unwrap();
        config.free_look_effects.scale_x = value;
        config.free_look_effects.scale_y = value;
        drop(config);
        let _ = Self::save();
    }

    /// 只改翻转位，保留灵敏度。
    pub fn set_free_look_invert_x(value: bool) {
        let mut config = CONFIG.write().unwrap();
        config.free_look_effects.invert_x = value;
        drop(config);
        let _ = Self::save();
    }

    pub fn set_free_look_invert_y(value: bool) {
        let mut config = CONFIG.write().unwrap();
        config.free_look_effects.invert_y = value;
        drop(config);
        let _ = Self::save();
    }

    /// 端口被占用后切换到候选端口：只改内存，不写盘。
    /// 不持久化是有意的——占用往往是临时的，记住新端口会让配置持续漂移。
    pub fn set_controller_port_in_memory(port: u16) {
        CONFIG.write().unwrap().controller_port = port;
    }

    pub fn set_web_port_in_memory(port: u16) {
        CONFIG.write().unwrap().web_port = port;
    }

    define_setter!(
        (web_port, u16),
        (web_bind_addr, Ipv4Addr),
        (controller_port, u16),
        (adb_path, String),
        (adb_connect_address, String),
        (adb_cleanup_policy, AdbCleanupPolicy),
        (always_on_top, bool),
        (titlebar_visible, bool),
        (vertical_mask_height, u32),
        (horizontal_mask_width, u32),
        (vertical_position, (i32, i32)),
        (horizontal_position, (i32, i32)),
        (active_mapping_file, String),
        (mapping_label_opacity, f32),
        (language, String),
        (clipboard_sync, bool),
        (touch_backend, TouchBackend),
        (video_codec, VideoCodec),
        (video_bit_rate, u32),
        (video_max_size, u32),
        (video_max_fps, u32),
        (display_id, i32),
        (new_display_enabled, bool),
        (new_display_use_main_size, bool),
        (new_display_width, u32),
        (new_display_height, u32),
        (new_display_dpi, u32),
        (audio_codec, AudioCodec),
        (audio_bit_rate, u32),
        (audio_source, AudioSource),
        (audio_dup, bool),
        (stay_awake, bool),
        (screen_off_timeout, i32),
        (power_off_on_close, bool),
        (free_look_enabled, bool),
        (free_look_exit_key, String),
        (free_look_exit_keys, Vec<String>),
        (free_look_enter_keys, Vec<String>),
        (free_look_blocked_mouse_buttons, Vec<String>),
        (access_rules, Vec<AccessRule>),
        (virtual_cursor_visible, bool),
    );
}

/// 移除指向"当前映射文件里不存在"的访问规则（防止旧 UI 误存的 pointer 编号长期残留）。
pub fn prune_access_rules(
    rules: Vec<AccessRule>,
    valid_ids: &std::collections::HashSet<String>,
) -> Vec<AccessRule> {
    rules
        .into_iter()
        .filter(|rule| valid_ids.contains(&rule.mapping_id))
        .collect()
}

/// 读取当前映射文件里"映射 id → 绑定的输入字符串"。
///
/// 迁移 v0→v2 需要它把旧的 `free_look_blocked_mouse_buttons` 转成按映射 id 的规则。
/// 只收集 `bind` / `pad_bind` 字段下的字符串，避免把 note/id 误当成绑定。
fn read_active_mapping_binds() -> std::collections::HashMap<String, Vec<String>> {
    let file = LocalConfig::get().active_mapping_file;
    if file.is_empty() {
        return std::collections::HashMap::new();
    }
    let path = relate_to_data_path(["mapping", &file]);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return std::collections::HashMap::new();
    };
    let Some(mappings) = value.get("mappings").and_then(|v| v.as_object()) else {
        return std::collections::HashMap::new();
    };
    let mut out = std::collections::HashMap::new();
    for (id, mapping) in mappings {
        let mut binds = Vec::new();
        collect_binds(mapping, &mut binds);
        if !binds.is_empty() {
            out.insert(id.clone(), binds);
        }
    }
    out
}

fn collect_binds(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_binds(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                if key == "bind" || key == "pad_bind" {
                    collect_binds(item, out);
                }
            }
        }
        _ => {}
    }
}

/// 配置版本迁移：按 `config_version` 升级 JSON。
///
/// v1 从未发布，所以**只保留一个 v0→v2 迁移器**（用户决定）。
/// `mapping_binds`：`mapping_id → 该映射绑定的输入字符串列表`，由调用方从当前映射文件读取；
/// 为空表示映射文件不可用（此时保留旧字段、不生成规则）。
///
/// 返回是否执行了迁移。文件版本高于当前程序版本时**不迁移、不改写**，
/// 避免用户在新版程序里用旧版程序打开时被降级破坏。
pub fn migrate_config(
    value: &mut serde_json::Value,
    mapping_binds: &std::collections::HashMap<String, Vec<String>>,
) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let version = obj
        .get("config_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if version > CURRENT_CONFIG_VERSION as u64 {
        log::warn!(
            "config.json 版本 {} 高于当前支持的 {}，跳过迁移",
            version,
            CURRENT_CONFIG_VERSION
        );
        return false;
    }

    let mut migrated = false;
    if version < CURRENT_CONFIG_VERSION as u64 {
        migrate_v0_to_v2(obj, mapping_binds);
        migrated = true;
    }
    if migrated {
        obj.insert(
            "config_version".to_string(),
            serde_json::Value::from(CURRENT_CONFIG_VERSION),
        );
    }
    migrated
}

/// v0 → v2（一次到位）：
/// - 单个 `free_look_exit_key` → `free_look_exit_keys` 数组（并删除旧键）；
/// - 补齐 `free_look_enter_keys` / `virtual_cursor_visible` / `access_rules`；
/// - `free_look_blocked_mouse_buttons` 非空且映射文件可用时，转成
///   `{mapping_id, when: "free_look_active", allow: false}` 规则；
///   映射文件不可用时**保留旧字段**（下次启动再试）。
fn migrate_v0_to_v2(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    mapping_binds: &std::collections::HashMap<String, Vec<String>>,
) {
    if obj.contains_key("free_look_exit_keys") {
        // 新字段已存在，旧字段只是残留。
        obj.remove("free_look_exit_key");
    } else {
        let keys = match obj.remove("free_look_exit_key") {
            Some(serde_json::Value::String(key)) if !key.is_empty() => {
                vec![serde_json::Value::String(key)]
            }
            _ => vec![serde_json::Value::String(default_free_look_exit_key())],
        };
        obj.insert(
            "free_look_exit_keys".to_string(),
            serde_json::Value::Array(keys),
        );
    }
    obj.entry("free_look_enter_keys")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    obj.entry("virtual_cursor_visible")
        .or_insert(serde_json::Value::Bool(false));
    obj.entry("access_rules")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));

    let blocked: Vec<String> = obj
        .get("free_look_blocked_mouse_buttons")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if blocked.is_empty() {
        return;
    }
    if mapping_binds.is_empty() {
        log::warn!(
            "迁移访问规则时无法读取映射文件，保留 free_look_blocked_mouse_buttons 以便下次重试"
        );
        return;
    }

    let mut rules = obj
        .get("access_rules")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for (mapping_id, binds) in mapping_binds {
        if binds.iter().any(|bind| blocked.iter().any(|key| key == bind)) {
            rules.push(serde_json::json!({
                "mapping_id": mapping_id,
                "when": crate::access_rules::STATE_FREE_LOOK_ACTIVE,
                "allow": false,
            }));
        }
    }
    obj.insert(
        "access_rules".to_string(),
        serde_json::Value::Array(rules),
    );
}
