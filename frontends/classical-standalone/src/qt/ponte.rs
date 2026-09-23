//! Os `QObject`s escritos em Rust, e a cola em C++ que eles chamam.
//!
//! A tela do jogo, pelo caminho mais simples: o quadro RGB565 da sessão pintado por um
//! `QQuickPaintedItem`. O jogo em si — abrir, rodar, trocar — mora no [`super::nucleo`]; aqui fica
//! só o que é da tela. O `QImage::Format_RGB16` **é** RGB565, então a
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

        /// Uma volta: entrada, emulação e, se a tela mudou, um quadro novo.
        #[qinvokable]
        fn passo(self: Pin<&mut Self>);

        /// Um `Qt::Key` apertado ou solto.
        #[qinvokable]
        fn tecla(self: Pin<&mut Self>, codigo: i32, apertada: bool);

        /// A janela perdeu o foco: nenhuma tecla continua apertada.
        #[qinvokable]
        fn solta(self: Pin<&mut Self>);

        /// Fecha o jogo aberto. A janela do jogo chama isto ao ser fechada.
        #[qinvokable]
        fn fecha(self: Pin<&mut Self>);

        /// Pausa ou retoma o jogo. Devolve se ficou pausado.
        #[qinvokable]
        fn pausa(self: Pin<&mut Self>) -> bool;

        /// Relê o título do jogo aberto para a linha de estado. A janela chama isto ao aparecer.
        #[qinvokable]
        fn atualiza(self: Pin<&mut Self>);

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

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QImage, QImageFormat, QPainterRenderHint, QRect, QString};

use zeebx::eframe::egui::Key;

use super::nucleo;

pub struct TelaDoJogoRust {
    estado: QString,
    suave: bool,
    quadro: Option<QImage>,
    /// A tela que está no [`TelaDoJogoRust::quadro`]: série e escritas. Igual, não há o que subir.
    chave: Option<(u64, u64)>,
}

impl Default for TelaDoJogoRust {
    fn default() -> Self {
        Self {
            estado: QString::default(),
            suave: nucleo::com(|nucleo| nucleo.settings.graphics.smooth),
            quadro: None,
            chave: None,
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
    pub fn passo(mut self: Pin<&mut Self>) {
        let chave_atual = self.rust().chave;
        let (volta, quadro) = nucleo::com(|nucleo| {
            let volta = nucleo.passo();
            let quadro = nucleo.tela().and_then(|tela| {
                let chave = (tela.serie(), tela.escritas());
                (Some(chave) != chave_atual).then(|| {
                    let imagem = imagem_rgb565(
                        &tela.to_rgb565_bytes(),
                        tela.width() as usize,
                        tela.height() as usize,
                    );
                    (chave, imagem)
                })
            });
            (volta, quadro)
        });
        if let Some((chave, imagem)) = quadro {
            let mut rust = self.as_mut().rust_mut();
            rust.quadro = Some(imagem);
            rust.chave = Some(chave);
            drop(rust);
            self.as_mut().update();
        }
        if let Some(estado) = volta.estado.map(|texto| QString::from(&texto)) {
            if self.estado() != &estado {
                self.as_mut().set_estado(estado);
            }
        }
        if volta.fechou {
            self.as_mut().fechou();
        }
    }

    pub fn tecla(self: Pin<&mut Self>, codigo: i32, apertada: bool) {
        if let Some(tecla) = tecla_do_qt(codigo) {
            nucleo::com(|nucleo| nucleo.tecla(tecla, apertada));
        }
    }

    pub fn solta(self: Pin<&mut Self>) {
        nucleo::com(|nucleo| nucleo.solta_teclas());
    }

    pub fn fecha(mut self: Pin<&mut Self>) {
        nucleo::com(|nucleo| nucleo.fecha());
        let mut rust = self.as_mut().rust_mut();
        rust.quadro = None;
        rust.chave = None;
    }

    pub fn pausa(self: Pin<&mut Self>) -> bool {
        nucleo::com(|nucleo| {
            nucleo.alterna_pausa();
            nucleo.pausada()
        })
    }

    pub fn atualiza(mut self: Pin<&mut Self>) {
        let estado = nucleo::com(|nucleo| nucleo.estado());
        self.as_mut().set_estado(QString::from(&estado));
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
