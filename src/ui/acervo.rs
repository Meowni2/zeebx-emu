//! O que a Z-Wheel sabe dos jogos: a caixa, o nome oficial e a descrição de cada um.
//!
//! A Z-Wheel não tira a capa do jogo — ela traz as capas no próprio pacote, em
//! `mod/<id>/assets/games/<game_id>/`, e liga cada pasta ao jogo pelo banco `tt_game_info`
//! (SQLite): a tabela `GAMEINFO` leva `game_id` ao `class_id` do applet e ao caminho da capa, e
//! `TITLETEXT` guarda o nome em cada idioma. O `class_id` é o mesmo ClassID que a biblioteca usa
//! como chave, então a ligação não depende de nome de arquivo nem de pasta.
//!
//! O pacote pode estar solto numa pasta ou num `.zip`; os dois são lidos sem extrair nada.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::video::icon::{self, Image};

/// O banco da Z-Wheel, ao lado do `tectoy.mod`.
const BANCO: &str = "tt_game_info";

/// O que a Z-Wheel guarda de um jogo.
#[derive(Debug, Clone, Default)]
pub struct Ficha {
    /// Por idioma, na chave de duas letras (`pt`, `en`, `es`).
    titulos: HashMap<String, String>,
    descricoes: HashMap<String, String>,
    pub capa: Option<Image>,
}

impl Ficha {
    pub fn titulo(&self, idioma: &str) -> Option<&str> {
        escolhe(&self.titulos, idioma)
    }

    pub fn descricao(&self, idioma: &str) -> Option<&str> {
        escolhe(&self.descricoes, idioma)
    }
}

/// O idioma pedido, e na falta dele o inglês: é o que todas as fichas trazem.
fn escolhe<'a>(mapa: &'a HashMap<String, String>, idioma: &str) -> Option<&'a str> {
    let curto = idioma.get(..2).unwrap_or(idioma).to_ascii_lowercase();
    mapa.get(&curto)
        .or_else(|| mapa.get("en"))
        .or_else(|| mapa.values().next())
        .map(String::as_str)
}

/// As fichas de todos os jogos que a Z-Wheel conhece, pelo ClassID.
#[derive(Debug, Default)]
pub struct Acervo {
    fichas: HashMap<u32, Ficha>,
}

impl Acervo {
    /// Lê o acervo do pacote da Z-Wheel em `caminho` — o `.zip`, a pasta ou o `.mod`.
    pub fn carrega(caminho: &Path) -> Option<Self> {
        let mut fonte = Fonte::abre(caminho)?;
        let banco = fonte.le(BANCO)?;
        let linhas = le_banco(&banco)
            .inspect_err(|err| eprintln!("acervo da Z-Wheel: {err}"))
            .ok()?;
        let mut fichas = HashMap::new();
        for (class_id, pasta, titulos) in linhas {
            let pasta = pasta.trim_start_matches("./").trim_end_matches('/');
            let mut descricoes = HashMap::new();
            for idioma in ["pt", "en", "es"] {
                if let Some(texto) = fonte.le(&format!("{pasta}/description_{idioma}.txt")) {
                    descricoes.insert(idioma.to_string(), limpa_descricao(&texto));
                }
            }
            // A grande primeiro: a pequena é a mesma arte em 160×227 com a borda do console.
            let capa = ["boxartlg.jpg", "boxart.bmp"]
                .iter()
                .filter_map(|nome| fonte.le(&format!("{pasta}/{nome}")))
                .find_map(|dados| icon::decode(&dados).ok());
            fichas.insert(
                class_id,
                Ficha {
                    titulos,
                    descricoes,
                    capa,
                },
            );
        }
        Some(Self { fichas })
    }

    pub fn ficha(&self, class_id: u32) -> Option<&Ficha> {
        self.fichas.get(&class_id)
    }

    pub fn len(&self) -> usize {
        self.fichas.len()
    }
}

/// `(class_id, pasta da capa, títulos por idioma)` de cada jogo do banco.
fn le_banco(dados: &[u8]) -> rusqlite::Result<Vec<(u32, String, HashMap<String, String>)>> {
    // O SQLite só abre arquivo. Copiar para o cache serve às duas origens igual, e a Z-Wheel
    // que estiver rodando nunca tem o banco dela aberto por nós.
    let copia = crate::loader::archive::cache_dir().join("z-wheel-tt_game_info.db");
    if let Some(pasta) = copia.parent() {
        let _ = std::fs::create_dir_all(pasta);
    }
    std::fs::write(&copia, dados)
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(err.into()))?;
    let conexao =
        rusqlite::Connection::open_with_flags(&copia, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut titulos: HashMap<i64, HashMap<String, String>> = HashMap::new();
    let mut consulta = conexao.prepare("SELECT game_id, lang_id, titletext FROM TITLETEXT")?;
    let linhas = consulta.query_map([], |linha| {
        Ok((
            linha.get::<_, i64>(0)?,
            linha.get::<_, i64>(1)?,
            linha.get::<_, String>(2)?,
        ))
    })?;
    for linha in linhas {
        let (jogo, idioma, titulo) = linha?;
        titulos
            .entry(jogo)
            .or_default()
            .insert(idioma_do_brew(idioma), titulo);
    }
    let mut consulta = conexao.prepare("SELECT game_id, class_id, boxart_path FROM GAMEINFO")?;
    let linhas = consulta.query_map([], |linha| {
        Ok((
            linha.get::<_, i64>(0)?,
            linha.get::<_, i64>(1)?,
            linha.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut jogos = Vec::new();
    for linha in linhas {
        let (jogo, class_id, pasta) = linha?;
        let Some(pasta) = pasta else { continue };
        jogos.push((
            class_id as u32,
            pasta,
            titulos.remove(&jogo).unwrap_or_default(),
        ));
    }
    Ok(jogos)
}

/// O idioma do BREW é um inteiro com as letras em little-endian: `"pt  "` é `0x20207470`.
fn idioma_do_brew(codigo: i64) -> String {
    let letras = (codigo as u32).to_le_bytes();
    String::from_utf8_lossy(&letras).trim().to_ascii_lowercase()
}

/// A descrição sem o aviso da loja que fechou em 2011, que abre quase todas entre `**`.
fn limpa_descricao(dados: &[u8]) -> String {
    let texto = String::from_utf8_lossy(dados);
    let texto = texto.trim_start_matches('\u{feff}').trim();
    let sem_aviso = match texto
        .strip_prefix("**")
        .and_then(|resto| resto.split_once("**"))
    {
        Some((_, depois)) => depois.trim_start(),
        None => texto,
    };
    sem_aviso.trim().to_string()
}

/// De onde vêm os arquivos do pacote: uma pasta, ou um zip com o prefixo até a pasta do módulo.
enum Fonte {
    Pasta(PathBuf),
    Zip(zip::ZipArchive<std::fs::File>, String),
}

impl Fonte {
    fn abre(caminho: &Path) -> Option<Self> {
        let e_zip = caminho
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        if e_zip {
            let arquivo = zip::ZipArchive::new(std::fs::File::open(caminho).ok()?).ok()?;
            let prefixo = arquivo
                .file_names()
                .find_map(|nome| nome.strip_suffix(BANCO).map(str::to_string))
                .filter(|prefixo| prefixo.is_empty() || prefixo.ends_with('/'))?;
            return Some(Self::Zip(arquivo, prefixo));
        }
        let pasta = if caminho.is_dir() {
            caminho.to_path_buf()
        } else {
            caminho.parent()?.to_path_buf()
        };
        acha_banco(&pasta, 3).map(Self::Pasta)
    }

    fn le(&mut self, relativo: &str) -> Option<Vec<u8>> {
        match self {
            Self::Pasta(pasta) => std::fs::read(pasta.join(relativo)).ok(),
            Self::Zip(arquivo, prefixo) => {
                let mut entrada = arquivo.by_name(&format!("{prefixo}{relativo}")).ok()?;
                let mut dados = Vec::new();
                entrada.read_to_end(&mut dados).ok()?;
                Some(dados)
            }
        }
    }
}

/// A pasta que guarda o banco, descendo de `pasta` até `profundidade` níveis: a raiz do pacote
/// tem o banco em `mod/<id>/`.
fn acha_banco(pasta: &Path, profundidade: usize) -> Option<PathBuf> {
    if pasta.join(BANCO).is_file() {
        return Some(pasta.to_path_buf());
    }
    if profundidade == 0 {
        return None;
    }
    let mut filhas: Vec<PathBuf> = std::fs::read_dir(pasta)
        .ok()?
        .flatten()
        .map(|entrada| entrada.path())
        .filter(|caminho| caminho.is_dir())
        .collect();
    filhas.sort();
    filhas
        .into_iter()
        .find_map(|filha| acha_banco(&filha, profundidade - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_idioma_do_brew_sai_das_letras_em_little_endian() {
        assert_eq!(idioma_do_brew(0x2020_7470), "pt");
        assert_eq!(idioma_do_brew(538996325), "en");
    }

    #[test]
    fn a_descricao_perde_o_aviso_da_loja() {
        let texto = "** Título disponível até 2011. **\nEste incrível jogo de xadrez.".as_bytes();
        assert_eq!(limpa_descricao(texto), "Este incrível jogo de xadrez.");
        assert_eq!(limpa_descricao(b"Sem aviso."), "Sem aviso.");
    }

    #[test]
    fn sem_o_idioma_pedido_vale_o_ingles() {
        let mapa = HashMap::from([("en".to_string(), "Alien Breaker".to_string())]);
        assert_eq!(escolhe(&mapa, "pt-BR"), Some("Alien Breaker"));
    }
}

/// Com o pacote real: `ZEEBX_Z_WHEEL=<zip ou pasta> cargo test --release acervo -- --ignored`.
#[cfg(test)]
mod pacote_real {
    use super::*;

    #[test]
    #[ignore]
    fn o_acervo_do_pacote_real_traz_capas_e_nomes() {
        let caminho = PathBuf::from(std::env::var("ZEEBX_Z_WHEEL").expect("ZEEBX_Z_WHEEL"));
        let acervo = Acervo::carrega(&caminho).expect("acervo");
        let com_capa = acervo.fichas.values().filter(|f| f.capa.is_some()).count();
        let alien = acervo
            .fichas
            .values()
            .find(|f| f.titulo("pt") == Some("Alien Breaker"));
        println!("{} fichas, {com_capa} com capa", acervo.len());
        assert!(acervo.len() > 0 && com_capa == acervo.len() && alien.is_some());
    }
}
