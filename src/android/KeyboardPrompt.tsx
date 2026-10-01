// Nunca ativa nada: "Configurar teclado" só abre a tela do ClipSync sobre o assunto.
import { Keyboard as KeyboardIcon } from "../components/Icons";
import { useApp } from "../store/app";

export function KeyboardPrompt({ onConfigure }: { onConfigure: () => void }) {
  const dismiss = useApp((s) => s.dismissKeyboardPrompt);
  return (
    <div className="onboard">
      <div className="logo"><KeyboardIcon size={36} /></div>
      <h3>Sincronização automática no Android</h3>
      <p>
        O Android só deixa um aplicativo ler a área de transferência enquanto ele está na tela.
        Para permitir que o ClipSync detecte cópias enquanto você usa outros aplicativos, você pode
        ativar o <strong>ClipSync Keyboard</strong>, um teclado simples e opcional.
      </p>
      <div className="steps">
        <div className="step"><span className="n">✓</span><span>Você não precisa usar o teclado: o ClipSync funciona sem ele.</span></div>
        <div className="step"><span className="n">✓</span><span>Ele não registra nem envia o que você digita.</span></div>
        <div className="step"><span className="n">✓</span><span>A ativação é feita por você, nas Configurações do Android.</span></div>
      </div>
      <div className="grow" />
      <button className="btn btn-primary btn-block cta" onClick={() => { void dismiss(); onConfigure(); }}>Configurar teclado</button>
      <button className="btn btn-ghost btn-block" onClick={() => void dismiss()}>Agora não</button>
    </div>
  );
}
