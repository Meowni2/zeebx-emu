//! O estado da interface Qt: as configurações, a biblioteca, a entrada do desktop e o jogo aberto.
//!
//! **Mora num `thread_local`, e não num `QObject`.** O cxx-qt cria cada `QObject` a partir do QML,
//! sem argumentos, e a tela do jogo, a biblioteca e as configurações precisam ver o mesmo estado.
//! Tudo roda na thread da interface — o emulador também, ver `docs/implementacao/01-arquitetura.md`
//! —, então um `RefCell` basta; um `Mutex` seria sincronização sem ninguém do outro lado.
//!
//! **Nunca emitir sinal de dentro do [`com`].** Um sinal chama o QML na hora, o QML pode chamar de
//! volta um `QObject` que também entra no [`com`], e o segundo empréstimo do `RefCell` derruba o
//! programa. Quem chama pega o que precisa aqui dentro e emite depois.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use zeebx::eframe::egui::Key;
use zeebx::input;
use zeebx::library::{self, Game};
use zeebx::session::Z_WHEEL;
use zeebx::input::bindings::Aparelho;
use zeebx::ui::calibracao::{Calibracao, Situacao};
use zeebx::ui::depuracao;
use zeebx::ui::entrada::EntradaDoDesktop;
use zeebx::ui::i18n::{self, Catalog};
use zeebx::ui::partida::{Abertura, Partida, Saida};
use zeebx::ui::settings::{self, Proporcao, Settings};
use zeebx::video::rasterizer::QuadroNaPlaca;
use zeebx::video::display::Framebuffer;

use super::ponte::qobject as gl;

thread_local! {
    static NUCLEO: RefCell<Option<Nucleo>> = const { RefCell::new(None) };
}

/// Roda `f` com o estado da interface, criando-o na primeira vez.
pub fn com<T>(f: impl FnOnce(&mut Nucleo) -> T) -> T {
    NUCLEO.with(|nucleo| {
        let mut nucleo = nucleo.borrow_mut();
        f(nucleo.get_or_insert_with(Nucleo::novo))
    })
}

/// Solta o jogo aberto com o contexto de GL dele ainda vivo. O `launch` chama isto depois de a
/// engine sair e antes de destruir o contexto — ver a ordem descrita lá.
pub fn encerra() {
    NUCLEO.with(|nucleo| {
        if let Some(mut nucleo) = nucleo.borrow_mut().take() {
            nucleo.fecha();
        }
    });
}

/// O que uma volta do jogo pede à janela. Ver [`Nucleo::passo`].
#[derive(Default)]
pub struct Volta {
    /// O jogo saiu sozinho e não há para onde voltar: a janela do jogo fecha.
    pub fechou: bool,
    /// Outro jogo abriu nesta volta — pela Z-Wheel —, e a linha de estado muda: o título dele, ou
    /// por que não abriu.
    pub estado: Option<String>,
    /// Por que o jogo parou, já traduzido; vazio enquanto ele roda. Um jogo que parou continua com
    /// o último quadro à mostra, e o motivo precisa aparecer em algum lugar.
    pub parou: String,
    /// O aviso de calibração do Boomerang neste quadro, se há um. Ver [`zeebx::ui::calibracao`].
    pub aviso: Option<AvisoNaTela>,
}

/// O aviso de calibração já com os textos, como a janela o desenha.
pub struct AvisoNaTela {
    pub titulo: String,
    /// Vazio depois de concluído.
    pub estado: String,
    pub parado: bool,
    /// Em radianos.
    pub giro: f32,
}

/// O que a tela do jogo mostra. Ver [`Nucleo::quadro`].
pub enum Quadro<'a> {
    /// A textura do rasterizador na placa, na resolução interna, sem voltar à CPU.
    Placa(QuadroNaPlaca),
    /// A tela do console em RGB565 — o 2D, o 3D por software, ou a placa com algo desenhado por
    /// cima em 2D.
    Tela(&'a Framebuffer),
}

pub struct Nucleo {
    pub settings: Settings,
    pub catalogo: Catalog,
    entrada: EntradaDoDesktop,
    partida: Option<Partida>,
    /// O contexto que o rasterizador na placa recebe emprestado. Criado na primeira abertura, e não
    /// no arranque: precisa do `QGuiApplication` de pé. Com ele, tudo que toca a sessão — o passo,
    /// a abertura, e o fim de uma partida, que solta texturas — acontece com o contexto corrente.
    gl: Option<Arc<glow::Context>>,
    /// "software", ou a placa que o contexto achou: vai para a linha de estado.
    placa: Option<String>,
    /// Os jogos da pasta de ROMs, e quais deles vão para a lista — a Z-Wheel sai dela e abre pelo
    /// botão, como no egui.
    pub jogos: Vec<Game>,
    pub lista: Vec<usize>,
    pub z_wheel: Option<PathBuf>,
    calibracao: Calibracao,
    /// As teclas apertadas na janela do jogo, como `egui::Key`. Ver
    /// [`zeebx::ui::entrada::tecla_apertada`].
    teclas: HashSet<Key>,
}

impl Nucleo {
    fn novo() -> Self {
        let settings = Settings::load();
        let jogos = settings
            .roms_dir
            .as_deref()
            .map(library::scan)
            .unwrap_or_default();
        let lista = (0..jogos.len())
            .filter(|&i| jogos[i].clsid != Some(Z_WHEEL))
            .collect();
        let z_wheel = library::z_wheel_de(settings.z_wheel_path.as_deref(), &jogos);
        // O mesmo idioma do egui: o escolhido nas configurações, ou o do sistema.
        let mut catalogo = Catalog::new(&settings::language_dirs());
        match &settings.language {
            Some(codigo) => {
                catalogo.select(codigo);
            }
            None => {
                catalogo.select_best(&i18n::system_language());
            }
        }
        Self {
            settings,
            catalogo,
            entrada: EntradaDoDesktop::inicia(),
            partida: None,
            gl: None,
            placa: None,
            jogos,
            lista,
            z_wheel,
            calibracao: Calibracao::default(),
            teclas: HashSet::new(),
        }
    }

    /// O que a tela do jogo mostra agora.
    ///
    /// **A textura grande só quando é ela que está na tela**: o [`Session::quadro_na_placa`] a
    /// devolve apenas se nada foi desenhado em 2D depois do último `eglSwapBuffers`. Com HUD pelo
    /// `IDisplay`, caixa de mensagem ou a Z-Wheel, que compõe o 3D na CPU, vale a tela de 640×480.
    ///
    /// [`Session::quadro_na_placa`]: zeebx::session::Session::quadro_na_placa
    pub fn quadro(&self) -> Option<Quadro<'_>> {
        let sessao = self.partida.as_ref()?.sessao();
        let placa = self.gl.as_ref().and(sessao.quadro_na_placa());
        Some(match placa {
            Some(quadro) => Quadro::Placa(quadro),
            None => Quadro::Tela(sessao.screen()),
        })
    }

    /// O título do jogo aberto e onde ele desenha.
    pub fn estado(&self) -> String {
        let Some(titulo) = self.titulo() else {
            return String::new();
        };
        let onde = match (&self.placa, self.settings.graphics.gpu_rasterizer) {
            (Some(placa), true) => format!("placa: {placa}"),
            _ => "software".to_string(),
        };
        format!("{titulo} — {onde}")
    }

    /// O nome do jogo aberto.
    ///
    /// A sessão só conhece a pasta de extração, que leva a impressão digital do pacote
    /// (`Zeebo-Extreme-Boia-Cross-21503726-1788761080`). O nome sai da biblioteca pelo ClassID, e
    /// só na falta dela a pasta, sem os dois números do fim — a regra do egui. O nome oficial da
    /// Z-Wheel, que o egui prefere quando ela descreve o jogo, entra com o acervo na fase 4.
    pub fn titulo(&self) -> Option<String> {
        let sessao = self.partida.as_ref()?.sessao();
        let classe = sessao.classe();
        if let Some(jogo) = self.jogos.iter().find(|jogo| jogo.clsid == Some(classe)) {
            return Some(jogo.title.clone());
        }
        let titulo = library::sem_impressao_digital(sessao.title());
        (!titulo.is_empty()).then_some(titulo)
    }

    /// Abre um jogo escolhido na janela principal. Não foi a Z-Wheel quem pediu: quando ele sair
    /// sozinho, a janela do jogo fecha em vez de voltar para ela.
    pub fn abre_pela_biblioteca(&mut self, caminho: &Path) -> Result<(), String> {
        let aberta = self.abre(caminho);
        if let Some(partida) = self.partida.as_mut() {
            partida.aberta_pela_z_wheel = false;
        }
        aberta
    }

    /// Troca a partida pela do jogo em `caminho`. Falhando, a de antes continua, como no egui.
    fn abre(&mut self, caminho: &Path) -> Result<(), String> {
        if self.gl.is_none() {
            match contexto_do_qt() {
                Ok((contexto, placa)) => {
                    self.gl = Some(contexto);
                    self.placa = Some(placa);
                }
                Err(erro) => eprintln!("{erro} — seguindo no rasterizador de software"),
            }
        }
        self.teclas.clear();
        // As contagens de calibração são da sessão: a nova começa do zero.
        self.calibracao.reinicia();
        if let Some(partida) = self.partida.as_mut() {
            partida.esquece_entrada();
        }
        let abertura = Abertura {
            settings: &self.settings,
            serial: None,
            gl: self.gl.clone(),
            instalados: self
                .jogos
                .iter()
                .filter_map(|jogo| Some((jogo.clsid?, library::id_do_modulo(&jogo.path)?)))
                .collect(),
        };
        let anterior = self.partida.as_ref();
        let com_placa = self.gl.is_some();
        let partida = no_contexto(com_placa, || Partida::abre(caminho, abertura, anterior))
            .map_err(|erro| erro.to_string())?;
        // A de antes sai aqui, e com o contexto corrente: as texturas dela moram nele.
        no_contexto(com_placa, || self.partida = Some(partida));
        Ok(())
    }

    /// Solta a partida com o contexto dela corrente: soltar texturas com o contexto do Qt Quick
    /// corrente apagaria as dele.
    pub fn fecha(&mut self) {
        if self.partida.is_some() {
            no_contexto(self.gl.is_some(), || self.partida = None);
        }
        self.teclas.clear();
    }

    /// O painel de depuração, quando ligado: os textos e a linha do tempo, como `(velocidade,
    /// quadros)` da amostra mais antiga para a mais nova. Ver [`zeebx::ui::depuracao`].
    pub fn depuracao(&self) -> Option<(Vec<String>, Vec<(u32, u32)>)> {
        let debug = self.settings.debug;
        let sessao = self.partida.as_ref()?.sessao();
        if !debug.overlay {
            return None;
        }
        let textos = depuracao::Textos::novos(
            &self.catalogo,
            debug,
            sessao.sample(),
            sessao.memory(),
            sessao.clock_ms(),
        );
        let textos = [textos.velocidade, textos.relogio, textos.memoria]
            .into_iter()
            .flatten()
            .collect();
        let historia = match debug.timeline {
            true => sessao.history().map(|a| (a.speed, a.fps)).collect(),
            false => Vec::new(),
        };
        Some((textos, historia))
    }

    pub fn alterna_pausa(&mut self) {
        if let Some(partida) = self.partida.as_mut() {
            partida.pausada = !partida.pausada;
        }
    }

    pub fn pausada(&self) -> bool {
        self.partida.as_ref().is_some_and(|partida| partida.pausada)
    }

    /// Uma tecla da janela do jogo. Cada evento vai à partida na hora: ver
    /// [`Partida::teclado_mudou`].
    pub fn tecla(&mut self, tecla: Key, apertada: bool) {
        match apertada {
            true => self.teclas.insert(tecla),
            false => self.teclas.remove(&tecla),
        };
        if let Some(partida) = self.partida.as_mut() {
            partida.teclado_mudou(self.teclas.iter().filter_map(|tecla| input::avk_de(*tecla)));
        }
    }

    /// A janela do jogo perdeu o foco: nenhuma tecla continua apertada.
    pub fn solta_teclas(&mut self) {
        self.teclas.clear();
    }

    /// Uma volta: entrada, emulação, e o que o jogo pediu depois — lançar outro, voltar à Z-Wheel,
    /// fechar.
    ///
    /// `area` é o tamanho da tela do jogo na janela, para a proporção "da janela".
    pub fn passo(&mut self, area: [f32; 2]) -> Volta {
        let Some(partida) = self.partida.as_mut() else {
            return Volta::default();
        };
        let pads = self.entrada.pads(&self.settings.controls, &self.teclas);
        let movimentos = self.entrada.movimentos(&self.settings.controls);
        let teclado = self.teclas.iter().filter_map(|tecla| input::avk_de(*tecla));
        let limite = self.settings.graphics.speed_limit;
        // A proporção "da janela" acompanha o tamanho dela; o destino só é refeito quando as
        // colunas a mais mudam. É GL, então vai com o contexto corrente.
        let da_janela = (self.settings.graphics.proporcao == Proporcao::Janela)
            .then(|| area[0] / area[1].max(1.0));
        let contexto = self.gl.clone();
        no_contexto(contexto.is_some(), || {
            if da_janela.is_some() {
                partida.sessao_mut().define_proporcao(da_janela);
            }
            partida.avanca(&pads, movimentos, teclado, limite);
            // **O que este contexto desenhou só fica garantido para o do Qt Quick depois de um
            // `glFinish`.** A textura do quadro é escrita aqui e lida lá, e a especificação do
            // OpenGL só promete a mudança visível a outro contexto quando o primeiro terminou o
            // trabalho. O `eglSwapBuffers` do jogo já lê o quadro de volta para a CPU, o que
            // espera a placa do mesmo jeito; o custo que sobra para este não foi medido.
            if let Some(contexto) = &contexto {
                use glow::HasContext;
                unsafe { contexto.finish() };
            }
        });

        // O pedido de lançar vem antes da saída: ver [`Partida::pedido_de_lancamento`].
        let lancado = partida.pedido_de_lancamento().and_then(|classe| {
            self.jogos
                .iter()
                .find(|jogo| jogo.clsid == Some(classe))
                .map(|jogo| jogo.path.clone())
        });
        let (proximo, pela_z_wheel) = match (lancado, partida.saida()) {
            (Some(caminho), _) => (Some(caminho), true),
            (None, Saida::Segue) => (None, false),
            (None, Saida::ReabreZWheel) if self.z_wheel.is_some() => (self.z_wheel.clone(), false),
            (None, Saida::ReabreZWheel | Saida::Fecha) => {
                self.fecha();
                return Volta {
                    fechou: true,
                    ..Volta::default()
                };
            }
        };
        let mut volta = Volta::default();
        if let Some(caminho) = proximo {
            volta.estado = Some(match self.abre(&caminho) {
                Ok(()) => self.estado(),
                Err(erro) => format!("não abriu: {erro}"),
            });
            if let Some(partida) = self.partida.as_mut() {
                partida.aberta_pela_z_wheel |= pela_z_wheel;
                partida.reinicia_relogio();
            }
        }
        if let Some(motivo) = self.partida.as_ref().and_then(|p| p.sessao().stopped_reason()) {
            volta.parou = self.catalogo.format("play.failed", &[("reason", &motivo)]);
        }
        volta.aviso = self.aviso_de_calibracao();
        volta
    }

    /// O aviso de calibração neste quadro, com os mesmos textos do egui.
    fn aviso_de_calibracao(&mut self) -> Option<AvisoNaTela> {
        let calibracao = self.partida.as_ref()?.sessao().calibracao();
        let controles = &self.settings.controls;
        let porta = controles
            .ligadas()
            .find(|(_, jogador)| jogador.aparelho == Aparelho::Boomerang)
            .map(|(indice, _)| indice);
        let leitura = porta.map(|porta| {
            let movimento = self.entrada.movimento_da_porta(controles, porta);
            let com_sensor = self.entrada.movimento_bruto_da_porta(controles, porta).is_some();
            (movimento, com_sensor)
        });
        let ligado = self.settings.movimento.aviso_de_calibracao;
        let vista = self.calibracao.quadro(calibracao, leitura, ligado, Instant::now())?;
        let porta = porta?;
        let chave = match vista.concluido {
            true => "calibration.toast.done",
            false => "calibration.toast.title",
        };
        let estado = match vista.situacao {
            None => String::new(),
            Some(Situacao::SemSensor) => self
                .entrada
                .sensor_da_porta(controles, porta)
                .descreve(&self.catalogo),
            Some(Situacao::Parado) => self.catalogo.get("calibration.toast.still").to_string(),
            Some(Situacao::Mexendo) => self.catalogo.get("calibration.toast.moving").to_string(),
        };
        Some(AvisoNaTela {
            titulo: self.catalogo.get(chave).to_string(),
            estado,
            parado: vista.situacao == Some(Situacao::Parado),
            giro: vista.giro,
        })
    }
}

/// Roda `f` com o contexto do rasterizador corrente, quando há um.
fn no_contexto<T>(com_placa: bool, f: impl FnOnce() -> T) -> T {
    let corrente = com_placa && gl::gl_torna_corrente();
    let resultado = f();
    if corrente {
        gl::gl_solta();
    }
    resultado
}

/// O contexto fora de tela, pronto para o `glow`, e o nome da placa. `Err` diz por que não deu.
fn contexto_do_qt() -> Result<(Arc<glow::Context>, String), String> {
    if !gl::gl_cria() {
        return Err("o Qt não criou o contexto fora de tela".into());
    }
    if !gl::gl_torna_corrente() {
        return Err("o contexto fora de tela não ficou corrente".into());
    }
    let contexto = unsafe {
        glow::Context::from_loader_function_cstr(|nome| {
            nome.to_str().map_or(0, gl::gl_funcao) as *const std::ffi::c_void
        })
    };
    let placa = {
        use glow::HasContext;
        unsafe { contexto.get_parameter_string(glow::RENDERER) }
    };
    gl::gl_solta();
    Ok((Arc::new(contexto), placa))
}
