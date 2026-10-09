import { ClipboardCheck } from "../components/Icons";
import { useT } from "../i18n";
import { useApp } from "../store/app";

export function Onboarding({ onPair }: { onPair: () => void }) {
  const t = useT();
  const finish = useApp((s) => s.finishOnboarding);
  return (
    <div className="onboard">
      <div className="logo"><ClipboardCheck size={36} /></div>
      <h3>{t("android.onboarding.title")}</h3>
      <p>{t("android.onboarding.body")}</p>
      <div className="steps">
        <div className="step"><span className="n">1</span><span>{t("android.onboarding.step1")}</span></div>
        <div className="step"><span className="n">2</span><span>{t("android.onboarding.step2")}</span></div>
        <div className="step"><span className="n">3</span><span>{t("android.onboarding.step3")}</span></div>
      </div>
      <div className="grow" />
      <button className="btn btn-primary btn-block cta" onClick={onPair}>{t("android.onboarding.pair")}</button>
      <button className="btn btn-ghost btn-block" onClick={() => finish()}>{t("android.onboarding.localOnly")}</button>
    </div>
  );
}
