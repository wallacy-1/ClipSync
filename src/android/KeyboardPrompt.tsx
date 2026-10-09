// Nunca ativa nada: "Configurar teclado" só abre a tela do ClipSync sobre o assunto.
import { Keyboard as KeyboardIcon } from "../components/Icons";
import { useT } from "../i18n";
import { rich } from "../components/ui";
import { useApp } from "../store/app";

export function KeyboardPrompt({ onConfigure }: { onConfigure: () => void }) {
  const t = useT();
  const dismiss = useApp((s) => s.dismissKeyboardPrompt);
  return (
    <div className="onboard">
      <div className="logo"><KeyboardIcon size={36} /></div>
      <h3>{t("android.kbPrompt.title")}</h3>
      <p>{rich(t("android.kbPrompt.body"))}</p>
      <div className="steps">
        <div className="step"><span className="n">✓</span><span>{t("android.kbPrompt.point1")}</span></div>
        <div className="step"><span className="n">✓</span><span>{t("android.kbPrompt.point2")}</span></div>
        <div className="step"><span className="n">✓</span><span>{t("android.kbPrompt.point3")}</span></div>
      </div>
      <div className="grow" />
      <button className="btn btn-primary btn-block cta" onClick={() => { void dismiss(); onConfigure(); }}>{t("android.kbPrompt.configure")}</button>
      <button className="btn btn-ghost btn-block" onClick={() => void dismiss()}>{t("android.kbPrompt.notNow")}</button>
    </div>
  );
}
