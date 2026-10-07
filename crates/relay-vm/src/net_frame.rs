//! Ethernet, ARP, IPv4, and DHCP frames for the Relay userspace NAT.
//!
//! The guest address is the QEMU slirp convention: 10.0.2.15/24 via
//! 10.0.2.2, with DNS at 10.0.2.3. Nix treats any non-loopback address as
//! internet access. These builders stay free of sockets so the bounds can
//! be checked without a host connection.

use std::sync::atomic::{AtomicU16, Ordering};

pub(crate) const MAX_FRAME: usize = 2048;
/// `VIRTIO_NET_F_MRG_RXBUF` header. Byte 10 is `num_buffers` (little-endian).
pub(crate) const VIRTIO_RX_HEADER: usize = 12;
pub(crate) const VIRTIO_F_MRG_RXBUF: u64 = 1 << 15;
pub(crate) const GUEST_IP: [u8; 4] = [10, 0, 2, 15];
pub(crate) const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];
pub(crate) const DNS_IP: [u8; 4] = [10, 0, 2, 3];
pub(crate) const UPSTREAM_DNS: [u8; 4] = [1, 1, 1, 1];
pub(crate) const GUEST_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
pub(crate) const GATEWAY_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x02];

const ETHERTYPE_IPV4: u16 = 0x0800;
const ETHERTYPE_ARP: u16 = 0x0806;

static IP_ID: AtomicU16 = AtomicU16::new(1);

/// True when an IPv4 header of `ihl_words` and `total_len` sits inside the
/// Ethernet frame. The parser refuses every span this rejects.
pub(crate) fn ipv4_span_ok(frame_len: usize, ihl_words: u8, total_len: u16) -> bool {
    let ihl = usize::from(ihl_words);
    let total = usize::from(total_len);
    (5..=15).contains(&ihl)
        && total >= ihl * 4
        && frame_len >= 14
        && total <= frame_len - 14
        && frame_len <= MAX_FRAME
}

#[cfg(kani)]
#[kani::proof]
fn ipv4_span_never_claims_bytes_past_the_frame() {
    let frame_len_u: u8 = kani::any();
    let ihl_words: u8 = kani::any();
    let total_len: u16 = kani::any();
    let frame_len = usize::from(frame_len_u);
    kani::assume(ihl_words <= 16);
    kani::assume(total_len <= 80);
    let ok = ipv4_span_ok(frame_len, ihl_words, total_len);
    if ok {
        let ihl = usize::from(ihl_words);
        let total = usize::from(total_len);
        assert!((5..=15).contains(&ihl));
        assert!(total >= ihl * 4);
        assert!(14 + total <= frame_len);
        assert!(frame_len <= MAX_FRAME);
    }
}

pub(crate) fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut index = 0;
    while index + 1 < data.len() {
        sum += u32::from(u16::from_be_bytes([data[index], data[index + 1]]));
        index += 2;
    }
    if index < data.len() {
        sum += u32::from(data[index]) << 8;
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

pub(crate) fn config_byte(offset: u64) -> u8 {
    match offset {
        0..=5 => GUEST_MAC[offset as usize],
        6 => 1, // VIRTIO_NET_S_LINK_UP
        _ => 0,
    }
}

/// Strip a virtio-net header (10 bytes, or 12 when mergeable buffers are on)
/// and return the Ethernet frame. A descriptor may be a full page; the IPv4
/// total length, not the descriptor length, ends the frame.
pub(crate) fn ethernet_from_virtio(buffer: &[u8]) -> Option<&[u8]> {
    for skip in [10usize, 12] {
        if buffer.len() < skip + 14 {
            continue;
        }
        let ethertype = u16::from_be_bytes([buffer[skip + 12], buffer[skip + 13]]);
        let frame = &buffer[skip..];
        if ethertype == ETHERTYPE_ARP && frame.len() >= 42 {
            return Some(&frame[..42]);
        }
        if ethertype != ETHERTYPE_IPV4 || frame.len() < 34 {
            continue;
        }
        let total = usize::from(u16::from_be_bytes([frame[16], frame[17]]));
        let end = 14 + total;
        if (20..=MAX_FRAME).contains(&end) && end <= frame.len() {
            return Some(&frame[..end]);
        }
    }
    None
}

/// Prefix a guest receive buffer. `num_buffers` is 1 when the driver
/// negotiated mergeable receive buffers. Otherwise the header is 10 bytes.
pub(crate) fn rx_virtio_packet(frame: &[u8], negotiated: u64) -> Vec<u8> {
    let header = if negotiated & VIRTIO_F_MRG_RXBUF != 0 {
        VIRTIO_RX_HEADER
    } else {
        10
    };
    let mut packet = vec![0u8; header + frame.len()];
    // GUEST_CSUM is offered. DATA_VALID lets the driver accept the checksum
    // we already computed. auth33 set this on a 1460-byte segment that did
    // not fit the first descriptor. auth34 cleared it on 128-byte segments
    // and the guest still sent no TCP ACK. Both are required together.
    packet[0] = 2;
    if header == VIRTIO_RX_HEADER {
        packet[10] = 1;
    }
    packet[header..].copy_from_slice(frame);
    packet
}

pub(crate) enum Ingress {
    /// Answer generated on the device (ARP, DHCP, gateway ICMP).
    Local(Vec<u8>),
    /// TCP or UDP the host NAT should carry.
    Forward,
}

pub(crate) fn classify(frame: &[u8]) -> Option<Ingress> {
    if frame.len() < 14 || frame.len() > MAX_FRAME {
        return None;
    }
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    if ethertype == ETHERTYPE_ARP {
        return arp_reply(frame).map(Ingress::Local);
    }
    if ethertype != ETHERTYPE_IPV4 {
        return None;
    }
    let ipv4 = parse_ipv4(frame)?;
    if ipv4.proto == 1 {
        return icmp_echo_reply(frame, &ipv4).map(Ingress::Local);
    }
    if ipv4.proto == 17 && udp_dest_port(&ipv4) == Some(67) {
        return dhcp_reply(frame, &ipv4).map(Ingress::Local);
    }
    if ipv4.proto == 6 || ipv4.proto == 17 {
        return Some(Ingress::Forward);
    }
    None
}

struct Ipv4<'a> {
    proto: u8,
    src: [u8; 4],
    dst: [u8; 4],
    payload: &'a [u8],
}

fn parse_ipv4(frame: &[u8]) -> Option<Ipv4<'_>> {
    if frame.len() < 34 {
        return None;
    }
    let version_ihl = frame[14];
    if version_ihl >> 4 != 4 {
        return None;
    }
    let ihl_words = version_ihl & 0x0f;
    let total_len = u16::from_be_bytes([frame[16], frame[17]]);
    if !ipv4_span_ok(frame.len(), ihl_words, total_len) {
        return None;
    }
    let ihl = usize::from(ihl_words) * 4;
    let payload_end = 14 + usize::from(total_len);
    Some(Ipv4 {
        proto: frame[23],
        src: frame[26..30].try_into().ok()?,
        dst: frame[30..34].try_into().ok()?,
        payload: &frame[14 + ihl..payload_end],
    })
}

fn arp_reply(frame: &[u8]) -> Option<Vec<u8>> {
    if frame.len() < 42 {
        return None;
    }
    let arp = &frame[14..42];
    if arp[0] != 0 || arp[1] != 1 || arp[2] != 8 || arp[3] != 0 || arp[4] != 6 || arp[5] != 4 {
        return None;
    }
    if u16::from_be_bytes([arp[6], arp[7]]) != 1 {
        return None;
    }
    let tpa: [u8; 4] = arp[24..28].try_into().ok()?;
    if tpa[0] != 10 || tpa[1] != 0 || tpa[2] != 2 || tpa == GUEST_IP {
        return None;
    }
    let sha: [u8; 6] = arp[8..14].try_into().ok()?;
    let spa: [u8; 4] = arp[14..18].try_into().ok()?;
    let mut reply = vec![0u8; 42];
    reply[0..6].copy_from_slice(&sha);
    reply[6..12].copy_from_slice(&GATEWAY_MAC);
    reply[12] = 0x08;
    reply[13] = 0x06;
    reply[14] = 0;
    reply[15] = 1;
    reply[16] = 8;
    reply[17] = 0;
    reply[18] = 6;
    reply[19] = 4;
    reply[20] = 0;
    reply[21] = 2;
    reply[22..28].copy_from_slice(&GATEWAY_MAC);
    reply[28..32].copy_from_slice(&tpa);
    reply[32..38].copy_from_slice(&sha);
    reply[38..42].copy_from_slice(&spa);
    Some(reply)
}

fn icmp_echo_reply(frame: &[u8], ipv4: &Ipv4<'_>) -> Option<Vec<u8>> {
    if ipv4.dst != GATEWAY_IP && ipv4.dst != DNS_IP {
        return None;
    }
    if ipv4.payload.len() < 8 || ipv4.payload[0] != 8 {
        return None;
    }
    let mut echo = ipv4.payload.to_vec();
    echo[0] = 0;
    echo[2] = 0;
    echo[3] = 0;
    let sum = internet_checksum(&echo);
    echo[2] = (sum >> 8) as u8;
    echo[3] = sum as u8;
    Some(wrap_guest(frame, ipv4_packet(ipv4.dst, ipv4.src, 1, &echo)))
}

fn udp_dest_port(ipv4: &Ipv4<'_>) -> Option<u16> {
    if ipv4.payload.len() < 4 {
        return None;
    }
    Some(u16::from_be_bytes([ipv4.payload[2], ipv4.payload[3]]))
}

fn dhcp_reply(frame: &[u8], ipv4: &Ipv4<'_>) -> Option<Vec<u8>> {
    let udp = ipv4.payload;
    if udp.len() < 8 + 240 {
        return None;
    }
    let bootp = &udp[8..];
    if bootp[0] != 1 || bootp[1] != 1 || bootp[2] != 6 {
        return None;
    }
    let kind = dhcp_message_type(bootp)?;
    let reply_kind = match kind {
        1 => 2, // DISCOVER -> OFFER
        3 => 5, // REQUEST -> ACK
        _ => return None,
    };
    let xid = &bootp[4..8];
    let chaddr: [u8; 6] = bootp[28..34].try_into().ok()?;
    let mut payload = vec![0u8; 8 + 300];
    payload[0] = 0;
    payload[1] = 67;
    payload[2] = 0;
    payload[3] = 68;
    let udp_len = payload.len() as u16;
    payload[4] = (udp_len >> 8) as u8;
    payload[5] = udp_len as u8;
    let bootp = &mut payload[8..];
    bootp[0] = 2;
    bootp[1] = 1;
    bootp[2] = 6;
    bootp[4..8].copy_from_slice(xid);
    bootp[16..20].copy_from_slice(&GUEST_IP);
    bootp[20..24].copy_from_slice(&GATEWAY_IP);
    bootp[28..34].copy_from_slice(&chaddr);
    bootp[236..240].copy_from_slice(&[0x63, 0x82, 0x53, 0x63]);
    let mut opt = 240;
    let options: &[&[u8]] = &[
        &[53, 1, reply_kind],
        &[54, 4, GATEWAY_IP[0], GATEWAY_IP[1], GATEWAY_IP[2], GATEWAY_IP[3]],
        &[51, 4, 0, 1, 0x51, 0x80],
        &[1, 4, 255, 255, 255, 0],
        &[3, 4, GATEWAY_IP[0], GATEWAY_IP[1], GATEWAY_IP[2], GATEWAY_IP[3]],
        &[6, 4, DNS_IP[0], DNS_IP[1], DNS_IP[2], DNS_IP[3]],
        &[255],
    ];
    for option in options {
        bootp[opt..opt + option.len()].copy_from_slice(option);
        opt += option.len();
    }
    let pseudo = transport_checksum(GATEWAY_IP, [255, 255, 255, 255], 17, &payload);
    payload[6] = (pseudo >> 8) as u8;
    payload[7] = pseudo as u8;
    let packet = ipv4_packet(GATEWAY_IP, [255, 255, 255, 255], 17, &payload);
    let mut reply = vec![0u8; 14 + packet.len()];
    reply[0..6].fill(0xff);
    reply[6..12].copy_from_slice(&GATEWAY_MAC);
    reply[12] = 0x08;
    reply[14..].copy_from_slice(&packet);
    let _ = frame;
    Some(reply)
}

fn dhcp_message_type(bootp: &[u8]) -> Option<u8> {
    if bootp.len() < 240 || bootp[236..240] != [0x63, 0x82, 0x53, 0x63] {
        return None;
    }
    let mut index = 240;
    while index < bootp.len() {
        let tag = bootp[index];
        if tag == 255 {
            break;
        }
        if tag == 0 {
            index += 1;
            continue;
        }
        if index + 1 >= bootp.len() {
            break;
        }
        let len = usize::from(bootp[index + 1]);
        if index + 2 + len > bootp.len() {
            break;
        }
        if tag == 53 && len >= 1 {
            return Some(bootp[index + 2]);
        }
        index += 2 + len;
    }
    None
}

pub(crate) struct ForwardHeader {
    pub guest_mac: [u8; 6],
    pub guest_ip: [u8; 4],
    pub proto: u8,
    pub src_port: u16,
    pub dst_port: u16,
    pub dst_ip: [u8; 4],
}

pub(crate) fn forward_header(frame: &[u8]) -> Option<(ForwardHeader, TcpView<'_>)> {
    let ipv4 = parse_ipv4(frame)?;
    if ipv4.payload.len() < 4 {
        return None;
    }
    let header = ForwardHeader {
        guest_mac: frame[6..12].try_into().ok()?,
        guest_ip: ipv4.src,
        proto: ipv4.proto,
        src_port: u16::from_be_bytes([ipv4.payload[0], ipv4.payload[1]]),
        dst_port: u16::from_be_bytes([ipv4.payload[2], ipv4.payload[3]]),
        dst_ip: ipv4.dst,
    };
    let tcp = if ipv4.proto == 6 {
        parse_tcp(ipv4.payload)?
    } else {
        TcpView {
            seq: 0,
            ack: 0,
            flags: 0,
            window: 0,
            window_scale: 0,
            mss: 1460,
            payload: &[],
        }
    };
    Some((header, tcp))
}

pub(crate) struct TcpView<'a> {
    pub seq: u32,
    pub ack: u32,
    pub flags: u8,
    pub window: u16,
    pub window_scale: u8,
    pub mss: u16,
    pub payload: &'a [u8],
}

fn parse_tcp(segment: &[u8]) -> Option<TcpView<'_>> {
    if segment.len() < 20 {
        return None;
    }
    let words = usize::from(segment[12] >> 4);
    if words < 5 || segment.len() < words * 4 {
        return None;
    }
    let flags = segment[13];
    Some(TcpView {
        seq: u32::from_be_bytes(segment[4..8].try_into().ok()?),
        ack: u32::from_be_bytes(segment[8..12].try_into().ok()?),
        flags,
        window: u16::from_be_bytes([segment[14], segment[15]]),
        window_scale: if flags & 0x02 != 0 {
            tcp_window_scale(segment)
        } else {
            0
        },
        mss: if flags & 0x02 != 0 {
            tcp_mss(segment)
        } else {
            1460
        },
        payload: &segment[words * 4..],
    })
}

fn tcp_window_scale(segment: &[u8]) -> u8 {
    let words = usize::from(segment[12] >> 4);
    let options = &segment[20..words * 4];
    let mut index = 0;
    while index < options.len() {
        match options[index] {
            0 => break,
            1 => index += 1,
            3 if index + 3 <= options.len() && options[index + 1] == 3 => {
                return options[index + 2].min(14);
            }
            _ => {
                if index + 1 >= options.len() {
                    break;
                }
                let len = usize::from(options[index + 1]);
                if len < 2 || index + len > options.len() {
                    break;
                }
                index += len;
            }
        }
    }
    0
}

fn tcp_mss(segment: &[u8]) -> u16 {
    let words = usize::from(segment[12] >> 4);
    let options = &segment[20..words * 4];
    let mut index = 0;
    while index < options.len() {
        match options[index] {
            0 => break,
            1 => index += 1,
            2 if index + 4 <= options.len() && options[index + 1] == 4 => {
                let mss = u16::from_be_bytes([options[index + 2], options[index + 3]]);
                return mss.clamp(536, 1460);
            }
            _ => {
                if index + 1 >= options.len() {
                    break;
                }
                let len = usize::from(options[index + 1]);
                if len < 2 || index + len > options.len() {
                    break;
                }
                index += len;
            }
        }
    }
    1460
}

/// Ethernet frame the guest transmits: source is the guest, destination is the gateway.
pub(crate) fn guest_tcp(
    guest_mac: [u8; 6],
    guest_ip: [u8; 4],
    guest_port: u16,
    dest_ip: [u8; 4],
    dest_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Option<Vec<u8>> {
    if payload.len() > 1460 {
        return None;
    }
    let mut tcp = vec![0u8; 20 + payload.len()];
    tcp[0..2].copy_from_slice(&guest_port.to_be_bytes());
    tcp[2..4].copy_from_slice(&dest_port.to_be_bytes());
    tcp[4..8].copy_from_slice(&seq.to_be_bytes());
    tcp[8..12].copy_from_slice(&ack.to_be_bytes());
    tcp[12] = 5 << 4;
    tcp[13] = flags;
    tcp[14..16].copy_from_slice(&65535u16.to_be_bytes());
    tcp[20..].copy_from_slice(payload);
    let sum = transport_checksum(guest_ip, dest_ip, 6, &tcp);
    tcp[16] = (sum >> 8) as u8;
    tcp[17] = sum as u8;
    let packet = ipv4_packet(guest_ip, dest_ip, 6, &tcp);
    let mut frame = Vec::with_capacity(14 + packet.len());
    frame.extend_from_slice(&GATEWAY_MAC);
    frame.extend_from_slice(&guest_mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.extend_from_slice(&packet);
    Some(frame)
}

pub(crate) fn guest_udp(
    guest_mac: [u8; 6],
    guest_ip: [u8; 4],
    guest_port: u16,
    dest_ip: [u8; 4],
    dest_port: u16,
    payload: &[u8],
) -> Option<Vec<u8>> {
    if payload.len() > 1400 {
        return None;
    }
    let mut udp = vec![0u8; 8 + payload.len()];
    udp[0..2].copy_from_slice(&guest_port.to_be_bytes());
    udp[2..4].copy_from_slice(&dest_port.to_be_bytes());
    let len = udp.len() as u16;
    udp[4..6].copy_from_slice(&len.to_be_bytes());
    udp[8..].copy_from_slice(payload);
    let sum = transport_checksum(guest_ip, dest_ip, 17, &udp);
    udp[6] = (sum >> 8) as u8;
    udp[7] = sum as u8;
    let packet = ipv4_packet(guest_ip, dest_ip, 17, &udp);
    let mut frame = Vec::with_capacity(14 + packet.len());
    frame.extend_from_slice(&GATEWAY_MAC);
    frame.extend_from_slice(&guest_mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.extend_from_slice(&packet);
    Some(frame)
}

pub(crate) fn tcp_ethernet(
    guest_mac: [u8; 6],
    guest_ip: [u8; 4],
    guest_port: u16,
    dest_ip: [u8; 4],
    dest_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Option<Vec<u8>> {
    if payload.len() > 1460 {
        return None;
    }
    let mut tcp = vec![0u8; 20 + payload.len()];
    tcp[0..2].copy_from_slice(&dest_port.to_be_bytes());
    tcp[2..4].copy_from_slice(&guest_port.to_be_bytes());
    tcp[4..8].copy_from_slice(&seq.to_be_bytes());
    tcp[8..12].copy_from_slice(&ack.to_be_bytes());
    tcp[13] = flags;
    tcp[14..16].copy_from_slice(&65535u16.to_be_bytes());
    if (flags & 0x02) != 0 && payload.is_empty() {
        tcp.resize(24, 0);
        tcp[0..2].copy_from_slice(&dest_port.to_be_bytes());
        tcp[2..4].copy_from_slice(&guest_port.to_be_bytes());
        tcp[4..8].copy_from_slice(&seq.to_be_bytes());
        tcp[8..12].copy_from_slice(&ack.to_be_bytes());
        tcp[12] = 6 << 4;
        tcp[13] = flags;
        tcp[14..16].copy_from_slice(&65535u16.to_be_bytes());
        tcp[20..24].copy_from_slice(&[2, 4, 0x05, 0xb4]);
    } else {
        tcp[12] = 5 << 4;
        tcp[20..].copy_from_slice(payload);
    }
    let sum = transport_checksum(dest_ip, guest_ip, 6, &tcp);
    tcp[16] = (sum >> 8) as u8;
    tcp[17] = sum as u8;
    Some(wrap_mac(
        guest_mac,
        ipv4_packet(dest_ip, guest_ip, 6, &tcp),
    ))
}

pub(crate) fn udp_ethernet(
    guest_mac: [u8; 6],
    guest_ip: [u8; 4],
    guest_port: u16,
    src_ip: [u8; 4],
    src_port: u16,
    payload: &[u8],
) -> Option<Vec<u8>> {
    if payload.len() > 1400 {
        return None;
    }
    let mut udp = vec![0u8; 8 + payload.len()];
    udp[0..2].copy_from_slice(&src_port.to_be_bytes());
    udp[2..4].copy_from_slice(&guest_port.to_be_bytes());
    let len = udp.len() as u16;
    udp[4..6].copy_from_slice(&len.to_be_bytes());
    udp[8..].copy_from_slice(payload);
    let sum = transport_checksum(src_ip, guest_ip, 17, &udp);
    udp[6] = (sum >> 8) as u8;
    udp[7] = sum as u8;
    Some(wrap_mac(guest_mac, ipv4_packet(src_ip, guest_ip, 17, &udp)))
}

pub(crate) fn udp_payload(frame: &[u8]) -> Option<&[u8]> {
    let ipv4 = parse_ipv4(frame)?;
    if ipv4.proto != 17 || ipv4.payload.len() < 8 {
        return None;
    }
    let len = usize::from(u16::from_be_bytes([ipv4.payload[4], ipv4.payload[5]]));
    if len < 8 || len > ipv4.payload.len() {
        return None;
    }
    Some(&ipv4.payload[8..len])
}

/// QNAME from a DNS query. AAAA and other types still return the name so the
/// NAT can send an empty answer instead of dropping the datagram.
pub(crate) fn dns_qname(query: &[u8]) -> Option<String> {
    if query.len() < 12 {
        return None;
    }
    let mut index = 12;
    let mut labels = Vec::new();
    for _ in 0..16 {
        if index >= query.len() {
            return None;
        }
        let len = query[index] as usize;
        if len == 0 {
            return Some(labels.join("."));
        }
        if query[index] & 0xc0 == 0xc0 {
            return None;
        }
        if len > 63 || index + 1 + len >= query.len() {
            return None;
        }
        labels.push(std::str::from_utf8(&query[index + 1..index + 1 + len]).ok()?.to_string());
        index += 1 + len;
    }
    None
}

/// DNS response payload (not Ethernet). Empty `addrs` is NOERROR/NODATA.
pub(crate) fn dns_a_reply(query: &[u8], addrs: &[[u8; 4]]) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    let mut question_end = 12;
    for _ in 0..16 {
        if question_end >= query.len() {
            return None;
        }
        let len = query[question_end] as usize;
        if len == 0 {
            question_end += 5; // 0 + type + class
            break;
        }
        if query[question_end] & 0xc0 == 0xc0 {
            return None;
        }
        question_end += 1 + len;
    }
    if question_end > query.len() {
        return None;
    }
    let mut out = query[..question_end].to_vec();
    out[2] = 0x81;
    out[3] = 0x80;
    let ancount = addrs.len().min(8) as u16;
    out[6..8].copy_from_slice(&ancount.to_be_bytes());
    out[8..12].copy_from_slice(&[0, 0, 0, 0]);
    for addr in addrs.iter().take(8) {
        out.extend_from_slice(&[0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04]);
        out.extend_from_slice(addr);
    }
    Some(out)
}

fn wrap_guest(request: &[u8], packet: Vec<u8>) -> Vec<u8> {
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&request[6..12]);
    wrap_mac(mac, packet)
}

fn wrap_mac(dst: [u8; 6], packet: Vec<u8>) -> Vec<u8> {
    let mut frame = Vec::with_capacity(60.max(14 + packet.len()));
    frame.extend_from_slice(&dst);
    frame.extend_from_slice(&GATEWAY_MAC);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.extend_from_slice(&packet);
    // Do not pad with zeros past the IPv4 total length. Linux virtio-net
    // with mergeable RX has accepted short frames for UDP DNS; padding TCP
    // SYN-ACK to 60 made auth28 show synack:1 + rx delivers but curl (28).
    frame
}

fn ipv4_packet(src: [u8; 4], dst: [u8; 4], proto: u8, payload: &[u8]) -> Vec<u8> {
    let total = 20 + payload.len();
    let mut header = vec![0u8; 20 + payload.len()];
    header[0] = 0x45;
    let total = total as u16;
    header[2] = (total >> 8) as u8;
    header[3] = total as u8;
    let id = IP_ID.fetch_add(1, Ordering::Relaxed);
    header[4] = (id >> 8) as u8;
    header[5] = id as u8;
    header[6] = 0x40;
    header[8] = 64;
    header[9] = proto;
    header[12..16].copy_from_slice(&src);
    header[16..20].copy_from_slice(&dst);
    header[20..].copy_from_slice(payload);
    let sum = internet_checksum(&header[..20]);
    header[10] = (sum >> 8) as u8;
    header[11] = sum as u8;
    header
}

fn transport_checksum(src: [u8; 4], dst: [u8; 4], proto: u8, segment: &[u8]) -> u16 {
    let mut buf = Vec::with_capacity(12 + segment.len() + 1);
    buf.extend_from_slice(&src);
    buf.extend_from_slice(&dst);
    buf.extend_from_slice(&[0, proto]);
    buf.extend_from_slice(&(segment.len() as u16).to_be_bytes());
    buf.extend_from_slice(segment);
    internet_checksum(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_of_a_filled_header_is_zero() {
        let mut header = vec![
            0x45, 0, 0, 20, 0, 1, 0x40, 0, 64, 1, 0, 0, 10, 0, 2, 2, 10, 0, 2, 15,
        ];
        let sum = internet_checksum(&header);
        header[10] = (sum >> 8) as u8;
        header[11] = sum as u8;
        assert_eq!(internet_checksum(&header), 0);
    }

    #[test]
    fn syn_ack_tcp_and_ip_checksums_verify() {
        let dest = [151, 101, 21, 91];
        let frame = tcp_ethernet(GUEST_MAC, GUEST_IP, 33356, dest, 443, 0x1000_0000, 2001, 0x12, &[])
            .expect("SYN-ACK frame");
        assert_eq!(&frame[0..6], &GUEST_MAC);
        assert_eq!(&frame[6..12], &GATEWAY_MAC);
        let ip = &frame[14..];
        let total = u16::from_be_bytes([ip[2], ip[3]]) as usize;
        assert_eq!(internet_checksum(&ip[..20]), 0);
        let tcp = &ip[20..total];
        let mut pseudo = Vec::new();
        pseudo.extend_from_slice(&dest);
        pseudo.extend_from_slice(&GUEST_IP);
        pseudo.extend_from_slice(&[0, 6]);
        pseudo.extend_from_slice(&(tcp.len() as u16).to_be_bytes());
        pseudo.extend_from_slice(tcp);
        assert_eq!(internet_checksum(&pseudo), 0);
        assert_eq!(tcp[12] >> 4, 6, "MSS option data offset");
        assert!(frame.len() >= 14 + total);
    }

    #[test]
    fn tcp_payload_checksum_verifies() {
        let dest = [151, 101, 21, 91];
        let payload = b"GET /nix-cache-info HTTP/1.1\r\nHost: cache.nixos.org\r\n\r\n";
        let frame = tcp_ethernet(
            GUEST_MAC, GUEST_IP, 33290, dest, 443, 0x1000_0001, 2002, 0x18, payload,
        )
        .expect("data frame");
        let ip = &frame[14..];
        let total = u16::from_be_bytes([ip[2], ip[3]]) as usize;
        assert_eq!(internet_checksum(&ip[..20]), 0);
        let tcp = &ip[20..total];
        assert_eq!(tcp[12] >> 4, 5);
        assert_eq!(&tcp[20..], payload);
        let mut pseudo = Vec::new();
        pseudo.extend_from_slice(&dest);
        pseudo.extend_from_slice(&GUEST_IP);
        pseudo.extend_from_slice(&[0, 6]);
        pseudo.extend_from_slice(&(tcp.len() as u16).to_be_bytes());
        pseudo.extend_from_slice(tcp);
        assert_eq!(internet_checksum(&pseudo), 0);
    }

    #[test]
    fn arp_for_the_gateway_is_answered() {
        let mut request = vec![0u8; 42];
        request[0..6].copy_from_slice(&GATEWAY_MAC);
        request[6..12].copy_from_slice(&GUEST_MAC);
        request[12] = 0x08;
        request[13] = 0x06;
        request[15] = 1;
        request[16] = 8;
        request[18] = 6;
        request[19] = 4;
        request[21] = 1;
        request[22..28].copy_from_slice(&GUEST_MAC);
        request[28..32].copy_from_slice(&GUEST_IP);
        request[38..42].copy_from_slice(&GATEWAY_IP);
        let Ingress::Local(reply) = classify(&request).unwrap() else {
            panic!("gateway ARP must be local");
        };
        assert_eq!(&reply[0..6], &GUEST_MAC);
        assert_eq!(&reply[22..28], &GATEWAY_MAC);
        assert_eq!(&reply[28..32], &GATEWAY_IP);
        assert_eq!(u16::from_be_bytes([reply[20], reply[21]]), 2);
    }

    #[test]
    fn dhcp_discover_offers_the_slirp_address() {
        let mut frame = vec![0u8; 14 + 20 + 8 + 240 + 8];
        frame[12] = 0x08;
        frame[14] = 0x45;
        let total = (frame.len() - 14) as u16;
        frame[16] = (total >> 8) as u8;
        frame[17] = total as u8;
        frame[23] = 17;
        frame[33] = 68;
        frame[37] = 67;
        let bootp = &mut frame[14 + 20 + 8..];
        bootp[0] = 1;
        bootp[1] = 1;
        bootp[2] = 6;
        bootp[4] = 0x11;
        bootp[236..240].copy_from_slice(&[0x63, 0x82, 0x53, 0x63]);
        bootp[240..243].copy_from_slice(&[53, 1, 1]);
        bootp[243] = 255;
        let Ingress::Local(reply) = classify(&frame).unwrap() else {
            panic!("DHCP discover must be answered locally");
        };
        let yiaddr = &reply[14 + 20 + 8 + 16..14 + 20 + 8 + 20];
        assert_eq!(yiaddr, &GUEST_IP);
        assert!(reply.windows(3).any(|window| window == [53, 1, 2]));
        assert!(reply.windows(6).any(|window| window == [6, 4, 10, 0, 2, 3]));
    }

    #[test]
    fn dns_a_reply_answers_the_question() {
        let mut query = vec![0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
        for label in ["channels", "nixos", "org"] {
            query.push(label.len() as u8);
            query.extend_from_slice(label.as_bytes());
        }
        query.extend_from_slice(&[0, 0, 1, 0, 1]);
        assert_eq!(dns_qname(&query).as_deref(), Some("channels.nixos.org"));
        let reply = dns_a_reply(&query, &[[1, 2, 3, 4]]).unwrap();
        assert_eq!(&reply[0..2], &[0x12, 0x34]);
        assert_eq!(reply[2] & 0x80, 0x80);
        assert_eq!(&reply[reply.len() - 4..], &[1, 2, 3, 4]);
    }

    #[test]
    fn span_helper_rejects_a_header_past_the_frame() {
        assert!(!ipv4_span_ok(20, 5, 40));
        assert!(ipv4_span_ok(14 + 20, 5, 20));
    }

    #[test]
    fn page_sized_virtio_descriptor_keeps_the_ipv4_frame() {
        let payload = b"dns";
        let frame = guest_udp(GUEST_MAC, GUEST_IP, 53000, DNS_IP, 53, payload).unwrap();
        let mut page = vec![0u8; 10 + 4096];
        page[10..10 + frame.len()].copy_from_slice(&frame);
        let parsed = ethernet_from_virtio(&page).expect("padded virtio buffer");
        assert_eq!(parsed, frame.as_slice());
    }

    #[test]
    fn mergeable_rx_header_keeps_one_buffer_and_the_frame() {
        let frame = guest_udp(GUEST_MAC, GUEST_IP, 53000, DNS_IP, 53, b"dns").unwrap();
        let packet = rx_virtio_packet(&frame, VIRTIO_F_MRG_RXBUF);
        assert_eq!(u16::from_le_bytes([packet[10], packet[11]]), 1);
        assert_eq!(ethernet_from_virtio(&packet).unwrap(), frame.as_slice());
        assert!(matches!(classify(&frame), Some(Ingress::Forward)));
    }
}
