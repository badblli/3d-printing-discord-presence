use std::{
    collections::BTreeMap,
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    time::{Duration, Instant},
};

use socket2::{Domain, Protocol, Socket, Type};

use super::Discovered;
use crate::{
    config::{validate::validate_connection, PrinterConnection},
    printers::bambu::models,
};

const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const PORTS: [u16; 2] = [1990, 2021];
const TARGET: &str = "urn:bambulab-com:device:3dprinter:1";
const MAX_PACKET: usize = 16 * 1024;

/// Parse only printer advertisements/responses, without I/O or logging packet data.
pub fn parse(packet: &[u8], from: Ipv4Addr) -> Option<Discovered> {
    if packet.len() > MAX_PACKET {
        return None;
    }
    let text = std::str::from_utf8(packet).ok()?;
    let mut lines = text.lines();
    let first = lines.next()?.trim();
    if !first.eq_ignore_ascii_case("NOTIFY * HTTP/1.1")
        && !first.eq_ignore_ascii_case("HTTP/1.1 200 OK")
    {
        return None;
    }
    let mut bambu = false;
    let mut location = "";
    let mut serial = "";
    let mut name = "";
    for line in lines.take_while(|line| !line.is_empty()) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "nt" | "st" => bambu |= value.eq_ignore_ascii_case(TARGET),
            "location" => location = value,
            "usn" => serial = value,
            "devname.bambu.com" => name = value,
            _ => {}
        }
    }
    if !bambu {
        return None;
    }
    let host = location
        .parse::<Ipv4Addr>()
        .ok()
        .filter(Ipv4Addr::is_private)
        .unwrap_or(from)
        .to_string();
    validate_connection(&PrinterConnection::Bambu {
        host: host.clone(),
        serial: serial.into(),
        model: None,
    })
    .ok()?;
    Some(Discovered {
        host,
        serial: serial.into(),
        name: name.chars().filter(|c| !c.is_control()).take(80).collect(),
        model: models::resolve(None, serial).into(),
        studio_code: false,
    })
}

fn listener(port: u16) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())?;
    socket.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

pub fn discover(seconds: u8) -> Vec<Discovered> {
    let deadline = Instant::now() + Duration::from_secs(u64::from(seconds.clamp(1, 15)));
    let sockets: Vec<_> = PORTS
        .into_iter()
        .filter_map(|port| listener(port).ok())
        .collect();
    // A failed bind on either port must not prevent searches on both ports.
    if let Some(socket) = sockets.first() {
        for port in PORTS {
            let request = format!("M-SEARCH * HTTP/1.1\r\nHOST: {GROUP}:{port}\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {TARGET}\r\n\r\n");
            let _ = socket.send_to(request.as_bytes(), SocketAddrV4::new(GROUP, port));
        }
    } else {
        return Vec::new();
    }
    let mut found = BTreeMap::new();
    // One extra byte makes truncated oversized datagrams rejectable too.
    let mut packet = [0; MAX_PACKET + 1];
    while Instant::now() < deadline {
        for socket in &sockets {
            // Bound each drain so busy LANs cannot extend the requested timeout.
            for _ in 0..64 {
                if Instant::now() >= deadline {
                    break;
                }
                match socket.recv_from(&mut packet) {
                    Ok((length, SocketAddr::V4(from))) => {
                        if let Some(printer) = parse(&packet[..length], *from.ip()) {
                            found.insert(printer.serial.clone(), printer);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        std::thread::sleep(
            Duration::from_millis(20).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    found.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const FROM: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 50);

    fn packet(first: &str, target: &str, location: &str, serial: &str, name: &str) -> Vec<u8> {
        format!("{first}\r\n{target}\r\nLocation: {location}\r\nUSN: {serial}\r\nDevName.bambu.com: {name}\r\n\r\n").into_bytes()
    }

    #[test]
    fn notify_and_search_response_use_case_insensitive_headers() {
        for (first, header) in [("NOTIFY * HTTP/1.1", "nT"), ("HTTP/1.1 200 OK", "sT")] {
            let bytes = packet(
                first,
                &format!("{header}: {TARGET}"),
                "192.168.1.50",
                "00M00A000000000",
                "Fake printer",
            );
            let printer = parse(&bytes, FROM).unwrap();
            assert_eq!(printer.host, "192.168.1.50");
            assert_eq!(printer.serial, "00M00A000000000");
            assert_eq!(printer.name, "Fake printer");
            assert_eq!(printer.model, "Bambu Lab X1 Carbon");
            assert!(!printer.studio_code);
        }
    }

    #[test]
    fn mixed_case_headers_prefer_valid_private_location() {
        let packet = "NOTIFY * HTTP/1.1\r\nnT: urn:bambulab-com:device:3dprinter:1\r\nlOcAtIoN: 192.168.1.51\r\nuSn: 00M00A000000000\r\ndEVnAME.BAMBU.COM: Fake printer\r\n\r\n";
        let printer = parse(packet.as_bytes(), FROM).unwrap();
        assert_eq!(printer.host, "192.168.1.51");
        assert_eq!(printer.name, "Fake printer");
        assert_eq!(printer.serial, "00M00A000000000");
    }

    #[test]
    fn rejects_other_devices_invalid_serials_and_oversized_packets() {
        for (target, serial) in [
            ("NT: ssdp:all", "00M00A000000000"),
            ("NT: urn:bambulab-com:device:3dprinter:1", "bad!"),
            ("NT: urn:bambulab-com:device:3dprinter:1", "short"),
        ] {
            assert!(parse(
                &packet("NOTIFY * HTTP/1.1", target, "", serial, "Fake"),
                FROM
            )
            .is_none());
        }
        assert!(parse(&vec![b'a'; MAX_PACKET + 1], FROM).is_none());
        assert!(parse(b"\xff", FROM).is_none());
        assert!(parse(
            &packet(
                "M-SEARCH * HTTP/1.1",
                &format!("ST: {TARGET}"),
                "",
                "00M00A000000000",
                "Fake"
            ),
            FROM
        )
        .is_none());
    }

    #[test]
    fn sanitizes_names_by_characters_and_falls_back_to_source() {
        for location in ["", "8.8.8.8", "invalid", "http://192.168.1.50"] {
            let bytes = packet(
                "NOTIFY * HTTP/1.1",
                &format!("NT: {TARGET}"),
                location,
                "00M00A000000000",
                &format!("F\t\0{}", "ö".repeat(100)),
            );
            let printer = parse(&bytes, FROM).unwrap();
            assert_eq!(printer.host, FROM.to_string());
            assert_eq!(printer.name.chars().count(), 80);
            assert!(!printer.name.chars().any(char::is_control));
        }
        let bytes = packet(
            "NOTIFY * HTTP/1.1",
            &format!("NT: {TARGET}"),
            "",
            "00M00A000000000",
            "Fake",
        );
        assert!(parse(&bytes, Ipv4Addr::new(8, 8, 8, 8)).is_none());
    }
}
