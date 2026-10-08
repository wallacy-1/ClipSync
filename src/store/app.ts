import { create } from "zustand";
import { useShallow } from "zustand/react/shallow";
import { isTransferActive, transferError } from "../lib/format";
import { api, errorText, listen } from "../lib/ipc";
import type {
  ClipboardItem,
  Device,
  DeviceIdentity,
  Filter,
  ImeStatus,
  PairingInfo,
  Platform,
  Settings,
  SharedPayload,
  CopiedFiles,
  LocalFile,
  NotifyPermission,
  SyncStatus,
  TransferRecord,
  UpdateInfo,
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
  /** Escolhidos (seletor ou arrastar) esperando o destino. */
  pendingFiles: LocalFile[] | null;
  /** "Compartilhar → ClipSync" aguardando destino (Android). */
  pendingShare: SharedPayload | null;
  /** Arquivos copiados no Explorer aguardando "Enviar para…" (Windows). */
  copiedFiles: CopiedFiles | null;
  /** Ids de transferências recebidas esperando "Aceitar/Recusar". */
  asks: string[];
  /** Android 13+: sem ela o progresso/Cancelar da transferência não aparece. null = não consultado. */
  notifyPerm: NotifyPermission | null;
  /** Nova versão disponível (desktop). */
  update: UpdateInfo | null;
  updateBusy: "check" | "install" | null;

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
  pickFile: (folder?: boolean) => Promise<void>;
  setPendingFiles: (f: LocalFile[] | null) => void;
  sendFiles: (deviceId: string, files: LocalFile[]) => Promise<void>;
  cancelTransfer: (id: string) => Promise<void>;
  resumeTransfer: (id: string) => Promise<void>;
  openTransfer: (id: string) => Promise<void>;
  revealTransfer: (id: string) => Promise<void>;
  removeTransfer: (id: string) => Promise<void>;
  clearFinishedTransfers: () => Promise<void>;
  checkShare: () => Promise<void>;
  sendShare: (deviceId: string) => Promise<void>;
  dismissShare: () => void;
  sendCopiedFiles: (deviceId: string) => Promise<void>;
  dismissCopiedFiles: () => void;
  acceptTransfer: (id: string) => Promise<void>;
  refreshNotifyPerm: () => Promise<void>;
  /** Pede (diálogo do sistema) ou, se negada de vez, abre as configurações. */
  requestNotifyPerm: () => Promise<void>;
  declineTransfer: (id: string) => Promise<void>;
  checkUpdate: () => Promise<void>;
  installUpdate: () => Promise<void>;
}

/** Mantém só alvos que ainda existem, não são este aparelho e não estão bloqueados. */
function pruneTargets(ids: string[], devices: Device[]): string[] {
  return ids.filter((id) => devices.some((d) => d.deviceId === id && !d.isSelf && d.state !== "blocked"));
}

let toastSeq = 0;
let initStarted = false;
let toastTimer: ReturnType<typeof setTimeout> | undefined;

/** Roda `fn`; erro vira toast. */
async function guarded<T>(fn: () => Promise<T>): Promise<T | undefined> {
  try {
    return await fn();
  } catch (e) {
    useApp.getState().showToast(errorText(e), "error");
    return undefined;
  }
}

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
  pendingFiles: null,
  pendingShare: null,
  copiedFiles: null,
  asks: [],
  notifyPerm: null,
  update: null,
  updateBusy: null,

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
      listen("share:received", () => void get().checkShare()),
      listen<CopiedFiles>("clipboard:files", (copiedFiles) => set({ copiedFiles })),
      listen<TransferRecord>("transfer:ask", (rec) => {
        set((s) => ({ transfers: [rec, ...s.transfers.filter((t) => t.id !== rec.id)], asks: s.asks.includes(rec.id) ? s.asks : [...s.asks, rec.id] }));
      }),
      listen<ClipboardItem>("clipboard:changed", (item) => {
        // Copiou outra coisa: a oferta de arquivos perdeu o sentido.
        set((s) => ({ items: upsertItem(s.items, item), totalItems: s.totalItems + 1, copiedFiles: null }));
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
        set((s) => ({ devices, sendTargets: pruneTargets(s.sendTargets, devices) }));
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
        if (reason === "too_many_attempts") {
          set({ pairing: null });
          get().showToast("Muitas tentativas com código errado. Gere um novo código.", "error");
        }
      }),
      listen<UpdateInfo>("update:available", (update) => {
        set({ update });
        get().showToast(`ClipSync ${update.version} disponível (Ajustes)`);
      }),
      listen("tray:shown", () => set({ query: "" })),
      listen<TransferRecord>("transfer:state", (rec) => {
        set((s) => {
          const busy = { ...s.transferBusy };
          delete busy[rec.id];
          const exists = s.transfers.some((t) => t.id === rec.id);
          const transfers = exists ? s.transfers.map((t) => (t.id === rec.id ? rec : t)) : [rec, ...s.transfers];
          // Saiu de `incoming` (aceito, recusado, cancelado pelo emissor): o pedido acabou.
          const asks = rec.state.kind === "incoming" ? s.asks : s.asks.filter((x) => x !== rec.id);
          return { transfers, transferBusy: busy, asks };
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

    // Pedidos "Aceitar/Recusar" que chegaram antes de a UI existir (janela aberta depois).
    try {
      const asks = await api.transferAsks();
      if (asks.length > 0) {
        set((s) => ({ transfers: [...asks, ...s.transfers.filter((t) => !asks.some((a) => a.id === t.id))], asks: asks.map((a) => a.id) }));
      }
    } catch { /* núcleo antigo sem o comando */ }

    // Atualização achada pela verificação em segundo plano antes de a janela abrir.
    if (b.platform === "windows") {
      try {
        const update = await api.updateStatus();
        if (update) set({ update });
      } catch { /* sem updater */ }
    }
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
  async send(itemId, ids) {
    if (selectPeers(get()).length === 0) {
      get().showToast("Nenhum dispositivo pareado para enviar", "error");
      return;
    }
    const deviceIds = pruneTargets(ids, get().devices);
    if (deviceIds.length === 0) {
      get().showToast("Escolha ao menos um dispositivo", "error");
      return;
    }
    await api.sendItem(itemId, deviceIds);
    const names = deviceIds.map((id) => get().devices.find((d) => d.deviceId === id)!.name);
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
      get().showToast(errorText(e), "error");
    }
  },
  async setDeviceFlags(deviceId, flags) {
    set({ devices: await api.setDeviceFlags(deviceId, flags) });
  },
  async removeDevice(deviceId) {
    const devices = await api.removeDevice(deviceId);
    set((s) => ({ devices, sendTargets: pruneTargets(s.sendTargets, devices) }));
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
      get().showToast(errorText(e), "error");
      return null;
    }
  },
  async setImeTheme(theme) {
    await guarded(async () => set({ ime: await api.imeSetTheme(theme) }));
  },
  async openImeSettings() {
    await guarded(api.imeOpenSettings);
    // O seletor/Configurações são do sistema: re-lê o estado quando o usuário voltar (e um pouco depois).
    setTimeout(() => void get().refreshIme(), 1500);
  },
  async showImePicker() {
    await guarded(api.imeShowPicker);
    setTimeout(() => void get().refreshIme(), 1500);
  },
  async refreshTransfers() {
    await guarded(async () => set({ transfers: await api.transferList(200), transfersLoaded: true }));
  },
  async pickFile(folder = false) {
    const picked = await guarded(() => api.transferPickFile(folder));
    if (!picked || picked.cancelled || picked.files.length === 0) return;
    set({ pendingFiles: picked.files.map((f) => ({ path: f.uri, name: f.name, size: f.size, dir: f.dir })) });
  },
  setPendingFiles: (pendingFiles) => set({ pendingFiles }),
  async sendFiles(deviceId, files) {
    if (get().notifyPerm === "prompt") void get().requestNotifyPerm();
    const name = get().devices.find((d) => d.deviceId === deviceId)?.name ?? deviceId;
    const recs: TransferRecord[] = [];
    const errors: string[] = [];
    // Um por vez: pasta vira zip no backend; um erro não impede os demais.
    for (const f of files) {
      try {
        const rec = await api.transferSend(deviceId, f.path);
        recs.push(rec);
        set((s) => ({ transfers: [rec, ...s.transfers.filter((t) => t.id !== rec.id)] }));
      } catch (e) {
        errors.push(`${f.name}: ${errorText(e)}`);
      }
    }
    set({ pendingFiles: null });
    if (errors.length > 0) get().showToast(errors.join(" · "), "error");
    else if (recs.length > 0) get().showToast(recs.length === 1 ? `Enviando ${recs[0].safeName} para ${name}` : `Enviando ${recs.length} arquivos para ${name}`);
  },
  async cancelTransfer(id) {
    set((s) => ({ transferBusy: { ...s.transferBusy, [id]: true } }));
    try {
      const rec = await api.transferCancel(id);
      set((s) => ({ transfers: s.transfers.map((t) => (t.id === id ? rec : t)) }));
    } catch (e) {
      get().showToast(errorText(e), "error");
    } finally {
      set((s) => { const b = { ...s.transferBusy }; delete b[id]; return { transferBusy: b }; });
    }
  },
  async resumeTransfer(id) {
    const rec = await guarded(() => api.transferResume(id));
    if (rec) set((s) => ({ transfers: [rec, ...s.transfers.filter((t) => t.id !== rec.id && t.id !== id)] }));
  },
  async openTransfer(id) {
    await guarded(() => api.transferOpen(id));
  },
  async revealTransfer(id) {
    await guarded(() => api.transferReveal(id));
  },
  async removeTransfer(id) {
    await guarded(async () => {
      await api.transferRemove(id);
      set((s) => ({ transfers: s.transfers.filter((t) => t.id !== id) }));
    });
  },
  async clearFinishedTransfers() {
    await guarded(async () => {
      await api.transferClearFinished();
      set((s) => ({ transfers: s.transfers.filter(isTransferActive) }));
    });
  },

  async checkShare() {
    const share = await guarded(api.shareTake);
    if (share && (share.files.length > 0 || share.text)) set({ pendingShare: share });
  },
  async sendShare(deviceId) {
    const share = get().pendingShare;
    if (!share) return;
    set({ pendingShare: null });
    const name = get().devices.find((d) => d.deviceId === deviceId)?.name ?? deviceId;
    if (share.files.length > 0 && get().notifyPerm === "prompt") void get().requestNotifyPerm();
    try {
      if (share.files.length > 0) {
        const recs: TransferRecord[] = [];
        for (const f of share.files) recs.push(await api.transferSend(deviceId, f.uri));
        set((s) => ({ transfers: [...recs.reverse(), ...s.transfers.filter((t) => !recs.some((r) => r.id === t.id))] }));
        get().showToast(recs.length === 1 ? `Enviando ${recs[0].safeName} para ${name}` : `Enviando ${recs.length} arquivos para ${name}`);
      } else if (share.text) {
        await api.shareText(share.text, [deviceId]);
        get().showToast(`Texto enviado para ${name}`);
      }
    } catch (e) {
      get().showToast(errorText(e), "error");
    }
  },
  dismissShare: () => set({ pendingShare: null }),
  async sendCopiedFiles(deviceId) {
    const offer = get().copiedFiles;
    if (!offer) return;
    set({ copiedFiles: null });
    await get().sendFiles(deviceId, offer.files);
  },
  dismissCopiedFiles: () => set({ copiedFiles: null }),
  async refreshNotifyPerm() {
    try { set({ notifyPerm: await api.notifyPermission() }); } catch { set({ notifyPerm: "unsupported" }); }
  },
  async requestNotifyPerm() {
    const cur = get().notifyPerm;
    if (cur !== "prompt" && cur !== "blocked") return;
    await guarded(() => api.notifyPermissionRequest(cur === "blocked"));
    // O diálogo/Configurações são do sistema: relê ao voltar (visibilitychange) e um pouco depois.
    setTimeout(() => void get().refreshNotifyPerm(), 1500);
  },
  async acceptTransfer(id) {
    set((s) => ({ asks: s.asks.filter((x) => x !== id) }));
    await guarded(() => api.transferAccept(id));
  },
  async declineTransfer(id) {
    set((s) => ({ asks: s.asks.filter((x) => x !== id) }));
    const rec = await guarded(() => api.transferDecline(id));
    if (rec) set((s) => ({ transfers: s.transfers.map((t) => (t.id === id ? rec : t)) }));
  },

  async checkUpdate() {
    set({ updateBusy: "check" });
    const res = await guarded(() => api.updateCheck());
    set({ updateBusy: null });
    if (res === undefined) return;
    set({ update: res });
    if (!res) get().showToast("Você já tem a versão mais recente");
  },

  async installUpdate() {
    set({ updateBusy: "install" });
    // Sucesso reinicia o app; só volta aqui em erro.
    await guarded(() => api.updateInstall());
    set({ updateBusy: null });
  },

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
