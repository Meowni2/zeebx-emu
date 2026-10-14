//! Testes com servidor simulado: o JSON é o que o servidor do RA devolve, copiado dos testes do
//! próprio rcheevos (`test/client/test_rc_client.c`), com o console trocado para o Zeebo.

use super::*;
use std::collections::HashMap;

/// O que o servidor simulado viu e o que ele responde, por valor de `r=`.
#[derive(Default)]
struct Roteiro {
    respostas: HashMap<&'static str, String>,
    pedidos: Vec<Pedido>,
    /// Quando ligado, os pedidos são descartados sem resposta.
    mudo: bool,
}

struct ServidorSimulado(Rc<RefCell<Roteiro>>);

impl Servidor for ServidorSimulado {
    fn pede(&mut self, pedido: Pedido, entrega: Entrega) {
        let mut roteiro = self.0.borrow_mut();
        let r = parametro(&pedido, "r").unwrap_or_default();
        roteiro.pedidos.push(pedido);
        if roteiro.mudo {
            return;
        }
        let corpo = roteiro
            .respostas
            .get(r.as_str())
            .cloned()
            .unwrap_or_else(|| {
                format!("{{\"Success\":false,\"Error\":\"sem resposta simulada para {r}\"}}")
            });
        entrega.responde(200, corpo.into_bytes());
    }
}

/// O valor de um parâmetro do pedido, no corpo do POST ou na URL.
fn parametro(pedido: &Pedido, nome: &str) -> Option<String> {
    let consulta = pedido
        .corpo
        .clone()
        .or_else(|| pedido.url.split_once('?').map(|(_, q)| q.to_string()))?;
    consulta.split('&').find_map(|par| {
        let (chave, valor) = par.split_once('=')?;
        (chave == nome).then(|| valor.to_string())
    })
}

/// Memória de mentira: `bytes` a partir de `base`.
struct MemoriaFalsa {
    base: u32,
    bytes: Vec<u8>,
}

impl Memoria for MemoriaFalsa {
    fn le(&self, endereco: u32, destino: &mut [u8]) -> bool {
        let Some(inicio) = endereco.checked_sub(self.base) else {
            return false;
        };
        let inicio = inicio as usize;
        match self.bytes.get(inicio..inicio + destino.len()) {
            Some(origem) => {
                destino.copy_from_slice(origem);
                true
            }
            None => false,
        }
    }
}

const LOGIN: &str = "{\"Success\":true,\"User\":\"User\",\"Token\":\"ApiToken\",\"Score\":12345,\
    \"SoftcoreScore\":123,\"Messages\":2,\"Permissions\":1,\"AccountType\":\"Registered\"}";

/// Duas conquistas: a 5501 quer o byte `0x1001` igual a 3 e o `0x1002` igual a 7; a 5502 lê
/// `0x9000`, fora da memória de mentira.
const CONJUNTO: &str = "{\"Success\":true,\
    \"GameId\":1234,\"Title\":\"Jogo de Teste\",\"ConsoleId\":70,\
    \"ImageIconUrl\":\"http://server/Images/112233.png\",\
    \"RichPresenceGameId\":1234,\"RichPresencePatch\":\"\",\"Sets\":[{\
      \"AchievementSetId\":1111,\"GameId\":1234,\"Title\":null,\"Type\":\"core\",\
      \"ImageIconUrl\":\"http://server/Images/112233.png\",\
      \"Achievements\":[\
       {\"ID\":5501,\"Title\":\"Primeira\",\"Description\":\"Desc1\",\"Flags\":3,\"Points\":5,\
        \"MemAddr\":\"0xH1001=3_0xH1002=7\",\"Author\":\"User1\",\"BadgeName\":\"00234\",\
        \"Created\":1367266583,\"Modified\":1376929305},\
       {\"ID\":5502,\"Title\":\"Fora do mapa\",\"Description\":\"Desc2\",\"Flags\":3,\"Points\":2,\
        \"MemAddr\":\"0xH9000=1\",\"Author\":\"User1\",\"BadgeName\":\"00235\",\
        \"Created\":1376970283,\"Modified\":1376970283}\
      ],\
      \"Leaderboards\":[]\
    }]}";

const SEM_DESBLOQUEIOS: &str = "{\"Success\":true,\"Unlocks\":[],\"HardcoreUnlocks\":[]}";

const DESCONHECIDO: &str =
    "{\"Success\":false,\"Error\":\"Unknown game\",\"Code\":\"not_found\",\"Status\":404}";

const DESBLOQUEIO: &str = "{\"Success\":true,\"Score\":5432,\"SoftcoreScore\":777,\
    \"AchievementID\":5501,\"AchievementsRemaining\":1}";

fn cliente(respostas: &[(&'static str, &str)]) -> (Conquistas, Rc<RefCell<Roteiro>>) {
    let roteiro = Rc::new(RefCell::new(Roteiro {
        respostas: respostas
            .iter()
            .map(|(r, corpo)| (*r, corpo.to_string()))
            .collect(),
        ..Default::default()
    }));
    let conquistas = Conquistas::novo(Box::new(ServidorSimulado(Rc::clone(&roteiro))));
    (conquistas, roteiro)
}

/// Um cliente já conectado, com o servidor pronto para carregar o jogo de teste. Para os testes
/// da sessão.
pub(crate) fn cliente_conectado_com_o_jogo_de_teste() -> Compartilhado {
    let (mut c, _) = cliente(&[
        ("login2", LOGIN),
        ("achievementsets", CONJUNTO),
        ("startsession", SEM_DESBLOQUEIOS),
        ("awardachievement", DESBLOQUEIO),
    ]);
    c.entra_com_senha("User", "Pa$$word");
    assert_eq!(c.eventos(), vec![Evento::Entrou(Ok(()))]);
    Rc::new(RefCell::new(c))
}

/// Um cliente conectado cujo jogo tem uma conquista só, a 7001, com a condição `condicao` na
/// sintaxe do RA. Para testar com um jogo de verdade.
pub(crate) fn cliente_conectado_com_a_condicao(condicao: &str) -> Compartilhado {
    let conjunto = CONJUNTO
        .replace("0xH1001=3_0xH1002=7", condicao)
        .replace("\"ID\":5501", "\"ID\":7001");
    let roteiro = Rc::new(RefCell::new(Roteiro::default()));
    {
        let mut r = roteiro.borrow_mut();
        r.respostas.insert("login2", LOGIN.into());
        r.respostas.insert("achievementsets", conjunto);
        r.respostas.insert("startsession", SEM_DESBLOQUEIOS.into());
        r.respostas.insert("awardachievement", DESBLOQUEIO.into());
    }
    let mut c = Conquistas::novo(Box::new(ServidorSimulado(roteiro)));
    c.entra_com_senha("User", "Pa$$word");
    assert_eq!(c.eventos(), vec![Evento::Entrou(Ok(()))]);
    Rc::new(RefCell::new(c))
}

fn memoria() -> MemoriaFalsa {
    MemoriaFalsa {
        base: 0x1000,
        bytes: vec![0; 0x100],
    }
}

/// Entra, carrega o jogo de teste e roda o primeiro quadro, que é quando o jogo é ativado.
fn com_jogo_carregado(memoria: &MemoriaFalsa) -> (Conquistas, Rc<RefCell<Roteiro>>) {
    let (mut c, roteiro) = cliente(&[
        ("login2", LOGIN),
        ("achievementsets", CONJUNTO),
        ("startsession", SEM_DESBLOQUEIOS),
        ("awardachievement", DESBLOQUEIO),
    ]);
    c.entra_com_senha("User", "Pa$$word");
    c.carrega_jogo("0123456789abcdef0123456789abcdef");
    c.quadro(memoria);
    assert_eq!(
        c.eventos(),
        vec![Evento::Entrou(Ok(())), Evento::JogoCarregado(Ok(()))]
    );
    (c, roteiro)
}

#[test]
fn o_mapa_provisorio_e_a_identidade() {
    let mapa = Mapa::provisorio();
    assert_eq!(mapa.traduz(0x1000_0040, 4), Some((0x1000_0040, 4)));
    assert_eq!(mapa.traduz(u32::MAX, 4), Some((u32::MAX, 1)));
}

#[test]
fn o_mapa_corta_a_leitura_no_fim_da_faixa_e_recusa_o_buraco() {
    let mapa = Mapa::de(vec![
        Faixa {
            ra: 0x10_0000,
            guest: 0x1000_0000,
            tamanho: 0x100,
        },
        Faixa {
            ra: 0,
            guest: 0x0001_0000,
            tamanho: 0x1000,
        },
    ]);
    assert_eq!(mapa.traduz(0x0ffe, 4), Some((0x0001_0ffe, 2)));
    assert_eq!(mapa.traduz(0x10_0010, 4), Some((0x1000_0010, 4)));
    assert_eq!(mapa.traduz(0x2000, 1), None);
}

/// O formato que o RA cobra: nome e versão do emulador, o sistema, e a versão do rcheevos.
#[test]
fn o_user_agent_diz_o_zeebx_o_sistema_e_o_rcheevos() {
    let (c, _) = cliente(&[]);
    let ua = c.user_agent();
    let esperado = format!("Zeebx/{} (", env!("CARGO_PKG_VERSION"));
    assert!(ua.starts_with(&esperado), "{ua}");
    assert!(ua.contains(") rcheevos/"), "{ua}");
}

#[test]
fn o_cliente_nasce_em_casual() {
    let (c, _) = cliente(&[]);
    assert!(!c.hardcore());
}

#[test]
fn entrar_com_senha_guarda_o_token_e_nao_a_senha() {
    let (mut c, roteiro) = cliente(&[("login2", LOGIN)]);
    c.entra_com_senha("User", "Pa$$word");
    assert_eq!(c.eventos(), vec![Evento::Entrou(Ok(()))]);
    let usuario = c.usuario().expect("conectado");
    assert_eq!(usuario.nome, "User");
    assert_eq!(usuario.token, "ApiToken");
    assert_eq!(usuario.pontos, 12345);
    assert_eq!(usuario.pontos_casual, 123);

    let pedido = &roteiro.borrow().pedidos[0];
    assert_eq!(parametro(pedido, "u").as_deref(), Some("User"));
    assert_eq!(parametro(pedido, "p").as_deref(), Some("Pa%24%24word"));
}

#[test]
fn a_senha_errada_chega_como_erro_com_o_texto_do_servidor() {
    let (mut c, _) = cliente(&[(
        "login2",
        "{\"Success\":false,\"Error\":\"Invalid User/Password combination.\",\
         \"Code\":\"invalid_credentials\",\"Status\":401}",
    )]);
    c.entra_com_senha("User", "errada");
    let eventos = c.eventos();
    assert!(
        matches!(&eventos[..], [Evento::Entrou(Err(texto))] if texto.contains("Invalid User/Password")),
        "{eventos:?}"
    );
    assert!(c.usuario().is_none());
}

#[test]
fn entrar_com_token_manda_o_token() {
    let (mut c, roteiro) = cliente(&[("login2", LOGIN)]);
    c.entra_com_token("User", "ApiToken");
    assert_eq!(c.eventos(), vec![Evento::Entrou(Ok(()))]);
    let pedido = &roteiro.borrow().pedidos[0];
    assert_eq!(parametro(pedido, "t").as_deref(), Some("ApiToken"));
    assert_eq!(parametro(pedido, "p"), None);
}

/// Um servidor que perde o pedido não pode deixar o login pendurado para sempre.
#[test]
fn um_pedido_descartado_sem_resposta_vira_falha() {
    let (mut c, roteiro) = cliente(&[]);
    roteiro.borrow_mut().mudo = true;
    c.entra_com_senha("User", "Pa$$word");
    let eventos = c.eventos();
    assert!(
        matches!(&eventos[..], [Evento::Entrou(Err(_))]),
        "{eventos:?}"
    );
}

#[test]
fn o_jogo_carregado_diz_o_titulo_e_o_hash() {
    let memoria = memoria();
    let (c, roteiro) = com_jogo_carregado(&memoria);
    let jogo = c.jogo().expect("carregado");
    assert_eq!(jogo.id, 1234);
    assert_eq!(jogo.titulo, "Jogo de Teste");
    assert_eq!(jogo.hash, "0123456789abcdef0123456789abcdef");

    let roteiro = roteiro.borrow();
    let sessao = roteiro
        .pedidos
        .iter()
        .find(|p| parametro(p, "r").as_deref() == Some("startsession"))
        .expect("a sessão foi aberta");
    assert_eq!(parametro(sessao, "h").as_deref(), Some("0"), "casual");
}

/// A conquista só dispara depois de ter sido falsa num quadro (o rcheevos a "arma"), e o
/// desbloqueio vai ao servidor marcado como casual.
#[test]
fn a_conquista_dispara_pela_memoria_e_o_desbloqueio_e_casual() {
    let mut memoria = memoria();
    let (mut c, roteiro) = com_jogo_carregado(&memoria);

    c.quadro(&memoria);
    assert_eq!(c.eventos(), vec![]);

    memoria.bytes[1] = 3;
    memoria.bytes[2] = 7;
    c.quadro(&memoria);
    let eventos = c.eventos();
    let [Evento::Desbloqueou(conquista)] = &eventos[..] else {
        panic!("{eventos:?}");
    };
    assert_eq!(conquista.id, 5501);
    assert_eq!(conquista.titulo, "Primeira");
    assert_eq!(conquista.pontos, 5);

    let roteiro = roteiro.borrow();
    let premio = roteiro
        .pedidos
        .iter()
        .find(|p| parametro(p, "r").as_deref() == Some("awardachievement"))
        .expect("o desbloqueio foi enviado");
    assert_eq!(parametro(premio, "a").as_deref(), Some("5501"));
    assert_eq!(parametro(premio, "h").as_deref(), Some("0"));
}

/// Um endereço que a memória não tem desliga a conquista em vez de lê-la como zero. É a única
/// validação enquanto o Zeebo não tem mapa no `consoleinfo.c`.
#[test]
fn a_conquista_sobre_endereco_que_nao_se_le_nunca_dispara() {
    let memoria = memoria();
    let (mut c, roteiro) = com_jogo_carregado(&memoria);
    for _ in 0..10 {
        c.quadro(&memoria);
    }
    assert_eq!(c.eventos(), vec![]);
    assert!(
        !roteiro
            .borrow()
            .pedidos
            .iter()
            .any(|p| parametro(p, "a").as_deref() == Some("5502"))
    );
}

#[test]
fn o_hash_que_o_ra_nao_conhece_carrega_sem_conquistas() {
    let (mut c, _) = cliente(&[("login2", LOGIN), ("achievementsets", DESCONHECIDO)]);
    c.entra_com_senha("User", "Pa$$word");
    c.carrega_jogo("ffffffffffffffffffffffffffffffff");
    assert_eq!(
        c.eventos(),
        vec![
            Evento::Entrou(Ok(())),
            Evento::JogoCarregado(Err(ErroAoCarregar::Desconhecido))
        ]
    );
    assert!(c.jogo().is_none());
}

/// Sem memória, o carregamento espera: validar os endereços sem memória desligaria todas as
/// conquistas. O [`Evento::JogoCarregado`] só chega no primeiro quadro, que é quando o jogo é
/// ativado, e a conquista continua valendo.
#[test]
fn o_jogo_so_e_ativado_quando_ha_memoria() {
    let mut memoria = memoria();
    let (mut c, _) = cliente(&[
        ("login2", LOGIN),
        ("achievementsets", CONJUNTO),
        ("startsession", SEM_DESBLOQUEIOS),
        ("awardachievement", DESBLOQUEIO),
    ]);
    c.entra_com_senha("User", "Pa$$word");
    c.carrega_jogo("0123456789abcdef0123456789abcdef");
    c.ocioso(None);
    c.ocioso(None);
    assert_eq!(c.eventos(), vec![Evento::Entrou(Ok(()))]);
    assert!(c.jogo().is_none(), "ainda não ativado");

    c.quadro(&memoria);
    assert_eq!(c.eventos(), vec![Evento::JogoCarregado(Ok(()))]);

    // Um quadro com a condição falsa arma a conquista; só o seguinte pode dispará-la.
    c.quadro(&memoria);
    memoria.bytes[1] = 3;
    memoria.bytes[2] = 7;
    c.quadro(&memoria);
    let eventos = c.eventos();
    assert!(
        matches!(&eventos[..], [Evento::Desbloqueou(x)] if x.id == 5501),
        "{eventos:?}"
    );
}

/// O hash de referência do Zeebo é o MD5 do `.mod` inteiro, feito pelo `rc_hash` do fork.
#[test]
fn o_hash_do_modulo_e_o_md5_do_arquivo_inteiro() {
    assert_eq!(
        hash_do_modulo(b"abc").as_deref(),
        Some("900150983cd24fb0d6963f7d28e17f72")
    );
}
