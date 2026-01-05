//! Network utilities

use std::net::{TcpStream, UdpSocket};
use std::time::Duration;

/// Check if internet connection is available
pub fn check_internet() -> bool {
    // Try to connect to a reliable server
    let hosts = [
        ("8.8.8.8", 53),        // Google DNS
        ("1.1.1.1", 53),        // Cloudflare DNS
        ("208.67.222.222", 53), // OpenDNS
    ];

    for (host, port) in hosts {
        if TcpStream::connect_timeout(
            &format!("{}:{}", host, port).parse().unwrap(),
            Duration::from_secs(3),
        )
        .is_ok()
        {
            return true;
        }
    }

    false
}

/// Get the local IP address
pub fn get_local_ip() -> Option<String> {
    // Connect to a remote address to determine local IP
    // (doesn't actually send any data)
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let local_addr = socket.local_addr().ok()?;
    Some(local_addr.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_local_ip() {
        // This test may fail on machines without network
        if let Some(ip) = get_local_ip() {
            assert!(!ip.is_empty());
            // Should be an IPv4 address
            assert!(ip.contains('.'));
        }
    }
}
