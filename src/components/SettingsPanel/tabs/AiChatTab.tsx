import { Component, For, Show, createEffect, createSignal, onMount } from "solid-js";
import { invoke } from "../../../invoke";
import { appLogger } from "../../../stores/appLogger";
import { cx } from "../../../utils";
import s from "../Settings.module.css";

// ---------------------------------------------------------------------------
// Types matching Rust backend
// ---------------------------------------------------------------------------

type ToolPhase = "plan" | "search" | "read" | "write";

interface ScheduledJob {
  id: string;
  cron_expr: string;
  goal: string;
  target_session?: string | null;
  max_duration_secs: number;
  enabled: boolean;
}

interface SchedulerConfig {
  jobs: ScheduledJob[];
}

interface AiChatConfig {
  provider: string;
  model: string;
  base_url?: string | null;
  temperature: number;
  context_lines: number;
  experimental_ai_block_enrichment: boolean;
  agent_model_overrides?: Record<ToolPhase, string> | null;
}

interface OllamaModel {
  name: string;
  size: number;
}

interface OllamaStatus {
  available: boolean;
  models: OllamaModel[];
}

// ---------------------------------------------------------------------------
// Provider definitions
// ---------------------------------------------------------------------------

interface ProviderDef {
  value: string;
  label: string;
  defaultUrl: string;
  modelPlaceholder: string;
  needsApiKey: boolean;
}

const PROVIDERS: ProviderDef[] = [
  { value: "ollama",     label: "Ollama",      defaultUrl: "http://localhost:11434/v1/", modelPlaceholder: "llama3.2",                  needsApiKey: false },
  { value: "anthropic",  label: "Anthropic",   defaultUrl: "",                               modelPlaceholder: "claude-sonnet-4-20250514", needsApiKey: true  },
  { value: "openai",     label: "OpenAI",      defaultUrl: "",                               modelPlaceholder: "gpt-4o",                   needsApiKey: true  },
  { value: "openrouter", label: "OpenRouter",  defaultUrl: "https://openrouter.ai/api/v1/",  modelPlaceholder: "anthropic/claude-sonnet-4-20250514", needsApiKey: true  },
  { value: "custom",     label: "Custom",      defaultUrl: "",                           modelPlaceholder: "model-name",               needsApiKey: true  },
];

function getProviderDef(value: string): ProviderDef | undefined {
  return PROVIDERS.find((p) => p.value === value);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const AiChatTab: Component = () => {
  // Form state
  const [provider, setProvider] = createSignal("ollama");
  const [model, setModel] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [temperature, setTemperature] = createSignal(0.7);
  const [contextLines, setContextLines] = createSignal(150);
  const [blockEnrichment, setBlockEnrichment] = createSignal(false);
  const [phaseSearch, setPhaseSearch] = createSignal("");
  const [phaseRead, setPhaseRead] = createSignal("");
  const [phaseWrite, setPhaseWrite] = createSignal("");

  // API key state
  const [apiKey, setApiKey] = createSignal("");
  const [hasKey, setHasKey] = createSignal(false);
  const [showKey, setShowKey] = createSignal(false);

  // Ollama state
  const [ollamaStatus, setOllamaStatus] = createSignal<OllamaStatus | null>(null);
  const [ollamaChecking, setOllamaChecking] = createSignal(false);

  // Connection test state
  const [testing, setTesting] = createSignal(false);
  const [testResult, setTestResult] = createSignal<{ ok: boolean; msg: string } | null>(null);

  // Scheduler state
  const [schedulerJobs, setSchedulerJobs] = createSignal<ScheduledJob[]>([]);
  const [newCron, setNewCron] = createSignal("");
  const [newGoal, setNewGoal] = createSignal("");

  // Debounce timer for auto-save
  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  // ---------------------------------------------------------------------------
  // Config persistence
  // ---------------------------------------------------------------------------

  const buildConfig = (): AiChatConfig => {
    const overrides: Record<string, string> = {};
    if (phaseSearch()) overrides.search = phaseSearch();
    if (phaseRead()) overrides.read = phaseRead();
    if (phaseWrite()) overrides.write = phaseWrite();
    return {
      provider: provider(),
      model: model(),
      base_url: baseUrl() || null,
      temperature: temperature(),
      context_lines: contextLines(),
      experimental_ai_block_enrichment: blockEnrichment(),
      agent_model_overrides: Object.keys(overrides).length > 0 ? overrides as Record<ToolPhase, string> : null,
    };
  };

  const saveConfig = () => {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        await invoke("save_ai_chat_config", { config: buildConfig() });
      } catch (e) {
        appLogger.error("config", "Failed to save AI Chat config", e);
      }
    }, 500);
  };

  // Load config on mount
  onMount(async () => {
    try {
      const config = await invoke<AiChatConfig>("load_ai_chat_config");
      setProvider(config.provider || "ollama");
      setModel(config.model || "");
      setBaseUrl(config.base_url || "");
      setTemperature(config.temperature ?? 0.7);
      setContextLines(config.context_lines ?? 150);
      setBlockEnrichment(config.experimental_ai_block_enrichment ?? false);
      const ov = config.agent_model_overrides;
      if (ov) {
        setPhaseSearch(ov.search || "");
        setPhaseRead(ov.read || "");
        setPhaseWrite(ov.write || "");
      }
    } catch (e) {
      appLogger.warn("config", "Failed to load AI Chat config", e);
    }

    // Check if API key exists
    try {
      const exists = await invoke<boolean>("has_ai_chat_api_key");
      setHasKey(exists);
    } catch (e) {
      appLogger.warn("config", "Failed to check AI Chat API key", e);
    }

    // Load scheduler config
    try {
      const sc = await invoke<SchedulerConfig>("load_scheduler_config");
      setSchedulerJobs(sc.jobs);
    } catch (e) {
      appLogger.warn("config", "Failed to load scheduler config", e);
    }
  });

  // Check Ollama status when provider is ollama
  const checkOllama = async () => {
    setOllamaChecking(true);
    try {
      const status = await invoke<OllamaStatus>("check_ollama_status");
      setOllamaStatus(status);
    } catch (e) {
      appLogger.warn("config", "Ollama status check failed", e);
      setOllamaStatus({ available: false, models: [] });
    } finally {
      setOllamaChecking(false);
    }
  };

  createEffect(() => {
    if (provider() === "ollama") {
      checkOllama();
    } else {
      setOllamaStatus(null);
    }
  });

  // ---------------------------------------------------------------------------
  // Handlers
  // ---------------------------------------------------------------------------

  const handleProviderChange = (value: string) => {
    setProvider(value);
    const def = getProviderDef(value);
    if (def) {
      setBaseUrl(def.defaultUrl);
    }
    // Clear test result on provider change
    setTestResult(null);
    saveConfig();
  };

  const handleModelChange = (value: string) => {
    setModel(value);
    saveConfig();
  };

  const handleBaseUrlChange = (value: string) => {
    setBaseUrl(value);
    saveConfig();
  };

  const handleTemperatureChange = (value: number) => {
    setTemperature(value);
    saveConfig();
  };

  const handleContextLinesChange = (value: number) => {
    setContextLines(value);
    saveConfig();
  };

  const handleBlockEnrichmentChange = (value: boolean) => {
    setBlockEnrichment(value);
    saveConfig();
  };

  const handleSaveKey = async () => {
    const key = apiKey().trim();
    if (!key) return;
    try {
      await invoke("save_ai_chat_api_key", { key });
      setApiKey("");
      setHasKey(true);
      setTestResult(null);
      appLogger.info("config", "AI Chat API key saved to keyring");
    } catch (e) {
      appLogger.error("config", "Failed to save AI Chat API key", e);
    }
  };

  const handleDeleteKey = async () => {
    try {
      await invoke("delete_ai_chat_api_key");
      setHasKey(false);
      setTestResult(null);
      appLogger.info("config", "AI Chat API key deleted");
    } catch (e) {
      appLogger.error("config", "Failed to delete AI Chat API key", e);
    }
  };

  const handleTestConnection = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const msg = await invoke<string>("test_ai_chat_connection");
      setTestResult({ ok: true, msg });
      appLogger.info("config", "AI Chat connection test passed");
    } catch (e) {
      const errMsg = String(e);
      setTestResult({ ok: false, msg: errMsg });
      appLogger.warn("config", "AI Chat connection test failed", errMsg);
    } finally {
      setTesting(false);
    }
  };

  // ---------------------------------------------------------------------------
  // Scheduler handlers
  // ---------------------------------------------------------------------------

  const saveScheduler = async (jobs: ScheduledJob[]) => {
    try {
      await invoke("save_scheduler_config", { config: { jobs } });
      setSchedulerJobs(jobs);
    } catch (e) {
      appLogger.error("config", "Failed to save scheduler config", e);
    }
  };

  const handleAddJob = async () => {
    const cron = newCron().trim();
    const goal = newGoal().trim();
    if (!cron || !goal) return;
    const id = `job-${Date.now().toString(36)}`;
    const job: ScheduledJob = {
      id,
      cron_expr: cron,
      goal,
      target_session: null,
      max_duration_secs: 300,
      enabled: true,
    };
    await saveScheduler([...schedulerJobs(), job]);
    setNewCron("");
    setNewGoal("");
  };

  const handleToggleJob = async (id: string) => {
    const jobs = schedulerJobs().map((j) =>
      j.id === id ? { ...j, enabled: !j.enabled } : j,
    );
    await saveScheduler(jobs);
  };

  const handleRemoveJob = async (id: string) => {
    await saveScheduler(schedulerJobs().filter((j) => j.id !== id));
  };

  // ---------------------------------------------------------------------------
  // Derived
  // ---------------------------------------------------------------------------

  const providerDef = () => getProviderDef(provider());
  const isOllama = () => provider() === "ollama";
  const ollamaAvailable = () => ollamaStatus()?.available ?? false;
  const ollamaModels = () => ollamaStatus()?.models ?? [];
  const needsApiKey = () => providerDef()?.needsApiKey ?? true;

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div class={s.section}>
      {/* ── Provider ── */}
      <h3>Provider</h3>

      <div class={s.group}>
        <label>Provider</label>
        <select
          value={provider()}
          onChange={(e) => handleProviderChange(e.currentTarget.value)}
        >
          <For each={PROVIDERS}>
            {(p) => <option value={p.value}>{p.label}</option>}
          </For>
        </select>
      </div>

      {/* Ollama status indicator */}
      <Show when={isOllama()}>
        <div class={s.group}>
          <div class={s.mcpStatusRow}>
            <Show
              when={!ollamaChecking()}
              fallback={
                <span class={s.mcpStatusText}>Checking Ollama...</span>
              }
            >
              <span
                class={cx(s.mcpStatusDot, ollamaAvailable() ? s.running : s.stopped)}
              />
              <span class={s.mcpStatusText}>
                {ollamaAvailable()
                  ? `Connected — ${ollamaModels().length} model${ollamaModels().length !== 1 ? "s" : ""} available`
                  : "Not detected — is Ollama running?"}
              </span>
              <button class={s.inlineBtn} onClick={checkOllama}>
                Recheck
              </button>
            </Show>
          </div>
        </div>
      </Show>

      {/* ── Model ── */}
      <h3>Model</h3>

      <div class={s.group}>
        <label>Model</label>
        <Show
          when={isOllama() && ollamaAvailable() && ollamaModels().length > 0}
          fallback={
            <input
              type="text"
              value={model()}
              placeholder={`e.g. ${providerDef()?.modelPlaceholder ?? "model-name"}`}
              onInput={(e) => handleModelChange(e.currentTarget.value)}
            />
          }
        >
          <select
            value={model()}
            onChange={(e) => handleModelChange(e.currentTarget.value)}
          >
            <option value="">— Select a model —</option>
            <For each={ollamaModels()}>
              {(m) => <option value={m.name}>{m.name}</option>}
            </For>
          </select>
        </Show>
      </div>

      {/* ── Connection ── */}
      <h3>Connection</h3>

      <div class={s.group}>
        <label>Base URL</label>
        <input
          type="text"
          value={baseUrl()}
          placeholder={providerDef()?.defaultUrl || "Leave empty for default, or enter custom URL"}
          onInput={(e) => handleBaseUrlChange(e.currentTarget.value)}
        />
        <p class={s.hint}>
          {providerDef()?.defaultUrl
            ? "Pre-filled for this provider. Edit for custom endpoints."
            : "Leave empty to use the provider's default endpoint."}
        </p>
      </div>

      {/* API Key */}
      <Show when={needsApiKey()}>
        <div class={s.group}>
          <label>API Key</label>
          <Show
            when={!hasKey()}
            fallback={
              <div>
                <div class={s.passwordRow}>
                  <input
                    type="password"
                    class={s.input}
                    value={apiKey()}
                    placeholder="Key saved — enter new key to replace"
                    onInput={(e) => setApiKey(e.currentTarget.value)}
                  />
                </div>
                <div style={{ display: "flex", "align-items": "center", gap: "8px", "margin-top": "8px" }}>
                  <span style={{ color: "var(--success)", "font-size": "var(--font-sm)" }}>Key saved</span>
                  <Show when={apiKey().trim()}>
                    <button class={s.testBtn} onClick={handleSaveKey}>
                      Save New Key
                    </button>
                  </Show>
                  <button
                    class={s.testBtn}
                    style={{ color: "var(--error)" }}
                    onClick={handleDeleteKey}
                  >
                    Delete Key
                  </button>
                </div>
              </div>
            }
          >
            <div class={s.passwordRow}>
              <input
                type={showKey() ? "text" : "password"}
                class={s.input}
                value={apiKey()}
                placeholder="Paste your API key"
                onInput={(e) => setApiKey(e.currentTarget.value)}
              />
              <button
                class={s.toggleBtn}
                onClick={() => setShowKey(!showKey())}
                title={showKey() ? "Hide" : "Show"}
              >
                {showKey()
                  ? <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M13.359 11.238C15.06 9.72 16 8 16 8s-3-5.5-8-5.5a7.028 7.028 0 0 0-2.79.588l.77.771A5.944 5.944 0 0 1 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.134 13.134 0 0 1 14.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755-.165.165-.337.328-.517.486l.708.709z"/><path d="M11.297 9.176a3.5 3.5 0 0 0-4.474-4.474l.823.823a2.5 2.5 0 0 1 2.829 2.829l.822.822zm-2.943 1.299.822.822a3.5 3.5 0 0 1-4.474-4.474l.823.823a2.5 2.5 0 0 0 2.829 2.829z"/><path d="M3.35 5.47c-.18.16-.353.322-.518.487A13.134 13.134 0 0 0 1.172 8l.195.288c.335.48.83 1.12 1.465 1.755C4.121 11.332 5.881 12.5 8 12.5c.716 0 1.39-.133 2.02-.36l.77.772A7.029 7.029 0 0 1 8 13.5C3 13.5 0 8 0 8s.939-1.721 2.641-3.238l.708.709zm10.296 8.884-12-12 .708-.708 12 12-.708.708z"/></svg>
                  : <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M16 8s-3-5.5-8-5.5S0 8 0 8s3 5.5 8 5.5S16 8 16 8zM1.173 8a13.133 13.133 0 0 1 1.66-2.043C4.12 4.668 5.88 3.5 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.133 13.133 0 0 1 14.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755C11.879 11.332 10.119 12.5 8 12.5c-2.12 0-3.879-1.168-5.168-2.457A13.134 13.134 0 0 1 1.172 8z"/><path d="M8 5.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM4.5 8a3.5 3.5 0 1 1 7 0 3.5 3.5 0 0 1-7 0z"/></svg>
                }
              </button>
            </div>
            <Show when={apiKey().trim()}>
              <button
                class={s.testBtn}
                style={{ "margin-top": "8px" }}
                onClick={handleSaveKey}
              >
                Save Key
              </button>
            </Show>
          </Show>
          <p class={s.hint}>Stored securely in your OS keyring</p>
        </div>
      </Show>

      {/* Ollama: no key needed note */}
      <Show when={!needsApiKey()}>
        <div class={s.group}>
          <p class={s.hint}>
            No API key required for Ollama (local inference).
          </p>
        </div>
      </Show>

      {/* Test Connection */}
      <div class={s.group}>
        <button
          class={cx(
            s.testBtn,
            testResult()?.ok === true && s.testBtnOk,
            testResult()?.ok === false && s.testBtnFail,
          )}
          disabled={testing() || !model()}
          onClick={handleTestConnection}
          title={!model() ? "Enter a model name first" : ""}
        >
          {testing()
            ? "Testing..."
            : testResult()?.ok === true
              ? "Connected"
              : testResult()?.ok === false
                ? "Failed — Retry?"
                : !model()
                  ? "Test Connection (set model first)"
                  : "Test Connection"}
        </button>
        <Show when={testResult()}>
          {(result) => (
            <p
              class={s.hint}
              style={{
                color: result().ok ? "var(--success)" : "var(--error)",
                "margin-top": "8px",
              }}
            >
              {result().msg.slice(0, 200)}
            </p>
          )}
        </Show>
      </div>

      {/* ── Parameters ── */}
      <h3>Parameters</h3>

      <div class={s.group}>
        <label>Context Lines</label>
        <div class={s.slider}>
          <input
            type="range"
            min={50}
            max={500}
            step={10}
            value={contextLines()}
            onInput={(e) => handleContextLinesChange(parseInt(e.currentTarget.value))}
          />
          <span>{contextLines()}</span>
        </div>
        <p class={s.hint}>
          Number of terminal output lines included as context per turn (50–500)
        </p>
      </div>

      <div class={s.group}>
        <label>Temperature</label>
        <div class={s.slider}>
          <input
            type="range"
            min={0}
            max={1}
            step={0.1}
            value={temperature()}
            onInput={(e) => handleTemperatureChange(parseFloat(e.currentTarget.value))}
          />
          <span>{temperature().toFixed(1)}</span>
        </div>
        <p class={s.hint}>
          Controls randomness of responses (0.0 = deterministic, 1.0 = creative)
        </p>
      </div>

      {/* ── Agent Model Overrides ── */}
      <h3>Agent Model Overrides</h3>

      <div class={s.group}>
        <p class={s.hint} style={{ "margin-bottom": "8px" }}>
          Use a cheaper model for search/read iterations and the main model for
          writes. Leave blank to use the default model for all phases.
        </p>
        <label>Search phase</label>
        <input
          class={s.input}
          type="text"
          placeholder={model() || "Same as default"}
          value={phaseSearch()}
          onInput={(e) => { setPhaseSearch(e.currentTarget.value); saveConfig(); }}
        />
        <label>Read phase</label>
        <input
          class={s.input}
          type="text"
          placeholder={model() || "Same as default"}
          value={phaseRead()}
          onInput={(e) => { setPhaseRead(e.currentTarget.value); saveConfig(); }}
        />
        <label>Write phase</label>
        <input
          class={s.input}
          type="text"
          placeholder={model() || "Same as default"}
          value={phaseWrite()}
          onInput={(e) => { setPhaseWrite(e.currentTarget.value); saveConfig(); }}
        />
      </div>

      {/* ── Scheduled Tasks ── */}
      <h3>Scheduled Tasks</h3>

      <div class={s.group}>
        <p class={s.hint} style={{ "margin-bottom": "8px" }}>
          Cron-triggered agent tasks. The agent runs with standard trust level
          (destructive commands require approval).
        </p>

        <For each={schedulerJobs()}>
          {(job) => (
            <div class={s.schedulerRow}>
              <label class={s.schedulerToggle}>
                <input
                  type="checkbox"
                  checked={job.enabled}
                  onChange={() => handleToggleJob(job.id)}
                />
              </label>
              <code class={s.schedulerCron}>{job.cron_expr}</code>
              <span class={s.schedulerGoal}>{job.goal}</span>
              <button
                class={s.schedulerRemove}
                onClick={() => handleRemoveJob(job.id)}
                title="Remove"
              >
                ×
              </button>
            </div>
          )}
        </For>

        <div class={s.schedulerAdd}>
          <input
            type="text"
            class={s.schedulerCronInput}
            value={newCron()}
            placeholder="0 0 * * * *"
            onInput={(e) => setNewCron(e.currentTarget.value)}
          />
          <input
            type="text"
            class={s.schedulerGoalInput}
            value={newGoal()}
            placeholder="Goal (e.g. run tests and report)"
            onInput={(e) => setNewGoal(e.currentTarget.value)}
          />
          <button
            class={s.testBtn}
            disabled={!newCron().trim() || !newGoal().trim()}
            onClick={handleAddJob}
          >
            Add
          </button>
        </div>
        <p class={s.hint}>
          Cron format: sec min hour day month weekday (6 fields).
          Example: <code>0 0 * * * *</code> = top of every hour.
        </p>
      </div>

      {/* ── Experimental ── */}
      <h3>Experimental</h3>

      <div class={s.group}>
        <label>
          <input
            type="checkbox"
            checked={blockEnrichment()}
            onChange={(e) => handleBlockEnrichmentChange(e.currentTarget.checked)}
          />
          {" "}Enrich command blocks with AI metadata
        </label>
        <p class={s.hint}>
          After each shell command completes, send its command line and a short
          output tail to the configured provider to derive a one-line intent.
          Rate-limited to ~10/min. Disabled by default — leaks command output
          to the provider and consumes tokens.
        </p>
      </div>

      <p class={s.hint} style={{ "margin-top": "16px", color: "var(--fg-muted)" }}>
        Settings are saved automatically when changed
      </p>
    </div>
  );
};
