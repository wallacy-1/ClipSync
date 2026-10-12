import { useState } from "react";
import { ClipboardCheck } from "../components/Icons";
import { useT } from "../i18n";
import { useApp } from "../store/app";
import { PermissionsSetup } from "./PermissionsSetup";

export function Onboarding({ onPair }: { onPair: () => void }) {
  const t = useT();
  const finish = useApp((s) => s.finishOnboarding);
  const platform = useApp((s) => s.platform);
  const [step, setStep] = useState<"intro" | "perms">("intro");

  if (step === "perms") {
    return (
      <PermissionsSetup>
        {(ready) => (
          <>
            <button className="btn btn-primary btn-block cta" disabled={!ready} onClick={onPair}>{t("android.onboarding.pair")}</button>
            <button className="btn btn-ghost btn-block" onClick={() => void finish()}>{t("android.onboarding.localOnly")}</button>
          </>
        )}
      </PermissionsSetup>
    );
  }

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
      {platform === "android" ? (
        <button className="btn btn-primary btn-block cta" onClick={() => setStep("perms")}>{t("android.onboarding.start")}</button>
      ) : (
        <>
          <button className="btn btn-primary btn-block cta" onClick={onPair}>{t("android.onboarding.pair")}</button>
          <button className="btn btn-ghost btn-block" onClick={() => void finish()}>{t("android.onboarding.localOnly")}</button>
        </>
      )}
    </div>
  );
}
