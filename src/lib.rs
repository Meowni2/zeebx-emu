//! Motor reutilizável do Zeebx.
//!
//! O binário desktop ainda declara módulos próprios durante a migração. Este alvo é a fronteira
//! que o frontend Libretro passará a depender, permitindo retirar UI e dispositivos do host por
//! feature sem duplicar o motor BREW.

pub mod audio;
pub mod brew;
pub mod config;
pub mod cpu;
pub mod input;
pub mod loader;
pub mod library;
pub mod machine;
pub mod ponte;
pub mod rede;
pub mod session;
pub mod storage;
#[cfg(feature = "desktop")]
pub mod ui;
pub mod video;

/// Configuração histórica da linha de comando: um Dragon na primeira porta e segunda livre.
/// Frontends devem escolher explicitamente os aparelhos que o guest enxerga.
pub const PORTAS_PADRAO: [Option<input::bindings::Aparelho>; input::PORTAS] =
    [Some(input::bindings::Aparelho::Controle), None];

/// Varredura de ROMs por teste. Depende de `PORTAS_PADRAO`, logo fica junto do motor enquanto os
/// testes de compatibilidade ainda usam `Session`.
#[cfg(test)]
pub mod varredura;
