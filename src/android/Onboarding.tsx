import { ClipboardCheck } from "../components/Icons";
import { useApp } from "../store/app";

export function Onboarding({ onPair }: { onPair: () => void }) {
  const finish = useApp((s) => s.finishOnboarding);
  return (
    <div className="onboard">
      <div className="logo"><ClipboardCheck size={36} /></div>
      <h3>Sua área de transferência em todo lugar</h3>
      <p>Copie no PC, cole no celular. Tudo pela sua rede local, cifrado, sem conta e sem nuvem.</p>
      <div className="steps">
        <div className="step"><span className="n">1</span><span>Instale o ClipSync no seu PC</span></div>
        <div className="step"><span className="n">2</span><span>Escaneie o código que aparece na janela</span></div>
        <div className="step"><span className="n">3</span><span>Pronto — o histórico já é compartilhado</span></div>
      </div>
      <div className="grow" />
      <button className="btn btn-primary btn-block cta" onClick={onPair}>Parear com o PC</button>
      <button className="btn btn-ghost btn-block" onClick={() => finish()}>Usar só neste dispositivo</button>
    </div>
  );
}
