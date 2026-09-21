//! Diretório temporário que se apaga sozinho, para os testes.
//!
//! Existia um vazamento pequeno e constante na suíte: os testes criavam `/tmp/zeebx-sql-*` e
//! `/tmp/zeebx-saves-*` e limpavam **no começo** — para o caso de a rodada anterior ter falhado —
//! e nunca no fim. Uma sessão com várias rodadas deixa dezenas de diretórios para trás. Com este
//! guarda, o diretório sai quando o teste termina, passe ele ou não.

use std::path::{Path, PathBuf};

/// Uma pasta em `temp_dir()` que se remove ao sair de escopo.
pub struct TempDir(PathBuf);

impl TempDir {
    /// Cria a pasta, apagando o que houver com o mesmo nome.
    pub fn new(nome: &str) -> Self {
        let dir = std::env::temp_dir().join(nome);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("criar diretório temporário");
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Um caminho dentro da pasta.
    pub fn join(&self, resto: &str) -> PathBuf {
        self.0.join(resto)
    }

    /// O caminho como `PathBuf`, para quem precisa de dono.
    pub fn to_path_buf(&self) -> PathBuf {
        self.0.clone()
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
