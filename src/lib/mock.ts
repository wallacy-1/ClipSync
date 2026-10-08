// Mock para rodar a UI no navegador (npm run dev sem Tauri).
// `?platform=android` troca o shell; `?state=offline|empty|error` simula cenários; `?ime=off|enabled|selected|prompt` o teclado.
import type { Bootstrap, ClipboardItem, Device, ImeStatus, PairingInfo, Settings, SyncStatus, TransferRecord } from "./types";

const params = new URLSearchParams(location.search);
const platform = (params.get("platform") as Bootstrap["platform"]) || "windows";
const scenario = params.get("state") || "";
const imeScenario = params.get("ime") || "off";
let ime: ImeStatus = {
  supported: platform === "android",
  enabled: imeScenario !== "off" && imeScenario !== "prompt",
  selected: imeScenario === "selected",
  theme: "default",
  themes: [{ id: "default", label: "Padrão" }, { id: "clipsync", label: "ClipSync" }],
};

const now = Date.now();
const min = 60_000;
const SELF = platform === "android" ? "pixel8" : "estacao";

const settings: Settings = {
  deviceName: platform === "android" ? "Pixel 8" : "Estação-Dev",
  autoSend: true,
  autoReceive: true,
  syncedTypes: ["text", "link", "code", "email", "image"],
  fileAccept: "limit",
  fileAutoAcceptMb: 100,
  notifications: true,
  notifyText: false,
  autostartInitialized: true,
  typesRev: 1,
  lanOnly: true,
  ignorePasswordManagers: true,
  autoExpireHours: 24,
  hideSensitive: true,
  sensitiveExpireMinutes: 2,
  launchAtLogin: false,
};

let devices: Device[] = [
  {
    deviceId: "estacao", name: "Estação-Dev", platform: "windows", publicKey: "aa", state: "connected",
    isSelf: SELF === "estacao", autoSend: true, autoReceive: true, lastSeen: now, address: null, pairedAt: now - 30 * 86400000, lastError: null, queued: 0,
  },
  {
    deviceId: "pixel8", name: "Pixel 8", platform: "android", publicKey: "bb",
    state: scenario === "offline" ? "disconnected" : "connected",
    isSelf: SELF === "pixel8", autoSend: true, autoReceive: true, lastSeen: now, address: "192.168.0.14:47820", pairedAt: now - 20 * 86400000, lastError: null,
    queued: scenario === "offline" ? 4 : 0,
  },
  {
    deviceId: "notebook", name: "Notebook-Casa", platform: "windows", publicKey: "cc", state: scenario === "error" ? "blocked" : "disconnected",
    isSelf: false, autoSend: false, autoReceive: true, lastSeen: now - 2 * 86400000, address: null, pairedAt: now - 60 * 86400000,
    lastError: scenario === "error" ? "chave de pareamento expirada" : null, queued: scenario === "error" ? 2 : 0,
  },
];

const mk = (p: Partial<ClipboardItem> & { content: string; contentType: ClipboardItem["contentType"] }): ClipboardItem => ({
  id: Math.random().toString(36).slice(2),
  sourceDeviceId: "estacao",
  createdAt: now,
  expiresAt: null,
  isPinned: false,
  isSensitive: false,
  syncState: "synced",
  metadata: {},
  ...p,
});

let items: ClipboardItem[] = scenario === "empty" ? [] : [
  mk({ content: "docker compose up -d --build", contentType: "command", createdAt: now - 2 * min, isPinned: true }),
  mk({ content: "https://github.com/clipsync/core/pull/482", contentType: "link", sourceDeviceId: "pixel8", createdAt: now - 8 * min }),
  mk({ content: "const { data } = await db.from('devices').select('*')", contentType: "code", createdAt: now - 14 * min }),
  mk({ content: "sk_live_51NfQ2mB7xKpLd8vR", contentType: "token", sourceDeviceId: "notebook", createdAt: now - 21 * min, isSensitive: true, expiresAt: now + 2 * min }),
  mk({ content: "Rua Aurora 128, ap. 41 — Porto Alegre / RS", contentType: "text", sourceDeviceId: "pixel8", createdAt: now - 40 * min }),
  mk({ content: "ssh deploy@10.0.4.12 -p 2202", contentType: "command", createdAt: now - 60 * min }),
  mk({ content: "https://tauri.app/v2/guides/distribution", contentType: "link", createdAt: now - 120 * min }),
  mk({ content: "git rebase -i HEAD~4", contentType: "command", sourceDeviceId: "notebook", createdAt: now - 26 * 60 * min }),
  mk({ content: "ana.souza@example.com", contentType: "email", sourceDeviceId: "pixel8", createdAt: now - 27 * 60 * min }),
];
if (scenario === "offline") items = items.map((it, i) => (i < 4 ? { ...it, syncState: "queued" } : it));

let sync: SyncStatus = {
  paused: false,
  pausedUntil: null,
  online: scenario !== "offline",
  connectedCount: scenario === "offline" ? 0 : 1,
  queuedCount: scenario === "offline" ? 4 : scenario === "error" ? 2 : 0,
  lastError: scenario === "error" ? "Falha ao enviar 2 itens para Notebook-Casa — chave de pareamento expirada." : null,
};

// Transferências de arquivo. `?transfers=waiting|failed|history|empty` troca o cenário.
const trScenario = params.get("transfers") || "";
const MB = 1024 * 1024;
const mkT = (p: Partial<TransferRecord> & { originalName: string; size: number; deviceId: string; direction: "in" | "out"; state: TransferRecord["state"] }): TransferRecord => ({
  id: Math.random().toString(36).slice(2),
  safeName: p.originalName,
  mime: null,
  chunkSize: 256 * 1024,
  sha256Expected: null,
  sha256Actual: null,
  purpose: null,
  bytesDone: 0,
  source: p.direction === "out" ? `C:\\Users\\wallacy\\${p.originalName}` : null,
  tempPath: null,
  finalPath: p.direction === "in" && p.state.kind === "completed" ? `C:\\Users\\wallacy\\Downloads\\${p.originalName}` : null,
  attempts: 1,
  createdAt: now - 5 * min,
  updatedAt: now,
  completedAt: p.state.kind === "completed" ? now - min : null,
  ...p,
});
const T = {
  enviando: () => mkT({ originalName: "relatório-métricas-Q3.pdf", size: 12.4 * MB, bytesDone: 8.1 * MB, deviceId: "pixel8", direction: "out", state: { kind: "transferring" } }),
  grande: () => mkT({ originalName: "build-1.4.2-arm64.apk", size: 1.2 * 1024 * MB, bytesDone: 450 * MB, deviceId: "notebook", direction: "out", state: { kind: "transferring" } }),
  recebendo: () => mkT({ originalName: "captura de tela — 2026-09-20 às 14.02.png", size: 3.8 * MB, bytesDone: 3.1 * MB, deviceId: "pixel8", direction: "in", state: { kind: "transferring" } }),
  fila: () => mkT({ originalName: "dump-clipsync-prod", size: 86.3 * MB, deviceId: "notebook", direction: "out", state: { kind: "queued" } }),
  verificando: () => mkT({ originalName: "contrato-assinado.pdf", size: 2.1 * MB, bytesDone: 2.1 * MB, deviceId: "pixel8", direction: "in", state: { kind: "verifying" } }),
  concluido: () => mkT({ originalName: "apresentação-clipsync.key", size: 54.6 * MB, bytesDone: 54.6 * MB, deviceId: "pixel8", direction: "out", state: { kind: "completed" } }),
  esperando: () => mkT({ originalName: "build-1.4.2-arm64.apk", size: 1.2 * 1024 * MB, bytesDone: 612 * MB, deviceId: "notebook", direction: "out", state: { kind: "waiting", reason: "disconnected" } }),
  oferta: () => mkT({ originalName: "vídeo-demo-sync.mp4", size: 128 * MB, deviceId: "pixel8", direction: "out", state: { kind: "offered" } }),
  falhaEspaco: () => mkT({ originalName: "vídeo-demo-sync.mp4", size: 128 * MB, bytesDone: 96 * MB, deviceId: "pixel8", direction: "out", state: { kind: "failed", error: "rejected:noSpace" } }),
  falhaChecksum: () => mkT({ originalName: "dump-clipsync-prod", size: 86.3 * MB, bytesDone: 86.3 * MB, deviceId: "notebook", direction: "in", state: { kind: "failed", error: "checksum:mismatch" } }),
  cancelado: () => mkT({ originalName: "backup-fotos-2026.zip", size: 3.4 * 1024 * MB, bytesDone: 920 * MB, deviceId: "notebook", direction: "out", state: { kind: "cancelled", by: "local" } }),
};
let transfers: TransferRecord[] = ({
  "": [T.enviando(), T.grande(), T.recebendo(), T.fila(), T.verificando(), T.concluido()],
  waiting: [T.esperando(), T.recebendo(), T.oferta(), T.fila(), T.concluido()],
  failed: [T.falhaEspaco(), T.falhaChecksum(), T.grande(), T.concluido()],
  history: [T.concluido(), T.cancelado(), T.falhaChecksum()],
  empty: [],
} as Record<string, TransferRecord[]>)[trScenario] ?? [];
function setTransfer(id: string, patch: Partial<TransferRecord>) {
  transfers = transfers.map((t) => (t.id === id ? { ...t, ...patch } : t));
  const rec = transfers.find((t) => t.id === id)!;
  emit("transfer:state", rec);
  return rec;
}

const listeners = new Map<string, Set<(p: unknown) => void>>();
function emit(name: string, payload: unknown) {
  listeners.get(name)?.forEach((h) => h(payload));
}
export function mockListen(name: string, h: (p: unknown) => void) {
  if (!listeners.has(name)) listeners.set(name, new Set());
  listeners.get(name)!.add(h);
  return () => listeners.get(name)?.delete(h);
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ?ask=1 simula um arquivo grande chegando que pede "Aceitar/Recusar".
if (params.get("ask")) {
  setTimeout(() => {
    const rec = mkT({ originalName: "video-ferias-2026.mp4", size: 850 * MB, deviceId: "pixel8", direction: "in", state: { kind: "incoming" } });
    transfers = [rec, ...transfers];
    emit("transfer:ask", rec);
  }, 1200);
}

// ?copied=1 simula Ctrl+C de arquivos no Explorer (oferta "Enviar para…").
if (params.get("copied")) {
  setTimeout(() => emit("clipboard:files", {
    files: [
      { path: "C:\Users\wallacy\Desktop\contrato-assinado.pdf", name: "contrato-assinado.pdf", size: 2.4 * MB },
      { path: "C:\Users\wallacy\Desktop\fotos-obra.zip", name: "fotos-obra.zip", size: 148 * MB },
      { path: "C:\Users\wallacy\Desktop\projeto", name: "projeto", size: 0, dir: true },
    ],
    skippedFiles: 0,
    copiedAt: Date.now(),
  }), 1200);
}

function filterItems(query?: string, category?: string) {
  let out = items.slice();
  if (query?.trim()) {
    const q = query.trim().toLowerCase();
    out = out.filter((i) => i.content.toLowerCase().includes(q) || i.contentType.includes(q));
  }
  switch (category) {
    case "pinned": out = out.filter((i) => i.isPinned); break;
    case "sensitive": out = out.filter((i) => i.isSensitive); break;
    case "text": out = out.filter((i) => i.contentType === "text"); break;
    case "link": out = out.filter((i) => i.contentType === "link"); break;
    case "email": out = out.filter((i) => i.contentType === "email"); break;
    case "code": out = out.filter((i) => i.contentType === "code" || i.contentType === "command"); break;
  }
  return out.sort((a, b) => Number(b.isPinned) - Number(a.isPinned) || b.createdAt - a.createdAt);
}

export async function mockInvoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await sleep(30);
  switch (cmd) {
    case "bootstrap":
      return {
        platform,
        me: { deviceId: SELF, name: settings.deviceName, platform, publicKey: "aa" },
        settings, devices, items: filterItems(), sync, listenPort: 47820,
        totalItems: 128, pinnedItems: items.filter((i) => i.isPinned).length,
        onboarded: scenario !== "first",
        keyboardPromptSeen: scenario !== "first" && imeScenario !== "prompt",
      } satisfies Bootstrap;
    case "set_onboarded": return;
    case "set_keyboard_prompt_seen": return;
    case "ime_status": return ime;
    case "ime_open_settings":
    case "ime_show_picker":
      // No navegador não há Configurações do Android: simula o usuário ativando.
      ime = { ...ime, enabled: true, selected: cmd === "ime_show_picker" || ime.selected };
      return;
    case "ime_set_theme": ime = { ...ime, theme: args.theme as string }; return ime;
    case "list_items": {
      const list = filterItems(args.query as string, args.category as string);
      return { items: list, total: 128, pinned: items.filter((i) => i.isPinned).length };
    }
    case "latest_item": return filterItems()[0] ?? null;
    case "copy_item": return;
    case "pin_item":
      items = items.map((i) => (i.id === args.id ? { ...i, isPinned: !!args.pinned } : i));
      return;
    case "delete_item":
      items = items.filter((i) => i.id !== args.id);
      return;
    case "clear_history":
      items = args.keepPinned ? items.filter((i) => i.isPinned) : [];
      emit("clipboard:expired", { ids: [], cleared: true });
      return 0;
    case "reveal_item": return items.find((i) => i.id === args.id)?.content ?? "";
    case "send_item": {
      const id = args.itemId as string;
      items = items.map((i) => (i.id === id ? { ...i, syncState: "sending" } : i));
      emit("clipboard:sent", { itemId: id, sent: 1, queued: 0, state: "sending" });
      setTimeout(() => {
        items = items.map((i) => (i.id === id ? { ...i, syncState: "synced" } : i));
        emit("sync:completed", { itemId: id });
      }, 900);
      return;
    }
    case "retry_queue":
    case "sync_status": return sync;
    case "set_paused":
      sync = { ...sync, paused: !!args.paused, pausedUntil: args.minutes ? now + (args.minutes as number) * min : null };
      emit("sync:state", sync);
      return sync;
    case "clipboard_poll_now": return null;
    case "get_settings": return settings;
    case "set_settings": Object.assign(settings, args.settings); emit("settings:changed", settings); return settings;
    case "list_devices": return devices;
    case "set_device_flags":
      devices = devices.map((d) => d.deviceId === args.deviceId
        ? { ...d, autoSend: (args.autoSend as boolean) ?? d.autoSend, autoReceive: (args.autoReceive as boolean) ?? d.autoReceive, state: args.blocked ? "blocked" : d.state }
        : d);
      emit("device:updated", devices);
      return devices;
    case "remove_device":
      devices = devices.filter((d) => d.deviceId !== args.deviceId);
      emit("device:updated", devices);
      return devices;
    case "connect_device": return;
    case "start_pairing": {
      const info: PairingInfo = {
        pairingId: "abc123", code: "4K7-Q2M", startedAt: now, expiresAt: Date.now() + 161_000,
        qrSvg: fakeQr(), payload: JSON.stringify({ v: 1, pairingId: "abc123", deviceId: "estacao", name: "Estação-Dev", platform: "windows", publicKey: "aa", addrs: ["192.168.0.10"], port: 47820, code: "4K7-Q2M" }),
      };
      emit("pairing:started", info);
      return info;
    }
    case "cancel_pairing": return;
    case "pair_with_payload":
    case "pair_with_code": {
      await sleep(1200);
      const code = String(args.code ?? "").replace(/[^a-z0-9]/gi, "").toUpperCase();
      if (cmd === "pair_with_code" && code !== "4K7Q2M") throw "código incorreto ou expirado";
      const id = { deviceId: "estacao", name: "Estação-Dev", platform: "windows", publicKey: "aa" };
      if (!devices.some((d) => d.deviceId === id.deviceId)) {
        devices = [...devices, { ...id, state: "connected", isSelf: false, autoSend: true, autoReceive: true, lastSeen: Date.now(), address: "192.168.0.10:47820", pairedAt: Date.now(), lastError: null, queued: 0 }];
      }
      emit("pairing:completed", { deviceId: id.deviceId, name: id.name });
      emit("device:updated", devices);
      return id;
    }
    case "window_action": return;
    case "quit": return;
    case "transfer_list": return transfers;
    case "transfer_peer_supports": return args.deviceId !== "notebook";
    case "transfer_pick_file":
      await sleep(400);
      if (args.folder) return { files: [{ uri: "C:\\Users\\wallacy\\Documents\\projeto", name: "projeto", size: 0, mime: null, dir: true }] };
      return { files: [
        { uri: "C:\\Users\\wallacy\\Documents\\relatório-métricas-Q3.pdf", name: "relatório-métricas-Q3.pdf", size: 12.4 * MB, mime: "application/pdf" },
        { uri: "C:\\Users\\wallacy\\Documents\\planilha.xlsx", name: "planilha.xlsx", size: 0.8 * MB, mime: null },
      ] };
    case "transfer_remove": {
      transfers = transfers.filter((t) => t.id !== args.id || !["completed", "failed", "cancelled"].includes(t.state.kind));
      return;
    }
    case "transfer_clear_finished": {
      const before = transfers.length;
      transfers = transfers.filter((t) => !["completed", "failed", "cancelled"].includes(t.state.kind));
      return before - transfers.length;
    }
    case "transfer_send": {
      const rec = mkT({ originalName: String(args.path).split(/[\\/]/).pop() ?? "arquivo", size: 12.4 * MB, deviceId: args.deviceId as string, direction: "out", state: { kind: "queued" } });
      transfers = [rec, ...transfers];
      // Simula o ciclo: offered → transferring (progresso) → verifying → completed.
      setTimeout(() => setTransfer(rec.id, { state: { kind: "offered" } }), 600);
      setTimeout(() => setTransfer(rec.id, { state: { kind: "transferring" } }), 1400);
      let done = 0;
      const tick = setInterval(() => {
        if (transfers.find((t) => t.id === rec.id)?.state.kind === "cancelled") { clearInterval(tick); return; }
        done = Math.min(rec.size, done + rec.size / 12);
        transfers = transfers.map((t) => (t.id === rec.id ? { ...t, bytesDone: done } : t));
        emit("transfer:progress", { id: rec.id, bytesDone: done, size: rec.size });
        if (done >= rec.size) {
          clearInterval(tick);
          setTimeout(() => setTransfer(rec.id, { state: { kind: "verifying" } }), 200);
          setTimeout(() => setTransfer(rec.id, { state: { kind: "completed" }, completedAt: Date.now() }), 1400);
        }
      }, 400);
      return rec;
    }
    case "transfer_cancel": {
      await sleep(500);
      return setTransfer(args.id as string, { state: { kind: "cancelled", by: "local" } });
    }
    case "transfer_resume": {
      const cur = transfers.find((t) => t.id === args.id)!;
      return setTransfer(cur.id, { state: { kind: "queued" }, bytesDone: 0, attempts: cur.attempts + 1 });
    }
    case "transfer_open":
    case "transfer_reveal": return;
    case "transfer_asks": return transfers.filter((t) => t.direction === "in" && t.state.kind === "incoming");
    case "transfer_accept": {
      setTransfer(args.id as string, { state: { kind: "transferring" } });
      setTimeout(() => setTransfer(args.id as string, { state: { kind: "completed" }, completedAt: Date.now() }), 1500);
      return;
    }
    case "transfer_decline": return setTransfer(args.id as string, { state: { kind: "cancelled", by: "local" } });
    case "notify_permission": return params.get("notify") ?? "granted";
    case "notify_permission_request": return;
    case "share_take": {
      // ?share=files|text simula um "Compartilhar → ClipSync" (consumido uma vez).
      const kind = params.get("share");
      params.delete("share");
      if (kind === "text") return { files: [], text: "https://github.com/wallacy/clipsync/issues/12" };
      if (kind === "files") return { files: [{ uri: "content://media/1", name: "IMG_2041.jpg", size: 3.2 * MB, mime: "image/jpeg" }, { uri: "content://media/2", name: "IMG_2042.jpg", size: 2.9 * MB, mime: "image/jpeg" }], text: null };
      return null;
    }
    case "share_text": return items[0];
    case "update_status": return null;
    case "update_check":
      await sleep(600);
      return { version: "1.1.0", currentVersion: "1.0.0", notes: null };
    case "update_install": await sleep(1200); return;
    default:
      throw new Error(`mock: comando desconhecido ${cmd}`);
  }
}

export async function mockScan(): Promise<string | null> {
  await sleep(1500);
  return JSON.stringify({ v: 1, pairingId: "abc123", deviceId: "estacao", name: "Estação-Dev", platform: "windows", publicKey: "aa", addrs: ["192.168.0.10"], port: 47820, code: "4K7-Q2M" });
}

function fakeQr() {
  const bits = "1110111100100101101110101010001101011011100001011101110";
  let rects = "";
  for (let i = 0; i < 49; i++) {
    if (bits[i % bits.length] === "1") rects += `<rect x="${(i % 7) * 10}" y="${Math.floor(i / 7) * 10}" width="9" height="9" rx="1.5" fill="#201e1d"/>`;
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 69 69">${rects}</svg>`;
}

// Simula um item chegando do celular a cada 25 s no cenário padrão.
if (!scenario) {
  setInterval(() => {
    const it = mk({ content: `https://example.com/nota/${Math.floor(Math.random() * 900 + 100)}`, contentType: "link", sourceDeviceId: SELF === "pixel8" ? "estacao" : "pixel8", createdAt: Date.now() });
    items = [it, ...items];
    emit("clipboard:received", { item: it, fromDeviceId: it.sourceDeviceId, applied: true });
  }, 25_000);
}
