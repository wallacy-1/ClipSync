import { useEffect, useRef, useState } from "react";
import { PLATFORM_LABEL, mmss } from "../lib/format";
import { api, errorText } from "../lib/ipc";
import { useApp } from "../store/app";

/** Pedidos de pareamento pela rede: o que chegou de outro aparelho e o que este enviou. */
export function PairPrompts() {
  return (
    <>
      <IncomingRequest />
      <OutgoingRequest />
    </>
  );
}

/** Outro dispositivo pediu: aceitar abre o pareamento e mostra o código para digitar lá. */
function IncomingRequest() {
  const req = useApp((s) => s.pairRequest);
  const pairing = useApp((s) => s.pairing);
  const now = useApp((s) => s.now);
  const { startPairing, cancelPairing, dismissPairRequest, showToast } = useApp.getState();
  const [accepted, setAccepted] = useState(false);
  const shown = useRef(false);

  useEffect(() => { setAccepted(false); shown.current = false; }, [req?.deviceId]);
  // Código já exibido sumiu: expirou ou a sessão acabou por tentativas erradas.
  useEffect(() => {
    if (!accepted) return;
    if (pairing) shown.current = true;
    else if (shown.current) dismissPairRequest();
  }, [accepted, pairing, dismissPairRequest]);

  if (!req) return null;
  const where = PLATFORM_LABEL[req.platform] ?? req.platform;

  if (accepted && pairing) {
    return (
      <div className="dialog-backdrop">
        <div className="dialog">
          <div className="dialog-title">Digite este código em {req.name}</div>
          <div className="dialog-body">O pareamento termina sozinho quando o código for digitado lá.</div>
          <div className="row-flex" style={{ gap: 12 }}>
            <span className="pair-code">{pairing.code}</span>
            <span className="pair-expires">expira em {mmss(pairing.expiresAt - now)}</span>
          </div>
          <div className="dialog-actions">
            <button className="btn btn-secondary" onClick={() => { void cancelPairing(); dismissPairRequest(); }}>Cancelar</button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="dialog-backdrop">
      <div className="dialog">
        <div className="dialog-title">{req.name} quer parear</div>
        <div className="dialog-body">
          Um {where} na sua rede pediu para sincronizar com este dispositivo. Aceite só se foi você quem pediu: um código vai aparecer aqui para você digitar lá.
        </div>
        <div className="dialog-actions">
          <button className="btn btn-secondary" onClick={dismissPairRequest}>Recusar</button>
          <button
            className="btn btn-primary"
            disabled={accepted}
            onClick={async () => {
              setAccepted(true);
              try {
                await startPairing();
              } catch (e) {
                showToast(errorText(e), "error");
                dismissPairRequest();
              }
            }}
          >
            {accepted ? "Abrindo…" : "Aceitar"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Este dispositivo pediu: espera o código que apareceu no outro. */
function OutgoingRequest() {
  const out = useApp((s) => s.pairOutgoing);
  const { dismissPairOutgoing, refreshDevices, showToast } = useApp.getState();
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => { setCode(""); setError(null); }, [out?.deviceId]);
  if (!out) return null;

  const valid = code.replace(/[^a-z0-9]/gi, "").length === 6;
  const submit = async () => {
    if (!valid || busy) return;
    setBusy(true);
    setError(null);
    try {
      const id = await api.pairWithCode(code, out.deviceId);
      await refreshDevices();
      showToast(`${id.name} pareado`);
      dismissPairOutgoing();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="dialog-backdrop">
      <div className="dialog">
        <div className="dialog-title">Pedido enviado para {out.name}</div>
        <div className="dialog-body">Aceite o pedido em {out.name}. Um código de 6 caracteres aparece lá; digite aqui.</div>
        <input
          className="input"
          value={code}
          autoFocus
          maxLength={7}
          placeholder="4K7-Q2M"
          autoCapitalize="characters"
          style={{ fontFamily: "var(--font-mono)", letterSpacing: ".18em", textTransform: "uppercase" }}
          onChange={(e) => setCode(e.target.value.toUpperCase())}
          onKeyDown={(e) => { if (e.key === "Enter") void submit(); }}
        />
        {error && <div className="dialog-body" style={{ color: "var(--color-danger-fg)", opacity: 1 }}>{error}</div>}
        <div className="dialog-actions">
          <button className="btn btn-secondary" onClick={dismissPairOutgoing}>Cancelar</button>
          <button className="btn btn-primary" disabled={!valid || busy} onClick={submit}>{busy ? "Pareando…" : "Parear"}</button>
        </div>
      </div>
    </div>
  );
}
