//! Núcleo reutilizável do Zeebx.
//!
//! O motor é compartilhado pelo desktop, Libretro, headless e Android.

pub mod audio;
pub mod brew;
pub mod config;
pub mod cpu;
pub mod input;
pub mod library;
pub mod loader;
pub mod machine;
pub mod ponte;
pub mod rede;
pub mod save_state;
pub mod session;
pub mod storage;
pub mod ui;
pub mod video;

/// Configuração histórica da linha de comando: um Dragon na primeira porta e segunda livre.
pub const PORTAS_PADRAO: [Option<input::bindings::Aparelho>; input::PORTAS] =
    [Some(input::bindings::Aparelho::Controle), None];

#[cfg(test)]
pub mod scratch;

#[cfg(test)]
pub mod varredura;
