import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { retryWrite } from "../utils/retryWrite";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("retryWrite", () => {
  it("resolves immediately on success", async () => {
    const fn = vi.fn().mockResolvedValue(undefined);
    const promise = retryWrite(fn);
    await vi.runAllTimersAsync();
    await promise;
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("retries on failure and succeeds on second attempt", async () => {
    const fn = vi.fn()
      .mockRejectedValueOnce(new Error("network"))
      .mockResolvedValueOnce(undefined);
    const promise = retryWrite(fn);
    await vi.runAllTimersAsync();
    await promise;
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("retries up to 3 times total then rejects", async () => {
    const fn = vi.fn().mockRejectedValue(new Error("offline"));
    const promise = retryWrite(fn).catch((e) => e);
    await vi.runAllTimersAsync();
    const result = await promise;
    expect(result).toBeInstanceOf(Error);
    expect((result as Error).message).toBe("offline");
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it("succeeds on third attempt", async () => {
    const fn = vi.fn()
      .mockRejectedValueOnce(new Error("fail1"))
      .mockRejectedValueOnce(new Error("fail2"))
      .mockResolvedValueOnce(undefined);
    const promise = retryWrite(fn);
    await vi.runAllTimersAsync();
    await promise;
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it("uses exponential backoff delays", async () => {
    const fn = vi.fn().mockRejectedValue(new Error("offline"));
    const promise = retryWrite(fn).catch(() => {});

    // First call is immediate
    expect(fn).toHaveBeenCalledTimes(1);

    // After 500ms: second call
    await vi.advanceTimersByTimeAsync(500);
    expect(fn).toHaveBeenCalledTimes(2);

    // After 1000ms more: third call
    await vi.advanceTimersByTimeAsync(1000);
    expect(fn).toHaveBeenCalledTimes(3);

    await vi.runAllTimersAsync();
    await promise;
  });
});
