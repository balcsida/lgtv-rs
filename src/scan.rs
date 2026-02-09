use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::str;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TvDevice {
    pub uuid: Option<String>,
    pub tv_name: Option<String>,
    pub address: String,
}

fn extract_uuid(response: &str) -> Option<String> {
    let start = response.find("uuid:")? + 5;
    let end = response[start..].find(':')? + start;
    Some(response[start..end].to_string())
}

fn extract_tv_name(response: &str) -> Option<String> {
    let marker = "DLNADeviceName.lge.com:";
    let start = response.find(marker)? + marker.len();
    let rest = &response[start..];
    let end = rest.find(['\r', '\n'])?;
    Some(rest[..end].trim().to_string())
}

pub async fn scan_for_tvs() -> Result<Vec<TvDevice>> {
    let ssdp_request = "M-SEARCH * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: 2\r\n\
         ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n"
        .to_string();

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;

    let multicast_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(239, 255, 255, 250)), 1900);

    let mut addresses = Vec::new();
    let attempts = 4;

    for _ in 0..attempts {
        socket.send_to(ssdp_request.as_bytes(), multicast_addr)?;

        let mut buf = [0u8; 4096];
        match socket.recv_from(&mut buf) {
            Ok((len, addr)) => {
                let response = str::from_utf8(&buf[..len]).unwrap_or("");

                if response.contains("LG") {
                    addresses.push(TvDevice {
                        uuid: extract_uuid(response),
                        tv_name: extract_tv_name(response),
                        address: addr.ip().to_string(),
                    });
                } else {
                    log::debug!("Unknown device: {}, {}", response, addr);
                }
            }
            Err(e) => {
                log::debug!("Error receiving response: {}", e);
            }
        }

        sleep(Duration::from_secs(2)).await;
    }

    // De-duplicate by address
    let mut unique_addresses = Vec::new();
    let mut seen_addresses = std::collections::HashSet::new();

    for device in addresses {
        if !seen_addresses.contains(&device.address) {
            seen_addresses.insert(device.address.clone());
            unique_addresses.push(device);
        }
    }

    Ok(unique_addresses)
}
