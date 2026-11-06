//! adb 归属判定：谁在监听 adb server 端口（默认 5037）。
//!
//! 背景（`docs/22` §22.2 用例 7）：端口冲突要提醒、避让，并调查是哪个程序启动了 adb；
//! 如果是 scrcpy-mask 自己启动的，退出时清理；外部 adb 由用户交互决定（`docs/22` §22.19 D27）。
//!
//! 实现只依赖 `std::process::Command`（`netstat` + PowerShell CIM），**不引入新 crate**。
//! 非 Windows 平台一律返回 `Unsupported`，只记日志、不做判定（安全降级）。

use std::path::Path;
use std::process::Command;

/// adb server 的归属。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbOwner {
    /// 本程序自带的 adb（`assets/platform-tools/adb.exe`）
    Bundled { pid: u32 },
    /// 外部 adb（用户自己装的 / 其它工具启动的）
    External { pid: u32, exe: Option<String> },
    /// 端口没人监听（adb 还没启动）
    NotFound,
    /// 当前平台不支持判定
    Unsupported,
}

impl AdbOwner {
    pub fn pid(&self) -> Option<u32> {
        match self {
            AdbOwner::Bundled { pid } | AdbOwner::External { pid, .. } => Some(*pid),
            AdbOwner::NotFound | AdbOwner::Unsupported => None,
        }
    }

    /// 中文描述，用于日志与前端提示。
    pub fn describe(&self) -> String {
        match self {
            AdbOwner::Bundled { pid } => format!("本程序自带 adb（pid {pid}）"),
            AdbOwner::External { pid, exe } => match exe {
                Some(exe) => format!("外部 adb（pid {pid}: {exe}）"),
                None => format!("外部 adb（pid {pid}）"),
            },
            AdbOwner::NotFound => "未发现 adb server".to_string(),
            AdbOwner::Unsupported => "当前平台不支持 adb 归属判定".to_string(),
        }
    }
}

/// 解析 `netstat -ano` 输出，找出监听指定端口的 PID。
///
/// 行格式：`TCP  0.0.0.0:5037  0.0.0.0:0  LISTENING  1234`（IPv6 为 `[::]:5037`）。
/// 中文系统同样输出 `LISTENING`，这里额外兼容"侦听"。
fn parse_netstat_listener(text: &str, port: u16) -> Option<u32> {
    let suffix = format!(":{port}");
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        if !cols[1].ends_with(&suffix) {
            continue;
        }
        let state = cols[3].to_ascii_uppercase();
        let listening = state == "LISTENING" || cols[3].contains("侦听");
        if !listening {
            continue;
        }
        if let Ok(pid) = cols[4].parse::<u32>() {
            return Some(pid);
        }
    }
    None
}

/// 归一化路径，便于比较（去掉 Windows 长路径前缀、统一小写）。
fn normalized(path: &str) -> String {
    let trimmed = path.trim().trim_start_matches("\\\\?\\");
    trimmed.replace('/', "\\").to_ascii_lowercase()
}

fn same_path(a: &str, b: &str) -> bool {
    let (na, nb) = (normalized(a), normalized(b));
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    if na == nb {
        return true;
    }
    // 一方是相对路径时，退化为"文件名 + 末级目录"比较
    match (Path::new(&na).file_name(), Path::new(&nb).file_name()) {
        (Some(fa), Some(fb)) => fa == fb && na.ends_with(&nb) || nb.ends_with(&na),
        _ => false,
    }
}

#[cfg(windows)]
fn listener_pid(port: u16) -> Option<u32> {
    let out = Command::new("netstat")
        .args(["-ano", "-p", "TCP"])
        .output()
        .ok()?;
    parse_netstat_listener(&String::from_utf8_lossy(&out.stdout), port)
}

#[cfg(windows)]
fn process_image_path(pid: u32) -> Option<String> {
    // 优先 CIM（wmic 在新版 Windows 已被移除）；失败则由调用方当作"路径未知"。
    let script = format!(
        "(Get-CimInstance Win32_Process -Filter \"ProcessId={pid}\" -ErrorAction SilentlyContinue).ExecutablePath"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

#[cfg(not(windows))]
fn listener_pid(_port: u16) -> Option<u32> {
    None
}

#[cfg(not(windows))]
fn process_image_path(_pid: u32) -> Option<String> {
    None
}

/// 本程序自带的 adb 路径（与 `config.rs` 里的 bundled 路径一致）。
fn bundled_adb_path() -> String {
    crate::utils::relate_to_root_path(["assets", "platform-tools", "adb.exe"])
        .to_string_lossy()
        .into_owned()
}

/// 判定 adb server 的归属。
pub fn detect(port: u16) -> AdbOwner {
    #[cfg(not(windows))]
    {
        let _ = port;
        AdbOwner::Unsupported
    }
    #[cfg(windows)]
    {
        let Some(pid) = listener_pid(port) else {
            return AdbOwner::NotFound;
        };
        let exe = process_image_path(pid);
        let bundled = bundled_adb_path();
        let is_bundled = exe
            .as_deref()
            .map(|p| same_path(p, &bundled))
            .unwrap_or(false);
        if is_bundled {
            AdbOwner::Bundled { pid }
        } else {
            AdbOwner::External { pid, exe }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipv4_listener() {
        let text = "  TCP    0.0.0.0:5037    0.0.0.0:0    LISTENING    4321\r\n";
        assert_eq!(parse_netstat_listener(text, 5037), Some(4321));
    }

    #[test]
    fn parses_ipv6_listener() {
        let text = "  TCP    [::]:5037    [::]:0    LISTENING    99\r\n";
        assert_eq!(parse_netstat_listener(text, 5037), Some(99));
    }

    #[test]
    fn ignores_other_ports_and_states() {
        let text = "  TCP    0.0.0.0:5000    0.0.0.0:0    LISTENING    11\r\n  TCP    0.0.0.0:5037    1.2.3.4:5555    ESTABLISHED    22\r\n";
        assert_eq!(parse_netstat_listener(text, 5037), None);
    }

    #[test]
    fn compares_bundled_path_ignoring_case_and_prefix() {
        let a = "C:\\App\\assets\\platform-tools\\adb.exe";
        let b = "\\\\?\\c:\\app\\assets\\platform-tools\\ADB.EXE";
        assert!(same_path(a, b));
        assert!(!same_path(a, "C:\\Other\\adb.exe"));
    }
}
