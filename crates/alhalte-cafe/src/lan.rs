//! Best-effort detection of the host's LAN address.
//!
//! `alhalte-cafe` prints the loopback URL and additionally a LAN URL when an
//! address can be detected, so the URL can be typed on a phone. Detection is
//! deliberately advisory: sign-in works for the whole private network through
//! `maitre-d --allow-lan-origin-port`, not just for the address printed here
//! (Issue #229).
//!
//! No packet is sent: the kernel only picks the source address it would use for
//! an off-link destination, which is the address of the interface holding the
//! default route. No new dependency is needed (std only).

use std::net::{IpAddr, Ipv4Addr, UdpSocket};

/// Returns the host's best-effort LAN IPv4 addresses.
///
/// Loopback, unspecified, and link-local addresses are filtered out and the
/// result is deduplicated. An empty result means detection failed (for example
/// because the host has no default route); callers must treat it as advisory.
pub fn detect_lan_ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut addresses = Vec::new();
    // Connecting a UDP socket performs a routing-table lookup only; it sends
    // nothing. Two destinations make the probe a little more robust when one
    // route is filtered.
    for destination in ["8.8.8.8:80", "1.1.1.1:80"] {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            continue;
        };
        if socket.connect(destination).is_err() {
            continue;
        }
        let Ok(local_addr) = socket.local_addr() else {
            continue;
        };
        let IpAddr::V4(ip) = local_addr.ip() else {
            continue;
        };
        if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() {
            continue;
        }
        if !addresses.contains(&ip) {
            addresses.push(ip);
        }
    }
    addresses
}

/// Formats the browser URL for one LAN address and the `maitre-d` port.
pub fn lan_url(ip: Ipv4Addr, port: u16) -> String {
    format!("http://{ip}:{port}/")
}

/// Returns the browser URLs for every detected LAN address.
///
/// Best-effort: an empty vector means no LAN address could be detected.
pub fn detect_lan_urls(port: u16) -> Vec<String> {
    detect_lan_ipv4_addresses()
        .into_iter()
        .map(|ip| lan_url(ip, port))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: alhalte-cafe/lan 0003 LAN URLs keep the maitre-d port
    #[test]
    fn lan_url_formats_the_private_address_and_port() {
        assert_eq!(
            lan_url(Ipv4Addr::new(192, 168, 1, 42), 8080),
            "http://192.168.1.42:8080/"
        );
        assert_eq!(
            lan_url(Ipv4Addr::new(10, 0, 0, 7), 3000),
            "http://10.0.0.7:3000/"
        );
    }
}
