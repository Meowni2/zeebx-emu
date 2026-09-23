//! Procura uma versão nova nas releases do GitHub.
//!
//! A pergunta é uma só — `releases/latest` — e a resposta chega numa thread, para a janela não
//! esperar a rede. A tag pode vir como `0.1.0` ou `v0.1.0`; rascunhos e pré-lançamentos o próprio
//! GitHub deixa de fora do `latest`.

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// Onde as releases são publicadas.
pub const REPOSITORIO: &str = "ZeebxTeam/zeebx-emu";

/// A versão deste binário.
pub const VERSAO_ATUAL: &str = env!("CARGO_PKG_VERSION");

/// Uma release mais nova que esta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lancamento {
    /// Sem o `v`: `0.2.0`.
    pub versao: String,
    /// A página da release, com as notas e os instaladores.
    pub pagina: String,
}

/// O que a procura respondeu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resposta {
    Nova(Lancamento),
    EmDia,
    Falhou(String),
}

/// Começa a procura numa thread. A resposta chega pelo canal, uma vez.
pub fn procura() -> Receiver<Resposta> {
    let (envio, recebe) = mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("atualizacao".into())
        .spawn(move || {
            let resposta = match consulta() {
                Ok(Some(lancamento)) => Resposta::Nova(lancamento),
                Ok(None) => Resposta::EmDia,
                Err(erro) => Resposta::Falhou(erro),
            };
            let _ = envio.send(resposta);
        });
    recebe
}

fn consulta() -> Result<Option<Lancamento>, String> {
    let url = format!("https://api.github.com/repos/{REPOSITORIO}/releases/latest");
    let agente = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        // Um 404 é resposta, não erro: é o que vem enquanto não há release nenhuma.
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut resposta = agente
        .get(&url)
        .header("User-Agent", &format!("zeebx/{VERSAO_ATUAL}"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|erro| erro.to_string())?;
    match resposta.status().as_u16() {
        200 => {}
        404 => return Ok(None),
        codigo => return Err(format!("o GitHub respondeu {codigo}")),
    }
    let texto = resposta
        .body_mut()
        .read_to_string()
        .map_err(|erro| erro.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&texto).map_err(|erro| erro.to_string())?;
    Ok(escolhe(&json, VERSAO_ATUAL))
}

/// A release do JSON, se ela for mais nova que `atual`.
fn escolhe(json: &serde_json::Value, atual: &str) -> Option<Lancamento> {
    if json["draft"].as_bool() == Some(true) || json["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let tag = json["tag_name"].as_str()?;
    let versao = sem_v(tag).to_string();
    if !mais_nova(&versao, atual) {
        return None;
    }
    let pagina = json["html_url"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://github.com/{REPOSITORIO}/releases/latest"));
    Some(Lancamento { versao, pagina })
}

fn sem_v(tag: &str) -> &str {
    let tag = tag.trim();
    tag.strip_prefix(['v', 'V']).unwrap_or(tag)
}

/// Os números de uma versão, `1.2.3` → `[1, 2, 3]`. O que vem depois de `-` ou `+` não conta, e
/// uma parte que não é número vale zero.
fn numeros(versao: &str) -> [u64; 3] {
    let nucleo = sem_v(versao).split(['-', '+']).next().unwrap_or("");
    let mut partes = nucleo
        .split('.')
        .map(|p| p.trim().parse::<u64>().unwrap_or(0));
    [
        partes.next().unwrap_or(0),
        partes.next().unwrap_or(0),
        partes.next().unwrap_or(0),
    ]
}

/// Se `candidata` é mais nova que `atual`.
pub fn mais_nova(candidata: &str, atual: &str) -> bool {
    numeros(candidata) > numeros(atual)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pergunta ao GitHub de verdade: `cargo test --release consulta_de_verdade -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore]
    fn consulta_de_verdade() {
        println!("{:?}", consulta());
    }

    #[test]
    fn compara_as_versoes_com_e_sem_v() {
        assert!(mais_nova("v0.2.0", "0.1.0"));
        assert!(mais_nova("0.1.1", "0.1.0"));
        assert!(mais_nova("1.0.0", "0.9.9"));
        assert!(mais_nova("0.10.0", "0.9.0"));
        assert!(!mais_nova("v0.1.0", "0.1.0"));
        assert!(!mais_nova("0.0.9", "0.1.0"));
    }

    #[test]
    fn so_aceita_release_publicada_e_mais_nova() {
        let json = |tag: &str, pre: bool| {
            serde_json::json!({
                "tag_name": tag,
                "html_url": format!("https://github.com/{REPOSITORIO}/releases/tag/{tag}"),
                "draft": false,
                "prerelease": pre,
            })
        };
        assert_eq!(
            escolhe(&json("v0.2.0", false), "0.1.0"),
            Some(Lancamento {
                versao: "0.2.0".into(),
                pagina: format!("https://github.com/{REPOSITORIO}/releases/tag/v0.2.0"),
            })
        );
        assert_eq!(escolhe(&json("0.1.0", false), "0.1.0"), None);
        assert_eq!(escolhe(&json("v0.3.0", true), "0.1.0"), None);
    }
}
