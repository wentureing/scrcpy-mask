use std::{io, net::SocketAddrV4};

use tokio::net::TcpListener;

/// 端口候选数量：起点端口被占用时，依次尝试 +1 … +9。
pub const PORT_CANDIDATE_COUNT: u16 = 10;

/// 生成候选端口序列（从 `start` 开始、最多 `count` 个）。
///
/// 超过 65535 时饱和（会重复最后一个端口，由 bind 自然失败去重）。
pub fn port_candidates(start: u16, count: u16) -> Vec<u16> {
    (0..count)
        .map(|offset| start.saturating_add(offset))
        .collect()
}

/// 在 `addr.port() .. addr.port()+10` 范围内找一个可用端口。
///
/// 成功时回调实际端口（用于更新内存配置），返回监听器与实际地址。
/// 全部失败才返回错误，错误里带最后一条 bind 失败原因。
pub async fn bind_with_candidates<F: Fn(u16)>(
    addr: SocketAddrV4,
    on_bound: F,
) -> Result<(TcpListener, SocketAddrV4), String> {
    let mut last_error = String::new();
    for (offset, port) in port_candidates(addr.port(), PORT_CANDIDATE_COUNT)
        .into_iter()
        .enumerate()
    {
        let candidate = SocketAddrV4::new(*addr.ip(), port);
        match TcpListener::bind(candidate).await {
            Ok(listener) => {
                if offset > 0 {
                    log::warn!(
                        "[Net] port {} is occupied, using {} (candidate {}/{})",
                        addr.port(),
                        port,
                        offset + 1,
                        PORT_CANDIDATE_COUNT
                    );
                }
                on_bound(port);
                return Ok((listener, candidate));
            }
            Err(e) => {
                last_error = format!("{candidate}: {e}");
                log::warn!("[Net] cannot bind {}: {}", candidate, e);
            }
        }
    }
    Err(format!(
        "no free port in {}..={} (last error: {})",
        addr.port(),
        addr.port().saturating_add(PORT_CANDIDATE_COUNT - 1),
        last_error
    ))
}

/// Prevent long-lived child processes (notably the ADB server on Windows) from
/// keeping a server port open after scrcpy-mask exits.
#[cfg(windows)]
pub fn disable_inheritance<T: std::os::windows::io::AsRawSocket>(socket: &T) -> io::Result<()> {
    windows_handle::set_inheritable(socket.as_raw_socket(), false)
}

#[cfg(not(windows))]
pub fn disable_inheritance<T>(_socket: &T) -> io::Result<()> {
    Ok(())
}

#[cfg(windows)]
mod windows_handle {
    use std::{ffi::c_void, io, os::windows::io::RawSocket};

    const HANDLE_FLAG_INHERIT: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
    }

    pub(super) fn set_inheritable(socket: RawSocket, inheritable: bool) -> io::Result<()> {
        let flags = if inheritable { HANDLE_FLAG_INHERIT } else { 0 };
        let result =
            unsafe { SetHandleInformation(socket as *mut c_void, HANDLE_FLAG_INHERIT, flags) };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::{
        ffi::c_void,
        net::{Ipv4Addr, TcpListener},
        os::windows::io::AsRawSocket,
    };

    use super::*;

    const HANDLE_FLAG_INHERIT: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetHandleInformation(handle: *mut c_void, flags: *mut u32) -> i32;
    }

    #[test]
    fn clears_listener_inherit_flag() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let socket = listener.as_raw_socket();

        windows_handle::set_inheritable(socket, true).unwrap();
        disable_inheritance(&listener).unwrap();

        let mut flags = 0;
        let get_result = unsafe { GetHandleInformation(socket as *mut c_void, &mut flags) };
        assert_ne!(get_result, 0, "failed to read test socket flags");
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
    }
}

#[cfg(test)]
mod candidate_tests {
    use super::*;

    #[test]
    fn candidates_are_consecutive() {
        assert_eq!(
            port_candidates(27798, 10),
            (27798..27808).collect::<Vec<_>>()
        );
    }

    #[test]
    fn candidates_saturate_at_max_port() {
        assert_eq!(port_candidates(65535, 3), vec![65535, 65535, 65535]);
    }

    #[test]
    fn candidates_start_from_zero() {
        assert_eq!(port_candidates(0, 2), vec![0, 1]);
    }
}
