use once_cell::sync::Lazy;
use serde::Serialize;
use std::sync::Mutex;
use tokio::sync::RwLock;

use crate::scrcpy::ScrcpyDevice;

static CONTROLLED_DEVICES: Lazy<RwLock<Vec<ScrcpyDevice>>> = Lazy::new(|| RwLock::new(Vec::new()));

/// 端口顺延提示：起点端口被占用时记一条，前端取走后清空。
static PORT_NOTICE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// "端口被占用、已改用别的端口"的提示。
///
/// 只放内存：端口号本身也不落盘（见 `docs/16` §16.4），提示自然也不必持久化。
pub struct PortNotice;

impl PortNotice {
    pub fn set(text: String) {
        if let Ok(mut slot) = PORT_NOTICE.lock() {
            *slot = Some(text);
        }
    }

    /// 取走提示（读后清空）。锁中毒时返回 None，不让提示影响主流程。
    pub fn take() -> Option<String> {
        PORT_NOTICE.lock().ok().and_then(|mut slot| slot.take())
    }
}

/// "端口被占用、已改用别的端口"的提示。
///
/// 只放内存：端口号本身也不落盘（见 `docs/16` §16.4），提示自然也不必持久化。
pub struct PortNotice;

impl PortNotice {
    pub fn set(text: String) {
        if let Ok(mut slot) = PORT_NOTICE.lock() {
            *slot = Some(text);
        }
    }

    /// 取走提示（读后清空）。锁中毒时返回 None，不让提示影响主流程。
    pub fn take() -> Option<String> {
        PORT_NOTICE.lock().ok().and_then(|mut slot| slot.take())
    }
}

pub struct ControlledDevice;

impl ControlledDevice {
    pub async fn get_device_list() -> Vec<ScrcpyDevice> {
        let device_list = CONTROLLED_DEVICES.read().await;
        device_list.clone()
    }

    pub async fn get_main_device() -> Option<ScrcpyDevice> {
        let device_list = CONTROLLED_DEVICES.read().await;
        device_list.iter().find(|device| device.main).cloned()
    }

    pub async fn is_any_device_controlled() -> bool {
        let device_list = CONTROLLED_DEVICES.read().await;
        !device_list.is_empty()
    }

    pub async fn is_scid_controlled(scid: &str) -> bool {
        let device_list = CONTROLLED_DEVICES.read().await;
        device_list.iter().any(|device| device.scid == scid)
    }

    pub async fn remove_device(scid: &str) {
        let mut device_list = CONTROLLED_DEVICES.write().await;
        device_list.retain(|device| device.scid != scid);
    }

    pub async fn add_device(device_id: String, scid: String, main: bool, socket_ids: Vec<String>) {
        let mut device_list = CONTROLLED_DEVICES.write().await;
        device_list.push(ScrcpyDevice::new(device_id, scid, main, socket_ids));
    }

    pub async fn update_device_name(scid: String, name: String) {
        let mut device_list = CONTROLLED_DEVICES.write().await;
        for device in device_list.iter_mut() {
            if device.scid == scid {
                device.name = name;
                return;
            }
        }
    }

    pub async fn update_device_size(scid: String, size: (u32, u32)) {
        let mut device_list = CONTROLLED_DEVICES.write().await;
        for device in device_list.iter_mut() {
            if device.scid == scid {
                device.device_size = size;
                return;
            }
        }
    }

    // Blocking variants for use from sync contexts (e.g. Bevy systems)

    pub fn get_main_device_blocking() -> Option<ScrcpyDevice> {
        let device_list = CONTROLLED_DEVICES.blocking_read();
        device_list.iter().find(|device| device.main).cloned()
    }
}

#[derive(Clone, Serialize)]
pub struct UpdateInfo {
    pub has_update: bool,
    pub latest_version: String,
    pub current_version: String,
    pub title: String,
    pub body: String,
    pub time: String,
}

impl UpdateInfo {
    pub async fn get() -> UpdateInfo {
        UPDATE_INFO.read().await.clone()
    }

    pub async fn set(info: UpdateInfo) {
        *UPDATE_INFO.write().await = info;
    }
}

static UPDATE_INFO: Lazy<RwLock<UpdateInfo>> = Lazy::new(|| {
    RwLock::new(UpdateInfo {
        has_update: false,
        latest_version: "Unknown".to_string(),
        current_version: "Unknown".to_string(),
        title: "Unknown".to_string(),
        body: "Unknown".to_string(),
        time: "".to_string(),
    })
});
