use std::{net::Ipv4Addr, path::Path};
#[derive(Clone, Copy)]
pub(super) struct Interface {
    pub address: Ipv4Addr,
    pub mask: u32,
}
impl Interface {
    pub fn accepts(self, address: Ipv4Addr) -> bool {
        private(address)
            && (u32::from(address) & self.mask) == (u32::from(self.address) & self.mask)
    }
}
fn private(ip: Ipv4Addr) -> bool {
    ip.is_private() && !ip.is_loopback() && !ip.is_unspecified()
}
#[cfg(unix)]
pub(super) fn interfaces() -> Result<Vec<Interface>, String> {
    let mut head = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err("lan_interfaces_unavailable".into());
    }
    struct List(*mut libc::ifaddrs);
    impl Drop for List {
        fn drop(&mut self) {
            unsafe { libc::freeifaddrs(self.0) };
        }
    }
    let _list = List(head);
    let mut rows = Vec::new();
    let mut next = head;
    while !next.is_null() {
        let row = unsafe { &*next };
        if !row.ifa_addr.is_null()
            && !row.ifa_netmask.is_null()
            && unsafe { (*row.ifa_addr).sa_family as i32 } == libc::AF_INET
            && row.ifa_flags & libc::IFF_UP as u32 != 0
            && row.ifa_flags & libc::IFF_LOOPBACK as u32 == 0
        {
            let ip = unsafe { &*(row.ifa_addr as *const libc::sockaddr_in) };
            let mask = unsafe { &*(row.ifa_netmask as *const libc::sockaddr_in) };
            let address = Ipv4Addr::from(u32::from_be(ip.sin_addr.s_addr));
            let mask = u32::from_be(mask.sin_addr.s_addr);
            let inverse = !mask;
            if private(address)
                && mask != 0
                && inverse & inverse.wrapping_add(1) == 0
                && !rows.iter().any(|i: &Interface| i.address == address)
            {
                rows.push(Interface { address, mask });
                if rows.len() > 16 {
                    return Err("capacity_exceeded: LAN interfaces".into());
                }
            }
        }
        next = row.ifa_next;
    }
    rows.sort_by_key(|row| row.address);
    Ok(rows)
}
#[cfg(not(unix))]
pub(super) fn interfaces() -> Result<Vec<Interface>, String> {
    Err("unsupported: LAN interfaces".into())
}

#[cfg(unix)]
pub(super) fn space(path: &Path, required: u64) -> Result<(), String> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| "lan_storage_unavailable")?;
    let mut value = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), value.as_mut_ptr()) } != 0 {
        return Err("lan_storage_unavailable".into());
    }
    let value = unsafe { value.assume_init() };
    let available = (value.f_bavail as u64).saturating_mul(value.f_frsize as u64);
    if available < required {
        Err("insufficient_space".into())
    } else {
        Ok(())
    }
}
#[cfg(not(unix))]
pub(super) fn space(_: &Path, _: u64) -> Result<(), String> {
    Err("unsupported: disk space".into())
}
