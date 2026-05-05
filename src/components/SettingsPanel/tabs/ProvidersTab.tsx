import {
  Component,
  For,
  Show,
  createMemo,
  createResource,
  createSignal,
  onMount,
} from "solid-js";
import { invoke } from "../../../invoke";
import { providerRegistryStore, type ProviderEntry, type ProviderType, type ModelEntry, type SlotName } from "../../../stores/providerRegistry";
import { agentConfigsStore } from "../../../stores/agentConfigs";
import { AGENTS, AGENT_TYPES, type AgentType } from "../../../agents";
import { useAgentDetection } from "../../../hooks/useAgentDetection";
import { appLogger } from "../../../stores/appLogger";
import s from "../Settings.module.css";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PROVIDER_TYPES: { value: ProviderType; label: string; comingSoon?: boolean }[] = [
  { value: "anthropic",   label: "Anthropic" },
  { value: "open_ai",     label: "OpenAI" },
  { value: "gemini",      label: "Google Gemini" },
  { value: "deep_seek",   label: "DeepSeek" },
  { value: "mistral",     label: "Mistral" },
  { value: "fireworks",   label: "Fireworks AI" },
  { value: "samba_nova",  label: "SambaNova" },
  { value: "moonshot",    label: "Moonshot" },
  { value: "xai",         label: "xAI (Grok)" },
  { value: "zai",         label: "Zhipu AI" },
  { value: "open_router", label: "OpenRouter" },
  { value: "requesty",    label: "Requesty" },
  { value: "lite_llm",    label: "LiteLLM" },
  { value: "ollama",      label: "Ollama (local)" },
  { value: "lm_studio",   label: "LM Studio (local)" },
  { value: "bedrock",     label: "AWS Bedrock", comingSoon: true },
  { value: "vertex",      label: "Google Vertex", comingSoon: true },
  { value: "custom",      label: "Custom (OpenAI-compatible)" },
];

const SLOT_LABELS: Record<SlotName, string> = {
  chat:       "Chat",
  agent_mid:  "Agent (mid)",
  agent_low:  "Agent (low)",
  agent_high: "Agent (high)",
  headless:   "Headless / Smart Prompts",
  enrichment: "Enrichment",
};

const SLOT_DESCRIPTIONS: Record<SlotName, string> = {
  chat:       "Used by the AI Chat panel for interactive conversations about your code.",
  agent_mid:  "Default model for AI agent sessions. Also used as fallback when Agent (low) or Agent (high) are unset.",
  agent_low:  "Used during agent search and read phases — pick a cheaper/faster model to save costs.",
  agent_high: "Used during agent write phases — pick a higher-quality model for code generation.",
  headless:   "Used by Smart Prompts in API mode for one-shot LLM calls (e.g. commit messages, code review).",
  enrichment: "Used for AI block enrichment (command intent labels) and diff triage annotations.",
};

const SLOT_NAMES: SlotName[] = [
  "chat", "agent_mid", "agent_low", "agent_high",
];

const AGENT_FALLBACK_SLOTS: SlotName[] = ["agent_low", "agent_high"];

const LOCAL_PROVIDER_TYPES: ProviderType[] = ["ollama", "lm_studio", "lite_llm"];

function needsApiKey(type: ProviderType): boolean {
  return !LOCAL_PROVIDER_TYPES.includes(type);
}

// ---------------------------------------------------------------------------
// Add Provider Wizard
// ---------------------------------------------------------------------------

const AddProviderForm: Component<{ onAdd: (e: ProviderEntry) => void; onCancel: () => void }> = (props) => {
  const [type, setType] = createSignal<ProviderType>("anthropic");
  const [label, setLabel] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [apiKey, setApiKey] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal("");

  const selectedDef = createMemo(() => PROVIDER_TYPES.find((p) => p.value === type()));

  async function submit() {
    if (!label().trim()) { setError("Label is required"); return; }
    if (selectedDef()?.comingSoon) { setError("This provider is not yet supported"); return; }
    if (needsApiKey(type()) && !apiKey().trim()) { setError("API key is required"); return; }
    setSaving(true);
    setError("");
    try {
      const id = `${type()}-${Date.now()}`;
      const entry: ProviderEntry = {
        id,
        type: type(),
        label: label().trim(),
        base_url: baseUrl().trim() || null,
      };
      if (needsApiKey(type()) && apiKey().trim()) {
        await providerRegistryStore.saveKey(id, apiKey().trim());
      }
      props.onAdd(entry);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div class={s.section} data-testid="add-provider-form">
      <h3>Add Provider</h3>
      <div class={s.group}>
        <label>Type</label>
        <select value={type()} onChange={(e) => setType(e.currentTarget.value as ProviderType)}>
          <For each={PROVIDER_TYPES}>
            {(p) => (
              <option value={p.value}>
                {p.label}{p.comingSoon ? " (coming soon)" : ""}
              </option>
            )}
          </For>
        </select>
      </div>
      <Show when={selectedDef()?.comingSoon}>
        <p class={s.hint}>
          Bedrock and Vertex require additional SDK integration — coming soon.
        </p>
      </Show>
      <div class={s.group}>
        <label>Label</label>
        <input
          type="text"
          placeholder="e.g. Anthropic (personal)"
          value={label()}
          onInput={(e) => setLabel(e.currentTarget.value)}
        />
      </div>
      <Show when={type() !== "anthropic" && type() !== "open_ai" && type() !== "gemini"}>
        <div class={s.group}>
          <label>Base URL (optional)</label>
          <input
            type="text"
            placeholder="Leave blank to use default"
            value={baseUrl()}
            onInput={(e) => setBaseUrl(e.currentTarget.value)}
          />
        </div>
      </Show>
      <Show when={needsApiKey(type())}>
        <div class={s.group}>
          <label>API Key</label>
          <input
            type="password"
            placeholder="sk-…"
            value={apiKey()}
            onInput={(e) => setApiKey(e.currentTarget.value)}
          />
        </div>
      </Show>
      <Show when={error()}>
        <p style={{ color: "var(--error)" }}>{error()}</p>
      </Show>
      <div class={s.actions}>
        <button class={s.saveBtn} onClick={submit} disabled={saving()}>
          {saving() ? "Adding…" : "Add"}
        </button>
        <button onClick={props.onCancel}>Cancel</button>
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Add Model Form
// ---------------------------------------------------------------------------

const AddModelForm: Component<{
  providerId: string;
  onAdd: (m: ModelEntry) => void;
  onCancel: () => void;
}> = (props) => {
  const [modelName, setModelName] = createSignal("");
  const [tier, setTier] = createSignal<ModelEntry["tier"]>("standard");
  const [error, setError] = createSignal("");

  function submit() {
    if (!modelName().trim()) { setError("Model name is required"); return; }
    const id = `model-${props.providerId}-${modelName().trim().replace(/[^a-z0-9]/gi, "-")}`;
    props.onAdd({ id, provider_id: props.providerId, model_name: modelName().trim(), tier: tier() });
  }

  return (
    <div data-testid="add-model-form" class={s.section} style={{ "margin-top": "8px" }}>
      <div class={s.group}>
        <label>Model name</label>
        <input
          type="text"
          placeholder="e.g. claude-sonnet-4-5-20241022"
          value={modelName()}
          onInput={(e) => setModelName(e.currentTarget.value)}
        />
      </div>
      <div class={s.group}>
        <label>Tier</label>
        <select value={tier()} onChange={(e) => setTier(e.currentTarget.value as ModelEntry["tier"])}>
          <option value="economic">Economic</option>
          <option value="standard">Standard</option>
          <option value="premium">Premium</option>
        </select>
      </div>
      <Show when={error()}>
        <p style={{ color: "var(--error)" }}>{error()}</p>
      </Show>
      <div class={s.actions}>
        <button class={s.saveBtn} onClick={submit}>Add model</button>
        <button onClick={props.onCancel}>Cancel</button>
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Provider Card
// ---------------------------------------------------------------------------

const ProviderCard: Component<{ provider: ProviderEntry }> = (props) => {
  const [showAddModel, setShowAddModel] = createSignal(false);
  const [keyInput, setKeyInput] = createSignal("");
  const [savingKey, setSavingKey] = createSignal(false);
  const [keyMsg, setKeyMsg] = createSignal("");

  const models = createMemo(() =>
    providerRegistryStore.state.registry.models.filter(
      (m) => m.provider_id === props.provider.id
    )
  );

  const hasKey = createMemo(() =>
    providerRegistryStore.state.keyStatus[props.provider.id] ?? false
  );

  const isOllama = () => props.provider.type === "ollama";

  const [ollamaModels] = createResource(
    () => isOllama() ? props.provider.id : null,
    async (providerId) => {
      try {
        const result = await invoke<{ available: boolean; models: string[] }>(
          "check_ollama_models",
          { providerId }
        );
        return result.models ?? [];
      } catch (e) {
        appLogger.warn("settings", `Ollama model check failed: ${String(e)}`);
        return [];
      }
    }
  );

  async function saveKey() {
    if (!keyInput().trim()) return;
    setSavingKey(true);
    setKeyMsg("");
    try {
      await providerRegistryStore.saveKey(props.provider.id, keyInput().trim());
      setKeyInput("");
      setKeyMsg("Key saved");
    } catch (e) {
      setKeyMsg(`Error: ${String(e)}`);
    } finally {
      setSavingKey(false);
    }
  }

  async function deleteKey() {
    setSavingKey(true);
    try {
      await providerRegistryStore.deleteKey(props.provider.id);
      setKeyMsg("Key removed");
    } catch (e) {
      setKeyMsg(`Error: ${String(e)}`);
    } finally {
      setSavingKey(false);
    }
  }

  return (
    <div
      data-testid={`provider-card-${props.provider.id}`}
      class={s.groupItem}
    >
      <div class={s.groupRow}>
        <div class={s.groupName}>
          <strong>{props.provider.label}</strong>
          {" "}
          <span class={s.hintInline}>
            ({PROVIDER_TYPES.find((p) => p.value === props.provider.type)?.label ?? props.provider.type})
          </span>
          {" "}
          <span
            style={{ color: hasKey() ? "var(--success)" : undefined }}
            class={!hasKey() ? s.hintInline : undefined}
            data-testid={`key-status-${props.provider.id}`}
          >
            {needsApiKey(props.provider.type)
              ? hasKey() ? "✓ key" : "no key"
              : "no key needed"}
          </span>
        </div>
        <button
          class={s.groupDeleteBtn}
          data-testid={`remove-provider-${props.provider.id}`}
          onClick={() => providerRegistryStore.removeProvider(props.provider.id)}
          title="Remove provider"
        >
          ×
        </button>
      </div>

      {/* Models */}
      <div style={{ "margin-top": "8px" }}>
        <div class={s.hintInline}>Models ({models().length}):</div>
        <For each={models()}>
          {(model) => (
            <div
              data-testid={`model-entry-${model.id}`}
              class={s.groupRow}
              style={{ "margin-top": "4px" }}
            >
              <span>{model.model_name}</span>
              <span class={s.hintInline}>({model.tier})</span>
              <button
                class={s.groupDeleteBtn}
                data-testid={`remove-model-${model.id}`}
                onClick={() => providerRegistryStore.removeModel(model.id)}
                title="Remove model"
              >
                ×
              </button>
            </div>
          )}
        </For>

        {/* Ollama live discovery */}
        <Show when={isOllama() && (ollamaModels()?.length ?? 0) > 0}>
          <div class={s.hint}>Available: {ollamaModels()?.join(", ")}</div>
        </Show>

        <Show
          when={showAddModel()}
          fallback={
            <button
              class={s.inlineBtn}
              data-testid={`add-model-btn-${props.provider.id}`}
              onClick={() => setShowAddModel(true)}
              style={{ "margin-top": "4px" }}
            >
              + Add model
            </button>
          }
        >
          <AddModelForm
            providerId={props.provider.id}
            onAdd={(m) => {
              providerRegistryStore.addModel(m);
              setShowAddModel(false);
            }}
            onCancel={() => setShowAddModel(false)}
          />
        </Show>
      </div>

      {/* API Key management */}
      <Show when={needsApiKey(props.provider.type)}>
        <div style={{ "margin-top": "8px" }}>
          <div class={s.passwordRow}>
            <input
              class={s.input}
              type="password"
              placeholder={hasKey() ? "Replace existing key…" : "Enter API key…"}
              value={keyInput()}
              onInput={(e) => setKeyInput(e.currentTarget.value)}
              data-testid={`key-input-${props.provider.id}`}
            />
            <button class={s.saveBtn} onClick={saveKey} disabled={savingKey() || !keyInput().trim()}>
              Save
            </button>
            <Show when={hasKey()}>
              <button class={s.testBtn} onClick={deleteKey} disabled={savingKey()} style={{ color: "var(--error)" }}>
                Remove
              </button>
            </Show>
          </div>
          <Show when={keyMsg()}>
            <div class={s.hint}>{keyMsg()}</div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Slot Assignments
// ---------------------------------------------------------------------------

interface AiChatConfig {
  temperature: number;
  context_lines: number;
  experimental_ai_block_enrichment: boolean;
}

const SlotAssignments: Component<{ detection: ReturnType<typeof useAgentDetection> }> = (props) => {
  const allModels = () => providerRegistryStore.state.registry.models;
  const isExternalApi = () => agentConfigsStore.getHeadlessAgent() === "api";
  const [enrichmentEnabled, setEnrichmentEnabled] = createSignal(false);
  let cachedConfig: AiChatConfig | null = null;

  onMount(async () => {
    try {
      cachedConfig = await invoke<AiChatConfig>("load_ai_chat_config");
      setEnrichmentEnabled(cachedConfig.experimental_ai_block_enrichment ?? false);
    } catch {
      // leave default false
    }
  });

  async function toggleEnrichment(enabled: boolean) {
    setEnrichmentEnabled(enabled);
    try {
      if (!cachedConfig) cachedConfig = await invoke<AiChatConfig>("load_ai_chat_config");
      cachedConfig = { ...cachedConfig, experimental_ai_block_enrichment: enabled };
      await invoke("save_ai_chat_config", { config: cachedConfig });
    } catch (e) {
      appLogger.error("config", "Failed to save enrichment toggle", e);
      setEnrichmentEnabled(!enabled);
    }
  }

  function modelLabel(modelId: string): string {
    const model = allModels().find((m) => m.id === modelId);
    if (!model) return modelId;
    const provider = providerRegistryStore.state.registry.providers.find(
      (p) => p.id === model.provider_id
    );
    return provider ? `${model.model_name} (${provider.label})` : model.model_name;
  }

  const [testResults, setTestResults] = createSignal<Record<string, string>>({});

  async function testSlot(slot: SlotName) {
    setTestResults((r) => ({ ...r, [slot]: "Testing…" }));
    try {
      const result = await invoke<string>("test_slot_connection", { slot });
      setTestResults((r) => ({ ...r, [slot]: result }));
    } catch (e) {
      setTestResults((r) => ({ ...r, [slot]: `Error: ${String(e)}` }));
    }
  }

  /** Render a single slot row (reused for both the loop and the headless slot) */
  function SlotRow(slotProps: { slot: SlotName; showLabel?: boolean }) {
    const currentModelId = () => providerRegistryStore.state.registry.slots[slotProps.slot];
    const isAgentTier = AGENT_FALLBACK_SLOTS.includes(slotProps.slot);
    const fallbackHint = () => isAgentTier && !currentModelId();
    const showLabel = () => slotProps.showLabel !== false;

    return (
      <div class={s.group} data-testid={`slot-row-${slotProps.slot}`}>
        <Show when={showLabel()}>
          <label>
            {SLOT_LABELS[slotProps.slot]}
            {" "}
            <span class={s.infoBadge}>
              ?
              <span class={s.infoBadgeTip}>{SLOT_DESCRIPTIONS[slotProps.slot]}</span>
            </span>
            <Show when={fallbackHint()}>
              {" "}
              <span class={s.hintInline}>(falls back to agent mid)</span>
            </Show>
          </label>
        </Show>
        <div class={s.passwordRow}>
          <select
            data-testid={`slot-select-${slotProps.slot}`}
            value={currentModelId() ?? ""}
            onChange={(e) => {
              const v = e.currentTarget.value;
              if (v) providerRegistryStore.setSlot(slotProps.slot, v);
              else providerRegistryStore.clearSlot(slotProps.slot);
            }}
          >
            <option value="">— unset —</option>
            <For each={allModels()}>
              {(model) => (
                <option value={model.id}>{modelLabel(model.id)}</option>
              )}
            </For>
          </select>
          <Show when={currentModelId()}>
            <button
              class={s.testBtn}
              data-testid={`test-slot-${slotProps.slot}`}
              onClick={() => testSlot(slotProps.slot)}
              title="Test connection"
            >
              Test
            </button>
          </Show>
        </div>
        <Show when={testResults()[slotProps.slot]}>
          <div class={s.hint}>{testResults()[slotProps.slot]}</div>
        </Show>
      </div>
    );
  }

  return (
    <div class={s.section} data-testid="slot-assignments">
      <h3>Slot Assignments</h3>
      <For each={SLOT_NAMES}>
        {(slot) => <SlotRow slot={slot} />}
      </For>

      {/* Headless / Smart Prompts — unified: agent selector + model slot */}
      <div class={s.group} data-testid="slot-row-headless">
        <label>
          {SLOT_LABELS.headless}
          {" "}
          <span class={s.infoBadge}>
            ?
            <span class={s.infoBadgeTip}>{SLOT_DESCRIPTIONS.headless}</span>
          </span>
        </label>
        <select
          value={agentConfigsStore.getHeadlessAgent() ?? ""}
          onChange={(e) => {
            const val = e.currentTarget.value;
            agentConfigsStore.setHeadlessAgent(val ? val as AgentType : null);
          }}
        >
          <option value="">— Not configured —</option>
          <For each={ALL_AGENT_TYPES.filter((t) => props.detection.isAvailable(t) && AGENTS[t]?.defaultHeadlessTemplate)}>
            {(type) => {
              const configs = () => agentConfigsStore.getRunConfigs(type);
              return (
                <>
                  <Show
                    when={configs().length > 0}
                    fallback={<option value={type}>{AGENTS[type]?.name ?? type}</option>}
                  >
                    <optgroup label={AGENTS[type]?.name ?? type}>
                      <option value={type}>{AGENTS[type]?.name ?? type} (default)</option>
                      <For each={configs()}>
                        {(cfg) => (
                          <option value={`${type}:${cfg.name}`}>
                            {cfg.name}
                            {cfg.is_default ? " (default)" : ""}
                          </option>
                        )}
                      </For>
                    </optgroup>
                  </Show>
                </>
              );
            }}
          </For>
          <option value="api">External API</option>
        </select>
        <Show when={isExternalApi()}>
          {(() => {
            const headlessModelId = () => providerRegistryStore.state.registry.slots["headless"];
            return (
              <div class={s.passwordRow} style={{ "margin-top": "8px" }}>
                <select
                  data-testid="slot-select-headless"
                  value={headlessModelId() ?? ""}
                  onChange={(e) => {
                    const v = e.currentTarget.value;
                    if (v) providerRegistryStore.setSlot("headless", v);
                    else providerRegistryStore.clearSlot("headless");
                  }}
                >
                  <option value="">— select model —</option>
                  <For each={allModels()}>
                    {(model) => (
                      <option value={model.id}>{modelLabel(model.id)}</option>
                    )}
                  </For>
                </select>
                <Show when={headlessModelId()}>
                  <button
                    class={s.testBtn}
                    data-testid="test-slot-headless"
                    onClick={() => testSlot("headless")}
                    title="Test connection"
                  >
                    Test
                  </button>
                </Show>
              </div>
            );
          })()}
          <Show when={testResults()["headless"]}>
            <div class={s.hint}>{testResults()["headless"]}</div>
          </Show>
        </Show>
        <p class={s.hint}>
          {isExternalApi()
            ? "Direct LLM calls via the model selected above"
            : "Agent CLI for one-shot Smart Prompts, or select External API for direct LLM calls"}
          {props.detection.loading() ? " — detecting..." : ""}
        </p>
      </div>

      {/* Enrichment — slot + enable toggle */}
      <div class={s.group} data-testid="slot-row-enrichment">
        <label>
          {SLOT_LABELS.enrichment}
          {" "}
          <span class={s.infoBadge}>
            ?
            <span class={s.infoBadgeTip}>{SLOT_DESCRIPTIONS.enrichment}</span>
          </span>
        </label>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={enrichmentEnabled()}
            onChange={(e) => toggleEnrichment(e.currentTarget.checked)}
          />
          <span>Enrich command blocks with AI metadata</span>
        </div>
        <Show when={enrichmentEnabled()}>
          <SlotRow slot="enrichment" showLabel={false} />
          <p class={s.hint}>
            Rate-limited to ~10/min. Sends command output to the provider.
          </p>
        </Show>
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// ProvidersTab
// ---------------------------------------------------------------------------

const ALL_AGENT_TYPES = AGENT_TYPES.filter((t): t is AgentType => t !== "git" && t !== "api");

export const ProvidersTab: Component = () => {
  const [showAddForm, setShowAddForm] = createSignal(false);
  const detection = useAgentDetection();

  onMount(() => {
    detection.detectAll();
  });

  const providers = () => providerRegistryStore.state.registry.providers;

  return (
    <div data-testid="providers-tab">
      <div class={s.section}>
        <h3>
          Providers
          <Show when={!showAddForm()}>
            {" "}
            <button
              class={s.inlineBtn}
              data-testid="add-provider-btn"
              onClick={() => setShowAddForm(true)}
            >
              + Add
            </button>
          </Show>
        </h3>

        <Show when={showAddForm()}>
          <AddProviderForm
            onAdd={(entry) => {
              providerRegistryStore.addProvider(entry);
              setShowAddForm(false);
            }}
            onCancel={() => setShowAddForm(false)}
          />
        </Show>

        <Show
          when={providers().length > 0}
          fallback={
            <p class={s.hint}>
              No providers configured. Add one to get started.
            </p>
          }
        >
          <For each={providers()}>
            {(provider) => <ProviderCard provider={provider} />}
          </For>
        </Show>
      </div>

      <SlotAssignments detection={detection} />
    </div>
  );
};
