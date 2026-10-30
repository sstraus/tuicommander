use super::*;

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("test scratch")
}
fn repo(path: &Path) {
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "WF05"],
        vec!["config", "user.email", "wf05@example.test"],
        vec!["commit", "--allow-empty", "-qm", "source"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(path)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}
fn definition(script: &str) -> CommandDefinition {
    CommandDefinition {
        version: 1,
        invocation: Invocation::Argv {
            argv: vec![python().into(), "-c".into(), script.into()],
        },
        workspace: "assigned".into(),
        target: CommandTarget::Local {
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
        },
        resources: vec![],
        secret_refs: BTreeMap::new(),
        stdin_artifact: None,
        timeout_secs: 10,
        output: CommandOutput::ExitStatus,
    }
}
fn identity(effect: &str) -> CommandIdentity {
    CommandIdentity {
        run: "run".into(),
        execution: "graph".into(),
        activation: effect.into(),
        attempt: effect.into(),
        generation: 1,
        effect: effect.into(),
        tool: None,
    }
}
fn adapters(root: &Path) -> CommandAdapters {
    CommandAdapters {
        build_slot: PathBuf::from("not-used"),
        rb: PathBuf::from("not-used"),
        freeze: root.join("BUILD_FREEZE"),
        agent_name: "wf05".into(),
    }
}

#[test]
fn zero_matched_tests_cannot_pass_required_scope() {
    // catches: exit zero authorizing a required scope with no actual matched tests.
    let root = scratch();
    let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap();
    repo(&workspace);
    let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
    for (index, (summary, success)) in [
        (r#"{"matched":0,"passed":0,"failed":0}"#, false),
        (r#"{"matched":2,"passed":1,"failed":1}"#, false),
        (r#"{"matched":2,"passed":2,"failed":0}"#, true),
    ]
    .iter()
    .enumerate()
    {
        let mut command = definition(&format!("print('{summary}')"));
        command.output = CommandOutput::TestSummary { minimum_matched: 1 };
        let receipt = journal
            .execute(
                &command,
                &identity(&format!("test-{index}")),
                &workspace,
                &root.path().join("runs.db"),
                &adapters(root.path()),
            )
            .unwrap();
        assert_eq!(receipt.succeeded, *success, "summary {index}");
        assert!(receipt.stopped && receipt.released);
    }
}

#[test]
fn lease_loss_keeps_effect_and_slot_uncertain() {
    // catches: client/lease loss releasing a resource and admitting overlapping replacement.
    let root = scratch();
    let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
    let adapters = adapters(root.path());
    let first = identity("first");
    let second = identity("replacement");
    let handle = root.path().join("handle.json");
    assert!(
        journal
            .admit(&first, "payload", &["mutation".into()], &handle, &adapters)
            .unwrap()
    );
    journal.uncertain(&first, "reservation lease lost").unwrap();
    drop(journal);
    let reopened = CommandJournal::for_runs(&root.path().join("runs.db"));
    assert!(
        !reopened
            .admit(
                &second,
                "replacement",
                &["mutation".into()],
                &handle,
                &adapters
            )
            .unwrap()
    );
    assert!(
        !reopened
            .admit(&first, "payload", &["mutation".into()], &handle, &adapters)
            .unwrap()
    );
    assert!(reopened.receipt("first").unwrap().is_none());
    assert!(
        reopened
            .observations("run")
            .unwrap()
            .iter()
            .any(|row| row.state == "uncertain")
    );
}

#[test]
fn linux_receipt_cannot_discharge_native_mac_requirement() {
    // catches: successful Linux execution being treated as required native macOS proof.
    let root = scratch();
    let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap();
    repo(&workspace);
    let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
    let mut receipt = journal
        .execute(
            &definition("print('ok')"),
            &identity("platform"),
            &workspace,
            &root.path().join("runs.db"),
            &adapters(root.path()),
        )
        .unwrap();
    receipt.os = "linux".into();
    receipt.arch = "aarch64".into();
    receipt.executor = "rb".into();
    assert!(receipt.proves_target(&CommandTarget::Rb));
    assert!(!receipt.proves_target(&CommandTarget::Local {
        os: "macos".into(),
        arch: "aarch64".into()
    }));
}

#[test]
fn freeze_blocks_new_admission_not_inspection() {
    // catches: freeze either starting new effects or denying inspection of retained receipts.
    let root = scratch();
    let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
    let adapters = adapters(root.path());
    std::fs::write(&adapters.freeze, "allowed: another-agent\n").unwrap();
    assert!(
        !journal
            .admit(
                &identity("frozen"),
                "payload",
                &[],
                &root.path().join("handle"),
                &adapters
            )
            .unwrap()
    );
    let observations = journal.observations("run").unwrap();
    assert_eq!(
        observations
            .iter()
            .map(|r| r.state.as_str())
            .collect::<Vec<_>>(),
        ["ready", "queued"]
    );
    assert!(observations.last().unwrap().reason.contains("BUILD_FREEZE"));
    std::fs::write(&adapters.freeze, "allowed: wf05\n").unwrap();
    assert!(
        journal
            .admit(
                &identity("frozen"),
                "payload",
                &[],
                &root.path().join("handle"),
                &adapters
            )
            .unwrap()
    );
}

#[test]
fn secret_output_is_redacted_before_immutable_receipt_and_reopen() {
    // catches: resolved environment values leaking through stdout, stderr or persisted receipts.
    let root = scratch();
    let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap();
    repo(&workspace);
    let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
    let mut command = definition(
        "import os,sys; print(os.environ['WF05_SECRET']); print(os.environ['WF05_SECRET'],file=sys.stderr)",
    );
    command
        .secret_refs
        .insert("WF05_SECRET".into(), "PATH".into());
    let receipt = journal
        .execute(
            &command,
            &identity("secret"),
            &workspace,
            &root.path().join("runs.db"),
            &adapters(root.path()),
        )
        .unwrap();
    assert_eq!(receipt.stdout.trim(), "[REDACTED]");
    assert_eq!(receipt.stderr.trim(), "[REDACTED]");
    let again = journal
        .execute(
            &command,
            &identity("secret"),
            &workspace,
            &root.path().join("runs.db"),
            &adapters(root.path()),
        )
        .unwrap();
    assert_eq!(again, receipt);
    let secret = std::env::var("PATH").unwrap();
    assert!(
        !encode(&journal.observations("run").unwrap())
            .unwrap()
            .contains(&secret)
    );
}

#[test]
fn explicit_shell_and_argv_remain_distinct_published_contracts() {
    // catches: implicit shell parsing or a shell contract losing its publication version.
    let mut command = definition("print('one')");
    command.invocation = Invocation::Argv {
        argv: vec!["sh".into(), "-c".into(), "true".into()],
    };
    assert!(command.validate().is_err());
    command.invocation = Invocation::Shell {
        program: "sh".into(),
        script: "true".into(),
        version: 1,
    };
    assert!(command.validate().is_ok());
    let json = encode(&command).unwrap();
    assert!(json.contains("shell") && json.contains("version"));
    command.invocation = Invocation::Shell {
        program: "sh".into(),
        script: "true".into(),
        version: 2,
    };
    assert!(command.validate().is_err());
}

#[test]
fn local_timeout_stops_descendant_writes_before_returning() {
    // catches: killing only the direct command and leaving its descendant mutating after stop proof.
    let root = scratch();
    let marker = root.path().join("writes");
    let child = "import sys,time; f=open(sys.argv[1],'a');\nwhile True: f.write('x'); f.flush(); time.sleep(.02)";
    let parent = "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',sys.argv[1],sys.argv[2]]); time.sleep(600)";
    let mut command = Command::new(python());
    command
        .args(["-c", parent, child])
        .arg(&marker)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let output = execute_process(command, Duration::from_secs(2), None, false).unwrap();
    assert_eq!(output.exit_code, -1);
    let size = std::fs::metadata(&marker)
        .expect("descendant started")
        .len();
    assert!(size > 0);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(std::fs::metadata(&marker).unwrap().len(), size);
}

#[test]
fn recorded_rb_stop_receipt_rejects_busy_unknown_and_wrong_fence() {
    // catches: a final-looking result or mismatched generation discharging a live reservation.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/rb-lookup.json")).unwrap();
    validate_remote_stop(&fixture).unwrap();
    for state in ["busy", "unknown"] {
        let mut fault = fixture.clone();
        fault["observation"]["state"] = state.into();
        assert!(validate_remote_stop(&fault).is_err());
    }
    let mut fault = fixture.clone();
    fault["observation"]["generation"] = "old-generation".into();
    assert!(validate_remote_stop(&fault).is_err());
    let mut fault = fixture;
    fault["final_receipt"]["fence"] = "other-fence".into();
    assert!(validate_remote_stop(&fault).is_err());
}

#[test]
fn conflicting_payload_cannot_reuse_an_immutable_command_receipt() {
    // catches: same effect key silently returning proof for a different command.
    let root = scratch();
    let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap();
    repo(&workspace);
    let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
    journal
        .execute(
            &definition("print('first')"),
            &identity("same-key"),
            &workspace,
            &root.path().join("runs.db"),
            &adapters(root.path()),
        )
        .unwrap();
    assert!(
        journal
            .execute(
                &definition("print('different')"),
                &identity("same-key"),
                &workspace,
                &root.path().join("runs.db"),
                &adapters(root.path())
            )
            .unwrap_err()
            .contains("conflicts")
    );
}

#[test]
fn stdin_artifact_is_snapshotted_and_bound_to_receipt() {
    // catches: stdin silently omitted or its bytes missing from the immutable subject.
    let root = scratch();
    let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap();
    repo(&workspace);
    std::fs::write(workspace.join("input.txt"), "typed input\n").unwrap();
    for args in [vec!["add", "input.txt"], vec!["commit", "-qm", "input"]] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&workspace)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
    let mut command = definition("import sys; print(sys.stdin.read(),end='')");
    command.stdin_artifact = Some("input.txt".into());
    let receipt = journal
        .execute(
            &command,
            &identity("stdin"),
            &workspace,
            &root.path().join("runs.db"),
            &adapters(root.path()),
        )
        .unwrap();
    assert!(receipt.succeeded);
    assert_eq!(receipt.stdout, "typed input\n");
    assert_eq!(
        receipt.subject.stdin_sha256.as_deref(),
        Some("3e043d1cb9ffffb6ae4702464f421d75fff3a68385f3c10e09c32fe1e34627d9")
    );
}

#[test]
#[ignore = "requires authorized native Mac execution and installed live rb adapters"]
fn live_rb_stdin_secret_transport_and_reconnect_produce_fenced_receipt() {
    // catches: the real wrapper losing stdin/secrets, logging raw values, or certifying before release.
    assert_eq!(
        std::env::consts::OS,
        "macos",
        "run this live adapter check natively on the Mac"
    );
    let root = scratch();
    let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap();
    repo(&workspace);
    std::fs::write(workspace.join("input.txt"), "live typed stdin\n").unwrap();
    for args in [vec!["add", "input.txt"], vec!["commit", "-qm", "input"]] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&workspace)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
    let mut command = definition(
        "import json,os,sys; print(json.dumps(dict(matched=1,passed=1,failed=0,stdin=sys.stdin.read(),secret=os.environ['WF05_SECRET'])))",
    );
    command.target = CommandTarget::Rb;
    command.resources = vec![CommandResource::Build];
    command.stdin_artifact = Some("input.txt".into());
    command
        .secret_refs
        .insert("WF05_SECRET".into(), "PATH".into());
    command.timeout_secs = 900;
    command.output = CommandOutput::TestSummary { minimum_matched: 1 };
    let mut installed = CommandAdapters::installed();
    installed.agent_name = "tuic-wf05".into();
    let receipt = journal
        .execute(
            &command,
            &identity("live-rb"),
            &workspace,
            &root.path().join("runs.db"),
            &installed,
        )
        .expect("live rb execution");
    assert!(receipt.succeeded, "live receipt reason: {}", receipt.reason);
    assert!(receipt.proves_target(&CommandTarget::Rb));
    assert!(!receipt.proves_target(&CommandTarget::Local {
        os: "macos".into(),
        arch: std::env::consts::ARCH.into()
    }));
    let output: serde_json::Value = serde_json::from_str(&receipt.stdout).unwrap();
    assert_eq!(output["stdin"], "live typed stdin\n");
    assert_eq!(output["secret"], "[REDACTED]");
    assert!(receipt.source_fingerprint.is_some());
    assert!(receipt.source_manifest_sha256.is_some());
    let run_dir = PathBuf::from(
        receipt.remote_handle.as_ref().unwrap()["run_dir"]
            .as_str()
            .unwrap(),
    );
    let secret = std::env::var("PATH").unwrap();
    for name in ["log", "mutation-metadata.json"] {
        assert!(
            !std::fs::read_to_string(run_dir.join(name))
                .unwrap()
                .contains(&secret),
            "secret value recorded in {name}"
        );
    }
    let handle = root.path().join("reconnect-handle.json");
    std::fs::write(
        &handle,
        encode(receipt.remote_handle.as_ref().unwrap()).unwrap(),
    )
    .unwrap();
    validate_remote_stop(&lookup_remote(&installed, &handle).unwrap()).unwrap();
    if let Some(directory) = std::env::var_os("TUIC_WF05_EVIDENCE_DIR") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("live-receipt.json"),
            encode(&receipt).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn tool_mutation_receipt_is_not_unchanged_artifact_proof_and_releases_slot() {
    // catches: a completed Tool mutation being either certified as old-source proof or left uncertain forever.
    for tool in [false, true] {
        let root = scratch();
        let workspace = root.path().join("repo");
        std::fs::create_dir(&workspace).unwrap();
        repo(&workspace);
        let journal = CommandJournal::for_runs(&root.path().join("runs.db"));
        let mut effect = identity("mutate");
        if tool {
            effect.tool = Some(("formatter".into(), 1));
        }
        let command =
            definition("from pathlib import Path; Path('new-file').write_text('changed')");
        let receipt = journal
            .execute(
                &command,
                &effect,
                &workspace,
                &root.path().join("runs.db"),
                &adapters(root.path()),
            )
            .unwrap();
        assert_eq!(receipt.succeeded, tool);
        assert!(!receipt.workspace_unchanged);
        assert!(receipt.stopped && receipt.released);
        assert!(!receipt.proves_target(&command.target));
        assert_eq!(
            std::fs::read_to_string(workspace.join("new-file")).unwrap(),
            "changed"
        );
        assert_eq!(
            journal.observations("run").unwrap().last().unwrap().state,
            "completed"
        );
    }
}

#[test]
fn json_escaped_secrets_do_not_reach_remote_durable_output_or_local_receipts() {
    // catches F3: JSON-escaped quotes, slashes and controls bypassing raw redaction.
    let root = scratch();
    let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap();
    repo(&workspace);
    let secret = "wf05-\"quoted\\credential\n\t";
    let script = "import json,os; print(json.dumps({'credential':os.environ['WF05_SECRET']}))";
    let output = Command::new(python())
        .args(["-c", REMOTE_COMMAND, r#"{"names":["WF05_SECRET"],"timeout":10}"#, "--", python(), "-c", script])
        .env("WF05_SECRET", secret).output().unwrap();
    assert!(output.status.success());
    let envelope = String::from_utf8(output.stdout).unwrap();
    let remote: serde_json::Value = serde_json::from_str(envelope.trim().strip_prefix("TUIC_COMMAND_RESULT=").unwrap()).unwrap();
    let decoded: serde_json::Value = serde_json::from_str(remote["stdout"].as_str().unwrap()).unwrap();
    assert_eq!(decoded["credential"], "[REDACTED]", "secret recoverable from durable envelope");
    let mut values = BTreeMap::new();
    values.insert("secret".into(), secret.into());
    let local = redact_captured(serde_json::to_string(&serde_json::json!({"credential":secret})).unwrap().as_bytes(), &values);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&local).unwrap()["credential"], "[REDACTED]");
}

#[test]
fn dual_stream_escaped_remote_output_fits_outer_capture() {
    // catches F4: valid bounded streams becoming a truncated, undecodable receipt envelope.
    let mut command = Command::new(python());
    command.args(["-c", REMOTE_COMMAND, r#"{"names":[],"timeout":10}"#, "--", python(), "-c", "import sys; sys.stdout.write(chr(1)*600000); sys.stderr.write(chr(2)*600000)"])
        .stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = execute_process(command, Duration::from_secs(20), None, false).unwrap();
    assert_eq!(output.exit_code, 0);
    let envelope = String::from_utf8(output.stdout).unwrap();
    let remote: serde_json::Value = serde_json::from_str(envelope.trim().strip_prefix("TUIC_COMMAND_RESULT=").unwrap()).expect("complete envelope within outer capture");
    assert!(remote["stdout"].as_str().unwrap().ends_with("[TRUNCATED]"));
    assert!(remote["stderr"].as_str().unwrap().ends_with("[TRUNCATED]"));
}

#[test]
fn missing_secret_does_not_reserve_named_resource() {
    // catches F5: deterministic preflight failure permanently reserving GUI/mutation resources.
    let root = scratch();
    let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap(); repo(&workspace);
    let db = root.path().join("runs.db");
    let journal = CommandJournal::for_runs(&db);
    let mut spec = definition("print('ok')");
    spec.resources = vec![CommandResource::Gui("preflight".into())];
    spec.secret_refs.insert("SECRET".into(), "WF05_DELIBERATELY_ABSENT_SECRET".into());
    assert!(journal.execute(&spec, &identity("missing-secret"), &workspace, &db, &adapters(root.path())).is_err());
    spec.secret_refs.clear();
    let receipt = journal.execute(&spec, &identity("replacement"), &workspace, &db, &adapters(root.path())).expect("preflight failure must not lock subsequent command");
    assert!(receipt.succeeded);
}

#[cfg(unix)]
fn isolated_gate(command: &mut Command, root: &Path) {
    command.env("BUILD_SLOT_DIR", root).env("BUILD_SLOT_FREE", "100").env("BUILD_SLOT_LOAD", "0")
        .env("BUILD_SLOT_DISK_GIB", "100").env("BUILD_SLOT_AGENTS", "0").env("BUILD_SLOT_POLL", "1").env("BUILD_SLOT_GRACE", "1");
}

#[cfg(unix)]
#[test]
fn mutation_wrapper_timeout_stops_separate_child_group() {
    // catches F2: killing wrapper PGID but certifying stop while its --gate group still writes.
    let root = scratch();
    let marker = root.path().join("writes");
    let pid_file = root.path().join("child.pid");
    let script = "import os,sys,time; open(sys.argv[2],'w').write(str(os.getpid())); os.close(1); os.close(2); f=open(sys.argv[1],'a');\nfor _ in range(150): f.write('x'); f.flush(); time.sleep(.02)";
    let wrapper = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/workflows/command/fixtures/build-slot.sh");
    let mut command = Command::new("bash");
    command.arg(wrapper).args(["--gate", python(), "-c", script]).arg(&marker).arg(&pid_file)
        .env("RB_BIN", "unused").stdout(Stdio::piped()).stderr(Stdio::piped());
    isolated_gate(&mut command, &root.path().join("slots"));
    let output = execute_process(command, Duration::from_secs(2), None, false).unwrap();
    assert_eq!(output.exit_code, -1);
    let before = std::fs::metadata(&marker).expect("gate child started").len();
    std::thread::sleep(Duration::from_millis(150));
    let after = std::fs::metadata(&marker).unwrap().len();
    // Cleanup only the child group recorded by this test, even on the RED implementation.
    let pid: i32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
    unsafe { libc::kill(-pid, libc::SIGKILL); }
    assert_eq!(before, after, "separately grouped gate child continued mutating after receipt");
}

#[cfg(unix)]
#[test]
fn local_wrapper_uses_same_freeze_identity_as_journal() {
    // catches F6: RB_AGENT_NAME admission disagreeing with BUILD_SLOT_AGENT freeze exception.
    // Environment isolation is process-local, never the concurrent parent test process.
    if std::env::var_os("WF05_IDENTITY_CHILD").is_none() {
        let root = scratch();
        std::fs::write(root.path().join("BUILD_FREEZE"), "allowed: wf05\n").unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "workflows::command::executor::tests::local_wrapper_uses_same_freeze_identity_as_journal", "--nocapture"])
            .env("WF05_IDENTITY_CHILD", root.path()).env_remove("BUILD_SLOT_AGENT");
        isolated_gate(&mut command, root.path());
        let output = command.output().unwrap();
        assert!(output.status.success(), "{} {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    }
    let root = scratch(); let workspace = root.path().join("repo");
    std::fs::create_dir(&workspace).unwrap(); repo(&workspace);
    let mut adapter = adapters(root.path());
    adapter.freeze = PathBuf::from(std::env::var_os("WF05_IDENTITY_CHILD").unwrap()).join("BUILD_FREEZE");
    adapter.build_slot = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/workflows/command/fixtures/build-slot.sh");
    let mut spec = definition("print('allowed identity')");
    spec.resources = vec![CommandResource::Mutation]; spec.timeout_secs = 3;
    let db = root.path().join("runs.db"); let journal = CommandJournal::for_runs(&db);
    let receipt = journal.execute(&spec, &identity("allowed"), &workspace, &db, &adapter).unwrap();
    assert!(receipt.succeeded, "{} {}", receipt.reason, receipt.stderr);
    adapter.agent_name = "other".into();
    assert!(journal.execute(&spec, &identity("denied"), &workspace, &db, &adapter).is_err());
    assert_eq!(journal.observations("run").unwrap().last().unwrap().state, "queued");
}
