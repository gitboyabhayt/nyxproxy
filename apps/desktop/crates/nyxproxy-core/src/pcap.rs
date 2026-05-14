//! Wireshark / pcap export (Feature GG).
//!
//! Writes captured [`HttpFlow`] traffic out as a classic `.pcap` file
//! (libpcap 2.4, Ethernet linktype = 1) that Wireshark, tshark, and
//! every pcap-aware tool understand. The frames are synthetic — we
//! never observe the wire — but we manufacture a minimum-correctness
//! TCP+IP+Ethernet stack so Wireshark dissectors run on the HTTP
//! payload exactly as if we'd captured the real bytes:
//!
//! 1. Three handshake frames (SYN, SYN/ACK, ACK).
//! 2. Request frame: TCP PSH/ACK carrying the raw HTTP/1.1 request.
//! 3. Response frame: TCP PSH/ACK carrying the raw HTTP/1.1 response.
//! 4. Connection teardown (FIN/ACK both ways).
//!
//! Each [`HttpFlow`] becomes its own TCP stream so the resulting pcap
//! is browsable by "Follow TCP Stream" in Wireshark out of the box.
//!
//! Limitations:
//! * IPv4 only; the synthetic source/destination IPs come from a
//!   `10.0.0.x` pool (client) and a `10.0.1.x` pool (server). They
//!   are NOT the real IPs.
//! * TLS bytes are not synthesised. For HTTPS flows we emit cleartext
//!   HTTP framing on port 443 — Wireshark will still show the request
//!   bytes and headers via HTTP dissector.
//! * If you need real packet captures, run `tcpdump`. This export is
//!   for sharing a session into Wireshark-driven workflows.

use std::io::Write;

use base64::Engine;

use crate::error::NyxResult;
use crate::model::{CapturedRequest, CapturedResponse, HeaderEntry, HttpFlow};

const LINKTYPE_ETHERNET: u32 = 1;
const PCAP_MAGIC: u32 = 0xa1b2c3d4;
const PCAP_VERSION_MAJOR: u16 = 2;
const PCAP_VERSION_MINOR: u16 = 4;
const SNAPLEN: u32 = 65535;

const TCP_SYN: u8 = 0x02;
const TCP_ACK: u8 = 0x10;
const TCP_PSH: u8 = 0x08;
const TCP_FIN: u8 = 0x01;

/// Serialise all `flows` to a pcap buffer.
pub fn write_pcap(flows: &[HttpFlow]) -> NyxResult<Vec<u8>> {
    let mut out = Vec::with_capacity(64 * 1024);
    write_global_header(&mut out)?;
    for (i, flow) in flows.iter().enumerate() {
        let stream_index = i as u32;
        let client_ip = ipv4_for_stream(stream_index, true);
        let server_ip = ipv4_for_stream(stream_index, false);
        let server_port: u16 = if flow.request.scheme == "https" { 443 } else { 80 };
        let client_port: u16 = 40000u16.wrapping_add((stream_index % 20000) as u16);

        // Sequence numbers — start at 0, grow with payload length.
        let mut client_seq: u32 = 1;
        let mut server_seq: u32 = 1;
        let base_ts = flow.started_at.timestamp_millis().max(0) as u64;

        // SYN
        emit_frame(
            &mut out,
            base_ts,
            0,
            client_ip,
            server_ip,
            client_port,
            server_port,
            client_seq,
            0,
            TCP_SYN,
            &[],
        )?;
        client_seq = client_seq.wrapping_add(1);
        // SYN/ACK
        emit_frame(
            &mut out,
            base_ts,
            1,
            server_ip,
            client_ip,
            server_port,
            client_port,
            server_seq,
            client_seq,
            TCP_SYN | TCP_ACK,
            &[],
        )?;
        server_seq = server_seq.wrapping_add(1);
        // ACK
        emit_frame(
            &mut out,
            base_ts,
            2,
            client_ip,
            server_ip,
            client_port,
            server_port,
            client_seq,
            server_seq,
            TCP_ACK,
            &[],
        )?;

        // Request payload
        let req_bytes = serialise_request(&flow.request);
        emit_frame(
            &mut out,
            base_ts,
            3,
            client_ip,
            server_ip,
            client_port,
            server_port,
            client_seq,
            server_seq,
            TCP_PSH | TCP_ACK,
            &req_bytes,
        )?;
        client_seq = client_seq.wrapping_add(req_bytes.len() as u32);

        // Response payload (if present)
        if let Some(resp) = &flow.response {
            let resp_bytes = serialise_response(resp);
            emit_frame(
                &mut out,
                base_ts,
                4,
                server_ip,
                client_ip,
                server_port,
                client_port,
                server_seq,
                client_seq,
                TCP_PSH | TCP_ACK,
                &resp_bytes,
            )?;
            server_seq = server_seq.wrapping_add(resp_bytes.len() as u32);
        }

        // FIN/ACK both ways
        emit_frame(
            &mut out,
            base_ts,
            5,
            client_ip,
            server_ip,
            client_port,
            server_port,
            client_seq,
            server_seq,
            TCP_FIN | TCP_ACK,
            &[],
        )?;
        emit_frame(
            &mut out,
            base_ts,
            6,
            server_ip,
            client_ip,
            server_port,
            client_port,
            server_seq,
            client_seq.wrapping_add(1),
            TCP_FIN | TCP_ACK,
            &[],
        )?;
    }
    Ok(out)
}

fn write_global_header<W: Write>(w: &mut W) -> NyxResult<()> {
    w.write_all(&PCAP_MAGIC.to_le_bytes())?;
    w.write_all(&PCAP_VERSION_MAJOR.to_le_bytes())?;
    w.write_all(&PCAP_VERSION_MINOR.to_le_bytes())?;
    w.write_all(&0_i32.to_le_bytes())?; // thiszone
    w.write_all(&0_u32.to_le_bytes())?; // sigfigs
    w.write_all(&SNAPLEN.to_le_bytes())?;
    w.write_all(&LINKTYPE_ETHERNET.to_le_bytes())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_frame<W: Write>(
    w: &mut W,
    base_ts_ms: u64,
    offset_ms: u64,
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> NyxResult<()> {
    let frame = build_frame(src_ip, dst_ip, src_port, dst_port, seq, ack, flags, payload);
    let total_ms = base_ts_ms + offset_ms;
    let ts_sec = (total_ms / 1000) as u32;
    let ts_usec = ((total_ms % 1000) * 1000) as u32;
    w.write_all(&ts_sec.to_le_bytes())?;
    w.write_all(&ts_usec.to_le_bytes())?;
    let cap_len = frame.len() as u32;
    w.write_all(&cap_len.to_le_bytes())?;
    w.write_all(&cap_len.to_le_bytes())?;
    w.write_all(&frame)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_frame(
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    // Ethernet header (14 bytes): dst MAC, src MAC, ethertype 0x0800.
    let mut frame = Vec::with_capacity(14 + 20 + 20 + payload.len());
    frame.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x02]); // dst
    frame.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]); // src
    frame.extend_from_slice(&[0x08, 0x00]); // ethertype IPv4

    // IPv4 + TCP
    let tcp_payload_len = payload.len();
    let tcp_len = 20 + tcp_payload_len;
    let ip_total_len = 20 + tcp_len;
    let ip_header_start = frame.len();
    frame.extend_from_slice(&[0x45, 0x00]); // version+IHL, DSCP
    frame.extend_from_slice(&(ip_total_len as u16).to_be_bytes());
    frame.extend_from_slice(&[0x00, 0x00, 0x40, 0x00]); // ID, flags
    frame.extend_from_slice(&[64, 6]); // TTL, proto=TCP
    frame.extend_from_slice(&[0, 0]); // checksum placeholder
    frame.extend_from_slice(&src_ip);
    frame.extend_from_slice(&dst_ip);
    // Compute and write IPv4 checksum
    let ip_checksum = checksum16(&frame[ip_header_start..ip_header_start + 20]);
    frame[ip_header_start + 10..ip_header_start + 12].copy_from_slice(&ip_checksum.to_be_bytes());

    // TCP header
    let tcp_header_start = frame.len();
    frame.extend_from_slice(&src_port.to_be_bytes());
    frame.extend_from_slice(&dst_port.to_be_bytes());
    frame.extend_from_slice(&seq.to_be_bytes());
    frame.extend_from_slice(&ack.to_be_bytes());
    frame.extend_from_slice(&[0x50, flags]); // data offset (5*4=20), flags
    frame.extend_from_slice(&65535_u16.to_be_bytes()); // window
    frame.extend_from_slice(&[0, 0]); // checksum placeholder (we leave 0 — Wireshark accepts)
    frame.extend_from_slice(&[0, 0]); // urgent ptr
    frame.extend_from_slice(payload);
    // Compute pseudo-header TCP checksum
    let tcp_checksum = tcp_checksum(&src_ip, &dst_ip, &frame[tcp_header_start..]);
    frame[tcp_header_start + 16..tcp_header_start + 18]
        .copy_from_slice(&tcp_checksum.to_be_bytes());

    frame
}

fn checksum16(bytes: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        sum = sum.wrapping_add(u16::from_be_bytes([bytes[i], bytes[i + 1]]) as u32);
        i += 2;
    }
    if i < bytes.len() {
        sum = sum.wrapping_add((bytes[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn tcp_checksum(src: &[u8; 4], dst: &[u8; 4], tcp_segment: &[u8]) -> u16 {
    let mut pseudo = Vec::with_capacity(12 + tcp_segment.len());
    pseudo.extend_from_slice(src);
    pseudo.extend_from_slice(dst);
    pseudo.extend_from_slice(&[0, 6]); // zero + protocol=TCP
    pseudo.extend_from_slice(&(tcp_segment.len() as u16).to_be_bytes());
    pseudo.extend_from_slice(tcp_segment);
    if pseudo.len() % 2 == 1 {
        pseudo.push(0);
    }
    checksum16(&pseudo)
}

fn ipv4_for_stream(index: u32, client: bool) -> [u8; 4] {
    // 10.0.0.x for clients, 10.0.1.x for servers. Wrap at /24 boundaries
    // and increment the third octet to keep streams distinct.
    let third = if client { 0 } else { 1 };
    let stream_third = (index / 250) as u8;
    let host = 1 + (index % 250) as u8;
    [10, 0, third + stream_third * 2, host]
}

fn serialise_request(req: &CapturedRequest) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "{} {} {}\r\n",
            req.method,
            if req.path.is_empty() { "/" } else { &req.path },
            req.http_version
        )
        .as_bytes(),
    );
    // Always emit a Host header if missing.
    let mut have_host = false;
    for h in &req.headers {
        if h.name.eq_ignore_ascii_case("host") {
            have_host = true;
        }
        out.extend_from_slice(format!("{}: {}\r\n", h.name, h.value).as_bytes());
    }
    if !have_host {
        out.extend_from_slice(format!("Host: {}\r\n", req.authority).as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&decode_body(&req.body_b64));
    out
}

fn serialise_response(resp: &CapturedResponse) -> Vec<u8> {
    let mut out = Vec::new();
    let reason = if resp.reason.is_empty() {
        default_reason(resp.status).to_string()
    } else {
        resp.reason.clone()
    };
    out.extend_from_slice(
        format!("{} {} {}\r\n", resp.http_version, resp.status, reason).as_bytes(),
    );
    for h in &resp.headers {
        out.extend_from_slice(format!("{}: {}\r\n", h.name, h.value).as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&decode_body(&resp.body_b64));
    out
}

fn decode_body(b64: &str) -> Vec<u8> {
    if b64.is_empty() {
        return Vec::new();
    }
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap_or_default()
}

fn default_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "OK",
    }
}

// Keep an explicit re-export so external users (and tests) don't need to
// pull `HeaderEntry` from `crate::model`.
pub use crate::model::HeaderEntry as PcapHeader;

// Silence dead-code warning for HeaderEntry import on toolchains that
// don't see the body_b64 path used.
#[allow(dead_code)]
fn _kept_alive() -> Option<HeaderEntry> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CapturedRequest, CapturedResponse, HeaderEntry, HttpFlow};
    use base64::Engine;

    fn sample_flow() -> HttpFlow {
        let body = b"hi";
        let req = CapturedRequest {
            method: "GET".into(),
            url: "http://example.com/".into(),
            scheme: "http".into(),
            authority: "example.com".into(),
            path: "/".into(),
            http_version: "HTTP/1.1".into(),
            headers: vec![HeaderEntry::new("user-agent", "nyxproxy-test")],
            body_b64: String::new(),
            body_size: 0,
        };
        let mut flow = HttpFlow::new(req);
        flow.response = Some(CapturedResponse {
            status: 200,
            http_version: "HTTP/1.1".into(),
            reason: "OK".into(),
            headers: vec![HeaderEntry::new("content-type", "text/plain")],
            body_b64: base64::engine::general_purpose::STANDARD.encode(body),
            body_size: body.len(),
            elapsed_ms: 12,
        });
        flow
    }

    #[test]
    fn header_magic_and_linktype_are_correct() {
        let bytes = write_pcap(&[sample_flow()]).unwrap();
        assert!(bytes.len() > 24);
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        assert_eq!(magic, PCAP_MAGIC);
        let linktype = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        assert_eq!(linktype, LINKTYPE_ETHERNET);
    }

    #[test]
    fn emits_seven_frames_per_flow() {
        // 3 handshake + req + resp + 2 fin = 7 frames per flow.
        let bytes = write_pcap(&[sample_flow()]).unwrap();
        let frames = count_frames(&bytes);
        assert_eq!(frames, 7);
    }

    #[test]
    fn frames_contain_http_payload_bytes() {
        let bytes = write_pcap(&[sample_flow()]).unwrap();
        // The HTTP request line must be present somewhere in the file.
        let needle = b"GET / HTTP/1.1\r\n";
        let found = bytes.windows(needle.len()).any(|w| w == needle);
        assert!(found, "expected request line in pcap bytes");
        let needle2 = b"HTTP/1.1 200 OK\r\n";
        let found2 = bytes.windows(needle2.len()).any(|w| w == needle2);
        assert!(found2, "expected response status line in pcap bytes");
    }

    #[test]
    fn ipv4_pool_separates_client_and_server() {
        let c = ipv4_for_stream(0, true);
        let s = ipv4_for_stream(0, false);
        assert_eq!(c[..3], [10, 0, 0]);
        assert_eq!(s[..3], [10, 0, 1]);
        assert_ne!(c, s);
    }

    fn count_frames(bytes: &[u8]) -> usize {
        let mut offset = 24usize; // skip global header
        let mut count = 0usize;
        while offset + 16 <= bytes.len() {
            let cap_len =
                u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;
            offset += 16 + cap_len;
            count += 1;
        }
        count
    }
}
