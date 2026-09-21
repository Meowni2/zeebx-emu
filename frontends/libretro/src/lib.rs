//! Core Libretro do Zeebx.
//!
//! O frontend é dono de vídeo, áudio e entrada; aqui só se traduz isso para o motor `zeebx`.
//! Nenhuma janela, placa de som ou controle do host é aberto por este crate: o motor roda pelo
//! passo virtual de [`zeebx::session::Session::run_frame`].
//!
//! Decisões de escopo do primeiro core estão em `docs/libretro/LIBRETRO_PLAN.md`: conteúdo
//! `.mod`/`.zip`, vídeo RGB565, áudio PCM16 a 44,1 kHz, RetroPad nas duas portas, teclado USB e
//! Boomerang alimentado pelo sensor do frontend. Save state, renderização em hardware e mouse do
//! guest ainda não existem e são declarados como ausentes.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use zeebx::audio::Mixer;
use zeebx::config::ZWheel;
use zeebx::input::Pad;
use zeebx::input::bindings::Aparelho;
use zeebx::session::{Session, StartError, Step};
use zeebx::storage::StoragePaths;

/// Versão da ABI que este core implementa.
const API_VERSION: u32 = 1;

/// Taxa nominal do core: 44,1 kHz estéreo.
///
/// A quantidade entregue por chamada **não** é fixa: sai do tempo virtual que passou desde a
/// chamada anterior. Ver `retro_run`.
const SAMPLE_RATE: u32 = 44_100;

/// Formato de pixel negociado com o frontend, em `retro_pixel_format`.
const PIXEL_FORMAT_RGB565: u32 = 2;

/// Comandos de ambiente usados.
/// Pede ao frontend que descarregue o conteúdo e volte ao menu dele.
const ENV_SHUTDOWN: u32 = 7;

/// Pergunta se o frontend aceita receber quadro nulo quando nada mudou.
const ENV_GET_CAN_DUPE: u32 = 3;
const ENV_GET_SYSTEM_DIRECTORY: u32 = 9;
const ENV_SET_PIXEL_FORMAT: u32 = 10;
const ENV_SET_INPUT_DESCRIPTORS: u32 = 11;
const ENV_GET_LOG_INTERFACE: u32 = 27;
const ENV_GET_SAVE_DIRECTORY: u32 = 31;
const ENV_SET_CONTROLLER_INFO: u32 = 35;

/// Tipos de dispositivo e identificadores de botão do RetroPad.
const DEVICE_NONE: u32 = 0;
const DEVICE_JOYPAD: u32 = 1;
const DEVICE_ANALOG: u32 = 5;
const ID_B: u32 = 0;
const ID_Y: u32 = 1;
const ID_SELECT: u32 = 2;
const ID_START: u32 = 3;
const ID_UP: u32 = 4;
const ID_DOWN: u32 = 5;
const ID_LEFT: u32 = 6;
const ID_RIGHT: u32 = 7;
const ID_A: u32 = 8;
const ID_X: u32 = 9;
const ID_L: u32 = 10;
const ID_R: u32 = 11;
const ANALOG_LEFT: u32 = 0;
const ANALOG_AXIS_X: u32 = 0;
const ANALOG_AXIS_Y: u32 = 1;

/// Subclasses de RetroPad que identificam aparelhos do console.
const DEVICE_ZPAD: u32 = ((1 + 1) << 8) | DEVICE_JOYPAD;
const DEVICE_BOOMERANG: u32 = ((2 + 1) << 8) | DEVICE_JOYPAD;

type EnvironmentFn = unsafe extern "C" fn(cmd: u32, data: *mut c_void) -> bool;
type VideoRefreshFn =
    unsafe extern "C" fn(data: *const c_void, width: u32, height: u32, pitch: usize);
type AudioSampleBatchFn = unsafe extern "C" fn(data: *const i16, frames: usize) -> usize;
type InputPollFn = unsafe extern "C" fn();
type InputStateFn = unsafe extern "C" fn(port: u32, device: u32, index: u32, id: u32) -> i16;

#[repr(C)]
pub struct RetroSystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

#[repr(C)]
pub struct RetroGameGeometry {
    base_width: u32,
    base_height: u32,
    max_width: u32,
    max_height: u32,
    aspect_ratio: f32,
}

#[repr(C)]
pub struct RetroSystemTiming {
    fps: f64,
    sample_rate: f64,
}

#[repr(C)]
pub struct RetroSystemAvInfo {
    geometry: RetroGameGeometry,
    timing: RetroSystemTiming,
}

#[repr(C)]
pub struct RetroGameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}

#[repr(C)]
struct RetroLogCallback {
    log: Option<unsafe extern "C" fn(level: u32, fmt: *const c_char, ...)>,
}

#[repr(C)]
struct RetroInputDescriptor {
    port: u32,
    device: u32,
    index: u32,
    id: u32,
    description: *const c_char,
}

#[repr(C)]
struct RetroControllerDescription {
    desc: *const c_char,
    id: u32,
}

// SAFETY: as tabelas de descritores são `static` e imutáveis; os ponteiros apontam para literais
// de vida estática. A ABI exige que continuem válidos durante todo o carregamento.
unsafe impl Sync for RetroControllerDescription {}

#[repr(C)]
struct RetroControllerInfo {
    types: *const RetroControllerDescription,
    num_types: u32,
}

// SAFETY: idem acima: a lista de portas é imutável e aponta para as descrições estáticas.
unsafe impl Sync for RetroControllerInfo {}

/// O que o frontend oferece ao core.
///
/// É `Copy` de propósito: os ponteiros são tirados do mutex **antes** de qualquer chamada, para
/// que o core nunca segure um cadeado seu enquanto o frontend executa. Um frontend que abra
/// diálogo, salve estado ou espere outra thread dentro do callback travaria o core — e foi o que
/// aconteceu com o disco cheio, quando o RetroArch abriu o aviso de gravação no meio do quadro.
#[derive(Default, Clone, Copy)]
struct Frontend {
    environ: Option<EnvironmentFn>,
    video: Option<VideoRefreshFn>,
    audio_batch: Option<AudioSampleBatchFn>,
    input_poll: Option<InputPollFn>,
    input_state: Option<InputStateFn>,
}

/// Estado do jogo carregado.
struct Core {
    session: Session,
    mixer: Mixer,
    /// Porta 1 e 2, como o guest as enxerga.
    portas: [Option<Aparelho>; zeebx::input::PORTAS],
    /// Buffer do quadro no formato negociado, reaproveitado a cada `retro_run`.
    frame: Vec<u8>,
    audio: Vec<i16>,
    /// Caminho do conteúdo, para o `retro_reset`.
    path: PathBuf,
    /// Se o frontend aceita quadro nulo quando a tela não mudou.
    aceita_dupe: bool,
    /// Assinatura do último quadro entregue.
    ultima_assinatura: Option<u64>,
    /// Relógio virtual da última chamada, para o áudio acompanhar o tempo que passou de verdade.
    ultimo_relogio_ms: u32,
    /// Amostras que o frontend não aceitou e ficam para a chamada seguinte.
    audio_pendente: Vec<i16>,
    /// Quantos quadros já foram apresentados depois da parada.
    ///
    /// A tela final fica à mostra por um instante antes de o frontend ser dispensado: sem isso o
    /// conteúdo some no mesmo quadro em que o jogo acaba, e quem está jogando não vê o desfecho.
    quadros_apos_parar: u32,
    /// Se o desfecho já foi relatado ao frontend.
    ///
    /// Sem isto o core repetiria a mesma linha a cada quadro depois da parada, e um log que cresce
    /// para sempre esconde justamente o instante em que o jogo parou.
    parou: bool,
}

fn frontend() -> &'static Mutex<Frontend> {
    static FRONTEND: OnceLock<Mutex<Frontend>> = OnceLock::new();
    FRONTEND.get_or_init(|| Mutex::new(Frontend::default()))
}

/// O estado do jogo visto pela ABI.
///
/// O motor não é `Send`: carrega `Rc`, ponteiros do JIT e do rasterizador, que valem enquanto
/// estiverem na mesma thread. A ABI Libretro garante que `retro_run`, `retro_reset` e
/// `retro_unload_game` são chamados na thread que carregou o jogo, e é essa a promessa que este
/// invólucro faz ao compilador.
struct EstadoDoCore(Core);

// SAFETY: ver o comentário acima — o core nunca é movido para outra thread.
unsafe impl Send for EstadoDoCore {}

fn core() -> &'static Mutex<Option<EstadoDoCore>> {
    static CORE: OnceLock<Mutex<Option<EstadoDoCore>>> = OnceLock::new();
    CORE.get_or_init(|| Mutex::new(None))
}

/// Uma cópia dos callbacks do frontend, sem manter o cadeado.
fn callbacks() -> Frontend {
    frontend().lock().map(|guard| *guard).unwrap_or_default()
}

unsafe fn environ(cmd: u32, data: *mut c_void) -> bool {
    let Some(callback) = callbacks().environ else {
        return false;
    };
    // SAFETY: o frontend promete que o callback aceita o comando pedido. O cadeado do core já foi
    // solto, então o frontend pode fazer o que quiser aqui dentro.
    unsafe { callback(cmd, data) }
}

fn log(mensagem: &str) {
    let Ok(texto) = CString::new(mensagem) else {
        return;
    };
    let mut callback = RetroLogCallback { log: None };
    let alvo = &mut callback as *mut RetroLogCallback as *mut c_void;
    if unsafe { environ(ENV_GET_LOG_INTERFACE, alvo) } {
        if let Some(escreve) = callback.log {
            // SAFETY: o frontend forneceu o callback e o formato é literal.
            unsafe { escreve(3, c"%s".as_ptr(), texto.as_ptr()) };
        }
    }
}

/// Descreve o aparelho de uma porta para o `GetConnectedDevices` do guest.
fn aparelho_do_dispositivo(device: u32) -> Option<Aparelho> {
    match device {
        DEVICE_NONE => None,
        DEVICE_ZPAD => Some(Aparelho::ZPad),
        DEVICE_BOOMERANG => Some(Aparelho::Boomerang),
        _ => Some(Aparelho::Controle),
    }
}

/// Lê o RetroPad e monta o estado que o console enxerga.
fn le_pad(porta: u32) -> Pad {
    let frente = callbacks();
    let (Some(poll), Some(state)) = (frente.input_poll, frente.input_state) else {
        return Pad::default();
    };
    // SAFETY: os callbacks vêm do frontend e são chamados na thread de `retro_run`.
    unsafe { poll() };
    let botao = |id: u32| -> bool {
        // SAFETY: consulta de estado do próprio frontend.
        unsafe { state(porta, DEVICE_JOYPAD, 0, id) != 0 }
    };
    let mut pad = Pad::default();
    let mapa = [
        (ID_UP, "up"),
        (ID_DOWN, "down"),
        (ID_LEFT, "left"),
        (ID_RIGHT, "right"),
        (ID_Y, "b1"),
        (ID_B, "b2"),
        (ID_X, "b3"),
        (ID_A, "b4"),
        (ID_L, "zl"),
        (ID_R, "zr"),
        (ID_START, "start"),
        (ID_SELECT, "back"),
    ];
    for (id, nome) in mapa {
        if botao(id) {
            if let Some(indice) = Pad::button_by_name(nome) {
                pad.press(indice, true);
            }
        }
    }
    // Os dois analógicos do RetroPad viram os quatro eixos do console, na faixa que o guest lê.
    let eixo = |index: u32, id: u32| -> i32 {
        // SAFETY: consulta de estado do próprio frontend.
        unsafe { state(porta, DEVICE_ANALOG, index, id) as i32 }
    };
    for (indice, valor) in [
        (0usize, eixo(ANALOG_LEFT, ANALOG_AXIS_X)),
        (1usize, eixo(ANALOG_LEFT, ANALOG_AXIS_Y)),
        (2usize, eixo(1, ANALOG_AXIS_X)),
        (3usize, eixo(1, ANALOG_AXIS_Y)),
    ] {
        // `-0x8000..=0x7fff` do frontend para o curso do manche do console.
        pad.set_axis(indice, valor / 256);
    }
    pad
}

/// Registra os aparelhos que o usuário pode escolher em cada porta.
unsafe fn registra_controladores() {
    static DESCRICOES: [RetroControllerDescription; 5] = [
        RetroControllerDescription {
            desc: c"Desconectado".as_ptr(),
            id: DEVICE_NONE,
        },
        RetroControllerDescription {
            desc: c"Dragon".as_ptr(),
            id: DEVICE_JOYPAD,
        },
        RetroControllerDescription {
            desc: c"Z-Pad".as_ptr(),
            id: DEVICE_ZPAD,
        },
        RetroControllerDescription {
            desc: c"Boomerang".as_ptr(),
            id: DEVICE_BOOMERANG,
        },
        RetroControllerDescription {
            desc: c"Teclado USB".as_ptr(),
            id: 3,
        },
    ];
    static PORTAS: [RetroControllerInfo; 3] = [
        RetroControllerInfo {
            types: DESCRICOES.as_ptr(),
            num_types: DESCRICOES.len() as u32,
        },
        RetroControllerInfo {
            types: DESCRICOES.as_ptr(),
            num_types: DESCRICOES.len() as u32,
        },
        RetroControllerInfo {
            types: std::ptr::null(),
            num_types: 0,
        },
    ];
    // SAFETY: as tabelas têm vida estática, como a ABI exige.
    unsafe {
        environ(ENV_SET_CONTROLLER_INFO, PORTAS.as_ptr() as *mut c_void);
    }
}

/// Rotula os botões para a tela de configuração do frontend.
unsafe fn registra_botoes() {
    let mut descritores: Vec<RetroInputDescriptor> = Vec::new();
    let rotulos = [
        (ID_UP, "Direcional cima"),
        (ID_DOWN, "Direcional baixo"),
        (ID_LEFT, "Direcional esquerda"),
        (ID_RIGHT, "Direcional direita"),
        (ID_Y, "Botão 1"),
        (ID_B, "Botão 2"),
        (ID_X, "Botão 3"),
        (ID_A, "Botão 4"),
        (ID_L, "ZL"),
        (ID_R, "ZR"),
        (ID_START, "HOME/Start"),
        (ID_SELECT, "Voltar"),
    ];
    for porta in 0..2u32 {
        for (id, texto) in rotulos {
            descritores.push(RetroInputDescriptor {
                port: porta,
                device: DEVICE_JOYPAD,
                index: 0,
                id,
                description: texto.as_ptr() as *const c_char,
            });
        }
        for (index, id, texto) in [
            (ANALOG_LEFT, ANALOG_AXIS_X, "Manche X"),
            (ANALOG_LEFT, ANALOG_AXIS_Y, "Manche Y"),
        ] {
            descritores.push(RetroInputDescriptor {
                port: porta,
                device: DEVICE_ANALOG,
                index,
                id,
                description: texto.as_ptr() as *const c_char,
            });
        }
    }
    descritores.push(RetroInputDescriptor {
        port: 0,
        device: 0,
        index: 0,
        id: 0,
        description: std::ptr::null(),
    });
    // SAFETY: a lista termina em `description` nulo e os textos são literais estáticos.
    unsafe {
        environ(
            ENV_SET_INPUT_DESCRIPTORS,
            descritores.as_ptr() as *mut c_void,
        );
    }
}

fn diretorio(cmd: u32) -> Option<PathBuf> {
    let mut ponteiro: *const c_char = std::ptr::null();
    let alvo = &mut ponteiro as *mut *const c_char as *mut c_void;
    if !unsafe { environ(cmd, alvo) } || ponteiro.is_null() {
        return None;
    }
    // SAFETY: o frontend entrega um caminho UTF-8 válido durante a chamada; copiamos aqui.
    let texto = unsafe { CStr::from_ptr(ponteiro) }
        .to_string_lossy()
        .into_owned();
    Some(PathBuf::from(texto))
}

/// `retro_api_version`.
#[unsafe(no_mangle)]
pub extern "C" fn retro_api_version() -> u32 {
    API_VERSION
}

/// `retro_set_environment`: o frontend entrega o caminho de volta ao sistema.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_set_environment(callback: Option<EnvironmentFn>) {
    if let Ok(mut guard) = frontend().lock() {
        guard.environ = callback;
    }
    // SAFETY: a partir daqui o ambiente está disponível para registrar o que precisa.
    unsafe {
        registra_controladores();
        registra_botoes();
    }
}

/// `retro_set_video_refresh`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_set_video_refresh(callback: Option<VideoRefreshFn>) {
    if let Ok(mut guard) = frontend().lock() {
        guard.video = callback;
    }
}

/// `retro_set_audio_sample_batch`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_set_audio_sample_batch(callback: Option<AudioSampleBatchFn>) {
    if let Ok(mut guard) = frontend().lock() {
        guard.audio_batch = callback;
    }
}

/// `retro_set_input_poll`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_set_input_poll(callback: Option<InputPollFn>) {
    if let Ok(mut guard) = frontend().lock() {
        guard.input_poll = callback;
    }
}

/// `retro_set_input_state`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_set_input_state(callback: Option<InputStateFn>) {
    if let Ok(mut guard) = frontend().lock() {
        guard.input_state = callback;
    }
}

/// `retro_set_audio_sample`: a variante de uma amostra não é usada; o core sempre entrega lote.
#[unsafe(no_mangle)]
pub extern "C" fn retro_set_audio_sample(_callback: Option<unsafe extern "C" fn(i16, i16)>) {}

/// `retro_init`.
#[unsafe(no_mangle)]
pub extern "C" fn retro_init() {}

/// `retro_deinit`.
#[unsafe(no_mangle)]
pub extern "C" fn retro_deinit() {
    if let Ok(mut guard) = core().lock() {
        *guard = None;
    }
}

/// `retro_get_system_info`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_get_system_info(info: *mut RetroSystemInfo) {
    if info.is_null() {
        return;
    }
    // SAFETY: o frontend passou um ponteiro válido para preencher.
    unsafe {
        *info = RetroSystemInfo {
            library_name: c"Zeebx".as_ptr(),
            library_version: concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char,
            valid_extensions: c"mod|zip".as_ptr(),
            need_fullpath: true,
            block_extract: true,
        };
    }
}

/// `retro_get_system_av_info`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_get_system_av_info(info: *mut RetroSystemAvInfo) {
    if info.is_null() {
        return;
    }
    // SAFETY: o frontend passou um ponteiro válido para preencher.
    unsafe {
        *info = RetroSystemAvInfo {
            geometry: RetroGameGeometry {
                base_width: 640,
                base_height: 480,
                max_width: 640,
                max_height: 480,
                aspect_ratio: 4.0 / 3.0,
            },
            timing: RetroSystemTiming {
                fps: 60.0,
                sample_rate: SAMPLE_RATE as f64,
            },
        };
    }
}

/// `retro_load_game`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_load_game(game: *const RetroGameInfo) -> bool {
    if game.is_null() {
        return false;
    }
    // SAFETY: ponteiro validado e lido apenas dentro da chamada.
    let caminho = unsafe { (*game).path };
    if caminho.is_null() {
        return false;
    }
    // SAFETY: caminho UTF-8 prometido pela ABI.
    let caminho = unsafe { CStr::from_ptr(caminho) }
        .to_string_lossy()
        .into_owned();
    let Some(save_dir) = diretorio(ENV_GET_SAVE_DIRECTORY) else {
        log("Zeebx: o frontend não informou diretório de saves; recusando carregar.");
        return false;
    };
    let sistema = diretorio(ENV_GET_SYSTEM_DIRECTORY);
    // O sistema guarda o que é da máquina e o que é descartável — a NAND `fs:/` e o cache de
    // conteúdo extraído. Os saves do título ficam no diretório de saves, que é o que o frontend
    // sincroniza. É a mesma divisão que o PPSSPP faz entre `flash0` e o memory stick.
    let storage = StoragePaths::for_frontend(&save_dir, sistema.as_deref());
    if let Err(erro) = storage.create_dirs() {
        log(&format!(
            "Zeebx: sem permissão em {} / {}: {erro}",
            storage.saves.display(),
            storage.cache.display()
        ));
        return false;
    }
    log(&format!(
        "Zeebx: saves em {}, sistema em {}",
        storage.saves.display(),
        storage.cache.display()
    ));
    // O formato de vídeo é negociado antes de rodar: sem ele não há como entregar quadro.
    let mut formato = PIXEL_FORMAT_RGB565;
    let alvo = &mut formato as *mut u32 as *mut c_void;
    if !unsafe { environ(ENV_SET_PIXEL_FORMAT, alvo) } {
        log("Zeebx: o frontend não aceita RGB565.");
        return false;
    }
    // O quadro repetido pode ir como nulo: economiza uma cópia de 600 KB por quadro e evita
    // ocupar o frontend com trabalho que não muda nada na tela.
    let mut aceita_dupe = false;
    let alvo_dupe = &mut aceita_dupe as *mut bool as *mut c_void;
    let aceita_dupe = unsafe { environ(ENV_GET_CAN_DUPE, alvo_dupe) } && aceita_dupe;
    let portas = [Some(Aparelho::Controle), None];
    let resultado = unsafe { carrega(&caminho, &storage, portas, PathBuf::from(&caminho)) };
    match resultado {
        Ok(mut novo) => {
            novo.aceita_dupe = aceita_dupe;
            novo.ultimo_relogio_ms = novo.session.clock_ms();
            if let Ok(mut guard) = core().lock() {
                *guard = Some(EstadoDoCore(novo));
            }
            true
        }
        Err(erro) => {
            log(&format!("Zeebx: não deu para abrir {caminho}: {erro}"));
            false
        }
    }
}

/// Monta o estado do jogo. Separado do export para concentrar o `unsafe` da ABI num lugar só.
unsafe fn carrega(
    caminho: &str,
    storage: &StoragePaths,
    portas: [Option<Aparelho>; zeebx::input::PORTAS],
    path: PathBuf,
) -> Result<Core, StartError> {
    let session = Session::start_software_with_storage(
        std::path::Path::new(caminho),
        portas,
        ZWheel::default(),
        storage,
    )?;
    let mut session = session;
    let mixer = session.grava_audio(SAMPLE_RATE);
    // O cache guarda a extração de **todo** jogo já aberto; sem poda, um acervo inteiro acaba
    // dentro da pasta de saves do frontend. A extração em uso nunca sai.
    match zeebx::loader::archive::prune_cache(
        &storage.cache,
        Some(session.content_root()),
        zeebx::loader::archive::CACHE_LIMIT_BYTES,
    ) {
        Ok(liberado) if liberado > 0 => log(&format!(
            "Zeebx: cache podado, {liberado} bytes liberados em {}",
            storage.cache.display()
        )),
        Ok(_) => {}
        Err(erro) => log(&format!("Zeebx: não deu para podar o cache: {erro}")),
    }
    Ok(Core {
        session,
        mixer,
        portas,
        frame: Vec::new(),
        audio: Vec::new(),
        path,
        aceita_dupe: false,
        ultima_assinatura: None,
        ultimo_relogio_ms: 0,
        audio_pendente: Vec::new(),
        quadros_apos_parar: 0,
        parou: false,
    })
}

/// `retro_run`: um quadro virtual, um quadro de vídeo e o áudio correspondente.
#[unsafe(no_mangle)]
pub extern "C" fn retro_run() {
    // Os buffers saem do estado antes das chamadas ao frontend: nenhum cadeado do core fica preso
    // enquanto o frontend executa, e é isso que impede um aviso dele — "disco cheio, quer salvar?"
    // — de travar o emulador.
    let (frame, audio, largura, altura, duplicado) = {
        let Ok(mut guard) = core().lock() else {
            return;
        };
        let Some(EstadoDoCore(estado)) = guard.as_mut() else {
            return;
        };
        // Entrada primeiro: o guest lê o controle dentro do quadro que vai rodar.
        for porta in 0..zeebx::input::PORTAS {
            if estado.portas[porta].is_none() {
                continue;
            }
            let pad = le_pad(porta as u32);
            estado.session.set_port_pad(porta, pad);
        }
        if let Step::Stopped = estado.session.run_frame() {
            if !estado.parou {
                estado.parou = true;
                let relogio = estado.session.clock_ms();
                let motivo = estado
                    .session
                    .stopped_reason()
                    .unwrap_or_else(|| "sem motivo relatado".to_string());
                log(&format!("Zeebx: parou em {relogio} ms virtuais: {motivo}"));
                // O log do próprio jogo é o que costuma nomear o motivo da parada; as últimas
                // linhas vão para o frontend, que é onde alguém vai olhar.
                let relato = estado.session.log();
                for linha in relato.iter().rev().take(6).rev() {
                    log(&format!("Zeebx:   {linha}"));
                }
                let memoria = estado.session.memory();
                log(&format!(
                    "Zeebx:   heap {} bytes, {} objetos",
                    memoria.0, memoria.1
                ));
                // No console, sair de um jogo devolve o controle à Z-Wheel, que é outro applet
                // instalado. Aqui o pedido é registrado, mas **a ABI não deixa o core pedir
                // outro conteúdo ao frontend**: ou o core carregaria a Z-Wheel por conta própria,
                // ou o frontend encerra o conteúdo. Ver o plano, seção de desfecho.
                if let Some(classe) = estado.session.take_launch_request() {
                    log(&format!(
                        "Zeebx: o shell pediu para abrir {classe:#010x}; trocar de conteúdo dentro do core ainda não existe"
                    ));
                }
                estado.quadros_apos_parar = 0;
            }
        }
        // Vídeo: o framebuffer do console, no formato negociado.
        let tela = estado.session.screen();
        let (largura, altura) = (tela.width(), tela.height());
        let mut quadro = std::mem::take(&mut estado.frame);
        tela.write_rgb565_into(&mut quadro);
        let assinatura = tela.signature();
        let duplicado = estado.aceita_dupe && estado.ultima_assinatura == Some(assinatura);
        estado.ultima_assinatura = Some(assinatura);
        // Áudio: **o tempo vem do relógio virtual**, não de um número fixo. Um jogo que passa dois
        // quadros virtuais entre duas chamadas precisa entregar o dobro de amostras, senão o som
        // atrasa em relação à imagem e o frontend engasga ao tentar acompanhar.
        let agora = estado.session.clock_ms();
        let decorrido = u64::from(agora.wrapping_sub(estado.ultimo_relogio_ms));
        estado.ultimo_relogio_ms = agora;
        let devidas = (decorrido.min(1000) * u64::from(SAMPLE_RATE) / 1000) as usize;
        let mut som = std::mem::take(&mut estado.audio);
        som.clear();
        // O que o frontend não aceitou da vez anterior vai na frente, para não sumir um pedaço.
        som.append(&mut estado.audio_pendente);
        for amostra in estado.mixer.render(devidas) {
            som.push((amostra.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
        }
        (quadro, som, largura, altura, duplicado)
    };
    let frente = callbacks();
    if let Some(video) = frente.video {
        let (ponteiro, _) = match duplicado {
            // Quadro nulo avisa "repete o anterior", que é o que a ABI oferece para tela parada.
            true => (std::ptr::null(), ()),
            false => (frame.as_ptr() as *const c_void, ()),
        };
        // SAFETY: o buffer vive durante a chamada; no quadro repetido o frontend reusa o último.
        unsafe {
            video(ponteiro, largura, altura, largura as usize * 2);
        }
    }
    // O retorno do lote é em quadros **aceitos**; o que sobrar espera a próxima chamada.
    let mut sobra = Vec::new();
    if let Some(batch) = frente.audio_batch {
        let quadros = audio.len() / 2;
        // SAFETY: o lote é intercalado em estéreo e o tamanho é o número de quadros.
        let aceitos = unsafe { batch(audio.as_ptr(), quadros) }.min(quadros);
        if aceitos < quadros {
            sobra = audio[aceitos * 2..].to_vec();
        }
    }
    // Os buffers voltam para o estado, para a próxima chamada reaproveitar a mesma alocação.
    let mut dispensar = false;
    if let Ok(mut guard) = core().lock() {
        if let Some(EstadoDoCore(estado)) = guard.as_mut() {
            estado.frame = frame;
            estado.audio = audio;
            // A sobra não pode crescer sem fim; meio segundo é o teto.
            let limite = (SAMPLE_RATE as usize / 2) * 2;
            sobra.truncate(limite);
            estado.audio_pendente = sobra;
            if estado.parou {
                estado.quadros_apos_parar += 1;
                // Dois segundos de tela parada bastam para ver o desfecho e o log.
                dispensar = estado.quadros_apos_parar == 120;
            }
        }
    }
    if dispensar {
        // Encerrar o conteúdo é o desfecho que a ABI oferece: o frontend volta ao menu dele, em
        // vez de ficar mostrando para sempre o último quadro de um jogo que acabou.
        log("Zeebx: jogo terminado; pedindo ao frontend para encerrar o conteúdo");
        unsafe {
            environ(ENV_SHUTDOWN, std::ptr::null_mut());
        }
    }
}

/// `retro_unload_game`.
#[unsafe(no_mangle)]
pub extern "C" fn retro_unload_game() {
    if let Ok(mut guard) = core().lock() {
        *guard = None;
    }
}

/// `retro_reset`: recarrega o conteúdo do zero, preservando saves e NAND.
#[unsafe(no_mangle)]
pub extern "C" fn retro_reset() {
    let Ok(mut guard) = core().lock() else {
        return;
    };
    let Some(EstadoDoCore(antigo)) = guard.as_ref() else {
        return;
    };
    let caminho = antigo.path.clone();
    let portas = antigo.portas;
    let Some(save_dir) = diretorio(ENV_GET_SAVE_DIRECTORY) else {
        return;
    };
    let sistema = diretorio(ENV_GET_SYSTEM_DIRECTORY);
    let storage = StoragePaths::for_frontend(&save_dir, sistema.as_deref());
    let texto = caminho.to_string_lossy().into_owned();
    // SAFETY: mesma montagem do carregamento, na thread de `retro_run`.
    match unsafe { carrega(&texto, &storage, portas, caminho.clone()) } {
        Ok(novo) => *guard = Some(EstadoDoCore(novo)),
        Err(erro) => log(&format!("Zeebx: reset falhou: {erro}")),
    }
}

/// `retro_set_controller_port_device`: troca o aparelho que o guest enxerga na porta.
#[unsafe(no_mangle)]
pub extern "C" fn retro_set_controller_port_device(port: u32, device: u32) {
    let Ok(mut guard) = core().lock() else {
        return;
    };
    let Some(EstadoDoCore(estado)) = guard.as_mut() else {
        return;
    };
    let porta = port as usize;
    if porta >= zeebx::input::PORTAS {
        return;
    }
    estado.portas[porta] = aparelho_do_dispositivo(device);
    estado.session.set_portas(estado.portas);
}

/// `retro_serialize_size`: save state ainda não existe.
#[unsafe(no_mangle)]
pub extern "C" fn retro_serialize_size() -> usize {
    0
}

/// `retro_serialize`: sem suporte declarado, sempre falha.
#[unsafe(no_mangle)]
pub extern "C" fn retro_serialize(_data: *mut c_void, _size: usize) -> bool {
    false
}

/// `retro_unserialize`: sem suporte declarado, sempre falha.
#[unsafe(no_mangle)]
pub extern "C" fn retro_unserialize(_data: *const c_void, _size: usize) -> bool {
    false
}

/// `retro_cheat_reset`.
#[unsafe(no_mangle)]
pub extern "C" fn retro_cheat_reset() {}

/// `retro_cheat_set`.
#[unsafe(no_mangle)]
pub extern "C" fn retro_cheat_set(_index: u32, _enabled: bool, _code: *const c_char) {}

/// `retro_load_game_special`: não há subsistemas.
#[unsafe(no_mangle)]
pub extern "C" fn retro_load_game_special(
    _game_type: u32,
    _info: *const RetroGameInfo,
    _num_info: usize,
) -> bool {
    false
}

/// `retro_get_region`.
#[unsafe(no_mangle)]
pub extern "C" fn retro_get_region() -> u32 {
    0
}

/// `retro_get_memory_data`: os saves são arquivos do BREW, não uma região contínua.
#[unsafe(no_mangle)]
pub extern "C" fn retro_get_memory_data(_id: u32) -> *mut c_void {
    std::ptr::null_mut()
}

/// `retro_get_memory_size`.
#[unsafe(no_mangle)]
pub extern "C" fn retro_get_memory_size(_id: u32) -> usize {
    0
}
