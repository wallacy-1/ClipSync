// Relay TCP de teste: grava todos os bytes que passam entre dois nós ClipSync
// para provar que nada trafega em texto puro. Com REPLAY=1, reinjeta no destino
// um quadro já enviado (ataque de replay) após N quadros.
// Uso: node scripts/relay.mjs <portaLocal> <hostDestino> <portaDestino> <arquivoLog>
import net from "node:net";
import fs from "node:fs";

const [listenPort, host, port, logFile] = process.argv.slice(2);
const REPLAY = process.env.REPLAY === "1";
const log = fs.createWriteStream(logFile, { flags: "a" });
const raw = fs.createWriteStream(logFile + ".raw", { flags: "a" });

function dump(dir, buf) {
  const ascii = buf.toString("latin1").replace(/[^\x20-\x7e]/g, ".");
  log.write(`[${new Date().toISOString()}] ${dir} ${buf.length}B\n  ascii: ${ascii.slice(0, 200)}\n  hex:   ${buf.subarray(0, 64).toString("hex")}\n`);
  raw.write(buf);
}

/** Separa o fluxo em quadros [u32 BE len][bytes]. */
function framer(onFrame) {
  let acc = Buffer.alloc(0);
  return (chunk) => {
    acc = Buffer.concat([acc, chunk]);
    while (acc.length >= 4) {
      const len = acc.readUInt32BE(0);
      if (acc.length < 4 + len) break;
      onFrame(acc.subarray(0, 4 + len));
      acc = acc.subarray(4 + len);
    }
  };
}

net.createServer((client) => {
  const up = net.connect(Number(port), host);
  let frames = 0;
  let savedFrame = null;
  let savedAt = 0;
  let replayed = false;
  const c2u = framer((f) => {
    frames++;
    dump("C->S", f);
    // guarda o primeiro quadro grande pós-handshake (um Clip cifrado) e o reinjeta 2 quadros depois
    if (REPLAY && !savedFrame && frames > 5 && f.length > 150) { savedFrame = Buffer.from(f); savedAt = frames; }
    up.write(f);
    if (REPLAY && savedFrame && frames >= savedAt + 2 && !replayed) {
      replayed = true;
      log.write(`[${new Date().toISOString()}] *** REPLAY de quadro C->S (${savedFrame.length}B) ***\n`);
      up.write(savedFrame);
    }
  });
  const u2c = framer((f) => { dump("S->C", f); client.write(f); });
  client.on("data", c2u);
  up.on("data", u2c);
  client.on("close", () => up.destroy());
  up.on("close", () => client.destroy());
  client.on("error", () => up.destroy());
  up.on("error", () => client.destroy());
}).listen(Number(listenPort), "0.0.0.0", () => console.log(`relay :${listenPort} -> ${host}:${port} (replay=${REPLAY})`));
