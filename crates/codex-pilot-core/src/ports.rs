use std::net::{TcpListener, ToSocketAddrs};

pub const DEFAULT_DEBUG_PORT: u16 = 9688;
pub const DEFAULT_HELPER_PORT: u16 = 58888;
pub const LAUNCHER_GUARD_PORT: u16 = 58887;

pub fn select_platform_loopback_port(requested: u16) -> u16 {
    select_loopback_port_with(
        requested,
        can_bind_loopback_port,
        find_available_loopback_port,
    )
}

fn select_loopback_port_with(
    requested: u16,
    can_bind: impl Fn(u16) -> bool,
    find_available: impl Fn() -> u16,
) -> u16 {
    if can_bind(requested) {
        requested
    } else {
        find_available()
    }
}

pub fn can_bind_loopback_port(port: u16) -> bool {
    if port == 0 {
        return true;
    }
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn find_available_loopback_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .unwrap_or(0)
}

pub fn can_connect_loopback_port(port: u16) -> bool {
    ("127.0.0.1", port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
        .and_then(|address| {
            std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(200))
                .ok()
        })
        .is_some()
}

pub fn acquire_loopback_port_guard(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_platform_loopback_port_keeps_available_port() {
        assert_eq!(select_loopback_port_with(12345, |_| true, || 54321), 12345);
    }

    #[test]
    fn select_platform_loopback_port_falls_back_when_requested_is_busy() {
        assert_eq!(select_loopback_port_with(12345, |_| false, || 54321), 54321);
    }
}
