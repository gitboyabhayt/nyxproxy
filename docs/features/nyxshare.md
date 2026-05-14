# Encrypted evidence packs `.nyxshare` (Leapfrog #8)

Package a slice of the current capture session (flows + issues + a manifest)
into a single binary file that can be safely shared with another tester or
attached to a report.

## File format

```
+--------+---------+--------+---------+---------------------+
| MAGIC  | VERSION |  SALT  |  NONCE  | ChaCha20-Poly1305   |
| 8 byt  | 1 byte  | 16 byt | 12 byt  | ciphertext (zstd-   |
| NYXSHARE        |        |         | compressed JSON)    |
+--------+---------+--------+---------+---------------------+
```

- **Magic** `NYXSHARE` — header marker so we can reject the wrong file type
  before attempting key derivation.
- **Version** — currently `1`. Reserved for future format upgrades.
- **Salt** — 16 random bytes for Argon2id KDF.
- **Nonce** — 12 random bytes used once per pack for ChaCha20-Poly1305.
- **Ciphertext** — payload (manifest + flows + issues) is JSON-encoded,
  zstd-compressed at level 3, then encrypted with ChaCha20-Poly1305 AEAD.

## Threat model

- The password is the only secret. We use Argon2id with `m_cost=64 MiB`,
  `t_cost=3`, `p=1` (recommended OWASP 2024 settings) so dictionary attacks
  on weak passwords are slow even on GPU.
- The header is fully authenticated (ChaCha20-Poly1305 is authenticated
  encryption). Any tampering — flipping a salt byte, swapping the magic,
  truncating the ciphertext — fails decryption with `decryption failed
  (wrong password or corrupted)`.
- A user who only sees the ciphertext cannot learn the manifest note, the
  number of flows, or even the exact uncompressed size beyond a small
  zstd-bound.

## Tests

`apps/desktop/crates/nyxproxy-core/src/nyxshare.rs` ships six tests:
round-trip, wrong-password rejection, truncated header rejection, bad-magic
rejection, empty-password rejection, header-layout sanity.

## Related Tauri commands

- `share_seal_cmd` — bundles the current history slice into bytes ready to
  write to disk.
- `share_unseal_cmd` — reverses the operation and returns the decoded
  payload.
- `write_bytes_cmd` / `read_bytes_cmd` — small helpers to persist the binary
  blob from the React layer without depending on a Tauri FS plugin.

## Why not just zip?

Plain zip is not authenticated — corrupted bytes silently decode to garbage,
and AES-encrypted zip variants have known padding-oracle issues. Pinning the
format to AEAD + Argon2id gives us forensic-grade guarantees: if a recipient
can open the pack, the contents are byte-identical to what we sealed.
