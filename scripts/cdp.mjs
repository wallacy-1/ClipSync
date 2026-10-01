// Ferramenta de teste: executa JS dentro da WebView2 de uma instância do ClipSync
// via Chrome DevTools Protocol (instâncias lançadas com --remote-debugging-port=9222;
// o WebView2 compartilha um processo, então todas as janelas aparecem na mesma porta).
// Uso: node scripts/cdp.mjs <perfil> "<expressão JS>"
// Ex.:  node scripts/cdp.mjs a "invoke('start_pairing')"
const [profile, expr] = process.argv.slice(2);
const PORT = process.env.CDP_PORT ?? ({ a: "9222", b: "9223", c: "9224", android: "9225" }[profile] ?? "9222");
if (!profile || !expr) {
  console.error("uso: node scripts/cdp.mjs <perfil> <js>");
  process.exit(2);
}

async function evalOn(page, expression) {
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }));
  const msg = await new Promise((r) => { ws.onmessage = (e) => r(JSON.parse(e.data)); });
  ws.close();
  return msg.result;
}

const targets = (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).filter((t) => t.type === "page" && !t.url.includes("#/tray"));
let page = null;
// Android (adb forward para localabstract:webview_devtools_remote_<pid>): uma única página.
if (profile === "android") page = targets[0] ?? null;
for (const t of page ? [] : targets) {
  const r = await evalOn(t, `document.querySelector('.titlebar .subtitle')?.textContent ?? ''`);
  if ((r?.result?.value ?? "").endsWith(`-${profile}`)) { page = t; break; }
}
// Única página sem subtítulo (ex.: release apontando para um devUrl fora do ar): usa-a mesmo assim.
if (!page && targets.length === 1) page = targets[0];
if (!page) { console.error(`perfil ${profile} não encontrado entre ${targets.length} páginas`); process.exit(1); }

const wrapped = `(async () => { const invoke = (c, a) => window.__TAURI_INTERNALS__.invoke(c, a ?? {}); return JSON.stringify(await (${expr})); })()`;
const res = await evalOn(page, wrapped);
if (res?.exceptionDetails) {
  console.error("ERRO:", res.exceptionDetails.exception?.description ?? JSON.stringify(res.exceptionDetails));
  process.exit(1);
}
console.log(res?.result?.value ?? JSON.stringify(res));
