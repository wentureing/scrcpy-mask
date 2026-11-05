use std::{collections::HashMap, net::SocketAddrV4, thread, time::Duration};

use bevy::log;
use copypasta::{ClipboardContext, ClipboardProvider};
use rust_i18n::t;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{
        broadcast,
        mpsc::{self, UnboundedReceiver},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::LocalConfig,
    mask::mask_command::MaskCommand,
    scrcpy::{
        adb::Device,
        connection::ScrcpyConnection,
        control_msg::{ScrcpyControlMsg, ScrcpyDeviceMsg},
    },
    utils::{
        LatestVideoFrame, mask_win_move_helper,
        share::{ControlledDevice, PortNotice},
        socket::{bind_with_candidates, disable_inheritance},
    },
    web::ws::WebSocketNotification,
};

#[derive(Debug)]
pub enum ControllerCommand {
    ConnectMainControl(String, bool),
    ConnectMainVideo(String, bool),
    ConnectMainAudio(String, bool),
    ConnectSubControl(String),
    ShutdownMain(String),
    ShutdownSub(String),
}

pub struct Controller;

/// 等待设备端 scrcpy server 连回来的超时时间。
///
/// 取值来自 `LocalConfig::connect_timeout_secs`（默认 10 秒），夹到 1..=120 秒防止误配置。
fn connect_accept_timeout() -> Duration {
    let secs = LocalConfig::get().connect_timeout_secs.clamp(1, 120);
    Duration::from_secs(secs)
}

/// accept 的收敛结果：超时与 accept 错误都收敛成 `Failed`，让调用方走同一段回滚。
enum AcceptOutcome {
    Accepted(TcpStream),
    Failed(String),
}

/// 带超时的 accept。
///
/// 修复点：原实现直接 `listener.accept().await`。设备端因 jar 版本不符、`app_process`
/// 崩溃、USB 抖动等原因没连回来时，整个 controller 主循环会永久停在这里，
/// 之后所有连接/断开命令都在 channel 里排队，前端表现为"点了没反应、只能重启应用"。
async fn accept_with_timeout(listener: &TcpListener) -> AcceptOutcome {
    let timeout = connect_accept_timeout();
    match tokio::time::timeout(timeout, listener.accept()).await {
        Ok(Ok((socket, _))) => AcceptOutcome::Accepted(socket),
        Ok(Err(e)) => AcceptOutcome::Failed(format!("accept error: {e}")),
        Err(_elapsed) => AcceptOutcome::Failed(format!(
            "accept timed out after {}s",
            timeout.as_secs()
        )),
    }
}

/// 连接失败/超时的统一回滚：通知前端、清理受控记录、移除取消令牌。
///
/// `signal_map` 里的 token 是在 accept 之前插入的，这里必须成对移除，否则会泄漏。
/// 同时删掉该设备建立的 `adb reverse` 规则（P0-3）：这条路径原来只删记录，
/// 规则会一直留在设备上，下次连接在旧端口上"假死"。
async fn rollback_pending_connection(
    scid: &str,
    socket_id: &str,
    ws_tx: &broadcast::Sender<WebSocketNotification>,
    signal_map: &mut HashMap<String, CancellationToken>,
) {
    ws_tx
        .send(WebSocketNotification::ScrcpyDeviceConnection {
            scid: scid.to_string(),
            main: true,
            connected: false,
        })
        .ok();
    // 删记录之前先找出 device_id：remove_device 之后就没法反查了。
    let device_id = ControlledDevice::get_device_list()
        .await
        .into_iter()
        .find(|device| device.scid == scid)
        .map(|device| device.device_id);
    ControlledDevice::remove_device(scid).await;
    if let Some(device_id) = device_id {
        let scid = scid.to_string();
        // adb 调用是阻塞的，丢到 blocking 线程池；失败只记日志。
        tokio::task::spawn_blocking(move || {
            let remote = format!("localabstract:scrcpy_{}", scid);
            match Device::remove_reverse(&device_id, &remote) {
                Ok(()) => log::info!("[Controller] removed adb reverse {}", remote),
                Err(e) => log::warn!("[Controller] {}", e),
            }
        });
    }
    signal_map.remove(socket_id);
}

impl Controller {
    pub fn start(
        addr: SocketAddrV4,
        cs_tx: broadcast::Sender<ScrcpyControlMsg>,
        v_tx: LatestVideoFrame,
        d_rx: UnboundedReceiver<ControllerCommand>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        ws_tx: broadcast::Sender<WebSocketNotification>,
    ) {
        thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    Controller::run_server(addr, cs_tx, v_tx, d_rx, m_tx, ws_tx).await;
                });
        });
    }

    async fn cr_msg_handler(
        mut cr_rx: UnboundedReceiver<ScrcpyDeviceMsg>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        ws_tx: broadcast::Sender<WebSocketNotification>,
    ) {
        loop {
            match cr_rx.recv().await {
                Some(msg) => match msg {
                    ScrcpyDeviceMsg::Clipboard { length: _, text } => {
                        if LocalConfig::get_clipboard_sync() {
                            let mut ctx = ClipboardContext::new().unwrap();
                            match ctx.set_contents(text) {
                                Ok(()) => log::info!(
                                    "[Controller] {}",
                                    t!("scrcpy.syncClipboardFromMain")
                                ),
                                Err(e) => log::info!(
                                    "[Controller] {}: {}",
                                    t!("scrcpy.syncClipboardFromMain"),
                                    e
                                ),
                            }
                        }
                    }
                    ScrcpyDeviceMsg::AckClipboard { .. } => {}
                    ScrcpyDeviceMsg::UhidOutput { .. } => {}
                    ScrcpyDeviceMsg::Rotation {
                        rotation,
                        width,
                        height,
                        scid,
                    } => {
                        ws_tx
                            .send(WebSocketNotification::ScrcpyDeviceRotation {
                                rotation,
                                width,
                                height,
                                scid: scid.clone(),
                            })
                            .ok();
                        let msg = mask_win_move_helper(width, height, &m_tx).await;
                        log::info!(
                            "[Controller] {}. {}",
                            t!(
                                "scrcpy.deviceRotation",
                                scid => scid,
                                degree => rotation * 90,
                            ),
                            msg
                        );
                    }
                    ScrcpyDeviceMsg::Unknown => {
                        log::warn!("[Controller] {}", t!("scrcpy.unknownControlMsg"))
                    }
                },
                None => {
                    log::info!("[Controller] {}", t!("scrcpy.crChannelClosed"));
                    break;
                }
            }
        }
    }

    async fn run_server(
        addr: SocketAddrV4,
        cs_tx: broadcast::Sender<ScrcpyControlMsg>,
        v_tx: LatestVideoFrame,
        mut d_rx: UnboundedReceiver<ControllerCommand>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        ws_tx: broadcast::Sender<WebSocketNotification>,
    ) {
        log::info!("[Controller] {}: {}", t!("scrcpy.startingController"), addr);
        let desired_controller_port = addr.port();
        let (listener, addr) = match bind_with_candidates(addr, |port| {
            LocalConfig::set_controller_port_in_memory(port);
            if port != desired_controller_port {
                // 端口是"静默顺延"的，用户只会看到设备连不上，所以留一条提示给前端（P0-5）。
                PortNotice::set(format!(
                    "控制服务端口 {} 已被占用，本次改用 {}（端口只改内存、不写盘）。\
                     若设备连不上，请检查是否有旧的本程序进程还占着端口。",
                    desired_controller_port, port
                ));
            }
        })
        .await
        {
            Ok(listener) => listener,
            Err(e) => {
                log::error!("[Controller] Failed to bind {}: {}", addr, e);
                return;
            }
        };
        if let Err(e) = disable_inheritance(&listener) {
            log::error!(
                "[Controller] Failed to disable socket inheritance for {}: {}",
                addr,
                e
            );
            return;
        }

        // scrcpy device msg handler
        let (cr_tx, cr_rx) = mpsc::unbounded_channel::<ScrcpyDeviceMsg>();
        let m_tx_copy = m_tx.clone();
        let ws_tx_copy = ws_tx.clone();
        tokio::spawn(async move { Self::cr_msg_handler(cr_rx, m_tx_copy, ws_tx_copy).await });

        // receive command from web server to accept and shutdown scrcpy connection
        log::info!("[Controller] {}", t!("scrcpy.startReceiveCommand"));
        let mut signal_map: HashMap<String, CancellationToken> = HashMap::new();
        loop {
            match d_rx.recv().await {
                Some(cmd) => match cmd {
                    ControllerCommand::ConnectMainControl(scid, meta_flag) => {
                        let socket_id = "main_control".to_string();

                        if !ControlledDevice::is_scid_controlled(&scid).await {
                            log::error!("{}: {}", t!("scrcpy.deviceNotRecorded"), scid);
                            rollback_pending_connection(&scid, &socket_id, &ws_tx, &mut signal_map)
                                .await;
                            continue;
                        }

                        let token = CancellationToken::new();
                        signal_map.insert(socket_id.clone(), token.clone());

                        log::info!(
                            "[Controller] {}: {}",
                            t!("scrcpy.creatingMainControl"),
                            scid
                        );
                        let cs_rx = cs_tx.subscribe();
                        let cr_tx_copy = cr_tx.clone();
                        let m_tx_copy = m_tx.clone();
                        match accept_with_timeout(&listener).await {
                            AcceptOutcome::Accepted(socket) => {
                                let ws_tx_copy = ws_tx.clone();
                                let scid_copy = scid.clone();
                                ws_tx_copy
                                    .send(WebSocketNotification::ScrcpyDeviceConnection {
                                        scid: scid_copy.clone(),
                                        main: true,
                                        connected: true,
                                    })
                                    .ok();
                                tokio::spawn(async move {
                                    ScrcpyConnection::new(socket)
                                        .handle_control(
                                            cs_rx, cr_tx_copy, m_tx_copy, scid, true, token,
                                            meta_flag,
                                        )
                                        .await;
                                    ws_tx_copy
                                        .send(WebSocketNotification::ScrcpyDeviceConnection {
                                            scid: scid_copy,
                                            main: true,
                                            connected: false,
                                        })
                                        .ok();
                                });
                            }
                            AcceptOutcome::Failed(err) => {
                                log::error!(
                                    "[Controller] {}: {} ({})",
                                    t!("scrcpy.errorAcceptingConnection"),
                                    err,
                                    scid
                                );
                                rollback_pending_connection(
                                    &scid,
                                    &socket_id,
                                    &ws_tx,
                                    &mut signal_map,
                                )
                                .await;
                            }
                        }
                    }
                    ControllerCommand::ConnectMainVideo(scid, meta_flag) => {
                        let socket_id = "main_video".to_string();

                        if !ControlledDevice::is_scid_controlled(&scid).await {
                            log::error!("{}: {}", t!("scrcpy.deviceNotRecorded"), scid);
                            rollback_pending_connection(&scid, &socket_id, &ws_tx, &mut signal_map)
                                .await;
                            continue;
                        }

                        let token = CancellationToken::new();
                        signal_map.insert(socket_id.clone(), token.clone());

                        log::info!("[Controller] {}: {}", t!("scrcpy.creatingMainVideo"), scid);
                        let v_tx_copy = v_tx.clone();
                        match accept_with_timeout(&listener).await {
                            AcceptOutcome::Accepted(socket) => {
                                match tokio::runtime::Builder::new_current_thread()
                                    .enable_all()
                                    .build()
                                {
                                    Ok(rt) => {
                                        thread::spawn(move || {
                                            rt.block_on(async move {
                                                ScrcpyConnection::new(socket)
                                                    .handle_video(token, v_tx_copy, meta_flag, &scid)
                                                    .await;
                                            });
                                        });
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "[Controller] failed to build video runtime: {}",
                                            e
                                        );
                                        rollback_pending_connection(
                                            &scid,
                                            &socket_id,
                                            &ws_tx,
                                            &mut signal_map,
                                        )
                                        .await;
                                    }
                                }
                            }
                            AcceptOutcome::Failed(err) => {
                                log::error!(
                                    "[Controller] {}: {} ({})",
                                    t!("scrcpy.errorAcceptingConnection"),
                                    err,
                                    scid
                                );
                                rollback_pending_connection(
                                    &scid,
                                    &socket_id,
                                    &ws_tx,
                                    &mut signal_map,
                                )
                                .await;
                            }
                        }
                    }
                    ControllerCommand::ConnectMainAudio(scid, meta_flag) => {
                        let socket_id = "main_audio".to_string();

                        if !ControlledDevice::is_scid_controlled(&scid).await {
                            log::error!("{}: {}", t!("scrcpy.deviceNotRecorded"), scid);
                            rollback_pending_connection(&scid, &socket_id, &ws_tx, &mut signal_map)
                                .await;
                            continue;
                        }

                        let token = CancellationToken::new();
                        signal_map.insert(socket_id.clone(), token.clone());

                        log::info!("[Controller] Creating main audio connection: {}", scid);
                        match accept_with_timeout(&listener).await {
                            AcceptOutcome::Accepted(socket) => {
                                match tokio::runtime::Builder::new_current_thread()
                                    .enable_all()
                                    .build()
                                {
                                    Ok(rt) => {
                                        thread::spawn(move || {
                                            rt.block_on(async move {
                                                ScrcpyConnection::new(socket)
                                                    .handle_audio(token, meta_flag, &scid)
                                                    .await;
                                            });
                                        });
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "[Controller] failed to build audio runtime: {}",
                                            e
                                        );
                                        rollback_pending_connection(
                                            &scid,
                                            &socket_id,
                                            &ws_tx,
                                            &mut signal_map,
                                        )
                                        .await;
                                    }
                                }
                            }
                            AcceptOutcome::Failed(err) => {
                                log::error!(
                                    "[Controller] {}: {} ({})",
                                    t!("scrcpy.errorAcceptingConnection"),
                                    err,
                                    scid
                                );
                                rollback_pending_connection(
                                    &scid,
                                    &socket_id,
                                    &ws_tx,
                                    &mut signal_map,
                                )
                                .await;
                            }
                        }
                    }
                    ControllerCommand::ConnectSubControl(scid) => {
                        let socket_id = format!("sub_control_{}", scid);

                        if !ControlledDevice::is_scid_controlled(&scid).await {
                            log::error!("{}: {}", t!("scrcpy.deviceNotRecorded"), scid);
                            rollback_pending_connection(&scid, &socket_id, &ws_tx, &mut signal_map)
                                .await;
                            continue;
                        }

                        let token = CancellationToken::new();
                        signal_map.insert(socket_id.clone(), token.clone());

                        log::info!("[Controller] {}: {}", t!("scrcpy.creatingSubControl"), scid);
                        let sc_rx = cs_tx.subscribe();
                        let cr_tx_copy = cr_tx.clone();
                        let m_tx_copy = m_tx.clone();
                        match accept_with_timeout(&listener).await {
                            AcceptOutcome::Accepted(socket) => {
                                let ws_tx_copy = ws_tx.clone();
                                let scid_copy = scid.clone();
                                ws_tx_copy
                                    .send(WebSocketNotification::ScrcpyDeviceConnection {
                                        scid: scid_copy.clone(),
                                        main: true,
                                        connected: true,
                                    })
                                    .ok();
                                tokio::spawn(async move {
                                    ScrcpyConnection::new(socket)
                                        .handle_control(
                                            sc_rx, cr_tx_copy, m_tx_copy, scid, false, token, true,
                                        )
                                        .await;
                                    ws_tx_copy
                                        .send(WebSocketNotification::ScrcpyDeviceConnection {
                                            scid: scid_copy,
                                            main: true,
                                            connected: false,
                                        })
                                        .ok();
                                });
                            }
                            AcceptOutcome::Failed(err) => {
                                log::error!(
                                    "[Controller] {}: {} ({})",
                                    t!("scrcpy.errorAcceptingConnection"),
                                    err,
                                    scid
                                );
                                rollback_pending_connection(
                                    &scid,
                                    &socket_id,
                                    &ws_tx,
                                    &mut signal_map,
                                )
                                .await;
                            }
                        }
                    }
                    ControllerCommand::ShutdownMain(scid) => {
                        if !signal_map.contains_key("main_control") {
                            log::warn!("[Controller] {}", t!("scrcpy.mainConnectionNotExist"));
                        } else {
                            log::info!("[Controller] {}: {}", t!("scrcpy.shutdownMain"), scid);
                            for socket_id in ["main_control", "main_video", "main_audio"] {
                                if let Some(token) = signal_map.get(socket_id) {
                                    token.cancel();
                                    signal_map.remove(socket_id);
                                }
                            }
                            for token in signal_map.values() {
                                token.cancel();
                            }
                            signal_map.clear();
                        }
                    }
                    ControllerCommand::ShutdownSub(scid) => {
                        let socket_id = format!("sub_control_{}", scid);
                        if !signal_map.contains_key(&socket_id) {
                            log::warn!(
                                "[Controller] {}: {}",
                                t!("scrcpy.subConnectionNotExist"),
                                socket_id
                            );
                        } else {
                            log::info!("[Controller] {}: {}", t!("scrcpy.shutdownSub"), scid);
                            if let Some(token) = signal_map.get(&socket_id) {
                                token.cancel();
                                signal_map.remove(&socket_id);
                            }
                        }
                    }
                },
                None => {
                    log::info!("[Controller] {}", t!("scrcpy.dChannelClosed"));
                    break;
                }
            }
        }
        log::info!("[Controller] {}", t!("scrcpy.controllerStopped"));
    }
}
