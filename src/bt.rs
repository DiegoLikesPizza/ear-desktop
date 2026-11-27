//! Bluetooth Classic RFCOMM transport.
//!
//! On Windows this talks straight to Winsock's `AF_BTH` sockets: connecting with a
//! service-class UUID makes the stack do the SDP lookup for us, so there is no
//! virtual COM port, no driver and no browser in the path.

use std::io;

pub const SPP_UUID: u128 = 0xaeac4a03_dff5_498f_843a_34487cf133eb;
pub const FASTPAIR_UUID: u128 = 0xdf21fe2c_2515_4fdb_8886_f12c4d67927c;

#[derive(Clone, Debug)]
pub struct PairedDevice {
    pub address: u64,
    pub name: String,
    pub connected: bool,
}

pub fn format_address(addr: u64) -> String {
    let b = addr.to_be_bytes();
    format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", b[2], b[3], b[4], b[5], b[6], b[7])
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::Once;
    use std::time::Duration;
    use windows::Win32::Devices::Bluetooth::*;
    use windows::Win32::Networking::WinSock::*;
    use windows::core::GUID;

    fn wsa_init() {
        static INIT: Once = Once::new();
        INIT.call_once(|| unsafe {
            let mut data = WSADATA::default();
            WSAStartup(0x0202, &mut data);
        });
    }

    fn last_err() -> io::Error {
        io::Error::from_raw_os_error(unsafe { WSAGetLastError().0 })
    }

    pub fn paired_devices() -> io::Result<Vec<PairedDevice>> {
        let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: true.into(),
            fReturnRemembered: true.into(),
            fReturnUnknown: false.into(),
            fReturnConnected: true.into(),
            fIssueInquiry: false.into(),
            cTimeoutMultiplier: 0,
            hRadio: Default::default(),
        };
        let mut info = BLUETOOTH_DEVICE_INFO { dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
        let mut out = Vec::new();
        unsafe {
            let find = match BluetoothFindFirstDevice(&params, &mut info) {
                Ok(h) => h,
                // ERROR_NO_MORE_ITEMS: nothing paired
                Err(e) if e.code().0 as u32 & 0xFFFF == 259 => return Ok(out),
                Err(e) => return Err(io::Error::other(e)),
            };
            loop {
                let len = info.szName.iter().position(|&c| c == 0).unwrap_or(info.szName.len());
                out.push(PairedDevice {
                    address: info.Address.Anonymous.ullLong,
                    name: String::from_utf16_lossy(&info.szName[..len]),
                    connected: info.fConnected.as_bool(),
                });
                info = BLUETOOTH_DEVICE_INFO { dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
                if BluetoothFindNextDevice(find, &mut info).is_err() {
                    break;
                }
            }
            let _ = BluetoothFindDeviceClose(find);
        }
        out.dedup_by_key(|d| d.address);
        Ok(out)
    }

    pub struct Rfcomm {
        sock: SOCKET,
    }

    // A Winsock SOCKET may be used for recv and send from different threads.
    unsafe impl Send for Rfcomm {}
    unsafe impl Sync for Rfcomm {}

    impl Rfcomm {
        pub fn connect(address: u64, service: u128) -> io::Result<Self> {
            wsa_init();
            unsafe {
                let sock = socket(AF_BTH as i32, SOCK_STREAM, BTHPROTO_RFCOMM as i32)?;
                let addr = SOCKADDR_BTH {
                    addressFamily: AF_BTH,
                    btAddr: address,
                    serviceClassId: GUID::from_u128(service),
                    port: 0,
                };
                let r = connect(sock, &addr as *const _ as *const SOCKADDR, size_of::<SOCKADDR_BTH>() as i32);
                if r != 0 {
                    let e = last_err();
                    closesocket(sock);
                    return Err(e);
                }
                Ok(Self { sock })
            }
        }

        pub fn set_read_timeout(&self, t: Option<Duration>) -> io::Result<()> {
            let ms: u32 = t.map(|d| d.as_millis().max(1) as u32).unwrap_or(0);
            let r = unsafe { setsockopt(self.sock, SOL_SOCKET, SO_RCVTIMEO, Some(&ms.to_ne_bytes())) };
            if r != 0 { Err(last_err()) } else { Ok(()) }
        }

        /// Ok(0) means the peer closed the channel.
        pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
            let n = unsafe { recv(self.sock, buf, SEND_RECV_FLAGS(0)) };
            if n < 0 { Err(last_err()) } else { Ok(n as usize) }
        }

        pub fn write_all(&self, mut data: &[u8]) -> io::Result<()> {
            while !data.is_empty() {
                let n = unsafe { send(self.sock, data, SEND_RECV_FLAGS(0)) };
                if n <= 0 {
                    return Err(last_err());
                }
                data = &data[n as usize..];
            }
            Ok(())
        }

        /// Unblocks a reader thread stuck in `read`.
        pub fn shutdown(&self) {
            unsafe {
                shutdown(self.sock, SD_BOTH);
            }
        }
    }

    impl Drop for Rfcomm {
        fn drop(&mut self) {
            unsafe {
                closesocket(self.sock);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::time::Duration;

    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "Bluetooth RFCOMM is only implemented for Windows so far")
    }

    pub fn paired_devices() -> io::Result<Vec<PairedDevice>> {
        Err(unsupported())
    }

    pub struct Rfcomm;
    impl Rfcomm {
        pub fn connect(_: u64, _: u128) -> io::Result<Self> {
            Err(unsupported())
        }
        pub fn set_read_timeout(&self, _: Option<Duration>) -> io::Result<()> {
            Err(unsupported())
        }
        pub fn read(&self, _: &mut [u8]) -> io::Result<usize> {
            Err(unsupported())
        }
        pub fn write_all(&self, _: &[u8]) -> io::Result<()> {
            Err(unsupported())
        }
        pub fn shutdown(&self) {}
    }
}

pub use imp::{Rfcomm, paired_devices};

/// Ask the device's Fast Pair RFCOMM service for its 3-byte model id (hex, e.g. "FEB1C7").
pub fn read_fastpair_model(address: u64) -> io::Result<String> {
    use std::time::{Duration, Instant};
    let s = Rfcomm::connect(address, FASTPAIR_UUID)?;
    s.set_read_timeout(Some(Duration::from_millis(1500)))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut acc = Vec::new();
    let mut buf = [0u8; 64];
    while Instant::now() < deadline {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                acc.extend_from_slice(&buf[..n]);
                // Fast Pair message stream: group 0x03 (device info), code 0x01 (model id), len 3
                if let Some(i) = acc.windows(4).position(|w| w == [0x03, 0x01, 0x00, 0x03])
                    && acc.len() >= i + 7
                {
                    let id = &acc[i + 4..i + 7];
                    return Ok(format!("{:02X}{:02X}{:02X}", id[0], id[1], id[2]));
                }
                if n == 7 && acc.len() == 7 {
                    return Ok(format!("{:02X}{:02X}{:02X}", acc[4], acc[5], acc[6]));
                }
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => break,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "device did not report a Fast Pair model id"))
}
