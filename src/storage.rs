//! Raízes persistentes e identidade imutável de conteúdo.
//!
//! O frontend Libretro vai fornecer a raiz de saves. O motor não deve espalhar cache, dados do
//! aparelho e saves de cada título dentro da ROM nem depender de `HOME`: todos nascem desta
//! estrutura. Por enquanto os caminhos são nativos; o adaptador `StorageFs` futuro trocará o
//! transporte, sem mudar este layout lógico.

use std::io::Read;
use std::path::{Path, PathBuf};

/// Diretórios que pertencem a um perfil Zeebx.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePaths {
    /// Raiz `.../zeebx` criada sob o diretório de saves do frontend.
    pub root: PathBuf,
    /// Conteúdo extraído, descartável e somente-leitura lógica.
    pub cache: PathBuf,
    /// O sistema de arquivos compartilhado do aparelho (`fs:/`).
    pub device: PathBuf,
    /// Overlay gravável de cada conteúdo.
    pub saves: PathBuf,
    /// Manifestos e metadados de conteúdo.
    pub metadata: PathBuf,
    /// Se o perfil usa overlay gravável por título.
    ///
    /// Falso no desktop histórico, onde o jogo grava ao lado do `.mod`; verdadeiro quando o
    /// frontend delimita a raiz de saves e o pacote precisa ficar intacto.
    pub overlay: bool,
}

impl StoragePaths {
    /// Cria o layout a partir de uma raiz de perfil já escolhida.
    ///
    /// A UI desktop atual usa sua configuração diretamente como raiz. O frontend Libretro usa
    /// [`StoragePaths::from_save_dir`] para acrescentar a pasta `zeebx` sem escrever na ROM.
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            cache: root.join("cache"),
            device: root.join("aparelho"),
            saves: root.join("saves"),
            metadata: root.join("metadata"),
            root,
            // O desktop histórico grava dentro do próprio jogo; só um frontend que delimita a
            // raiz pede overlay.
            overlay: false,
        }
    }

    /// Cria o layout lógico do perfil dentro de `save_dir`.
    ///
    /// É a raiz que um frontend Libretro fornece: pasta `zeebx` própria, nunca ao lado da ROM.
    pub fn from_save_dir(save_dir: impl AsRef<Path>) -> Self {
        Self {
            overlay: true,
            ..Self::from_root(save_dir.as_ref().join("zeebx"))
        }
    }

    /// Identidade do conteúdo, para nomear overlay e cache.
    ///
    /// O hash é dos bytes do arquivo escolhido no frontend — o `.zip` quando é pacote, o `.mod`
    /// quando é módulo solto. É o que separa o save de duas versões do mesmo título.
    pub fn content_id(&self, path: &Path) -> std::io::Result<ContentId> {
        ContentId::from_reader(std::fs::File::open(path)?)
    }

    /// Onde fica o overlay privado de um conteúdo.
    pub fn save_for(&self, content: &ContentId) -> PathBuf {
        self.saves.join(content.as_str())
    }

    /// Onde fica o conteúdo extraído e reutilizável.
    pub fn cache_for(&self, content: &ContentId) -> PathBuf {
        self.cache.join(content.as_str())
    }

    /// Onde fica o manifesto do conteúdo.
    pub fn metadata_for(&self, content: &ContentId) -> PathBuf {
        self.metadata.join(format!("{}.json", content.as_str()))
    }
}

/// Hash BLAKE3 completo de bytes que definem um conteúdo.
///
/// O valor é hexadecimal para poder virar parte de caminho sem escapamento. Não é título nem
/// ClassID: estes podem coincidir entre versões ou homebrews diferentes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentId(String);

impl ContentId {
    /// Calcula a identidade de um fluxo sem carregá-lo inteiro na memória.
    pub fn from_reader(mut reader: impl Read) -> std::io::Result<Self> {
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(Self(hasher.finalize().to_hex().to_string()))
    }

    /// Aceita somente um hash hexadecimal BLAKE3 completo vindo de metadata já validada.
    pub fn parse(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfil_nao_mistura_cache_save_e_aparelho() {
        let paths = StoragePaths::from_save_dir("/saves");
        assert_eq!(paths.root, PathBuf::from("/saves/zeebx"));
        assert_eq!(paths.cache, PathBuf::from("/saves/zeebx/cache"));
        assert_eq!(paths.device, PathBuf::from("/saves/zeebx/aparelho"));
        assert_eq!(paths.saves, PathBuf::from("/saves/zeebx/saves"));
        assert_eq!(paths.metadata, PathBuf::from("/saves/zeebx/metadata"));
    }

    #[test]
    fn raiz_de_frontend_liga_overlay_e_o_desktop_nao() {
        assert!(StoragePaths::from_save_dir("/saves").overlay);
        assert!(!StoragePaths::from_root("/config/zeebx").overlay);
    }

    #[test]
    fn hash_e_estavel_e_valido_para_caminho() {
        let one = ContentId::from_reader(&b"zeebo"[..]).unwrap();
        let two = ContentId::from_reader(&b"zeebo"[..]).unwrap();
        let other = ContentId::from_reader(&b"zeebo!"[..]).unwrap();
        assert_eq!(one, two);
        assert_ne!(one, other);
        assert_eq!(ContentId::parse(one.as_str()), Some(one));
    }
}
