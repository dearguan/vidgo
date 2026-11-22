use std::time::Duration;

/// 统一创建 HTTP 客户端：尊重用户代理设置，失败时回退为直连。
pub fn client(proxy: &str, timeout: Duration) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("Vidgo/", env!("CARGO_PKG_VERSION")))
        .timeout(timeout);
    let proxy = proxy.trim();
    if !proxy.is_empty() {
        if let Ok(parsed) = reqwest::Proxy::all(proxy) {
            builder = builder.proxy(parsed);
        }
    }
    builder.build().map_err(|e| format!("创建网络客户端失败：{e}"))
}

/// 隐藏代理地址中的账号密码，避免写进上报内容。
pub fn mask_proxy(proxy: &str) -> String {
    let proxy = proxy.trim();
    if proxy.is_empty() {
        return "未设置".into();
    }
    let Some(scheme_end) = proxy.find("://") else {
        return proxy.to_string();
    };
    let rest = &proxy[scheme_end + 3..];
    let host_start = rest.find('@').map(|i| i + 1).unwrap_or(0);
    format!("{}://{}", &proxy[..scheme_end], &rest[host_start..])
}
