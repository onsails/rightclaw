# Telegram Attachments Support

## Overview

Add full attachment support to the RightClaw Telegram bot: receiving files/images/voice/video from users, passing them to Claude Code sessions, and sending CC-generated files back to users.

## Goals

- Support all Telegram attachment types: photo, document, video, audio, voice, video_note, sticker, animation
- Unified YAML input format for CC invocations (replacing XML batch)
- Extended JSON output schema with typed attachments (replacing `media_paths` stub)
- Per-attachment size validation with user notification
- Periodic cleanup of attachment files in both sandbox and no-sandbox modes

## Non-Goals

- Transcription or format conversion of voice/video (handled by CC or skills)
- Media groups (multiple photos as album) — treated as individual messages
- Inline query attachments

---

## Input Format

### Stdin Piping

All CC invocations switch from positional argument (after `--`) to stdin piping. This applies to both sandbox (SSH) and no-sandbox modes.

**Rationale:** Eliminates shell argument length limits, removes shell escaping for message content, natural fit for structured data.

### Format Selection

Two formats, selected automatically:

1. **Plain text** — single message in the debounce batch, no attachments. Raw string piped to stdin.
2. **YAML** — multiple messages in the batch, OR any message has attachments.

```yaml
messages:
  - id: 12345
    ts: "2026-04-08T12:00:00Z"
    text: "analyze this chart and export to CSV"
    attachments:
      - type: photo
        path: /sandbox/inbox/photo_12345_0.jpg
        mime_type: image/jpeg
      - type: document
        path: /sandbox/inbox/doc_12345_1.pdf
        filename: quarterly_report.pdf
        mime_type: application/pdf
  - id: 12346
    ts: "2026-04-08T12:00:01Z"
    text: "also check this recording"
    attachments:
      - type: voice
        path: /sandbox/inbox/voice_12346_0.ogg
        mime_type: audio/ogg
  - id: 12347
    ts: "2026-04-08T12:00:01Z"
    text: "plain follow-up"
```

### YAML Schema

Per-message fields:
- `id` (integer) — Telegram message ID
- `ts` (string) — ISO 8601 timestamp
- `text` (string, optional) — message text or caption
- `attachments` (array, optional) — omitted entirely when empty

Per-attachment fields:
- `type` (string, required) — one of: `photo`, `document`, `video`, `audio`, `voice`, `video_note`, `sticker`, `animation`
- `path` (string, required) — absolute path inside sandbox or agent dir
- `mime_type` (string, required) — MIME type from Telegram
- `filename` (string, optional) — original filename, present for `document` type only

---

## Output Schema

Replaces the `media_paths` stub in `ReplyOutput`.

```json
{
  "type": "object",
  "properties": {
    "content": { "type": ["string", "null"] },
    "reply_to_message_id": { "type": ["integer", "null"] },
    "attachments": {
      "type": ["array", "null"],
      "items": {
        "type": "object",
        "properties": {
          "type": { "enum": ["photo", "document", "video", "audio", "voice", "video_note", "sticker", "animation"] },
          "path": { "type": "string" },
          "filename": { "type": ["string", "null"] },
          "caption": { "type": ["string", "null"] }
        },
        "required": ["type", "path"]
      }
    }
  },
  "required": ["content"]
}
```

### Reply-to Behavior

- **Single message input:** Bot auto-sets `reply_to_message_id` to the triggering message's Telegram ID, regardless of CC's output value.
- **Batched YAML input:** CC may set `reply_to_message_id` to one of the message IDs from the batch. Bot uses it if present, otherwise no reply-to.

---

## Inbound Pipeline (Telegram to CC)

### Handler Changes (`handler.rs`)

Current behavior: `msg.text()` returns `None` for media messages, handler returns `Ok(())`.

New behavior:

1. Extract text from `msg.text()` OR `msg.caption()` (media messages carry captions).
2. Check all attachment fields on the message: `photo()`, `document()`, `video()`, `audio()`, `voice()`, `video_note()`, `sticker()`, `animation()`.
3. Build `InboundAttachment` structs with `file_id`, type, mime_type, filename, file_size.
4. Messages with neither text nor attachments: `return Ok(())`.
5. Pack into `DebounceMsg { message_id, text: Option<String>, attachments: Vec<InboundAttachment>, timestamp }`.
6. Route to worker via existing DashMap channel.

### Worker Download Step

After debounce window closes, before CC invocation:

```
for each message in batch:
  for each attachment:
    1. bot.get_file(file_id) -> File { path }
    2. Check file_size -- if > 20MB:
       - Send Telegram message: "Attachment skipped: {filename_or_type} exceeds 20MB limit"
       - Remove from attachment list (other attachments proceed)
    3. Download to host: {agent_dir}/tmp/inbox/{type}_{message_id}_{index}.{ext}
    4. Sandbox mode: openshell::upload_file(sandbox, host_path, "/sandbox/inbox/{name}")
       No-sandbox mode: move to {agent_dir}/inbox/{name}
    5. Store resolved path in ResolvedAttachment
```

**File naming:** `{type}_{message_id}_{index}.{ext}` — e.g., `photo_12345_0.jpg`, `doc_12345_0.pdf`

**Extension resolution:** Derived from mime_type. Fallback to `.bin` for unknown types.

### Input Formatting

After downloads complete:

- **Single message, no attachments** -> pipe raw text string to stdin
- **Otherwise** -> serialize to YAML, pipe to stdin

### SSH Stdin Integration

Current: `ssh -F config host sh -c 'claude -p --flags -- "escaped message"'`

New: `ssh -F config host sh -c 'claude -p --flags'` with stdin piped through SSH. SSH forwards stdin to the remote process natively. This removes the positional `--` argument and eliminates `shlex::try_quote()` for message content (still needed for flag values).

No-sandbox mode: `Command::new("claude").stdin(Stdio::piped())`, write input, drop stdin handle, `wait_with_output()`.

---

## Outbound Pipeline (CC to Telegram)

### Response Parsing

Replace `media_paths: Option<Vec<String>>` with `attachments: Option<Vec<OutboundAttachment>>` in `ReplyOutput`. Deserialize from CC's structured JSON output.

### Send Flow

```
1. Parse ReplyOutput from CC JSON
2. If content is Some and non-empty:
   - Send as text message
   - Single input: reply_to triggering message ID
   - Batch input: reply_to CC's reply_to_message_id if set
3. If attachments is Some and non-empty:
   For each attachment:
     a. Validate path starts with /sandbox/outbox/ (or {agent_dir}/outbox/)
        -- reject with warning if outside allowed directory
     b. Sandbox mode: openshell::download_file(sandbox, path, host_temp)
        No-sandbox mode: read from {agent_dir}/outbox/
     c. Check Telegram size limits:
        - photo: 10MB
        - document/video/audio/voice/animation: 50MB
        -- on violation: send text "Attachment too large: {filename_or_type}"
     d. Send via matching teloxide method:
        - photo -> bot.send_photo(chat_id, InputFile::file(path))
        - document -> bot.send_document(...).file_name(filename)
        - video -> bot.send_video(...)
        - audio -> bot.send_audio(...)
        - voice -> bot.send_voice(...)
        - animation -> bot.send_animation(...)
     e. Set caption if present
4. Clean up host temp files from download step
```

---

## Directory Structure

### Sandbox Mode

```
/sandbox/
  inbox/       # Bot uploads received attachments here
  outbox/      # CC writes files here for bot to send back
```

### No-Sandbox Mode

```
{agent_dir}/
  inbox/       # Received attachments
  outbox/      # CC-generated files
  tmp/inbox/   # Temporary download before upload to sandbox
```

Directories created by bot on startup.

---

## Cleanup

### Schedule

- **Interval:** every 1 hour (hardcoded)
- **Retention:** 7 days default, configurable via `agent.yaml` at `attachments.retention_days`

### Execution

- **Sandbox mode:** `ssh_exec(config, host, &["find", "/sandbox/inbox", "/sandbox/outbox", "-mtime", "+{retention_days}", "-delete"])` via existing `openshell::ssh_exec()`. `retention_days` from config.
- **No-sandbox mode:** `tokio::fs` walk of `{agent_dir}/inbox/` and `{agent_dir}/outbox/`, delete files with mtime older than `retention_days`

### Host Temp Files

`{agent_dir}/tmp/inbox/` cleaned immediately after upload to sandbox. Outbound temp files cleaned after sending to Telegram.

---

## System Prompt Addition

Added to generated agent definition (`agent_def.rs`) for every agent:

```
## Message Input Format

You receive user messages via stdin in one of two formats:

1. **Plain text** — a single message with no attachments
2. **YAML** — multiple messages or messages with attachments, with a `messages:` root key

YAML schema:
  messages:
    - id: <telegram_message_id>
      ts: <ISO 8601 timestamp>
      text: <message text or caption>
      attachments:
        - type: photo|document|video|audio|voice|video_note|sticker|animation
          path: <absolute path to file>
          mime_type: <MIME type>
          filename: <original filename, documents only>

Use the Read tool to view images and files at the given paths.

## Sending Attachments

Write files to /sandbox/outbox/ (or the outbox/ directory in your working directory).
Include them in your JSON response under the `attachments` array.

Size limits enforced by the bot:
- Photos: max 10MB
- Documents, videos, audio, voice, animations: max 50MB

Do not produce files exceeding these limits. If you need to send large data,
split into multiple smaller files or use a different format.
```

---

## Type Changes

### New Types (`crates/bot/src/telegram/attachments.rs`)

```rust
enum AttachmentKind {
    Photo,
    Document,
    Video,
    Audio,
    Voice,
    VideoNote,
    Sticker,
    Animation,
}

/// Extracted from Telegram message, before download
struct InboundAttachment {
    file_id: String,
    kind: AttachmentKind,
    mime_type: Option<String>,
    filename: Option<String>,
    file_size: Option<u32>,
}

/// After download, with resolved filesystem path
struct ResolvedAttachment {
    kind: AttachmentKind,
    path: PathBuf,
    mime_type: String,
    filename: Option<String>,
}

/// From CC JSON response
struct OutboundAttachment {
    kind: AttachmentKind,
    path: String,
    filename: Option<String>,
    caption: Option<String>,
}
```

### Modified Types

- `DebounceMsg` — add `attachments: Vec<InboundAttachment>` field
- `ReplyOutput` — replace `media_paths: Option<Vec<String>>` with `attachments: Option<Vec<OutboundAttachment>>`

### New Module

`crates/bot/src/telegram/attachments.rs` — all attachment logic:
- `extract_attachments(msg: &Message) -> Vec<InboundAttachment>`
- `download_and_upload(attachments, bot, agent_dir, sandbox_config) -> Vec<ResolvedAttachment>`
- `send_attachments(attachments: &[OutboundAttachment], bot, chat_id, sandbox_config) -> Result<()>`
- `spawn_cleanup_task(agent_dir, sandbox_config, retention_days)`
- `mime_to_extension(mime: &str) -> &str`

---

## Codegen Changes

### `agent_def.rs`

- Append input/output format documentation to agent system prompt
- Update `REPLY_SCHEMA_JSON` constant with new `attachments` schema

### `reply-schema.json`

Replace:
```json
"media_paths": { "type": ["array", "null"], "items": { "type": "string" } }
```

With new `attachments` schema (see Output Schema section above).

---

## Configuration

### `agent.yaml` Extension

```yaml
attachments:
  retention_days: 7  # default, how long to keep inbox/outbox files
```

---

## Migration

- `media_paths` field removed from `ReplyOutput` and reply schema
- Old agents with `media_paths` in their schema get updated on next `rightclaw up` (codegen regenerates schema)
- No database migration needed
