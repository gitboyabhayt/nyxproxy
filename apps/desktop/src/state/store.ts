import { create } from "zustand";

import {
  AiApi,
  CaApi,
  HistoryApi,
  ProxyApi,
  SettingsApi,
  listen,
} from "@/tauri/api";
import type {
  AiProvidersResponse,
  CaInfo,
  HistoryEntry,
  ProxyConfig,
  ProxyEvent,
  ProxyStatus,
  Settings,
} from "@/tauri/types";

interface ToastEntry {
  id: number;
  level: "info" | "warning" | "error";
  message: string;
  ts: number;
}

interface AppStoreState {
  ready: boolean;
  initError: string | null;
  proxy: {
    status: ProxyStatus | null;
    config: ProxyConfig | null;
  };
  history: HistoryEntry[];
  selectedFlowId: string | null;
  ca: CaInfo | null;
  settings: Settings | null;
  providers: AiProvidersResponse | null;
  toasts: ToastEntry[];
  repeaterDrafts: Record<string, RepeaterDraft>;
}

export interface RepeaterDraft {
  id: string;
  title: string;
  method: string;
  url: string;
  headers: Array<{ name: string; value: string }>;
  body: string;
  follow_redirects: boolean;
  insecure: boolean;
  lastResponse?: import("@/tauri/types").CapturedResponse | null;
  lastError?: string | null;
}

interface AppStoreActions {
  init(): Promise<void>;
  startProxy(): Promise<void>;
  stopProxy(): Promise<void>;
  refreshStatus(): Promise<void>;
  refreshHistory(): Promise<void>;
  selectFlow(id: string | null): void;
  saveProxyConfig(cfg: ProxyConfig): Promise<void>;
  saveSettings(settings: Settings): Promise<void>;
  clearHistory(): Promise<void>;
  setHistoryNote(id: string, note: string | null): Promise<void>;
  toggleStar(id: string, starred: boolean): Promise<void>;
  reloadProviders(): Promise<void>;
  reloadCa(): Promise<void>;
  toast(level: "info" | "warning" | "error", message: string): void;
  dismissToast(id: number): void;
  upsertRepeaterDraft(draft: RepeaterDraft): void;
  removeRepeaterDraft(id: string): void;
}

export type AppStore = AppStoreState & AppStoreActions;

let toastSeq = 1;

export const useAppStore = create<AppStore>((set, get) => ({
  ready: false,
  initError: null,
  proxy: { status: null, config: null },
  history: [],
  selectedFlowId: null,
  ca: null,
  settings: null,
  providers: null,
  toasts: [],
  repeaterDrafts: {},

  async init() {
    try {
      const [status, config, settings, history, ca] = await Promise.all([
        ProxyApi.status(),
        ProxyApi.getConfig(),
        SettingsApi.get(),
        HistoryApi.list(),
        CaApi.info(),
      ]);
      set({
        proxy: { status, config },
        settings,
        history,
        ca,
        ready: true,
      });
      get().reloadProviders().catch(() => undefined);

      await listen<ProxyEvent>("nyxproxy://proxy", (event) => {
        if (event.kind === "flow" && event.flow) {
          set((s) => ({ history: [{ flow: event.flow!, note: null, starred: false }, ...s.history] }));
        } else if (event.kind === "error" && event.message) {
          get().toast("error", event.message);
        } else if (event.kind === "started" || event.kind === "stopped") {
          get().refreshStatus().catch(() => undefined);
        }
      });
    } catch (err) {
      set({ initError: String(err) });
    }
  },

  async startProxy() {
    try {
      await ProxyApi.start();
      await get().refreshStatus();
      get().toast("info", "Proxy started");
    } catch (err) {
      get().toast("error", `Start failed: ${err}`);
    }
  },

  async stopProxy() {
    try {
      await ProxyApi.stop();
      await get().refreshStatus();
      get().toast("info", "Proxy stopped");
    } catch (err) {
      get().toast("error", `Stop failed: ${err}`);
    }
  },

  async refreshStatus() {
    const status = await ProxyApi.status();
    set((s) => ({ proxy: { ...s.proxy, status } }));
  },

  async refreshHistory() {
    const history = await HistoryApi.list();
    set({ history });
  },

  selectFlow(id) {
    set({ selectedFlowId: id });
  },

  async saveProxyConfig(cfg) {
    await ProxyApi.setConfig(cfg);
    set((s) => ({ proxy: { ...s.proxy, config: cfg } }));
  },

  async saveSettings(settings) {
    await SettingsApi.set(settings);
    set({ settings });
  },

  async clearHistory() {
    await HistoryApi.clear();
    set({ history: [], selectedFlowId: null });
  },

  async setHistoryNote(id, note) {
    await HistoryApi.setNote(id, note);
    set((s) => ({
      history: s.history.map((entry) =>
        entry.flow.id === id ? { ...entry, note } : entry
      ),
    }));
  },

  async toggleStar(id, starred) {
    await HistoryApi.setStarred(id, starred);
    set((s) => ({
      history: s.history.map((entry) =>
        entry.flow.id === id ? { ...entry, starred } : entry
      ),
    }));
  },

  async reloadProviders() {
    try {
      const providers = await AiApi.listProviders();
      set({ providers });
    } catch (err) {
      get().toast("warning", `Provider list unavailable: ${err}`);
    }
  },

  async reloadCa() {
    const ca = await CaApi.info();
    set({ ca });
  },

  toast(level, message) {
    const entry: ToastEntry = { id: toastSeq++, level, message, ts: Date.now() };
    set((s) => ({ toasts: [...s.toasts, entry] }));
    setTimeout(() => get().dismissToast(entry.id), 5000);
  },

  dismissToast(id) {
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
  },

  upsertRepeaterDraft(draft) {
    set((s) => ({ repeaterDrafts: { ...s.repeaterDrafts, [draft.id]: draft } }));
  },

  removeRepeaterDraft(id) {
    set((s) => {
      const next = { ...s.repeaterDrafts };
      delete next[id];
      return { repeaterDrafts: next };
    });
  },
}));
