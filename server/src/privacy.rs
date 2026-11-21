//! Privacy: a live check of the relay proxy, and an audit of what can be
//! learned about where this service runs.
//!
//! Only what the service can measure is reported. Two things it cannot see
//! are listed as "not checked here" with the reason: the host's open ports
//! (the service sees only its own network namespace) and a probe of the
//! public address from outside (making one would reveal where it runs).

use std::io;
use std::time::Duration;

use nostr_sdk::Url;
use rmcp::schemars::{self, JsonSchema};
use serde::Serialize;

use crate::config::is_loopback_host;
use crate::relay::{ProxyKind, ProxySpec, ProxyStatus, ProxyTransport};
use crate::service::{is_plain_text, RelayState};

/// How long one relay may take to answer through the proxy.
#[cfg(not(test))]
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
pub const CHECK_TIMEOUT: Duration = Duration::from_millis(500);
/// A connection slower than this is reported as slow.
pub const SLOW_MS: u64 = 2_000;

/// One relay reached (or not) through the proxy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RelayCheck {
    pub relay: String,
    pub ok: bool,
    /// Machine-readable outcome: `SOCKS5 00`, `SOCKS5 04`, `HTTP 407`,
    /// `ECONNREFUSED`, `ETIMEDOUT`, `DNS`, `TLS`, ...
    pub code: String,
    /// What it means, in a sentence.
    pub reason: String,
    /// Time to open the tunnel, when it opened.
    pub ms: Option<u64>,
}

/// Result of `check_proxy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ProxyCheck {
    /// The proxy that was checked, password hidden; empty for none.
    pub proxy: String,
    /// `connected` (every relay), `partial`, `failed` or `direct` (no proxy).
    pub status: String,
    /// Unix seconds.
    pub checked_at: u64,
    pub relays: Vec<RelayCheck>,
}

/// `host` and `port` a relay URL connects to.
fn relay_target(relay: &str) -> Option<(String, u16)> {
    let url = Url::parse(relay).ok()?;
    let host = url
        .host_str()?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    Some((host, url.port_or_known_default().unwrap_or(443)))
}

/// Error text that is safe to show and log: one short printable line.
fn plain(text: &str) -> String {
    text.chars().filter(|c| is_plain_text(*c)).take(160).collect()
}

/// Turn a SOCKS5 failure into a code and a sentence.
fn socks(e: &tokio_socks::Error, proxy_addr: &str) -> (String, String) {
    use tokio_socks::Error as S;
    let (code, reason): (&str, String) = match e {
        S::Io(inner) => return classify(inner, proxy_addr),
        S::ProxyServerUnreachable => (
            "unreachable",
            format!("Could not connect to the proxy at {proxy_addr}."),
        ),
        S::NoAcceptableAuthMethods => (
            "SOCKS5 auth FF",
            "The proxy accepts none of the login methods offered: add a user name and password, or remove them.".into(),
        ),
        S::PasswordAuthFailure(c) => {
            return (
                format!("SOCKS5 auth {c:02X}"),
                "The proxy rejected the user name or password.".into(),
            )
        }
        S::GeneralSocksServerFailure => (
            "SOCKS5 01",
            "The proxy failed for its own reasons (general failure).".into(),
        ),
        S::ConnectionNotAllowedByRuleset => ("SOCKS5 02", "The proxy's rules do not allow this connection.".into()),
        S::NetworkUnreachable => ("SOCKS5 03", "The proxy has no route to the relay's network.".into()),
        S::HostUnreachable => (
            "SOCKS5 04",
            "The proxy could not reach the relay (host unreachable).".into(),
        ),
        S::ConnectionRefused => ("SOCKS5 05", "The relay refused the proxy's connection.".into()),
        S::TtlExpired => (
            "SOCKS5 06",
            "The connection to the relay timed out at the proxy.".into(),
        ),
        S::CommandNotSupported => ("SOCKS5 07", "The proxy does not support opening connections.".into()),
        S::AddressTypeNotSupported => (
            "SOCKS5 08",
            "The proxy does not accept this kind of relay address.".into(),
        ),
        _ => (
            "SOCKS5",
            "The proxy's reply is not valid SOCKS5. Is this a SOCKS5 proxy?".into(),
        ),
    };
    (code.into(), reason)
}

/// Turn an HTTP `CONNECT` refusal into a code and a sentence.
fn http(status: &str) -> (String, String) {
    let reason = match status {
        "403" => "The proxy forbids connections to this relay.".to_string(),
        "407" => "The proxy needs a user name and password: add them to the address.".to_string(),
        "502" => "The proxy could not reach the relay.".to_string(),
        "503" | "504" => "The proxy is busy or timed out reaching the relay.".to_string(),
        other => format!("The proxy answered {other} instead of opening a tunnel."),
    };
    (format!("HTTP {status}"), reason)
}

/// Turn a failure to open a tunnel into a code and a sentence.
pub fn classify(e: &io::Error, proxy_addr: &str) -> (String, String) {
    if let Some(s) = e.get_ref().and_then(|i| i.downcast_ref::<tokio_socks::Error>()) {
        return socks(s, proxy_addr);
    }
    if let Some(ProxyStatus(code)) = e.get_ref().and_then(|i| i.downcast_ref::<ProxyStatus>()) {
        return http(code);
    }
    let text = e.to_string();
    let (code, reason) = match e.kind() {
        io::ErrorKind::ConnectionRefused => (
            "ECONNREFUSED",
            format!("Nothing is listening at {proxy_addr}. Is the proxy running on that port?"),
        ),
        io::ErrorKind::TimedOut => (
            "ETIMEDOUT",
            "No answer in time. Check the address and any firewall in between.".into(),
        ),
        io::ErrorKind::UnexpectedEof => (
            "EOF",
            "The proxy closed the connection before the tunnel opened.".into(),
        ),
        io::ErrorKind::InvalidData if text.to_ascii_lowercase().contains("certificate") => (
            "TLS",
            "The proxy's certificate is not trusted by the public web roots.".into(),
        ),
        _ if text.contains("lookup") || text.contains("resolve") => {
            ("DNS", "The proxy's host name could not be resolved.".into())
        }
        _ => return ("error".into(), plain(&text)),
    };
    (code.into(), reason)
}

/// The sentence for a relay reached in `ms` milliseconds.
fn reached(ms: u64) -> String {
    let slow = if ms > SLOW_MS { ", but slowly" } else { "" };
    format!("Reached through the proxy{slow}.")
}

/// Open a tunnel to each relay through `proxy`, all at once.
pub async fn check(proxy: Option<&ProxySpec>, shown: &str, relays: &[String], now: u64) -> ProxyCheck {
    let Some(spec) = proxy else {
        return ProxyCheck {
            proxy: String::new(),
            status: "direct".into(),
            checked_at: now,
            relays: Vec::new(),
        };
    };
    let transport = ProxyTransport::new(spec.clone());
    let proxy_addr = format!("{}:{}", spec.host, spec.port);
    let one = |relay: &String| {
        let (transport, proxy_addr, relay) = (&transport, &proxy_addr, relay.clone());
        async move {
            let Some((host, port)) = relay_target(&relay) else {
                return RelayCheck {
                    relay,
                    ok: false,
                    code: "invalid".into(),
                    reason: "The relay address has no host.".into(),
                    ms: None,
                };
            };
            let started = std::time::Instant::now();
            let outcome = tokio::time::timeout(CHECK_TIMEOUT, transport.tunnel(&host, port))
                .await
                .unwrap_or_else(|_| Err(io::Error::from(io::ErrorKind::TimedOut)));
            let ms = started.elapsed().as_millis() as u64;
            let ok_code = if spec.kind == ProxyKind::Socks5 {
                "SOCKS5 00"
            } else {
                "HTTP 200"
            };
            match outcome {
                Ok(_) => RelayCheck {
                    relay,
                    ok: true,
                    code: ok_code.into(),
                    reason: reached(ms),
                    ms: Some(ms),
                },
                Err(e) => {
                    let (code, reason) = classify(&e, proxy_addr);
                    RelayCheck {
                        relay,
                        ok: false,
                        code,
                        reason,
                        ms: None,
                    }
                }
            }
        }
    };
    let results = futures_util::future::join_all(relays.iter().map(one)).await;
    let reached = results.iter().filter(|r| r.ok).count();
    let status = match reached {
        0 => "failed",
        n if n == results.len() => "connected",
        _ => "partial",
    };
    ProxyCheck {
        proxy: shown.to_string(),
        status: status.into(),
        checked_at: now,
        relays: results,
    }
}

/// Listening TCP sockets in the service's network namespace: the ports that
/// can be reached from outside it, and the loopback-only listeners (such as
/// Docker's embedded DNS resolver on `127.0.0.11`), which cannot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listening {
    /// Ports bound to an address other than loopback.
    pub open: Vec<u16>,
    /// `address:port` of listeners bound to a loopback address only.
    pub loopback: Vec<String>,
}

/// The loopback address in a `/proc/net/tcp{,6}` local address column, if it
/// is one: `127.0.0.0/8`, `::1`, or an IPv4-mapped `::ffff:127.x.x.x`. The
/// kernel prints each 32-bit word in host (little-endian) byte order.
fn loopback(hex: &str) -> Option<String> {
    let v4 = |word: &str| -> Option<String> {
        let n = u32::from_str_radix(word, 16).ok()?.swap_bytes();
        let ip = std::net::Ipv4Addr::from(n);
        ip.is_loopback().then(|| ip.to_string())
    };
    match hex.len() {
        8 => v4(hex),
        32 if hex == "00000000000000000000000001000000" => Some("[::1]".into()),
        32 if hex[..24].eq_ignore_ascii_case("0000000000000000FFFF0000") => v4(&hex[24..]),
        _ => None,
    }
}

/// Listening TCP sockets, from the text of `/proc/net/tcp` or `/proc/net/tcp6`.
pub fn listening_sockets(table: &str) -> Listening {
    let mut out = Listening::default();
    for line in table.lines().skip(1) {
        let mut cols = line.split_whitespace();
        let (Some(local), Some(state)) = (cols.nth(1), cols.nth(1)) else { continue };
        let Some((addr, port)) = local.rsplit_once(':') else { continue };
        let Ok(port) = u16::from_str_radix(port, 16) else { continue };
        if state != "0A" {
            continue;
        }
        match loopback(addr) {
            Some(ip) => out.loopback.push(format!("{ip}:{port}")),
            None => out.open.push(port),
        }
    }
    out.open.sort_unstable();
    out.open.dedup();
    out.loopback.sort();
    out.loopback.dedup();
    out
}

/// Listening TCP sockets in this process's network namespace, or `None`
/// where the kernel does not publish them (not Linux).
pub fn own_listening(dir: &std::path::Path) -> Option<Listening> {
    let v4 = std::fs::read_to_string(dir.join("tcp")).ok()?;
    let v6 = std::fs::read_to_string(dir.join("tcp6")).unwrap_or_default();
    let (a, b) = (listening_sockets(&v4), listening_sockets(&v6));
    let mut all = Listening {
        open: [a.open, b.open].concat(),
        loopback: [a.loopback, b.loopback].concat(),
    };
    all.open.sort_unstable();
    all.open.dedup();
    all.loopback.sort();
    all.loopback.dedup();
    Some(all)
}

/// Evidence lines for the listening check.
fn listening_evidence(l: &Listening) -> Vec<String> {
    let open = if l.open.is_empty() {
        "0".to_string()
    } else {
        l.open.iter().map(u16::to_string).collect::<Vec<_>>().join(", ")
    };
    let mut lines = vec![format!("listening TCP ports   {open}")];
    if !l.loopback.is_empty() {
        lines.push(format!("loopback only         {}", l.loopback.join(", ")));
    }
    lines
}

/// One finding of the privacy audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PrivacyCheck {
    pub id: String,
    /// `network`, `ports` or `exposure`.
    pub area: String,
    pub title: String,
    /// `pass`, `to_fix`, `fail`, `note` or `not_checked`.
    pub result: String,
    /// Points taken off the score.
    pub points: u32,
    pub summary: String,
    /// Facts the result is based on, one per line.
    pub evidence: Vec<String>,
    pub fix: Option<String>,
}

/// Result of `get_privacy_audit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PrivacyAudit {
    /// 100 minus the points of the checks that ran.
    pub score: u32,
    /// Checks that ran (everything but `not_checked`).
    pub checked: u32,
    pub total: u32,
    pub ran_at: u64,
    pub checks: Vec<PrivacyCheck>,
}

/// What the audit looks at.
pub struct AuditInputs<'a> {
    pub proxy: Option<&'a ProxySpec>,
    /// The proxy as shown (password hidden).
    pub proxy_shown: String,
    pub proxy_locked: bool,
    /// The operator says a transparent proxy covers this service.
    pub transparent: bool,
    /// How many proxies are listed.
    pub configured: usize,
    /// What the service does while none is reachable.
    pub offline: crate::settings::OfflinePolicy,
    pub relays: Vec<RelayState>,
    pub last_check: Option<ProxyCheck>,
    /// The Electrum server, `host:port`.
    pub electrum: String,
    pub electrum_plain: bool,
    pub electrum_transport: String,
    pub announce: bool,
    pub listening: Option<Listening>,
    pub now: u64,
}

/// Whether a host is on this machine or a private network.
pub fn is_private_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    if is_loopback_host(&host) {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            ip.is_private() || ip.is_link_local() || ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64
        }
        Ok(std::net::IpAddr::V6(ip)) => (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80,
        Err(_) => {
            !host.contains('.')
                || [".local", ".lan", ".home.arpa", ".internal"]
                    .iter()
                    .any(|s| host.ends_with(s))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finding(
    id: &str,
    area: &str,
    title: &str,
    result: &str,
    points: u32,
    summary: impl Into<String>,
    evidence: Vec<String>,
    fix: Option<&str>,
) -> PrivacyCheck {
    PrivacyCheck {
        id: id.into(),
        area: area.into(),
        title: title.into(),
        result: result.into(),
        points,
        summary: summary.into(),
        evidence,
        fix: fix.map(str::to_string),
    }
}

/// Score what can be measured; list what cannot.
pub fn audit(i: AuditInputs<'_>) -> PrivacyAudit {
    let mut checks = Vec::new();
    let connected = i.relays.iter().filter(|r| r.connected).count();
    let relays_line = format!("relays connected   {connected} of {}", i.relays.len());

    match i.proxy {
        // The operator vouches for the network path; nothing here can see it.
        _ if i.transparent => checks.push(finding(
            "relays_via_proxy",
            "network",
            "A transparent proxy is declared",
            "not_checked",
            0,
            "The operator states that a transparent proxy carries this service's traffic. It is not checked, \
             and the proxy list is not used.",
            vec!["transparent proxy   declared by the operator".into(), relays_line],
            None,
        )),
        None if i.configured > 0 => checks.push(finding(
            "relays_via_proxy",
            "network",
            "No proxy is reachable",
            "fail",
            30,
            match i.offline {
                crate::settings::OfflinePolicy::KillSwitch => {
                    "Every proxy is down, so the kill switch has taken the service offline."
                }
                _ => "Every proxy is down, so relays connect straight to this machine and see its IP address.",
            },
            vec![
                format!("proxies configured   {}", i.configured),
                format!(
                    "when offline   {}",
                    match i.offline {
                        crate::settings::OfflinePolicy::KillSwitch => "kill switch",
                        crate::settings::OfflinePolicy::AdminOnly => "operators only",
                        crate::settings::OfflinePolicy::InvoicingOnly => "invoicing only",
                        crate::settings::OfflinePolicy::Open => "carry on",
                    }
                ),
                relays_line,
            ],
            Some("Bring a proxy back, or check each with Test in Settings."),
        )),
        None => checks.push(finding(
            "relays_via_proxy",
            "network",
            "Relays see this machine's IP address",
            "fail",
            30,
            "There is no proxy: every relay connects straight to this machine.",
            vec!["proxy   none".into(), relays_line],
            Some("Set a proxy for relay connections, for example Tor at socks5://127.0.0.1:9050."),
        )),
        Some(spec) => {
            if connected > 0 {
                checks.push(finding(
                    "relays_via_proxy",
                    "network",
                    "Your IP is hidden from relays",
                    "pass",
                    0,
                    "Relay connections go through the proxy, so relays see the proxy's address.",
                    vec![format!("proxy   {}", i.proxy_shown), relays_line],
                    None,
                ));
            } else {
                checks.push(finding(
                    "relays_via_proxy",
                    "network",
                    "No relay is connected through the proxy",
                    "fail",
                    30,
                    "The service cannot be reached until a relay connects through the proxy.",
                    vec![format!("proxy   {}", i.proxy_shown), relays_line],
                    Some("Check the proxy with Test in Settings."),
                ));
            }
            checks.push(if i.proxy_locked {
                finding(
                    "proxy_locked",
                    "network",
                    "The proxy can only be changed on the host",
                    "pass",
                    0,
                    "A stolen operator key cannot point the service at a proxy that learns its IP.",
                    vec!["DEADHOP_PROXY_LOCKED   true".into()],
                    None,
                )
            } else {
                finding(
                    "proxy_locked",
                    "network",
                    "The proxy can be changed remotely",
                    "to_fix",
                    10,
                    "A stolen operator key could set a proxy it controls and learn this machine's IP.",
                    vec!["DEADHOP_PROXY_LOCKED   false".into()],
                    Some("Set DEADHOP_PROXY_LOCKED=true on the host."),
                )
            });
            checks.push(finding(
                "names_at_proxy",
                "network",
                "Relay names are resolved by the proxy",
                "pass",
                0,
                "No DNS lookups for relays leave this machine.",
                vec![format!(
                    "{}   the relay's name is sent to the proxy",
                    if spec.kind == ProxyKind::Socks5 {
                        "socks5"
                    } else {
                        "http CONNECT"
                    }
                )],
                None,
            ));
            checks.push(match &i.last_check {
                None => finding(
                    "proxy_check",
                    "network",
                    "The proxy has not been tested yet",
                    "not_checked",
                    0,
                    "Run Test next to the proxy in Settings.",
                    Vec::new(),
                    None,
                ),
                Some(c) => {
                    let evidence = c.relays.iter().map(|r| format!("{}   {}", r.relay, r.code)).collect();
                    match c.status.as_str() {
                        "connected" => finding(
                            "proxy_check",
                            "network",
                            "Every relay is reachable through the proxy",
                            "pass",
                            0,
                            "The last test reached all relays.",
                            evidence,
                            None,
                        ),
                        "partial" => finding(
                            "proxy_check",
                            "network",
                            "Some relays are not reachable through the proxy",
                            "to_fix",
                            5,
                            "The last test reached only some relays.",
                            evidence,
                            Some("Remove the relays the proxy cannot reach, or fix the proxy."),
                        ),
                        _ => finding(
                            "proxy_check",
                            "network",
                            "No relay was reachable through the proxy",
                            "fail",
                            20,
                            "The last test reached no relay.",
                            evidence,
                            Some("Check the proxy with Test in Settings."),
                        ),
                    }
                }
            });
        }
    }

    let electrum_host = i.electrum.rsplit_once(':').map_or(i.electrum.as_str(), |(h, _)| h);
    let private = is_private_host(electrum_host);
    checks.push(if private {
        finding(
            "electrum_network",
            "network",
            "Electrum stays on your network",
            "pass",
            0,
            "The chain backend is on this machine or a private network; it does not use the proxy.",
            vec![format!("electrum   {}", i.electrum)],
            None,
        )
    } else {
        finding(
            "electrum_network",
            "network",
            "Electrum is outside your network",
            "to_fix",
            10,
            "The Electrum server sees this machine's IP and which addresses it watches.",
            vec![format!("electrum   {}", i.electrum)],
            Some("Run Fulcrum or electrs on your own node and point DEADHOP_ELECTRUM at it."),
        )
    });
    checks.push(
        if i.electrum_plain && !is_loopback_host(&electrum_host.to_ascii_lowercase()) {
            finding(
                "electrum_link",
                "network",
                "The Electrum link is not encrypted",
                "to_fix",
                10,
                "Anyone on the network path can read which addresses the service watches.",
                vec![format!("transport   {}", i.electrum_transport)],
                Some("Use the server's TLS port (tls://) and pin its certificate."),
            )
        } else {
            finding(
                "electrum_link",
                "network",
                "The Electrum link is protected",
                "pass",
                0,
                "TLS, or a server on this machine.",
                vec![format!("transport   {}", i.electrum_transport)],
                None,
            )
        },
    );

    checks.push(match &i.listening {
        None => finding(
            "service_ports",
            "ports",
            "Listening sockets were not checked",
            "not_checked",
            0,
            "This system does not publish /proc/net/tcp.",
            Vec::new(),
            None,
        ),
        Some(l) if l.open.is_empty() => finding(
            "service_ports",
            "ports",
            "deadhop listens on nothing",
            "pass",
            0,
            if l.loopback.is_empty() {
                "No TCP port is open in the service's network namespace; it only dials out."
            } else {
                "Nothing in the service's network namespace can be reached from outside it; it only dials out. Loopback-only listeners, such as Docker's DNS resolver on 127.0.0.11, are listed for completeness."
            },
            listening_evidence(l),
            None,
        ),
        Some(l) => finding(
            "service_ports",
            "ports",
            "Ports are open next to the service",
            "to_fix",
            10,
            "Something in the service's network namespace accepts connections from outside it.",
            listening_evidence(l),
            Some("Run the service in its own container or network namespace, without other listeners."),
        ),
    });
    checks.push(finding(
        "host_ports",
        "ports",
        "The host's open ports",
        "not_checked",
        0,
        "The service sees only its own network namespace, not the host's.",
        Vec::new(),
        Some("On the host, list them with `ss -tlnp`, or check your firewall."),
    ));
    checks.push(finding(
        "internet_probe",
        "ports",
        "Ports reachable from the internet",
        "not_checked",
        0,
        "Probing this machine's public address from outside would itself reveal where the service runs.",
        Vec::new(),
        Some("Scan your public address from another network you control."),
    ));

    checks.push(if i.announce {
        finding(
            "announce",
            "exposure",
            "The service announces itself",
            "to_fix",
            5,
            "Public discovery events make the service easy to find and watch.",
            vec!["DEADHOP_ANNOUNCE   true".into()],
            Some("Set DEADHOP_ANNOUNCE=false unless you want to be listed."),
        )
    } else {
        finding(
            "announce",
            "exposure",
            "No public announcements",
            "pass",
            0,
            "The service publishes no discovery events.",
            vec!["DEADHOP_ANNOUNCE   false".into()],
            None,
        )
    });

    let lost: u32 = checks.iter().map(|c| c.points).sum();
    let checked = checks.iter().filter(|c| c.result != "not_checked").count() as u32;
    PrivacyAudit {
        score: 100u32.saturating_sub(lost),
        checked,
        total: checks.len() as u32,
        ran_at: i.now,
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::parse_proxy;
    use crate::relay::tests::{connect_proxy, socks5_proxy};
    use tokio::net::TcpListener;

    fn code(e: io::Error) -> (String, String) {
        classify(&e, "127.0.0.1:9050")
    }

    #[test]
    fn every_failure_becomes_a_code_and_a_sentence() {
        use tokio_socks::Error as S;
        let socks = |e: S| code(io::Error::other(e)).0;
        for (e, want) in [
            (S::ProxyServerUnreachable, "unreachable"),
            (S::NoAcceptableAuthMethods, "SOCKS5 auth FF"),
            (S::PasswordAuthFailure(1), "SOCKS5 auth 01"),
            (S::GeneralSocksServerFailure, "SOCKS5 01"),
            (S::ConnectionNotAllowedByRuleset, "SOCKS5 02"),
            (S::NetworkUnreachable, "SOCKS5 03"),
            (S::HostUnreachable, "SOCKS5 04"),
            (S::ConnectionRefused, "SOCKS5 05"),
            (S::TtlExpired, "SOCKS5 06"),
            (S::CommandNotSupported, "SOCKS5 07"),
            (S::AddressTypeNotSupported, "SOCKS5 08"),
            (S::UnknownError, "SOCKS5"),
            (S::Io(io::Error::from(io::ErrorKind::ConnectionRefused)), "ECONNREFUSED"),
        ] {
            assert_eq!(socks(e), want);
        }
        assert!(code(io::Error::other(S::PasswordAuthFailure(1))).1.contains("password"));
        assert!(code(io::Error::other(S::ProxyServerUnreachable))
            .1
            .contains("127.0.0.1:9050"));

        let http = |status: &str| {
            code(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                ProxyStatus(status.into()),
            ))
        };
        for (status, words) in [
            ("403", "forbids"),
            ("407", "user name and password"),
            ("502", "could not reach"),
            ("503", "busy"),
            ("418", "answered 418"),
        ] {
            let (c, reason) = http(status);
            assert_eq!(c, format!("HTTP {status}"));
            assert!(reason.contains(words), "{reason}");
        }

        let (c, reason) = code(io::Error::from(io::ErrorKind::ConnectionRefused));
        assert_eq!(c, "ECONNREFUSED");
        assert!(reason.contains("127.0.0.1:9050"));
        assert_eq!(code(io::Error::from(io::ErrorKind::TimedOut)).0, "ETIMEDOUT");
        assert_eq!(code(io::Error::from(io::ErrorKind::UnexpectedEof)).0, "EOF");
        assert_eq!(
            code(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid peer certificate: UnknownIssuer"
            ))
            .0,
            "TLS"
        );
        assert_eq!(code(io::Error::other("failed to lookup address information")).0, "DNS");
        assert_eq!(code(io::Error::other("could not resolve host")).0, "DNS");
        // Anything else is shown as itself, on one printable line.
        let (c, reason) = code(io::Error::new(io::ErrorKind::InvalidData, "odd\nreply\u{202e}"));
        assert_eq!((c.as_str(), reason.as_str()), ("error", "oddreply"));
    }

    #[test]
    fn slow_relays_are_called_slow() {
        assert_eq!(reached(40), "Reached through the proxy.");
        assert_eq!(reached(SLOW_MS + 1), "Reached through the proxy, but slowly.");
    }

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 1 1 0 100 0 0 10 0
   1: 0100007F:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 2 1 0 100 0 0 10 0
   2: 0100007F:C350 0100007F:01BB 01 00000000:00000000 00:00000000 00000000     0        0 3 1 0 100 0 0 10 0
   3: garbage
   4: 0B00007F:95D5 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 4 1 0 100 0 0 10 0
   5: 0100007F:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 5 1 0 100 0 0 10 0
   6: 00000000:ZZZZ 00000000:0000 0A 0
   7: 00000000 00000000:0000 0A 0
";
    const TCP6: &str = "  sl  local_address                         remote_address                        st
   0: 00000000000000000000000000000000:1F90 00000000000000000000000000000000:0000 0A 0
   1: 00000000000000000000000000000000:01BB 00000000000000000000000000000000:0000 0A 0
   2: 00000000000000000000000001000000:0277 00000000000000000000000000000000:0000 0A 0
   3: 0000000000000000FFFF00000500007F:0035 00000000000000000000000000000000:0000 0A 0
   4: 0000000000000000FFFF00000A00000A:0050 00000000000000000000000000000000:0000 0A 0
";

    fn listening(open: &[u16], loopback: &[&str]) -> Listening {
        Listening {
            open: open.to_vec(),
            loopback: loopback.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn listening_sockets_come_from_the_kernel_tables() {
        // Established connections and junk lines are ignored; duplicates merge;
        // loopback-only listeners are told apart from reachable ones.
        assert_eq!(
            listening_sockets(TCP),
            listening(&[8080], &["127.0.0.11:38357", "127.0.0.1:22"])
        );
        assert_eq!(
            listening_sockets(TCP6),
            listening(&[80, 443, 8080], &["127.0.0.5:53", "[::1]:631"])
        );

        let dir = tempfile::tempdir().unwrap();
        // Not Linux: nothing to read.
        assert_eq!(own_listening(dir.path()), None);
        std::fs::write(dir.path().join("tcp"), TCP).unwrap();
        // No IPv6 table is fine.
        assert_eq!(
            own_listening(dir.path()),
            Some(listening(&[8080], &["127.0.0.11:38357", "127.0.0.1:22"]))
        );
        std::fs::write(dir.path().join("tcp6"), TCP6).unwrap();
        assert_eq!(
            own_listening(dir.path()),
            Some(listening(
                &[80, 443, 8080],
                &["127.0.0.11:38357", "127.0.0.1:22", "127.0.0.5:53", "[::1]:631"]
            ))
        );
    }

    #[test]
    fn private_hosts_are_recognised() {
        for h in [
            "127.0.0.1",
            "localhost",
            "10.0.0.5",
            "192.168.1.2",
            "172.16.4.4",
            "169.254.1.1",
            "100.64.0.9",
            "[fd00::1]",
            "fe80::1",
            "::1",
            "fulcrum",
            "node.lan",
            "node.local",
            "nas.home.arpa",
            "box.internal",
        ] {
            assert!(is_private_host(h), "{h}");
        }
        for h in ["8.8.8.8", "100.128.0.1", "2001:db8::1", "electrum.blockstream.info"] {
            assert!(!is_private_host(h), "{h}");
        }
    }

    fn relay(connected: bool) -> RelayState {
        RelayState {
            url: "wss://relay.example".into(),
            status: if connected { "connected" } else { "disconnected" }.into(),
            connected,
        }
    }

    fn proxy_check(status: &str) -> ProxyCheck {
        ProxyCheck {
            proxy: "socks5://127.0.0.1:9050".into(),
            status: status.into(),
            checked_at: 1,
            relays: vec![RelayCheck {
                relay: "wss://relay.example".into(),
                ok: status == "connected",
                code: "SOCKS5 00".into(),
                reason: String::new(),
                ms: Some(5),
            }],
        }
    }

    fn inputs(proxy: Option<&ProxySpec>) -> AuditInputs<'_> {
        AuditInputs {
            proxy,
            proxy_shown: proxy.map_or(String::new(), |_| "socks5://127.0.0.1:9050".into()),
            proxy_locked: true,
            transparent: false,
            configured: usize::from(proxy.is_some()),
            offline: crate::settings::OfflinePolicy::KillSwitch,
            relays: vec![relay(true)],
            last_check: Some(proxy_check("connected")),
            electrum: "127.0.0.1:50001".into(),
            electrum_plain: true,
            electrum_transport: "tcp".into(),
            announce: false,
            listening: Some(Listening::default()),
            now: 9,
        }
    }

    fn result(a: &PrivacyAudit, id: &str) -> String {
        a.checks
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.result.clone())
            .unwrap_or_default()
    }

    #[test]
    fn the_audit_scores_only_what_it_can_measure() {
        let tor = parse_proxy("socks5://127.0.0.1:9050").unwrap().unwrap();
        let web = parse_proxy("http://proxy.lan:3128").unwrap().unwrap();

        // Everything in order: only the two checks it cannot make are open.
        let best = audit(inputs(Some(&tor)));
        assert_eq!((best.score, best.checked, best.total, best.ran_at), (100, 8, 10, 9));
        assert_eq!(result(&best, "host_ports"), "not_checked");
        assert_eq!(result(&best, "internet_probe"), "not_checked");
        assert_eq!(result(&best, "proxy_check"), "pass");

        // No proxy: relays see the IP; proxy-only checks do not apply.
        let direct = audit(inputs(None));
        assert_eq!(result(&direct, "relays_via_proxy"), "fail");
        assert_eq!(result(&direct, "proxy_locked"), "");
        assert_eq!(direct.score, 70);

        // A proxy nothing connects through, not locked, last test failed,
        // an untested proxy, a partial test, an HTTP proxy.
        let mut i = inputs(Some(&tor));
        i.relays = vec![relay(false)];
        i.proxy_locked = false;
        i.last_check = Some(proxy_check("failed"));
        let bad = audit(i);
        assert_eq!(result(&bad, "relays_via_proxy"), "fail");
        assert_eq!(result(&bad, "proxy_locked"), "to_fix");
        assert_eq!(result(&bad, "proxy_check"), "fail");
        assert_eq!(bad.score, 40);
        let mut i = inputs(Some(&web));
        i.last_check = None;
        let untested = audit(i);
        assert_eq!(result(&untested, "proxy_check"), "not_checked");
        assert!(untested
            .checks
            .iter()
            .any(|c| c.evidence.iter().any(|e| e.contains("http CONNECT"))));
        let mut i = inputs(Some(&tor));
        i.last_check = Some(proxy_check("partial"));
        assert_eq!(result(&audit(i), "proxy_check"), "to_fix");

        // A public Electrum server over plaintext, open ports, announcements.
        let mut i = inputs(Some(&tor));
        i.electrum = "electrum.example:50001".into();
        i.listening = Some(listening(&[22, 8080], &[]));
        i.announce = true;
        let exposed = audit(i);
        assert_eq!(result(&exposed, "electrum_network"), "to_fix");
        assert_eq!(result(&exposed, "electrum_link"), "to_fix");
        assert_eq!(result(&exposed, "service_ports"), "to_fix");
        assert!(exposed
            .checks
            .iter()
            .any(|c| c.evidence == vec!["listening TCP ports   22, 8080".to_string()]));
        assert_eq!(result(&exposed, "announce"), "to_fix");
        assert_eq!(exposed.score, 65);

        // Docker's DNS resolver on 127.0.0.11 is loopback-only: not an open port.
        let mut i = inputs(Some(&tor));
        i.listening = Some(listening(&[], &["127.0.0.11:38357"]));
        let docker = audit(i);
        assert_eq!(result(&docker, "service_ports"), "pass");
        let check = docker.checks.iter().find(|c| c.id == "service_ports").unwrap();
        assert_eq!(
            check.evidence,
            vec!["listening TCP ports   0", "loopback only         127.0.0.11:38357"]
        );
        assert!(check.summary.contains("Loopback-only listeners"));

        // Where the kernel tables are missing, that check is not made.
        let mut i = inputs(Some(&tor));
        i.listening = None;
        let unknown = audit(i);
        assert_eq!(result(&unknown, "service_ports"), "not_checked");
        assert_eq!(unknown.checked, 7);
    }

    /// A TCP listener that accepts and holds connections, as a relay would.
    async fn relay_listener() -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                let (s, _) = l.accept().await.unwrap();
                held.push(s);
            }
        });
        format!("ws://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn relays_are_checked_through_the_proxy() {
        // No proxy: nothing to test.
        let direct = check(None, "", &[], 7).await;
        assert_eq!((direct.status.as_str(), direct.checked_at), ("direct", 7));

        let good = relay_listener().await;
        let (port, _p) = socks5_proxy(None).await;
        let tor = parse_proxy(&format!("socks5://127.0.0.1:{port}")).unwrap().unwrap();
        let ok = check(Some(&tor), "socks5://…", std::slice::from_ref(&good), 1).await;
        assert_eq!(ok.status, "connected");
        assert_eq!(ok.relays[0].code, "SOCKS5 00");
        assert!(ok.relays[0].ms.is_some());

        // One relay with no host: the others are still checked.
        let partial = check(Some(&tor), "", &[good.clone(), "wss://".into()], 1).await;
        assert_eq!(partial.status, "partial");
        assert_eq!(partial.relays[1].code, "invalid");

        // An HTTP proxy, with and without the login it wants.
        let (port, _h) = connect_proxy(false, None).await;
        let web = parse_proxy(&format!("http://127.0.0.1:{port}")).unwrap().unwrap();
        assert_eq!(
            check(Some(&web), "", std::slice::from_ref(&good), 1).await.relays[0].code,
            "HTTP 200"
        );
        let (port, _h) = connect_proxy(false, Some("dTpw")).await;
        let web = parse_proxy(&format!("http://127.0.0.1:{port}")).unwrap().unwrap();
        let refused = check(Some(&web), "", std::slice::from_ref(&good), 1).await;
        assert_eq!(
            (refused.status.as_str(), refused.relays[0].code.as_str()),
            ("failed", "HTTP 407")
        );

        // A proxy port nobody listens on.
        let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        let gone = parse_proxy(&format!("socks5://127.0.0.1:{port}")).unwrap().unwrap();
        let down = check(Some(&gone), "", std::slice::from_ref(&good), 1).await;
        assert_eq!(down.status, "failed");
        assert!(!down.relays[0].reason.is_empty());

        // A proxy that accepts but never answers.
        let silent = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = silent.local_addr().unwrap().port();
        let hold = tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                held.push(silent.accept().await.unwrap().0);
            }
        });
        let mute = parse_proxy(&format!("socks5://127.0.0.1:{port}")).unwrap().unwrap();
        let timed_out = check(Some(&mute), "", std::slice::from_ref(&good), 1).await;
        assert_eq!(timed_out.relays[0].code, "ETIMEDOUT");
        hold.abort();
    }
}
