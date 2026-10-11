//! Textos do núcleo que chegam ao usuário (menu da bandeja, toasts do Windows, erros exibidos na UI),
//! em pt-BR e en. Sem crate: uma tabela `match` com as duas línguas lado a lado (chave faltando em uma
//! delas não compila). O idioma é o `Settings.language_resolved`, que o front grava a partir do
//! `navigator.language`/ajuste; antes do primeiro bootstrap vale pt-BR.
//!
//! Textos de log e nomes de arquivo gerados (`imagem-….png`, `pasta.zip`) ficam como estão.
use crate::models::Settings;
use std::sync::atomic::{AtomicBool, Ordering};

static EN: AtomicBool = AtomicBool::new(false);

/// `code` é o idioma já resolvido ("pt-BR" ou "en"); o que não for `pt*` vira inglês.
pub fn set_lang(code: &str) {
    let code = code.trim().to_ascii_lowercase();
    EN.store(!code.is_empty() && !code.starts_with("pt"), Ordering::Relaxed);
}

pub fn apply(settings: &Settings) {
    set_lang(&settings.language_resolved);
}

#[allow(dead_code)]
pub fn is_en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// Texto da chave no idioma ativo. Chave desconhecida devolve "?" (o teste abaixo pega em dev).
pub fn tr(key: &str) -> &'static str {
    lookup(is_en(), key).unwrap_or("?")
}

/// `tr` com `{nome}` trocado pelos valores dados.
pub fn trf(key: &str, vars: &[(&str, &str)]) -> String {
    let mut s = tr(key).to_string();
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

macro_rules! table {
    ($($k:literal => ($pt:literal, $en:literal)),* $(,)?) => {
        fn lookup(en: bool, key: &str) -> Option<&'static str> {
            match key {
                $($k => Some(if en { $en } else { $pt }),)*
                _ => None,
            }
        }
        #[cfg(test)]
        const KEYS: &[&str] = &[$($k),*];
    };
}

table! {
    // Menu da bandeja
    "tray.window" => ("ClipSync — Painel rápido", "ClipSync — Quick panel"),
    "tray.open" => ("Abrir ClipSync", "Open ClipSync"),
    "tray.paste_last" => ("Copiar último item", "Copy last item"),
    "tray.send_last" => ("Enviar último item", "Send last item"),
    "tray.pause" => ("Pausar sync", "Pause sync"),
    "tray.pause_15" => ("15 minutos", "15 minutes"),
    "tray.pause_60" => ("1 hora", "1 hour"),
    "tray.pause_480" => ("8 horas", "8 hours"),
    "tray.pause_0" => ("Até eu retomar", "Until I resume"),
    "tray.resume" => ("Retomar sync", "Resume sync"),
    "tray.quit" => ("Sair", "Quit"),

    // Atalhos globais (nomes e erros de validação)
    "sc.open_panel" => ("Abrir painel rápido", "Open quick panel"),
    "sc.paste_last" => ("Copiar último", "Copy last"),
    "sc.send_last" => ("Enviar último", "Send last"),
    "sc.toggle_pause" => ("Pausar sync", "Pause sync"),
    "sc.invalid" => ("Atalho inválido: {combo}", "Invalid shortcut: {combo}"),
    "sc.reason" => ("{combo} {why}. Escolha outra combinação.", "{combo} {why}. Pick another combination."),
    "sc.dup" => ("{combo} já é o atalho de \"{other}\".", "{combo} is already the shortcut for \"{other}\"."),
    "sc.taken" => (
        "{combo} ({action}) já está em uso por outro programa. Escolha outra combinação.",
        "{combo} ({action}) is already used by another program. Pick another combination."
    ),
    "sc.why_modifier" => (
        "precisa de Ctrl, Alt ou Win, senão a tecla some da digitação",
        "needs Ctrl, Alt or Win, otherwise the key disappears from typing"
    ),
    "sc.why_win" => ("é reservado do Windows (Win+tecla)", "is reserved by Windows (Win+key)"),
    "sc.why_ctrl" => (
        "é usado pelos programas (Ctrl+tecla: copiar, salvar, buscar…)",
        "is used by programs (Ctrl+key: copy, save, find…)"
    ),
    "sc.why_alt" => ("é usado pelos menus dos programas (Alt+tecla)", "is used by program menus (Alt+key)"),
    "sc.why_windows" => ("é reservado do Windows", "is reserved by Windows"),
    "sc.why_altgr" => ("digita um símbolo com AltGr no teclado ABNT2", "types a symbol with AltGr on ABNT2 keyboards"),
    "sc.why_paste_plain" => ("é \"colar sem formatação\" em muitos programas", "is \"paste without formatting\" in many programs"),
    "sc.why_save_as" => ("é \"Salvar como\" em muitos programas", "is \"Save as\" in many programs"),
    "sc.why_palette" => (
        "é a paleta de comandos e a janela anônima de editores e navegadores",
        "is the command palette and incognito window in editors and browsers"
    ),
    "sc.why_new_folder" => (
        "é \"nova pasta\" no Explorer e janela anônima no Chrome",
        "is \"new folder\" in Explorer and incognito window in Chrome"
    ),
    "sc.why_reopen_tab" => ("reabre a aba fechada nos navegadores", "reopens the closed tab in browsers"),
    "sc.why_redo" => ("é \"refazer\" em muitos programas", "is \"redo\" in many programs"),
    "sc.why_devtools" => ("abre as ferramentas de desenvolvedor nos navegadores", "opens developer tools in browsers"),
    "sc.why_close_window" => ("fecha a janela nos navegadores", "closes the window in browsers"),
    "sc.why_bookmarks" => ("mostra a barra de favoritos nos navegadores", "shows the bookmarks bar in browsers"),
    "sc.why_task_manager" => ("abre o Gerenciador de Tarefas", "opens Task Manager"),
    "sc.why_prev_tab" => ("volta de aba nos programas", "goes back a tab in programs"),
    "sc.why_clear_data" => ("limpa os dados de navegação nos navegadores", "clears browsing data in browsers"),

    // Toasts do Windows
    "notify.file_from" => ("Arquivo recebido de {from}", "File received from {from}"),
    "notify.open" => ("Abrir", "Open"),
    "notify.reveal" => ("Mostrar na pasta", "Show in folder"),
    "notify.wants_to_send" => ("{from} quer enviar um arquivo", "{from} wants to send you a file"),
    "notify.accept" => ("Aceitar", "Accept"),
    "notify.decline" => ("Recusar", "Decline"),
    "notify.sensitive" => ("Conteúdo sensível (oculto)", "Sensitive content (hidden)"),
    "notify.copied_from" => ("Copiado de {from}", "Copied from {from}"),
    "notify.received_from" => ("Recebido de {from}", "Received from {from}"),
    "notify.other_device" => ("outro dispositivo", "another device"),
    // Notificação do Foreground Service de transferência (Android)
    "fgs.sending" => ("Enviando arquivo", "Sending file"),
    "fgs.receiving" => ("Recebendo arquivo", "Receiving file"),

    // Prefixos de AppError
    "err.db" => ("banco de dados", "database"),
    "err.io" => ("io", "io"),
    "err.crypto" => ("criptografia", "encryption"),
    "err.net" => ("rede", "network"),
    "err.pairing" => ("pareamento", "pairing"),

    // Erros que chegam à UI
    "err.item_not_found" => ("item não encontrado", "item not found"),
    "err.not_image" => ("o item não é uma imagem", "the item is not an image"),
    "err.image_gone" => ("imagem não está mais disponível", "image is no longer available"),
    "err.image_old_peer" => ("dispositivo desatualizado não recebe imagens", "outdated device can't receive images"),
    "err.image_unsupported" => ("imagem no clipboard indisponível nesta plataforma", "clipboard images aren't available on this platform"),
    "err.transfer_missing" => ("transferência {id} não encontrada", "transfer {id} not found"),
    "err.transfer_register" => ("falha ao registrar a transferência", "failed to register the transfer"),
    "err.dest_invalid" => ("nome de destino inválido", "invalid destination name"),
    "err.dest_exists" => ("destino já existe", "destination already exists"),
    "err.png_invalid" => ("PNG inválido: {e}", "Invalid PNG: {e}"),
    "err.png_color" => ("PNG com cor não suportada: {e}", "PNG color type not supported: {e}"),
    "err.core_not_started" => ("ClipSync ainda não iniciou", "ClipSync hasn't started yet"),
    "err.bad_text" => ("texto inválido", "invalid text"),
    "err.internal" => ("erro interno", "internal error"),
    // Nomes de arquivo gerados (arquivos existentes mantêm o nome que já têm)
    "file.image" => ("imagem", "image"),
    "file.folder" => ("pasta", "folder"),
    "err.text_too_big" => ("texto vazio ou grande demais", "text is empty or too large"),
    "err.text_too_big_1mb" => ("texto vazio ou grande demais (máx. 1 MB)", "text is empty or too large (max 1 MB)"),
    "err.device_not_found" => ("dispositivo não encontrado", "device not found"),
    "err.device_blocked" => ("dispositivo bloqueado", "device is blocked"),
    "err.device_not_paired" => ("dispositivo não pareado", "device is not paired"),
    "err.open_not_web" => ("Só links http/https podem ser abertos", "Only http/https links can be opened"),
    "err.open_bad_email" => ("Endereço de e-mail inválido", "Invalid email address"),
    "err.open_nothing" => ("Este item não tem link para abrir", "This item has no link to open"),
    "err.addr_invalid" => ("Endereço inválido. Use algo como 192.168.0.10", "Invalid address. Use something like 192.168.0.10"),
    "err.addr_v6_scope" => (
        "Endereço IPv6 com interface (%) não é suportado; use um endereço global",
        "IPv6 addresses with an interface (%) aren't supported; use a global address"
    ),
    "err.addr_v6_link_local" => (
        "Endereço IPv6 local (fe80::) não funciona sem a interface; use um endereço global",
        "Link-local IPv6 addresses (fe80::) don't work without the interface; use a global address"
    ),
    "err.picker_no_activity" => ("sem Activity para abrir o seletor", "no Activity to open the picker"),
    "err.picker_no_reply" => ("seletor não respondeu", "the picker didn't respond"),
    "err.picker_unavailable" => ("seletor de arquivos indisponível nesta plataforma", "file picker isn't available on this platform"),
    "err.transfer_not_found" => ("transferência não encontrada", "transfer not found"),
    "err.transfer_unfinished" => ("a transferência ainda não terminou", "the transfer hasn't finished yet"),
    "err.transfer_running" => ("transferência em andamento", "transfer in progress"),
    "err.transfer_sender_only" => (
        "só o lado que envia pode retomar; o outro reoferece ao reconectar",
        "only the sending side can resume; the other re-offers on reconnect"
    ),
    "err.transfer_queue_full" => ("fila de transferências cheia para este dispositivo", "transfer queue is full for this device"),
    "err.file_too_big" => ("arquivo maior que o limite de {gib} GiB", "file is larger than the {gib} GiB limit"),
    "err.file_gone" => ("o arquivo não está mais disponível", "the file is no longer available"),
    "err.file_unreadable" => ("arquivo inacessível: {e}", "file is not accessible: {e}"),
    "err.not_a_file" => ("não é um arquivo", "not a file"),
    "err.downloads_unavailable" => ("pasta de downloads indisponível: {e}", "downloads folder is not available: {e}"),
    "err.folder_unavailable" => ("envio de pasta indisponível nesta plataforma", "sending folders isn't available on this platform"),
    "err.ask_expired" => ("pedido não encontrado ou expirado", "request not found or expired"),
    "err.update_desktop_only" => ("atualização pelo app só no desktop", "in-app updates are desktop only"),
    "err.update_none" => ("nenhuma atualização disponível", "no update available"),
    "err.update_running" => ("atualização já em andamento", "update already in progress"),
    "err.android_only" => ("disponível só no Android", "available on Android only"),
    "err.keyboard_android_only" => ("ClipSync Keyboard só existe no Android", "ClipSync Keyboard only exists on Android"),
    "err.autostart_on" => (
        "não foi possível ligar o início com o Windows: {e}",
        "couldn't turn on starting with Windows: {e}"
    ),
    "err.autostart_off" => (
        "não foi possível desligar o início com o Windows: {e}",
        "couldn't turn off starting with Windows: {e}"
    ),

    // Pareamento e conexão (aparecem em toasts e no estado do dispositivo)
    "err.pair_no_addr" => ("nenhum endereço alcançável", "no reachable address"),
    "err.pair_self" => ("este é o próprio dispositivo", "this is the device itself"),
    "err.pair_code_len" => ("código deve ter 6 caracteres", "the code must have 6 characters"),
    "err.pair_none_listening" => ("nenhum dispositivo em modo de pareamento na rede", "no device in pairing mode on the network"),
    "err.pair_bad_code" => ("código incorreto", "wrong code"),
    "err.pair_bad_code_or_expired" => ("código incorreto ou expirado", "wrong or expired code"),
    "err.pair_bad_code_or_silent" => (
        "código incorreto, expirado ou o outro dispositivo não respondeu",
        "wrong or expired code, or the other device didn't respond"
    ),
    "err.pair_no_reply" => ("o outro dispositivo não respondeu", "the other device didn't respond"),
    "err.pair_no_reply_update" => (
        "o outro dispositivo não respondeu; atualize o ClipSync nele",
        "the other device didn't respond; update ClipSync on it"
    ),
    "err.pair_cancelled" => ("pareamento cancelado", "pairing cancelled"),
    "err.pair_identity" => ("identidade não confere com a chave do handshake", "identity doesn't match the handshake key"),
    "err.pair_old_peer" => ("o outro dispositivo usa uma versão antiga do ClipSync", "the other device uses an old ClipSync version"),
    "err.pair_no_session" => ("nenhuma sessão de pareamento ativa", "no active pairing session"),
    "err.pair_left_network" => ("o dispositivo não está mais na rede", "the device is no longer on the network"),
    "err.pair_unreachable" => ("não foi possível alcançar o dispositivo", "couldn't reach the device"),
    "err.pair_bad_reply" => ("resposta inválida", "invalid reply"),
    "err.pair_declined" => ("pedido recusado", "request declined"),
    "err.pair_throttled" => ("aguarde alguns segundos e peça de novo", "wait a few seconds and ask again"),
    "err.pair_no_sender" => ("pedido sem remetente", "request without a sender"),
    "err.qr_not_clipsync" => ("QR Code não é do ClipSync", "This QR code isn't from ClipSync"),
    "err.qr_other_version" => (
        "QR de outra versão do ClipSync; atualize os dois dispositivos",
        "QR code from another ClipSync version; update both devices"
    ),
    "err.qr_invalid" => ("QR Code inválido", "Invalid QR code"),
    "err.net_timeout_connect" => ("timeout conectando em {addr}", "timed out connecting to {addr}"),
    "err.net_unreachable" => ("não foi possível alcançar {addr}", "couldn't reach {addr}"),
    "err.net_hello_timeout" => ("timeout no hello", "timed out on hello"),
    "err.net_handshake_timeout" => ("timeout no handshake", "timed out on handshake"),
    "err.net_protocol" => ("versão de protocolo incompatível", "incompatible protocol version"),
    "err.net_idle" => ("conexão ociosa", "idle connection"),
    "err.net_closed" => ("conexão fechada", "connection closed"),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_both_languages() {
        for key in KEYS {
            assert!(!lookup(false, key).unwrap().is_empty(), "{key} (pt)");
            assert!(!lookup(true, key).unwrap().is_empty(), "{key} (en)");
        }
    }

    #[test]
    fn language_switch_and_vars() {
        set_lang("en");
        assert_eq!(tr("tray.quit"), "Quit");
        assert_eq!(trf("sc.invalid", &[("combo", "Ctrl+X")]), "Invalid shortcut: Ctrl+X");
        set_lang("pt-BR");
        assert_eq!(tr("tray.quit"), "Sair");
        set_lang("");
        assert!(!is_en());
        assert_eq!(tr("nope"), "?");
    }
}
