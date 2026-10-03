//! Toasts do Windows: arquivo recebido (Abrir / Mostrar na pasta), pedido de aceite
//! (Aceitar / Recusar) e, se ligado, texto recebido. Só quando a janela principal não
//! está em foco (com ela em foco a UI já mostra). Fora do Windows tudo é no-op.
use crate::models::ClipboardItem;
use crate::transfer::store::TransferRecord;
use tauri::AppHandle;

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::state::AppState;
    use tauri::Manager;
    use tauri_winrt_notification::Toast;

    /// Instalado, o atalho do instalador registra o AUMID = identifier. Em debug não há
    /// atalho: sem um AUMID registrado o Windows descarta o toast calado.
    fn app_id(app: &AppHandle) -> String {
        if cfg!(debug_assertions) {
            Toast::POWERSHELL_APP_ID.to_string()
        } else {
            app.config().identifier.clone()
        }
    }

    fn wanted(app: &AppHandle) -> bool {
        if !app.state::<AppState>().settings().notifications {
            return false;
        }
        let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
        log::debug!("[NOTIFY] main focused={focused}");
        !focused
    }

    fn device_name(app: &AppHandle, id: &str) -> String {
        app.state::<AppState>().db.get_device(id).ok().flatten().map(|d| d.name).unwrap_or_else(|| "outro dispositivo".into())
    }

    fn size(n: u64) -> String {
        const MB: f64 = 1024.0 * 1024.0;
        match n {
            n if n < 1024 => format!("{n} B"),
            n if (n as f64) < MB => format!("{:.0} KB", n as f64 / 1024.0),
            n if (n as f64) < 1024.0 * MB => format!("{:.1} MB", n as f64 / MB).replace('.', ","),
            n => format!("{:.2} GB", n as f64 / (1024.0 * MB)).replace('.', ","),
        }
    }

    /// Botão = `"<ação>:<id>"`; clique no corpo = `None` (abre a janela).
    fn on_action(app: &AppHandle, action: Option<String>) {
        let Some(action) = action else {
            crate::tray::show_main(app);
            return;
        };
        let (verb, id) = action.split_once(':').unwrap_or((action.as_str(), ""));
        let r = match verb {
            "open" => crate::commands::open_transfer(app, id, false),
            "reveal" => crate::commands::open_transfer(app, id, true),
            "accept" => crate::transfer::manager::accept(app, id),
            "decline" => crate::transfer::manager::decline(app, id).map(|_| ()),
            _ => Ok(()),
        };
        if let Err(e) = r {
            log::warn!("[NOTIFY] ação {verb} falhou id={id}: {e}");
        }
    }

    fn show(app: &AppHandle, toast: Toast) {
        let app2 = app.clone();
        let toast = toast.on_activated(move |action| {
            on_action(&app2, action);
            Ok(())
        });
        if let Err(e) = toast.show() {
            log::warn!("[NOTIFY] toast falhou: {e}");
        }
    }

    pub fn file_received(app: &AppHandle, rec: &TransferRecord) {
        if !wanted(app) {
            return;
        }
        let from = device_name(app, &rec.device_id);
        let toast = Toast::new(&app_id(app))
            .title(&format!("Arquivo recebido de {from}"))
            .text1(&rec.safe_name)
            .text2(&format!("{} · Downloads\\ClipSync", size(rec.size)))
            .add_button("Abrir", &format!("open:{}", rec.id))
            .add_button("Mostrar na pasta", &format!("reveal:{}", rec.id));
        show(app, toast);
    }

    pub fn ask(app: &AppHandle, rec: &TransferRecord) {
        // Pedido de aceite avisa mesmo com a janela aberta mas sem foco; com foco a UI já pergunta.
        if !wanted(app) {
            return;
        }
        let from = device_name(app, &rec.device_id);
        let toast = Toast::new(&app_id(app))
            .title(&format!("{from} quer enviar um arquivo"))
            .text1(&rec.safe_name)
            .text2(&size(rec.size))
            .add_button("Aceitar", &format!("accept:{}", rec.id))
            .add_button("Recusar", &format!("decline:{}", rec.id));
        show(app, toast);
    }

    pub fn text_received(app: &AppHandle, item: &ClipboardItem, from_device_id: &str, applied: bool) {
        if !wanted(app) || !app.state::<AppState>().settings().notify_text {
            return;
        }
        let from = device_name(app, from_device_id);
        // Conteúdo sensível nunca vai para a central de notificações.
        let preview = if item.is_sensitive {
            "Conteúdo sensível (oculto)".to_string()
        } else {
            let t: String = item.content.chars().take(120).collect();
            t.split_whitespace().collect::<Vec<_>>().join(" ")
        };
        let title = if applied { format!("Copiado de {from}") } else { format!("Recebido de {from}") };
        show(app, Toast::new(&app_id(app)).title(&title).text1(&preview));
    }
}

#[cfg(windows)]
pub use imp::{ask, file_received, text_received};

#[cfg(not(windows))]
pub fn file_received(_app: &AppHandle, _rec: &TransferRecord) {}
#[cfg(not(windows))]
pub fn ask(_app: &AppHandle, _rec: &TransferRecord) {}
#[cfg(not(windows))]
pub fn text_received(_app: &AppHandle, _item: &ClipboardItem, _from_device_id: &str, _applied: bool) {}
