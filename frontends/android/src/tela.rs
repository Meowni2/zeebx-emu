//! A janela do Android como superfície de OpenGL ES, e o pincel do egui em cima dela.
//!
//! Sem o eframe, o contexto é nosso. O que o Android entrega é uma `ANativeWindow`; o resto é
//! EGL, e quem fala EGL aqui é o glutin — a mesma caixa que o desktop já usa para o contexto
//! fora de tela do rasterizador. O ciclo de vida é o do aplicativo: a janela **nasce e morre
//! várias vezes** (trocar de aplicativo, apagar a tela), e por isso a [`Tela`] é criada e
//! destruída junto com ela, enquanto o resto do programa continua de pé.

use std::num::NonZeroU32;
use std::sync::Arc;

use android_activity::AndroidApp;
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext,
    PossiblyCurrentGlContext, Version,
};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface};
use raw_window_handle::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle,
};

pub struct Tela {
    contexto: PossiblyCurrentContext,
    superficie: Surface<WindowSurface>,
    pub pincel: egui_glow::Painter,
    /// O tamanho da superfície em pixels. O egui trabalha em pontos; a conversão é a densidade.
    pub tamanho: [u32; 2],
}

impl Tela {
    /// Monta o contexto sobre a janela que a atividade acabou de criar.
    pub fn nova(app: &AndroidApp) -> Result<Self, String> {
        let janela = app.native_window().ok_or("a atividade não tem janela")?;
        let largura = NonZeroU32::new(janela.width() as u32).ok_or("janela sem largura")?;
        let altura = NonZeroU32::new(janela.height() as u32).ok_or("janela sem altura")?;

        let janela_crua = RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(
            janela.ptr().cast(),
        ));
        let display_cru = RawDisplayHandle::Android(AndroidDisplayHandle::new());

        // No Android só existe EGL: dizer isso explicitamente evita o glutin tentar GLX.
        let display = unsafe { Display::new(display_cru, DisplayApiPreference::Egl) }
            .map_err(|erro| format!("sem display EGL: {erro}"))?;

        let modelo = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .compatible_with_native_window(janela_crua)
            .build();
        // A melhor configuração é a com mais amostras que ainda case com a janela.
        let config = unsafe { display.find_configs(modelo) }
            .map_err(|erro| format!("sem configuração EGL: {erro}"))?
            .reduce(|a, b| if b.num_samples() > a.num_samples() { b } else { a })
            .ok_or("nenhuma configuração EGL serve para esta janela")?;

        // O manifesto pede GLES 3.0, e é o que o rasterizador do núcleo espera quando a placa
        // for ligada. Pedir explicitamente evita cair num contexto 2.0 por omissão.
        let atributos = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
            .build(Some(janela_crua));
        let contexto = unsafe { display.create_context(&config, &atributos) }
            .map_err(|erro| format!("sem contexto de GL: {erro}"))?;

        let atributos =
            SurfaceAttributesBuilder::<WindowSurface>::new().build(janela_crua, largura, altura);
        let superficie = unsafe { display.create_window_surface(&config, &atributos) }
            .map_err(|erro| format!("sem superfície: {erro}"))?;

        let contexto = contexto
            .make_current(&superficie)
            .map_err(|erro| format!("o contexto não ficou corrente: {erro}"))?;

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|nome| display.get_proc_address(nome))
        };
        let pincel = egui_glow::Painter::new(Arc::new(gl), "", None, false)
            .map_err(|erro| format!("o egui_glow não montou: {erro}"))?;

        log::info!("tela de {}x{}", largura.get(), altura.get());
        Ok(Self {
            contexto,
            superficie,
            pincel,
            tamanho: [largura.get(), altura.get()],
        })
    }

    /// A janela mudou de tamanho — girou, ou a barra do sistema apareceu.
    pub fn redimensiona(&mut self, app: &AndroidApp) {
        let Some(janela) = app.native_window() else {
            return;
        };
        let (Some(largura), Some(altura)) = (
            NonZeroU32::new(janela.width() as u32),
            NonZeroU32::new(janela.height() as u32),
        ) else {
            return;
        };
        self.superficie.resize(&self.contexto, largura, altura);
        self.tamanho = [largura.get(), altura.get()];
    }

    /// Desenha o que o egui produziu e troca os buffers.
    pub fn pinta(
        &mut self,
        primitivas: &[egui::ClippedPrimitive],
        texturas: &egui::TexturesDelta,
        pontos_por_pixel: f32,
    ) {
        self.pincel.clear(self.tamanho, [0.0, 0.0, 0.0, 1.0]);
        self.pincel
            .paint_and_update_textures(self.tamanho, pontos_por_pixel, primitivas, texturas);
        if let Err(erro) = self.superficie.swap_buffers(&self.contexto) {
            log::error!("a troca de buffers falhou: {erro}");
        }
    }

    /// Volta a ser o contexto corrente depois de a atividade ter sido retomada.
    pub fn retoma(&self) {
        if !self.contexto.is_current() {
            if let Err(erro) = self.contexto.make_current(&self.superficie) {
                log::error!("o contexto não voltou a ser corrente: {erro}");
            }
        }
    }
}

impl Drop for Tela {
    fn drop(&mut self) {
        // O pincel tem texturas e programas da GL; soltá-los enquanto o contexto ainda existe
        // é o que evita o vazamento quando a janela é recriada.
        self.pincel.destroy();
    }
}
