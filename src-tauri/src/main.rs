// release 版不弹控制台窗口（该属性必须放在 bin crate 根，即本文件）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// 把启动错误/崩溃信息写到 %APPDATA%\Vidgo\crash.log（macOS: ~/Library/Application Support/Vidgo）
fn write_crash_log(msg: &str) {
    let dir = dirs::data_dir()
        .map(|d| d.join("Vidgo"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(dir.join("crash.log"), format!("[{}] {}\n", ts, msg));
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        write_crash_log(&format!("panic: {}", info));
    }));

    if let Err(e) = vidgo_lib::run() {
        write_crash_log(&format!("启动失败: {}", e));
    }
}
