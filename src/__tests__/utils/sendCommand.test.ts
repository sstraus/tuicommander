import { describe, it, expect, afterEach } from "vitest";
import { sendCommand } from "../../utils/sendCommand";

/**
 * Fake writer that records every call in order. Returns a resolved promise
 * so sendCommand's internal awaits don't stall.
 */
function makeRecorder() {
  const calls: string[] = [];
  const writeFn = async (data: string): Promise<void> => {
    calls.push(data);
  };
  return { writeFn, calls };
}

/**
 * Replace navigator.platform for the duration of a test so isWindows()
 * returns the expected value. Restored via afterEach.
 */
function setPlatform(value: string) {
  Object.defineProperty(navigator, "platform", {
    value,
    configurable: true,
  });
}

describe("sendCommand", () => {
  const originalPlatform = navigator.platform;

  afterEach(() => {
    setPlatform(originalPlatform);
  });

  it("always sends Ctrl-U prefix when an agent is attached (ignores shellFamily)", async () => {
    setPlatform("Win32");
    const { writeFn, calls } = makeRecorder();
    await sendCommand(writeFn, "ls", "claude", "windows-native");
    expect(calls).toEqual(["\x15ls", "\r"]);
  });

  it("sends Ctrl-U for POSIX shellFamily even when running on Windows (git-bash regression)", async () => {
    setPlatform("Win32");
    const { writeFn, calls } = makeRecorder();
    await sendCommand(writeFn, "ls", null, "posix");
    expect(calls).toEqual(["\x15ls", "\r"]);
  });

  it("skips Ctrl-U for windows-native shellFamily when no agent", async () => {
    setPlatform("Win32");
    const { writeFn, calls } = makeRecorder();
    await sendCommand(writeFn, "dir", null, "windows-native");
    expect(calls).toEqual(["dir", "\r"]);
  });

  it("falls back to platform heuristic for unknown shellFamily on Windows (skip)", async () => {
    setPlatform("Win32");
    const { writeFn, calls } = makeRecorder();
    await sendCommand(writeFn, "echo hi", null, "unknown");
    expect(calls).toEqual(["echo hi", "\r"]);
  });

  it("falls back to platform heuristic for unknown shellFamily on macOS (send Ctrl-U)", async () => {
    setPlatform("MacIntel");
    const { writeFn, calls } = makeRecorder();
    await sendCommand(writeFn, "ls", null, "unknown");
    expect(calls).toEqual(["\x15ls", "\r"]);
  });

  it("falls back to platform heuristic when shellFamily omitted on macOS", async () => {
    setPlatform("MacIntel");
    const { writeFn, calls } = makeRecorder();
    await sendCommand(writeFn, "ls");
    expect(calls).toEqual(["\x15ls", "\r"]);
  });

  it("falls back to platform heuristic when shellFamily omitted on Windows", async () => {
    setPlatform("Win32");
    const { writeFn, calls } = makeRecorder();
    await sendCommand(writeFn, "dir");
    expect(calls).toEqual(["dir", "\r"]);
  });

  it("sends Enter as a separate write regardless of prefix decision", async () => {
    setPlatform("MacIntel");
    const { writeFn, calls } = makeRecorder();
    await sendCommand(writeFn, "foo", null, "posix");
    expect(calls.length).toBe(2);
    expect(calls[1]).toBe("\r");
  });
});
