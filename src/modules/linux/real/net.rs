use super::common::{error_value, io_error, ok_unit};
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustix::net::{AddressFamily, SocketFlags, SocketType, socket_with};
use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

/// AF_INET datagram socket used purely as an ioctl handle for interface config.
fn inet_dgram_socket() -> io::Result<OwnedFd> {
    socket_with(
        AddressFamily::INET,
        SocketType::DGRAM,
        SocketFlags::CLOEXEC,
        None,
    )
    .map_err(io::Error::from)
}

pub(crate) fn link_up(interface: &str, span: Span) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value("linux-link-up", error.to_string(), span));
    }
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-link-up", error, span)),
    };
    let result = link_up_with_socket(fd.as_raw_fd(), interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-link-up", error, span)),
    }
}

pub(crate) fn link_down(interface: &str, span: Span) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value("linux-link-down", error.to_string(), span));
    }
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-link-down", error, span)),
    };
    let result = link_down_with_socket(fd.as_raw_fd(), interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-link-down", error, span)),
    }
}

pub(crate) fn set_ipv4_address(
    interface: &str,
    address: &str,
    netmask: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value(
            "linux-set-ipv4-address",
            error.to_string(),
            span,
        ));
    }
    let address = match parse_ipv4(address, "address") {
        Ok(address) => address,
        Err(error) => {
            return Ok(error_value(
                "linux-set-ipv4-address",
                error.to_string(),
                span,
            ));
        }
    };
    let netmask = match parse_ipv4(netmask, "netmask") {
        Ok(netmask) => netmask,
        Err(error) => {
            return Ok(error_value(
                "linux-set-ipv4-address",
                error.to_string(),
                span,
            ));
        }
    };
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-set-ipv4-address", error, span)),
    };
    let result = set_ipv4_address_with_socket(fd.as_raw_fd(), interface, address, netmask);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-set-ipv4-address", error, span)),
    }
}

pub(crate) fn add_default_ipv4_route(
    gateway: &str,
    interface: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    if !interface.is_empty()
        && let Err(error) = validate_interface(interface)
    {
        return Ok(error_value(
            "linux-add-default-ipv4-route",
            error.to_string(),
            span,
        ));
    }
    let gateway = match parse_ipv4(gateway, "gateway") {
        Ok(gateway) => gateway,
        Err(error) => {
            return Ok(error_value(
                "linux-add-default-ipv4-route",
                error.to_string(),
                span,
            ));
        }
    };
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-add-default-ipv4-route", error, span)),
    };
    let result = add_default_ipv4_route_with_socket(fd.as_raw_fd(), gateway, interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-add-default-ipv4-route", error, span)),
    }
}

pub(crate) fn flush_ipv4_addresses(interface: &str, span: Span) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value(
            "linux-flush-ipv4-addresses",
            error.to_string(),
            span,
        ));
    }
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-flush-ipv4-addresses", error, span)),
    };
    let result = flush_ipv4_addresses_with_socket(fd.as_raw_fd(), interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) if error.kind() == io::ErrorKind::AddrNotAvailable => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-flush-ipv4-addresses", error, span)),
    }
}

pub(crate) fn del_default_ipv4_route(
    gateway: &str,
    interface: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    if let Err(error) = validate_interface(interface) {
        return Ok(error_value(
            "linux-del-default-ipv4-route",
            error.to_string(),
            span,
        ));
    }
    let gateway = match parse_ipv4(gateway, "gateway") {
        Ok(gateway) => gateway,
        Err(error) => {
            return Ok(error_value(
                "linux-del-default-ipv4-route",
                error.to_string(),
                span,
            ));
        }
    };
    let fd = match inet_dgram_socket() {
        Ok(fd) => fd,
        Err(error) => return Ok(io_error("linux-del-default-ipv4-route", error, span)),
    };
    let result = del_default_ipv4_route_with_socket(fd.as_raw_fd(), gateway, interface);
    match result {
        Ok(()) => Ok(ok_unit()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-del-default-ipv4-route", error, span)),
    }
}

fn link_up_with_socket(fd: RawFd, interface: &str) -> io::Result<()> {
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    fill_ifreq_name(&mut request, interface);
    if unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS as _, &mut request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        request.ifr_ifru.ifru_flags |= libc::IFF_UP as libc::c_short;
    }
    if unsafe { libc::ioctl(fd, libc::SIOCSIFFLAGS as _, &request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_ipv4_address_with_socket(
    fd: RawFd,
    interface: &str,
    address: std::net::Ipv4Addr,
    netmask: std::net::Ipv4Addr,
) -> io::Result<()> {
    set_ifreq_sockaddr(fd, interface, libc::SIOCSIFADDR, address)?;
    set_ifreq_sockaddr(fd, interface, libc::SIOCSIFNETMASK, netmask)
}

fn set_ifreq_sockaddr(
    fd: RawFd,
    interface: &str,
    request_code: libc::c_ulong,
    address: std::net::Ipv4Addr,
) -> io::Result<()> {
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    fill_ifreq_name(&mut request, interface);
    request.ifr_ifru.ifru_addr = ipv4_sockaddr(address);
    if unsafe { libc::ioctl(fd, request_code as _, &request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn add_default_ipv4_route_with_socket(
    fd: RawFd,
    gateway: std::net::Ipv4Addr,
    interface: &str,
) -> io::Result<()> {
    let interface = if interface.is_empty() {
        None
    } else {
        Some(CString::new(interface).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "interface name contains NUL")
        })?)
    };
    let mut route: libc::rtentry = unsafe { std::mem::zeroed() };
    route.rt_gateway = ipv4_sockaddr(gateway);
    route.rt_dst = ipv4_sockaddr(std::net::Ipv4Addr::UNSPECIFIED);
    route.rt_genmask = ipv4_sockaddr(std::net::Ipv4Addr::UNSPECIFIED);
    route.rt_flags = (libc::RTF_UP | libc::RTF_GATEWAY) as libc::c_ushort;
    if let Some(interface) = interface.as_ref() {
        route.rt_dev = interface.as_ptr() as *mut libc::c_char;
    }
    if unsafe { libc::ioctl(fd, libc::SIOCADDRT as _, &route) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn link_down_with_socket(fd: RawFd, interface: &str) -> io::Result<()> {
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    fill_ifreq_name(&mut request, interface);
    if unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS as _, &mut request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        request.ifr_ifru.ifru_flags &= !(libc::IFF_UP as libc::c_short);
    }
    if unsafe { libc::ioctl(fd, libc::SIOCSIFFLAGS as _, &request) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn flush_ipv4_addresses_with_socket(fd: RawFd, interface: &str) -> io::Result<()> {
    set_ifreq_sockaddr(
        fd,
        interface,
        libc::SIOCDIFADDR,
        std::net::Ipv4Addr::UNSPECIFIED,
    )
}

fn del_default_ipv4_route_with_socket(
    fd: RawFd,
    gateway: std::net::Ipv4Addr,
    interface: &str,
) -> io::Result<()> {
    let interface_c = CString::new(interface)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "interface name contains NUL"))?;
    let mut route: libc::rtentry = unsafe { std::mem::zeroed() };
    route.rt_gateway = ipv4_sockaddr(gateway);
    route.rt_dst = ipv4_sockaddr(std::net::Ipv4Addr::UNSPECIFIED);
    route.rt_genmask = ipv4_sockaddr(std::net::Ipv4Addr::UNSPECIFIED);
    route.rt_flags = (libc::RTF_UP | libc::RTF_GATEWAY) as libc::c_ushort;
    route.rt_dev = interface_c.as_ptr() as *mut libc::c_char;
    if unsafe { libc::ioctl(fd, libc::SIOCDELRT as _, &route) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn ipv4_sockaddr(address: std::net::Ipv4Addr) -> libc::sockaddr {
    let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    addr.sin_family = libc::AF_INET as libc::sa_family_t;
    addr.sin_addr = libc::in_addr {
        s_addr: u32::from(address).to_be(),
    };
    unsafe { std::mem::transmute(addr) }
}

fn fill_ifreq_name(request: &mut libc::ifreq, interface: &str) {
    for (index, byte) in interface.as_bytes().iter().copied().enumerate() {
        request.ifr_name[index] = byte as libc::c_char;
    }
}

fn validate_interface(interface: &str) -> io::Result<()> {
    if interface.is_empty() || interface.len() >= libc::IFNAMSIZ {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "interface name is empty or too long",
        ));
    }
    Ok(())
}

fn parse_ipv4(value: &str, label: &str) -> io::Result<std::net::Ipv4Addr> {
    value.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid IPv4 {label}: {value}"),
        )
    })
}
