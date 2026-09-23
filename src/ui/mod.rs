//! O que a interface guarda, separado em dois: o que precisa de uma janela do host e o que não
//! precisa.
//!
//! A biblioteca, os ajustes, os saves e o catálogo de idiomas são lidos **pelo núcleo** — o
//! `machine`, o `loader` e a [`crate::session`] falam com eles —, então existem em toda
//! plataforma. A [`app::App`], a vitrine e a janela sem interface só existem onde há uma janela
//! de desktop. No Android quem monta a tela é outro frontend, sobre o mesmo núcleo — e sobre o
//! mesmo painel de depuração e o mesmo pintor de GL.

/// O repositório do projeto.
///
/// Mora aqui, e não em cada frontend, porque é o mesmo projeto visto de telas diferentes. A
/// primeira versão da tela "sobre" do Android trazia a própria cópia destes dois endereços, e o
/// do Discord estava simplesmente errado — um convite inventado, que não levava a lugar nenhum.
/// Uma constante duplicada não avisa quando as cópias divergem; uma constante só, sim.
pub const REPOSITORIO: &str = "https://github.com/ZeebxTeam/zeebx-emu";

/// O convite do servidor de conversa.
pub const DISCORD: &str = "https://discord.gg/D96HjsKTPa";

pub mod acervo;
/// O painel de depuração e o pintor de GL são os mesmos no desktop e no Android. Quem monta a
/// janela é cada frontend; estes dois só desenham.
#[cfg(any(feature = "desktop", target_os = "android"))]
pub mod depuracao;
#[cfg(any(feature = "desktop", target_os = "android"))]
pub mod gpu;
pub mod i18n;
pub mod library;
pub mod saves;
pub mod settings;

#[cfg(feature = "desktop")]
pub mod app;
#[cfg(feature = "desktop")]
pub mod atualizacao;
#[cfg(feature = "desktop")]
pub mod discord;
#[cfg(feature = "desktop")]
pub mod window;

#[cfg(feature = "desktop")]
pub use app::App;
