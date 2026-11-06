use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use rust_i18n::t;
use serde::Deserialize;
use std::net::Ipv4Addr;
use tokio::sync::oneshot;

use crate::{
    config::{AUDIO_BIT_RATE_MIN, AdbCleanupPolicy, LocalConfig, TouchBackend},
    is_available_language,
    mask::mask_command::MaskCommand,
    scrcpy::{
        adb::Adb,
        media::{AudioCodec, AudioSource, VideoCodec},
    },
    utils::{
        IDENTIFIER, check_for_update, get_mask_scale_factor, mask_win_move_helper,
        relate_to_data_path,
        share::{ControlledDevice, PortNotice, UpdateInfo},
    },
    web::{JsonResponse, WebServerError},
};

#[derive(Debug, Clone)]
pub struct AppStatConfig {
    m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
}

pub fn routers(
    m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
) -> Router {
    Router::new()
        .route("/get_config", get(get_config))
        .route("/update_config", post(update_config))
        .route("/port_notice", get(get_port_notice))
        .route("/access_state", get(get_access_state))
        .route("/open_data_path", get(open_data_path))
        .route("/get_update_info", get(get_update_info))
        .route("/check_update", get(check_update))
        .with_state(AppStatConfig { m_tx })
}

/// 取走"端口被占用、已改用别的端口"的提示（取走即清空，避免每次进设置页都弹）。
async fn get_port_notice() -> Result<JsonResponse, WebServerError> {
    Ok(JsonResponse::success(
        "port notice".to_string(),
        Some(serde_json::json!({ "notice": PortNotice::take() })),
    ))
}

/// 访问规则诊断：当前状态 + 自由视角是否激活（规则命中判断依赖这两项）。
async fn get_access_state() -> Result<JsonResponse, WebServerError> {
    use crate::mask::mapping::{free_look, mapping_trigger};
    Ok(JsonResponse::success(
        "access state".to_string(),
        Some(serde_json::json!({
            "state": mapping_trigger::current_access_state(),
            "free_look_active": free_look::is_active_global(),
        })),
    ))
}

async fn get_config() -> Result<JsonResponse, WebServerError> {
    let config = LocalConfig::get();
    Ok(JsonResponse::success(
        t!("web.config.getLocalConfigSuccess"),
        Some(serde_json::to_value(&config).unwrap()),
    ))
}

async fn open_data_path() -> Result<JsonResponse, WebServerError> {
    let path = dirs::data_dir().unwrap().join(IDENTIFIER);
    opener::open(path).map_err(|e| {
        WebServerError::bad_request(format!("{}: {}", t!("web.config.openDataPathFailed"), e))
    })?;

    return Ok(JsonResponse::success(
        t!("web.config.openDataPathSuccess"),
        None,
    ));
}

async fn get_update_info() -> Result<JsonResponse, WebServerError> {
    let info = UpdateInfo::get().await;
    return Ok(JsonResponse::success(
        t!("web.config.getUpdateInfoSuccess"),
        Some(serde_json::to_value(&info).unwrap()),
    ));
}

async fn check_update() -> Result<JsonResponse, WebServerError> {
    match check_for_update().await {
        Err(e) => {
            return Err(WebServerError::bad_request(e.to_string()));
        }
        Ok(()) => {
            let info = UpdateInfo::get().await;
            return Ok(JsonResponse::success(
                t!("web.config.getUpdateInfoSuccess"),
                Some(serde_json::to_value(&info).unwrap()),
            ));
        }
    };
}

#[derive(Deserialize)]
struct PostDataUpdateConfig {
    key: String,
    value: serde_json::Value,
    #[serde(default)]
    space: PixelSpace,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PixelSpace {
    Logical,
    Physical,
}

impl Default for PixelSpace {
    fn default() -> Self {
        Self::Logical
    }
}

async fn scale_factor_for_pixel_space(
    pixel_space: PixelSpace,
    state: &AppStatConfig,
) -> Result<f32, WebServerError> {
    match pixel_space {
        PixelSpace::Logical => Ok(1.0),
        PixelSpace::Physical => get_mask_scale_factor(&state.m_tx)
            .await
            .map_err(WebServerError::bad_request),
    }
}

fn u32_to_logical(value: u64, scale_factor: f32) -> u32 {
    (value as f32 / scale_factor).round() as u32
}

fn i32_to_logical(value: i64, scale_factor: f32) -> i32 {
    (value as f32 / scale_factor).round() as i32
}

async fn update_config(
    State(state): State<AppStatConfig>,
    Json(payload): Json<PostDataUpdateConfig>,
) -> Result<JsonResponse, WebServerError> {
    // sync with src/config.rs
    match payload.key.as_str() {
        "language" => {
            if let Some(value) = payload.value.as_str() {
                if !is_available_language(value) {
                    return Err(WebServerError::bad_request(format!(
                        "{}: {}",
                        t!("web.config.invalidLanguage"),
                        value
                    )));
                }
                rust_i18n::set_locale(value);
                LocalConfig::set_language(value.to_string());
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.setLanguageSuccess"), value),
                    None,
                ));
            } else {
                return Err(WebServerError::bad_request(t!(
                    "web.config.languageMustBeString"
                )));
            }
        }
        "web_port" => {
            if let Some(value) = payload.value.as_u64() {
                LocalConfig::set_web_port(value as u16);
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.restartToApplyWebPort"), value),
                    None,
                ));
            } else {
                return Err(WebServerError::bad_request(t!(
                    "web.config.webPortMustBeU16"
                )));
            }
        }
        "web_bind_addr" => {
            if let Some(value) = payload.value.as_str() {
                let value = value.trim();
                let addr = value.parse::<Ipv4Addr>().map_err(|_| {
                    WebServerError::bad_request(format!(
                        "{}: {}",
                        t!("web.config.webBindAddrMustBeIpv4"),
                        value
                    ))
                })?;
                LocalConfig::set_web_bind_addr(addr);
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.restartToApplyWebBindAddr"), addr),
                    None,
                ));
            } else {
                return Err(WebServerError::bad_request(t!(
                    "web.config.webBindAddrMustBeIpv4"
                )));
            }
        }
        "adb_path" => {
            if let Some(value) = payload.value.as_str() {
                match Adb::check_adb_path(value) {
                    Ok(_) => {
                        LocalConfig::set_adb_path(value.to_string());
                        return Ok(JsonResponse::success(
                            format!("{}: {}", t!("web.config.adbPathSetSuccess"), value),
                            None,
                        ));
                    }
                    Err(e) => {
                        return Err(WebServerError::bad_request(format!(
                            "{}: {}",
                            t!("web.config.adbPathSetFailed"),
                            e
                        )));
                    }
                }
            } else {
                return Err(WebServerError::bad_request(t!(
                    "web.config.adbPathMustBeString"
                )));
            }
        }
        "adb_connect_address" => {
            if let Some(value) = payload.value.as_str() {
                let address = value.trim();
                LocalConfig::set_adb_connect_address(address.to_string());
                return Ok(JsonResponse::success(
                    format!(
                        "{}: {}",
                        t!("web.config.setAdbConnectAddressSuccess"),
                        address
                    ),
                    None,
                ));
            } else {
                return Err(WebServerError::bad_request(t!(
                    "web.config.adbConnectAddressMustBeString"
                )));
            }
        }
        "adb_cleanup_policy" => {
            let policy: AdbCleanupPolicy = serde_json::from_value(payload.value.clone())
                .map_err(|_| WebServerError::bad_request("adb_cleanup_policy must be auto/always/never"))?;
            LocalConfig::set_adb_cleanup_policy(policy);
            return Ok(JsonResponse::success(
                format!("adb 清理策略已设为 {:?}", policy),
                None,
            ));
        }
        "controller_port" => {
            if let Some(value) = payload.value.as_u64() {
                LocalConfig::set_controller_port(value as u16);
                return Ok(JsonResponse::success(
                    format!(
                        "{}: {}",
                        t!("web.config.restartToApplyControllerPort"),
                        value
                    ),
                    None,
                ));
            } else {
                return Err(WebServerError::bad_request(t!(
                    "web.config.controllerPortMustBeU16"
                )));
            }
        }
        "always_on_top" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_always_on_top(value);
                let (oneshot_tx, oneshot_rx) = oneshot::channel::<Result<String, String>>();
                state
                    .m_tx
                    .send((MaskCommand::WinSwitchLevel { top: value }, oneshot_tx))
                    .unwrap();
                let msg = oneshot_rx.await.unwrap().unwrap();
                return Ok(JsonResponse::success(msg, None));
            } else {
                return Err(WebServerError::bad_request(t!(
                    "web.config.alwaysOnTopMustBeBool"
                )));
            }
        }
        "titlebar_visible" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_titlebar_visible(value);
                let (oneshot_tx, oneshot_rx) = oneshot::channel::<Result<String, String>>();
                state
                    .m_tx
                    .send((MaskCommand::ToggleTitlebar, oneshot_tx))
                    .unwrap();
                let msg = oneshot_rx.await.unwrap().unwrap();
                return Ok(JsonResponse::success(msg, None));
            } else {
                return Err(WebServerError::bad_request(t!(
                    "web.config.titlebarVisibleMustBeBool"
                )));
            }
        }
        "vertical_mask_height" => {
            if let Some(value) = payload.value.as_u64() {
                let scale_factor = scale_factor_for_pixel_space(payload.space, &state).await?;
                LocalConfig::set_vertical_mask_height(u32_to_logical(value, scale_factor));
                if let Some(main_device) = ControlledDevice::get_main_device().await {
                    let (device_w, device_h) = main_device.device_size;
                    let msg = mask_win_move_helper(device_w, device_h, &state.m_tx).await;
                    return Ok(JsonResponse::success(
                        format!("{}. {}", t!("web.config.setVerticalMaskHeightSuccess"), msg),
                        None,
                    ));
                }
                return Ok(JsonResponse::success(
                    format!("{}", t!("web.config.setVerticalMaskHeightSuccess")),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.verticalMaskHeightMustBeu32"
            )));
        }
        "horizontal_mask_width" => {
            if let Some(value) = payload.value.as_u64() {
                let scale_factor = scale_factor_for_pixel_space(payload.space, &state).await?;
                LocalConfig::set_horizontal_mask_width(u32_to_logical(value, scale_factor));
                if let Some(main_device) = ControlledDevice::get_main_device().await {
                    let (device_w, device_h) = main_device.device_size;
                    let msg = mask_win_move_helper(device_w, device_h, &state.m_tx).await;
                    return Ok(JsonResponse::success(
                        format!(
                            "{}. {}",
                            t!("web.config.setHorizontalMaskWidthSuccess"),
                            msg
                        ),
                        None,
                    ));
                }
                return Ok(JsonResponse::success(
                    t!("web.config.setHorizontalMaskWidthSuccess"),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.horizontalMaskWidthMustBeu32"
            )));
        }
        "vertical_position" => {
            if let Some(value) = payload.value.as_array() {
                if value.len() == 2 {
                    if let (Some(x), Some(y)) = (value[0].as_i64(), value[1].as_i64()) {
                        let scale_factor =
                            scale_factor_for_pixel_space(payload.space, &state).await?;
                        LocalConfig::set_vertical_position((
                            i32_to_logical(x, scale_factor),
                            i32_to_logical(y, scale_factor),
                        ));
                        if let Some(main_device) = ControlledDevice::get_main_device().await {
                            let (device_w, device_h) = main_device.device_size;
                            let msg = mask_win_move_helper(device_w, device_h, &state.m_tx).await;
                            return Ok(JsonResponse::success(
                                format!("{}. {}", t!("web.config.setVerticalPositionSuccess"), msg),
                                None,
                            ));
                        }
                        return Ok(JsonResponse::success(
                            format!("{}", t!("web.config.setVerticalPositionSuccess")),
                            None,
                        ));
                    }
                }
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.verticalPositionTypeError"
            )));
        }
        "horizontal_position" => {
            if let Some(value) = payload.value.as_array() {
                if value.len() == 2 {
                    if let (Some(x), Some(y)) = (value[0].as_i64(), value[1].as_i64()) {
                        let scale_factor =
                            scale_factor_for_pixel_space(payload.space, &state).await?;
                        LocalConfig::set_horizontal_position((
                            i32_to_logical(x, scale_factor),
                            i32_to_logical(y, scale_factor),
                        ));
                        if let Some(main_device) = ControlledDevice::get_main_device().await {
                            let (device_w, device_h) = main_device.device_size;
                            let msg = mask_win_move_helper(device_w, device_h, &state.m_tx).await;
                            return Ok(JsonResponse::success(
                                format!(
                                    "{}. {}",
                                    t!("web.config.setHorizontalPositionSuccess"),
                                    msg
                                ),
                                None,
                            ));
                        }
                        return Ok(JsonResponse::success(
                            format!("{}", t!("web.config.setHorizontalPositionSuccess")),
                            None,
                        ));
                    }
                }
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.horizontalPositionTypeError"
            )));
        }
        "active_mapping_file" => {
            return Err(WebServerError::bad_request(format!(
                "{}",
                t!("web.config.pleaseRequestForOperation", api => "/api/mapping/change_active_mapping")
            )));
        }
        "mapping_label_opacity" => {
            if let Some(value) = payload.value.as_f64() {
                if value <= 1.0 && value >= 0.0 {
                    LocalConfig::set_mapping_label_opacity(value as f32);
                    return Ok(JsonResponse::success(
                        format!(
                            "{}: {}",
                            t!("web.config.setMappingLabelOpacitySuccess"),
                            value
                        ),
                        None,
                    ));
                }
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.mappingLabelOpacityRange"
            )));
        }
        "touch_backend" => {
            let backend: TouchBackend = serde_json::from_value(payload.value.clone())
                .map_err(|_| WebServerError::bad_request("touch_backend must be sdk or uhid"))?;
            if ControlledDevice::is_any_device_controlled().await {
                return Err(WebServerError::bad_request(
                    "Disconnect all devices before changing the touch backend / 请先断开所有设备再切换触控后端",
                ));
            }
            LocalConfig::set_touch_backend(backend);
            return Ok(JsonResponse::success(
                "触控后端已保存，下次连接生效 / Touch backend saved for next connection",
                None,
            ));
        }
        "clipboard_sync" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_clipboard_sync(value);
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.setClipboardSyncSuccess"), value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.clipboardSyncTypeError"
            )));
        }
        "video_codec" => {
            if let Some(value) = payload.value.as_str() {
                let codec = match value {
                    "H264" => VideoCodec::H264,
                    "H265" => VideoCodec::H265,
                    "AV1" => VideoCodec::AV1,
                    _ => {
                        return Err(WebServerError::bad_request(format!(
                            "{}: {}",
                            t!("web.config.invalidVideoCodec"),
                            value
                        )));
                    }
                };
                LocalConfig::set_video_codec(codec);
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.setVideoCodecSuccess"), value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.videoCodecTypeError"
            )));
        }
        "video_bit_rate" => {
            if let Some(value) = payload.value.as_u64() {
                LocalConfig::set_video_bit_rate(value as u32);
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.setVideoBitRateSuccess"), value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.videoBitRateTypeError"
            )));
        }
        "video_max_size" => {
            if let Some(value) = payload.value.as_u64() {
                LocalConfig::set_video_max_size(value as u32);
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.setVideoMaxSizeSuccess"), value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.videoMaxSizeTypeError"
            )));
        }
        "video_max_fps" => {
            if let Some(value) = payload.value.as_u64() {
                LocalConfig::set_video_max_fps(value as u32);
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.setVideoMaxFpsSuccess"), value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.videoMaxFpsTypeError"
            )));
        }
        "display_id" => {
            if let Some(value) = payload.value.as_i64() {
                let value = i32::try_from(value).map_err(|_| {
                    WebServerError::bad_request(t!("web.config.displayIdTypeError"))
                })?;
                LocalConfig::set_display_id(value);
                return Ok(JsonResponse::success(
                    format!("{}: {}", t!("web.config.setDisplayIdSuccess"), value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.displayIdTypeError"
            )));
        }
        "new_display_enabled" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_new_display_enabled(value);
                return Ok(JsonResponse::success(
                    format!(
                        "{}: {}",
                        t!("web.config.setNewDisplayEnabledSuccess"),
                        value
                    ),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.newDisplayEnabledTypeError"
            )));
        }
        "new_display_use_main_size" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_new_display_use_main_size(value);
                return Ok(JsonResponse::success(
                    format!(
                        "{}: {}",
                        t!("web.config.setNewDisplayUseMainSizeSuccess"),
                        value
                    ),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.newDisplayUseMainSizeTypeError"
            )));
        }
        "new_display_width" => {
            if let Some(value) = payload.value.as_u64() {
                if let Ok(value) = u32::try_from(value) {
                    if value > 0 {
                        LocalConfig::set_new_display_width(value);
                        return Ok(JsonResponse::success(
                            format!("{}: {}", t!("web.config.setNewDisplayWidthSuccess"), value),
                            None,
                        ));
                    }
                }
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.newDisplayWidthTypeError"
            )));
        }
        "new_display_height" => {
            if let Some(value) = payload.value.as_u64() {
                if let Ok(value) = u32::try_from(value) {
                    if value > 0 {
                        LocalConfig::set_new_display_height(value);
                        return Ok(JsonResponse::success(
                            format!("{}: {}", t!("web.config.setNewDisplayHeightSuccess"), value),
                            None,
                        ));
                    }
                }
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.newDisplayHeightTypeError"
            )));
        }
        "new_display_dpi" => {
            if let Some(value) = payload.value.as_u64() {
                if let Ok(value) = u32::try_from(value) {
                    if value > 0 {
                        LocalConfig::set_new_display_dpi(value);
                        return Ok(JsonResponse::success(
                            format!("{}: {}", t!("web.config.setNewDisplayDpiSuccess"), value),
                            None,
                        ));
                    }
                }
            }
            return Err(WebServerError::bad_request(t!(
                "web.config.newDisplayDpiTypeError"
            )));
        }
        "audio_codec" => {
            if let Some(value) = payload.value.as_str() {
                let codec = match value {
                    "OPUS" => AudioCodec::Opus,
                    "AAC" => AudioCodec::Aac,
                    "FLAC" => AudioCodec::Flac,
                    "RAW" => AudioCodec::Raw,
                    _ => {
                        return Err(WebServerError::bad_request(format!(
                            "Invalid audio codec: {}",
                            value
                        )));
                    }
                };
                LocalConfig::set_audio_codec(codec);
                return Ok(JsonResponse::success(
                    format!("Audio codec set: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request("Audio codec must be string"));
        }
        "audio_bit_rate" => {
            if let Some(value) = payload.value.as_u64() {
                let value = u32::try_from(value)
                    .map_err(|_| WebServerError::bad_request("Audio bit rate must be u32"))?;
                if value < AUDIO_BIT_RATE_MIN {
                    return Err(WebServerError::bad_request(format!(
                        "Audio bit rate must be at least {}",
                        AUDIO_BIT_RATE_MIN
                    )));
                }
                LocalConfig::set_audio_bit_rate(value);
                return Ok(JsonResponse::success(
                    format!("Audio bit rate set: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request("Audio bit rate must be u32"));
        }
        "audio_source" => {
            if let Some(value) = payload.value.as_str() {
                let source = match value {
                    "OUTPUT" => AudioSource::Output,
                    "PLAYBACK" => AudioSource::Playback,
                    "MIC" => AudioSource::Mic,
                    _ => {
                        return Err(WebServerError::bad_request(format!(
                            "Invalid audio source: {}",
                            value
                        )));
                    }
                };
                LocalConfig::set_audio_source(source);
                if !source.is_playback() {
                    LocalConfig::set_audio_dup(false);
                }
                return Ok(JsonResponse::success(
                    format!("Audio source set: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request("Audio source must be string"));
        }
        "audio_dup" => {
            if let Some(value) = payload.value.as_bool() {
                if value && !LocalConfig::get().audio_source.is_playback() {
                    return Err(WebServerError::bad_request(
                        "Audio duplication requires playback source",
                    ));
                }
                LocalConfig::set_audio_dup(value);
                return Ok(JsonResponse::success(
                    format!("Audio duplication set: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(
                "Audio duplication must be bool",
            ));
        }
        "stay_awake" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_stay_awake(value);
                return Ok(JsonResponse::success(
                    format!("Stay awake set: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request("Stay awake must be bool"));
        }
        "screen_off_timeout" => {
            if let Some(value) = payload.value.as_i64() {
                let value = i32::try_from(value)
                    .map_err(|_| WebServerError::bad_request("Screen off timeout must be i32"))?;
                if value < -1 {
                    return Err(WebServerError::bad_request(
                        "Screen off timeout must be -1 or greater",
                    ));
                }
                LocalConfig::set_screen_off_timeout(value);
                return Ok(JsonResponse::success(
                    format!("Screen off timeout set: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(
                "Screen off timeout must be i32",
            ));
        }
        "power_off_on_close" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_power_off_on_close(value);
                return Ok(JsonResponse::success(
                    format!("Power off on close set: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(
                "Power off on close must be bool",
            ));
        }
        "free_look_enabled" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_free_look_enabled(value);
                return Ok(JsonResponse::success(
                    if value {
                        "无点击转视角已开启".to_string()
                    } else {
                        "无点击转视角已关闭".to_string()
                    },
                    None,
                ));
            }
            return Err(WebServerError::bad_request(
                "free_look_enabled must be bool",
            ));
        }
        "free_look_sensitivity" => {
            if let Some(value) = payload.value.as_f64() {
                let value = value as f32;
                if value.is_finite() && (0.05..=20.0).contains(&value.abs()) {
                    LocalConfig::set_free_look_sensitivity(value);
                    return Ok(JsonResponse::success(
                        format!("无点击转视角灵敏度已设为 {}", value),
                        None,
                    ));
                }
            }
            return Err(WebServerError::bad_request(
                "free_look_sensitivity must be a finite number within 0.05..=20 (negative reverses)",
            ));
        }
        "free_look_invert_x" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_free_look_invert_x(value);
                return Ok(JsonResponse::success(
                    format!("无点击转视角 X 轴翻转: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request("free_look_invert_x must be bool"));
        }
        "free_look_invert_y" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_free_look_invert_y(value);
                return Ok(JsonResponse::success(
                    format!("无点击转视角 Y 轴翻转: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request("free_look_invert_y must be bool"));
        }
        "free_look_exit_key" => {
            // 兼容旧前端：单个字符串视为单元素数组，走同一套校验。
            if let Some(value) = payload.value.as_str() {
                let keys = validate_free_look_keys(
                    "free_look_exit_keys",
                    vec![value.trim().to_string()],
                    &[],
                    &[],
                    false,
                )?;
                LocalConfig::set_free_look_exit_keys(keys.clone());
                return Ok(JsonResponse::success(
                    format!("退出键已设为 {:?}", keys),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(
                "free_look_exit_key must be string",
            ));
        }
        "free_look_exit_keys" => {
            let Some(value) = payload.value.as_array() else {
                return Err(WebServerError::bad_request(
                    "free_look_exit_keys must be an array of strings",
                ));
            };
            let keys = json_string_list("free_look_exit_keys", value)?;
            let keys = validate_free_look_keys(
                "free_look_exit_keys",
                keys,
                &LocalConfig::free_look_enter_keys(),
                &LocalConfig::free_look_blocked_mouse_buttons(),
                false,
            )?;
            LocalConfig::set_free_look_exit_keys(keys.clone());
            return Ok(JsonResponse::success(
                format!("退出键已设为 {:?}", keys),
                None,
            ));
        }
        "free_look_enter_keys" => {
            let Some(value) = payload.value.as_array() else {
                return Err(WebServerError::bad_request(
                    "free_look_enter_keys must be an array of strings",
                ));
            };
            let keys = json_string_list("free_look_enter_keys", value)?;
            let keys = validate_free_look_keys(
                "free_look_enter_keys",
                keys,
                &LocalConfig::free_look_exit_keys(),
                &LocalConfig::free_look_blocked_mouse_buttons(),
                true,
            )?;
            LocalConfig::set_free_look_enter_keys(keys.clone());
            return Ok(JsonResponse::success(
                format!("恢复键已设为 {:?}", keys),
                None,
            ));
        }
        "free_look_blocked_mouse_buttons" => {
            let Some(value) = payload.value.as_array() else {
                return Err(WebServerError::bad_request(
                    "free_look_blocked_mouse_buttons must be an array of strings",
                ));
            };
            let keys = json_string_list("free_look_blocked_mouse_buttons", value)?;
            let keys = validate_free_look_blocked_keys(keys)?;
            LocalConfig::set_free_look_blocked_mouse_buttons(keys.clone());
            return Ok(JsonResponse::success(
                format!("屏蔽鼠标键已设为 {:?}", keys),
                None,
            ));
        }
        "virtual_cursor_visible" => {
            if let Some(value) = payload.value.as_bool() {
                LocalConfig::set_virtual_cursor_visible(value);
                return Ok(JsonResponse::success(
                    format!("虚拟光标: {}", value),
                    None,
                ));
            }
            return Err(WebServerError::bad_request(
                "virtual_cursor_visible must be bool",
            ));
        }
        "access_rules" => {
            let Some(value) = payload.value.as_array() else {
                return Err(WebServerError::bad_request(
                    "access_rules must be an array",
                ));
            };
            let mut rules = Vec::with_capacity(value.len());
            for item in value {
                let Some(obj) = item.as_object() else {
                    return Err(WebServerError::bad_request(
                        "access_rules 的每一项都必须是对象",
                    ));
                };
                let mapping_id = obj
                    .get("mapping_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if mapping_id.is_empty() {
                    return Err(WebServerError::bad_request(
                        "access_rules[].mapping_id 不能为空",
                    ));
                }
                let when = obj
                    .get("when")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if crate::access_rules::normalize_state(when).is_none() {
                    return Err(WebServerError::bad_request(format!(
                        "access_rules[].when 不支持状态 {}（可选：free_look_active / free_look_inactive / fps / normal / observation / raw_input）",
                        when
                    )));
                }
                let allow = obj
                    .get("allow")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                // 可选：映射内操作（DirectionPad 的 up/down/left/right、Script 的 press 等）
                let action = obj
                    .get("action")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                rules.push(crate::access_rules::AccessRule {
                    mapping_id: mapping_id.to_string(),
                    action,
                    when: when.to_string(),
                    allow,
                });
            }
            LocalConfig::set_access_rules(rules.clone());
            return Ok(JsonResponse::success(
                format!("访问规则已更新（{} 条）", rules.len()),
                None,
            ));
        }
        _ => Err(WebServerError::bad_request(format!(
            "{}: {}",
            t!("web.config.invalidMappingKey"),
            payload.key
        ))),
    }
}

/// 检查退出键是否已被当前生效映射占用（D10：冲突则提示更换且不生效）。
///
/// 用 JSON 值递归查找字符串相等，避免为 13 类映射各写一遍解析逻辑；
/// 映射文件不存在 / 解析失败时按"不冲突"处理（不阻塞用户改键）。
fn free_look_exit_key_conflicts(key: &str) -> bool {
    let file = LocalConfig::get().active_mapping_file;
    if file.is_empty() {
        return false;
    }
    let path = relate_to_data_path(["mapping", &file]);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    json_contains_string(&value, key)
}

fn json_contains_string(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s == needle,
        serde_json::Value::Array(items) => items
            .iter()
            .any(|item| json_contains_string(item, needle)),
        serde_json::Value::Object(map) => {
            map.values().any(|item| json_contains_string(item, needle))
        }
        _ => false,
    }
}

/// 把 JSON 数组转成去重后的字符串列表（空字符串忽略）。
fn json_string_list(
    field: &str,
    value: &[serde_json::Value],
) -> Result<Vec<String>, WebServerError> {
    let mut out: Vec<String> = Vec::new();
    for item in value {
        let Some(text) = item.as_str() else {
            return Err(WebServerError::bad_request(format!(
                "{} 的每一项都必须是字符串",
                field
            )));
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if !out.iter().any(|v| v == text) {
            out.push(text.to_string());
        }
    }
    Ok(out)
}

/// 校验退出键/恢复键列表：
/// - 必须是设置页支持的键名；
/// - 不能与另一组重复；
/// - 不能与屏蔽列表重叠（用户明确要求"提醒避免这种情况"）；
/// - 不能已被当前映射占用。
fn validate_free_look_keys(
    field: &str,
    keys: Vec<String>,
    other: &[String],
    blocked: &[String],
    allow_empty: bool,
) -> Result<Vec<String>, WebServerError> {
    if keys.is_empty() && !allow_empty {
        return Err(WebServerError::bad_request(format!("{} 不能为空", field)));
    }
    for key in &keys {
        if crate::mask::mapping::free_look::parse_key(key).is_none() {
            return Err(WebServerError::bad_request(format!(
                "{} 不支持按键 {}",
                field, key
            )));
        }
        if other.iter().any(|v| v == key) {
            return Err(WebServerError::bad_request(format!(
                "按键 {} 同时出现在退出键和恢复键里，请只保留一个",
                key
            )));
        }
        if blocked.iter().any(|v| v == key) {
            return Err(WebServerError::bad_request(format!(
                "按键 {} 同时是退出/恢复键与屏蔽键，请更换其中一个",
                key
            )));
        }
        if free_look_exit_key_conflicts(key) {
            return Err(WebServerError::bad_request(format!(
                "按键 {} 已被当前映射使用，请更换",
                key
            )));
        }
    }
    Ok(keys)
}

/// 屏蔽列表只接受鼠标键（`M-Left` 这类），避免误把键盘键填进来。
fn validate_free_look_blocked_keys(
    keys: Vec<String>,
) -> Result<Vec<String>, WebServerError> {
    for key in &keys {
        if !key.starts_with("M-") {
            return Err(WebServerError::bad_request(format!(
                "屏蔽列表只接受鼠标键（形如 M-Left），收到 {}",
                key
            )));
        }
    }
    Ok(keys)
}
