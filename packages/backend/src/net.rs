//! 出网请求的公共防护。
//!
//! 封面、试听、下载这三条路径的目标地址都由远端决定（客户端元数据、provider 接口或第三方
//! 解析服务）。不校验就发请求，整台机器会变成一个可以被外部操纵的代理：攻击者拿到的是
//! 本机发起的请求，能穿过防火墙探测内网，甚至把内网响应带回给请求方（SSRF）。
//!
//! 这里提供三层防护，调用方按需组合：
//!   1. `validate_public_url` —— 请求前校验字面量地址并解析域名确认是公网地址；
//!   2. `public_redirect_policy` —— 重定向的每一跳重新校验，否则第 1 步会被 302 绕过；
//!   3. `PublicDnsResolver` —— 建连时只接受公网地址，堵住"校验时和连接时解析结果不同"的重绑定窗口。

use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use reqwest::{
    Url,
    dns::{Addrs, Name, Resolve, Resolving},
    redirect,
};
use url::Host;

/// 域名解析的超时上限。调用方可以传更小的值，这里只封顶。
const MAX_RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);
/// 重定向跳数上限。跳数越多，需要校验的面越大，也越容易绕过。
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, thiserror::Error)]
pub(crate) enum NetError {
    #[error("请求目标不是公网地址")]
    NotPublicAddress,
    #[error("解析目标域名失败: {0}")]
    Resolve(#[source] io::Error),
}

/// 校验目标地址：只允许 http/https，且主机必须位于公网。
///
/// 字面量地址直接判断；域名会解析一次，只要有任意一个地址不是公网地址就拒绝。
pub(crate) async fn validate_public_url(url: &Url, timeout: Duration) -> Result<(), NetError> {
    if !has_safe_url_destination(url) {
        return Err(NetError::NotPublicAddress);
    }

    let port = url
        .port_or_known_default()
        .ok_or(NetError::NotPublicAddress)?;
    let timeout = timeout.min(MAX_RESOLVE_TIMEOUT);

    match url.host().ok_or(NetError::NotPublicAddress)? {
        Host::Ipv4(address) => check_public(IpAddr::V4(address)),
        Host::Ipv6(address) => check_public(IpAddr::V6(address)),
        Host::Domain(host) => {
            let addresses = tokio::time::timeout(timeout, tokio::net::lookup_host((host, port)))
                .await
                .map_err(|_| {
                    NetError::Resolve(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "解析目标域名超时",
                    ))
                })?
                .map_err(NetError::Resolve)?
                .collect::<Vec<SocketAddr>>();

            if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
                return Err(NetError::NotPublicAddress);
            }
            Ok(())
        }
    }
}

fn check_public(address: IpAddr) -> Result<(), NetError> {
    if is_public_ip(address) {
        Ok(())
    } else {
        Err(NetError::NotPublicAddress)
    }
}

/// 重定向策略：限制跳数，并且每一跳都重新校验目标。
///
/// 这里只做字面量判断、不查 DNS —— 每多一跳多一次解析会让重定向链变成放大攻击面。
pub(crate) fn public_redirect_policy() -> redirect::Policy {
    redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.error("重定向次数过多")
        } else if has_safe_url_destination(attempt.url()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

fn has_safe_url_destination(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }

    match url.host() {
        Some(Host::Ipv4(address)) => is_public_ip(IpAddr::V4(address)),
        Some(Host::Ipv6(address)) => is_public_ip(IpAddr::V6(address)),
        Some(Host::Domain(host)) => !host.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = address.segments();
    // 只允许 2000::/3 全球单播，并排除文档地址 2001:db8::/32。
    (segments[0] & 0xe000) == 0x2000 && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
}

/// 只返回公网地址的 DNS 解析器。
///
/// 与 `validate_public_url` 的区别是它在建连时才生效，用来堵住重绑定窗口。
#[derive(Clone, Copy)]
pub(crate) struct PublicDnsResolver {
    timeout: Duration,
}

impl PublicDnsResolver {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self {
            timeout: timeout.min(MAX_RESOLVE_TIMEOUT),
        }
    }
}

impl Resolve for PublicDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let timeout = self.timeout;
        Box::pin(async move {
            let addresses = tokio::time::timeout(timeout, tokio::net::lookup_host((host, 0)))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "解析域名超时"))??;
            let addresses = addresses
                .filter(|address| is_public_ip(address.ip()))
                .collect::<Vec<_>>();
            if addresses.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "目标域名没有可用的公网地址",
                )
                .into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{is_public_ip, validate_public_url};
    use std::time::Duration;

    #[test]
    fn rejects_non_public_addresses() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.168.0.1",
            "224.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(address.parse().unwrap()), "{address}");
        }
        for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(is_public_ip(address.parse().unwrap()), "{address}");
        }
    }

    #[test]
    fn accepts_only_safe_url_destinations() {
        // 域名是否解析到公网要等真正请求时才知道；这里只判断字面量。
        for value in [
            "https://example.com/cover.jpg",
            "http://8.8.8.8/audio.flac",
            "https://[2606:4700:4700::1111]/audio.flac",
        ] {
            assert!(
                super::has_safe_url_destination(&value.parse().unwrap()),
                "{value}"
            );
        }
        for value in [
            "http://localhost/x",
            "http://LOCALHOST/x",
            "http://127.0.0.1/x",
            "http://[fc00::1]/x",
            "file:///etc/passwd",
            "ftp://example.com/x",
            "http://user@example.com/x",
        ] {
            assert!(
                !super::has_safe_url_destination(&value.parse().unwrap()),
                "{value}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_private_literal_urls_before_requesting_them() {
        for value in [
            "http://127.0.0.1/cover.jpg",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/cover.jpg",
            "http://user:pass@example.com/cover.jpg",
            "file:///etc/passwd",
        ] {
            let url = value.parse().unwrap();
            assert!(matches!(
                validate_public_url(&url, Duration::from_secs(1)).await,
                Err(super::NetError::NotPublicAddress)
            ));
        }
    }
}
