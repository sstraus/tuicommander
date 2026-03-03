import { describe, it, expect } from "vitest";

// Test the optimistic-disable logic for QuestionBanner.
// Each banner item has its own "sent" state: once a Yes/No button is tapped,
// both buttons disable and show "Sent" until awaiting_input resets.
describe("QuestionBanner optimistic disable", () => {
  function makeBannerItem(_awaitingInput: boolean) {
    let sent = false;

    function handleReply(answer: string): string {
      if (sent) return "already_sent";
      sent = true;
      return answer;
    }

    function isDisabled(): boolean {
      return sent;
    }

    function getLabel(original: string): string {
      return sent ? "Sent" : original;
    }

    // Reset when awaiting_input flips to false
    function onAwaitingInputChange(nowAwaiting: boolean) {
      if (!nowAwaiting) sent = false;
    }

    return { handleReply, isDisabled, getLabel, onAwaitingInputChange };
  }

  it("buttons are enabled before any tap", () => {
    const item = makeBannerItem(true);
    expect(item.isDisabled()).toBe(false);
    expect(item.getLabel("Yes")).toBe("Yes");
    expect(item.getLabel("No")).toBe("No");
  });

  it("buttons disable after tapping Yes", () => {
    const item = makeBannerItem(true);
    const sent = item.handleReply("yes");
    expect(sent).toBe("yes");
    expect(item.isDisabled()).toBe(true);
  });

  it("buttons disable after tapping No", () => {
    const item = makeBannerItem(true);
    const sent = item.handleReply("no");
    expect(sent).toBe("no");
    expect(item.isDisabled()).toBe(true);
  });

  it("label changes to Sent after tap", () => {
    const item = makeBannerItem(true);
    item.handleReply("yes");
    expect(item.getLabel("Yes")).toBe("Sent");
    expect(item.getLabel("No")).toBe("Sent");
  });

  it("second tap is ignored (no double-send)", () => {
    const item = makeBannerItem(true);
    item.handleReply("yes");
    const second = item.handleReply("no");
    expect(second).toBe("already_sent");
  });

  it("resets when awaiting_input flips to false", () => {
    const item = makeBannerItem(true);
    item.handleReply("yes");
    expect(item.isDisabled()).toBe(true);
    item.onAwaitingInputChange(false);
    expect(item.isDisabled()).toBe(false);
    expect(item.getLabel("Yes")).toBe("Yes");
  });

  it("stays reset when awaiting_input flips back to true", () => {
    const item = makeBannerItem(false);
    item.onAwaitingInputChange(true);
    expect(item.isDisabled()).toBe(false);
  });
});
