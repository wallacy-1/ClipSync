// Fora do Tauri (npm run dev no navegador) cai num mock em memória.
import type {
  Bootstrap,
  ClipboardItem,
  Device,
  DeviceIdentity,
  ImeStatus,
  ItemsPage,
  PairingInfo,
  PickedFiles,
  SharedPayload,
  NotifyPermission,
  Settings,
  SyncStatus,
  TransferRecord,
  UpdateInfo,
} from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Unlisten = () => void;
type Handler<T = unknown> = (payload: T) => void;

let tauriInvoke: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
let tauriListen: (<T>(name: string, h: (e: { payload: T }) => void) => Promise<Unlisten>) | null = null;

async function ensureTauri() {
  if (!inTauri || tauriInvoke) return;
  const core = await import("@tauri-apps/api/core");
  const event = await import("@tauri-apps/api/event");
  tauriInvoke = core.invoke;
  tauriListen = event.listen;
}

/** Texto legível de um erro: comandos rejeitam com string; plugins, com `{ message }`. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e && typeof e.message === "string") return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

/** Versão do app (`tauri.conf.json`); no navegador com mock, "dev". */
export async function appVersion(): Promise<string> {
  if (!inTauri) return "dev";
  const { getVersion } = await import("@tauri-apps/api/app");
  return getVersion();
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) {
    await ensureTauri();
    return tauriInvoke!(cmd, args) as Promise<T>;
  }
  const { mockInvoke } = await import("./mock");
  return mockInvoke(cmd, args) as Promise<T>;
}

export async function listen<T = unknown>(name: string, handler: Handler<T>): Promise<Unlisten> {
  if (inTauri) {
    await ensureTauri();
    return tauriListen!<T>(name, (e) => handler(e.payload));
  }
  const { mockListen } = await import("./mock");
  return mockListen(name, handler as Handler);
}

export const api = {
  bootstrap: () => call<Bootstrap>("bootstrap"),
  setOnboarded: () => call<void>("set_onboarded"),
  setKeyboardPromptSeen: () => call<void>("set_keyboard_prompt_seen"),
  listItems: (query?: string, category?: string, limit?: number) =>
    call<ItemsPage>("list_items", { query, category, limit }),
  latestItem: () => call<ClipboardItem | null>("latest_item"),
  copyItem: (id: string) => call<void>("copy_item", { id }),
  pinItem: (id: string, pinned: boolean) => call<void>("pin_item", { id, pinned }),
  deleteItem: (id: string) => call<void>("delete_item", { id }),
  clearHistory: (keepPinned: boolean) => call<number>("clear_history", { keepPinned }),
  revealItem: (id: string) => call<string>("reveal_item", { id }),
  sendItem: (itemId: string, deviceIds: string[]) => call<void>("send_item", { itemId, deviceIds }),
  retryQueue: () => call<SyncStatus>("retry_queue"),
  syncStatus: () => call<SyncStatus>("sync_status"),
  setPaused: (paused: boolean, minutes?: number) => call<SyncStatus>("set_paused", { paused, minutes }),
  clipboardPollNow: () => call<ClipboardItem | null>("clipboard_poll_now"),
  getSettings: () => call<Settings>("get_settings"),
  setSettings: (settings: Settings) => call<Settings>("set_settings", { settings }),
  shortcutFailures: () => call<string[]>("shortcut_failures"),
  shortcutsSuspend: (suspend: boolean) => call<void>("shortcuts_suspend", { suspend }),
  listDevices: () => call<Device[]>("list_devices"),
  setDeviceFlags: (deviceId: string, flags: { autoSend?: boolean; autoReceive?: boolean; blocked?: boolean }) =>
    call<Device[]>("set_device_flags", { deviceId, ...flags }),
  removeDevice: (deviceId: string) => call<Device[]>("remove_device", { deviceId }),
  connectDevice: (deviceId: string) => call<void>("connect_device", { deviceId }),
  startPairing: () => call<PairingInfo>("start_pairing"),
  cancelPairing: () => call<void>("cancel_pairing"),
  pairWithPayload: (raw: string) => call<DeviceIdentity>("pair_with_payload", { raw }),
  pairWithCode: (code: string) => call<DeviceIdentity>("pair_with_code", { code }),
  windowAction: (action: string) => call<void>("window_action", { action }),
  quit: () => call<void>("quit"),
  imeStatus: () => call<ImeStatus>("ime_status"),
  imeOpenSettings: () => call<void>("ime_open_settings"),
  imeShowPicker: () => call<void>("ime_show_picker"),
  imeSetTheme: (theme: string) => call<ImeStatus>("ime_set_theme", { theme }),
  transferList: (limit?: number) => call<TransferRecord[]>("transfer_list", { limit }),
  transferSend: (deviceId: string, path: string) => call<TransferRecord>("transfer_send", { deviceId, path }),
  transferCancel: (id: string) => call<TransferRecord>("transfer_cancel", { id }),
  transferResume: (id: string) => call<TransferRecord>("transfer_resume", { id }),
  transferAsks: () => call<TransferRecord[]>("transfer_asks"),
  transferAccept: (id: string) => call<void>("transfer_accept", { id }),
  transferDecline: (id: string) => call<TransferRecord>("transfer_decline", { id }),
  transferPickFile: (folder = false) => call<PickedFiles>("transfer_pick_file", { folder }),
  transferClearFinished: () => call<number>("transfer_clear_finished"),
  transferRemove: (id: string) => call<void>("transfer_remove", { id }),
  transferPeerSupports: (deviceId: string) => call<boolean>("transfer_peer_supports", { deviceId }),
  transferOpen: (id: string) => call<void>("transfer_open", { id }),
  transferReveal: (id: string) => call<void>("transfer_reveal", { id }),
  notifyPermission: () => call<NotifyPermission>("notify_permission"),
  notifyPermissionRequest: (openSettings: boolean) => call<void>("notify_permission_request", { openSettings }),
  shareTake: () => call<SharedPayload | null>("share_take"),
  shareText: (text: string, deviceIds: string[]) => call<ClipboardItem>("share_text", { text, deviceIds }),
  updateStatus: () => call<UpdateInfo | null>("update_status"),
  updateCheck: () => call<UpdateInfo | null>("update_check"),
  updateInstall: () => call<void>("update_install"),
};

/** Abre a câmera e devolve o conteúdo do QR (só Android/iOS). */
export async function scanQr(): Promise<string | null> {
  if (!inTauri) {
    const { mockScan } = await import("./mock");
    return mockScan();
  }
  const scanner = await import("@tauri-apps/plugin-barcode-scanner");
  let perm = await scanner.checkPermissions();
  if (perm !== "granted") perm = await scanner.requestPermissions();
  if (perm !== "granted") throw new Error("Permissão de câmera negada");
  const res = await scanner.scan({ windowed: true, formats: [scanner.Format.QRCode] });
  return res?.content ?? null;
}

export async function cancelScan() {
  if (!inTauri) return;
  const scanner = await import("@tauri-apps/plugin-barcode-scanner");
  await scanner.cancel();
}

export async function startDragging() {
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().startDragging();
}

/** Arquivos soltos sobre a janela (só desktop). Devolve o cancelamento do listener. */
export async function onFileDrop(handler: (paths: string[]) => void): Promise<Unlisten> {
  if (!inTauri) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === "drop" && e.payload.paths.length > 0) handler(e.payload.paths);
  });
}
