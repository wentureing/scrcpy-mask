use std::{
    fs::{File, create_dir_all},
    io::Write,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::RwLock,
};

use crate::{
    DEFAULT_LANGUAGE,
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
}

fn default_web_bind_addr() -> Ipv4Addr {
    Ipv4Addr::new(127, 0, 0, 1)
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
        let config: LocalConfig = serde_json::from_str(&config_string)
            .map_err(|e| format!("{}: {}", t!("localConfig.serializeConfigError"), e))?;
        *CONFIG.write().unwrap() = config;
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
    );
}
