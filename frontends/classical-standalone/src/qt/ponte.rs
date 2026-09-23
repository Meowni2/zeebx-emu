//! Os `QObject`s escritos em Rust, e a cola em C++ que eles chamam.
//!
//! Por enquanto é só a tela do jogo, e ainda pelo caminho mais simples: o quadro RGB565 da
//! sessão pintado por um `QQuickPaintedItem`. Abrir, rodar e trocar de jogo é a
//! [`zeebx::ui::partida::Partida`], a mesma da janela do egui. O `QImage::Format_RGB16` **é** RGB565, então a
//! CPU não converte nada — a mesma regra do egui de subir o quadro só quando a tela mudou vale
//! aqui.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;
        include!("cxx-qt-lib/qsizef.h");
        type QSizeF = cxx_qt_lib::QSizeF;
    }

    unsafe extern "C++" {
        include!(<QtQuick/QQuickPaintedItem>);
        type QQuickPaintedItem;
    }

    // Ver `cpp/gl_qt.h`.
    #[namespace = "zeebx"]
    unsafe extern "C++" {
        include!("gl_qt.h");
        fn prepara_gl();
        fn gl_cria() -> bool;
        fn gl_torna_corrente() -> bool;
        fn gl_solta();
        fn gl_destroi();
        fn gl_funcao(nome: &str) -> usize;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(QString, estado)]
        #[qproperty(bool, suave)]
        type TelaDoJogo = super::TelaDoJogoRust;

        #[qinvokable]
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut Self>, painter: *mut QPainter);

        /// Começa o jogo do caminho dado; `placa` liga o rasterizador na placa.
        #[qinvokable]
        fn abre(self: Pin<&mut Self>, caminho: &QString, placa: bool);

        /// Uma volta: entrada, emulação e, se a tela mudou, um quadro novo.
        #[qinvokable]
        fn passo(self: Pin<&mut Self>);

        /// Um `Qt::Key` apertado ou solto.
        #[qinvokable]
        fn tecla(self: Pin<&mut Self>, codigo: i32, apertada: bool);

        /// A janela perdeu o foco: nenhuma tecla continua apertada.
        #[qinvokable]
        fn solta(self: Pin<&mut Self>);

        /// O jogo saiu sozinho e não há para onde voltar: a janela fecha, como a do egui.
        #[qsignal]
        fn fechou(self: Pin<&mut Self>);

        #[inherit]
        fn size(self: &Self) -> QSizeF;

        #[inherit]
        fn update(self: Pin<&mut Self>);
    }

    // Sem isto o construtor gerado repassa o pai `QObject*` à base, que quer um `QQuickItem*`.
    impl cxx_qt::Initialize for TelaDoJogo {}
}

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QImage, QImageFormat, QPainterRenderHint, QRect, QString};

use zeebx::input;
use zeebx::eframe::egui::Key;
use zeebx::library::{self, Game};
use zeebx::ui::entrada::EntradaDoDesktop;
use zeebx::ui::partida::{Abertura, Partida, Saida};
use zeebx::ui::settings::Settings;

pub struct TelaDoJogoRust {
    estado: QString,
    suave: bool,
    settings: Settings,
    /// Os controles do host, os Wii Remotes e os sensores: a mesma leitura da janela do egui.
    entrada: EntradaDoDesktop,
    partida: Option<Partida>,
    /// O contexto que o rasterizador na placa recebe emprestado; `None` é o de software. Com ele,
    /// tudo que toca a sessão — o passo, a abertura, e o fim de uma partida, que solta texturas —
    /// acontece com o contexto corrente.
    gl: Option<Arc<glow::Context>>,
    /// "software", ou a placa que o contexto achou: vai para a linha de estado.
    onde: String,
    /// Os jogos da pasta de ROMs: é deles que a Z-Wheel pede para lançar.
    jogos: Vec<Game>,
    z_wheel: Option<PathBuf>,
    /// As teclas apertadas, como `egui::Key`: é contra ele que o mapeamento é conferido. Ver
    /// [`tecla_mapeada`].
    teclas: HashSet<Key>,
    quadro: Option<QImage>,
    chave: Option<(u64, u64)>,
}

impl Default for TelaDoJogoRust {
    fn default() -> Self {
        let settings = Settings::load();
        let jogos = settings
            .roms_dir
            .as_deref()
            .map(library::scan)
            .unwrap_or_default();
        let z_wheel = library::z_wheel_de(settings.z_wheel_path.as_deref(), &jogos);
        Self {
            estado: QString::default(),
            suave: settings.graphics.smooth,
            settings,
            entrada: EntradaDoDesktop::inicia(),
            partida: None,
            gl: None,
            onde: "software".into(),
            jogos,
            z_wheel,
            teclas: HashSet::new(),
            quadro: None,
            chave: None,
        }
    }
}

/// Roda `f` com o contexto do rasterizador corrente, quando há um.
fn no_contexto<T>(com_placa: bool, f: impl FnOnce() -> T) -> T {
    let corrente = com_placa && qobject::gl_torna_corrente();
    let resultado = f();
    if corrente {
        qobject::gl_solta();
    }
    resultado
}

impl TelaDoJogoRust {
    /// Troca a partida pela do jogo em `caminho`. Falhando, a de antes continua, como no egui.
    fn abre_partida(&mut self, caminho: &Path) -> Result<(), String> {
        self.teclas.clear();
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
        let aberta = no_contexto(self.gl.is_some(), || Partida::abre(caminho, abertura, anterior));
        let partida = aberta.map_err(|erro| erro.to_string())?;
        // A de antes sai aqui, e com o contexto corrente: as texturas dela moram nele.
        no_contexto(self.gl.is_some(), || self.partida = Some(partida));
        self.quadro = None;
        self.chave = None;
        Ok(())
    }

    fn estado_do_jogo(&self) -> String {
        match &self.partida {
            Some(partida) => format!("{} — {}", partida.sessao().title(), self.onde),
            None => String::new(),
        }
    }

    /// Solta a partida com o contexto dela corrente: soltar texturas com o contexto do Qt Quick
    /// corrente apagaria as dele.
    fn fecha(&mut self) {
        if self.partida.is_some() {
            no_contexto(self.gl.is_some(), || self.partida = None);
        }
    }
}

impl Drop for TelaDoJogoRust {
    fn drop(&mut self) {
        self.fecha();
    }
}

/// O contexto fora de tela, pronto para o `glow`. `Err` diz por que não deu.
fn contexto_do_qt() -> Result<Arc<glow::Context>, String> {
    if !qobject::gl_cria() {
        return Err("o Qt não criou o contexto fora de tela".into());
    }
    if !qobject::gl_torna_corrente() {
        return Err("o contexto fora de tela não ficou corrente".into());
    }
    let gl = unsafe {
        glow::Context::from_loader_function_cstr(|nome| {
            nome.to_str()
                .map_or(0, qobject::gl_funcao) as *const std::ffi::c_void
        })
    };
    qobject::gl_solta();
    Ok(Arc::new(gl))
}

impl qobject::TelaDoJogo {
    pub fn abre(mut self: Pin<&mut Self>, caminho: &QString, placa: bool) {
        let caminho = PathBuf::from(String::from(caminho));
        let estado = {
            let mut rust = self.as_mut().rust_mut();
            let rust = &mut *rust;
            rust.fecha();
            if placa && rust.gl.is_none() {
                match contexto_do_qt() {
                    Ok(gl) => {
                        use glow::HasContext;
                        let placa = no_contexto(true, || unsafe {
                            gl.get_parameter_string(glow::RENDERER)
                        });
                        rust.onde = format!("placa: {placa}");
                        rust.gl = Some(gl);
                    }
                    Err(erro) => eprintln!("{erro} — seguindo no rasterizador de software"),
                }
            }
            match rust.abre_partida(&caminho) {
                Ok(()) => rust.estado_do_jogo(),
                Err(erro) => format!("não abriu: {erro}"),
            }
        };
        self.set_estado(QString::from(&estado));
    }

    pub fn passo(mut self: Pin<&mut Self>) {
        let (mudou, estado, fechou) = {
            let mut rust = self.as_mut().rust_mut();
            let rust = &mut *rust;
            let Some(partida) = rust.partida.as_mut() else {
                return;
            };
            let pads = rust.entrada.pads(&rust.settings.controls, &rust.teclas);
            let movimentos = rust.entrada.movimentos(&rust.settings.controls);
            let teclado = rust.teclas.iter().filter_map(|tecla| input::avk_de(*tecla));
            let limite = rust.settings.graphics.speed_limit;
            no_contexto(rust.gl.is_some(), || {
                partida.avanca(&pads, movimentos, teclado, limite);
            });

            // O pedido de lançar vem antes da saída: ver [`Partida::pedido_de_lancamento`].
            let lancado = partida.pedido_de_lancamento().and_then(|classe| {
                rust.jogos
                    .iter()
                    .find(|jogo| jogo.clsid == Some(classe))
                    .map(|jogo| jogo.path.clone())
            });
            let (proximo, pela_z_wheel, fechou) = match (lancado, partida.saida()) {
                (Some(caminho), _) => (Some(caminho), true, false),
                (None, Saida::Segue) => (None, false, false),
                (None, Saida::ReabreZWheel) if rust.z_wheel.is_some() => {
                    (rust.z_wheel.clone(), false, false)
                }
                (None, Saida::ReabreZWheel | Saida::Fecha) => (None, false, true),
            };
            let mut estado = None;
            if let Some(caminho) = proximo {
                match rust.abre_partida(&caminho) {
                    Ok(()) => estado = Some(rust.estado_do_jogo()),
                    Err(erro) => estado = Some(format!("não abriu: {erro}")),
                }
                if let Some(partida) = rust.partida.as_mut() {
                    partida.aberta_pela_z_wheel |= pela_z_wheel;
                    partida.reinicia_relogio();
                }
            }
            if fechou {
                rust.fecha();
            }

            let mut mudou = false;
            if let Some(partida) = &rust.partida {
                let tela = partida.sessao().screen();
                let chave = (tela.serie(), tela.escritas());
                if rust.chave != Some(chave) {
                    rust.quadro = Some(imagem_rgb565(
                        &tela.to_rgb565_bytes(),
                        tela.width() as usize,
                        tela.height() as usize,
                    ));
                    rust.chave = Some(chave);
                    mudou = true;
                }
                if estado.is_none() {
                    estado = partida.sessao().stopped_reason();
                }
            }
            (mudou, estado, fechou)
        };
        if let Some(estado) = estado.map(|texto| QString::from(&texto)) {
            if self.estado() != &estado {
                self.as_mut().set_estado(estado);
            }
        }
        if mudou {
            self.as_mut().update();
        }
        if fechou {
            self.fechou();
        }
    }
}

impl cxx_qt::Initialize for qobject::TelaDoJogo {
    fn initialize(self: Pin<&mut Self>) {
        // Trocar a suavização repinta o quadro que já está na tela.
        self.on_suave_changed(|tela| tela.update()).release();
    }
}

impl qobject::TelaDoJogo {
    pub fn tecla(mut self: Pin<&mut Self>, codigo: i32, apertada: bool) {
        let Some(tecla) = tecla_do_qt(codigo) else {
            return;
        };
        let mut rust = self.as_mut().rust_mut();
        let rust = &mut *rust;
        match apertada {
            true => rust.teclas.insert(tecla),
            false => rust.teclas.remove(&tecla),
        };
        // Cada evento vai à partida na hora: ver [`Partida::teclado_mudou`].
        if let Some(partida) = rust.partida.as_mut() {
            partida.teclado_mudou(rust.teclas.iter().filter_map(|tecla| input::avk_de(*tecla)));
        }
    }

    pub fn solta(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().teclas.clear();
    }

    /// O quadro centrado e ampliado para caber, na proporção dele.
    ///
    /// # Safety
    ///
    /// `painter` vem do Qt, válido durante a chamada.
    pub unsafe fn paint(self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        let Some(painter) = (unsafe { painter.as_mut() }) else {
            return;
        };
        let mut painter = unsafe { Pin::new_unchecked(painter) };
        let Some(quadro) = self.rust().quadro.as_ref() else {
            return;
        };
        let area = self.size();
        let (largura, altura) = (quadro.width() as f64, quadro.height() as f64);
        let escala = (area.width() / largura).min(area.height() / altura);
        let (w, h) = (largura * escala, altura * escala);
        let destino = QRect::new(
            ((area.width() - w) / 2.0) as i32,
            ((area.height() - h) / 2.0) as i32,
            w as i32,
            h as i32,
        );
        painter
            .as_mut()
            .set_render_hint(QPainterRenderHint::SmoothPixmapTransform, *self.suave());
        painter.as_mut().draw_image(&destino, quadro);
    }
}

/// O `QImage` construído sobre bytes próprios exige linhas alinhadas a quatro bytes. Uma
/// largura ímpar em RGB565 não é: a linha ganha dois bytes de enchimento.
fn imagem_rgb565(bytes: &[u8], largura: usize, altura: usize) -> QImage {
    let linha = largura * 2;
    let passo = (linha + 3) & !3;
    let dados = match passo == linha {
        true => bytes.to_vec(),
        false => {
            let mut dados = vec![0u8; passo * altura];
            for (destino, origem) in dados.chunks_exact_mut(passo).zip(bytes.chunks_exact(linha)) {
                destino[..linha].copy_from_slice(origem);
            }
            dados
        }
    };
    // O `Format_RGB16` é o `u16` na ordem da máquina; o framebuffer entrega little-endian, que
    // é a ordem dos três sistemas suportados.
    unsafe { QImage::from_raw_bytes(dados, largura as i32, altura as i32, QImageFormat::Format_RGB16) }
}

/// O nome que o egui dá à tecla, a partir do `Qt::Key`. É o nome que está gravado no
/// `settings.json`: mudar de interface não pode desfazer o mapeamento de ninguém.
/// O `egui::Key` de um `Qt::Key`, passando pelo nome.
fn tecla_do_qt(codigo: i32) -> Option<Key> {
    nome_da_tecla(codigo).and_then(|nome| Key::from_name(&nome))
}

fn nome_da_tecla(codigo: i32) -> Option<String> {
    let nome = match codigo {
        0x0100_0000 => "Escape",
        0x0100_0001 => "Tab",
        0x0100_0003 => "Backspace",
        // `Return` e o `Enter` do teclado numérico são uma tecla só para o egui.
        0x0100_0004 | 0x0100_0005 => "Enter",
        0x0100_0006 => "Insert",
        0x0100_0007 => "Delete",
        0x0100_0010 => "Home",
        0x0100_0011 => "End",
        0x0100_0012 => "Left",
        0x0100_0013 => "Up",
        0x0100_0014 => "Right",
        0x0100_0015 => "Down",
        0x0100_0016 => "PageUp",
        0x0100_0017 => "PageDown",
        0x20 => "Space",
        0x2c => "Comma",
        0x2d => "Minus",
        0x2e => "Period",
        0x2f => "Slash",
        0x3b => "Semicolon",
        0x3d => "Equals",
        0x5b => "OpenBracket",
        0x5c => "Backslash",
        0x5d => "CloseBracket",
        0x60 => "Backtick",
        0x27 => "Quote",
        // Dígitos e letras: o `Qt::Key` é o próprio ASCII maiúsculo, que é o nome no egui.
        0x30..=0x39 | 0x41..=0x5a => return char::from_u32(codigo as u32).map(String::from),
        // F1 a F35.
        0x0100_0030..=0x0100_0052 => return Some(format!("F{}", codigo - 0x0100_0030 + 1)),
        _ => return None,
    };
    Some(nome.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Key, tecla_do_qt};

    /// A seta do Qt é a seta do egui: daí em diante vale [`zeebx::ui::entrada::tecla_apertada`],
    /// que aceita as duas grafias do `settings.json`.
    #[test]
    fn a_seta_do_qt_e_a_do_egui() {
        assert_eq!(tecla_do_qt(0x0100_0013), Some(Key::ArrowUp));
    }

    #[test]
    fn as_teclas_do_qt_tem_o_nome_do_egui() {
        // O `settings.json` de quem já usa o emulador guarda estes nomes.
        for (codigo, nome) in [
            (0x0100_0013, "Up"),
            (0x0100_0004, "Enter"),
            (0x20, "Space"),
            (0x5a, "Z"),
            (0x34, "4"),
            (0x0100_0003, "Backspace"),
            (0x0100_0039, "F10"),
        ] {
            assert_eq!(tecla_do_qt(codigo), Key::from_name(nome), "{nome}");
            assert!(Key::from_name(nome).is_some(), "{nome} não é tecla do egui");
        }
    }
}
