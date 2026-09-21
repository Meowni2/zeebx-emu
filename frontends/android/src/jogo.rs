//! O jogo rodando.
//!
//! Num aparelho de mão o controle é físico, e botão na tela só rouba espaço: o quadro ocupa
//! tudo, e quem sai é o "voltar" do Android. O que aparece por cima é o painel de depuração,
//! ligado nas configurações — e ele é o [`zeebx::ui::depuracao::painel`], o mesmo do desktop.

use std::time::Instant;

use zeebx::session::Step;
use zeebx::ui::depuracao;
use zeebx::ui::settings::Scaling;

use crate::{Emulador, ORCAMENTO};

impl Emulador {
    pub(crate) fn jogo(&mut self, ctx: &egui::Context) {
        let Some(sessao) = self.sessao.as_mut() else {
            return;
        };

        // Com a pergunta na tela, ou em pausa, o jogo fica parado — inclusive o relógio, para
        // ele não gastar o orçamento todo de uma vez quando voltar.
        let parado = self.confirmando || self.pausado;
        if parado {
            self.ultimo = Instant::now();
        } else {
            // O orçamento é o tempo real que passou desde o quadro anterior, preso ao teto: é
            // quanto o jogo precisa emular para acompanhar o relógio do mundo.
            let passou = self.ultimo.elapsed().min(ORCAMENTO);
            self.ultimo = Instant::now();
            sessao.set_port_pad(0, self.pad);
            if sessao.step(passou, self.settings.graphics.speed_limit) == Step::Stopped {
                let motivo = sessao.stopped_reason().unwrap_or_default();
                log::info!("o jogo parou: {motivo}");
                self.erro = Some(
                    self.catalogo
                        .format("play.failed", &[("reason", &motivo)]),
                );
                self.fecha();
                return;
            }
        }

        // Um jogo que sai sozinho — pelo menu dele, pelo `ISHELL_CloseApplet` — volta para a
        // biblioteca, que é a tela inicial de quem abriu por ela.
        if sessao.saiu_sozinho() {
            self.fecha();
            return;
        }

        let tela = sessao.screen();
        let (largura, altura) = (tela.width() as usize, tela.height() as usize);
        let chave = (tela.serie(), tela.escritas(), self.settings.graphics.smooth);
        // Subir a textura só quando o quadro mudou: a tela repinta mais vezes que o jogo
        // desenha, e cada subida inteira custa uma conversão e uma ida à placa.
        if self.textura.is_none() || self.quadro != Some(chave) {
            let rgba: Vec<u8> = tela
                .to_argb()
                .into_iter()
                .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255])
                .collect();
            let imagem = egui::ColorImage::from_rgba_unmultiplied([largura, altura], &rgba);
            let filtro = match self.settings.graphics.smooth {
                true => egui::TextureOptions::LINEAR,
                false => egui::TextureOptions::NEAREST,
            };
            match &mut self.textura {
                Some(textura) => textura.set(imagem, filtro),
                textura => *textura = Some(ctx.load_texture("tela", imagem, filtro)),
            }
            self.quadro = Some(chave);
        }

        // Os números vêm da sessão antes de ela ser solta: desenhar precisa do `self` inteiro.
        let painel = self.settings.debug.overlay.then(|| {
            (
                sessao.sample(),
                sessao.memory(),
                sessao.clock_ms(),
                sessao
                    .history()
                    .map(|amostra| (amostra.speed, amostra.fps))
                    .collect::<Vec<_>>(),
            )
        });

        // O painel é uma faixa embaixo, e não uma sobreposição: reservar espaço encolhe o
        // quadro em vez de tapá-lo. Precisa ser declarado antes do painel central, porque no
        // egui quem pede espaço primeiro é quem o recebe.
        if let Some((amostra, memoria, relogio, historia)) = painel {
            let debug = self.settings.debug;
            let escuro = egui::Frame::NONE
                .fill(egui::Color32::from_black_alpha(200))
                .inner_margin(egui::Margin::symmetric(8, 2));
            egui::TopBottomPanel::bottom("painel-debug")
                .frame(escuro)
                .show(ctx, |ui| {
                    depuracao::painel(
                        ui,
                        &self.catalogo,
                        debug,
                        amostra,
                        memoria,
                        relogio,
                        &historia,
                    );
                });
        }

        let preto = egui::Frame::NONE.fill(egui::Color32::BLACK);
        egui::CentralPanel::default().frame(preto).show(ctx, |ui| {
            if let Some(textura) = &self.textura {
                let tamanho = coloca(
                    ui.available_size(),
                    egui::vec2(largura as f32, altura as f32),
                    self.settings.graphics.scaling,
                    self.settings.graphics.keep_aspect,
                );
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((textura.id(), tamanho)).fit_to_exact_size(tamanho));
                });
            }
        });

        if self.confirmando {
            self.pergunta(ctx);
        }
    }

    /// A pergunta do "voltar": fechar o jogo, pausar, ou continuar de onde parou.
    fn pergunta(&mut self, ctx: &egui::Context) {
        // A cortina escurece o jogo e, mais importante, come o toque: sem ela um dedo fora da
        // caixa iria parar no jogo atrás.
        egui::Area::new(egui::Id::new("cortina"))
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                let tela = ui.ctx().viewport_rect();
                ui.painter()
                    .rect_filled(tela, 0.0, egui::Color32::from_black_alpha(180));
            });

        let mut fechar = false;
        let mut continuar = false;
        let mut alterna_pausa = false;
        let pausado = self.pausado;
        let titulo = self.tr("play.close.title").to_string();
        let aviso = self.tr("play.close.warning").to_string();
        let continuar_rotulo = self.tr("play.continue").to_string();
        let pausa_rotulo = match pausado {
            true => self.tr("play.resume").to_string(),
            false => self.tr("play.pause").to_string(),
        };
        let parar_rotulo = self.tr("play.stop").to_string();

        egui::Window::new(titulo)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(aviso);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    continuar =
                        ui.add_sized([150.0, 48.0], egui::Button::new(&continuar_rotulo)).clicked();
                    alterna_pausa =
                        ui.add_sized([110.0, 48.0], egui::Button::new(&pausa_rotulo)).clicked();
                    fechar = ui.add_sized([110.0, 48.0], egui::Button::new(&parar_rotulo)).clicked();
                });
                ui.add_space(4.0);
            });

        if continuar {
            self.confirmando = false;
            self.pausado = false;
        }
        if alterna_pausa {
            self.pausado = !pausado;
            self.confirmando = false;
        }
        if fechar {
            self.fecha();
        }
    }
}

/// Que tamanho o quadro ocupa na área disponível.
///
/// É a mesma regra do desktop: pixel inteiro nunca borra e sobra borda; caber respeita a
/// proporção; preencher só deforma quando o formato foi dispensado.
fn coloca(area: egui::Vec2, quadro: egui::Vec2, modo: Scaling, manter: bool) -> egui::Vec2 {
    let aspecto = quadro.x / quadro.y.max(1.0);
    match modo {
        Scaling::Integer => {
            let fator = (area.x / quadro.x).min(area.y / quadro.y);
            match fator >= 1.0 {
                true => quadro * fator.floor(),
                // Menor que o quadro original não há múltiplo inteiro: cabe, e pronto.
                false => caber(area, aspecto),
            }
        }
        Scaling::Fit => caber(area, aspecto),
        Scaling::Stretch => match manter {
            true => caber(area, aspecto),
            false => area,
        },
    }
}

/// O maior retângulo com a proporção dada que cabe na área.
fn caber(area: egui::Vec2, aspecto: f32) -> egui::Vec2 {
    match area.x / area.y.max(1.0) > aspecto {
        true => egui::vec2(area.y * aspecto, area.y),
        false => egui::vec2(area.x, area.x / aspecto),
    }
}
