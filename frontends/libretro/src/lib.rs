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
use std::path::{Path, PathBuf};
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

/// Mostra um aviso ao jogador, na tela do próprio frontend.
const ENV_SET_MESSAGE: u32 = 6;

/// Pergunta se o frontend entrega todos os botões numa palavra só.
const ENV_GET_INPUT_BITMASKS: u32 = 51;

/// Identificador especial do RetroPad que devolve os botões como máscara de bits.
const ID_JOYPAD_MASK: u32 = 256;

/// Recebe as teclas do frontend. É por aqui que a Z-Wheel navega: ela pede `AVK_0` e `AVK_CLR`,
/// que não existem no RetroPad.
const ENV_SET_KEYBOARD_CALLBACK: u32 = 12;

/// Códigos de tecla da ABI (`enum retro_key`), que seguem os do SDL 1.2.
const RETROK_BACKSPACE: u32 = 8;
const RETROK_RETURN: u32 = 13;
const RETROK_ESCAPE: u32 = 27;
const RETROK_ASTERISK: u32 = 42;
const RETROK_HASH: u32 = 35;
const RETROK_0: u32 = 48;
const RETROK_UP: u32 = 273;
const RETROK_DOWN: u32 = 274;
const RETROK_RIGHT: u32 = 275;
const RETROK_LEFT: u32 = 276;

/// A última classe que o shell pediu para abrir.
///
/// **Instrumento de teste, e só dele.** O log do core sai pelo callback do frontend, que é
/// **variádico** — uma função `extern "C" fn(...)` não pode ser escrita em Rust estável —, então o
/// teste observa por aqui o que a interface mostraria como texto. Fora de `cfg(test)` não existe.
#[cfg(test)]
static ULTIMA_ABERTURA: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// O frontend oferece um contexto de placa para o core desenhar.
///
/// **O core não pede, e é assim que ele recusa.** No `libretro`, quem *oferece* é o core: ele
/// preenche o struct e chama o ambiente. Pedir e ignorar a resposta deixaria o frontend esperando
/// que nós desenhássemos no framebuffer dele — que ninguém desenhou. Enquanto o encaixe não
/// existe, o core **não chama** `SET_HW_RENDER`, e o RetroArch segue no caminho de software, que
/// é o medido e comparado com o da placa. Ver `docs/libretro/LIBRETRO_PLAN.md`, item 5.
#[allow(dead_code)]
const ENV_SET_HW_RENDER: u32 = 14;

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
type KeyboardEventFn = unsafe extern "C" fn(down: bool, keycode: u32, character: u32, modifiers: u16);

#[repr(C)]
struct RetroMessage {
    msg: *const c_char,
    frames: u32,
}

#[repr(C)]
struct RetroKeyboardCallback {
    callback: Option<KeyboardEventFn>,
}
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
    /// As raízes do perfil, para trocar de applet sem perder saves nem cache.
    storage: StoragePaths,
    /// Os jogos encontrados ao lado do conteúdo: ClassID do applet e o que carregar.
    ///
    /// É o que permite atender o pedido de lançamento do shell: a Z-Wheel pede uma classe, e aqui
    /// se sabe qual `.mod` ou `.zip` responde por ela.
    jogos: Vec<(u32, PathBuf)>,
    /// O conteúdo da Z-Wheel, para voltar a ela quando um jogo que ela abriu termina.
    z_wheel: Option<PathBuf>,
    /// Se a sessão atual foi aberta pela Z-Wheel — a volta é para ela, como no console.
    aberto_pela_z_wheel: bool,
    /// Se o frontend entrega os botões do RetroPad numa máscara de bits.
    bitmasks: bool,
    /// Se o frontend aceita quadro nulo quando a tela não mudou.
    aceita_dupe: bool,
    /// Assinatura do último quadro entregue.
    ultima_assinatura: Option<u64>,
    /// Relógio virtual da última chamada, para o áudio acompanhar o tempo que passou de verdade.
    ultimo_relogio_ms: u32,
    /// Amostras que o frontend não aceitou e ficam para a chamada seguinte.
    audio_pendente: Vec<i16>,
    /// Se já avisou que o quadro saiu do tamanho do console.
    avisou_tamanho: bool,
    /// Estado anterior do Select do RetroPad, para o atalho de `AVK_CLR`.
    select_antes: bool,
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

/// Fila de teclas do frontend.
///
/// O callback não executa guest: ele **enfileira**, e `retro_run` entrega as teclas ao motor na
/// thread normal do core. Executar o emulador de dentro do callback seria reentrância em cima do
/// estado que `retro_run` está usando.
fn teclas() -> &'static Mutex<std::collections::VecDeque<(u32, bool)>> {
    static TECLAS: OnceLock<Mutex<std::collections::VecDeque<(u32, bool)>>> = OnceLock::new();
    TECLAS.get_or_init(|| Mutex::new(std::collections::VecDeque::new()))
}

/// Traduz uma tecla do frontend para o código virtual do BREW, quando existe.
///
/// `Esc` e `P` ficam de fora de propósito: são as teclas da interface do frontend, não do jogo.
fn avk_da_tecla(keycode: u32) -> Option<u32> {
    use zeebx::input::avk;
    Some(match keycode {
        RETROK_UP => avk::UP,
        RETROK_DOWN => avk::DOWN,
        RETROK_LEFT => avk::LEFT,
        RETROK_RIGHT => avk::RIGHT,
        RETROK_RETURN => avk::SELECT,
        RETROK_BACKSPACE | RETROK_ESCAPE => avk::CLR,
        RETROK_ASTERISK => avk::STAR,
        RETROK_HASH => avk::POUND,
        RETROK_0..=57 => avk::ZERO + (keycode - RETROK_0),
        _ => return None,
    })
}

/// O callback de teclado do frontend: só enfileira.
unsafe extern "C" fn tecla_recebida(down: bool, keycode: u32, _character: u32, _modifiers: u16) {
    let Some(avk) = avk_da_tecla(keycode) else {
        return;
    };
    if let Ok(mut fila) = teclas().lock() {
        // Um teto evita que uma tecla presa (ou um frontend repetindo sem parar) cresça sem fim.
        if fila.len() < 1024 {
            fila.push_back((avk, down));
        }
    }
}

fn core() -> &'static Mutex<Option<EstadoDoCore>> {
    static CORE: OnceLock<Mutex<Option<EstadoDoCore>>> = OnceLock::new();
    CORE.get_or_init(|| Mutex::new(None))
}

/// Uma cópia dos callbacks do frontend, sem manter o cadeado.
fn callbacks() -> Frontend {
    frontend().lock().map(|guard| *guard).unwrap_or_default()
}

/// Avança um aviso ao jogador. Também vai para o log, porque nem todo frontend mostra mensagem
/// — e um aviso que ninguém vê não serve para nada.
fn aviso(texto: &str) {
    let Ok(texto_c) = CString::new(texto) else {
        return;
    };
    let mensagem = RetroMessage {
        // Três segundos a 60 Hz: tempo de ler sem atrapalhar quem está jogando.
        msg: texto_c.as_ptr(),
        frames: 180,
    };
    unsafe {
        environ(
            ENV_SET_MESSAGE,
            &mensagem as *const RetroMessage as *mut c_void,
        );
    }
    log(&format!("Zeebx: {texto}"));
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

/// Se o Select do RetroPad está apertado na porta dada.
fn le_select(porta: u32) -> bool {
    let frente = callbacks();
    let (Some(poll), Some(state)) = (frente.input_poll, frente.input_state) else {
        return false;
    };
    // SAFETY: callbacks do frontend, chamados na thread de `retro_run`.
    unsafe {
        poll();
        state(porta, DEVICE_JOYPAD, 0, ID_SELECT) != 0
    }
}

/// Lê o RetroPad e monta o estado que o console enxerga.
///
/// Com `bitmasks`, os doze botões vêm numa palavra só — uma chamada ao frontend por quadro em vez
/// de doze. O `id` especial `ID_JOYPAD_MASK` devolve os bits na ordem dos `RETRO_DEVICE_ID_JOYPAD_*`.
fn le_pad(porta: u32, bitmasks: bool) -> Pad {
    let frente = callbacks();
    let (Some(poll), Some(state)) = (frente.input_poll, frente.input_state) else {
        return Pad::default();
    };
    // SAFETY: os callbacks vêm do frontend e são chamados na thread de `retro_run`.
    unsafe { poll() };
    let mascara = match bitmasks {
        // SAFETY: consulta de estado do próprio frontend.
        true => unsafe { state(porta, DEVICE_JOYPAD, 0, ID_JOYPAD_MASK) as u32 },
        false => 0,
    };
    let botao = |id: u32| -> bool {
        match bitmasks {
            true => mascara & (1 << id) != 0,
            // SAFETY: consulta de estado do próprio frontend.
            false => unsafe { state(porta, DEVICE_JOYPAD, 0, id) != 0 },
        }
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
        // O rótulo vai no `id` que o core realmente lê, senão a tela de mapeamento do frontend
        // ensina o jogador a apertar o botão errado.
        (ID_B, "Botão 1"),
        (ID_Y, "Botão 2"),
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
            valid_extensions: c"mod|zip|7z".as_ptr(),
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
    // Teclado: a Z-Wheel navega por `AVK_*`, que o RetroPad não produz.
    static TECLADO: RetroKeyboardCallback = RetroKeyboardCallback {
        callback: Some(tecla_recebida),
    };
    unsafe {
        environ(
            ENV_SET_KEYBOARD_CALLBACK,
            &TECLADO as *const RetroKeyboardCallback as *mut c_void,
        );
    }
    // O formato de vídeo é negociado antes de rodar: sem ele não há como entregar quadro.
    let mut formato = PIXEL_FORMAT_RGB565;
    let alvo = &mut formato as *mut u32 as *mut c_void;
    if !unsafe { environ(ENV_SET_PIXEL_FORMAT, alvo) } {
        log("Zeebx: o frontend não aceita RGB565.");
        return false;
    }
    // Uma chamada por quadro em vez de doze, quando o frontend entrega a máscara.
    let bitmasks = unsafe { environ(ENV_GET_INPUT_BITMASKS, std::ptr::null_mut()) };
    log(&format!(
        "Zeebx: botões por {}",
        match bitmasks {
            true => "máscara de bits",
            false => "consulta individual",
        }
    ));
    // O quadro repetido pode ir como nulo: economiza uma cópia de 600 KB por quadro e evita
    // ocupar o frontend com trabalho que não muda nada na tela.
    let mut aceita_dupe = false;
    let alvo_dupe = &mut aceita_dupe as *mut bool as *mut c_void;
    let aceita_dupe = unsafe { environ(ENV_GET_CAN_DUPE, alvo_dupe) } && aceita_dupe;
    let portas = [Some(Aparelho::Controle), None];
    let resultado = unsafe { carrega(&caminho, &storage, portas, PathBuf::from(&caminho)) };
    match resultado {
        Ok(mut novo) => {
            novo.bitmasks = bitmasks;
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
    // **A ordem importa: a biblioteca e a fonte vêm antes da sessão.**
    //
    // A fonte é lida quando a máquina é construída — ela entra no `font` do motor. Instalá-la
    // depois deixava a sessão inteira sem fonte, e o jogo não desenhava texto nenhum: era o que
    // fazia a tela do Double Dragon ficar branca e vazia, porque o que ele desenha ali é a
    // mensagem "Memory is insufficient. Please delete some files." em fundo branco.
    let (pasta, jogos) = biblioteca(caminho);
    let fonte = prepara_fonte(storage, &jogos);
    match &fonte {
        Some(onde) => log(&format!("Zeebx: fonte do sistema em {}", onde.display())),
        None => aviso(&format!(
            "Sem tectoy.ttf no acervo ({} jogos): o texto do sistema não será desenhado",
            jogos.len()
        )),
    }

    let mut session = Session::start_software_with_storage(
        std::path::Path::new(caminho),
        portas,
        ZWheel::default(),
        storage,
    )?;
    let mixer = session.grava_audio(SAMPLE_RATE);
    // **1x e proporção nativa, sempre.** O core entrega o quadro do console em 640×480, sem
    // resolução interna ampliada e sem esticar: quem ajusta shader precisa de uma fonte previsível,
    // e o upscale é papel do frontend.
    session.define_resolucao_interna(1);
    session.define_proporcao(None);
    if !jogos.is_empty() {
        session.set_installed_applets(
            jogos
                .iter()
                .filter_map(|(classe, caminho)| {
                    Some((*classe, zeebx::library::id_do_modulo(caminho)?))
                }),
        );
        log(&format!(
            "Zeebx: {} jogo(s) instalados a partir de {}",
            jogos.len(),
            pasta.as_deref().unwrap_or(std::path::Path::new(".")).display()
        ));
    }
    // A Z-Wheel é o shell: quando ela é o conteúdo, guardar o caminho é o que permite voltar a
    // ela depois que um jogo termina — o que o console faz.
    let z_wheel = match session.classe() == zeebx::session::Z_WHEEL {
        true => Some(path.clone()),
        false => None,
    };

    Ok(Core {
        session,
        mixer,
        portas,
        frame: Vec::new(),
        audio: Vec::new(),
        path,
        storage: storage.clone(),
        jogos,
        z_wheel,
        aberto_pela_z_wheel: false,
        bitmasks: false,
        aceita_dupe: false,
        ultima_assinatura: None,
        ultimo_relogio_ms: 0,
        audio_pendente: Vec::new(),
        avisou_tamanho: false,
        select_antes: false,
        quadros_apos_parar: 0,
        parou: false,
    })
}

/// A biblioteca de jogos ao lado do conteúdo: `(pasta, [(ClassID, o que carregar)])`.
///
/// É o que responde ao pedido de lançamento do shell — a Z-Wheel pede uma classe e aqui se sabe
/// qual arquivo a atende. Sem isso ela abre vazia, porque enumera os instalados e não acha nenhum.
/// A deduplicação por ClassID evita listar duas vezes quem tem o `.zip` **e** uma cópia extraída.
fn biblioteca(caminho: &str) -> (Option<PathBuf>, Vec<(u32, PathBuf)>) {
    let pasta = std::path::Path::new(caminho)
        .parent()
        .map(std::path::Path::to_path_buf);
    let mut jogos: Vec<(u32, PathBuf)> = Vec::new();
    let mut vistas: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for jogo in pasta.as_deref().map(zeebx::library::scan).unwrap_or_default() {
        let Some(classe) = jogo.clsid else {
            continue;
        };
        if vistas.insert(classe) {
            jogos.push((classe, jogo.path));
        }
    }
    (pasta, jogos)
}

/// Instala a fonte do sistema na raiz do aparelho, se ainda não estiver lá.
///
/// Sai do cache quando a Z-Wheel já foi extraída e, quando não foi, do próprio pacote compactado —
/// extraindo **só** o arquivo, porque não vale materializar o pacote inteiro por 190 KB.
fn prepara_fonte(storage: &StoragePaths, jogos: &[(u32, PathBuf)]) -> Option<PathBuf> {
    zeebx::loader::archive::fonte_do_sistema_em(&storage.cache, &storage.device).or_else(|| {
        let candidatos: Vec<&PathBuf> = jogos
            .iter()
            .filter(|(classe, _)| *classe == zeebx::session::Z_WHEEL)
            .map(|(_, caminho)| caminho)
            .collect();
        let pacote = candidatos
            .iter()
            .find(|caminho| zeebx::loader::archive::embalado(caminho))
            .or_else(|| candidatos.first())?;
        zeebx::loader::archive::instala_fonte_do_pacote(pacote, &storage.device)
    })
}

/// Troca o applet em execução dentro do mesmo core.
///
/// É o que o console faz quando a Z-Wheel abre um jogo e quando o jogo fecha: o shell continua
/// sendo o shell. O frontend não participa — para ele, `retro_run` só devolveu outro quadro.
fn troca_para(estado: &mut Core, caminho: &Path, aberto_pela_z_wheel: bool) -> Result<(), StartError> {
    let mut session = Session::start_software_with_storage(
        caminho,
        estado.portas,
        ZWheel::default(),
        &estado.storage,
    )?;
    // O novo applet nasce sem lista de instalados: sem isto a Z-Wheel volta vazia.
    session.set_installed_applets(
        estado
            .jogos
            .iter()
            .filter_map(|(classe, caminho)| {
                Some((*classe, zeebx::library::id_do_modulo(caminho)?))
            }),
    );
    let mixer = session.grava_audio(SAMPLE_RATE);
    session.define_resolucao_interna(1);
    session.define_proporcao(None);
    estado.session = session;
    estado.mixer = mixer;
    estado.path = caminho.to_path_buf();
    estado.aberto_pela_z_wheel = aberto_pela_z_wheel;
    estado.parou = false;
    estado.quadros_apos_parar = 0;
    estado.ultima_assinatura = None;
    estado.select_antes = false;
    estado.ultimo_relogio_ms = estado.session.clock_ms();
    Ok(())
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
        // Teclas que o frontend entregou desde o último quadro. O callback só enfileira; aqui
        // elas entram no guest, na thread normal do core.
        if let Ok(mut fila) = teclas().lock() {
            while let Some((avk, down)) = fila.pop_front() {
                estado.session.set_key(avk, down);
            }
        }
        // O shell pediu outro applet — a Z-Wheel escolheu um jogo, ou o jogo mandou voltar. O
        // pedido vive **dentro** do motor (`Machine::pending_launch`), e a UI desktop já o atende
        // assim; aqui ele troca de sessão sem o frontend saber.
        if let Some(classe) = estado.session.take_launch_request() {
            // Instrumento do teste, e só dele: o log do core sai pelo callback do frontend, que é
            // **variádico** e por isso não pode ser implementado num teste em Rust estável. Aqui o
            // teste observa o que a interface do frontend mostraria como texto.
            #[cfg(test)]
            ULTIMA_ABERTURA.store(classe, std::sync::atomic::Ordering::Relaxed);
            let alvo = estado
                .jogos
                .iter()
                .find(|(c, _)| *c == classe)
                .map(|(_, caminho)| caminho.clone());
            match alvo {
                Some(caminho) => {
                    let e_z_wheel = classe == zeebx::session::Z_WHEEL;
                    match troca_para(estado, &caminho, !e_z_wheel) {
                        Ok(()) => log(&format!(
                            "Zeebx: o shell pediu {classe:#010x}; abrindo {}",
                            caminho.display()
                        )),
                        Err(erro) => log(&format!(
                            "Zeebx: o shell pediu {classe:#010x} e não deu para abrir: {erro}"
                        )),
                    }
                }
                None => aviso(&format!(
                    "O shell pediu {classe:#010x}, que não está na pasta de jogos"
                )),
            }
        }
        // Atalho: o Select do RetroPad vale como `AVK_CLR`, que é o "voltar" do console. Sem ele
        // a Z-Wheel fica presa na abertura em quem não tem teclado mapeado no frontend.
        let select = le_select(0);
        if select != estado.select_antes {
            estado.select_antes = select;
            estado.session.set_key(zeebx::input::avk::CLR, select);
        }
        // Entrada primeiro: o guest lê o controle dentro do quadro que vai rodar.
        for porta in 0..zeebx::input::PORTAS {
            if estado.portas[porta].is_none() {
                continue;
            }
            let pad = le_pad(porta as u32, estado.bitmasks);
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
        // O console é 640×480, e é esse o quadro que o shader espera receber. Um tamanho
        // diferente é avisado uma vez, em vez de aparecer como imagem torta sem explicação.
        if !estado.avisou_tamanho && (largura != 640 || altura != 480) {
            estado.avisou_tamanho = true;
            log(&format!(
                "Zeebx: quadro {largura}x{altura}, fora dos 640x480 do console"
            ));
        }
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
                if estado.quadros_apos_parar == 120 {
                    // **A volta para a Z-Wheel.** No console, fechar um jogo devolve o controle ao
                    // shell, e o shell é outro applet instalado — então aqui se troca de sessão,
                    // como a UI desktop já faz. Sem Z-Wheel disponível, o desfecho possível é
                    // pedir ao frontend que encerre o conteúdo.
                    let voltar = estado.z_wheel.clone().filter(|_| {
                        estado.aberto_pela_z_wheel
                            || estado.session.classe() == zeebx::session::Z_WHEEL
                    });
                    match voltar {
                        Some(caminho) => match troca_para(estado, &caminho, false) {
                            Ok(()) => log("Zeebx: fim do jogo; de volta à Z-Wheel"),
                            Err(erro) => {
                                log(&format!("Zeebx: não deu para voltar à Z-Wheel: {erro}"));
                                dispensar = true;
                            }
                        },
                        None => dispensar = true,
                    }
                }
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


#[cfg(test)]
mod testes {
    use super::*;
    use std::ffi::CString;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Quantos quadros o vídeo do frontend recebeu.
    static QUADROS: AtomicU32 = AtomicU32::new(0);
    /// Qual botão do RetroPad o teste está segurando. `u32::MAX` é nenhum.
    static BOTAO: AtomicU32 = AtomicU32::new(u32::MAX);
    /// A pasta que o frontend de teste entrega como sistema e como saves.
    static PASTA: OnceLock<CString> = OnceLock::new();
    /// O sistema de fora, quando `ZEEBX_CORE_SISTEMA` aponta para uma árvore de aparelho real.
    static SISTEMA: OnceLock<CString> = OnceLock::new();
    /// A assinatura de cada quadro entregue, na ordem.
    static ASSINATURAS: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());

    /// O ambiente mínimo que o core precisa, respondendo como um frontend de verdade.
    ///
    /// O que não temos responde `false` — é o que o RetroArch faz com o que não conhece, e é
    /// assim que o caminho de recusa do core também fica exercitado.
    unsafe extern "C" fn ambiente(cmd: u32, dados: *mut c_void) -> bool {
        match cmd {
            // Aceita RGB565 e recusa o resto: é o formato que o console entrega, e recusar os
            // outros faz o core seguir pelo caminho que ele usa no RetroArch.
            ENV_SET_PIXEL_FORMAT => {
                !dados.is_null() && unsafe { *(dados as *const u32) } == PIXEL_FORMAT_RGB565
            }
            ENV_GET_SYSTEM_DIRECTORY | ENV_GET_SAVE_DIRECTORY => {
                // O sistema pode vir de fora, e vem por `ZEEBX_CORE_SISTEMA`: é assim que o teste
                // encontra o aparelho de verdade, com os jogos instalados, e consegue exercitar o
                // ciclo da Z-Wheel de ponta a ponta. Sem a variável, cada um recebe a pasta
                // temporária do teste.
                let pasta = if cmd == ENV_GET_SYSTEM_DIRECTORY {
                    SISTEMA.get().or_else(|| PASTA.get())
                } else {
                    PASTA.get()
                };
                let Some(pasta) = pasta else {
                    return false;
                };
                if dados.is_null() {
                    return false;
                }
                // SAFETY: o core promete um `*const *const c_char` para escrita.
                unsafe { *(dados as *mut *const c_char) = pasta.as_ptr() };
                true
            }
            _ => false,
        }
    }

    unsafe extern "C" fn video(dados: *const c_void, largura: u32, altura: u32, passo: usize) {
        QUADROS.fetch_add(1, Ordering::Relaxed);
        // Assinatura barata do quadro: muda quando a imagem muda, que é o que o teste precisa
        // saber para dizer se a entrada chegou ao guest — um controle que não chega deixa a tela
        // parada, e um botão errado também, e as duas coisas se separam olhando o resto.
        let total = (passo as u64) * u64::from(altura);
        let bytes =
            unsafe { std::slice::from_raw_parts(dados as *const u8, total.min(1 << 22) as usize) };
        let mut assinatura = 1469598103934665603u64;
        for &b in bytes.iter().step_by(97) {
            assinatura = (assinatura ^ u64::from(b)).wrapping_mul(1099511628211);
        }
        if let Ok(mut vistos) = ASSINATURAS.lock() {
            vistos.push(assinatura);
        }
        let _ = (largura, altura);
    }

    unsafe extern "C" fn audio(_dados: *const i16, quadros: usize) -> usize {
        quadros
    }

    unsafe extern "C" fn entrada(_porta: u32, _dispositivo: u32, _indice: u32, id: u32) -> i16 {
        // O core pergunta pelo estado de cada botão, um a um.
        (BOTAO.load(Ordering::Relaxed) == id) as i16
    }

    extern "C" fn sem_poll() {}

    /// **O core, exercitado pela própria ABI.**
    ///
    /// É o teste que faltava para o ciclo da Z-Wheel do lado do core: o motor é medido pela
    /// varredura, e o laço do core — que troca de sessão quando o shell pede — não tinha prova
    /// automática nenhuma. Aqui não há janela nem RetroArch, mas o caminho é o mesmo:
    /// `retro_init`, `retro_load_game`, quadros e `retro_unload_game`.
    ///
    /// **O que este teste prova, e o que ele não prova.**
    ///
    /// Prova: o core carrega conteúdo pela própria ABI, entrega quadros e desmonta limpo, sem
    /// RetroArch e sem janela. Com a Z-Wheel, entrega 360 quadros.
    ///
    /// **Não prova a troca de sessão.** Dirigindo o controle pelas seis teclas do RetroPad — 30
    /// quadros por tecla — nenhuma imagem distinta aparece (medido: 1 por fase) e o shell não pede
    /// abertura nenhuma. Ou a Z-Wheel headless não chega ao estado em que aceita a escolha, ou a
    /// entrada não chega ao guest pelo caminho do core. As duas hipóteses estão abertas e este
    /// teste **não** escolhe entre elas: quem fecha o ciclo é o RetroArch, com controle de
    /// verdade, que é o que o item 8 do plano pede.
    ///
    /// **Sem `ZEEBX_CORE_ROM` ele não roda.** ROM não entra na árvore do repositório, e um teste
    /// que baixa conteúdo sozinho é pior que um teste que não roda.
    ///
    /// ```bash
    /// ZEEBX_CORE_ROM="roms/Z-Wheel.zip" cargo test -p zeebx-libretro -- --nocapture
    /// ```
    #[test]
    fn a_abi_do_core_roda_uma_rom() {
        let Ok(caminho) = std::env::var("ZEEBX_CORE_ROM") else {
            eprintln!("sem ZEEBX_CORE_ROM: nada a rodar");
            return;
        };
        let pasta = std::env::temp_dir().join(format!("zeebx-core-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let _ = PASTA.set(CString::new(pasta.to_string_lossy().to_string()).unwrap());
        if let Ok(fora) = std::env::var("ZEEBX_CORE_SISTEMA") {
            let _ = SISTEMA.set(CString::new(fora).unwrap());
        }
        QUADROS.store(0, Ordering::Relaxed);

        let caminho_c = CString::new(caminho.clone()).unwrap();
        let quadros_pedidos = std::env::var("ZEEBX_CORE_QUADROS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(60u32);

        let mut abriu = ULTIMA_ABERTURA.load(Ordering::Relaxed);
        let mut qual = None;
        // Antes de dirigir o controle não pode haver pedido de abertura nenhum: se houver, ele
        // veio do carregamento do conteúdo, e a leitura do laço abaixo estaria medindo outra coisa.
        assert_eq!(abriu, 0, "o shell pediu abertura antes de qualquer tecla");
        assert!(qual.is_none());
        if let Ok(mut vistos) = ASSINATURAS.lock() {
            vistos.clear();
        }
        let info = RetroGameInfo {
            path: caminho_c.as_ptr(),
            data: std::ptr::null(),
            size: 0,
            meta: std::ptr::null(),
        };
        unsafe {
            retro_set_environment(Some(ambiente));
            retro_set_video_refresh(Some(video));
            retro_set_audio_sample_batch(Some(audio));
            retro_set_input_poll(Some(sem_poll));
            retro_set_input_state(Some(entrada));
            retro_init();
            retro_set_controller_port_device(0, DEVICE_JOYPAD);
            assert!(retro_load_game(&info), "o core recusou {caminho}");
            for _ in 0..quadros_pedidos {
                retro_run();
            }
            // **Dirige a Z-Wheel.** A pergunta desta parte é prática: com que botão o jogador
            // confirma a escolha, e o pedido de abertura chega ao core? Cada botão do RetroPad é
            // segurado por vinte quadros e solto por dez, e o teste para no primeiro que o shell
            // aceitar. Sem o pedido, ele diz que nenhum serviu — que também é resposta.
            abriu = ULTIMA_ABERTURA.load(Ordering::Relaxed);
            qual = None;
            for botao in [ID_A, ID_B, ID_X, ID_Y, ID_START, ID_SELECT] {
                let antes = QUADROS.load(Ordering::Relaxed);
                BOTAO.store(botao, Ordering::Relaxed);
                for _ in 0..20 {
                    retro_run();
                }
                BOTAO.store(u32::MAX, Ordering::Relaxed);
                for _ in 0..10 {
                    retro_run();
                }
                // A imagem mudou enquanto o botão estava apertado? É o que separa "a entrada não
                // chega" de "chega, e o botão é outro".
                let distintas = ASSINATURAS
                    .lock()
                    .map(|v| {
                        let inicio = (antes as usize).min(v.len());
                        v[inicio..].iter().collect::<std::collections::BTreeSet<_>>().len()
                    })
                    .unwrap_or(0);
                eprintln!("botão {botao}: {distintas} imagem(ns) distinta(s) em 30 quadros");
                abriu = ULTIMA_ABERTURA.load(Ordering::Relaxed);
                if abriu != 0 {
                    qual = Some(botao);
                    break;
                }
            }
            retro_unload_game();
            retro_deinit();
        }
        eprintln!(
            "pedido de abertura: {abriu:#010x} ({})",
            match abriu {
                0 => "nenhum botão abriu".to_string(),
                _ => format!("com o botão {qual:?}"),
            }
        );

        let quadros = QUADROS.load(Ordering::Relaxed);
        assert!(quadros > 0, "nenhum quadro chegou ao frontend");
        eprintln!("quadros entregues ao frontend: {quadros}");
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
