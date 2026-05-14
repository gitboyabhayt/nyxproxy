# Wireshark / pcap export (Feature GG)

NyxProxy can serialise its captured history to a libpcap 2.4 file
(Ethernet linktype = 1) that Wireshark, tshark, and every pcap-aware
tool understand.

## What gets written

Each `HttpFlow` becomes one **synthetic** TCP stream so the resulting
pcap is browsable by *Follow TCP Stream* in Wireshark:

1. SYN
2. SYN/ACK
3. ACK
4. PSH/ACK carrying the raw HTTP/1.1 request bytes
5. PSH/ACK carrying the raw HTTP/1.1 response bytes (if present)
6. FIN/ACK (client → server)
7. FIN/ACK (server → client)

Total **7 frames per flow**.

## Bytes, not bits

We never observe the wire — we generate Ethernet, IPv4, and TCP
headers around the captured HTTP bytes. This is enough for Wireshark's
HTTP dissector to colourise the request/response cleanly. Limitations:

- **IPv4 only.** Source IPs come from `10.0.0.x` (client) and `10.0.1.x`
  (server). They are *not* real IPs.
- **No TLS framing.** HTTPS flows are emitted as cleartext HTTP on
  port 443 — Wireshark still dissects them via the HTTP dissector.
- **Timestamps.** The first frame uses the flow's `started_at`; the
  six subsequent frames are spaced 1 ms apart so Wireshark's view is
  visually ordered.

If you need real packet captures, run `tcpdump`. This export is for
sharing a session into Wireshark-driven workflows (forensic review,
filing tickets, archival).

## UI

Open **Project options** → *Export as pcap*. The picker writes to the
path you choose. The return value is the number of flows written.

## Programmatic access

```rust
use nyxproxy_core::pcap::write_pcap;

let bytes = write_pcap(&flows)?;
std::fs::write("session.pcap", bytes)?;
```

## Tested

```
cargo test -p nyxproxy-core --lib pcap
```

4 tests covering the pcap magic, frame count per flow, payload-bytes
roundtrip (request and response lines visible in the file), and the
client/server IP-pool separation.
