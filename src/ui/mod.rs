//! O que a interface guarda, separado em dois: o que precisa de uma janela do host e o que não
//! precisa.
//!
//! A biblioteca, os ajustes, os saves e o catálogo de idiomas são lidos **pelo núcleo** — o
//! `machine`, o `loader` e a [`crate::session`] falam com eles —, então existem em toda
//! plataforma. A [`app::App`], a vitrine, o pintor de GL e a janela sem interface só existem
//! onde há uma janela de desktop: no Android quem monta a tela é outro frontend, sobre o mesmo
//! núcleo.

pub mod acervo;
pub mod depuracao;
pub mod gpu;
pub mod i18n;
pub mod library;
pub mod saves;
pub mod settings;

#[cfg(not(target_os = "android"))]
pub mod app;
#[cfg(not(target_os = "android"))]
pub mod atualizacao;
#[cfg(not(target_os = "android"))]
pub mod discord;
#[cfg(not(target_os = "android"))]
pub mod window;

#[cfg(not(target_os = "android"))]
pub use app::App;
