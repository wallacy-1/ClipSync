import { create } from "zustand";
import { useShallow } from "zustand/react/shallow";
import { isTransferActive, transferError } from "../lib/format";
import { api, listen } from "../lib/ipc";
import type {
  ClipboardItem,
  Device,
  DeviceIdentity,
  Filter,
  ImeStatus,
  PairingInfo,
  Platform,
  Settings,
  SyncStatus,
  TransferRecord,
} from "../lib/types";

export type View = "history" | "pinned" | "devices" | "transfers" | "settings";

interface Toast {
  id: number;
  text: string;
  kind?: "error";
}

interface AppStore {
  ready: boolean;
  platform: Platform;
  me: DeviceIdentity | null;
  settings: Settings | null;
  devices: Device[];
  items: ClipboardItem[];
  totalItems: number;
  pinnedItems: number;
  sync: SyncStatus;
  onboarded: boolean;
  keyboardPromptSeen: boolean;
  /** ClipSync Keyboard (IME opcional). null = ainda não consultado. */
  ime: ImeStatus | null;
  now: number;

  view: View;
  query: string;
  filter: Filter;
  selectedId: string | null;
  revealed: Record<string, string>;
  sendTargets: string[];
  pairing: PairingInfo | null;
  toast: Toast | null;
  /** Transferências de arquivo, mais recente primeiro. Carregadas sob demanda. */
  transfers: TransferRecord[];
  transfersLoaded: boolean;
  /** Linhas ocupadas ("Cancelando…") até o evento de estado chegar. */
  transferBusy: Record<string, true>;
  /** Arquivo escolhido aguardando destino (diálogo/folha "escolher destino"). */
  pendingFile: { path: string; name: string; size: number } | null;

  init: () => Promise<void>;
  setView: (v: View) => void;
  setQuery: (q: string) => void;
  setFilter: (f: Filter) => void;
  select: (id: string | null) => void;
  toggleTarget: (id: string) => void;
  refreshItems: () => Promise<void>;
  refreshDevices: () => Promise<void>;
  copy: (id: string) => Promise<void>;
  pin: (id: string, pinned: boolean) => Promise<void>;
  remove: (id: string) => Promise<void>;
  reveal: (id: string) => Promise<string>;
  hide: (id: string) => void;
  send: (itemId: string, deviceIds: string[]) => Promise<void>;
  setPaused: (paused: boolean, minutes?: number) => Promise<void>;
  retry: () => Promise<void>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
  setDeviceFlags: (deviceId: string, flags: { autoSend?: boolean; autoReceive?: boolean; blocked?: boolean }) => Promise<void>;
  removeDevice: (deviceId: string) => Promise<void>;
  startPairing: () => Promise<void>;
  cancelPairing: () => Promise<void>;
  clearHistory: (keepPinned: boolean) => Promise<void>;
  finishOnboarding: () => Promise<void>;
  dismissKeyboardPrompt: () => Promise<void>;
  refreshIme: () => Promise<ImeStatus | null>;
  setImeTheme: (theme: string) => Promise<void>;
  openImeSettings: () => Promise<void>;
  showImePicker: () => Promise<void>;
  showToast: (text: string, kind?: "error") => void;

  refreshTransfers: () => Promise<void>;
  pickFile: () => Promise<void>;
  setPendingFile: (f: { path: string; name: string; size: number } | null) => void;
  sendFile: (deviceId: string, path: string) => Promise<void>;
  cancelTransfer: (id: string) => Promise<void>;
  resumeTransfer: (id: string) => Promise<void>;
  openTransfer: (id: string) => Promise<void>;
  revealTransfer: (id: string) => Promise<void>;
  clearFinishedTransfers: () => void;
}

let toastSeq = 0;
let initStarted = false;
let toastTimer: ReturnType<typeof setTimeout> | undefined;

function upsertItem(list: ClipboardItem[], it: ClipboardItem) {
  const rest = list.filter((x) => x.id !== it.id && x.content !== it.content);
  return [it, ...rest];
}

export const useApp = create<AppStore>((set, get) => ({
  ready: false,
  platform: "windows",
  me: null,
  settings: null,
  devices: [],
  items: [],
  totalItems: 0,
  pinnedItems: 0,
  sync: { paused: false, pausedUntil: null, online: false, connectedCount: 0, queuedCount: 0, lastError: null },
  onboarded: true,
  keyboardPromptSeen: true,
  ime: null,
  now: Date.now(),

  view: "history",
  query: "",
  filter: "all",
  selectedId: null,
  revealed: {},
  sendTargets: [],
  pairing: null,
  toast: null,
  transfers: [],
  transfersLoaded: false,
  transferBusy: {},
  pendingFile: null,

  async init() {
    // StrictMode monta duas vezes em dev: registrar listeners só uma vez.
    if (initStarted) return;
    initStarted = true;
    const b = await api.bootstrap();
    const connected = b.devices.filter((d) => !d.isSelf && d.state === "connected").map((d) => d.deviceId);
    set({
      ready: true,
      platform: b.platform,
      me: b.me,
      settings: b.settings,
      devices: b.devices,
      items: b.items,
      totalItems: b.totalItems,
      pinnedItems: b.pinnedItems,
      sync: b.sync,
      onboarded: b.onboarded,
      keyboardPromptSeen: b.keyboardPromptSeen,
      selectedId: b.items[0]?.id ?? null,
      sendTargets: connected,
    });

    setInterval(() => set({ now: Date.now() }), 1000);

    await Promise.all([
      listen<ClipboardItem>("clipboard:changed", (item) => {
        set((s) => ({ items: upsertItem(s.items, item), totalItems: s.totalItems + 1 }));
      }),
      listen<{ item: ClipboardItem; fromDeviceId: string; applied: boolean }>("clipboard:received", ({ item, fromDeviceId }) => {
        set((s) => ({ items: upsertItem(s.items, item), totalItems: s.totalItems + 1 }));
        const from = get().devices.find((d) => d.deviceId === fromDeviceId)?.name ?? "outro dispositivo";
        get().showToast(`Recebido de ${from}`);
      }),
      listen<{ itemId: string; state: ClipboardItem["syncState"] }>("clipboard:sent", ({ itemId, state }) => {
        set((s) => ({ items: s.items.map((i) => (i.id === itemId ? { ...i, syncState: state } : i)) }));
      }),
      listen<{ itemId: string }>("sync:completed", ({ itemId }) => {
        set((s) => ({ items: s.items.map((i) => (i.id === itemId ? { ...i, syncState: "synced" } : i)) }));
      }),
      listen<{ ids: string[]; cleared?: boolean }>("clipboard:expired", ({ ids, cleared }) => {
        if (cleared) void get().refreshItems();
        else set((s) => ({ items: s.items.filter((i) => !ids.includes(i.id)), totalItems: Math.max(0, s.totalItems - ids.length) }));
      }),
      listen<Device[]>("device:updated", (devices) => {
        set({ devices });
        void api.syncStatus().then((sync) => set({ sync }));
      }),
      listen<SyncStatus>("sync:state", (sync) => set({ sync })),
      listen<Settings>("settings:changed", (settings) => set({ settings })),
      listen<{ deviceId: string; name: string }>("pairing:completed", ({ name }) => {
        set({ pairing: null });
        get().showToast(`${name} pareado`);
        void get().refreshDevices();
      }),
      listen<{ reason: string }>("pairing:failed", ({ reason }) => {
        if (reason === "expired") set({ pairing: null });
      }),
      listen("tray:shown", () => set({ query: "" })),
      listen<TransferRecord>("transfer:state", (rec) => {
        set((s) => {
          const busy = { ...s.transferBusy };
          delete busy[rec.id];
          const exists = s.transfers.some((t) => t.id === rec.id);
          const transfers = exists ? s.transfers.map((t) => (t.id === rec.id ? rec : t)) : [rec, ...s.transfers];
          return { transfers, transferBusy: busy };
        });
        if (rec.state.kind === "completed" && rec.direction === "in") get().showToast(`Recebido: ${rec.safeName}`);
        if (rec.state.kind === "failed") {
          const dev = get().devices.find((d) => d.deviceId === rec.deviceId)?.name ?? "dispositivo";
          get().showToast(`${rec.safeName}: ${transferError(rec.state.error, dev)}`, "error");
        }
      }),
      listen<{ id: string; bytesDone: number; size: number }>("transfer:progress", ({ id, bytesDone, size }) => {
        set((s) => ({ transfers: s.transfers.map((t) => (t.id === id ? { ...t, bytesDone, size } : t)) }));
      }),
    ]);
  },

  setView: (view) => set({ view, filter: view === "pinned" ? "pinned" : get().filter === "pinned" ? "all" : get().filter }),
  setQuery: (query) => set({ query }),
  setFilter: (filter) => set({ filter }),
  select: (selectedId) => set({ selectedId }),
  toggleTarget: (id) =>
    set((s) => ({ sendTargets: s.sendTargets.includes(id) ? s.sendTargets.filter((x) => x !== id) : [...s.sendTargets, id] })),

  async refreshItems() {
    const page = await api.listItems(undefined, undefined, 200);
    set({ items: page.items, totalItems: page.total, pinnedItems: page.pinned });
  },
  async refreshDevices() {
    set({ devices: await api.listDevices() });
  },

  async copy(id) {
    await api.copyItem(id);
    get().showToast("Copiado");
  },
  async pin(id, pinned) {
    await api.pinItem(id, pinned);
    set((s) => ({
      items: s.items.map((i) => (i.id === id ? { ...i, isPinned: pinned, expiresAt: pinned ? null : i.expiresAt } : i)),
      pinnedItems: s.pinnedItems + (pinned ? 1 : -1),
    }));
  },
  async remove(id) {
    await api.deleteItem(id);
    set((s) => {
      const items = s.items.filter((i) => i.id !== id);
      return { items, totalItems: Math.max(0, s.totalItems - 1), selectedId: s.selectedId === id ? items[0]?.id ?? null : s.selectedId };
    });
  },
  async reveal(id) {
    const content = await api.revealItem(id);
    set((s) => ({ revealed: { ...s.revealed, [id]: content } }));
    return content;
  },
  hide(id) {
    set((s) => {
      const r = { ...s.revealed };
      delete r[id];
      return { revealed: r };
    });
  },
  async send(itemId, deviceIds) {
    if (deviceIds.length === 0) {
      get().showToast("Escolha ao menos um dispositivo", "error");
      return;
    }
    await api.sendItem(itemId, deviceIds);
    const names = deviceIds.map((id) => get().devices.find((d) => d.deviceId === id)?.name ?? id);
    get().showToast(`Enviando para ${names.join(", ")}`);
  },
  async setPaused(paused, minutes) {
    set({ sync: await api.setPaused(paused, minutes) });
  },
  async retry() {
    set({ sync: await api.retryQueue() });
    get().showToast("Tentando reconectar…");
  },
  async saveSettings(patch) {
    const cur = get().settings;
    if (!cur) return;
    const next = { ...cur, ...patch };
    set({ settings: next });
    try {
      set({ settings: await api.setSettings(next) });
    } catch (e) {
      set({ settings: cur });
      get().showToast(String(e), "error");
    }
  },
  async setDeviceFlags(deviceId, flags) {
    set({ devices: await api.setDeviceFlags(deviceId, flags) });
  },
  async removeDevice(deviceId) {
    set({ devices: await api.removeDevice(deviceId) });
    get().showToast("Dispositivo removido e chaves revogadas");
  },
  async startPairing() {
    set({ pairing: await api.startPairing() });
  },
  async cancelPairing() {
    await api.cancelPairing();
    set({ pairing: null });
  },
  async clearHistory(keepPinned) {
    await api.clearHistory(keepPinned);
    await get().refreshItems();
    get().showToast("Histórico limpo");
  },
  async finishOnboarding() {
    await api.setOnboarded();
    set({ onboarded: true });
  },
  async dismissKeyboardPrompt() {
    set({ keyboardPromptSeen: true });
    await api.setKeyboardPromptSeen();
  },
  async refreshIme() {
    if (get().platform !== "android") return null;
    try {
      const ime = await api.imeStatus();
      set({ ime });
      return ime;
    } catch (e) {
      get().showToast(String(e), "error");
      return null;
    }
  },
  async setImeTheme(theme) {
    try {
      set({ ime: await api.imeSetTheme(theme) });
    } catch (e) {
      get().showToast(String(e), "error");
    }
  },
  async openImeSettings() {
    try { await api.imeOpenSettings(); } catch (e) { get().showToast(String(e), "error"); }
    // O seletor/Configurações são do sistema: re-lê o estado quando o usuário voltar (e um pouco depois).
    setTimeout(() => void get().refreshIme(), 1500);
  },
  async showImePicker() {
    try { await api.imeShowPicker(); } catch (e) { get().showToast(String(e), "error"); }
    setTimeout(() => void get().refreshIme(), 1500);
  },
  async refreshTransfers() {
    try {
      set({ transfers: await api.transferList(200), transfersLoaded: true });
    } catch (e) {
      get().showToast(String(e), "error");
    }
  },
  async pickFile() {
    try {
      const picked = await api.transferPickFile();
      if (picked.cancelled) return;
      set({ pendingFile: { path: picked.uri, name: picked.name, size: picked.size } });
    } catch (e) {
      get().showToast(String(e), "error");
    }
  },
  setPendingFile: (pendingFile) => set({ pendingFile }),
  async sendFile(deviceId, path) {
    try {
      const rec = await api.transferSend(deviceId, path);
      set((s) => ({ transfers: [rec, ...s.transfers.filter((t) => t.id !== rec.id)], pendingFile: null }));
      const name = get().devices.find((d) => d.deviceId === deviceId)?.name ?? deviceId;
      get().showToast(`Enviando ${rec.safeName} para ${name}`);
    } catch (e) {
      get().showToast(String(e), "error");
    }
  },
  async cancelTransfer(id) {
    set((s) => ({ transferBusy: { ...s.transferBusy, [id]: true } }));
    try {
      const rec = await api.transferCancel(id);
      set((s) => ({ transfers: s.transfers.map((t) => (t.id === id ? rec : t)) }));
    } catch (e) {
      get().showToast(String(e), "error");
    } finally {
      set((s) => { const b = { ...s.transferBusy }; delete b[id]; return { transferBusy: b }; });
    }
  },
  async resumeTransfer(id) {
    try {
      const rec = await api.transferResume(id);
      set((s) => ({ transfers: [rec, ...s.transfers.filter((t) => t.id !== rec.id && t.id !== id)] }));
    } catch (e) {
      get().showToast(String(e), "error");
    }
  },
  async openTransfer(id) {
    try { await api.transferOpen(id); } catch (e) { get().showToast(String(e), "error"); }
  },
  async revealTransfer(id) {
    try { await api.transferReveal(id); } catch (e) { get().showToast(String(e), "error"); }
  },
  // Só esconde na sessão: o backend mantém 30 dias de retenção.
  clearFinishedTransfers: () => set((s) => ({ transfers: s.transfers.filter(isTransferActive) })),

  showToast(text, kind) {
    const id = ++toastSeq;
    set({ toast: { id, text, kind } });
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => set((s) => (s.toast?.id === id ? { toast: null } : {})), 2600);
  },
}));

export function selectVisibleItems(s: AppStore): ClipboardItem[] {
  const q = s.query.trim().toLowerCase();
  const f = s.view === "pinned" ? "pinned" : s.filter;
  return s.items.filter((it) => {
    if (q && !(it.content.toLowerCase().includes(q) || it.contentType.includes(q))) return false;
    switch (f) {
      case "pinned": return it.isPinned;
      case "sensitive": return it.isSensitive;
      case "text": return it.contentType === "text";
      case "link": return it.contentType === "link";
      case "email": return it.contentType === "email";
      case "code": return it.contentType === "code" || it.contentType === "command";
      default: return true;
    }
  });
}

export function selectPeers(s: AppStore): Device[] {
  return s.devices.filter((d) => !d.isSelf && d.pairedAt !== null);
}

export function selectDiscovered(s: AppStore): Device[] {
  return s.devices.filter((d) => !d.isSelf && d.pairedAt === null);
}

// Selectors que devolvem arrays novos precisam de comparação rasa.
export const usePeers = () => useApp(useShallow(selectPeers));
export const useDiscovered = () => useApp(useShallow(selectDiscovered));
export const useVisibleItems = () => useApp(useShallow(selectVisibleItems));
