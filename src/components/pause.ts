import { locale, t, type Key } from "../i18n";

const option = (minutes: number | undefined, key: Key) => ({ minutes, get label() { return t(key); } });

/** Durações oferecidas para pausar a sincronização (`minutes` indefinido = até retomar). `label` segue o idioma ativo. */
export const PAUSE_OPTIONS: { minutes: number | undefined; readonly label: string }[] = [
  option(15, "pause.min15"),
  option(60, "pause.hour1"),
  option(480, "pause.hour8"),
  option(undefined, "pause.untilResume"),
];

/** "até 14:30" ou "até você retomar", para frases do tipo "Pausado …". */
export function pausedUntilText(pausedUntil: number | null): string {
  if (!pausedUntil) return t("pause.untilYouResume");
  return t("pause.untilTime", { time: new Date(pausedUntil).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" }) });
}
