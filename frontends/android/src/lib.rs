//! O Zeebx no Android: a mesma emulação, outra tela.
//!
//! Este pacote é o frontend inteiro. Ele não sabe emular nada — quem faz isso é a
//! [`zeebx::session::Session`], a mesma que a janela do desktop e a linha de comando giram. O
//! que existe aqui é o que muda de plataforma: de onde vêm as ROMs, como o quadro chega à tela
//! e como o botão do aparelho vira botão do console.
//!
//! **O laço de eventos é nosso, e não do eframe.** A razão está em [`entrada`]: pelo winit os
//! botões do controle chegam como `Key::Unidentified` e os eixos do manche não chegam. Girando
//! a [`android_activity::AndroidApp`] direto, a entrada é a que o Android mandou. O preço é o
//! ciclo de vida da janela e o contexto de GL, que ficam em [`tela`].
//!
//! **A imagem é a do rasterizador de software.** O caminho da placa (`placa: true`) pede um
//! contexto de GL emprestado, e agora ele existe — ligá-lo é o passo seguinte, depois de medir.

mod entrada;
mod tela;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent, WindowManagerFlags};

use zeebx::input::Pad;
use zeebx::session::{Session, Step};
use zeebx::ui::settings::Settings;

use entrada::Entrada;
use tela::Tela;

/// Quanto tempo real o jogo pode tomar num quadro da interface. O mesmo teto do desktop: se o
/// jogo não acompanha, o que se perde é velocidade do jogo, não a resposta da tela.
const ORCAMENTO: Duration = Duration::from_millis(16);

/// O ponto em que o Android entra.
///
/// A `NativeActivity` carrega o `.so`, acha este símbolo e o chama numa linha de execução
/// própria. Daqui para baixo, tudo é nosso.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Zeebx"),
    );

    // Um pânico do Rust escreve no stderr, e o Android joga o stderr fora: sem isto o
    // aplicativo morre sem deixar uma linha no `logcat`. O gancho manda a mensagem e o lugar
    // para o log do sistema antes de o processo cair.
    std::panic::set_hook(Box::new(|info| {
        let onde = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "lugar desconhecido".to_string());
        log::error!("pânico em {onde}: {info}");
    }));

    log::info!("Zeebx começando");

    let interno = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp"));

    // **O Android não define `HOME`.** Sem ele o `settings::config_dir` do núcleo cai no seu
    // último recurso, o diretório corrente — que num aplicativo Android é `/`. O carregador
    // então tenta extrair os `.zip` em `/cache` e montar a raiz do aparelho em `/aparelho`, e
    // as duas coisas falham com "permission denied" num erro que *parece* ser da ROM.
    //
    // Definir `HOME` para o diretório privado do aplicativo faz todas as regras que já existem
    // apontarem para o lugar certo, sem uma linha de `cfg` no núcleo.
    unsafe { std::env::set_var("HOME", &interno) };

    // Os ajustes moram no diretório interno do aplicativo, que é só dele: o `Settings` do núcleo
    // é o mesmo do desktop, e aceita um caminho explícito justamente para isto.
    let ajustes = interno.join("zeebx.json");

    // A pasta de ROMs é escolhida pelo usuário e guardada nos ajustes. O padrão é `zeebo/roms`
    // na raiz do armazenamento, que é onde uma coleção costuma estar — e lê-la exige o "acesso a
    // todos os arquivos", que o manifesto declara e o sistema concede em Ajustes.
    let externo = app.external_data_path().map(|dir| dir.join("roms"));
    if let Some(dir) = &externo {
        let _ = std::fs::create_dir_all(dir);
    }
    let padrao = PathBuf::from("/sdcard/zeebo/roms");

    // A pasta interna do aplicativo serve de atalho no seletor: ela é a única que o Android
    // deixa ler sem permissão nenhuma.
    let minha_pasta = Some(interno.join("roms"));
    if let Some(dir) = &minha_pasta {
        let _ = std::fs::create_dir_all(dir);
    }

    // Tela cheia de verdade: num aparelho de mão a barra de status só rouba altura do jogo — e
    // era ela que cobria o caminho no seletor de pastas.
    app.set_window_flags(WindowManagerFlags::FULLSCREEN, WindowManagerFlags::empty());

    let mut emulador = Emulador::novo(ajustes, padrao, externo, minha_pasta, app.clone());
    gira(app, &mut emulador);
}

/// O laço: colhe eventos, deixa o jogo andar, desenha, repete.
fn gira(app: AndroidApp, emulador: &mut Emulador) {
    let ctx = egui::Context::default();
    // A interface do núcleo foi desenhada para um monitor. Num aparelho de mão, à distância de
    // um braço, o mesmo tamanho em pontos fica ilegível: a densidade do aparelho é o piso, e
    // daí para cima é o que o dedo precisa para acertar um botão.
    let pontos_por_pixel = app
        .config()
        .density()
        .map(|dpi| dpi as f32 / 160.0)
        .unwrap_or(2.0)
        .clamp(1.5, 3.0);
    ctx.set_pixels_per_point(pontos_por_pixel);

    let mut entrada = Entrada::nova(pontos_por_pixel);
    let mut tela: Option<Tela> = None;
    let mut visivel = false;
    let mut sair = false;
    let comeco = Instant::now();

    while !sair {
        // Com janela, o laço gira o mais rápido que a troca de buffers deixar; sem ela, não há
        // o que desenhar e esperar é o certo — é o que mantém o aplicativo parado em segundo
        // plano em vez de queimar bateria.
        let espera = match visivel && tela.is_some() {
            true => Some(Duration::ZERO),
            false => None,
        };

        app.poll_events(espera, |evento| match evento {
            PollEvent::Main(MainEvent::InitWindow { .. }) => match Tela::nova(&app) {
                Ok(nova) => tela = Some(nova),
                Err(erro) => log::error!("a tela não subiu: {erro}"),
            },
            PollEvent::Main(MainEvent::TerminateWindow { .. }) => tela = None,
            PollEvent::Main(MainEvent::WindowResized { .. })
            | PollEvent::Main(MainEvent::ContentRectChanged { .. })
            | PollEvent::Main(MainEvent::ConfigChanged { .. }) => {
                if let Some(tela) = &mut tela {
                    tela.redimensiona(&app);
                }
            }
            PollEvent::Main(MainEvent::Resume { .. }) | PollEvent::Main(MainEvent::GainedFocus) => {
                visivel = true;
                if let Some(tela) = &tela {
                    tela.retoma();
                }
            }
            PollEvent::Main(MainEvent::Pause) | PollEvent::Main(MainEvent::LostFocus) => {
                visivel = false
            }
            PollEvent::Main(MainEvent::Destroy) => sair = true,
            _ => {}
        });

        if sair {
            break;
        }

        entrada.comeca_quadro();
        if let Ok(mut fila) = app.input_events_iter() {
            // O iterador entrega um evento por chamada e quer a resposta na hora: `Handled`
            // diz ao Android que o evento morre aqui — é isso que impede o "voltar" de fechar
            // a atividade por baixo de nós.
            while fila.next(|evento| {
                entrada.recebe(evento, &mut emulador.pad);
                InputStatus::Handled
            }) {}
        }
        if entrada.voltar {
            emulador.voltar();
        }

        let Some(tela) = tela.as_mut() else {
            continue;
        };
        if !visivel {
            continue;
        }

        let [largura, altura] = tela.tamanho;
        let cru = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(
                    largura as f32 / pontos_por_pixel,
                    altura as f32 / pontos_por_pixel,
                ),
            )),
            time: Some(comeco.elapsed().as_secs_f64()),
            // O limite da GPU do aparelho, e não o palpite do egui: é ele que decide até onde o
            // atlas de fontes pode crescer.
            max_texture_side: Some(tela.pincel.max_texture_side()),
            events: std::mem::take(&mut entrada.eventos),
            ..Default::default()
        };

        let saida = ctx.run(cru, |ctx| emulador.desenha(ctx));
        let primitivas = ctx.tessellate(saida.shapes, saida.pixels_per_point);
        tela.pinta(&primitivas, &saida.textures_delta, saida.pixels_per_point);
    }

    log::info!("Zeebx encerrando");
}

/// O aplicativo: ou a lista de jogos, ou um jogo rodando.
struct Emulador {
    /// Onde os ajustes são lidos e gravados. É um arquivo do mesmo formato do desktop.
    ajustes: PathBuf,
    settings: Settings,
    /// A pasta sendo navegada, quando o seletor está aberto. `None` é a biblioteca na tela.
    navegando: Option<PathBuf>,
    /// A pasta privada do aplicativo. Sempre legível, sem permissão nenhuma — é o atalho que
    /// funciona mesmo quando o "acesso a todos os arquivos" não foi concedido.
    minha_pasta: Option<PathBuf>,
    /// A ponte para a atividade: é por ela que se pergunta e se pede a permissão.
    app: AndroidApp,
    /// O que há na pasta de ROMs, lido uma vez na abertura e a cada recarga pedida.
    achados: Vec<PathBuf>,
    sessao: Option<Session>,
    /// O quadro do console como textura do egui, trocada a cada quadro novo.
    textura: Option<egui::TextureHandle>,
    /// Os botões apertados agora, alimentados pela fila nativa.
    pad: Pad,
    /// O "voltar" foi apertado com um jogo aberto: a pergunta está na tela.
    confirmando: bool,
    /// Por que o último jogo não abriu, quando não abriu.
    erro: Option<String>,
    ultimo: Instant,
}

impl Emulador {
    fn novo(
        ajustes: PathBuf,
        padrao: PathBuf,
        externo: Option<PathBuf>,
        minha_pasta: Option<PathBuf>,
        app: AndroidApp,
    ) -> Self {
        let mut settings = Settings::load_from(&ajustes);
        // Sem pasta escolhida ainda: o padrão, se ele existir, senão o diretório do aplicativo,
        // que sempre existe e nunca pede permissão.
        if settings.roms_dir.is_none() {
            settings.roms_dir = match padrao.is_dir() {
                true => Some(padrao),
                false => externo,
            };
        }
        let roms = settings.roms_dir.clone().unwrap_or_default();
        let achados = procura(&roms);
        log::info!("{} arquivos em {}", achados.len(), roms.display());
        Self {
            navegando: None,
            minha_pasta,
            app,
            ajustes,
            settings,
            achados,
            sessao: None,
            textura: None,
            pad: Pad::default(),
            confirmando: false,
            erro: None,
            ultimo: Instant::now(),
        }
    }

    /// O botão "voltar" do Android.
    ///
    /// Ele nunca fecha o aplicativo por conta própria: com um jogo aberto, pergunta; no seletor,
    /// cancela a escolha; na biblioteca, não faz nada — sair é jogo do sistema.
    fn voltar(&mut self) {
        if self.confirmando {
            self.confirmando = false;
        } else if self.sessao.is_some() {
            self.confirmando = true;
        } else if self.navegando.is_some() {
            self.navegando = None;
        }
    }

    /// Fecha o jogo e volta para a biblioteca.
    fn fecha(&mut self) {
        self.sessao = None;
        self.textura = None;
        self.confirmando = false;
        self.pad = Pad::default();
    }

    /// Relê a pasta escolhida e diz no log o que achou.
    fn recarrega(&mut self) {
        let roms = self.roms();
        self.achados = procura(&roms);
        log::info!("{} arquivos em {}", self.achados.len(), roms.display());
    }

    /// A pasta escolhida agora. Vazia quando nenhuma foi escolhida ainda.
    fn roms(&self) -> PathBuf {
        self.settings.roms_dir.clone().unwrap_or_default()
    }

    fn abre(&mut self, caminho: &std::path::Path) {
        self.erro = None;
        match Session::start_with(
            caminho,
            zeebx::PORTAS_PADRAO,
            None,
            false,
            None,
            Default::default(),
        ) {
            Ok(sessao) => {
                log::info!("abriu {}", sessao.title());
                self.sessao = Some(sessao);
                self.ultimo = Instant::now();
            }
            Err(erro) => {
                log::error!("não abriu {}: {erro:?}", caminho.display());
                self.erro = Some(format!("{erro:?}"));
            }
        }
    }

    /// Qual das três telas está no ar.
    fn desenha(&mut self, ctx: &egui::Context) {
        if self.sessao.is_some() {
            self.jogo(ctx);
        } else if self.navegando.is_some() {
            self.seletor(ctx);
        } else {
            self.biblioteca(ctx);
        }
    }
}

/// O Android concede acesso a todo o armazenamento? Chama o
/// `Environment.isExternalStorageManager()`.
///
/// Sem isto o aplicativo só lê a própria pasta — e, pior, **lê sem erro nenhum na listagem**: o
/// `read_dir` funciona e o `File::open` é que falha. É por isso que a biblioteca enchia de nomes
/// e nenhum jogo abria.
fn tem_acesso_a_arquivos(app: &AndroidApp) -> bool {
    fn tenta(app: &AndroidApp) -> Result<bool, jni::errors::Error> {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let classe = env.find_class("android/os/Environment")?;
        env.call_static_method(classe, "isExternalStorageManager", "()Z", &[])?
            .z()
    }
    match tenta(app) {
        Ok(tem) => tem,
        Err(erro) => {
            log::error!("não deu para perguntar ao Android sobre o acesso: {erro}");
            false
        }
    }
}

/// Abre a tela em que o usuário concede o acesso a todos os arquivos.
///
/// O `MANAGE_EXTERNAL_STORAGE` não tem diálogo: quem o quer manda o usuário a uma página de
/// Ajustes, e essa página é aberta por `Intent`. Daí o JNI — é a única parte do aplicativo que
/// fala com o Java, e ela existe porque não há como pedir isso de outro jeito.
fn pede_acesso_a_arquivos(app: &AndroidApp) {
    fn tenta(app: &AndroidApp) -> Result<(), jni::errors::Error> {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let atividade = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr().cast()) };

        // `package:<nome>` diz à tela de Ajustes de qual aplicativo ela trata. Sem isso ela
        // abre a lista de todos, e o usuário tem de caçar o Zeebx no meio.
        let nome = env
            .call_method(&atividade, "getPackageName", "()Ljava/lang/String;", &[])?
            .l()?;
        let nome: String = env.get_string(&nome.into())?.into();

        let texto = env.new_string(format!("package:{nome}"))?;
        let uri = env
            .call_static_method(
                "android/net/Uri",
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[(&texto).into()],
            )?
            .l()?;

        let acao = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
        let intencao = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[(&acao).into(), (&uri).into()],
        )?;
        env.call_method(
            &atividade,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[(&intencao).into()],
        )?;
        Ok(())
    }
    if let Err(erro) = tenta(app) {
        log::error!("não deu para abrir a tela de permissão: {erro}");
    }
}

/// Um `.mod` é o jogo; um `.zip` é o pacote que o carregador abre.
fn e_modulo(caminho: &std::path::Path) -> bool {
    caminho
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mod") || e.eq_ignore_ascii_case("zip"))
}

/// Os módulos que a pasta tem.
fn procura(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut achados: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entrada| entrada.path())
        .filter(|caminho| e_modulo(caminho))
        .collect();
    achados.sort();
    achados
}

impl Emulador {
    /// A lista de jogos, quando nenhum está aberto.
    fn biblioteca(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(12.0);
            ui.heading("Zeebx");

            // Sem o acesso concedido, a listagem funciona e a abertura não: o usuário veria uma
            // biblioteca cheia em que nenhum jogo abre. Melhor dizer isso antes.
            if !tem_acesso_a_arquivos(&self.app) {
                ui.colored_label(
                    egui::Color32::LIGHT_YELLOW,
                    "O Zeebx só enxerga a própria pasta.",
                );
                ui.label("Para abrir ROMs de qualquer lugar do aparelho, conceda o acesso a arquivos.");
                if ui
                    .add_sized(
                        [ui.available_width(), 52.0],
                        egui::Button::new("Conceder acesso aos arquivos"),
                    )
                    .clicked()
                {
                    pede_acesso_a_arquivos(&self.app);
                }
                ui.add_space(8.0);
            }

            ui.horizontal(|ui| {
                ui.label("Pasta de ROMs:");
                ui.label(self.roms().display().to_string());
            });
            if ui
                .add_sized([ui.available_width(), 48.0], egui::Button::new("Escolher pasta"))
                .clicked()
            {
                // Abre no que já está escolhido, se ele existir; senão na raiz do armazenamento.
                let inicio = self.roms();
                self.navegando = Some(match inicio.is_dir() {
                    true => inicio,
                    false => PathBuf::from("/sdcard"),
                });
            }
            ui.add_space(6.0);

            if let Some(erro) = &self.erro {
                ui.colored_label(egui::Color32::LIGHT_RED, erro);
            }
            ui.separator();

            if ui.button("Reler a pasta").clicked() {
                self.recarrega();
            }
            ui.add_space(8.0);

            if self.achados.is_empty() {
                let pasta = self.roms();
                ui.label("Nenhum .mod ou .zip aqui.");
                if !pasta.as_os_str().is_empty() && !pasta.is_dir() {
                    ui.colored_label(egui::Color32::LIGHT_YELLOW, "A pasta não existe, ou o aplicativo não pode lê-la.");
                    ui.label("Em Ajustes › Aplicativos › Zeebx, conceda \"acesso a todos os arquivos\".");
                }
                return;
            }

            let mut abrir = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for caminho in &self.achados {
                    let nome = caminho
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    // Alvo de toque: um botão de largura inteira e alto o bastante para o dedo.
                    if ui
                        .add_sized([ui.available_width(), 56.0], egui::Button::new(nome))
                        .clicked()
                    {
                        abrir = Some(caminho.clone());
                    }
                }
            });
            if let Some(caminho) = abrir {
                self.abre(&caminho);
            }
        });
    }

    /// O navegador de pastas.
    ///
    /// É um navegador nosso, e não o seletor do sistema, por uma razão de fundo: o
    /// `ACTION_OPEN_DOCUMENT_TREE` devolve um `content://`, e o carregador do núcleo abre
    /// caminho de arquivo. Um navegador sobre o `std::fs` devolve o que ele já sabe usar — e
    /// não custa uma linha de JNI.
    fn seletor(&mut self, ctx: &egui::Context) {
        let Some(atual) = self.navegando.clone() else {
            return;
        };

        // O que a pasta tem: as subpastas para navegar, e quantos módulos para decidir.
        let mut pastas: Vec<PathBuf> = Vec::new();
        let mut modulos = 0usize;
        let mut ilegivel = None;
        match std::fs::read_dir(&atual) {
            Ok(entradas) => {
                for entrada in entradas.flatten() {
                    let caminho = entrada.path();
                    if caminho.is_dir() {
                        pastas.push(caminho);
                    } else if e_modulo(&caminho) {
                        modulos += 1;
                    }
                }
                pastas.sort();
            }
            Err(erro) => ilegivel = Some(erro.to_string()),
        }

        egui::TopBottomPanel::top("caminho").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(atual.display().to_string());
            ui.add_space(6.0);
        });

        egui::TopBottomPanel::bottom("acoes").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let largura = (ui.available_width() - 24.0) / 2.0;
                if ui
                    .add_sized([largura, 56.0], egui::Button::new("Cancelar"))
                    .clicked()
                {
                    self.navegando = None;
                }
                let rotulo = match modulos {
                    0 => "Usar esta pasta (vazia)".to_string(),
                    1 => "Usar esta pasta (1 jogo)".to_string(),
                    n => format!("Usar esta pasta ({n} jogos)"),
                };
                if ui
                    .add_sized([largura, 56.0], egui::Button::new(rotulo))
                    .clicked()
                {
                    self.settings.roms_dir = Some(atual.clone());
                    if let Err(erro) = self.settings.save_to(&self.ajustes) {
                        log::error!("não gravou os ajustes: {erro}");
                    }
                    self.navegando = None;
                    self.recarrega();
                }
            });
            ui.add_space(8.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Ir direto aos dois lugares que sempre existem poupa uma dúzia de toques — e a
            // pasta do aplicativo é a única que dispensa permissão.
            let mut atalho = None;
            ui.horizontal(|ui| {
                if ui.button("Armazenamento").clicked() {
                    atalho = Some(PathBuf::from("/sdcard"));
                }
                if let Some(minha) = &self.minha_pasta {
                    if ui.button("Pasta do aplicativo").clicked() {
                        atalho = Some(minha.clone());
                    }
                }
            });
            if let Some(destino) = atalho {
                self.navegando = Some(destino);
            }
            ui.add_space(4.0);

            if let Some(erro) = &ilegivel {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("Não deu para ler: {erro}"));
                ui.label("Em Ajustes › Aplicativos › Zeebx, conceda \"acesso a todos os arquivos\".");
            }

            let mut ir = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(acima) = atual.parent() {
                    if ui
                        .add_sized([ui.available_width(), 52.0], egui::Button::new("⬆  .."))
                        .clicked()
                    {
                        ir = Some(acima.to_path_buf());
                    }
                }
                for pasta in &pastas {
                    let nome = pasta
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .add_sized([ui.available_width(), 52.0], egui::Button::new(format!("📁  {nome}")))
                        .clicked()
                    {
                        ir = Some(pasta.clone());
                    }
                }
            });
            if let Some(destino) = ir {
                self.navegando = Some(destino);
            }
        });
    }

    /// O jogo rodando: um pedaço de emulação e o quadro na tela inteira.
    fn jogo(&mut self, ctx: &egui::Context) {
        let Some(sessao) = self.sessao.as_mut() else {
            return;
        };

        // Com a pergunta na tela o jogo fica parado — inclusive o relógio, para ele não gastar
        // o orçamento todo de uma vez quando voltar.
        if self.confirmando {
            self.ultimo = Instant::now();
        } else {
            // O orçamento é o tempo que passou desde o quadro anterior, preso ao teto: é quanto
            // o jogo precisa emular para acompanhar o relógio do mundo.
            let passou = self.ultimo.elapsed().min(ORCAMENTO);
            self.ultimo = Instant::now();
            sessao.set_port_pad(0, self.pad);
            if sessao.step(passou, true) == Step::Stopped {
                let motivo = sessao.stopped_reason().unwrap_or_default();
                log::info!("o jogo parou: {motivo}");
                self.erro = Some(motivo);
                self.fecha();
                return;
            }
        }

        let tela = sessao.screen();
        let (largura, altura) = (tela.width() as usize, tela.height() as usize);
        let rgba: Vec<u8> = tela
            .to_argb()
            .into_iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255])
            .collect();
        let imagem = egui::ColorImage::from_rgba_unmultiplied([largura, altura], &rgba);
        match &mut self.textura {
            Some(textura) => textura.set(imagem, egui::TextureOptions::NEAREST),
            textura => {
                *textura = Some(ctx.load_texture("tela", imagem, egui::TextureOptions::NEAREST))
            }
        }

        // Num aparelho de mão o controle é físico, e botão na tela só rouba espaço: o jogo
        // ocupa a tela inteira, e quem volta é o "voltar" do Android.
        let preto = egui::Frame::NONE.fill(egui::Color32::BLACK);
        egui::CentralPanel::default().frame(preto).show(ctx, |ui| {
            if let Some(textura) = &self.textura {
                // A tela do console é 4:3 e não deve esticar: cabe na largura ou na altura, o
                // que vier primeiro.
                let livre = ui.available_size();
                let escala = (livre.x / largura as f32).min(livre.y / altura as f32);
                let tamanho = egui::vec2(largura as f32 * escala, altura as f32 * escala);
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((textura.id(), tamanho)).fit_to_exact_size(tamanho));
                });
            }
        });

        if self.confirmando {
            self.pergunta(ctx);
        }
    }

    /// A pergunta do "voltar": fechar o jogo, ou continuar de onde parou.
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
        egui::Window::new("Fechar o jogo?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label("O progresso não salvo do jogo se perde.");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    continuar = ui
                        .add_sized([150.0, 48.0], egui::Button::new("Continuar jogando"))
                        .clicked();
                    fechar = ui
                        .add_sized([150.0, 48.0], egui::Button::new("Fechar"))
                        .clicked();
                });
                ui.add_space(4.0);
            });

        if continuar {
            self.confirmando = false;
        }
        if fechar {
            self.fecha();
        }
    }
}
