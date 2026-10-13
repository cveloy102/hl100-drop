# HL100 Drop protocol v1

All file transfers use IPv4 TCP directly between devices in the same interface subnet. Loopback is allowed for tests. No account, pairing, TLS, compression, relay, cloud transfer or public-address fallback. IPv6 and routed cross-subnet networks are outside v1.

Discovery: UDP port 41001, multicast 239.255.100.100 with TTL=1 and interface subnet broadcast. Announcements every 2 seconds, expiry after 7 seconds. Example:

```json
{"app":"hl100-drop","protocol":1,"id":"persistent-device-id","name":"My PC","port":41000,"platform":"windows"}
```

Default TCP port: 41000. Control frame: 4-byte unsigned big-endian JSON byte length followed by UTF-8 JSON (limit 4 MiB). Sender opens one TCP connection per batch:

1. Offer: `{protocol:1,id,name,transfer,entries:[{path,size,directory}]}`. Paths use `/`; directories have size 0.
2. Receiver replies `{ok:true}` or `{ok:false,error}`.
3. For each non-directory entry in order, sender writes exactly `size` raw bytes. Receiver saves, closes, finalizes the file and sends a control ACK. Directory entries carry no bytes or per-entry ACK.
4. Receiver sends a final ACK after the complete manifest. Sender reports completion only after reading it.

Receiver uses a local transfer ID, preserves existing files, reserves a collision-free top-level name, writes partial files and renames each finished file. On failure, completed files remain and unfinished partial files are removed. Empty directories are preserved. Invalid relative paths are rejected to prevent writes outside the chosen receive directory. Symbolic links are not sent.

Streaming buffer: 1 MiB. Progress events no more often than every 150 ms. Cancellation stops the next read/write iteration; a blocked network operation has a 15-second timeout. Resume is not implemented in v1; retry starts a new transfer.

## C ABI

`hl100_command` returns an owned UTF-8 JSON string; release with `hl100_free`. Commands:

```json
{"op":"start","config":{"id":"persistent-id","name":"PC","folder":"receive-path","port":41000}}
{"op":"poll"}
{"op":"send","address":"192.168.1.2","port":41000,"name":"Phone","paths":["C:/files/report.pdf"]}
{"op":"cancel","transfer":"local-transfer-id"}
{"op":"stop"}
```

`poll` returns `peers`, `events`, `busy`. Events: `started`, `transfer`, `progress`, `complete`, `error`. `complete.status` is `done`, `cancelled` or `error`. Commands return `{ok:false,error}` on failure. Changing settings is allowed only when no transfers are active.

Android uses the same engine via JNI. Its send manifest additionally carries a content URI per source. JNI calls `openSource` lazily and writes directly to SAF descriptors returned by `openReceive`; `finishReceive` renames partial documents. No staging copy is required.
