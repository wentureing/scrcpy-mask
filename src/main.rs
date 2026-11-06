use std::{
    fs::File,
    net::{Ipv4Addr, SocketAddrV4},
    sync::OnceLock,
};

use bevy::{
    log::{BoxedLayer, LogPlugin, tracing_subscriber::Layer},
    prelude::*,
    window::{PresentMode, WindowLevel},
};
use scrcpy_mask::{
    DEFAULT_LANGUAGE,
    config::LocalConfig,
    is_available_language,
    mask::{MaskPlugins, mask_command::MaskCommand},
    scrcpy::{
        control_msg::ScrcpyControlMsg,
        controller::{self, ControllerCommand},
    },
    tokio_tasks::{TokioTasksPlugin, TokioTasksRuntime},
    utils::{
        ChannelReceiverM, ChannelReceiverV, ChannelSenderCS, ChannelSenderD, ChannelSenderWS,
        LatestVideoFrame, check_for_update, relate_to_data_path,
    },
    web::{self, ws::WebSocketNotification},
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing_appender::non_blocking::WorkerGuard;

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

fn log_custom_layer(_app: &mut App) -> Option<BoxedLayer> {
    let file = File::create(relate_to_data_path(["app.log"])).unwrap_or_else(|e| {
        panic!("Failed to create log file: {}", e);
    });
    let (non_blocking, guard) = tracing_appender::non_blocking(file);
    let _ = LOG_GUARD.set(guard);
    Some(
        bevy::log::tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_file(false)
            .with_line_number(true)
            .with_ansi(false)
            .boxed(),
    )
}

fn main() {
    rust_i18n::set_locale(DEFAULT_LANGUAGE);

    if let Err(e) = LocalConfig::load() {
        println!("LocalConfig load failed. {}", e);
    }
    LocalConfig::prefer_bundled_adb();

    let mut local_config = LocalConfig::get();
    // update language
    let language = local_config.language;
    if is_available_language(&language) {
        rust_i18n::set_locale(&language);
    } else {
        rust_i18n::set_locale(DEFAULT_LANGUAGE);
        LocalConfig::set_language(DEFAULT_LANGUAGE.to_string());
        local_config = LocalConfig::get();
    }
    // update config file
    LocalConfig::save().unwrap();

    ffmpeg_next::init().unwrap();

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(LogPlugin {
                custom_layer: log_custom_layer,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "scrcpy-mask".into(),
                    has_shadow: false,
                    transparent: true, // for windows: https://github.com/bevyengine/bevy/issues/7544
                    decorations: false,
                    present_mode: PresentMode::AutoVsync,
                    resizable: true,
                    visible: false,
                    focused: false,
                    window_level: if local_config.always_on_top {
                        WindowLevel::AlwaysOnTop
                    } else {
                        WindowLevel::Normal
                    },
                    #[cfg(target_os = "macos")]
                    composite_alpha_mode: bevy::window::CompositeAlphaMode::PostMultiplied,
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(TokioTasksPlugin::default())
    .add_plugins(MaskPlugins)
    .add_systems(Startup, (start_servers, check_for_update_system));

    #[cfg(target_os = "macos")]
    {
        app.insert_resource(bevy::ecs::schedule::MainThreadExecutor::default())
            .add_systems(Startup, macos_menu);
    }

    #[cfg(not(target_os = "macos"))]
    {
        use scrcpy_mask::window_alpha;
        app.add_systems(Startup, window_alpha::detect_alpha_mode);
        app.add_systems(PostStartup, window_alpha::apply_alpha_mode);
    }

    // 启动时记录 adb 归属，便于排查"退出时为什么没清理 adb"（docs/22 §22.19 D26/D27）。
    log_adb_owner();

    app.run();

    // 退出清理：先删 reverse（需要 adb server），再按策略决定是否停掉 adb server。
    // 注意：任务管理器强杀进程时不会执行到这里，这是已知限制。
    let config = LocalConfig::get();
    cleanup_adb_reverse();
    cleanup_adb_server(config.adb_cleanup_policy, &config.adb_path);
}

/// 启动时记录 adb 归属（只记日志，不阻塞启动流程）。
fn log_adb_owner() {
    let owner = scrcpy_mask::scrcpy::adb_owner::detect(5037);
    log::info!("[Startup] adb owner: {}", owner.describe());
}

/// 按策略清理 adb server（`docs/22` §22.19 D26/D27）。
///
/// - `Auto`：只有检测到监听 5037 的是**本程序自带的 adb** 时才 `kill-server`；
/// - `Always`：一律 `kill-server`；
/// - `Never`：不动。
///
/// 非 Windows 平台归属判定为 `Unsupported`，`Auto` 下不会误杀。
fn cleanup_adb_server(policy: scrcpy_mask::config::AdbCleanupPolicy, adb_path: &str) {
    use scrcpy_mask::scrcpy::adb_owner::{self, AdbOwner};

    const ADB_SERVER_PORT: u16 = 5037;
    let owner = adb_owner::detect(ADB_SERVER_PORT);
    log::info!("[Teardown] adb owner: {}", owner.describe());

    if !adb_owner::should_kill(policy, &owner) {
        if matches!(owner, AdbOwner::External { .. }) {
            log::info!("[Teardown] 外部 adb 未清理（策略 Auto，可用设置改为 always）");
        }
        return;
    }

    match std::process::Command::new(adb_path)
        .arg("kill-server")
        .output()
    {
        Ok(out) if out.status.success() => log::info!("[Teardown] adb server stopped"),
        Ok(out) => log::warn!(
            "[Teardown] adb kill-server failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(e) => log::warn!("[Teardown] cannot run adb ({}): {e}", adb_path),
    }
}

/// 遍历当前受控设备，逐条移除 `adb reverse` 规则。
fn cleanup_adb_reverse() {
    use scrcpy_mask::{scrcpy::adb::Device, utils::share::ControlledDevice};

    let devices = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(ControlledDevice::get_device_list()),
        Err(e) => {
            eprintln!("[Teardown] cannot build runtime: {e}");
            return;
        }
    };
    if devices.is_empty() {
        return;
    }
    log::info!("[Teardown] cleaning {} adb reverse rule(s)", devices.len());

    for device in devices {
        let remote = format!("localabstract:scrcpy_{}", device.scid);
        match Device::remove_reverse(&device.device_id, &remote) {
            Ok(()) => log::info!("[Teardown] removed adb reverse {remote}"),
            Err(e) => log::warn!("[Teardown] {}", e),
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_menu(executor: Res<bevy::ecs::schedule::MainThreadExecutor>) {
    use muda::{Menu, Submenu};
    // remove default menu
    executor
        .0
        .spawn(async move {
            let menu = Menu::new();
            let submenu = Submenu::new("scrcpy-mask", true);
            menu.append(&submenu).unwrap();
            menu.init_for_nsapp();
        })
        .detach();
}

fn start_servers(mut commands: Commands) {
    let config = LocalConfig::get();
    let web_addr = SocketAddrV4::new(config.web_bind_addr, config.web_port);
    let controller_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, config.controller_port);

    let (cs_tx, _) = broadcast::channel::<ScrcpyControlMsg>(1000);
    let (ws_tx, _) = broadcast::channel::<WebSocketNotification>(1000);
    let v_channel = LatestVideoFrame::default();
    let (m_tx, m_rx) =
        crossbeam_channel::unbounded::<(MaskCommand, oneshot::Sender<Result<String, String>>)>();
    let (d_tx, d_rx) = mpsc::unbounded_channel::<ControllerCommand>();

    commands.insert_resource(ChannelSenderCS(cs_tx.clone()));
    commands.insert_resource(ChannelReceiverV(v_channel.clone()));
    commands.insert_resource(ChannelReceiverM(m_rx));
    commands.insert_resource(ChannelSenderD(d_tx.clone()));
    commands.insert_resource(ChannelSenderWS(ws_tx.clone()));
    web::Server::start(web_addr, cs_tx.clone(), d_tx, m_tx.clone(), ws_tx.clone());
    controller::Controller::start(controller_addr, cs_tx, v_channel, d_rx, m_tx, ws_tx);
}

fn check_for_update_system(runtime: ResMut<TokioTasksRuntime>) {
    runtime.spawn_background_task(move |_ctx| async move {
        if let Err(e) = check_for_update().await {
            log::error!("{}", e);
        }
    });
}
