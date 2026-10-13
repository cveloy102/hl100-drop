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

Streaming buffer: 1 MiB. Progress events no more often than every 150 ms. Cancellation stops the next read/write iteration; a blocked network operation has a 15-second timeout. Legacy v1 retries start a new transfer. Version 1.1 optionally negotiates persistent resumption as described below.

## C ABI

`hl100_command` returns an owned UTF-8 JSON string; release with `hl100_free`. Commands:

```json
{"op":"start","config":{"id":"persistent-id","name":"PC","folder":"receive-path","port":41000}}
{"op":"poll"}
{"op":"send","address":"192.168.1.2","port":41000,"name":"Phone","paths":["C:/files/report.pdf"]}
{"op":"cancel","transfer":"local-transfer-id"}
{"op":"stop"}
```

`poll` returns `peers`, `events`, `busy`. Events: `started`, `transfer`, `file`, `progress`, `complete`, `error`. `complete.status` is `done`, `cancelled` or `error`. Commands return `{ok:false,error}` on failure. Changing settings is allowed only when no transfers are active.

Android uses the same engine via JNI. Its send manifest additionally carries a content URI per source. JNI calls `openSource` lazily and writes directly to SAF descriptors returned by `openReceive`; `finishReceive` renames partial documents. No staging copy is required.

### Local file metadata

`transfer` includes `entries:[{path,size,directory}]` for the first 200 manifest
entries, in manifest order; `files` still counts the full manifest and `total`
remains the full batch byte count. This limit bounds polling payloads and history.
The receiver emits `file` for each of those entries after successfully creating
its directory or finalizing its file. It contains `transfer`, `path`, `size`,
`directory` and `local_path`. Disk storage returns the actual collision-resolved
path; storage without filesystem paths (Android SAF) returns null. An item whose
save fails has no `file` event. Previously finalized files remain available even
if the batch later fails. Capture the path before `finish`, which releases the
partial-file record, and publish it only after a successful finalization.

These fields and events are local C ABI/JNI additions. Network protocol v1 and
ACKs are unchanged; existing Windows/Android clients ignore unknown event types
and additional fields. The updated macOS app can transfer to older clients.

## Optional resume extension (DROP 1.1)

Discovery may include `resume:true`. A new sender offers protocol 1 plus `resume:true`, a stable safe ASCII transfer ID and an optional `action:"cancel"`. An old receiver ignores these fields and replies `{ok:true}`; the sender falls back to normal v1 with no pause support. A capable receiver replies `{ok:true,resume:true,id,offsets:[[writtenBytes,finished],...]}` in manifest order. The receiver owns these offsets: they come from actual partial-file lengths and finalized files, not the sender progress counter.

Each unfinished non-directory file sends `size-writtenBytes` raw bytes and receives the normal per-file ACK. A finalized file sends no bytes and no per-file ACK. The final batch ACK is unchanged. Control frames are never inserted into the raw byte stream. Pause closes the socket; a subsequent offer negotiates offsets again. Replies `status:"paused"` or `status:"busy"` defer retry; `status:"cancelled"` terminates it. A stable device ID prevents resuming into a different receiver after address reassignment.

The receiver validates the stable sender ID and the complete manifest against its local journal. It preserves partial files on transport interruption, persists manual pause and collision-resolved roots, and skips completed files. Source sizes and modification times are checked before reopening. Native and Android journals survive application exit. Cancellation removes unfinished partial files and leaves finalized files intact. If the peer is offline, sender cancellation is queued until it can reach the receiver.

New local commands: `{op:"pause",transfer}` and `{op:"resume",transfer}`. A `state` event supplies `status` (`active`, `waiting`, `paused`) and `resumable`. `busy` describes active I/O; manual pause does not prevent exiting or updating the application. `display_entries` and `display_count` describe top-level UI items, with aggregate folder sizes, separately from the complete wire manifest.

Persistent state is flushed at file and state boundaries; there is no per-block fsync, block ACK, compression or prehash. Ordinary application/process restarts are supported. This is not a transactional durability guarantee against sudden power loss or arbitrary external modification of partial files.
