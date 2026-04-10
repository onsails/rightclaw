# Thinking Stop Button Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an inline "Stop" button to thinking messages that kills the CC process, with correct behavior for both `show_thinking: true` and `show_thinking: false` modes.

**Architecture:** Shared `DashMap<(ChatId, i32), CancellationToken>` injected via dptree deps. Worker inserts token before CC invocation, removes after exit. Callback query handler looks up token and cancels. Worker's stream loop adds a `select!` branch on the token. Thinking message is always sent (with keyboard) regardless of `show_thinking` — when false, it's a static anchor deleted on normal completion.

**Tech Stack:** teloxide (InlineKeyboardMarkup, CallbackQuery), tokio_util::sync::CancellationToken, dashmap

---

### Task 1: Define `StopTokens` type and add to `WorkerContext`

**Files:**
- Modify: `crates/bot/src/telegram/mod.rs:1-13`
- Modify: `crates/bot/src/telegram/worker.rs:40-66`

- [ ] **Step 1: Add `StopTokens` type alias to `mod.rs`**

In `crates/bot/src/telegram/mod.rs`, add after the `BotType` alias (line 17):

```rust
use std::sync::Arc;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;

/// Shared map of active CC sessions that can be stopped via inline button.
/// Key: (chat_id, eff_thread_id). Value: CancellationToken to kill the CC process.
pub type StopTokens = Arc<DashMap<(i64, i64), CancellationToken>>;
```

Note: use `(i64, i64)` to match `SessionKey` type (not `ChatId` — avoids import coupling).

- [ ] **Step 2: Add `stop_tokens` field to `WorkerContext`**

In `crates/bot/src/telegram/worker.rs`, add to the `WorkerContext` struct after `show_thinking`:

```rust
    /// Shared map for stop button — worker inserts token before CC, removes after exit.
    pub stop_tokens: super::StopTokens,
```

- [ ] **Step 3: Build and verify compilation**

Run: `cargo check -p rightclaw-bot`
Expected: compilation errors in `handler.rs` where `WorkerContext` is constructed (missing field) — this is expected, will be fixed in Task 3.

- [ ] **Step 4: Commit**

```bash
git add crates/bot/src/telegram/mod.rs crates/bot/src/telegram/worker.rs
git commit -m "feat: define StopTokens type and add to WorkerContext"
```

---

### Task 2: Inject `StopTokens` into dispatch and handler

**Files:**
- Modify: `crates/bot/src/telegram/dispatch.rs:55-134`
- Modify: `crates/bot/src/telegram/handler.rs:81-175`

- [ ] **Step 1: Create and inject `StopTokens` in `dispatch.rs`**

In `run_telegram`, after `let show_thinking_arc` (line 92), add:

```rust
    let stop_tokens: super::StopTokens = Arc::new(DashMap::new());
```

Add to the `dptree::deps!` block (after the `show_thinking_arc` entry):

```rust
            Arc::clone(&stop_tokens)
```

- [ ] **Step 2: Add `stop_tokens` parameter to `handle_message` and pass to `WorkerContext`**

In `handle_message` signature (handler.rs), add parameter:

```rust
    stop_tokens: super::StopTokens,
```

In the `WorkerContext` construction (around line 151-165), add:

```rust
                    stop_tokens: Arc::clone(&stop_tokens),
```

- [ ] **Step 3: Build and verify compilation**

Run: `cargo check -p rightclaw-bot`
Expected: PASS (all `WorkerContext` constructions now have the field)

- [ ] **Step 4: Commit**

```bash
git add crates/bot/src/telegram/dispatch.rs crates/bot/src/telegram/handler.rs
git commit -m "feat: inject StopTokens into dispatch and handler"
```

---

### Task 3: Add callback query handler for Stop button

**Files:**
- Modify: `crates/bot/src/telegram/handler.rs` (add `handle_stop_callback`)
- Modify: `crates/bot/src/telegram/dispatch.rs` (add callback query branch)

- [ ] **Step 1: Write test for callback data parsing**

In `crates/bot/src/telegram/handler.rs`, add to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn parse_stop_callback_data_valid() {
        let data = "stop:12345:678";
        let parts: Vec<&str> = data.splitn(3, ':').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "stop");
        assert_eq!(parts[1].parse::<i64>().unwrap(), 12345);
        assert_eq!(parts[2].parse::<i64>().unwrap(), 678);
    }

    #[test]
    fn parse_stop_callback_data_zero_thread() {
        let data = "stop:12345:0";
        let parts: Vec<&str> = data.splitn(3, ':').collect();
        assert_eq!(parts[2].parse::<i64>().unwrap(), 0);
    }

    #[test]
    fn parse_stop_callback_data_invalid() {
        let data = "stop:notanumber:0";
        let parts: Vec<&str> = data.splitn(3, ':').collect();
        assert!(parts[1].parse::<i64>().is_err());
    }
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p rightclaw-bot -- parse_stop_callback`
Expected: 3 tests PASS

- [ ] **Step 3: Add `handle_stop_callback` function in `handler.rs`**

Add after `handle_doctor`:

```rust
/// Handle the Stop button callback query from thinking messages.
///
/// Callback data format: `stop:{chat_id}:{eff_thread_id}`
/// Looks up the CancellationToken in StopTokens and cancels it.
pub async fn handle_stop_callback(
    bot: BotType,
    q: CallbackQuery,
    stop_tokens: super::StopTokens,
) -> ResponseResult<()> {
    let data = q.data.as_deref().unwrap_or("");
    let parts: Vec<&str> = data.splitn(3, ':').collect();

    let answered = if parts.len() == 3
        && parts[0] == "stop"
        && let Ok(chat_id) = parts[1].parse::<i64>()
        && let Ok(thread_id) = parts[2].parse::<i64>()
    {
        let key = (chat_id, thread_id);
        if let Some(entry) = stop_tokens.get(&key) {
            entry.value().cancel();
            drop(entry); // release DashMap read guard before await
            bot.answer_callback_query(&q.id).text("Stopping...").await?;
            true
        } else {
            bot.answer_callback_query(&q.id).text("Already finished").await?;
            true
        }
    } else {
        false
    };

    if !answered {
        // Malformed callback data — answer anyway (Telegram requires it)
        bot.answer_callback_query(&q.id).await?;
    }

    Ok(())
}
```

- [ ] **Step 4: Add callback query branch in `dispatch.rs`**

Import `handle_stop_callback` in the import line (line 25):

```rust
use super::handler::{handle_doctor, handle_mcp, handle_message, handle_reset, handle_start, handle_stop_callback, AgentDir, AuthCodeSlot, AuthWatcherFlag, DebugFlag, MaxBudgetUsd, MaxTurns, RefreshTx, RightclawHome, ShowThinking, SshConfigPath};
```

After the `message_handler` definition (line 117), add the callback query handler:

```rust
    let callback_handler = Update::filter_callback_query()
        .endpoint(handle_stop_callback);
```

Change the schema (line 118) to branch both:

```rust
    let schema = dptree::entry()
        .branch(message_handler)
        .branch(callback_handler);
```

- [ ] **Step 5: Build and verify compilation**

Run: `cargo check -p rightclaw-bot`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/bot/src/telegram/handler.rs crates/bot/src/telegram/dispatch.rs
git commit -m "feat: add callback query handler for Stop button"
```

---

### Task 4: Build `stop_keyboard` helper and modify thinking message sending

**Files:**
- Modify: `crates/bot/src/telegram/worker.rs`

This task changes the thinking message to always be sent (regardless of `show_thinking`) and attaches an inline keyboard with the Stop button.

- [ ] **Step 1: Add `stop_keyboard` helper function**

In `worker.rs`, add a helper near the top (after the constants):

```rust
/// Build the inline keyboard with a single "Stop" button for thinking messages.
fn stop_keyboard(chat_id: i64, eff_thread_id: i64) -> teloxide::types::InlineKeyboardMarkup {
    teloxide::types::InlineKeyboardMarkup::new(vec![vec![
        teloxide::types::InlineKeyboardButton::callback(
            "\u{26d4} Stop",
            format!("stop:{chat_id}:{eff_thread_id}"),
        ),
    ]])
}
```

- [ ] **Step 2: Add test for `stop_keyboard`**

In the `#[cfg(test)]` module at the bottom of `worker.rs`:

```rust
    #[test]
    fn stop_keyboard_format() {
        let kb = stop_keyboard(12345, 678);
        let buttons = &kb.inline_keyboard;
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].len(), 1);
        assert_eq!(buttons[0][0].text, "\u{26d4} Stop");
        match &buttons[0][0].kind {
            teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => {
                assert_eq!(data, "stop:12345:678");
            }
            other => panic!("expected CallbackData, got {other:?}"),
        }
    }
```

- [ ] **Step 3: Run test**

Run: `cargo test -p rightclaw-bot -- stop_keyboard_format`
Expected: PASS

- [ ] **Step 4: Modify thinking message sending in `invoke_cc`**

Replace the thinking message block (lines 989-1020) with logic that:
1. Always sends a thinking message on first displayable event (not gated by `show_thinking`)
2. Attaches `stop_keyboard` to every send/edit
3. When `show_thinking: false`, sends static "⏳ Working..." once and never updates

The current code (lines 989-1020):
```rust
                        // Update thinking message (throttled to 2s).
                        if ctx.show_thinking
                            && super::stream::format_event(&event).is_some()
                            && last_edit.elapsed() >= Duration::from_secs(2)
                        {
                            ...
                        }
```

Replace with:

```rust
                        // Thinking message: always send (Stop button anchor).
                        // show_thinking=true: update with events every 2s.
                        // show_thinking=false: send static "Working..." once, no updates.
                        if super::stream::format_event(&event).is_some() {
                            let kb = stop_keyboard(chat_id, eff_thread_id);

                            if thinking_msg_id.is_none() {
                                // First displayable event — send thinking message.
                                let text = if ctx.show_thinking {
                                    super::stream::format_thinking_message(
                                        ring_buffer.events(),
                                        &usage,
                                        ctx.max_turns,
                                    )
                                } else {
                                    "\u{23f3} Working...".to_string()
                                };
                                let mut send = ctx.bot.send_message(tg_chat_id, &text)
                                    .parse_mode(teloxide::types::ParseMode::Html)
                                    .reply_markup(kb);
                                if eff_thread_id != 0 {
                                    send = send.message_thread_id(
                                        ThreadId(MessageId(eff_thread_id as i32)),
                                    );
                                }
                                if let Ok(msg) = send.await {
                                    thinking_msg_id = Some(msg.id);
                                }
                                last_edit = tokio::time::Instant::now();
                            } else if ctx.show_thinking
                                && last_edit.elapsed() >= Duration::from_secs(2)
                            {
                                // Throttled update (show_thinking=true only).
                                let text = super::stream::format_thinking_message(
                                    ring_buffer.events(),
                                    &usage,
                                    ctx.max_turns,
                                );
                                if let Some(msg_id) = thinking_msg_id {
                                    let _ = ctx
                                        .bot
                                        .edit_message_text(tg_chat_id, msg_id, &text)
                                        .parse_mode(teloxide::types::ParseMode::Html)
                                        .reply_markup(kb)
                                        .await;
                                }
                                last_edit = tokio::time::Instant::now();
                            }
                        }
```

- [ ] **Step 5: Build and verify compilation**

Run: `cargo check -p rightclaw-bot`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/bot/src/telegram/worker.rs
git commit -m "feat: always send thinking message with Stop keyboard"
```

---

### Task 5: Add stop token lifecycle and `select!` branch in stream loop

**Files:**
- Modify: `crates/bot/src/telegram/worker.rs`

- [ ] **Step 1: Insert stop token before CC invocation**

In `invoke_cc`, right after the `let mut child = cmd.spawn()...` block and stdin write (around line 899), add:

```rust
    // Insert stop token so callback handler can kill this CC session.
    let stop_token = CancellationToken::new();
    ctx.stop_tokens.insert((chat_id, eff_thread_id), stop_token.clone());
```

- [ ] **Step 2: Add `stopped` flag and `select!` branch**

Add a `let mut stopped = false;` alongside `let mut timed_out = false;` (line 941).

In the main `tokio::select!` loop (lines 943-1035), add a third branch:

```rust
            _ = stop_token.cancelled() => {
                stopped = true;
                child.kill().await.ok();
                break;
            }
```

The full `tokio::select!` block becomes:

```rust
        tokio::select! {
            line_result = lines.next_line() => {
                // ... existing line processing ...
            }
            _ = tokio::time::sleep_until(deadline) => {
                timed_out = true;
                child.kill().await.ok();
                break;
            }
            _ = stop_token.cancelled() => {
                stopped = true;
                child.kill().await.ok();
                break;
            }
        }
```

- [ ] **Step 3: Remove stop token after CC exits**

After the `child.wait().await` line (line 1038), add:

```rust
    // Remove stop token — session no longer cancellable.
    ctx.stop_tokens.remove(&(chat_id, eff_thread_id));
```

- [ ] **Step 4: Build and verify compilation**

Run: `cargo check -p rightclaw-bot`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/bot/src/telegram/worker.rs
git commit -m "feat: stop token lifecycle and select! branch for CC kill"
```

---

### Task 6: Post-completion thinking message edits

**Files:**
- Modify: `crates/bot/src/telegram/worker.rs`

This task implements the 4-way behavior matrix for thinking message after CC exits.

- [ ] **Step 1: Modify the "Final update of thinking message" block**

The current "Final update" code (lines 960-972) runs inside the `StreamEvent::Result` match arm. This only handles normal completion with `show_thinking: true`. We need to move the final thinking message handling to AFTER the stream loop and `child.wait()`, where we know whether it was stopped, timed out, or normal.

Remove the final update from inside the `Result` arm (lines 960-972). Keep only `usage` and `result_line` extraction:

```rust
                            super::stream::StreamEvent::Result(json) => {
                                usage = super::stream::parse_usage(json);
                                result_line = Some(json.clone());
                            }
```

- [ ] **Step 2: Add post-loop thinking message handling**

After the stop token removal (`ctx.stop_tokens.remove(...)`) and before the timeout check, add:

```rust
    // Final thinking message update based on completion mode.
    if let Some(msg_id) = thinking_msg_id {
        if stopped {
            // Stopped by user — show final state, remove keyboard.
            let text = if ctx.show_thinking {
                let mut msg = super::stream::format_thinking_message(
                    ring_buffer.events(),
                    &usage,
                    ctx.max_turns,
                );
                msg.push_str("\n\u{26d4} Stopped");
                msg
            } else {
                "\u{23f3} Working...\n\u{26d4} Stopped".to_string()
            };
            let _ = ctx.bot
                .edit_message_text(tg_chat_id, msg_id, &text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await;
        } else if ctx.show_thinking {
            // Normal finish with thinking — final cost/turns, remove keyboard.
            let text = super::stream::format_thinking_message(
                ring_buffer.events(),
                &usage,
                ctx.max_turns,
            );
            let _ = ctx.bot
                .edit_message_text(tg_chat_id, msg_id, &text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await;
        } else {
            // Normal finish without thinking — delete the anchor message.
            let _ = ctx.bot.delete_message(tg_chat_id, msg_id).await;
        }
    }
```

Note: `edit_message_text` without `.reply_markup()` automatically removes the inline keyboard. This is the desired behavior for all non-delete cases.

- [ ] **Step 3: Handle stopped state in the return path**

After the timeout check and before stdout parsing, add early return for stopped:

```rust
    // Handle user-initiated stop.
    if stopped {
        tracing::info!(?chat_id, "CC session stopped by user");
        return Ok(None); // No reply to send — thinking message already updated.
    }
```

- [ ] **Step 4: Build and verify compilation**

Run: `cargo check -p rightclaw-bot`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/bot/src/telegram/worker.rs
git commit -m "feat: post-completion thinking message edits (4-way matrix)"
```

---

### Task 7: Final build and clippy

**Files:** None (verification only)

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: PASS

- [ ] **Step 2: Clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: PASS (no new warnings)

- [ ] **Step 3: Run all tests**

Run: `cargo test --workspace`
Expected: All existing tests pass, new tests pass

- [ ] **Step 4: Commit any clippy fixes**

If clippy flagged anything:
```bash
git add -u
git commit -m "fix: address clippy warnings from stop button feature"
```

---

### Task 8: Verify `lib.rs` bot construction passes `stop_tokens`

**Files:**
- Modify: `crates/bot/src/lib.rs:90-110` (approximate — where `run_telegram` is called)

The `run_telegram` function doesn't take `StopTokens` as a parameter — it creates the `DashMap` internally in `dispatch.rs`. So `lib.rs` should not need changes. But verify:

- [ ] **Step 1: Verify `run_telegram` signature unchanged**

Read `dispatch.rs` `run_telegram` signature — `StopTokens` is created inside the function body, not passed in. No changes to `lib.rs` needed.

- [ ] **Step 2: Full integration check**

Run: `cargo build --workspace`
Expected: PASS

- [ ] **Step 3: Final commit if any loose ends**

```bash
git add -u
git commit -m "chore: final cleanup for stop button feature"
```
