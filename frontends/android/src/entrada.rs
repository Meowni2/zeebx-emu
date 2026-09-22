//! Do evento do Android ao controle do Zeebo, e ao dedo na interface.
//!
//! **Este arquivo é a razão de o frontend do Android não usar o eframe.** O caminho do winit
//! perde o controle duas vezes: ele traduz os códigos de tecla de um `Gamepad` para
//! `Key::Unidentified`, que o egui joga fora, e os eixos do manche nem chegam a virar evento de
//! teclado — são `MotionEvent`, e o winit só olha os de toque. Lendo a fila nativa direto, o
//! que chega é o que o Android mandou: `Keycode::ButtonA`, `Axis::X`, o gatilho, o "voltar".
//!
//! O mapa de botões é o mesmo que o desktop dá a um controle moderno
//! ([`zeebx::input::bindings::Player::with_gamepad`]): sul no `b1`, leste no `b2`, oeste no
//! `b3`, norte no `b4`, os gatilhos superiores no `zl`/`zr` e o Start no `back` — o controle do
//! Zeebo não tem Start, quem ocupa o lugar dele é o HOME.

use android_activity::input::{
    Axis, InputEvent, KeyAction, Keycode, MotionAction, Source, ToolType,
};

use zeebx::input::{self, AXIS_CURSO, Pad};

/// Abaixo disto o manche está parado. O manche de um aparelho de mão nunca volta exatamente ao
/// centro, e sem zona morta o personagem anda sozinho.
const ZONA_MORTA: f32 = 0.12;

/// O que o laço colhe de uma rodada de eventos.
#[derive(Default)]
pub struct Entrada {
    /// Os eventos que a interface vai consumir: toques e as teclas que navegam menus.
    pub eventos: Vec<egui::Event>,
    /// O "voltar" foi apertado nesta rodada.
    pub voltar: bool,
    /// Quantos pixels cabem num ponto do egui.
    pixels_por_ponto: f32,
}

impl Entrada {
    pub fn nova(pixels_por_ponto: f32) -> Self {
        Self {
            pixels_por_ponto,
            ..Default::default()
        }
    }

    /// Limpa o que foi colhido no quadro anterior. O `voltar` é um pulso: vale um quadro.
    pub fn comeca_quadro(&mut self) {
        self.eventos.clear();
        self.voltar = false;
    }

    /// Recebe um evento da fila nativa e o distribui: o controle vai para o [`Pad`], o dedo e as
    /// teclas de navegação vão para a lista do egui.
    pub fn recebe(&mut self, evento: &InputEvent, pad: &mut Pad) {
        match evento {
            InputEvent::KeyEvent(tecla) => self.tecla(tecla, pad),
            InputEvent::MotionEvent(movimento) => self.movimento(movimento, pad),
            _ => {}
        }
    }

    fn tecla(&mut self, tecla: &android_activity::input::KeyEvent, pad: &mut Pad) {
        let apertada = match tecla.action() {
            KeyAction::Down => true,
            KeyAction::Up => false,
            // `Multiple` é repetição de texto; não é botão apertado nem solto.
            _ => return,
        };
        let codigo = tecla.key_code();

        if codigo == Keycode::Back {
            // Só a borda de descida: segurar o botão não deve abrir dois diálogos.
            if !apertada {
                self.voltar = true;
            }
            return;
        }

        let [cima, baixo, esquerda, direita] = input::DPAD;
        let botao = match codigo {
            Keycode::DpadUp => Some(cima),
            Keycode::DpadDown => Some(baixo),
            Keycode::DpadLeft => Some(esquerda),
            Keycode::DpadRight => Some(direita),
            // Sul, leste, oeste e norte — a mesma ordem do mapa de controle do desktop.
            Keycode::ButtonA | Keycode::DpadCenter | Keycode::Enter => Some(0),
            Keycode::ButtonB => Some(1),
            Keycode::ButtonX => Some(2),
            Keycode::ButtonY => Some(3),
            // O Zeebo tem **um** gatilho de cada lado, o ZL e o ZR.
            Keycode::ButtonL1 => Some(6),
            Keycode::ButtonR1 => Some(4),
            Keycode::ButtonL2 => Some(5),
            Keycode::ButtonR2 => Some(7),
            Keycode::ButtonThumbl => Some(10),
            Keycode::ButtonThumbr => Some(8),
            // O console não tem Start: o HOME, que aqui é o `back`, ocupa o lugar dele.
            Keycode::ButtonStart | Keycode::ButtonSelect => Some(9),
            Keycode::ButtonMode => Some(11),
            outro => {
                if apertada {
                    log::info!("tecla sem mapa: {outro:?}");
                }
                None
            }
        };
        if let Some(indice) = botao {
            pad.press(indice, apertada);
        }

        // O mesmo botão também navega a interface: num aparelho de mão a biblioteca é
        // percorrida com o direcional, não com o dedo.
        let navegacao = match codigo {
            Keycode::DpadUp => Some(egui::Key::ArrowUp),
            Keycode::DpadDown => Some(egui::Key::ArrowDown),
            Keycode::DpadLeft => Some(egui::Key::ArrowLeft),
            Keycode::DpadRight => Some(egui::Key::ArrowRight),
            Keycode::ButtonA | Keycode::DpadCenter | Keycode::Enter => Some(egui::Key::Enter),
            Keycode::ButtonB => Some(egui::Key::Escape),
            _ => None,
        };
        if let Some(key) = navegacao {
            self.eventos.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: apertada,
                repeat: tecla.repeat_count() > 0,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }

    fn movimento(&mut self, movimento: &android_activity::input::MotionEvent, pad: &mut Pad) {
        let fonte = movimento.source();
        if matches!(fonte, Source::Joystick | Source::Gamepad | Source::Dpad) {
            self.manche(movimento, pad);
            return;
        }
        self.toque(movimento);
    }

    /// Os eixos do manche e o direcional que alguns controles reportam como "chapéu".
    fn manche(&mut self, movimento: &android_activity::input::MotionEvent, pad: &mut Pad) {
        let Some(ponteiro) = movimento.pointers().next() else {
            return;
        };
        for (indice, eixo) in [Axis::X, Axis::Y, Axis::Z, Axis::Rz].into_iter().enumerate() {
            let cru = ponteiro.axis_value(eixo);
            let valor = match cru.abs() < ZONA_MORTA {
                true => 0.0,
                false => cru,
            };
            pad.set_axis(indice, (valor * AXIS_CURSO as f32) as i32);
        }

        // Nem todo controle manda o direcional como tecla: há quem o reporte como um par de
        // eixos que só vale -1, 0 ou 1. Quando ele vem assim, é aqui que vira botão.
        let [cima, baixo, esquerda, direita] = input::DPAD;
        let x = ponteiro.axis_value(Axis::HatX);
        let y = ponteiro.axis_value(Axis::HatY);
        if x != 0.0
            || y != 0.0
            || pad.is_down(cima)
            || pad.is_down(baixo)
            || pad.is_down(esquerda)
            || pad.is_down(direita)
        {
            pad.press(esquerda, x < -0.5);
            pad.press(direita, x > 0.5);
            pad.press(cima, y < -0.5);
            pad.press(baixo, y > 0.5);
        }
    }

    /// O dedo na tela, traduzido em ponteiro do egui.
    fn toque(&mut self, movimento: &android_activity::input::MotionEvent) {
        let indice = movimento.pointer_index();
        let ponteiro = movimento.pointer_at_index(indice);
        if ponteiro.tool_type() == ToolType::Palm {
            return;
        }
        let posicao = egui::pos2(
            ponteiro.axis_value(Axis::X) / self.pixels_por_ponto,
            ponteiro.axis_value(Axis::Y) / self.pixels_por_ponto,
        );

        match movimento.action() {
            MotionAction::Down | MotionAction::PointerDown => {
                // O egui decide o clique pelo lugar onde o ponteiro **estava**: sem este
                // `PointerMoved` antes, o primeiro toque numa tela nova não acerta nada.
                self.eventos.push(egui::Event::PointerMoved(posicao));
                self.eventos.push(egui::Event::PointerButton {
                    pos: posicao,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            MotionAction::Move => {
                self.eventos.push(egui::Event::PointerMoved(posicao));
            }
            MotionAction::Up | MotionAction::PointerUp => {
                self.eventos.push(egui::Event::PointerButton {
                    pos: posicao,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                // Sem isto o egui acha que o dedo continua pairando e deixa o botão aceso.
                self.eventos.push(egui::Event::PointerGone);
            }
            MotionAction::Cancel => {
                self.eventos.push(egui::Event::PointerGone);
            }
            _ => {}
        }
    }
}
