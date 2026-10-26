"""Focused launcher contracts. No test starts a desktop application."""
import json
import os
import re
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parent.parent


class AgentTestPlanTests(unittest.TestCase):
    def plan(self, *arguments, **environment):
        env = {**os.environ, **environment}
        env.pop("CARGO_TARGET_DIR", None)
        result = subprocess.run(
            ["make", "--no-print-directory", "agent-test", "AGENT_TEST_DRY_RUN=1", *arguments],
            cwd=ROOT, env=env, text=True, capture_output=True, check=True,
        )
        return json.loads(result.stdout)

    def test_dry_run_cannot_reuse_daily_config_binary_or_listener(self):
        binary = ROOT / ".tmp/agent-test/agent-test/tuicommander-agent-test"
        before = binary.stat() if binary.exists() else None
        plan = self.plan(TUIC_APP_INSTANCE="", TUIC_PORT="9876", TUIC_DEV_BIN_DIR="/unsafe")
        self.assertEqual(plan["environment"]["TUIC_APP_INSTANCE"], "agent-test")
        self.assertEqual(plan["environment"]["TUIC_PORT"], "19876")
        self.assertTrue(plan["config_dir"].endswith("/com.tuic.commander/instances/agent-test"))
        self.assertEqual(Path(plan["binary"]).name, "tuicommander-agent-test")
        self.assertTrue(Path(plan["binary"]).is_relative_to(ROOT / ".tmp"))
        self.assertEqual(plan["listener"], "127.0.0.1:19876 (no port fallback)")
        self.assertEqual(plan["disabled"], ["global shortcuts", "dictation/microphone", "Telegram", "updater", "global MCP install", "global CLI mutations", "global agent hooks", "deep links", "sleep inhibition"])
        self.assertIn("tauri/custom-protocol", plan["cargo"])
        self.assertNotIn("dictation", plan["cargo"])
        self.assertNotIn("CARGO_TARGET_DIR", plan["environment"])
        self.assertEqual(binary.stat() if binary.exists() else None, before,
                         "dry run must neither create nor replace an existing warm executable")

    def test_speech_output_is_explicit_and_keeps_capture_disabled(self):
        # Bug caught: speech reproduction enables the microphone or daily state.
        plan = self.plan("AGENT_TEST_INSTANCE=agent-test-1996", "AGENT_TEST_SPEECH_OUTPUT=1")
        self.assertEqual(plan["environment"]["TUIC_AGENT_TEST_SPEECH_OUTPUT"], "1")
        self.assertIn("tauri/custom-protocol,dictation", plan["cargo"])
        self.assertIn("microphone", plan["disabled"])
        self.assertIn("global shortcuts", plan["disabled"])
        result = subprocess.run(["make", "--no-print-directory", "agent-test",
                                 "AGENT_TEST_DRY_RUN=1", "AGENT_TEST_SPEECH_OUTPUT=1"],
                                cwd=ROOT, text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("agent-test-<name>", result.stderr)

    def test_agent_test_window_profile_contract(self):
        # Contract only: native visibility/paint/focus are observed separately
        # by agent-test-visible.py, not inferred from these settings/source checks.
        plan = self.plan()
        override = json.loads(plan["build_environment"]["TAURI_CONFIG"])
        window = override.get("app", {}).get("windows", [{}])[0]
        for key, expected in {
            "visible": False,
            "focus": False,
            "focusable": False,
            "width": 1400,
            "height": 900,
            "minWidth": 640,
            "minHeight": 420,
            "alwaysOnTop": True,
            "backgroundThrottling": "disabled",
        }.items():
            with self.subTest(setting=key):
                self.assertEqual(window.get(key), expected)
        capabilities = override["app"]["security"]["capabilities"]
        self.assertEqual(capabilities[0], "default")
        self.assertEqual(capabilities[1]["windows"], ["main"])
        self.assertEqual(capabilities[1]["permissions"],
                         ["core:window:deny-set-focus", "core:window:deny-show"])
        source = (ROOT / "src-tauri/src/lib.rs").read_text()
        # The negated runtime policy is essential: test=false, daily=true.
        # Window focus=false alone cannot stop Tauri's application activation.
        self.assertTrue(re.search(r'let builder = tauri::Builder::default\(\);\s*#\[cfg\(target_os = "macos"\)\]\s*let builder = builder\.activate_ignoring_other_apps\(!agent_test::active\(\)\);', source), "macOS activation must be false for agent-test and true for daily")
        self.assertRegex(source, r'if !agent_test::active\(\) \{\s*sanitize_window_state\(\);\s*\}')
        self.assertTrue("agent_test::present_without_focus(&window)" in source, "Ready must present agent-test without making it key")
        policy = (ROOT / "src-tauri/src/agent_test.rs").read_text()
        self.assertTrue("native.orderFront(None)" in policy, "agent-test must use the non-key native presentation path")
        # Bug caught (2095): a covered test window turns the page hidden and stops rAF.
        self.assertIn("_setWindowOcclusionDetectionEnabled: false", policy)
        # Bug caught (2095): Tao's launch activation makes the test app frontmost.
        self.assertIn("app.set_activation_policy(tauri::ActivationPolicy::Prohibited)", policy)
        self.assertIn(".map(agent_test::without_launch_activation)", source)
        self.assertNotIn("makeKeyAndOrderFront", policy)
        self.assertNotIn(".show()", policy)
        self.assertTrue(".with_filter(|_| !agent_test::active())" in source, "saved window restore must not focus agent-test")

    def test_overrides_remain_named_and_use_a_separate_socket(self):
        default = self.plan()
        other = self.plan("AGENT_TEST_INSTANCE=scope-check", "AGENT_TEST_PORT=19877")
        self.assertEqual(other["environment"]["TUIC_APP_INSTANCE"], "scope-check")
        self.assertEqual(other["environment"]["TUIC_PORT"], "19877")
        self.assertNotEqual(default["socket"], other["socket"])
        self.assertLess(len(other["socket"].encode()), 104)
        # Bug caught: agents follow the TCP listener into a 401 instead of
        # connecting to the selected instance's unauthenticated Unix socket.
        for plan in [default, other]:
            self.assertEqual(plan["connect"],
                             f"curl --unix-socket '{plan['socket']}' http://localhost/health # Agents use this socket; TCP requires Basic auth")

    def test_static_wiring_keeps_guards_before_machine_wide_effects(self):
        # Static audit requested by the story: dropping a runtime guard must fail
        # even though this peer cannot start the desktop app to observe OS effects.
        guarded = [
            ("global_hotkey.rs", "pub fn register(", "app.global_shortcut().register"),
            ("dictation/ownership.rs", "pub fn acquire(", "let open ="),
            ("telegram/native.rs", "pub(crate) fn start(", "super::settings::paths()"),
            ("telegram/settings.rs", "pub(crate) async fn change_owner_with_phase(", "ensure_directory(&paths)"),
            ("agent_mcp.rs", "fn launch_owns_agent_configs(", 'std::env::var("TUIC_MCP_CONFIG_OWNER")'),
            ("agent_mcp.rs", "fn install_spec(", "ensure_spec_entry("),
            ("updater.rs", "pub async fn check_update_channel(", "get_channel_config("),
        ]
        for file, signature, effect in guarded:
            with self.subTest(file=file, signature=signature):
                source = (ROOT / "src-tauri/src" / file).read_text().split(signature, 1)[1]
                self.assertLess(source.index("crate::agent_test::active()"), source.index(effect))
        source = (ROOT / "src-tauri/src/lib.rs").read_text()
        updater_setup = source.split('.setup(move |app| {', 1)[1]
        self.assertLess(updater_setup.index('if !agent_test::active()'), updater_setup.index('.plugin(tauri_plugin_updater'))
        listener = (ROOT / "src-tauri/src/mcp_http/mod.rs").read_text()
        self.assertRegex(listener, r'let host = if crate::agent_test::active\(\)\s*\{\s*"127.0.0.1"')
        self.assertRegex(listener, r'let attempts = if base_port == 0 \|\| crate::agent_test::active\(\)\s*\{\s*1')
        policy = (ROOT / "src-tauri/src/agent_test.rs").read_text()
        self.assertIn('cfg!(feature = "dictation")', policy)
        self.assertIn('cfg!(debug_assertions)', policy)

    def test_invalid_overrides_fail_closed_instead_of_using_production(self):
        for argument in ["AGENT_TEST_INSTANCE=", "AGENT_TEST_INSTANCE=default", "AGENT_TEST_INSTANCE=../escape", "AGENT_TEST_PORT=9876", "AGENT_TEST_PORT=0", "AGENT_TEST_PORT=65536"]:
            with self.subTest(argument=argument):
                result = subprocess.run(["make", "--no-print-directory", "agent-test", "AGENT_TEST_DRY_RUN=1", argument], cwd=ROOT, text=True, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("agent-test:", result.stderr)


if __name__ == "__main__":
    unittest.main()
