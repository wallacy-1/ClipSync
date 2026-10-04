//! Transferência de arquivos P2P sobre o mesmo canal Noise/TCP do clipboard (quadros tag 2).
//! Estado em SQLite; bytes só no filesystem (`.part` → destino após SHA-256).
#[cfg(target_os = "android")]
pub mod android;
pub mod dest;
pub mod folder;
pub mod hash;
pub mod manager;
pub mod names;
pub mod receiver;
pub mod sender;
pub mod source;
pub mod state;
pub mod store;
/// Unidade lógica de índice/ack/checkpoint (= 5 quadros Noise de ~60 KB).
pub const CHUNK_SIZE: u32 = 256 * 1024;
/// Chunks em voo sem `FileAck` antes de o emissor bloquear (2 MiB).
pub const WINDOW_CHUNKS: u32 = 8;
/// Receptor faz fsync + checkpoint + `FileAck` a cada N chunks (1 MiB).
pub const ACK_EVERY_CHUNKS: u32 = 4;
/// Folga para `u32` de chunks e FAT32.
pub const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_CHUNKS: u32 = (MAX_FILE_SIZE / CHUNK_SIZE as u64) as u32;
/// Máximo de transferências abertas (fila + ativas) por device.
pub const MAX_OPEN_PER_DEVICE: usize = 20;
/// Sem `FileAck` por este tempo com dados pendentes → considera a conexão morta.
pub const ACK_TIMEOUT_SECS: u64 = 30;
/// Emissor: espera pelo `FileAccept` de um peer que pergunta ao usuário (`file-ask-v1`).
pub const ASK_WAIT_SECS: u64 = 10 * 60;
/// Receptor: pedido sem resposta é recusado antes de o emissor desistir.
pub const ASK_TTL_SECS: u64 = 9 * 60;
/// Espaço livre exigido: `size * 105 % + este extra`.
pub const FREE_SPACE_MARGIN: u64 = 64 * 1024 * 1024;

/// `total_chunks` correto para um tamanho; 0 B → 0 chunks (só Offer/Done/Result).
pub fn total_chunks(size: u64, chunk_size: u32) -> u32 {
    if chunk_size == 0 {
        return 0;
    }
    size.div_ceil(chunk_size as u64).min(u32::MAX as u64) as u32
}

/// Validação de um `FileOffer` recebido: coerência interna e limites locais.
pub fn validate_offer(size: u64, chunk_size: u32, total_chunks_declared: u32) -> std::result::Result<(), &'static str> {
    if chunk_size == 0 || chunk_size > 4 * CHUNK_SIZE {
        return Err("chunk_size inválido");
    }
    if size > MAX_FILE_SIZE {
        return Err("tooLarge");
    }
    if total_chunks_declared != total_chunks(size, chunk_size) {
        return Err("total_chunks não confere com size/chunk_size");
    }
    if total_chunks_declared > MAX_CHUNKS {
        return Err("tooLarge");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_math() {
        assert_eq!(total_chunks(0, CHUNK_SIZE), 0);
        assert_eq!(total_chunks(1, CHUNK_SIZE), 1);
        assert_eq!(total_chunks(CHUNK_SIZE as u64, CHUNK_SIZE), 1);
        assert_eq!(total_chunks(CHUNK_SIZE as u64 + 1, CHUNK_SIZE), 2);
        assert_eq!(total_chunks(MAX_FILE_SIZE, CHUNK_SIZE), MAX_CHUNKS);
        assert_eq!(MAX_CHUNKS, 16_384);
    }

    #[test]
    fn offer_validation() {
        assert!(validate_offer(1_000, CHUNK_SIZE, 1).is_ok());
        assert!(validate_offer(0, CHUNK_SIZE, 0).is_ok());
        assert_eq!(validate_offer(1_000, CHUNK_SIZE, 2), Err("total_chunks não confere com size/chunk_size"));
        assert_eq!(validate_offer(MAX_FILE_SIZE + 1, CHUNK_SIZE, MAX_CHUNKS + 1), Err("tooLarge"));
        assert_eq!(validate_offer(1, 0, 1), Err("chunk_size inválido"));
        assert_eq!(validate_offer(1, 8 * CHUNK_SIZE, 1), Err("chunk_size inválido"));
    }
}
