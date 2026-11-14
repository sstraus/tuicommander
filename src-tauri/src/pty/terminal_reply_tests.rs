use super::*;
use crate::state::VtLogBuffer;

// Catches: canonical queries are dropped, or retried only on another output
// chunk, so a child that goes raw and silently waits never receives its replies.
#[tokio::test]
async fn canonical_queries_are_delivered_after_raw_mode_without_more_output() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "canonical-query-then-raw";
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer = pair.master.take_writer().unwrap();
    let mut command = CommandBuilder::new("python3");
    command.args([
        "-c",
        r#"import os, select, termios, time, tty
old = termios.tcgetattr(0)
assert old[3] & termios.ICANON
os.write(1, b'\x1b[?u\x1b[c\x1b[6n')
assert os.read(0, 64) == b'GO\n'
try:
    tty.setraw(0)
    start = time.monotonic()
    reply = b''
    while not reply.endswith(b'R'):
        if not select.select([0], [], [], max(0, 2 - (time.monotonic() - start)))[0]:
            break
        reply += os.read(0, 64)
finally:
    termios.tcsetattr(0, termios.TCSANOW, old)
print('\nCURSOR-RESULT:' + reply.hex())
"#,
    ]);
    let child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    state.session_maps.sessions.insert(
        sid.into(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "python3".into(),
        }),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(4096)));
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    let mut query = Vec::new();
    // One-byte reads exercise escape reassembly as well as reply ordering.
    // The child's GO barrier keeps ICANON on until all queries were processed.
    while !query.ends_with(b"\x1b[6n") {
        let mut byte = [0];
        reader.read_exact(&mut byte).unwrap();
        query.extend_from_slice(&byte);
        let data = escapes.push(&utf8.push(&byte));
        let (clean, actions) = strip_kitty_sequences(&data);
        process_kitty_actions(&actions, sid, &state);
        processor.process_chunk(&clean, &silence, sid, &state);
    }
    assert!(tty_would_swallow_reply(&state, sid));
    let mut events = state.event_bus.subscribe();
    spawn_reader_thread(
        reader,
        Arc::new(AtomicBool::new(false)),
        sid.into(),
        state.clone(),
        None,
    );
    // Release the child only after every canonical query has been seen.
    state.write_pty_parts(sid, &[b"GO\n"]).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if matches!(events.recv().await.unwrap(), crate::state::AppEvent::SessionClosed { session_id, .. } if session_id == sid) {
                break;
            }
        }
    }).await.expect("PTY child did not finish its bounded reply read");
    let (output, _) = state
        .session_maps
        .output_buffers
        .get(sid)
        .unwrap()
        .lock()
        .read_last(4096);
    let output = String::from_utf8(output).unwrap();
    cleanup_session(sid, &state);
    assert!(
        output.contains("CURSOR-RESULT:1b5b3f30751b5b3f36631b5b313b3152"),
        "queued replies missing or reordered: {output:?}"
    );
    assert!(
        !output.contains("^[["),
        "canonical replies must not echo: {output:?}"
    );
}
