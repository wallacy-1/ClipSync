// i18n sem biblioteca: `pt.ts` é a fonte, `en.ts` é tipado como `typeof pt` (chave faltando = erro de tipo).
// O idioma ativo vive no store (`lang`); `setLang` só espelha aqui para `t()` poder ser chamado fora de componentes.
import { useMemo } from "react";
import { useApp } from "../store/app";
import { en } from "./en";
import { pt } from "./pt";

export type Lang = "pt-BR" | "en";
export type LangPref = "auto" | Lang;

type Plural = { one: string; other: string };
type Leaf = string | Plural;
type Tree = { [k: string]: Leaf | Tree };
type Paths<T, P extends string = ""> = {
  [K in keyof T & string]: T[K] extends Leaf ? `${P}${K}` : Paths<T[K], `${P}${K}.`>;
}[keyof T & string];

export type Key = Paths<typeof pt>;
export type Vars = Record<string, string | number>;

const TABLES: Record<Lang, Tree> = { "pt-BR": pt, en };

let current: Lang = "pt-BR";

export function setLang(lang: Lang) {
  current = lang;
  if (typeof document !== "undefined") document.documentElement.lang = lang;
}

export const getLang = () => current;

/** Locale para `Intl`/`toLocale*String`: o idioma ativo do app, não o do sistema. */
export const locale = () => current;

/** "auto" → idioma do sistema (`pt*` → pt-BR, qualquer outro → en). */
export function resolveLang(pref: LangPref | undefined): Lang {
  if (pref === "pt-BR" || pref === "en") return pref;
  const sys = (typeof navigator !== "undefined" && navigator.language) || "pt-BR";
  return sys.toLowerCase().startsWith("pt") ? "pt-BR" : "en";
}

function lookup(table: Tree, key: string): Leaf | undefined {
  let node: Leaf | Tree | undefined = table;
  for (const part of key.split(".")) {
    if (node === undefined || typeof node === "string" || "one" in node) return undefined;
    node = (node as Tree)[part];
  }
  return node as Leaf | undefined;
}

/**
 * Texto traduzido. `{nome}` é trocado por `vars.nome`. Valor `{ one, other }` escolhe a forma por `vars.n`
 * (via `Intl.PluralRules`; 0 conta como plural também em pt, onde o CLDR o põe em "one").
 */
export function t(key: Key, vars?: Vars): string {
  const leaf = lookup(TABLES[current], key) ?? lookup(TABLES["pt-BR"], key);
  if (leaf === undefined) return key;
  let text: string;
  if (typeof leaf === "string") {
    text = leaf;
  } else {
    const n = Number(vars?.n ?? 0);
    const form = n === 0 ? "other" : new Intl.PluralRules(current).select(n);
    text = form === "one" ? leaf.one : leaf.other;
  }
  return vars ? text.replace(/\{(\w+)\}/g, (m, name: string) => (name in vars ? String(vars[name]) : m)) : text;
}

/** `t` ligado ao idioma do store: o componente re-renderiza ao trocar de idioma. */
export function useT() {
  const lang = useApp((s) => s.lang);
  // Identidade nova a cada idioma: quem usa `t` em deps de memo/efeito recalcula.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => (key: Key, vars?: Vars) => t(key, vars), [lang]);
}
