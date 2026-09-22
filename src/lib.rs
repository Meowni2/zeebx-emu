//! O núcleo do Zeebx como biblioteca.
//!
//! O emulador sempre foi separável do que o observa — a linha de comando (`zeebx run`) e a
//! janela do egui já eram dois frontends do mesmo laço. O que faltava era dizer isso ao Cargo:
//! aqui os módulos viram uma biblioteca, e o binário de desktop passa a ser mais um consumidor
//! dela, ao lado do `.so` que o Android carrega.
//!
//! O que depende de uma janela de desktop está atrás de `cfg(not(target_os = "android"))` —
//! ver [`ui`].

pub mod audio;
pub mod brew;
pub mod cpu;
pub mod input;
pub mod loader;
pub mod machine;
pub mod ponte;
pub mod rede;
pub mod session;
pub mod ui;
pub mod video;

/// Varredura de ROMs por teste — ver [`varredura`]. Só existe em compilação de teste.
#[cfg(test)]
mod varredura;

/// O padrão sem janela: um controle na primeira porta, a segunda livre. É o que sempre houve.
///
/// Mora aqui, e não no binário de desktop, porque quem roda sem janela — a varredura de ROMs,
/// e agora o Android — precisa do mesmo padrão.
pub const PORTAS_PADRAO: [Option<input::bindings::Aparelho>; input::PORTAS] =
    [Some(input::bindings::Aparelho::Controle), None];
