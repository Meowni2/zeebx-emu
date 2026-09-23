//! Os `QObject`s escritos em Rust, e a cola em C++ que eles chamam.
//!
//! Por enquanto é só a tela do jogo, e ainda pelo caminho mais simples: o quadro RGB565 da
//! sessão pintado por um `QQuickPaintedItem`. O `QImage::Format_RGB16` **é** RGB565, então a
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

        #[inherit]
        fn size(self: &Self) -> QSizeF;

        #[inherit]
        fn update(self: Pin<&mut Self>);
    }

    // Sem isto o construtor gerado repassa o pai `QObject*` à base, que quer um `QQuickItem*`.
    impl cxx_qt::Initialize for TelaDoJogo {}
}

use std::collections::HashSet;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, Instant};

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QImage, QImageFormat, QPainterRenderHint, QRect, QString};

use crate::input::bindings::Source;
use crate::input::gamepads::Gamepads;
use crate::session::Session;
use crate::ui::settings::Settings;

/// O mesmo teto do egui: uma pausa longa não vira uma fatia gigante de emulação.
const MAX_SLICE: Duration = Duration::from_millis(100);

pub struct TelaDoJogoRust {
    estado: QString,
    suave: bool,
    settings: Settings,
    gamepads: Gamepads,
    sessao: Option<Session>,
    /// Se a sessão usa o contexto fora de tela do Qt. Com ele, todo passo — e a destruição da
    /// sessão, que solta texturas — acontece com o contexto corrente.
    na_placa: bool,
    /// Pelo nome do egui: é o que o `settings.json` guarda.
    teclas: HashSet<String>,
    quadro: Option<QImage>,
    chave: Option<(u64, u64)>,
    ultimo_passo: Instant,
}

impl Default for TelaDoJogoRust {
    fn default() -> Self {
        let settings = Settings::load();
        Self {
            estado: QString::default(),
            suave: settings.graphics.smooth,
            settings,
            gamepads: Gamepads::default(),
            sessao: None,
            na_placa: false,
            teclas: HashSet::new(),
            quadro: None,
            chave: None,
            ultimo_passo: Instant::now(),
        }
    }
}

impl TelaDoJogoRust {
    /// Solta a sessão com o contexto dela corrente: soltar texturas com o contexto do Qt Quick
    /// corrente apagaria as dele.
    fn fecha(&mut self) {
        if self.sessao.is_none() {
            return;
        }
        let corrente = self.na_placa && qobject::gl_torna_corrente();
        self.sessao = None;
        if corrente {
            qobject::gl_solta();
        }
    }
}

impl Drop for TelaDoJogoRust {
    fn drop(&mut self) {
        self.fecha();
    }
}

/// O contexto fora de tela, pronto para o `glow`. `Err` diz por que não deu.
fn contexto_do_qt() -> Result<std::sync::Arc<glow::Context>, String> {
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
    Ok(std::sync::Arc::new(gl))
}

impl qobject::TelaDoJogo {
    pub fn abre(mut self: Pin<&mut Self>, caminho: &QString, placa: bool) {
        let caminho = String::from(caminho);
        let estado = {
            let mut rust = self.as_mut().rust_mut();
            let rust = &mut *rust;
            rust.fecha();
            rust.quadro = None;
            rust.chave = None;
            let contexto = match placa {
                true => match contexto_do_qt() {
                    Ok(gl) => Some(gl),
                    Err(erro) => {
                        eprintln!("{erro} — seguindo no rasterizador de software");
                        None
                    }
                },
                false => None,
            };
            let descricao = contexto.as_ref().map(|gl| {
                use glow::HasContext;
                unsafe { gl.get_parameter_string(glow::RENDERER) }
            });
            rust.na_placa = contexto.is_some();
            let portas = std::array::from_fn(|porta| {
                rust.settings
                    .controls
                    .player(porta)
                    .filter(|jogador| jogador.ligada)
                    .map(|jogador| jogador.aparelho)
            });
            let aberta = Session::start_with(
                Path::new(&caminho),
                portas,
                None,
                rust.na_placa,
                contexto,
                rust.settings.z_wheel,
            );
            let estado = match aberta {
                Ok(mut sessao) => {
                    let audio = &rust.settings.audio;
                    if let Some(erro) = sessao.set_audio(audio.enabled, audio.volume) {
                        eprintln!("áudio: {erro}");
                    }
                    let onde = descricao.map_or("software".to_string(), |placa| format!("placa: {placa}"));
                    let estado = format!("{} — {onde}", sessao.title());
                    rust.sessao = Some(sessao);
                    rust.ultimo_passo = Instant::now();
                    estado
                }
                Err(erro) => format!("não abriu: {erro:?}"),
            };
            if rust.na_placa {
                qobject::gl_solta();
            }
            estado
        };
        self.set_estado(QString::from(&estado));
    }

    pub fn passo(mut self: Pin<&mut Self>) {
        let (mudou, parou) = {
            let mut rust = self.as_mut().rust_mut();
            let rust = &mut *rust;
            let Some(sessao) = rust.sessao.as_mut() else {
                return;
            };
            rust.gamepads.poll();
            for (porta, jogador) in rust.settings.controls.ligadas() {
                let device = jogador.device.as_deref();
                let pad = jogador.pad(
                    |fonte| match fonte {
                        Source::Key { name } => rust.teclas.contains(name),
                        _ => rust.gamepads.is_active(device, porta, fonte),
                    },
                    |eixo| rust.gamepads.value(device, porta, eixo),
                );
                sessao.set_port_pad(porta, pad);
            }
            let agora = Instant::now();
            let fatia = (agora - rust.ultimo_passo).min(MAX_SLICE);
            rust.ultimo_passo = agora;
            let corrente = rust.na_placa && qobject::gl_torna_corrente();
            if !sessao.mostra_quadro_intermediario() {
                let _ = sessao.step(fatia, rust.settings.graphics.speed_limit);
            }
            if corrente {
                qobject::gl_solta();
            }
            let tela = sessao.screen();
            let chave = (tela.serie(), tela.escritas());
            let mudou = rust.chave != Some(chave);
            if mudou {
                rust.quadro = Some(imagem_rgb565(
                    &tela.to_rgb565_bytes(),
                    tela.width() as usize,
                    tela.height() as usize,
                ));
                rust.chave = Some(chave);
            }
            (mudou, sessao.stopped_reason())
        };
        if let Some(motivo) = parou {
            if self.estado() != &QString::from(&motivo) {
                self.as_mut().set_estado(QString::from(&motivo));
            }
        }
        if mudou {
            self.update();
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
        let Some(nome) = nome_da_tecla(codigo) else {
            return;
        };
        let mut rust = self.as_mut().rust_mut();
        match apertada {
            true => rust.teclas.insert(nome),
            false => rust.teclas.remove(&nome),
        };
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
    use super::nome_da_tecla;

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
            assert_eq!(nome_da_tecla(codigo).as_deref(), Some(nome));
            assert!(eframe::egui::Key::from_name(nome).is_some(), "{nome} não é tecla do egui");
        }
    }
}
