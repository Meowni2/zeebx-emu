//! O estado da máquina em seções: registradores, memória e os contadores de alocação.
//!
//! ## O que entra, e por quê
//!
//! **Toda região gravável** do mapa. O que fica de fora é o que o carregador monta uma vez e
//! ninguém altera depois: as vtables e a página nula. As duas são derivadas da imagem, então
//! gravá-las só gastaria bytes — e um estado que carrega vtables velhas por cima das novas seria
//! pior que não carregar nada.
//!
//! ## O corte nas regiões que só crescem
//!
//! O heap (64 MB), a região de objetos (4 MB) e as superfícies (8 MB) nascem **zeradas** e só
//! avançam: `next` é o primeiro endereço nunca usado, e abaixo dele está tudo o que o jogo pediu.
//! Gravar além disso seria escrever dezenas de megabytes de zero em cada save state. O corte é o
//! `next` do alocador de cada região, e é o que faz o tamanho do estado ser proporcional ao que o
//! jogo usou — alguns megabytes, e não oitenta.
//!
//! ## O que ainda não entra
//!
//! A região de objetos entra, mas o **livro** dela não: [`crate::brew::objects::ObjectStore`]
//! guarda a interface de cada objeto num `enum` que ainda não tem codificação estável. Enquanto
//! isso e as tabelas de estado por objeto não entrarem, o `retro_serialize_size` do core continua
//! respondendo zero — ver o plano. Restaurar um estado parcial é exatamente o que o critério
//! proíbe, e a porta é essa.

use super::{
    Callback, DecodedImage, Machine, MemStream, OpenFile, SoundState, Timer,
};
use crate::video::display::Framebuffer;
use crate::cpu::{CpuBackend, Reg};
use crate::save_state::{Erro, Guardavel, Leitor, Secoes};

/// Os registradores, na ordem em que a seção os grava.
///
/// A ordem é fixa e explícita: gravar "todos os registradores" na ordem do `enum` deixaria o
/// formato refém de alguém reordenar a enumeração.
const REGISTRADORES: [Reg; 16] = [
    Reg::R0,
    Reg::R1,
    Reg::R2,
    Reg::R3,
    Reg::R4,
    Reg::R5,
    Reg::R6,
    Reg::R7,
    Reg::R8,
    Reg::R9,
    Reg::R10,
    Reg::R11,
    Reg::R12,
    Reg::Sp,
    Reg::Lr,
    Reg::Pc,
];

/// Quanto de uma região foi usado, a partir do primeiro endereço nunca usado.
///
/// Um `next` fora da região seria defeito do alocador; gravar tudo é a resposta segura.
fn usado(proximo: u32, base: u32, tamanho: usize) -> usize {
    let usado = proximo.saturating_sub(base) as usize;
    if usado > tamanho { tamanho } else { usado }
}

/// O nome da seção de uma região.
fn secao_da_regiao(nome: &str) -> String {
    format!("mem.{nome}")
}

impl<C: CpuBackend> Machine<C> {
    /// Quantos bytes de uma região devem ser gravados.
    ///
    /// O `next` do alocador da região, quando ela tem um; o tamanho inteiro, quando não tem.
    /// Devolve `None` para a região que não deve ser gravada de jeito nenhum.
    fn quanto_gravar(&self, nome: &str, base: u32, tamanho: usize, gravavel: bool) -> Option<usize> {
        if !gravavel {
            return None;
        }
        // **As três regiões que só crescem vão cortadas no primeiro endereço nunca usado**, e o
        // corte é possível porque os livros das três entram no estado: é de lá que sai o tamanho
        // esperado na volta. Gravar as três inteiras seriam 76 MB de zero por save state.
        let usado = match nome {
            "heap" => usado(self.heap.proximo(), base, tamanho),
            "objects" => usado(self.objects.proximo(), base, tamanho),
            "surfaces" => usado(self.superficies.proximo(), base, tamanho),
            _ => tamanho,
        };
        Some(usado)
    }

    /// O quanto de uma região o **estado** diz que era usado.
    ///
    /// Vem do arquivo, e não da máquina de agora: quem carrega um save state carrega o heap que
    /// estava lá. É o que permite conferir o tamanho da seção antes de escrever qualquer byte.
    fn quanto_do_estado(
        leitor: &Leitor<'_>,
        nome: &str,
        base: u32,
        tamanho: usize,
    ) -> Result<usize, Erro> {
        let (prefixo, campo) = match nome {
            "heap" => ("heap", "heap.next"),
            "objects" => ("objects", "objects.next"),
            "surfaces" => ("surfaces", "surfaces.next"),
            _ => return Ok(tamanho),
        };
        let proximo = leitor.u32(campo)?;
        let inicio = match prefixo {
            "objects" => base,
            _ => leitor.u32(&format!("{prefixo}.base"))?,
        };
        Ok(usado(proximo, inicio, tamanho))
    }

    /// Grava o estado em uma seção por região, mais os registradores e os livros.
    pub fn grava_estado(&self) -> Vec<u8> {
        let mut secoes = Secoes::nova();
        secoes.poe_u32s(
            "cpu.regs",
            REGISTRADORES.iter().map(|reg| self.cpu.read_reg(*reg)),
        );
        secoes.poe_u32("cpu.thumb", u32::from(self.cpu.em_thumb()));
        // As flags e o relógio virtual. Sem eles o jogo volta com a memória certa e as decisões
        // erradas: a comparação que ele fez antes de salvar vale, e o desvio vem depois.
        secoes.poe_u32("cpu.cpsr", self.cpu.cpsr());
        secoes.poe(
            "cpu.instructions",
            self.cpu.instructions().to_le_bytes().to_vec(),
        );
        for regiao in self.module.mem.regions() {
            let Some(quanto) =
                self.quanto_gravar(regiao.name, regiao.base, regiao.bytes.len(), regiao.writable)
            else {
                continue;
            };
            let mut bytes = vec![0u8; quanto];
            // A memória **viva** está no núcleo, e não no mapa do carregador: é por aqui que se
            // lê o que o jogo escreveu desde o `reset`.
            if self.cpu.read_mem(regiao.base, &mut bytes).is_ok() {
                secoes.poe(&secao_da_regiao(regiao.name), bytes);
            }
        }
        // Os três livros. O das superfícies vai com o nome da região, para a seção e o livro
        // terem o mesmo nome — quem lê o arquivo não precisa de tabela de tradução.
        self.grava_entrada_e_tempo(&mut secoes);
        self.grava_tabelas_numericas(&mut secoes);
        self.grava_fontes_e_arquivos(&mut secoes);
        self.grava_conteudo(&mut secoes);
        self.grava_superficies_e_imagens(&mut secoes);
        self.grava_escalares_e_mapas(&mut secoes);
        self.heap.grava_com_prefixo("heap", &mut secoes);
        self.objects.grava(&mut secoes);
        self.superficies.grava_com_prefixo("surfaces", &mut secoes);
        secoes.fecha()
    }

    pub fn restaura_estado(&mut self, arquivo: &[u8]) -> Result<(), Erro> {
        let leitor = Leitor::abre(arquivo)?;

        let registradores = leitor.u32s("cpu.regs")?;
        if registradores.len() != REGISTRADORES.len() {
            return Err(Erro::Secao {
                nome: "cpu.regs".to_string(),
                motivo: format!(
                    "o estado tem {} registradores e a máquina tem {}",
                    registradores.len(),
                    REGISTRADORES.len()
                ),
            });
        }
        let _thumb = leitor.u32("cpu.thumb")?;
        let cpsr = leitor.u32("cpu.cpsr")?;
        let bytes_do_relogio = leitor.secao("cpu.instructions").ok_or_else(|| Erro::Secao {
            nome: "cpu.instructions".to_string(),
            motivo: "a seção não está no arquivo".to_string(),
        })?;
        if bytes_do_relogio.len() != 8 {
            return Err(Erro::Secao {
                nome: "cpu.instructions".to_string(),
                motivo: format!("esperava 8 bytes e tem {}", bytes_do_relogio.len()),
            });
        }
        let relogio = u64::from_le_bytes(bytes_do_relogio.try_into().unwrap());

        // A memória é lida e conferida antes de qualquer escrita.
        let mut regioes: Vec<(u32, Vec<u8>)> = Vec::new();
        for regiao in self.module.mem.regions() {
            let Some(_) = self.quanto_gravar(regiao.name, regiao.base, regiao.bytes.len(), regiao.writable) else {
                continue;
            };
            let secao = secao_da_regiao(regiao.name);
            let bytes = leitor.secao(&secao).ok_or_else(|| Erro::Secao {
                nome: secao.clone(),
                motivo: "a seção não está no arquivo".to_string(),
            })?;
            // **O tamanho esperado sai do próprio estado**, e não da máquina de agora.
            let quanto = Self::quanto_do_estado(&leitor, regiao.name, regiao.base, regiao.bytes.len())?;
            if bytes.len() != quanto {
                return Err(Erro::Secao {
                    nome: secao,
                    motivo: format!(
                        "o estado tem {} bytes desta região e esta máquina espera {quanto} \
                         — é um estado de outro jogo ou de outra versão do motor",
                        bytes.len()
                    ),
                });
            }
            regioes.push((regiao.base, bytes.to_vec()));
        }

        // Daqui para baixo é aplicação: ou tudo, ou nada.
        self.restaura_entrada_e_tempo(&leitor)?;
        self.restaura_tabelas_numericas(&leitor)?;
        self.restaura_fontes_e_arquivos(&leitor)?;
        self.restaura_conteudo(&leitor)?;
        self.restaura_superficies_e_imagens(&leitor)?;
        self.restaura_escalares_e_mapas(&leitor)?;
        self.heap.restaura_com_prefixo("heap", &leitor)?;
        self.objects.restaura(&leitor)?;
        self.superficies
            .restaura_com_prefixo("surfaces", &leitor)?;
        for (base, bytes) in regioes {
            self.cpu.write_mem(base, &bytes).map_err(|erro| Erro::Secao {
                nome: format!("mem.{base:#010x}"),
                motivo: format!("não deu para escrever: {erro}"),
            })?;
        }
        for (reg, valor) in REGISTRADORES.iter().zip(registradores) {
            self.cpu.write_reg(*reg, valor);
        }
        // O `CPSR` fica por último: escrever no `PC` pode mexer nos bits de modo em alguns
        // núcleos, e o valor que veio do estado é o que manda.
        self.cpu.set_cpsr(cpsr);
        self.cpu.set_instructions(relogio);
        Ok(())
    }
}


/// O código do nome de um sinal de entrada.
///
/// Os dois nomes são o conjunto **fechado** que o `IHIDDevice` usa para avisar quem registrou:
/// `RegisterForButtonEvent` e `RegisterForPositionChange`. Guardar o nome como texto deixaria o
/// formato refém de uma string; guardar o código deixa a leitura impossível de errar em silêncio —
/// código desconhecido é recusa, e não um registro perdido.
fn codigo_do_sinal_de_entrada(nome: &str) -> u32 {
    match nome {
        "RegisterForButtonEvent" => 0,
        "RegisterForPositionChange" => 1,
        // Um nome novo entra aqui **e** na tabela do teste `o_nome_do_sinal_vai_e_volta`: sem isso
        // o save state passaria a perder o registro em silêncio.
        _ => u32::MAX,
    }
}

/// O nome de um sinal de entrada pelo código.
fn nome_do_sinal_de_entrada(codigo: u32) -> Option<&'static str> {
    match codigo {
        0 => Some("RegisterForButtonEvent"),
        1 => Some("RegisterForPositionChange"),
        _ => None,
    }
}

impl<C: CpuBackend> Machine<C> {
    /// A entrada e o agendamento: filas de tecla e de botão, quem registrou sinal de aparelho, os
    /// temporizadores vencendo e os retornos pendentes.
    ///
    /// São as tabelas que um jogo sente na hora: sem as filas, a tecla que ele ainda não leu
    /// desaparece; sem os timers, o relógio que ele armou para daqui a duzentos milissegundos deixa
    /// de existir; e sem os sinais, o aviso de que o manche mudou não chega a quem o pediu.
    fn grava_entrada_e_tempo(&self, secoes: &mut Secoes) {
        secoes.poe_u32s(
            "entrada.teclas",
            self.teclas.iter().map(|(avk, baixo)| [*avk, u32::from(*baixo)]).flatten(),
        );
        for (porta, fila) in self.pad_events.iter().enumerate() {
            secoes.poe_u32s(
                &format!("entrada.pad_events.{porta}"),
                fila.iter()
                    .map(|(indice, baixo)| [*indice as u32, u32::from(*baixo)])
                    .flatten(),
            );
        }
        secoes.poe_mapa("entrada.portas", self.portas_de_aparelho.iter().map(|(a, p)| (*a, *p as u32)));
        secoes.poe_trios(
            "entrada.sinais",
            self.input_signals.iter().map(|((nome, porta), sinal)| {
                (
                    codigo_do_sinal_de_entrada(nome),
                    *porta as u32,
                    *sinal,
                )
            }),
        );
        secoes.poe_trios("agenda.timers", self.timers.iter().map(|t| {
            (t.deadline_ms, t.callback.function, t.callback.context)
        }));
        secoes.poe_trios("agenda.sinais", self.signals.iter().map(|(id, cb)| {
            (*id, cb.function, cb.context)
        }));
        secoes.poe_u32s(
            "agenda.pendentes",
            self.pending_signals
                .iter()
                .flat_map(|cb| [cb.function, cb.context]),
        );
    }

    /// Lê e confere entrada e agendamento. Nada é aplicado se alguma seção não bater.
    fn restaura_entrada_e_tempo(
        &mut self,
        leitor: &Leitor<'_>,
    ) -> Result<(), Erro> {
        // **Em ordem**: a fila de teclas é uma sequência, e não um mapa.
        let teclas = leitor.pares_em_ordem("entrada.teclas")?;
        let portas = leitor.pares("entrada.portas")?;
        let sinais_crus = leitor.trios("entrada.sinais")?;
        let timers = leitor.trios("agenda.timers")?;
        let signals = leitor.trios("agenda.sinais")?;
        let pendentes = leitor.pares_em_ordem("agenda.pendentes")?;

        let mut filas = Vec::new();
        for porta in 0..self.pad_events.len() {
            filas.push(leitor.pares_em_ordem(&format!("entrada.pad_events.{porta}"))?);
        }

        // O registro do sinal de aparelho é o único com nome: código desconhecido é recusa.
        let mut sinais = std::collections::BTreeMap::new();
        for (codigo, porta, sinal) in sinais_crus {
            let nome = nome_do_sinal_de_entrada(codigo).ok_or_else(|| Erro::Secao {
                nome: "entrada.sinais".to_string(),
                motivo: format!(
                    "o estado registra o sinal de aparelho {codigo}, que este motor não conhece"
                ),
            })?;
            if porta as usize >= self.pad_events.len() {
                return Err(Erro::Secao {
                    nome: "entrada.sinais".to_string(),
                    motivo: format!("a porta {porta} não existe"),
                });
            }
            sinais.insert((nome, porta as usize), sinal);
        }
        let mut portas_de_aparelho = std::collections::HashMap::new();
        for (aparelho, porta) in portas {
            if porta as usize >= self.pad_events.len() {
                return Err(Erro::Secao {
                    nome: "entrada.portas".to_string(),
                    motivo: format!("a porta {porta} não existe"),
                });
            }
            portas_de_aparelho.insert(aparelho, porta as usize);
        }

        // Daqui para baixo é aplicação.
        self.teclas = teclas
            .into_iter()
            .map(|(avk, baixo)| (avk, baixo != 0))
            .collect();
        for (porta, fila) in filas.into_iter().enumerate() {
            self.pad_events[porta] = fila
                .into_iter()
                .map(|(indice, baixo)| (indice as usize, baixo != 0))
                .collect();
        }
        self.portas_de_aparelho = portas_de_aparelho;
        self.input_signals = sinais;
        self.timers = timers
            .into_iter()
            .map(|(deadline_ms, function, context)| Timer {
                deadline_ms,
                callback: Callback { function, context },
            })
            .collect();
        self.signals = signals
            .into_iter()
            .map(|(id, function, context)| (id, Callback { function, context }))
            .collect();
        self.pending_signals = pendentes
            .into_iter()
            .map(|(function, context)| Callback { function, context })
            .collect();
        Ok(())
    }
}


/// As tabelas de estado por objeto cujo conteúdo são **números**.
///
/// A lista é declarada uma vez, aqui, e é ela que grava e que lê — com o mesmo nome de seção dos
/// dois lados. Foi assim que a maior parte do estado entrou: cada tabela é a mesma forma
/// (`HashMap<u32, número>` ou `HashMap<u32, punhado de números>`), e o que muda é só o nome.
///
/// **O que não entra:** as tabelas que guardam pixels ou bytes em quantidade — `bitmaps`,
/// `images`, `gl_last_frame`. Elas são a maior parte do que sobra e precisam de um formato próprio
/// (comprimir, ou apontar para a memória do guest quando o conteúdo já está lá). Enquanto não
/// entrarem, o core continua dizendo que não salva.
impl<C: CpuBackend> Machine<C> {
    /// As tabelas numéricas: `(nome da seção, valores)`.
    /// Grava as tabelas numéricas, uma seção por tabela.
    fn grava_tabelas_numericas(&self, secoes: &mut Secoes) {
        for (nome, valores) in self.tabelas_numericas() {
            secoes.poe_u32s(nome, valores);
        }
    }

    /// Lê e aplica as tabelas numéricas.
    ///
    /// Como no resto: **lê tudo antes de escrever qualquer coisa**. E confere a faixa de cada campo
    /// que é menor que `u32` no motor — um volume que não cabe em `u16` ou um `dono` que não é
    /// booleano é arquivo corrompido, e não valor a acomodar.
    fn restaura_tabelas_numericas(&mut self, leitor: &Leitor<'_>) -> Result<(), Erro> {
        let mapa = |nome: &str| -> Result<std::collections::HashMap<u32, u32>, Erro> {
            Ok(leitor.pares(nome)?.into_iter().collect())
        };
        let dib_buffers = mapa("tab.dib_buffers")?;
        let dib_capacity = mapa("tab.dib_capacity")?;
        let transformacoes = mapa("tab.transformacoes")?;
        let canvases = mapa("tab.canvases")?;
        let feeds = mapa("tab.feeds")?;
        let image_bitmaps = mapa("tab.image_bitmaps")?;
        let image_info = mapa("tab.image_info")?;

        let mut transparency_crua = mapa("tab.transparency")?;
        for (indice, valor) in transparency_crua.iter() {
            if *valor > u32::from(u16::MAX) {
                return Err(Erro::Secao {
                    nome: "tab.transparency".to_string(),
                    motivo: format!("o objeto {indice:#010x} tem transparência {valor}, que não cabe num u16"),
                });
            }
        }
        let transparency: std::collections::HashMap<u32, u16> = transparency_crua
            .drain()
            .map(|(a, v)| (a, v as u16))
            .collect();

        let mut streams = std::collections::HashMap::new();
        for registro in leitor.registros("tab.streams", 5)? {
            let dono = match registro[4] {
                0 => false,
                1 => true,
                outro => {
                    return Err(Erro::Secao {
                        nome: "tab.streams".to_string(),
                        motivo: format!("o campo `dono` vale {outro}, e é booleano"),
                    })
                }
            };
            streams.insert(
                registro[0],
                MemStream {
                    buffer: registro[1],
                    size: registro[2],
                    position: registro[3],
                    dono,
                },
            );
        }

        let mut sounds = std::collections::HashMap::new();
        for registro in leitor.registros("tab.sounds", 9)? {
            if registro[3] > u32::from(u16::MAX) {
                return Err(Erro::Secao {
                    nome: "tab.sounds".to_string(),
                    motivo: format!("o som {:#010x} tem volume {}", registro[0], registro[3]),
                });
            }
            let mut info = [0u8; 5];
            for (destino, valor) in info.iter_mut().zip(&registro[4..9]) {
                if *valor > 255 {
                    return Err(Erro::Secao {
                        nome: "tab.sounds".to_string(),
                        motivo: format!("o `AEESoundInfo` do som {:#010x} tem byte {valor}", registro[0]),
                    });
                }
                *destino = *valor as u8;
            }
            sounds.insert(
                registro[0],
                SoundState {
                    notify: Callback {
                        function: registro[1],
                        context: registro[2],
                    },
                    info,
                    volume: registro[3] as u16,
                },
            );
        }

        // Aplicação.
        self.dib_buffers = dib_buffers;
        self.dib_capacity = dib_capacity;
        self.transformacoes = transformacoes;
        self.canvases = canvases;
        self.feeds = feeds;
        self.image_bitmaps = image_bitmaps;
        self.image_info = image_info;
        self.transparency = transparency;
        self.streams = streams;
        self.sounds = sounds;
        Ok(())
    }

    fn tabelas_numericas(&self) -> Vec<(&'static str, Vec<u32>)> {
        let mapa = |m: &std::collections::HashMap<u32, u32>| -> Vec<u32> {
            let mut pares: Vec<(u32, u32)> = m.iter().map(|(a, b)| (*a, *b)).collect();
            pares.sort_unstable();
            pares.into_iter().flat_map(|(a, b)| [a, b]).collect()
        };
        vec![
            ("tab.dib_buffers", mapa(&self.dib_buffers)),
            ("tab.dib_capacity", mapa(&self.dib_capacity)),
            ("tab.transformacoes", mapa(&self.transformacoes)),
            ("tab.canvases", mapa(&self.canvases)),
            ("tab.feeds", mapa(&self.feeds)),
            ("tab.image_bitmaps", mapa(&self.image_bitmaps)),
            ("tab.image_info", mapa(&self.image_info)),
            (
                "tab.transparency",
                mapa(
                    &self
                        .transparency
                        .iter()
                        .map(|(a, v)| (*a, u32::from(*v)))
                        .collect(),
                ),
            ),
            // Onde ficava a leitura de cada stream de memória: são quatro números por objeto.
            (
                "tab.streams",
                self.streams
                    .iter()
                    .map(|(id, s)| {
                        vec![
                            *id,
                            s.buffer,
                            s.size,
                            s.position,
                            u32::from(s.dono),
                        ]
                    })
                    .flatten()
                    .collect(),
            ),
            // O estado de cada `ISound`: quem avisa, os cinco bytes do `AEESoundInfo` e o volume.
            (
                "tab.sounds",
                self.sounds
                    .iter()
                    .map(|(id, s)| {
                        let mut registro = vec![
                            *id,
                            s.notify.function,
                            s.notify.context,
                            u32::from(s.volume),
                        ];
                        registro.extend(s.info.iter().map(|b| u32::from(*b)));
                        registro
                    })
                    .flatten()
                    .collect(),
            ),
        ]
    }
}


/// As métricas de fonte e os arquivos abertos.
///
/// Duas tabelas que **não** guardam conteúdo, e é por isso que são baratas:
///
/// - as métricas de fonte são sete números por objeto — vêm do `.bid` e são consultadas a cada
///   `GetFontMetrics`;
/// - um arquivo aberto tem **caminho e deslocamento**, e não bytes: o conteúdo está no disco. Na
///   volta o arquivo é **reaberto pelo caminho** e o deslocamento é devolvido com um `seek`. É
///   honesto e é o que um save state de console faz: se o arquivo mudou no disco entre salvar e
///   carregar, o estado carrega o arquivo de agora — e isso vai dito aqui, para não virar surpresa.
impl<C: CpuBackend> Machine<C> {
    fn grava_fontes_e_arquivos(&self, secoes: &mut Secoes) {
        secoes.poe_registros(
            "tab.fontes",
            self.fontes
                .iter()
                .map(|(id, m)| {
                    vec![
                        *id,
                        u32::from(m.ascent),
                        u32::from(m.descent),
                        u32::from(m.leading),
                        u32::from(m.max_char_width),
                        u32::from(m.height),
                        u32::from(m.bold),
                        u32::from(m.italic),
                    ]
                })
                .collect::<Vec<_>>(),
        );
        // Um objeto por arquivo, com os próprios nomes de seção: o identificador entra no nome, e
        // não numa lista paralela de identificadores que alguém teria de manter em ordem.
        secoes.poe_u32s("arq.ids", self.open_files.keys().copied());
        for (id, arquivo) in &self.open_files {
            secoes.poe_texto(&format!("arq.{id}.guest"), &arquivo.guest_path);
            secoes.poe_texto(&format!("arq.{id}.caminho"), &arquivo.caminho.to_string_lossy());
            // O deslocamento lido **agora**: é ele que diz em que ponto da leitura o jogo estava.
            let posicao = deslocamento(&arquivo.file).unwrap_or(0);
            secoes.poe(&format!("arq.{id}.pos"), posicao.to_le_bytes().to_vec());
        }
    }

    fn restaura_fontes_e_arquivos(&mut self, leitor: &Leitor<'_>) -> Result<(), Erro> {
        let mut fontes = std::collections::HashMap::new();
        for registro in leitor.registros("tab.fontes", 8)? {
            let menor = |indice: usize| -> Result<u16, Erro> {
                u16::try_from(registro[indice]).map_err(|_| Erro::Secao {
                    nome: "tab.fontes".to_string(),
                    motivo: format!(
                        "a fonte {:#010x} tem o campo {indice} valendo {}, que não cabe num u16",
                        registro[0], registro[indice]
                    ),
                })
            };
            fontes.insert(
                registro[0],
                crate::machine::font::Metricas {
                    ascent: menor(1)?,
                    descent: menor(2)?,
                    leading: menor(3)?,
                    max_char_width: menor(4)?,
                    height: menor(5)?,
                    bold: registro[6] != 0,
                    italic: registro[7] != 0,
                },
            );
        }

        // Os arquivos são **reabertos** antes de qualquer coisa ser aplicada: um caminho que não
        // existe mais é recusa, e não uma tabela de arquivos com uma entrada vazia.
        let ids = leitor.u32s("arq.ids")?;
        let mut abertos = std::collections::HashMap::new();
        for id in ids {
            let guest_path = leitor.texto(&format!("arq.{id}.guest"))?;
            let caminho = std::path::PathBuf::from(leitor.texto(&format!("arq.{id}.caminho"))?);
            let bytes_da_posicao = leitor.secao(&format!("arq.{id}.pos")).ok_or_else(|| Erro::Secao {
                nome: format!("arq.{id}.pos"),
                motivo: "a seção não está no arquivo".to_string(),
            })?;
            if bytes_da_posicao.len() != 8 {
                return Err(Erro::Secao {
                    nome: format!("arq.{id}.pos"),
                    motivo: format!("esperava 8 bytes e tem {}", bytes_da_posicao.len()),
                });
            }
            let posicao = u64::from_le_bytes(bytes_da_posicao.try_into().unwrap());
            let mut file = std::fs::File::open(&caminho).map_err(|erro| Erro::Secao {
                nome: format!("arq.{id}"),
                motivo: format!(
                    "o estado guarda \"{}\" aberto e ele não pôde ser reaberto: {erro}",
                    caminho.display()
                ),
            })?;
            use std::io::Seek as _;
            file.seek(std::io::SeekFrom::Start(posicao)).map_err(|erro| Erro::Secao {
                nome: format!("arq.{id}"),
                motivo: format!("não deu para voltar o arquivo ao byte {posicao}: {erro}"),
            })?;
            abertos.insert(id, OpenFile { file, guest_path, caminho });
        }

        self.fontes = fontes;
        self.open_files = abertos;
        Ok(())
    }
}

/// Em que byte do arquivo a leitura está.
fn deslocamento(file: &std::fs::File) -> std::io::Result<u64> {
    use std::io::Seek as _;
    let mut copia = file;
    copia.stream_position()
}


/// As tabelas que guardam **conteúdo**: preferências, parâmetros de coleção, dados de `IConfig`,
/// o texto decifrado, a resposta da rede e o que o `IWeb` já baixou.
///
/// Todas usam o mesmo ajudante de blocos, e a diferença entre elas é só quantos números formam a
/// chave — um para `HashMap<u32, Vec<u8>>`, dois para `HashMap<(u32, u32), Vec<u8>>`. Guardam o
/// conteúdo porque ele **não está em lugar nenhum** fora do motor: ao contrário dos arquivos
/// abertos, que se reabrem do disco, um `SetPrefs` que o jogo fez já não existe em disco nenhum.
impl<C: CpuBackend> Machine<C> {
    fn grava_conteudo(&self, secoes: &mut Secoes) {
        secoes.poe_blocos(
            "cont.prefs",
            2,
            self.prefs
                .iter()
                .map(|((classe, versao), dados)| {
                    (vec![*classe, u32::from(*versao)], dados.clone())
                })
                .collect::<Vec<_>>(),
        );
        secoes.poe_blocos(
            "cont.parametros",
            2,
            self.parametros_de_colecao
                .iter()
                .map(|((a, b), dados)| (vec![*a, *b], dados.clone()))
                .collect::<Vec<_>>(),
        );
        secoes.poe_blocos(
            "cont.sources",
            1,
            self.sources
                .iter()
                .map(|(id, dados)| (vec![*id], dados.clone()))
                .collect::<Vec<_>>(),
        );
        secoes.poe_blocos(
            "cont.paginas",
            1,
            self.paginas_html
                .iter()
                .map(|(id, dados)| (vec![*id], dados.clone()))
                .collect::<Vec<_>>(),
        );
        // O `IConfig` tem um mapa por objeto: uma chave de dois números por entrada, o objeto e o
        // item.
        secoes.poe_blocos(
            "cont.config",
            2,
            self.config_items
                .iter()
                .flat_map(|(objeto, itens)| {
                    itens
                        .iter()
                        .map(move |(item, dados)| (vec![*objeto, *item], dados.clone()))
                })
                .collect::<Vec<_>>(),
        );
        secoes.poe_blocos(
            "cont.plaintexts",
            1,
            self.plaintexts
                .iter()
                .enumerate()
                .map(|(ordem, dados)| (vec![ordem as u32], dados.clone()))
                .collect::<Vec<_>>(),
        );
        secoes.poe_blocos("cont.resposta", 1, vec![(vec![0u32], self.web_response.clone())]);
    }

    fn restaura_conteudo(&mut self, leitor: &Leitor<'_>) -> Result<(), Erro> {
        // A largura da chave é conferida **antes** de qualquer índice: um arquivo que diga "chave
        // de um número" numa tabela de dois faria o índice `chave[1]` entrar em pânico, e pânico
        // no carregamento de um save state derruba o frontend.
        let conferir = |nome: &str, chave: &[u32], esperada: usize| -> Result<(), Erro> {
            if chave.len() != esperada {
                return Err(Erro::Secao {
                    nome: nome.to_string(),
                    motivo: format!(
                        "a chave tem {} número(s) e esta tabela usa {esperada}",
                        chave.len()
                    ),
                });
            }
            Ok(())
        };
        let mut prefs = std::collections::HashMap::new();
        for (chave, dados) in leitor.blocos("cont.prefs")? {
            conferir("cont.prefs", &chave, 2)?;
            prefs.insert((chave[0], chave[1] as u16), dados);
        }
        let mut parametros = std::collections::HashMap::new();
        for (chave, dados) in leitor.blocos("cont.parametros")? {
            conferir("cont.parametros", &chave, 2)?;
            parametros.insert((chave[0], chave[1]), dados);
        }
        let mut sources = std::collections::HashMap::new();
        for (chave, dados) in leitor.blocos("cont.sources")? {
            conferir("cont.sources", &chave, 1)?;
            sources.insert(chave[0], dados);
        }
        let mut paginas_html = std::collections::HashMap::new();
        for (chave, dados) in leitor.blocos("cont.paginas")? {
            conferir("cont.paginas", &chave, 1)?;
            paginas_html.insert(chave[0], dados);
        }
        let mut config_items: std::collections::HashMap<u32, std::collections::HashMap<u32, Vec<u8>>> =
            std::collections::HashMap::new();
        for (chave, dados) in leitor.blocos("cont.config")? {
            conferir("cont.config", &chave, 2)?;
            config_items.entry(chave[0]).or_default().insert(chave[1], dados);
        }
        let mut plaintexts = std::collections::VecDeque::new();
        for (chave, dados) in leitor.blocos("cont.plaintexts")? {
            conferir("cont.plaintexts", &chave, 1)?;
            let ordem = chave[0] as usize;
            // A fila é **ordenada**, e a ordem é o conteúdo: o índice gravado é o lugar dela.
            while plaintexts.len() <= ordem {
                plaintexts.push_back(Vec::new());
            }
            plaintexts[ordem] = dados;
        }
        let resposta = leitor
            .blocos("cont.resposta")?
            .into_iter()
            .map(|(_, dados)| dados)
            .next()
            .ok_or_else(|| Erro::Secao {
                nome: "cont.resposta".to_string(),
                motivo: "a seção não tem a resposta".to_string(),
            })?;

        self.prefs = prefs;
        self.parametros_de_colecao = parametros;
        self.sources = sources;
        self.paginas_html = paginas_html;
        self.config_items = config_items;
        self.plaintexts = plaintexts;
        self.web_response = resposta;
        Ok(())
    }
}



/// Empacota um `Vec<bool>` em bits, um por pixel.
///
/// O `DecodedImage` guarda um booleano por pixel dizendo se ele é desenhado. Um byte por pixel
/// gastaria oito vezes o necessário numa tabela que já guarda dois bytes de cor por pixel.
fn empacota_bits(bits: &[bool]) -> Vec<u8> {
    let mut saida = vec![0u8; bits.len().div_ceil(8)];
    for (indice, bit) in bits.iter().enumerate() {
        if *bit {
            saida[indice / 8] |= 1 << (indice % 8);
        }
    }
    saida
}

/// Desempacota o que [`empacota_bits`] escreveu.
fn desempacota_bits(bytes: &[u8], quantos: usize) -> Vec<bool> {
    (0..quantos)
        .map(|indice| bytes[indice / 8] & (1 << (indice % 8)) != 0)
        .collect()
}

/// As superfícies e as imagens decodificadas: as **duas famílias que guardam pixels**.
///
/// Elas são o que resta de conteúdo no motor depois das tabelas numéricas, e não dá para
/// re-derivá-las de nada: ao contrário de uma superfície do guest, que está na memória que entra no
/// estado, um `Framebuffer` é uma cópia do lado do host, criada por `Framebuffer::new` e preenchida
/// com o que o jogo desenhou. O mesmo vale para uma imagem já decodificada — o arquivo de origem
/// pode ter mudado no disco, e decodificar de novo daria outra coisa.
///
/// Cada superfície ocupa duas seções, com o identificador no nome: uma de números (tamanho, o que
/// já foi escrito e a caixa suja) e uma de pixels. É de propósito — assim os pixels ficam num bloco
/// corrido, e não misturados com números no meio.
impl<C: CpuBackend> Machine<C> {
    fn grava_superficies_e_imagens(&self, secoes: &mut Secoes) {
        secoes.poe_u32s("sup.ids", self.bitmaps.keys().copied());
        for (id, superficie) in &self.bitmaps {
            grava_superficie(secoes, &format!("sup.{id}"), superficie);
        }
        // A tela é uma superfície também, e a única que sempre existe: quem olha o quadro vê esta.
        grava_superficie(secoes, "sup.tela", &self.screen);

        secoes.poe_u32s("img.ids", self.images.keys().copied());
        for (id, imagem) in &self.images {
            secoes.poe_u32s(
                &format!("img.{id}.meta"),
                [
                    imagem.width,
                    imagem.height,
                    u32::from(imagem.frame_width),
                    imagem.pixels.len() as u32,
                    imagem.alfa.len() as u32,
                ],
            );
            secoes.poe(&format!("img.{id}.pixels"), pixels_em_bytes(&imagem.pixels));
            secoes.poe(
                &format!("img.{id}.opaque"),
                empacota_bits(&imagem.opaque),
            );
            secoes.poe(&format!("img.{id}.alfa"), imagem.alfa.clone());
        }
    }

    fn restaura_superficies_e_imagens(&mut self, leitor: &Leitor<'_>) -> Result<(), Erro> {
        let ids = leitor.u32s("sup.ids")?;
        let mut superficies = std::collections::HashMap::new();
        for id in ids {
            superficies.insert(id, le_superficie(leitor, &format!("sup.{id}"))?);
        }
        let tela = le_superficie(leitor, "sup.tela")?;

        let ids = leitor.u32s("img.ids")?;
        let mut imagens = std::collections::HashMap::new();
        for id in ids {
            let meta = leitor.u32s(&format!("img.{id}.meta"))?;
            if meta.len() != 5 {
                return Err(Erro::Secao {
                    nome: format!("img.{id}.meta"),
                    motivo: format!("esperava 5 números e veio {}", meta.len()),
                });
            }
            let (largura, altura, quadro) = (meta[0], meta[1], meta[2]);
            let quantos = (largura as usize) * (altura as usize);
            if meta[3] as usize != quantos {
                return Err(Erro::Secao {
                    nome: format!("img.{id}.meta"),
                    motivo: format!(
                        "a imagem é {largura}x{altura} e diz ter {} pixels",
                        meta[3]
                    ),
                });
            }
            let pixels = bytes_em_pixels(&secao_exigida(leitor, &format!("img.{id}.pixels"))?);
            if pixels.len() != quantos {
                return Err(Erro::Secao {
                    nome: format!("img.{id}.pixels"),
                    motivo: format!("esperava {quantos} pixels e veio {}", pixels.len()),
                });
            }
            let opacos = secao_exigida(leitor, &format!("img.{id}.opaque"))?;
            if opacos.len() < quantos.div_ceil(8) {
                return Err(Erro::Secao {
                    nome: format!("img.{id}.opaque"),
                    motivo: format!(
                        "esperava {} bytes de bits e veio {}",
                        quantos.div_ceil(8),
                        opacos.len()
                    ),
                });
            }
            let alfa = secao_exigida(leitor, &format!("img.{id}.alfa"))?;
            if alfa.len() != meta[4] as usize {
                return Err(Erro::Secao {
                    nome: format!("img.{id}.alfa"),
                    motivo: format!("esperava {} bytes de alfa e veio {}", meta[4], alfa.len()),
                });
            }
            imagens.insert(
                id,
                std::rc::Rc::new(DecodedImage {
                    width: largura,
                    height: altura,
                    pixels,
                    opaque: desempacota_bits(&opacos, quantos),
                    alfa,
                    frame_width: quadro as u16,
                }),
            );
        }

        self.bitmaps = superficies;
        self.screen = tela;
        self.images = imagens;
        Ok(())
    }
}

/// Grava uma superfície em duas seções: os números e os pixels.
fn grava_superficie(secoes: &mut Secoes, prefixo: &str, superficie: &Framebuffer) {
    let (escritas, serie) = (superficie.escritas(), superficie.serie());
    let sujo = superficie.sujeira().unwrap_or([0; 4]);
    secoes.poe_u32s(
        &format!("{prefixo}.meta"),
        [
            superficie.width(),
            superficie.height(),
            escritas as u32,
            (escritas >> 32) as u32,
            serie as u32,
            (serie >> 32) as u32,
            u32::from(superficie.sujeira().is_some()),
            sujo[0],
            sujo[1],
            sujo[2],
            sujo[3],
        ],
    );
    secoes.poe(&format!("{prefixo}.pixels"), pixels_em_bytes(superficie.pixels()));
}

/// Lê uma superfície gravada por [`grava_superficie`].
fn le_superficie(leitor: &Leitor<'_>, prefixo: &str) -> Result<Framebuffer, Erro> {
    let meta = leitor.u32s(&format!("{prefixo}.meta"))?;
    if meta.len() != 11 {
        return Err(Erro::Secao {
            nome: format!("{prefixo}.meta"),
            motivo: format!("esperava 11 números e veio {}", meta.len()),
        });
    }
    let (largura, altura) = (meta[0], meta[1]);
    let quantos = (largura as usize) * (altura as usize);
    let pixels = bytes_em_pixels(&secao_exigida(leitor, &format!("{prefixo}.pixels"))?);
    if pixels.len() != quantos {
        return Err(Erro::Secao {
            nome: format!("{prefixo}.pixels"),
            motivo: format!("a superfície é {largura}x{altura} e vieram {} pixels", pixels.len()),
        });
    }
    let mut superficie = Framebuffer::new(largura, altura);
    let escritas = u64::from(meta[2]) | (u64::from(meta[3]) << 32);
    let serie = u64::from(meta[4]) | (u64::from(meta[5]) << 32);
    let sujo = match meta[6] {
        0 => None,
        1 => Some([meta[7], meta[8], meta[9], meta[10]]),
        outro => {
            return Err(Erro::Secao {
                nome: format!("{prefixo}.meta"),
                motivo: format!("o campo da caixa suja vale {outro}, e é booleano"),
            })
        }
    };
    superficie.restaura_estado(escritas, serie, sujo, pixels);
    Ok(superficie)
}

/// Uma seção que tem de existir.
fn secao_exigida<'a>(leitor: &'a Leitor<'a>, nome: &str) -> Result<Vec<u8>, Erro> {
    leitor
        .secao(nome)
        .map(|bytes| bytes.to_vec())
        .ok_or_else(|| Erro::Secao {
            nome: nome.to_string(),
            motivo: "a seção não está no arquivo".to_string(),
        })
}

/// Pixels RGB565 como bytes, na ordem de leitura.
fn pixels_em_bytes(pixels: &[u16]) -> Vec<u8> {
    let mut saida = Vec::with_capacity(pixels.len() * 2);
    for pixel in pixels {
        saida.extend_from_slice(&pixel.to_le_bytes());
    }
    saida
}

/// O caminho de volta de [`pixels_em_bytes`]. Tamanho ímpar é recusa.
fn bytes_em_pixels(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|par| u16::from_le_bytes([par[0], par[1]]))
        .collect()
}


/// Os escalares, os conjuntos e os mapas simples.
///
/// É a parte do estado que **não tem forma própria**: um contador de segundos, o estado do gerador
/// aleatório, qual applet está rodando, quais módulos o shell tem instalados. Sozinhos, cada um
/// seria dez linhas de gravação e dez de leitura; juntos, são uma seção de números e um punhado de
/// mapas.
///
/// ## O que fica de fora, e é decisão
///
/// - `stalled`: o motor está **parado no meio de uma sondagem**. Gravar daqui e carregar depois
///   prometeria um estado que nunca existiu — não se sabe o que a sondagem tinha visto. Enquanto
///   ele não entrar, o estado não está completo, e a porta do `retro_serialize_size` continua
///   fechada. É o tipo de buraco que precisa ser dito, e não coberto;
/// - `serial`: o `BufWriter` de um arquivo aberto. Como os arquivos abertos, ele se refaz do disco
///   — mas o gravador de log não faz parte do jogo, e fica fora;
/// - `modulos_instalados` e `enumerations`: listas de **texto**. Precisam do mesmo tratamento de
///   texto que os caminhos, e ficam para a passada seguinte;
/// - o relógio em microssegundos (`clock_us`) **não** entra: ele é o contador de instruções
///   dividido pela taxa, e o contador já entra. Gravar os dois seria guardar a mesma coisa duas
///   vezes, com a chance de voltarem discordando;
/// - `z_wheel` é caminho de conteúdo, e quem o conhece é o core.
impl<C: CpuBackend> Machine<C> {
    fn grava_escalares_e_mapas(&self, secoes: &mut Secoes) {
        let mut numeros: Vec<u32> = vec![
            self.epoch_seconds,
            self.random_state,
            self.nesting,
            self.modo_do_sistema,
            self.device_bitmap,
            self.display_target,
            self.current_applet,
            self.applet_class,
            self.egl_next_handle,
            self.gles_next_name,
            self.egl_swaps,
            self.gl_clears,
            self.unpack_alignment,
            self.updates_na_volta as u32,
            u32::from(self.applet_fechado),
            u32::from(self.wheel_boot_skipped),
            // Booleanos de presença: 1 e 0, para o campo poder ser conferido na volta.
            u32::from(self.pending_launch.is_some()),
            u32::from(self.escritas_do_quadro_gl.is_some()),
            u32::from(self.scale_source.is_some()),
            u32::from(self.current_thread.is_some()),
            u32::from(self.ativacao_pendente.is_some()),
        ];
        numeros.push(self.next_vsync_us as u32);
        numeros.push((self.next_vsync_us >> 32) as u32);
        numeros.push(self.orcamento as u32);
        numeros.push((self.orcamento >> 32) as u32);
        numeros.push(self.proximo_serial as u32);
        numeros.push((self.proximo_serial >> 32) as u32);
        // E os valores dos que existem.
        numeros.push(self.pending_launch.unwrap_or(0));
        numeros.push(self.escritas_do_quadro_gl.unwrap_or(0) as u32);
        numeros.push((self.escritas_do_quadro_gl.unwrap_or(0) >> 32) as u32);
        numeros.push(self.scale_source.unwrap_or((0, 0)).0 as u32);
        numeros.push(self.scale_source.unwrap_or((0, 0)).1 as u32);
        numeros.push(self.current_thread.unwrap_or(0));
        numeros.push(self.ativacao_pendente.unwrap_or((0, 0)).0);
        numeros.push(self.ativacao_pendente.unwrap_or((0, 0)).1);
        secoes.poe_u32s("esc.numeros", numeros);

        let mut conjuntos: Vec<u32> = self.installed_applets.iter().copied().collect();
        conjuntos.sort_unstable();
        secoes.poe_u32s("esc.applets", conjuntos);
        let mut herdados: Vec<u32> = self.dib_herdados.iter().copied().collect();
        herdados.sort_unstable();
        secoes.poe_u32s("esc.dib_herdados", herdados);
        let mut avisando: Vec<u32> = self.widgets_avisando.iter().copied().collect();
        avisando.sort_unstable();
        secoes.poe_u32s("esc.widgets_avisando", avisando);

        secoes.poe_mapa("esc.mif", self.mif_no_guest.iter().map(|(a, b)| (*a, *b)));
        secoes.poe_u32s(
            "esc.ext_modules",
            self.ext_modules.iter().map(|m| m.unwrap_or(u32::MAX)),
        );
        secoes.poe_mapa(
            "esc.rolagem",
            self.rolagem_html.iter().map(|(a, b)| (*a, *b as u32)),
        );
        secoes.poe_mapa(
            "esc.rolagem_maxima",
            self.rolagem_maxima_html.iter().map(|(a, b)| (*a, *b as u32)),
        );
        secoes.poe_trios(
            "esc.image_notify",
            self.image_notify
                .iter()
                .map(|(id, cb)| (*id, cb.function, cb.context)),
        );
        secoes.poe_registros(
            "esc.dib_decodificador",
            self.dib_do_decodificador
                .iter()
                .map(|(id, (a, b))| vec![*id, *a, *b])
                .collect::<Vec<_>>(),
        );
        secoes.poe_registros(
            "esc.dib_publicado",
            self.dib_publicado
                .iter()
                .map(|(id, quando)| vec![*id, *quando as u32, (*quando >> 32) as u32])
                .collect::<Vec<_>>(),
        );
        // As listas de números por objeto: `vetores` e `collections`. Cada uma tem uma chave, um
        // bloco de números e um campo solto — o bloco vai como bytes de u32, e o campo na meta.
        secoes.poe_blocos(
            "esc.vetores",
            1,
            self.vetores
                .iter()
                .map(|(id, (valores, extra))| {
                    let mut bytes = Vec::with_capacity(4 + valores.len() * 4);
                    bytes.extend_from_slice(&(valores.len() as u32).to_le_bytes());
                    for valor in valores {
                        bytes.extend_from_slice(&valor.to_le_bytes());
                    }
                    bytes.extend_from_slice(&extra.to_le_bytes());
                    (vec![*id], bytes)
                })
                .collect::<Vec<_>>(),
        );
        secoes.poe_blocos(
            "esc.collections",
            1,
            self.collections
                .iter()
                .map(|(id, (valores, extra))| {
                    let mut bytes = Vec::with_capacity(4 + valores.len() * 4);
                    bytes.extend_from_slice(&(valores.len() as u32).to_le_bytes());
                    for valor in valores {
                        bytes.extend_from_slice(&valor.to_le_bytes());
                    }
                    bytes.extend_from_slice(&(*extra as u32).to_le_bytes());
                    (vec![*id], bytes)
                })
                .collect::<Vec<_>>(),
        );
    }


}

/// Quantos números a seção `esc.numeros` tem.
///
/// Está escrito aqui **e** conferido na leitura: um estado gravado com outro número de campos é
/// recusa, e não leitura deslocada. É o que obriga quem acrescentar um campo a mexer na conta — e
/// o teste abaixo cobra que ela esteja certa.
const ESCALARES: usize = 35;

impl<C: CpuBackend> Machine<C> {
    fn restaura_escalares_e_mapas(&mut self, leitor: &Leitor<'_>) -> Result<(), Erro> {
        let numeros = leitor.u32s("esc.numeros")?;
        if numeros.len() != ESCALARES {
            return Err(Erro::Secao {
                nome: "esc.numeros".to_string(),
                motivo: format!(
                    "o estado tem {} números e esta versão do motor usa {ESCALARES}",
                    numeros.len()
                ),
            });
        }
        let booleano = |indice: usize, campo: &str| -> Result<bool, Erro> {
            match numeros[indice] {
                0 => Ok(false),
                1 => Ok(true),
                outro => Err(Erro::Secao {
                    nome: "esc.numeros".to_string(),
                    motivo: format!("o campo `{campo}` vale {outro}, e é booleano"),
                }),
            }
        };
        let u64_de = |alto: usize| -> u64 {
            u64::from(numeros[alto]) | (u64::from(numeros[alto + 1]) << 32)
        };

        let epoch_seconds = numeros[0];
        let random_state = numeros[1];
        let nesting = numeros[2];
        let modo_do_sistema = numeros[3];
        let device_bitmap = numeros[4];
        let display_target = numeros[5];
        let current_applet = numeros[6];
        let applet_class = numeros[7];
        let egl_next_handle = numeros[8];
        let gles_next_name = numeros[9];
        let egl_swaps = numeros[10];
        let gl_clears = numeros[11];
        let unpack_alignment = numeros[12];
        let updates_na_volta = numeros[13] as usize;
        let applet_fechado = booleano(14, "applet_fechado")?;
        let wheel_boot_skipped = booleano(15, "wheel_boot_skipped")?;
        let tem_pending_launch = booleano(16, "pending_launch")?;
        let tem_escritas = booleano(17, "escritas_do_quadro_gl")?;
        let tem_scale = booleano(18, "scale_source")?;
        let tem_thread = booleano(19, "current_thread")?;
        let tem_ativacao = booleano(20, "ativacao_pendente")?;
        let next_vsync_us = u64_de(21);
        let orcamento = u64_de(23);
        let proximo_serial = u64_de(25);
        // **Os índices do fim são contados a partir do começo da lista**, e não do campo anterior:
        // foi aqui que eu errei a primeira vez, lendo o `pending_launch` na metade alta do
        // `proximo_serial`. O teste pegou porque o valor que voltou era o de outro campo.
        let pending_launch = tem_pending_launch.then_some(numeros[27]);
        let escritas_do_quadro_gl = tem_escritas.then(|| u64_de(28));
        let scale_source = tem_scale.then(|| (numeros[30] as i32, numeros[31] as i32));
        let current_thread = tem_thread.then_some(numeros[32]);
        let ativacao_pendente = tem_ativacao.then(|| (numeros[33], numeros[34]));

        // Os mapas e conjuntos, lidos antes de qualquer aplicação.
        let installed_applets: std::collections::HashSet<u32> =
            leitor.u32s("esc.applets")?.into_iter().collect();
        let dib_herdados: std::collections::HashSet<u32> =
            leitor.u32s("esc.dib_herdados")?.into_iter().collect();
        let widgets_avisando: std::collections::HashSet<u32> =
            leitor.u32s("esc.widgets_avisando")?.into_iter().collect();
        let mif_no_guest: std::collections::HashMap<u32, u32> =
            leitor.pares("esc.mif")?.into_iter().collect();
        let ext_modules: Vec<Option<u32>> = leitor
            .u32s("esc.ext_modules")?
            .into_iter()
            .map(|m| (m != u32::MAX).then_some(m))
            .collect();
        let rolagem_html: std::collections::HashMap<u32, usize> = leitor
            .pares("esc.rolagem")?
            .into_iter()
            .map(|(a, v)| (a, v as usize))
            .collect();
        let rolagem_maxima_html: std::collections::HashMap<u32, usize> = leitor
            .pares("esc.rolagem_maxima")?
            .into_iter()
            .map(|(a, v)| (a, v as usize))
            .collect();
        let image_notify: std::collections::HashMap<u32, Callback> = leitor
            .trios("esc.image_notify")?
            .into_iter()
            .map(|(id, function, context)| (id, Callback { function, context }))
            .collect();
        let mut dib_do_decodificador = std::collections::HashMap::new();
        for registro in leitor.registros("esc.dib_decodificador", 3)? {
            dib_do_decodificador.insert(registro[0], (registro[1], registro[2]));
        }
        let mut dib_publicado = std::collections::HashMap::new();
        for registro in leitor.registros("esc.dib_publicado", 3)? {
            dib_publicado.insert(
                registro[0],
                u64::from(registro[1]) | (u64::from(registro[2]) << 32),
            );
        }
        let mut vetores = std::collections::HashMap::new();
        for (chave, bytes) in leitor.blocos("esc.vetores")? {
            vetores.insert(chave[0], le_lista_com_extra(&bytes, "esc.vetores")?);
        }
        let mut collections = std::collections::HashMap::new();
        for (chave, bytes) in leitor.blocos("esc.collections")? {
            let (valores, extra) = le_lista_com_extra(&bytes, "esc.collections")?;
            collections.insert(chave[0], (valores, extra as usize));
        }

        // Aplicação.
        self.epoch_seconds = epoch_seconds;
        self.random_state = random_state;
        self.nesting = nesting;
        self.modo_do_sistema = modo_do_sistema;
        self.device_bitmap = device_bitmap;
        self.display_target = display_target;
        self.current_applet = current_applet;
        self.applet_class = applet_class;
        self.egl_next_handle = egl_next_handle;
        self.gles_next_name = gles_next_name;
        self.egl_swaps = egl_swaps;
        self.gl_clears = gl_clears;
        self.unpack_alignment = unpack_alignment;
        self.updates_na_volta = updates_na_volta;
        self.applet_fechado = applet_fechado;
        self.wheel_boot_skipped = wheel_boot_skipped;
        self.pending_launch = pending_launch;
        self.escritas_do_quadro_gl = escritas_do_quadro_gl;
        self.scale_source = scale_source;
        self.current_thread = current_thread;
        self.ativacao_pendente = ativacao_pendente;
        self.next_vsync_us = next_vsync_us;
        self.orcamento = orcamento;
        self.proximo_serial = proximo_serial;
        self.installed_applets = installed_applets;
        self.dib_herdados = dib_herdados;
        self.widgets_avisando = widgets_avisando;
        self.mif_no_guest = mif_no_guest;
        self.ext_modules = ext_modules;
        self.rolagem_html = rolagem_html;
        self.rolagem_maxima_html = rolagem_maxima_html;
        self.image_notify = image_notify;
        self.dib_do_decodificador = dib_do_decodificador;
        self.dib_publicado = dib_publicado;
        self.vetores = vetores;
        self.collections = collections;
        Ok(())
    }
}

/// Lê uma lista de números gravada como bytes, com o campo solto no fim.
fn le_lista_com_extra(bytes: &[u8], onde: &str) -> Result<(Vec<u32>, u32), Erro> {
    let malformada = |motivo: String| Erro::Secao {
        nome: onde.to_string(),
        motivo,
    };
    if bytes.len() < 8 {
        return Err(malformada(format!(
            "esperava a contagem e o campo solto, e tem {} bytes",
            bytes.len()
        )));
    }
    let quantos = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if bytes.len() != 4 + quantos * 4 + 4 {
        return Err(malformada(format!(
            "diz ter {quantos} valores ({} bytes com o campo solto) e tem {}",
            4 + quantos * 4 + 4,
            bytes.len()
        )));
    }
    let valores = bytes[4..4 + quantos * 4]
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let extra = u32::from_le_bytes([
        bytes[4 + quantos * 4],
        bytes[4 + quantos * 4 + 1],
        bytes[4 + quantos * 4 + 2],
        bytes[4 + quantos * 4 + 3],
    ]);
    Ok((valores, extra))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::unicorn::UnicornCpu;
    use crate::loader::self as loader;

    /// O menor módulo que o carregador aceita. Não precisa fazer nada: o alvo aqui é o estado da
    /// máquina em volta dele — memória, registradores e os contadores de alocação.
    fn modulo() -> crate::loader::modfile::ModImage {
        let code = [
            0xe3a0_0010u32.to_le_bytes(), // mov r0, #16
            0xe12f_ff1eu32.to_le_bytes(), // bx lr
        ]
        .concat();
        crate::loader::modfile::ModImage::parse(code).unwrap()
    }

    fn maquina() -> Machine<UnicornCpu> {
        let module = loader::load(&modulo()).unwrap();
        let mut machine = Machine::new(UnicornCpu::new().unwrap(), module, ".");
        machine.cpu.reset(&machine.module.mem).unwrap();
        machine
    }

    /// **As flags e o relógio voltam com o estado.**
    ///
    /// É o par que faz um save state "quase" funcionar quando falta: a memória volta, o programa
    /// volta, as flags ficam as de outra execução — a comparação que o jogo fez antes de salvar
    /// continua valendo e a decisão seguinte pode tomar o outro caminho. E o relógio é o tempo que
    /// o jogo enxerga: sem ele, os `SetTimer` e o áudio por quadro saem de outro instante.
    #[test]
    fn as_flags_e_o_relogio_vem_de_volta() {
        let mut antes = maquina();
        // O bit de negativo, para o valor ser diferente do que a máquina nova tem.
        let cpsr = antes.cpu.cpsr() | (1 << 31);
        antes.cpu.set_cpsr(cpsr);
        antes.cpu.set_instructions(12_345_678);

        let mut depois = maquina();
        assert_eq!(depois.cpu.instructions(), 0, "a máquina nova começa com o relógio zerado");

        let arquivo = antes.grava_estado();
        depois.restaura_estado(&arquivo).expect("restaurou");

        assert_eq!(depois.cpu.cpsr(), cpsr, "as flags não voltaram");
        assert_eq!(depois.cpu.instructions(), 12_345_678, "o relógio não voltou");
    }

    /// **O estado volta igual**: registradores, heap, pilha e o livro do heap.
    ///
    /// O heap é o caso interessante porque a memória dele *não* vai inteira: vai até o primeiro
    /// endereço nunca usado. Se o corte estiver errado, o que o jogo tinha escrito some — e um
    /// save state que perde os dados do jogo é pior que um save state que não existe.
    #[test]
    fn registradores_e_memoria_vem_de_volta() {
        let mut antes = maquina();
        let bloco = antes.heap.alloc(0x4000).expect("bloco no heap");
        let no_heap = bloco + 0x100;
        antes.cpu.write_mem(no_heap, b"estado do jogo").unwrap();
        let na_pilha = loader::STACK_BASE + 0x300;
        antes.cpu.write_mem(na_pilha, &[7u8; 16]).unwrap();
        antes.cpu.write_reg(Reg::R5, 0xdead_beef);
        antes.cpu.write_reg(Reg::R12, 0x1020_3040);

        let arquivo = antes.grava_estado();

        let mut depois = maquina();
        depois.restaura_estado(&arquivo).expect("restaurou");

        assert_eq!(depois.cpu.read_reg(Reg::R5), 0xdead_beef);
        assert_eq!(depois.cpu.read_reg(Reg::R12), 0x1020_3040);
        let mut lido = [0u8; 14];
        depois.cpu.read_mem(no_heap, &mut lido).unwrap();
        assert_eq!(&lido, b"estado do jogo");
        let mut pilha = [0u8; 16];
        depois.cpu.read_mem(na_pilha, &mut pilha).unwrap();
        assert_eq!(pilha, [7u8; 16]);

        // E a máquina restaurada aloca onde a original alocaria, como no teste do heap sozinho.
        let proximo_antes = antes.heap.alloc(64);
        let proximo_depois = depois.heap.alloc(64);
        assert_eq!(proximo_depois, proximo_antes);
    }

    /// **O heap volta com o `next` que o estado tinha**, e não com o da máquina de agora.
    ///
    /// É o contrário do que eu escrevi primeiro: eu tratei "o heap do estado é maior" como erro,
    /// e não é — é o caso normal. Quem carrega um save state carrega o heap que estava lá.
    #[test]
    fn o_heap_volta_com_o_tamanho_do_estado() {
        let mut antes = maquina();
        antes.heap.alloc(0x2000).expect("bloco no heap");
        let arquivo = antes.grava_estado();

        let mut depois = maquina();
        assert_eq!(depois.heap.proximo(), loader::HEAP_BASE, "a máquina nova começa vazia");
        depois.restaura_estado(&arquivo).expect("restaurou");
        assert_eq!(depois.heap.proximo(), antes.heap.proximo());
    }

    /// Um estado de **outro módulo** é recusado, e sem escrever nada.
    ///
    /// Aqui a máquina tem outro módulo, logo outra região `module`: o tamanho não bate, e o erro
    /// diz os dois números. Aplicar isso escreveria dados de um jogo por cima do código de outro.
    #[test]
    fn estado_de_outro_modulo_e_recusado() {
        let mut antes = maquina();
        antes.heap.alloc(0x1000).expect("bloco no heap");
        let arquivo = antes.grava_estado();

        let maior = [
            0xe3a0_0010u32.to_le_bytes(),
            0xe3a0_1010u32.to_le_bytes(),
            0xe3a0_2010u32.to_le_bytes(),
            0xe3a0_3010u32.to_le_bytes(),
            0xe12f_ff1eu32.to_le_bytes(),
        ]
        .concat();
        let module = loader::load(
            &crate::loader::modfile::ModImage::parse(maior).unwrap(),
        )
        .unwrap();
        let mut outra = Machine::new(UnicornCpu::new().unwrap(), module, ".");
        outra.cpu.reset(&outra.module.mem).unwrap();

        let proximo_antes = outra.heap.proximo();
        match outra.restaura_estado(&arquivo) {
            Err(Erro::Secao { nome, motivo }) => {
                assert_eq!(nome, "mem.module");
                assert!(motivo.contains("bytes desta região"), "{motivo}");
            }
            outro => panic!("devia recusar o estado de outro módulo, e devolveu {outro:?}"),
        }
        assert_eq!(
            outra.heap.proximo(),
            proximo_antes,
            "a recusa não pode ter aplicado metade"
        );
    }

    /// **O tamanho do estado, medido.** É o que decide se isso é usável: um save state que grava
    /// a memória inteira sairia com 84 MB, e o RetroArch grava um a cada atalho do jogador.
    ///
    /// O corte no heap é o que faz a diferença, e as regiões que ainda vão inteiras (objetos e
    /// superfícies) são a maior parte do que sobra. O teste imprime o número porque ele muda
    /// quando as próximas seções entrarem — e é bom que mude à vista.
    #[test]
    fn o_estado_nao_carrega_a_memoria_inteira() {
        let mut maquina = maquina();
        maquina.heap.alloc(64 * 1024).expect("bloco no heap");
        let arquivo = maquina.grava_estado();
        let mb = arquivo.len() as f64 / (1024.0 * 1024.0);
        eprintln!(
            "estado da máquina mínima: {} bytes ({mb:.2} MB); a memória mapeada é 84 MB",
            arquivo.len()
        );
        assert!(
            arquivo.len() < 24 * 1024 * 1024,
            "o estado passou de 24 MB: {} bytes",
            arquivo.len()
        );
    }

    #[test]
    fn um_arquivo_que_nao_e_save_state_e_recusado() {
        let mut maquina = maquina();
        assert!(matches!(
            maquina.restaura_estado(b"nao sou eu"),
            Err(Erro::Truncado { .. })
        ));
    }

    /// **Entrada e agendamento voltam inteiros.**
    ///
    /// São as tabelas que o jogo sente na hora: a fila de teclas que ele ainda não leu, a fila de
    /// eventos de botão, quem registrou aviso de aparelho, os temporizadores vencendo e os retornos
    /// pendentes. Sem elas o save state "volta" e o jogo perde o que já tinha na mão.
    #[test]
    fn entrada_e_agendamento_vem_de_volta() {
        use crate::input::avk;

        let mut antes = maquina();
        antes.set_key(avk::SELECT, true);
        antes.set_key(avk::ZERO, false);
        let mut pad = crate::input::Pad::default();
        pad.press(1, true);
        antes.set_port_pad(0, pad);
        antes
            .input_signals
            .insert(("RegisterForPositionChange", 0), 0x5555_0000);
        antes.timers.push(Timer {
            deadline_ms: 424_242,
            callback: Callback {
                function: 0x1000_2000,
                context: 0xdead_0000,
            },
        });
        antes.signals.insert(
            0x77,
            Callback {
                function: 0x1000_3000,
                context: 0xbeef_0000,
            },
        );
        antes.pending_signals.push(Callback {
            function: 0x1000_4000,
            context: 0x1234_0000,
        });

        let arquivo = antes.grava_estado();
        let mut depois = maquina();
        assert!(depois.teclas.is_empty(), "a máquina nova começa sem fila");
        depois.restaura_estado(&arquivo).expect("restaurou");

        assert_eq!(depois.teclas, antes.teclas, "a fila de teclas não voltou");
        assert_eq!(
            depois.pad_events[0], antes.pad_events[0],
            "a fila de eventos de botão não voltou"
        );
        assert_eq!(
            depois.input_signals.get(&("RegisterForPositionChange", 0)),
            Some(&0x5555_0000),
            "o registro do aviso de aparelho não voltou"
        );
        assert_eq!(depois.timers.len(), 1, "o temporizador não voltou");
        assert_eq!(depois.timers[0].deadline_ms, 424_242);
        assert_eq!(depois.timers[0].callback.function, 0x1000_2000);
        assert_eq!(depois.signals.get(&0x77).map(|c| c.context), Some(0xbeef_0000));
        assert_eq!(depois.pending_signals.len(), 1);
        assert_eq!(depois.pending_signals[0].function, 0x1000_4000);
    }

    /// O código do nome do sinal de aparelho vai e volta para **todos** os nomes conhecidos.
    ///
    /// É a mesma ideia do teste das 62 interfaces: nome novo sem entrada na tabela deixa isto
    /// vermelho, e não um registro que se perde em silêncio num save state.
    #[test]
    fn o_nome_do_sinal_vai_e_volta() {
        for nome in ["RegisterForButtonEvent", "RegisterForPositionChange"] {
            let codigo = codigo_do_sinal_de_entrada(nome);
            assert_ne!(codigo, u32::MAX, "{nome} não tem código");
            assert_eq!(nome_do_sinal_de_entrada(codigo), Some(nome));
        }
        assert_eq!(nome_do_sinal_de_entrada(9999), None);
        assert_eq!(codigo_do_sinal_de_entrada("inventado"), u32::MAX);
    }

    /// **As tabelas numéricas voltam**, e um valor fora da faixa do motor é recusado.
    ///
    /// A segunda metade é a que importa mais: um volume que não cabe em `u16` ou um `dono` que não
    /// é booleano é arquivo corrompido, e acomodar em silêncio seria carregar um estado que o jogo
    /// não escreveu.
    #[test]
    fn as_tabelas_numericas_vem_de_volta() {
        let mut antes = maquina();
        antes.dib_buffers.insert(0x11, 0x1000);
        antes.dib_capacity.insert(0x11, 640 * 480 * 2);
        antes.transformacoes.insert(0x22, 7);
        antes.canvases.insert(0x33, 0x2000);
        antes.feeds.insert(0x44, 12);
        antes.transparency.insert(0x55, 0x8000);
        antes.streams.insert(
            0x66,
            MemStream {
                buffer: 0x3000,
                size: 512,
                position: 128,
                dono: true,
            },
        );
        let mut info = [0u8; 5];
        info[1] = 9;
        antes.sounds.insert(
            0x77,
            SoundState {
                notify: Callback {
                    function: 0x1000_5000,
                    context: 0x7777,
                },
                info,
                volume: 42,
            },
        );

        let arquivo = antes.grava_estado();
        let mut depois = maquina();
        depois.restaura_estado(&arquivo).expect("restaurou");

        assert_eq!(depois.dib_capacity.get(&0x11), Some(&(640 * 480 * 2)));
        assert_eq!(depois.transformacoes.get(&0x22), Some(&7));
        assert_eq!(depois.canvases.get(&0x33), Some(&0x2000));
        assert_eq!(depois.feeds.get(&0x44), Some(&12));
        assert_eq!(depois.transparency.get(&0x55), Some(&0x8000));
        let stream = depois.streams.get(&0x66).expect("o stream voltou");
        assert_eq!((stream.buffer, stream.size, stream.position), (0x3000, 512, 128));
        assert!(stream.dono, "o `dono` do buffer não voltou");
        let som = depois.sounds.get(&0x77).expect("o som voltou");
        assert_eq!(som.volume, 42);
        assert_eq!(som.info[1], 9);
        assert_eq!(som.notify.function, 0x1000_5000);
    }

    /// Um volume que não cabe num `u16` é **recusado**, e não acomodado.
    #[test]
    fn som_com_volume_impossivel_e_recusado() {
        use crate::save_state::Secoes;

        let mut secoes = Secoes::nova();
        // As seções que o restaurador lê primeiro, vazias, e a dos sons com o valor impossível.
        for nome in [
            "tab.dib_buffers",
            "tab.dib_capacity",
            "tab.transformacoes",
            "tab.canvases",
            "tab.feeds",
            "tab.image_bitmaps",
            "tab.image_info",
            "tab.transparency",
            "tab.streams",
        ] {
            secoes.poe_u32s(nome, []);
        }
        // id, notify.function, notify.context, volume, cinco bytes de info
        secoes.poe_u32s("tab.sounds", [0x77u32, 0, 0, 999_999, 0, 0, 0, 0, 0]);
        let arquivo = secoes.fecha();
        let leitor = crate::save_state::Leitor::abre(&arquivo).expect("abriu");

        let mut maquina = maquina();
        match maquina.restaura_tabelas_numericas(&leitor) {
            Err(Erro::Secao { nome, motivo }) => {
                assert_eq!(nome, "tab.sounds");
                assert!(motivo.contains("999999"), "{motivo}");
            }
            outro => panic!("devia recusar o volume impossível, e devolveu {outro:?}"),
        }
    }

    /// **As métricas de fonte e os arquivos abertos voltam.**
    ///
    /// O caso do arquivo é o interessante: ele não guarda bytes, guarda **caminho e deslocamento**,
    /// e a volta reabre o arquivo pelo caminho e devolve o deslocamento. É o que faz um jogo que
    /// estava lendo no meio de um arquivo continuar lendo do mesmo ponto.
    #[test]
    fn fontes_e_arquivos_abertos_vem_de_volta() {
        use std::io::{Read as _, Write as _};

        // Um arquivo de verdade, em disco, com conteúdo conhecido.
        let caminho = std::env::temp_dir().join("zeebx-teste-do-estado.bin");
        let mut criado = std::fs::File::create(&caminho).expect("criar o arquivo de teste");
        let duzentos_e_cinquenta_e_seis: Vec<u8> = (0u8..=255).collect();
        criado
            .write_all(&duzentos_e_cinquenta_e_seis)
            .expect("escrever");
        drop(criado);

        let mut antes = maquina();
        antes.fontes.insert(
            0x1010,
            crate::machine::font::Metricas {
                ascent: 13,
                descent: 3,
                leading: 1,
                max_char_width: 9,
                height: 17,
                bold: true,
                italic: false,
            },
        );
        {
            let mut file = std::fs::File::open(&caminho).expect("abrir para ler");
            // O jogo já leu sete bytes deste arquivo.
            let mut lidos = [0u8; 7];
            file.read_exact(&mut lidos).expect("ler sete bytes");
            assert_eq!(lidos, [0u8, 1, 2, 3, 4, 5, 6]);
            antes.open_files.insert(
                0x2020,
                OpenFile {
                    file,
                    guest_path: "./dados/cena.bin".to_string(),
                    caminho: caminho.clone(),
                },
            );
        }

        let arquivo = antes.grava_estado();
        let mut depois = maquina();
        depois.restaura_estado(&arquivo).expect("restaurou");

        let metricas = depois.fontes.get(&0x1010).expect("as métricas voltaram");
        assert_eq!(
            (
                metricas.ascent,
                metricas.descent,
                metricas.leading,
                metricas.max_char_width,
                metricas.height
            ),
            (13, 3, 1, 9, 17)
        );
        assert!(metricas.bold);
        assert!(!metricas.italic);

        let aberto = depois.open_files.get_mut(&0x2020).expect("o arquivo voltou");
        assert_eq!(aberto.guest_path, "./dados/cena.bin");
        let mut seguinte = [0u8; 2];
        aberto.file.read_exact(&mut seguinte).expect("ler o seguinte");
        assert_eq!(
            seguinte,
            [7, 8],
            "o arquivo devia continuar do byte 7, e não do começo"
        );
        let _ = std::fs::remove_file(&caminho);
    }

    /// **As tabelas de conteúdo voltam** — preferências, parâmetros, `IConfig`, texto decifrado,
    /// resposta da rede.
    ///
    /// Elas guardam o conteúdo porque ele **não existe em lugar nenhum** fora do motor: ao contrário
    /// de um arquivo aberto, que se reabre do disco, um `SetPrefs` que o jogo fez já não está em
    /// disco nenhum.
    #[test]
    fn as_tabelas_de_conteudo_vem_de_volta() {
        let mut antes = maquina();
        antes.prefs.insert((0x0100_0001, 3), vec![1, 2, 3]);
        antes.prefs.insert((0x0100_0002, 0), Vec::new());
        antes
            .parametros_de_colecao
            .insert((0x10, 0x20), b"parametro".to_vec());
        antes.sources.insert(0x30, b"fonte".to_vec());
        antes.paginas_html.insert(0x40, b"<html>".to_vec());
        antes
            .config_items
            .entry(0x50)
            .or_default()
            .insert(0x60, b"valor".to_vec());
        antes.plaintexts.push_back(b"primeiro".to_vec());
        antes.plaintexts.push_back(b"segundo".to_vec());
        antes.web_response = b"resposta".to_vec();

        let arquivo = antes.grava_estado();
        let mut depois = maquina();
        depois.restaura_estado(&arquivo).expect("restaurou");

        assert_eq!(depois.prefs.get(&(0x0100_0001, 3)), Some(&vec![1, 2, 3]));
        assert_eq!(depois.prefs.get(&(0x0100_0002, 0)), Some(&Vec::new()));
        assert_eq!(
            depois.parametros_de_colecao.get(&(0x10, 0x20)),
            Some(&b"parametro".to_vec())
        );
        assert_eq!(depois.sources.get(&0x30), Some(&b"fonte".to_vec()));
        assert_eq!(depois.paginas_html.get(&0x40), Some(&b"<html>".to_vec()));
        assert_eq!(
            depois.config_items.get(&0x50).and_then(|i| i.get(&0x60)),
            Some(&b"valor".to_vec())
        );
        // A fila de texto decifrado volta **na ordem**, que é o conteúdo dela.
        let fila: Vec<&Vec<u8>> = depois.plaintexts.iter().collect();
        assert_eq!(
            fila,
            vec![&b"primeiro".to_vec(), &b"segundo".to_vec()],
            "a ordem do texto decifrado não voltou"
        );
        assert_eq!(depois.web_response, b"resposta".to_vec());
    }

    /// Um bloco com tamanho maior do que o arquivo tem é **recusado**.
    #[test]
    fn bloco_com_tamanho_mentiroso_e_recusado() {
        use crate::save_state::Secoes;

        let mut secoes = Secoes::nova();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u32.to_le_bytes()); // um item
        bytes.extend_from_slice(&2u32.to_le_bytes()); // chave de dois números
        bytes.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0]);
        bytes.extend_from_slice(&999u32.to_le_bytes()); // diz ter 999 bytes
        bytes.extend_from_slice(b"curto");
        secoes.poe("blocos", bytes);
        let arquivo = secoes.fecha();
        let leitor = crate::save_state::Leitor::abre(&arquivo).expect("abriu");
        match leitor.blocos("blocos") {
            Err(Erro::Secao { nome, motivo }) => {
                assert_eq!(nome, "blocos");
                assert!(motivo.contains("999"), "{motivo}");
            }
            outro => panic!("devia recusar o bloco mentiroso, e devolveu {outro:?}"),
        }
    }

    /// **As superfícies e as imagens decodificadas voltam** — os pixels, e o que a superfície já
    /// viveu (`escritas` e `serie`), que é por onde o frontend sabe o que mudou.
    #[test]
    fn superficies_e_imagens_vem_de_volta() {
        let mut antes = maquina();
        // Uma superfície de 4x2 com pixels distintos, para o teste distinguir pixel de pixel.
        let mut superficie = Framebuffer::new(4, 2);
        for (indice, valor) in [0x1111u16, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777, 0x8888]
            .iter()
            .enumerate()
        {
            superficie.set_pixel_native((indice % 4) as i32, (indice / 4) as i32, *valor);
        }
        // Escrever mexe no `touched` e na caixa suja, que é o que o frontend lê.
        assert!(superficie.is_dirty());
        antes.bitmaps.insert(0x9000, superficie);

        let mut imagem = DecodedImage {
            width: 2,
            height: 2,
            pixels: vec![0xaaaa, 0xbbbb, 0xcccc, 0xdddd],
            opaque: vec![true, false, true, true],
            alfa: vec![0, 51, 255, 7],
            frame_width: 1,
        };
        imagem.frame_width = 1;
        antes.images.insert(0xa000, std::rc::Rc::new(imagem));

        let arquivo = antes.grava_estado();
        let mut depois = maquina();
        depois.restaura_estado(&arquivo).expect("restaurou");

        let voltou = depois.bitmaps.get(&0x9000).expect("a superfície voltou");
        assert_eq!((voltou.width(), voltou.height()), (4, 2));
        assert_eq!(
            voltou.pixels(),
            &[0x1111u16, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777, 0x8888],
            "os pixels não voltaram"
        );
        let antes_da_gravacao = antes.bitmaps.get(&0x9000).expect("a original");
        assert_eq!(
            voltou.escritas(),
            antes_da_gravacao.escritas(),
            "o contador de escritas não voltou"
        );
        assert_eq!(
            voltou.serie(),
            antes_da_gravacao.serie(),
            "a série da superfície não voltou"
        );
        assert_eq!(
            voltou.sujeira(),
            antes_da_gravacao.sujeira(),
            "a caixa suja não voltou"
        );

        let imagem = depois.images.get(&0xa000).expect("a imagem voltou");
        assert_eq!((imagem.width, imagem.height), (2, 2));
        assert_eq!(imagem.pixels, vec![0xaaaa, 0xbbbb, 0xcccc, 0xdddd]);
        assert_eq!(imagem.opaque, vec![true, false, true, true], "o `opaque` não voltou");
        assert_eq!(imagem.alfa, vec![0, 51, 255, 7], "o alfa não voltou");
        assert_eq!(imagem.frame_width, 1);
    }

    /// A superfície volta **sem** perder a caixa suja: gravar um estado não pode consumir o aviso
    /// de que a tela mudou, senão o quadro seguinte sai velho por causa do save state.
    #[test]
    fn a_caixa_suja_nao_e_consumida_ao_gravar() {
        let mut maquina = maquina();
        let mut superficie = Framebuffer::new(2, 2);
        superficie.set_pixel_native(1, 1, 0x1234);
        let esperada = superficie.sujeira();
        assert!(esperada.is_some(), "desenhar devia sujar a superfície");
        maquina.bitmaps.insert(0x1000, superficie);

        let _ = maquina.grava_estado();
        assert_eq!(
            maquina.bitmaps.get(&0x1000).and_then(|s| s.sujeira()),
            esperada,
            "gravar o estado consumiu a caixa suja"
        );
    }

    /// Os bits do `opaque` vão empacotados e voltam na ordem certa.
    #[test]
    fn os_bits_do_opaque_vao_e_voltam() {
        for quantos in [0usize, 1, 7, 8, 9, 16, 17, 100] {
            let bits: Vec<bool> = (0..quantos).map(|i| i % 3 == 0).collect();
            let bytes = empacota_bits(&bits);
            assert_eq!(bytes.len(), quantos.div_ceil(8), "tamanho com {quantos} bits");
            assert_eq!(desempacota_bits(&bytes, quantos), bits, "com {quantos} bits");
        }
    }

    /// **Os escalares e os mapas simples voltam** — e a conta de campos está certa.
    ///
    /// A primeira metade deste teste é a que protege o formato: se alguém acrescentar um campo sem
    /// mexer na lista, a contagem deixa de bater e o teste fica vermelho **antes** de um save state
    /// sair deslocado.
    #[test]
    fn os_escalares_vem_de_volta_e_a_conta_esta_certa() {
        let antes = maquina();
        let mut secoes = Secoes::nova();
        antes.grava_escalares_e_mapas(&mut secoes);
        let arquivo = secoes.fecha();
        let leitor = crate::save_state::Leitor::abre(&arquivo).expect("abriu");
        assert_eq!(
            leitor.u32s("esc.numeros").expect("a seção").len(),
            ESCALARES,
            "a conta de campos está desatualizada: quem acrescentou um campo não mexeu em ESCALARES"
        );
    }

    #[test]
    fn escalares_e_mapas_vem_de_volta() {
        let mut antes = maquina();
        antes.epoch_seconds = 1_700_000_000;
        antes.random_state = 0x1234_5678;
        antes.nesting = 3;
        antes.current_applet = 0x0102_0304;
        antes.applet_class = 0x0102_0305;
        antes.next_vsync_us = 0x1_2345_6789;
        antes.orcamento = 0x9_8765_4321;
        antes.proximo_serial = 0x5_5555_5555;
        antes.applet_fechado = true;
        antes.wheel_boot_skipped = true;
        antes.pending_launch = Some(0x0102_8e35);
        antes.escritas_do_quadro_gl = Some(0x4_4444_4444);
        antes.scale_source = Some((-2, 7));
        antes.current_thread = Some(0x5151);
        antes.ativacao_pendente = Some((0x61, 0x62));
        antes.installed_applets.insert(0x1);
        antes.installed_applets.insert(0x2);
        antes.mif_no_guest.insert(0x70, 0x80);
        antes.ext_modules = vec![Some(0x90), None, Some(0xa0)];
        antes.rolagem_html.insert(0xb0, 12);
        antes.image_notify.insert(
            0xc0,
            Callback {
                function: 0x1000_6000,
                context: 0xd0,
            },
        );
        antes.dib_do_decodificador.insert(0xe0, (0xf0, 0x11));
        antes.dib_publicado.insert(0x12, 0x13_0000_0014);
        antes.vetores.insert(0x14, (vec![1, 2, 3], 9));
        antes.collections.insert(0x15, (vec![4, 5], 6));

        let arquivo = antes.grava_estado();
        let mut depois = maquina();
        depois.restaura_estado(&arquivo).expect("restaurou");

        assert_eq!(depois.epoch_seconds, 1_700_000_000);
        assert_eq!(depois.random_state, 0x1234_5678);
        assert_eq!(depois.nesting, 3);
        assert_eq!(depois.current_applet, 0x0102_0304);
        assert_eq!(depois.applet_class, 0x0102_0305);
        assert_eq!(depois.next_vsync_us, 0x1_2345_6789, "o u64 alto não voltou");
        assert_eq!(depois.orcamento, 0x9_8765_4321);
        assert_eq!(depois.proximo_serial, 0x5_5555_5555);
        assert!(depois.applet_fechado);
        assert!(depois.wheel_boot_skipped);
        assert_eq!(depois.pending_launch, Some(0x0102_8e35));
        assert_eq!(depois.escritas_do_quadro_gl, Some(0x4_4444_4444));
        assert_eq!(depois.scale_source, Some((-2, 7)));
        assert_eq!(depois.current_thread, Some(0x5151));
        assert_eq!(depois.ativacao_pendente, Some((0x61, 0x62)));
        assert_eq!(depois.installed_applets.len(), 2);
        assert_eq!(depois.mif_no_guest.get(&0x70), Some(&0x80));
        assert_eq!(depois.ext_modules, vec![Some(0x90), None, Some(0xa0)]);
        assert_eq!(depois.rolagem_html.get(&0xb0), Some(&12));
        assert_eq!(
            depois.image_notify.get(&0xc0).map(|c| c.function),
            Some(0x1000_6000)
        );
        assert_eq!(depois.dib_do_decodificador.get(&0xe0), Some(&(0xf0, 0x11)));
        assert_eq!(depois.dib_publicado.get(&0x12), Some(&0x13_0000_0014));
        assert_eq!(depois.vetores.get(&0x14), Some(&(vec![1, 2, 3], 9)));
        assert_eq!(depois.collections.get(&0x15), Some(&(vec![4, 5], 6)));
    }

    /// Um `pending_launch` ausente tem de voltar ausente — e não virar zero.
    #[test]
    fn a_ausencia_de_pending_launch_nao_vira_zero() {
        let mut antes = maquina();
        antes.pending_launch = None;
        let arquivo = antes.grava_estado();
        let mut depois = maquina();
        depois.pending_launch = Some(0x9999);
        depois.restaura_estado(&arquivo).expect("restaurou");
        assert_eq!(
            depois.pending_launch, None,
            "o estado disse que não havia pedido de abertura, e voltou um"
        );
    }
}
