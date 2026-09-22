//! Síntese por banco de amostras (SoundFont) para o MIDI.
//!
//! **Por que existe.** A comparação com o Zeebulator mediu o que a tabela de timbres pode e o que
//! ela não pode: depois de refeita contra a medição, o erro de centroide dela caiu de 7635 para
//! 701 Hz, mas três coisas continuaram fora de alcance, e todas as três são **material de amostra**:
//!
//! | | amostra real | tabela de timbres |
//! |---|---|---|
//! | razão de harmônicos das cordas | 8,37 | 0,60 |
//! | razão de harmônicos da distorção | 4,58 | 0,84 |
//! | centroide da bateria | 7092 Hz | 6661 Hz |
//!
//! Aqui a partitura é tocada com o banco, e essas três passam a vir do próprio banco.
//!
//! **Por que `rustysynth` e não o código do Zeebulator.** O Zeebulator é GPLv3 e o Zeebx é
//! `GPL-2.0-only`; as duas licenças não se combinam, então nada dele pode ser copiado — nem o
//! invólucro em volta do TinySoundFont. O `rustysynth` é **MIT** e **Rust puro**, então entra no
//! core Libretro sem trazer biblioteca de host nenhuma, que é a regra do projeto.
//!
//! **Por que o banco não vem embutido.** São 32 MB (medido: GeneralUser GS, 32.319.396 B) e a
//! carga pede +64 MiB de RSS, porque as amostras viram `float`. Embutido, o `.so` do core sairia de
//! 15,6 MB para ~48 MB em seis alvos de CI — e um core que baixa 48 MB para rodar um jogo de 1 MB
//! não se paga. O banco é **opcional**, procurado na pasta do aparelho, e sem ele o MIDI volta para
//! a tabela de timbres, que é o que já toca hoje.
//!
//! O caminho de busca é o mesmo da fonte do sistema: a pasta do aparelho que o frontend entregou
//! primeiro, e depois o perfil do desktop. `ZEEBX_SOUNDFONT` aponta um arquivo direto, para
//! experimentar sem instalar nada.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::audio::wav::Sound;

/// Teto de duração de uma música sintetizada, em segundos.
///
/// O mesmo do sintetizador de tabela, e pela mesma razão: uma partitura com defeito não pode
/// reservar memória sem fim.
///
/// **Não é um teto de conveniência: é o mesmo número dos dois caminhos.** A primeira versão tinha
/// 36 s aqui, e a música do Double Dragon — 47,5 s — saía cortada, o que só apareceu porque a
/// medição comparou a duração com a da referência. Dois caminhos que dizem tocar a mesma partitura
/// não podem ter tetos diferentes.
const MAX_SEGUNDOS: f64 = 300.0;

/// Quantos quadros por bloco na renderização. O sequenciador do `rustysynth` trabalha em blocos.
const BLOCO: usize = 1024;

/// Onde o banco é procurado, em ordem de preferência.
///
/// `ZEEBX_SOUNDFONT` primeiro porque é o caminho de quem está experimentando; depois a pasta do
/// aparelho, que é o que o frontend controla; e por último o perfil do desktop.
pub fn candidatos(aparelho: &Path) -> Vec<PathBuf> {
    let mut saida = Vec::new();
    if let Some(caminho) = std::env::var_os("ZEEBX_SOUNDFONT") {
        saida.push(PathBuf::from(caminho));
    }
    for base in [
        aparelho.join("soundfonts"),
        crate::config::config_dir().join("aparelho").join("soundfonts"),
    ] {
        for nome in bancos_em(&base) {
            saida.push(nome);
        }
    }
    saida
}

/// Os `.sf2` de uma pasta, em ordem estável.
///
/// Estável porque dois bancos no mesmo diretório não podem dar resultados diferentes entre
/// execuções por causa da ordem em que o sistema de arquivos lista — a mesma razão da fonte.
fn bancos_em(base: &Path) -> Vec<PathBuf> {
    let Ok(entradas) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut bancos: Vec<PathBuf> = entradas
        .filter_map(Result::ok)
        .map(|entrada| entrada.path())
        .filter(|caminho| {
            caminho
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("sf2"))
        })
        .collect();
    bancos.sort();
    bancos
}

/// O primeiro banco que existir de verdade, entre os candidatos.
pub fn primeiro_banco(aparelho: &Path) -> Option<PathBuf> {
    candidatos(aparelho).into_iter().find(|c| c.is_file())
}

/// Um banco carregado.
///
/// Caro de construir (32 MB de amostras convertidas para `float`) e barato de reusar, então fica
/// guardado por caminho: um jogo que toca doze músicas carrega o banco uma vez.
pub struct Banco {
    /// `Arc` porque o `Synthesizer` do `rustysynth` pede a fonte por `Arc`: uma voz nova por
    /// música, e o banco de 32 MB não é copiado junto.
    fonte: Arc<rustysynth::SoundFont>,
}

impl Banco {
    fn carrega(caminho: &Path) -> Option<Self> {
        let bytes = std::fs::read(caminho).ok()?;
        let mut leitor = std::io::Cursor::new(bytes);
        let fonte = Arc::new(rustysynth::SoundFont::new(&mut leitor).ok()?);
        Some(Self { fonte })
    }

    /// Quantos presets o banco tem, para o relatório dizer que ele é um banco de verdade.
    pub fn presets(&self) -> usize {
        self.fonte.get_presets().len()
    }
}

/// O banco já carregado, por caminho. Um só na prática; o mapa tolera que o caminho mude.
static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Banco>>>> = OnceLock::new();

/// Carrega o banco de `caminho`, reusando o que já estiver em memória.
pub fn abre(caminho: &Path) -> Option<Arc<Banco>> {
    let guarda = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut mapa = guarda.lock().ok()?;
    if let Some(banco) = mapa.get(caminho) {
        return Some(banco.clone());
    }
    let banco = Arc::new(Banco::carrega(caminho)?);
    mapa.insert(caminho.to_path_buf(), banco.clone());
    Some(banco)
}

/// Toca uma partitura SMF com o banco, devolvendo PCM mono na taxa pedida.
///
/// `None` quando o banco não abre, quando os bytes não são um SMF que o `rustysynth` aceite, ou
/// quando não sobra nenhuma amostra — e aí quem chamou segue para a tabela de timbres.
pub fn toca(banco: &Banco, bytes: &[u8], taxa: u32) -> Option<Sound> {
    let mut leitor = std::io::Cursor::new(bytes);
    let midi = rustysynth::MidiFile::new(&mut leitor).ok()?;
    let comprimento = midi.get_length().min(MAX_SEGUNDOS);
    let ajustes = rustysynth::SynthesizerSettings::new(taxa as i32);
    let sintetizador = rustysynth::Synthesizer::new(&banco.fonte, &ajustes).ok()?;
    let mut sequencia = rustysynth::MidiFileSequencer::new(sintetizador);
    sequencia.play(&Arc::new(midi), false);

    let total = ((comprimento + 0.5) * f64::from(taxa)) as usize + 1;
    let mut amostras = Vec::with_capacity(total);
    let (mut esquerda, mut direita) = (vec![0.0f32; BLOCO], vec![0.0f32; BLOCO]);
    while amostras.len() < total {
        sequencia.render(&mut esquerda, &mut direita);
        for i in 0..BLOCO {
            if amostras.len() >= total {
                break;
            }
            // Mono: a média dos dois canais. O console tem uma caixa só, e o mixer duplica depois.
            amostras.push(0.5 * (esquerda[i] + direita[i]));
        }
        if sequencia.end_of_sequence() {
            break;
        }
    }
    // A cauda: o banco pode terminar antes do comprimento declarado, e um som mais curto que a
    // partitura faria o jogo achar que a música acabou cedo.
    amostras.resize(total, 0.0);
    // **O nível é o mesmo dos dois caminhos.** A tabela de timbres deixa o pico em 0,8 (ver
    // `midi::normaliza`), e o banco saía em 0,53: a mesma música trocava de volume conforme o
    // aparelho tivesse ou não um `.sf2` instalado, e no jogo isso muda o balanço entre a trilha e
    // os efeitos, que passam pelo mesmo misturador.
    crate::audio::midi::normaliza(&mut amostras);
    Some(Sound {
        rate: taxa,
        channels: 1,
        samples: amostras,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O banco de teste, do harness de comparação que vive fora do repositório.
    fn banco_de_teste() -> Option<PathBuf> {
        let candidatos = [
            std::env::var_os("ZEEBX_SOUNDFONT").map(PathBuf::from),
            Some(PathBuf::from(
                "/home/rafaelfrequiao/zeebx-midi-harness/zbfont/GeneralUser-GS.sf2",
            )),
        ];
        candidatos.into_iter().flatten().find(|c| c.is_file())
    }

    /// Um SMF de uma nota, para não depender de arquivo.
    fn uma_nota(programa: u8, nota: u8, _pulsos: u32) -> Vec<u8> {
        let mut trilha = vec![0x00, 0xc0, programa, 0x00, 0x90, nota, 100];
        trilha.extend([0x81, 0x70]);
        trilha.extend([0x80, nota, 0x40, 0x00, 0xff, 0x2f, 0x00]);
        let mut out = b"MThd".to_vec();
        out.extend(6u32.to_be_bytes());
        out.extend(0u16.to_be_bytes());
        out.extend(1u16.to_be_bytes());
        out.extend(96i16.to_be_bytes());
        out.extend(b"MTrk");
        out.extend((trilha.len() as u32).to_be_bytes());
        out.extend(trilha);
        out
    }

    /// **O banco carrega e toca.** O teste se declara dispensado quando não há banco no disco, e
    /// diz isso: um teste que passa sem provar nada é pior que um teste ausente.
    #[test]
    fn o_banco_toca_a_partitura() {
        let Some(caminho) = banco_de_teste() else {
            eprintln!("sem banco .sf2 no disco: teste dispensado (use ZEEBX_SOUNDFONT)");
            return;
        };
        let banco = abre(&caminho).expect("o banco abre");
        assert!(
            banco.presets() > 100,
            "um banco General MIDI tem centenas de presets, tem {}",
            banco.presets()
        );
        let som = toca(&banco, &uma_nota(0, 69, 240), 22_050).expect("toca");
        assert_eq!(som.rate, 22_050);
        assert_eq!(som.channels, 1);
        let pico = som.samples.iter().fold(0.0f32, |a, s| a.max(s.abs()));
        assert!(pico > 0.01, "o piano tinha de soar, pico {pico}");
    }


    /// **O banco distingue o arco da palheta.** Era o defeito mais audível da tabela de timbres:
    /// 29 (*overdrive*), 30 (distorcida), 42 (violoncelo) e 48 (cordas) saíam com a **mesma onda**.
    ///
    /// A medida é o **ataque**, e não o centroide: numa nota só os dois têm centroide parecido
    /// (medido: 435 Hz e 426 Hz), então centroide não separa. O que separa é o comportamento —
    /// palheta ataca no talo, arco começa macio e cresce. No soundfont de referência a guitarra
    /// começa em 1,00 do próprio pico e o violoncelo em 0,46.
    #[test]
    fn o_banco_distingue_o_arco_da_palheta() {
        let Some(caminho) = banco_de_teste() else {
            eprintln!("sem banco .sf2 no disco: teste dispensado (use ZEEBX_SOUNDFONT)");
            return;
        };
        let banco = abre(&caminho).expect("o banco abre");
        let envelope = |programa: u8| -> Vec<f32> {
            let som = toca(&banco, &uma_nota(programa, 60, 480), 22_050).expect("toca");
            let janela = som.rate as usize / 5;
            let mut saida: Vec<f32> = (0..5)
                .map(|i| {
                    let trecho = &som.samples[i * janela..(i + 1) * janela];
                    (trecho.iter().map(|s| s * s).sum::<f32>() / janela as f32).sqrt()
                })
                .collect();
            let pico = saida.iter().fold(0.0f32, |a, s| a.max(*s));
            if pico > 0.0 {
                for valor in &mut saida {
                    *valor /= pico;
                }
            }
            saida
        };
        let palheta = envelope(29);
        let arco = envelope(42);
        assert!(
            arco[0] < palheta[0] * 0.75,
            "o arco tinha de atacar mais macio que a palheta: {:.2} contra {:.2}",
            arco[0],
            palheta[0]
        );
    }
}
