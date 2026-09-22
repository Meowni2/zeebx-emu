//! O **formato** do save state: versionado, com seções nomeadas e conferência de integridade.
//!
//! Existe separado do estado de propósito. O `retro_serialize_size` do core responde **zero**
//! enquanto o estado não estiver inteiro dentro dele — zero é como o frontend entende "este core
//! não salva", e é a única resposta honesta enquanto houver campo de fora. O que este módulo
//! resolve primeiro é a parte que o objetivo nomeia: **formato versionado**.
//!
//! ## Por que seções nomeadas
//!
//! O estado de uma máquina tem cento e noventa campos, e boa parte deles são tabelas de objetos do
//! guest. Um bloco opaco de bytes obrigaria a salvar e a carregar na mesma ordem para sempre: mudar
//! o campo de lugar viraria save state incompatível **em silêncio**. Com nome, cada pedaço se
//! acha, e uma seção que o leitor não conhece é **pulada** — é o que permite ler um estado gravado
//! por uma versão mais nova sem adivinhar.
//!
//! ## Integridade
//!
//! O `crc32` do conteúdo vai no cabeçalho. Sem ele, um arquivo truncado por falta de espaço em
//! disco carregaria como estado válido, e o jogo quebraria em algum lugar sem relação com o
//! problema. Com ele, a recusa acontece na porta.
//!
//! ## Layout
//!
//! ```text
//! 0..4    assinatura "ZBXS"
//! 4..6    versão do formato, u16 (little-endian)
//! 6..10   tamanho do conteúdo, u32
//! 10..14  crc32 do conteúdo, u32
//! 14..    conteúdo: seções, uma após a outra
//! ```
//!
//! Cada seção é `u8` com o tamanho do nome, o nome, `u32` com o tamanho dos dados, e os dados.

use std::collections::BTreeMap;
use std::ops::Range;

/// Assinatura do arquivo. Quatro bytes que dizem "isto é um save state do Zeebx".
pub const ASSINATURA: [u8; 4] = *b"ZBXS";

/// Versão do formato. Sobe quando o **significado** das seções muda, não quando uma seção nova
/// aparece: seção desconhecida é pulada pelo leitor, e é assim que um estado antigo continua
/// legível depois de o motor ganhar um campo.
pub const VERSAO: u16 = 1;

/// O tamanho do cabeçalho, em bytes.
const CABECALHO: usize = 14;

/// O que deu errado ao abrir um estado.
///
/// Cada variante diz **o que** estava errado e **com o que** se deparou. "Save state inválido"
/// sozinho obriga quem lê a investigar o arquivo a partir do zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Erro {
    /// O arquivo não começa com [`ASSINATURA`].
    Assinatura([u8; 4]),
    /// O arquivo tem uma versão que este motor não sabe ler.
    Versao { encontrada: u16, suportada: u16 },
    /// O arquivo acaba antes do que o cabeçalho promete.
    Truncado { esperado: usize, encontrado: usize },
    /// O conteúdo não bate com o `crc32` do cabeçalho.
    Integridade { esperado: u32, encontrado: u32 },
    /// Uma seção está malformada por dentro.
    Secao { nome: String, motivo: String },
}

impl std::fmt::Display for Erro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Erro::Assinatura(bytes) => write!(
                f,
                "não é um save state do Zeebx: os quatro primeiros bytes são {bytes:02x?}, \
                 e deviam ser {ASSINATURA:02x?}"
            ),
            Erro::Versao {
                encontrada,
                suportada,
            } => write!(
                f,
                "save state da versão {encontrada}, e este motor lê até a {suportada}"
            ),
            Erro::Truncado {
                esperado,
                encontrado,
            } => write!(
                f,
                "save state cortado: o cabeçalho promete {esperado} bytes e o arquivo tem {encontrado}"
            ),
            Erro::Integridade {
                esperado,
                encontrado,
            } => write!(
                f,
                "save state corrompido: crc32 {encontrado:08x} onde o cabeçalho diz {esperado:08x}"
            ),
            Erro::Secao { nome, motivo } => {
                write!(f, "seção \"{nome}\" malformada: {motivo}")
            }
        }
    }
}

impl std::error::Error for Erro {}

/// Grava as seções no formato. A ordem em que entram é a ordem em que saem no arquivo.
pub fn escreve(secoes: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut conteudo = Vec::new();
    for (nome, dados) in secoes {
        let nome = nome.as_bytes();
        // O nome cabe num byte: são nomes de seção, não caminhos.
        let tamanho_do_nome = u8::try_from(nome.len().min(u8::MAX as usize)).unwrap_or(u8::MAX);
        conteudo.push(tamanho_do_nome);
        conteudo.extend_from_slice(&nome[..tamanho_do_nome as usize]);
        conteudo.extend_from_slice(&(dados.len() as u32).to_le_bytes());
        conteudo.extend_from_slice(dados);
    }

    let mut saida = Vec::with_capacity(CABECALHO + conteudo.len());
    saida.extend_from_slice(&ASSINATURA);
    saida.extend_from_slice(&VERSAO.to_le_bytes());
    saida.extend_from_slice(&(conteudo.len() as u32).to_le_bytes());
    saida.extend_from_slice(&crc32(&conteudo).to_le_bytes());
    saida.extend_from_slice(&conteudo);
    saida
}

/// O `crc32` que o cabeçalho guarda.
fn crc32(dados: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(dados);
    crc.sum()
}

/// Um estado aberto, pronto para ter as seções lidas.
#[derive(Debug)]
pub struct Leitor<'a> {
    dados: &'a [u8],
    secoes: BTreeMap<String, Range<usize>>,
}

impl<'a> Leitor<'a> {
    /// Confere o cabeçalho e indexa as seções.
    ///
    /// **Não** interpreta nenhuma seção: quem sabe o que cada uma significa é o dono do estado.
    pub fn abre(arquivo: &'a [u8]) -> Result<Self, Erro> {
        if arquivo.len() < CABECALHO {
            return Err(Erro::Truncado {
                esperado: CABECALHO,
                encontrado: arquivo.len(),
            });
        }
        let assinatura = [arquivo[0], arquivo[1], arquivo[2], arquivo[3]];
        if assinatura != ASSINATURA {
            return Err(Erro::Assinatura(assinatura));
        }
        let versao = u16::from_le_bytes([arquivo[4], arquivo[5]]);
        if versao > VERSAO {
            return Err(Erro::Versao {
                encontrada: versao,
                suportada: VERSAO,
            });
        }
        let prometido = u32::from_le_bytes([arquivo[6], arquivo[7], arquivo[8], arquivo[9]]) as usize;
        let esperado = u32::from_le_bytes([arquivo[10], arquivo[11], arquivo[12], arquivo[13]]);
        let disponivel = arquivo.len() - CABECALHO;
        if disponivel < prometido {
            return Err(Erro::Truncado {
                esperado: CABECALHO + prometido,
                encontrado: arquivo.len(),
            });
        }
        let conteudo = &arquivo[CABECALHO..CABECALHO + prometido];
        let encontrado = crc32(conteudo);
        if encontrado != esperado {
            return Err(Erro::Integridade {
                esperado,
                encontrado,
            });
        }

        let mut secoes = BTreeMap::new();
        let mut posicao = 0usize;
        while posicao < conteudo.len() {
            let tamanho_do_nome = conteudo[posicao] as usize;
            posicao += 1;
            if posicao + tamanho_do_nome > conteudo.len() {
                return Err(Erro::Secao {
                    nome: "?".to_string(),
                    motivo: "o cabeçalho da seção passa do fim do arquivo".to_string(),
                });
            }
            let nome = String::from_utf8_lossy(&conteudo[posicao..posicao + tamanho_do_nome])
                .into_owned();
            posicao += tamanho_do_nome;
            if posicao + 4 > conteudo.len() {
                return Err(Erro::Secao {
                    nome,
                    motivo: "falta o tamanho dos dados".to_string(),
                });
            }
            let tamanho = u32::from_le_bytes([
                conteudo[posicao],
                conteudo[posicao + 1],
                conteudo[posicao + 2],
                conteudo[posicao + 3],
            ]) as usize;
            posicao += 4;
            if posicao + tamanho > conteudo.len() {
                return Err(Erro::Secao {
                    nome,
                    motivo: format!(
                        "diz ter {tamanho} bytes e restam {}",
                        conteudo.len() - posicao
                    ),
                });
            }
            secoes.insert(nome, posicao..posicao + tamanho);
            posicao += tamanho;
        }
        Ok(Self {
            dados: conteudo,
            secoes,
        })
    }

    /// Os dados de uma seção, ou `None` se o arquivo não a tem.
    pub fn secao(&self, nome: &str) -> Option<&'a [u8]> {
        self.secoes.get(nome).map(|faixa| &self.dados[faixa.clone()])
    }

    /// Os nomes das seções, em ordem.
    pub fn nomes(&self) -> Vec<&str> {
        self.secoes.keys().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secoes() -> Vec<(String, Vec<u8>)> {
        vec![
            ("cpu".to_string(), vec![1, 2, 3, 4]),
            ("heap".to_string(), vec![9; 8]),
        ]
    }

    #[test]
    fn a_ida_e_a_volta_devolvem_as_secoes() {
        let arquivo = escreve(&secoes());
        let leitor = Leitor::abre(&arquivo).expect("o estado abriu");
        assert_eq!(leitor.nomes(), vec!["cpu", "heap"]);
        assert_eq!(leitor.secao("cpu"), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(leitor.secao("heap"), Some(&[9u8; 8][..]));
        assert_eq!(leitor.secao("inexistente"), None);
    }

    #[test]
    fn uma_secao_desconhecida_e_pulada_sem_quebrar() {
        let mut todas = secoes();
        todas.push(("secao-do-futuro".to_string(), vec![7; 5]));
        let arquivo = escreve(&todas);
        let leitor = Leitor::abre(&arquivo).expect("o estado abriu");
        // As conhecidas continuam legíveis: é isto que deixa um motor velho ler um estado novo.
        assert_eq!(leitor.secao("cpu"), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(leitor.secao("secao-do-futuro"), Some(&[7u8; 5][..]));
    }

    #[test]
    fn um_byte_trocado_no_conteudo_e_recusado() {
        let mut arquivo = escreve(&secoes());
        let ultimo = arquivo.len() - 1;
        arquivo[ultimo] ^= 0xff;
        match Leitor::abre(&arquivo) {
            Err(Erro::Integridade { .. }) => {}
            outro => panic!("devia recusar por integridade, e devolveu {outro:?}"),
        }
    }

    #[test]
    fn uma_versao_do_futuro_e_recusada_dizendo_qual() {
        let mut arquivo = escreve(&secoes());
        arquivo[4..6].copy_from_slice(&(VERSAO + 3).to_le_bytes());
        match Leitor::abre(&arquivo) {
            Err(Erro::Versao {
                encontrada,
                suportada,
            }) => {
                assert_eq!(encontrada, VERSAO + 3);
                assert_eq!(suportada, VERSAO);
            }
            outro => panic!("devia recusar pela versão, e devolveu {outro:?}"),
        }
    }

    #[test]
    fn arquivo_de_outra_coisa_e_recusado() {
        match Leitor::abre(b"NOPE nao sou um save state") {
            Err(Erro::Assinatura(bytes)) => assert_eq!(&bytes, b"NOPE"),
            outro => panic!("devia recusar pela assinatura, e devolveu {outro:?}"),
        }
    }

    #[test]
    fn um_estado_cortado_e_recusado_com_os_dois_numeros() {
        let arquivo = escreve(&secoes());
        let cortado = &arquivo[..arquivo.len() - 3];
        match Leitor::abre(cortado) {
            Err(Erro::Truncado {
                esperado,
                encontrado,
            }) => {
                assert_eq!(esperado, arquivo.len());
                assert_eq!(encontrado, cortado.len());
            }
            outro => panic!("devia recusar por corte, e devolveu {outro:?}"),
        }
    }

    #[test]
    fn um_arquivo_vazio_e_recusado_sem_panico() {
        assert!(matches!(
            Leitor::abre(&[]),
            Err(Erro::Truncado {
                esperado: CABECALHO,
                encontrado: 0
            })
        ));
    }

    /// A mensagem de erro diz o que houve e com o que se deparou. Sem isto, quem recebe "save
    /// state inválido" começa a investigação do zero.
    #[test]
    fn as_mensagens_dizem_o_que_houve() {
        let texto = Erro::Versao {
            encontrada: 7,
            suportada: VERSAO,
        }
        .to_string();
        assert!(texto.contains('7'), "{texto}");
        assert!(texto.contains(&VERSAO.to_string()), "{texto}");
        let texto = Erro::Integridade {
            esperado: 0xdead_beef,
            encontrado: 0x1234_5678,
        }
        .to_string();
        assert!(texto.contains("deadbeef"), "{texto}");
        assert!(texto.contains("12345678"), "{texto}");
    }
}
