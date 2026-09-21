# Plano Libretro — Zeebx

> Estado: planejamento técnico. Nenhum core Libretro foi implementado ainda.
>
> Branch: `feat/libretro-core`, baseada em `upstream/development` no commit `6c74975` (`v0.2.1`).

## Objetivo

Entregar o Zeebx como um core Libretro para Linux **x86_64** e **AArch64**, inicialmente para
RetroArch. O core recebe um jogo Zeebo (`.mod`, `.zip`, `.7z`), executa o
applet BREW, entrega vídeo, áudio e input ao frontend, e mantém conteúdo, saves e dados do
"aparelho" em locais separados.

O primeiro alvo é um core software portátil. Janela nativa, OpenGL de host, CPAL, gamepads do
host e UI desktop não fazem parte do core.

## Estado atual medido

O projeto é um binário Rust único (`src/main.rs`), sem alvo `lib`.

Peças que podem ser reutilizadas:

- `src/session.rs::Session`: ciclo de vida de módulo/applet, Dynarmic, timers, callbacks,
  framebuffer e estado de entrada;
- `src/input/mod.rs::Pad`: estado dos botões/eixos do Z-Pad;
- `src/audio/mod.rs::Mixer`: mistura de vozes e streams; `Mixer::silent(rate)` serve a hosts
  sem dispositivo de áudio;
- `src/video/display.rs::Framebuffer`: a tela atual é 640×480 RGB565;
- `src/loader/archive.rs`: seleção segura do `.mod` em ZIP e extração de árvore inteira;
- `src/brew/vfs.rs::Vfs`: semântica BREW de `fs:/`, `fs:/~/` e caminhos relativos.

Acoplamentos a remover do caminho Libretro:

- `Session::start_with()` recebe `Option<Arc<eframe::glow::Context>>`;
- `Machine::usa_placa()` depende de `eframe::glow`;
- `Session` usa `ui::library` e `ui::settings::ZWheel`;
- `loader::archive`, widgets e SQL usam `ui::settings::config_dir()`;
- dependências desktop atuais incluem `eframe`, `glutin`, `minifb`, `rfd`, `gilrs`, `cpal` e
  Discord.

O `Session::step(Duration, speed_limit)` também **não** é a unidade correta para
`retro_run()`: ele mede/limita tempo real com `Instant`, recupera atraso e encerra por orçamento
de parede. Um core Libretro precisa de passo virtual determinístico.

## Decisões de MVP

| Assunto | Decisão |
|---|---|
| CPU | Dynarmic, como a `Session` atual |
| Vídeo | software RGB565, 640×480, 4:3 |
| Áudio | estéreo PCM16, 44 100 Hz; a quantidade por `retro_run` sai do tempo **virtual** decorrido |
| Entrada | até duas portas RetroPad → Z-Pad |
| Conteúdo inicial | `.mod` e `.zip` |
| `.7z` | feito: decodificador embutido, limites iguais aos do zip e prova com jogo real |
| Renderização HW | fora do MVP |
| Save states | fora do MVP; API retorna sem suporte |
| Core Options | nenhuma até existir opção funcional real |
| Dados persistentes | pasta `zeebx/` sob diretório de saves do frontend |
| Rede do guest | desligada por padrão; nenhuma conexão implícita do frontend |

## Arquitetura proposta

```text
src/lib.rs                         motor compartilhado (`zeebx` rlib)
src/main.rs                        CLI e frontend desktop
src/session.rs                     Session desktop + driver determinístico Libretro
src/storage.rs                     raízes persistentes, identidade e layout de conteúdo
src/loader/archive.rs              ZIP/7z, validação, cache e extração atômica
src/brew/vfs.rs                    conteúdo read-only + overlay de saves + aparelho
frontends/libretro/Cargo.toml      pacote/alvo independente `zeebx-libretro`
frontends/libretro/src/lib.rs      estado do core e exports C ABI (`cdylib`)
frontends/libretro/src/bindings.rs bindings gerados e versionados de libretro.h
frontends/libretro/include/        `libretro.h` oficial fixado em revisão conhecida
frontends/libretro/zeebx_libretro.info
                                  metadados distribuídos do core
docs/libretro/LIBRETRO_PLAN.md     este plano
```

A raiz torna-se workspace Cargo. O pacote raiz `zeebx` expõe o motor como `rlib` e mantém o
binário desktop; `frontends/libretro` é outro pacote, depende de `zeebx` por path e é o único que
produz `cdylib`. Isto impede dependências/exports Libretro de vazarem para a CLI e deixa o target
visível numa pasta própria.

### Features e workspace Cargo

Esboço:

```toml
# Cargo.toml raiz
[workspace]
members = [".", "frontends/libretro"]
resolver = "2"

[package]
name = "zeebx"

[features]
default = ["desktop"]
desktop = ["dep:eframe", "dep:glutin", "dep:minifb", "dep:rfd", "dep:gilrs", "dep:cpal"]

# frontends/libretro/Cargo.toml
[package]
name = "zeebx-libretro"

[lib]
name = "zeebx_libretro"
crate-type = ["cdylib"]

[dependencies]
zeebx = { path = "../..", default-features = false }
```

O build do core deve funcionar isoladamente:

```bash
cargo build --locked --release -p zeebx-libretro
```

Não basta o core não *usar* dependências desktop: elas precisam ser opcionais no motor e os
módulos que as importam precisam estar atrás de `cfg(feature = "desktop")`.

## Driver de frame determinístico

Criar caminho específico, por exemplo:

```rust
pub fn Session::run_libretro_frame(&mut self) -> FrameResult;
```

Ele deve:

1. não consultar `Instant` nem aplicar freio de relógio de parede;
2. entregar o `EVT_APP_START` na primeira execução, depois que áudio e input estiverem prontos;
3. executar a volta BREW: `advance`, sinais, callbacks, threads, timers e apresentações;
4. respeitar o tempo virtual/vblank do emulador;
5. guardar a última tela e telas intermediárias de `IDISPLAY_Update`;
6. retornar estado de parada sem pânico;
7. informar progresso de tempo virtual para o adaptador de áudio.

`Session::step()` permanece para a UI desktop. Implementar `retro_run()` chamando
`step(Duration::from_millis(16), ...)` é proibido: produziria timing, quantidade de áudio e
progresso de CPU dependentes do desempenho do host.

Antes de implementar a ABI, criar testes que executem número fixo de frames duas vezes e comparem
relógio virtual, estado de entrada e checksum do framebuffer.

## Contrato Libretro

Vendorizar `libretro.h` oficial numa revisão fixada. Gerar bindings Rust uma vez e versionar o
arquivo gerado, de modo que o build normal não dependa de `libclang`. Validar layouts/offsets nos
alvos x86_64 e AArch64.

Todo export deve usar ABI C e não deixar `panic` atravessar a fronteira:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_...()
```

Entradas públicas precisam de fronteira de erro/pânico segura e log pelo frontend quando a
interface de log estiver disponível.

### Exports base obrigatórios

| Grupo | Funções |
|---|---|
| Callbacks | `retro_set_environment`, `retro_set_video_refresh`, `retro_set_audio_sample`, `retro_set_audio_sample_batch`, `retro_set_input_poll`, `retro_set_input_state` |
| Ciclo | `retro_init`, `retro_deinit`, `retro_api_version`, `retro_get_system_info`, `retro_get_system_av_info` |
| Execução | `retro_set_controller_port_device`, `retro_reset`, `retro_run` |
| Estado | `retro_serialize_size`, `retro_serialize`, `retro_unserialize` |
| Cheats | `retro_cheat_reset`, `retro_cheat_set` |
| Conteúdo | `retro_load_game`, `retro_load_game_special`, `retro_unload_game` |
| Região/memória | `retro_get_region`, `retro_get_memory_data`, `retro_get_memory_size` |

Mesmo APIs sem recurso precisam existir.

### Comportamento MVP de APIs sem recurso

| API | Resultado |
|---|---|
| `retro_load_game_special` | `false`; não há subsistemas |
| `retro_cheat_reset`/`retro_cheat_set` | no-op |
| `retro_serialize_size` | `0` |
| `retro_serialize`/`retro_unserialize` | `false` |
| `retro_get_memory_data` | `NULL` |
| `retro_get_memory_size` | `0` |
| `retro_get_region` | `RETRO_REGION_NTSC` |

`retro_reset()` não deve ser no-op. Enquanto não existir reset interno seguro, ele recria sessão a
partir do caminho e configuração original, preservando saves e dados persistentes.

`retro_unload_game()` deve destruir sessão, mixer, buffers e conteúdo temporário. `retro_deinit()`
também deve limpar estado se o frontend chamou a sequência incompleta.

### Ordem do ciclo de vida

```text
retro_set_environment
retro_init
retro_set_video_refresh / audio / input setters
retro_load_game
retro_get_system_av_info
retro_run*
retro_unload_game
retro_deinit
```

`retro_set_environment()` é chamado antes de `retro_init()`. Os ponteiros/string de
`retro_get_system_info()` e descritores devem ter vida estática. `retro_load_game()` deve recusar
ponteiro ou caminho nulo e limpar sessão anterior caso o frontend viole o fluxo.

## Informação de sistema e conteúdo

`retro_get_system_info()` deve anunciar:

```text
library_name      = "Zeebx"
library_version   = versão do Cargo
valid_extensions  = "mod|zip|7z"
need_fullpath     = true
block_extract     = true
supports_no_game  = false
```

`.mif` não é conteúdo inicializável. É manifesto lateral que identifica o applet de um `.mod`.

`block_extract=true` é obrigatório para ZIP/7z: o core precisa receber o arquivo compactado
inteiro e selecionar o módulo correto, preservando `.mif` e todos os recursos vizinhos. Não usar
`game->data`/`game->size` no caminho com `need_fullpath=true`.

`need_fullpath` não autoriza assumir que `game->path` é caminho POSIX acessível por `std::fs`: um
frontend pode fornecer caminho VFS/SAF. O core precisa obter VFS antes do load e ler conteúdo,
cache, saves e system directory pelo adaptador VFS.

Ao carregar, negociar `RETRO_ENVIRONMENT_SET_PIXEL_FORMAT` para `RETRO_PIXEL_FORMAT_RGB565`.
Recusar carregamento se o frontend não aceitar o formato.

## Vídeo

Configuração inicial de `retro_system_av_info`:

```text
base_width   = 640
base_height  = 480
max_width    = 640
max_height   = 480
aspect_ratio = 4.0 / 3.0
fps          = 60.0
sample_rate  = 44100.0
```

Cada `retro_run()` deve chamar o callback de vídeo. Se o jogo não produziu nova tela, enviar o
último framebuffer é a regra MVP mais simples. `NULL` para duplicar frame só deve ser usado após
negociar explicitamente essa capacidade do frontend.

Para RGB565:

```text
width = 640
height = 480
pitch = 1280
```

Se resolução ou geometria passar a variar, chamar `RETRO_ENVIRONMENT_SET_GEOMETRY`.

### Renderização hardware futura

Não usar Glutin/Eframe no core. Uma variante GL real exigirá:

- `RETRO_ENVIRONMENT_SET_HW_RENDER`;
- callbacks `context_reset` e `context_destroy`;
- recriação de todo recurso de GL após perda de contexto;
- funções via `get_proc_address` do frontend;
- framebuffer do frontend e `RETRO_HW_FRAME_BUFFER_VALID`.

Isto é uma segunda rota de vídeo, não uma opção trivial sobre o MVP software.

## Áudio

Libretro espera PCM16 estéreo intercalado no callback batch. O `Mixer` atual gera `f32`; o
adaptador deve aplicar clamp e converter para `i16`.

Configuração MVP:

```text
44 100 Hz, estéreo; a cada retro_run o core entrega (ms virtuais decorridos x 44,1) amostras
```

Fluxo:

1. criar `Mixer::silent(44_100)` em `retro_load_game()`;
2. conectá-lo à sessão antes do primeiro `EVT_APP_START`;
3. renderizar a quantidade associada ao passo virtual;
4. chamar `retro_audio_sample_batch`;
5. tratar retorno curto em frames com fila limitada e política documentada de descarte;
6. nunca bloquear `retro_run()` esperando áudio.

Não usar callback assíncrono de áudio do ambiente. Vídeo, áudio e input precisam sair da thread
síncrona normal do core.

## Entrada e controladores

Registrar:

```text
RETRO_ENVIRONMENT_SET_CONTROLLER_INFO
RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS
```

Porta Libretro 0 mapeia porta Zeebo 0; porta 1 mapeia porta Zeebo 1. O aparelho configurado antes
do load deve entrar em `Session::start_software()`, pois jogos podem enumerar HID já no
`EVT_APP_START`.

Mapeamento inicial:

| RetroPad | Zeebo |
|---|---|
| D-pad | `input::DPAD` |
| Y | `b1` |
| B | `b2` |
| X | `b3` |
| A | `b4` |
| L | `zl` |
| R | `zr` |
| Start | `start` |
| Select | `back`/HOME |
| analógico esquerdo X/Y | `Pad::axes[0]`/`[1]` |
| analógico direito X/Y | `Pad::axes[2]`/`[3]`, quando aplicável |

Em cada `retro_run()`:

1. chamar `input_poll()` pelo menos uma vez;
2. consultar joypad e analógicos das portas ativas;
3. normalizar valores `i16` para o curso de `Pad::set_axis()` (`-128..=128`);
4. aplicar os dois `Pad` antes de executar guest.

Input bitmask é otimização futura. Poll individual é válido e mais simples para o primeiro core.

### Aparelhos Zeebo por porta

O emulador já distingue quatro aparelhos que o guest pode enumerar por USB/HID:

| Aparelho Zeebo | Identidade/efeito atual | Fonte Libretro |
|---|---|---|
| Dragon (`Controle`) | joystick `1EAA:0135` | RetroPad padrão |
| Z-Pad | joystick `1A5C:3033`; botões/eixos iguais ao Dragon | subtipo RetroPad `Z-Pad` |
| Boomerang | receptor compartilhado, botões limitados e acelerômetro | subtipo RetroPad `Boomerang` + sensor/fallback |
| Teclado USB | enumeração HID de teclado e eventos AVK | subtipo `Teclado USB` + RetroKeyboard |

Registrar `RETRO_ENVIRONMENT_SET_CONTROLLER_INFO` para as duas portas. Cada lista deve conter
`Nenhum`, `Dragon`, `Z-Pad`, `Boomerang` e `Teclado USB`. Os três primeiros controles de jogo
podem ser subclasses de `RETRO_DEVICE_JOYPAD` usando `RETRO_DEVICE_SUBCLASS`; o core deve, como a
spec pede, continuar fazendo poll do tipo-base (`RETRO_DEVICE_JOYPAD`/`RETRO_DEVICE_ANALOG`) e
usar o subtipo apenas para definir o aparelho que o guest enxerga. `retro_set_controller_port_device`
atualiza `Machine::set_portas()` e reapresenta input descriptors; para títulos que enumeram HID no
boot, documentar reset obrigatório após trocar aparelho.

Boomerang não é um Z-Pad completo. Expor somente D-pad, `b1`, `b2` e HOME nos descritores dele,
pois estes são os bits que o receptor físico entrega. O guest já recebe relatório específico com
contador, alternância de jogador e sinal periódico de posição; preservar esse protocolo em vez de
transformar aceleração em eixos de Z-Pad.

### Movimento e Boomerang

O standalone hoje lê Wii Remote e IMUs de controles diretamente de `/dev/input` em Linux. O core
Libretro **não deve** reutilizar esses leitores: o frontend é dono de dispositivos de host, e abrir
um Wii Remote/IMU por fora falha em sandbox, duplica input e só funcionaria em parte dos sistemas.

No `retro_load_game()`, quando uma porta está configurada como Boomerang, negociar
`RETRO_ENVIRONMENT_GET_SENSOR_INTERFACE`. Se existir, chamar `set_sensor_state(porta,
RETRO_SENSOR_ACCELEROMETER_ENABLE, 100)` e, a cada `retro_run()`, ler
`ACCELEROMETER_X/Y/Z` para alimentar `Session::set_port_motion(porta, aceleracao)`. Os 100 Hz
pedidos combinam com o ritmo do receptor emulado; a máquina pode continuar emitindo os pacotes no
ritmo virtual mesmo que o frontend só atualize a amostra em 60 Hz.

A API de sensor Libretro é experimental, mas a convenção oficial é aceleração em **g**, incluindo
gravidade: parado, aproximadamente `[0, 0, 1]`; +X é direita, +Y é cima e +Z aponta ao usuário
visto de frente. A orientação física do controle e o referencial do Boomerang continuam distintos.
Portanto o adaptador precisa:

- manter calibração por porta em `<save-dir>/zeebx/input.json`;
- normalizar a leitura parada para `[0, 0, 1] g`;
- aplicar a rotação conhecida do protocolo Boomerang e orientação configurável, inclusive inversão
de eixos;
- usar último valor válido, não zero, entre leituras;
- desabilitar sensor em `retro_unload_game()`;
- informar claramente quando o frontend não tem sensor ou recusa habilitá-lo.

Um controle/Wii Remote conectado ao host só fornecerá movimento se o **frontend Libretro** o expor
por essa interface. Isso deve ser testado em RetroArch por plataforma; não é correto prometer
suporte universal a Wii Remote apenas porque o standalone Linux o lê.

Como fallback portátil, oferecer opção real por porta somente depois do MVP:

```text
zeebx_boomerang_p1_source = sensor | analog_right | static
zeebx_boomerang_p2_source = sensor | analog_right | static
```

`analog_right` converte o analógico direito em inclinação X/Y e conserva gravidade Z; `static`
entrega `[0, 0, 1]`. Não habilitar essa opção até calibrar e testar jogos Boomerang. Giroscópio não
é necessário ao protocolo conhecido; não pedir `RETRO_SENSOR_GYROSCOPE_*` sem medida que o use.

### Teclado USB e eventos BREW

`IHID` não cobre todas as entradas BREW. A Z-Wheel, em particular, pode esperar `AVK_0` ou
`AVK_CLR`; só entregar estado de Z-Pad pode deixá-la sem navegação.

Quando a porta estiver configurada como `Teclado USB`, registrar
`RETRO_ENVIRONMENT_SET_KEYBOARD_CALLBACK` e manter uma fila de transições. O callback não executa
guest diretamente; `retro_run()` drena a fila e chama `Session::set_key(avk, down)`. Isto evita
reentrância e mantém todo acesso ao emulador na thread normal do core. Usar o callback para
transições/texto, sem confundir `keycode` com caractere Unicode: uma tecla pode gerar múltiplos
caracteres, e eventos só de tecla ou só de caractere são válidos. Também fazer poll de
`RETRO_DEVICE_KEYBOARD` para estado sustentado, como teclas direcionais mantidas.

Mapeamento mínimo a testar/documentar:

| Tecla Libretro | AVK BREW |
|---|---|
| setas | `AVK_UP`, `AVK_DOWN`, `AVK_LEFT`, `AVK_RIGHT` |
| Enter | `AVK_SELECT` |
| Backspace/Escape | `AVK_CLR` |
| `0`–`9` | `AVK_0`–`AVK_9` |
| `*`/`#` | `AVK_STAR`/`AVK_POUND` |

RetroPad Select pode gerar `AVK_CLR` na rota de Z-Wheel como atalho, mas não substitui teclado
real. O teclado físico é global no Libretro; a seleção de `Teclado USB` na porta decide apenas o
que o guest enumera. Suportar duas portas de teclado distintas exigiria prova de que o BREW e a ABI
Libretro conseguem distingui-las.

### Mouse USB

Não anunciar `RETRO_DEVICE_MOUSE`, `RETRO_DEVICE_POINTER` ou mouse USB no MVP. O código atual não
implementa aparelho `Mouse`, enumeração HID de mouse, UIDs, botões ou eventos BREW correspondentes;
a busca no motor só encontra mouse da interface desktop. Também não há medida no corpus que prove
qual dispositivo USB/método BREW os títulos do Zeebo esperam para mouse.

Suporte futuro a mouse exige primeiro implementar e medir o dispositivo guest. Só então o core
poderá mapear `RETRO_DEVICE_MOUSE` (delta X/Y, botões e roda) ou `RETRO_DEVICE_POINTER` (posição
absoluta/touch) para esse protocolo e acrescentá-lo a `SET_CONTROLLER_INFO`.

## Rede do guest

A `Machine` atual nasce com rede ligada e pode usar `ureq` para atender `IWeb`. Esse padrão é
inadequado num core: abrir uma ROM não pode gerar conexão externa inesperada.

O core Libretro deve iniciar com rede **desligada**, sem ler `ZEEBX_SERVIDOR` nem outras variáveis
do processo. Quando houver uma opção funcional e documentada, ela pode permitir rede por decisão
explícita do usuário; host/porta de redirecionamento devem ser controlados pelo core, não por
ambiente global. Rede não deve ser chamada de thread auxiliar e falhas devem voltar ao guest como
erro BREW, sem pânico.

## Armazenamento, ROMs e savegames

### Layout Libretro

Em `retro_set_environment()`, antes de pedir quaisquer diretórios, negociar
`RETRO_ENVIRONMENT_GET_VFS_INTERFACE` com versão requerida 3. Essa versão traz criação e
listagem de diretórios, necessárias para cache/overlay. Guardar apenas interface emprestada pelo
frontend e encapsular operações atrás de `StorageFs`; nenhum caminho retornado pelo frontend pode
ser passado diretamente a `std::fs`/`fopen`.

`StorageFs` precisa cobrir open/read/write/seek, stat, mkdir, listagem, rename e remoção. Ele
precisa também representar `StoragePath` sem assumir `PathBuf`: um `saf://` pode não aceitar
`parent()`/`join()` nativos. Descoberta de `.mif`, recursos irmãos e extensão deve usar listagem e
junção de caminho fornecidas pelo adaptador. ZIP/7z devem receber leitor VFS; o ZIP precisa de
adaptador `Read + Seek`. Para os arquivos comuns, o cache/overlay usa operações VFS e rename
atômico também em caminhos SAF/Android; a compatibilidade completa desses caminhos continua
condicionada à solução SQLite descrita abaixo. Só materializar arquivo temporário local se o
frontend autorizar localização nativa e o arquivo for removido no unload.

O perfil usa **os dois** diretórios que o frontend fornece, com a mesma divisão que o PPSSPP faz
entre `flash0` (sistema) e o memory stick (saves):

```text
<system-dir>/zeebx/            # o que é da máquina ou descartável
├── aparelho/                  # fs:/ compartilhado entre títulos (a NAND do console)
└── cache/<conteúdo>/          # extração do pacote: descartável e o que enche disco

<save-dir>/zeebx/              # o que o jogador quer guardar
├── saves/<conteúdo>/          # overlay gravável do título
└── metadata/<conteúdo>.json   # índice de origem e identidade
```

**O nome `zeebx` é fixo e minúsculo.** Nunca pode sair do nome de exibição do core: o RetroArch já
cria as pastas dele com esse nome — `states/Zeebx`, `saves/Título.srm` — e num host que distingue
caixa (`saves/zeebx` e `saves/Zeebx` são pastas diferentes) o jogo passaria a ter dois perfis.

Sem `GET_SYSTEM_DIRECTORY` tudo cai no diretório de saves, que é o mínimo que a ABI garante.
Não alterar `HOME`, `XDG_CONFIG_HOME` ou diretório corrente do processo do frontend.

### Disco cheio

O cache guarda a extração de **todo** título já aberto; sem limite, um acervo de 62 jogos passa de
1,5 GB dentro do diretório do frontend e enche o disco de quem só queria jogar. Medido nesta
máquina: 66 zips somam 1,6 GB extraídos.

Regra: o core poda o cache a cada carga, mantendo a extração em uso e apagando as mais antigas até
caber em `CACHE_LIMIT_BYTES` (512 MB). A extração em uso nunca é removida, nem que sozinha estoure
o teto — apagar o jogo em execução seria pior que o disco cheio.

Disco cheio não pode travar o frontend. Duas regras de código sustentam isso:

1. o core **nunca** chama callback do frontend segurando um mutex próprio: os ponteiros são copiados
   antes da chamada. Um aviso do frontend ("disco cheio, quer salvar?") deixava de travar quando o
   `retro_run` passou a soltar o cadeado antes de vídeo e áudio;
2. falha de escrita no VFS é erro do guest, não pânico: o jogo recebe `IFAILED` e segue, e o motivo
   fica no relatório.

Se `GET_SAVE_DIRECTORY` não retornar diretório gravável, `retro_load_game()` deve falhar com
mensagem clara. Não usar a pasta da ROM como fallback e não aceitar execução que prometa save mas
o perca em diretório temporário.

### Três camadas

| Camada | Onde | Persistente | Compartilhada | Conteúdo |
|---|---|---:|---:|---|
| `cache/<conteúdo>/` | system | descartável | não | ROM extraída, `.mod`, `.mif`, assets |
| `saves/<conteúdo>/` | save | sim | não | arquivos relativos gravados pelo jogo |
| `aparelho/` | system | sim | sim | `fs:/`, Z-Wheel, dados globais |

Isto preserva o comportamento medido do console: Zeeboids pode gravar em
`fs:/zeeboiddata`, e Zeebo F.C. pode ler esses dados em outra sessão.

### VFS em camadas

Substituir raiz única atual por algo equivalente a:

```rust
pub struct Vfs {
    content_root: PathBuf, // somente leitura lógica
    save_root: PathBuf,    // overlay por jogo
    device_root: PathBuf,  // fs:/ compartilhado
    boundary: PathBuf,     // limite seguro de fs:/~/../
}
```

Regras:

- leitura relativa/`fs:/~/`: overlay primeiro, conteúdo base depois;
- criação/escrita relativa: sempre no overlay;
- `READWRITE`/`APPEND` de arquivo que existe só na base: copy-on-write para overlay;
- mapear `READWRITE`/`APPEND` ao modo VFS `UPDATE_EXISTING`, pois abrir write sem esse flag
  trunca arquivo existente;
- remoção de arquivo da base: criar tombstone no overlay, para que o arquivo não reapareça na
  próxima listagem/abertura;
- listagem: união overlay + base menos tombstones; overlay vence, comparação sem caixa e ordem
  determinística;
- `fs:/...`: `device_root`;
- `fs:/mod/...`: manter fallback atual entre aparelho e instalação;
- nunca alterar ROM, ZIP/7z original ou cache de conteúdo;
- preservar limites de `..`, caminhos absolutos e case-insensitivity do VFS atual.

A seleção do caminho deve ocorrer depois de conhecer o modo de abertura. Portanto,
`machine/file.rs` não pode escolher `vfs.resolve()` antes de saber se a operação é leitura,
criação, append ou read-write.

### Limite crítico: APIs de arquivo do motor e SQLite

A adaptação não é só trocar `loader/archive.rs`. Hoje o caminho de core usa `std::fs`/`PathBuf` em
`Session`, archive, `Vfs`, `machine/file.rs`, shell, mídia, fontes, widgets e SQL; `Machine` mantém
`std::fs::File` aberto. Além disso, `rusqlite` abre bancos por caminho nativo e pode criar journal
ou WAL. Um path `saf://` do frontend não é válido para essas APIs.

Antes de prometer Android/SAF, criar uma camada `StorageFs`/`GuestFile` usada por **toda** E/S do
motor, com backend nativo para desktop e backend `retro_vfs_interface` para frontend. Fazer
inventário de cada chamada `std::fs` que ainda entra no caminho de core; UI, testes e CLI podem
continuar nativos atrás da feature `desktop`.

SQLite exige decisão explícita:

1. implementar uma SQLite VFS sobre `retro_vfs_interface`; ou
2. materializar cópia transacional do banco num diretório nativo autorizado, abrir com SQLite e
   sincronizar de volta pelo VFS, incluindo journal/WAL e falhas de commit.

A opção 2 só é válida onde o frontend fornece local nativo autorizado; não serve para tornar
`SAF` magicamente compatível. Enquanto a opção não existir, o MVP deve declarar suporte a VFS
**nativo** (Linux x86_64/AArch64) e não prometer carga por SAF/Android. O critério de Android entra
depois de testes de SQLite, VFS, ZIP e save real nessa plataforma.

### Identidade

O fingerprint atual (`nome + tamanho + mtime`) não basta. Para ZIP/7z:

```text
content_hash = BLAKE3(bytes do arquivo compactado)
```

Para `.mod` direto, usar ao menos bytes do `.mod`, `.mif` associado e contexto de origem.

Diretório humano:

```text
<titulo-normalizado>-<classid>-<hash-curto>
```

Hash completo permanece na metadata. Atualização de ROM ganha save próprio por segurança; dados
globais do aparelho permanecem compartilhados.

## Extração ZIP e 7z

Não basta extrair `.mod` e `.mif`. Recursos, bancos, fontes e áudio podem ser abertos depois do
boot. Extrair toda a árvore validada.

ZIP já tem base em `loader/archive.rs`. Para 7z, usar decoder embutido/Rust ou biblioteca
portável. Não chamar binário externo `7z`: isto quebra AArch64, Android, sandbox e distribuição
portátil.

Antes de anunciar `7z`, validar com arquivos reais. Arquivos criptografados sem senha devem ser
recusados com mensagem clara.

Requisitos de segurança para ambos os formatos:

- recusar caminhos absolutos e `..`;
- recusar symlinks, hardlinks e nós especiais;
- limitar número de entradas;
- limitar tamanho de entrada e total descompactado;
- não extrair arquivo dentro de arquivo recursivamente;
- extrair para `<hash>.partial`;
- validar `.mod`/`.mif` e manifesto;
- renomear atomicamente para `<hash>` após sucesso;
- lock por hash para instâncias concorrentes.

### Cache ou `/tmp`

O cache fica em `<system-dir>/zeebx/cache/`, persistente entre sessões, para não descomprimir o
pacote em toda abertura — e **fora** do diretório de saves, que é o que o frontend sincroniza. Ele é
podado a 512 MB a cada carga (`CACHE_LIMIT_BYTES`), preservando a extração em uso.

Modo futuro opcional:

```text
zeebx_extract_mode = cache | temporary
```

No modo `temporary`, conteúdo pode ir para diretório temporário privado e ser removido no unload,
mas saves e `aparelho` continuam sempre sob `<save-dir>/zeebx/`. Nunca gravar save em `/tmp`.

### Concorrência e integridade

`aparelho/` representa um único console virtual e é compartilhado entre títulos. Duas instâncias
simultâneas podem corromper banco SQLite, configuração ou saves globais. A ABI VFS atual não oferece
lock exclusivo portável, portanto o core **não pode prometer** que bloqueará segunda instância em
SAF/sandbox só criando `<save-dir>/zeebx/.lock>`.

Implementar lock de perfil como melhoria best-effort no backend nativo, e tratar concorrência como
não suportada/documentada nos backends VFS sem lock atômico. O lock de cache por hash segue
separado: ele coordena apenas extração quando o backend permitir criação exclusiva. Não usar simples
"arquivo existe" como garantia de exclusão mútua.

Escritas de metadata, manifestos, tombstones e saves produzidos pelo core devem usar arquivo
provisório + rename atômico quando o backend VFS garantir essa operação. No unload, fechar e
sincronizar arquivos que o guest ainda mantém abertos. SQLite e outros writers precisam de política
própria de commit/journal; rename atômico de arquivos externos não protege seus bancos abertos.

## Environment callbacks úteis

| Callback | Uso |
|---|---|
| `SET_PIXEL_FORMAT` | RGB565 obrigatório no MVP |
| `GET_SAVE_DIRECTORY` | raiz de `zeebx/` |
| `GET_SYSTEM_DIRECTORY` | firmware/dados futuros |
| `GET_LOG_INTERFACE` | logs de load/VFS/erro |
| `SET_CONTROLLER_INFO` | dispositivos das portas |
| `SET_INPUT_DESCRIPTORS` | rótulos RetroPad |
| `SET_GEOMETRY` | somente quando geometria variar |
| `GET_CORE_OPTIONS_VERSION` | somente quando houver opções reais |
| `SET_CORE_OPTIONS_V2` | opções futuras |
| `GET_VARIABLE_UPDATE` | reler opções futuras |
| `GET_VFS_INTERFACE` | obrigatório antes de diretórios/load; toda E/S do core passa por ele |
| `GET_VFS_AUTHORIZED_LOCATIONS` | quando disponível, validar raízes VFS onde cache/arquivo pode existir |

Não registrar opção cosmética ou que não possa ser aplicada de forma segura numa sessão viva.

## Catálogo, identificação e capas

O acervo tem duas camadas de identidade, e as duas existem hoje:

| Identidade | Para que serve | Onde |
|---|---|---|
| BLAKE3 dos bytes do pacote | cache e overlay de saves: a mesma ROM abre no mesmo lugar, uma ROM trocada não herda save | `StoragePaths::content_id` |
| No-Intro (CRC32/MD5/SHA1 de um arquivo dentro do tape) | dizer **qual jogo é**, com o nome oficial, e achar capa | `ferramentas/catalogo.py` |

### O que o No-Intro de Zeebo hasheia

O DAT oficial é `Mobile - Zeebo` (libretro-database, `metadat/no-intro/`). Ele hasheia **um arquivo
dentro de `mod/<pasta>/`**, e o nome no DAT é o caminho interno sem as barras:

```text
mod274754sound.ggz                              -> mod/274754/sound.ggz
mod280386resources.pakz                         -> mod/280386/resources.pakz
modnfsresourcestracksworld_3401.viv             -> mod/nfs/resources/tracks/world_3401.viv
```

A pasta do módulo **nem sempre é numérica** (`mod/nfs/...`); comparar por prefixo erra o Need For
Speed. A comparação é pelo caminho inteiro sem separador.

### Medição contra o acervo do usuário

```text
pacotes: 62
verificados pelo No-Intro: 57 de 57 do DAT (0 divergentes)
fora do DAT: 5
```

Os cinco fora do DAT são os quatro ports da Data East e um homebrew:

- `Bad Dudes vs. DragonNinja`
- `Caveman Ninja`
- `Dark Seal`
- `Karnov's Revenge`
- `Kingdom Hearts V CAST (Zeebo Homebrew)`

### O que a ferramenta escreve

`ferramentas/catalogo.py --roms DIR --saida DIR [--icones]`:

- `Mobile - Zeebo.lpl` — playlist do RetroArch, `label` com o nome No-Intro, `crc32` do pacote e
  `db_name` do banco;
- `thumbnails/Mobile - Zeebo/{Named_Boxarts,Named_Snaps,Named_Titles,Named_Logos}/` — as quatro
  pastas que o RetroArch procura, com o nome exato de cada título;
- `catalogo.json` — hashes, verificação e o caminho do `.mod`/`.mif` de cada pacote.

### As capas oficiais vêm dentro da própria Z-Wheel

O pacote da Z-Wheel traz as capas da loja e o banco que as liga aos jogos:

```text
mod/274755/tt_game_info                     SQLite
  GAMEINFO(game_id, class_id, boxart_path, ...)   -> class_id é o ClassID do applet
  TITLETEXT(game_id, lang_id, titletext)          -> nome oficial por idioma
mod/274755/assets/games/<game_id>/boxartlg.jpg    170x220, a maior publicada
mod/274755/assets/games/<game_id>/boxart.bmp      160x227
```

A chave do cruzamento é o **ClassID do applet**, que o emulador já lê do `.mif` — não é nome de
arquivo nem de pasta. Medido contra o acervo:

```text
capas oficiais gravadas: 58 de 62
sem capa: Kingdom Hearts (homebrew, sem ClassID de applet no .mif)
          Z-Wheel, Zeebo App e Zenonia (não têm ficha na loja)
```

`--zwheel PACOTE` liga esse caminho; `--capas-ao-lado` copia a capa para o lado do `.zip`, que é
onde o frontend **standalone** procura (`library::cover` lê `<jogo>.png|jpg|bmp` ao lado do
arquivo). Com `--icones`, o ícone do `.mif` vai para `Named_Titles` como provisório — ícone de menu,
65×42 no máximo, não é capa.

O que ainda falta para "fullset publicado":

1. capas em resolução maior do que a da loja (170×220 é o que o pacote tem);
2. ~~RDB de Zeebo~~ — **feito**, e o "Scan Content" do RetroArch casa por hash;
3. envio das capas ao repositório de thumbnails do Libretro, com o nome do sistema igual ao do
   banco (`Mobile - Zeebo`), que é o diretório que o RetroArch procura — as capas **já estão** no
   formato e no nome certos, prontas para subir.

### RDB

**Resolvido.** O RDB de Zeebo não existia em lugar nenhum (135 RDBs instalados, nenhum de Zeebo) e
agora existe, gerado por `ferramentas/rdb.py` a partir do formato do próprio RetroArch
(`libretro-db/libretrodb.c` e `database_info.c`), conferido contra os RDBs oficiais:

```text
16 bytes     "RARCHDB\0" + uint64 **big-endian** com o offset dos metadados
registros    mapas msgpack em sequência, um por jogo
1 byte       0xC0, sentinela de fim
metadados    mapa msgpack { "count": N }
```

Campos de cada registro, iguais aos dos RDBs oficiais: `name`, `description`, `rom_name`, `size`,
`crc` (binário de 4 bytes, big-endian), `md5`, `sha1`.

Cada jogo entra **duas vezes**: com o CRC do `.zip` e com o CRC do arquivo que o No-Intro hasheia
dentro dele. O scanner consulta `crc:or(b"<arquivo de dentro>", b"<pacote>")`, então o acervo casa
pelo pacote e também pelo ROM interno — este último sobrevive a recompactar o zip.

Medido, com o core instalado:

```text
[Scanner]: Add "Double Dragon (Brazil) (Es,Pt)" to "Mobile - Zeebo.lpl"
[Scanner]: Add "Zeebo Sports Peteca (Brazil) (Es,Pt)" to "Mobile - Zeebo.lpl"
[Scanner]: Add "Caveman Ninja (Brazil) (Es,Pt)" to "Mobile - Zeebo.lpl"
[Scanner]: Add "Zenonia (Brazil) (Es,Pt)" to "Mobile - Zeebo.lpl"
```

As duas últimas linhas não estão no No-Intro nem têm ficha na loja: elas casam pelo registro de
**CRC do `.zip`**, que é gerado para todos os 62 pacotes. Ou seja, o banco nomeia o acervo inteiro,
não só a parte verificada pelo DAT.

Duas condições para o scan achar o banco, e as duas são fáceis de esquecer:

1. o banco precisa ser o `<nome do database>.rdb` dentro de `content_database_path` (na config do
   usuário: `~/.config/retroarch/database/rdb`), com o nome igual ao campo `database` do `.info`;
2. o scanner padrão exige que o conteúdo já case com um **core instalado**
   (`scan_without_core_match = "false"`). Sem o core na pasta de cores, ele nem entra na fase de
   banco e marca `??` em tudo. Com o core instalado funciona; se ainda assim não casar, ligue
   `scan_without_core_match`.

### Destinos explícitos, e o `.info` junto do core

Duas lições desta etapa, as duas medidas na máquina:

1. **o core e o `.info` andam juntos.** Um `.info` antigo, sem a linha `database`, instalado ao lado
   de um core novo, quebra o scan pelo menu: o RetroArch não associa banco nenhum àquele core, e
   todo conteúdo sai como `??`. Foi exatamente o que aconteceu aqui — o `.info` tinha sido copiado
   antes de a linha existir;
2. **nenhum artefato pode cair na pasta do frontend por acidente.** `--saida` era usado para
   playlist, catálogo e cache do DAT, e apontá-lo para a configuração do RetroArch largou
   `Mobile - Zeebo.lpl`, `catalogo.json` e `Mobile - Zeebo.dat` na raiz dela. Agora playlist,
   catálogo e DAT têm destino próprio (`--playlists`, `--catalogo` e o DAT ao lado do catálogo).

### Playlist

O RetroArch 1.20 grava playlist como **um documento JSON** com cabeçalho e `items` — não o formato
antigo de um JSON por linha. O `crc32` leva o sufixo `|crc` e o `db_name` leva `.lpl`. A ferramenta
copia o formato de quem lê, em vez de inventar.

## Desfecho de um jogo

No console, sair de um jogo devolve o controle à Z-Wheel, que é apenas outro applet instalado. No
Libretro isso tem duas formas possíveis, e elas não são equivalentes:

| Caminho | Como funciona | Estado |
|---|---|---|
| Frontend encerra o conteúdo | o core chama `RETRO_ENVIRONMENT_SHUTDOWN` (7) | **implementado** |
| Core carrega a Z-Wheel | o próprio core inicia outra `Session` com o `.mod` da Z-Wheel e segue apresentando | **implementado** |

O motor já sabe que o shell pediu outro applet: `Machine::pending_launch`, exposto por
`Session::take_launch_request()`, é o que a UI desktop usa para voltar à Z-Wheel. O que **não**
existe na ABI é um "carregue este outro conteúdo" do core para o frontend — nenhum comando faz o
frontend trocar de jogo a pedido do core. Logo, a volta à Z-Wheel só pode acontecer se o core a
carregar internamente.

### Ligação feita no core

O motor já tinha as peças; faltava o core usá-las.

| Peça | Como ficou |
|---|---|
| Botões | os quatro de face seguem o aparelho: **1 embaixo** (`B` do RetroPad, `X` do PlayStation, `B` do SNES) e a numeração sobe no sentido horário — 2 à esquerda (`Y`), 3 em cima (`X`), 4 à direita (`A`). Os rótulos de mapeamento apontam para o mesmo `id` que o core lê |
| Vídeo | **1x e proporção nativa, sempre**: `define_resolucao_interna(1)` e `define_proporcao(None)` na carga e na troca de sessão. Quadro fora de 640×480 é avisado uma vez no log, em vez de aparecer como imagem torta sem explicação |
| Teclado | `SET_KEYBOARD_CALLBACK` recebe as teclas e **enfileira**; `retro_run` entrega os `AVK`. Mapeados: setas, Enter, Backspace/Esc, dígitos, `*` e `#`. O Select do RetroPad vale como `AVK_CLR` |
| Biblioteca | `library::scan` na pasta do conteúdo, deduplicada por ClassID, alimenta `set_installed_applets` — sem isso a Z-Wheel abre vazia |
| Troca de sessão | o pedido de lançamento encerra a sessão atual e inicia a nova com o mesmo `StoragePaths`; o frontend nem percebe |
| Volta à Z-Wheel | ao terminar o jogo, se a sessão era a Z-Wheel ou foi aberta por ela, o core volta para ela em vez de pedir `SHUTDOWN` |
| Fonte do sistema | `fonte_do_sistema_em(cache, aparelho)` copia o `tectoy.ttf` do pacote para a raiz do aparelho do frontend. Se o acervo só tiver a Z-Wheel extraída, `instala_fonte_do_pacote` a pega de ao lado do módulo |

Medido, headless, com a Z-Wheel como conteúdo:

```text
Zeebx: 63 jogo(s) instalados a partir de /media/.../zeebo/ROMs
Zeebx: fonte do sistema em .../system/zeebx/aparelho/shared/fonts/tectoy.ttf
tela: 640x480, 398 cores distintas
```

Os 63 são 62 títulos mais a própria Z-Wheel; antes da deduplicação por ClassID eram 124, porque a
pasta de ROMs tem os `.zip` **e** uma cópia já extraída.

Comportamento atual:

1. o desfecho é relatado **uma vez**, com o relógio virtual (`parou em N ms virtuais`), seguido das
   últimas linhas do log do próprio jogo e do uso de heap;
2. se o shell pediu um applet e ele está na pasta de jogos, a sessão troca; se não está, o ClassID
   vai para o log dizendo que não foi encontrado;
3. a tela final fica à mostra **2 segundos**; então o core volta à Z-Wheel, se ela estiver
   disponível, ou pede `SHUTDOWN` para o frontend voltar ao menu dele;
4. o pedido de descarga é feito **fora** do mutex do estado, como todas as chamadas ao frontend.

O caminho do core-carrega-a-Z-Wheel exige resolver o ClassID para um pacote instalado — o core ainda
não tem varredura de biblioteca — e decidir a política de "próximo conteúdo" com o frontend. Fica
como feature, com o motor já pronto para iniciar uma sessão nova.

### Instalação e CI

- `ferramentas/instala_core.py` copia o `.so` **e** o `.info` **juntos**. Os dois andam em par: um
  `.info` velho ao lado de um core novo quebra o scan pela interface, sem dizer nada — já aconteceu
  aqui. O script avisa quando o `.info` de destino era diferente do que vai entrar;
- `.github/workflows/libretro.yml` compila os dois alvos — **x86_64** e **AArch64 em runner ARM64
  nativo**, porque cross-build de `unicorn`/`dynarmic` exigiria toolchain e sysroot que ninguém quer
  manter só para empacotar — e confere três coisas, nenhuma delas "compilou":
  1. os 25 símbolos `retro_*` estão exportados (`nm -D`);
  2. o `.so` não puxa `libX11`, Wayland, EGL, ALSA, udev nem GTK (`ldd`);
  3. o pacote publicado leva o `.info` com o campo `database`.

### Avisos ao jogador e leitura de botões

- `SET_MESSAGE` mostra avisos na tela do frontend ("sem `tectoy.ttf`", "o shell pediu uma classe que
  não está na pasta"), além de registrá-los no log — nem todo frontend exibe mensagem, e aviso que
  ninguém vê não serve;
- os botões são lidos por **máscara de bits** quando o frontend anuncia suporte
  (`GET_INPUT_BITMASKS`), o que reduz doze consultas por quadro a uma. **O RetroArch 1.20 devolve
  falso**, então na prática ele usa consulta individual; o caminho da máscara fica para frontends
  que o anunciem. O log diz qual dos dois está em uso, em vez de deixar isso invisível.

## O que falta para os ports da Data East: a interface `IFont`

Investigando o quadro branco do Double Dragon, a causa **de fundo** apareceu: ele pede três classes
que respondemos como desconhecidas.

```text
classes que o jogo pediu e não temos:
  0x0102f679   AEECLSID_FONT_STANDARD11
  0x01030852   AEECLSID_FONT_STANDARD15
  0x0102f681   AEECLSID_FONT_STANDARD36
```

São as **fontes de bitmap padrão do BREW**, e o `AEEFontsStandard.bid` do SDK confirma o que cada uma
é (ascent/descent documentados) e qual interface implementam:

```c
#define AEECLSID_FONT_STANDARD11  0x0102f679   /* IFont */
#define AEECLSID_FONT_STANDARD15  0x01030852   /* IFont */
#define AEECLSID_FONT_STANDARD36  0x0102f681   /* IFont */
```

**`IFont`, não `ITypeface`** — é essa a diferença que importa, e é por isso que o mapeamento não
podia ser só "apontar para a nossa fonte TrueType". O `AEEFont.h` lista os seis métodos, e um
emulador de referência já no disco (`projects/zeebo-emulator/.../zeemu/brew/BrewFont.cpp`) dá a
ordem da vtable e a tabela de métricas de todas as classes:

```c
AddRef(0)  Release(1)  QueryInterface(2)  DrawText(3)  GetInfo(4)  MeasureText(5)
```

A família inteira são 30 classes: `FONTSYS*` (6), `FONT_STANDARD*` (11), `FONT_BASIC*` (10),
`FONT_FIXED4X6`, `AEECLSID_FONT` e o `BITFONTFOUNDRY`. As assinaturas saem dos próprios exemplos do
SDK:

```c
IFont_GetInfo(pFont, &info, sizeof(info))
IFont_MeasureText(pFont, pszBuf, WSTRLEN(pszBuf), IFONT_MAXWIDTH, &nChars, &extent.width)
```

O que fazer, na ordem que o projeto adota — **medir antes de inventar**:

1. usar a sonda já existente (`--sonda=0x0102f679`) numa ROM que peça a classe, para registrar por
   qual slot o jogo chama e com que argumentos. O `DrawText` é o único com layout ainda não medido;
2. implementar `Interface::Font` com os seis slots, `GetInfo` escrevendo as quatro métricas e
   `MeasureText` devolvendo largura e altura da tabela;
3. mapear as 30 classes para essa interface, e o `DrawText` desenhando no destino corrente — o mesmo
   caminho que o `IDISPLAY_DrawText` já usa.

Sem isso, todo título que desenha com as fontes do sistema cai no mesmo lugar: o Double Dragon, o
Resident Evil 4 (`0x0102f681`) e os ports da Data East.

## Save states

Save states exigem snapshot pointer-free e versionado de:

- CPU Dynarmic;
- memória guest;
- heap e objetos BREW;
- callbacks, sinais, threads e timers;
- VFS relevante;
- áudio;
- rasterizador e recursos gráficos;
- extensões e estado de applet.

Quando implementado, `retro_serialize_size()` não pode crescer entre `retro_load_game()` e
`retro_unload_game()`. Validar:

```text
salva → executa N frames → restaura → executa N frames
```

Comparar relógio, framebuffer, memória e áudio conforme aplicável.

Save state não deve fingir ser rollback de arquivos persistentes: `saves/` e `aparelho/` ficam fora
do snapshot e mantêm a última escrita confirmada no host. Esta regra deve aparecer na documentação
do recurso quando ele existir. Qualquer estado que mantenha ponteiro, descritor de arquivo ou
recurso de host precisa ser reconstruído a partir de representação serializada, nunca copiado cru.

## Distribuição e builds

Alvos iniciais:

```text
x86_64-unknown-linux-gnu
aarch64-unknown-linux-gnu
```

Artefato distribuído:

```text
zeebx_libretro.so
```

O host atual só possui o target Rust x86_64. AArch64 requer target Rust, linker, C/C++20,
CMake, Ninja e sysroot coerentes. `unicorn-engine` compila C/QEMU; Dynarmic compila C++20/CMake;
Linux também liga `libatomic` conforme `build.rs` atual.

O build local de `cargo check --locked` chegou à build de Dynarmic, mas falhou por falta de espaço
em disco enquanto CMake/Ninja construía centenas de alvos — não por erro de fonte. CI e cross-build
precisam de espaço de disco adequado e cache de dependências.

Recomendação:

1. validar x86_64 localmente;
2. usar runner AArch64 nativo primeiro;
3. adicionar cross-build após toolchain/sysroot estar comprovado;
4. testar carregamento real em RetroArch, não apenas compilação/dlopen.

CI deve verificar o pacote separado:

```text
cargo build --locked --release -p zeebx-libretro --target x86_64-unknown-linux-gnu
cargo build --locked --release -p zeebx-libretro --target aarch64-unknown-linux-gnu
readelf -h
nm -D / checagem de símbolos retro_*
ldd
smoke test RetroArch
```

A release coleta somente `target/<triple>/release/libzeebx_libretro.so` e o
`frontends/libretro/zeebx_libretro.info`; não deve incluir binário desktop, settings da UI ou
artefatos de outros membros do workspace.

## Arquivo `.info`

Distribuir `zeebx_libretro.info` junto do `.so`, por exemplo:

```ini
display_name = "Zeebx"
authors = "ZeebxTeam"
supported_extensions = "mod|zip"
categories = "Emulator"
systemname = "Zeebo"
manufacturer = "TecToy"
licenses = "GPLv2"
firmware_count = 0
supports_no_game = "false"
```

Adicionar `7z` somente depois de suporte real.

## Validação executada

### Ferramenta

RetroArch 1.20.0 com perfil isolado em `/tmp/zeebx-ra` (nunca o perfil do usuário), drivers
`video = null`, `audio = null`, `input = null`, `--max-frames=N` e screenshot final:

```bash
XDG_CONFIG_HOME=/tmp/zeebx-ra retroarch -c /tmp/zeebx-ra/ra.cfg \
  -L /tmp/zeebx-ra/cores/zeebx_libretro.so --max-frames=900 \
  --max-frames-ss --max-frames-ss-path=saida.png "'ROM.zip'"
```

### O que o frontend confirmou

| Etapa | Resultado |
|---|---|
| Símbolos exportados | 25 funções `retro_*` |
| `ldd` do `.so` | só `libc`, `libm`, `libstdc++`, `libgcc` — sem X11, Wayland, EGL, ALSA, udev, GTK |
| `cargo tree -p zeebx-libretro` | sem eframe, cpal, gilrs, glutin, minifb, rfd, Discord |
| `retro_api_version` | 1 |
| `retro_get_system_av_info` | 640×480, aspecto 1,333, 60 Hz, 44 100 Hz |
| `SET_CONTROLLER_INFO` / `SET_INPUT_DESCRIPTORS` | aceitos |
| `SAVE_DIRECTORY` / `SYSTEM_DIRECTORY` | usados; perfil criado em `<save>/Zeebx/zeebx/` |
| `SET_PIXEL_FORMAT` | RGB565 aceito |
| Extração | `cache/<título>-<hash BLAKE3>/mod/<id>/<nome>.mod` |

### Por jogo

| Título | Quadros | Tempo relatado pelo frontend | Quadro entregue |
|---|---:|---:|---|
| Zeebo Sports Peteca | 900 | 14 s | 2 734 cores distintas |
| Crash Bandicoot Nitro Kart 3D | 900 | 14 s | 5 959 cores distintas |
| Double Dragon | 3 600 | 59 s | branco uniforme |

**O quadro branco do Double Dragon: resolvido, e eram três defeitos empilhados.**

1. `fonte_do_sistema()` procurava no cache **do desktop** em vez do cache que o frontend forneceu;
2. `fonte_do_console()` — a busca na hora de desenhar — também usava o caminho do desktop, então a
   fonte instalada em `<raiz do aparelho>/shared/fonts/tectoy.ttf` nunca era encontrada;
3. **e a ordem estava errada**: a fonte era instalada **depois** de a sessão existir. O motor lê a
   fonte quando a máquina é construída, então a sessão inteira ficava sem fonte nenhuma.

O relatório do próprio jogo já dizia o que faltava:

```text
texto que não soubemos desenhar:
  Application is finished
  Memory is insufficient.
  Please delete
  by pushing the button.
  some files.
```

A tela do Double Dragon é **branco com essa mensagem em preto**. Sem fonte, saía branco puro — que
parecia "não desenha nada" e era "desenha texto invisível". Medido, antes e depois:

```text
antes:  1 cor   (branco puro)
depois: 2 cores (branco + preto) — o texto aparece
```

O que a mensagem revela é outra coisa, e essa continua: o Double Dragon pede três classes que não
temos (`0x0102f679`, `0x0102f681`, `0x01030852`) e não acha `./udata/ddz.sav`, e por isso mostra
"Memory is insufficient". O emulador agora **mostra** o problema em vez de escondê-lo.

Peteca e Crash provam vídeo real pelo caminho Libretro. Double Dragon carrega, roda 59 segundos
virtuais sem erro e não quebra, mas entrega quadro branco.

**Pendência de investigação, não defeito da ABI.** A tabela de `docs/implementacao/11-compatibilidade.md`
foi medida com o caminho `zeebx run` (**Unicorn**, `src/main.rs::run_frames`), enquanto o core usa
`Session` (**Dynarmic**). Antes de acusar o core é preciso rodar a mesma ROM nos dois caminhos e
comparar; o próprio documento lista Double Dragon com "ponteiro recusado, texto sem fonte".

### O que os testes no Wayland acharam

Rodando o Crash Nitro Kart no RetroArch do usuário (Wayland + Vulkan, driver de vídeo real), o log
do core mostrou o defeito mais caro desta etapa:

```text
Zeebx: parou em 19640 ms virtuais: o jogo terminou
```

O jogo rodava 19,6 s e a sessão se encerrava sozinha. Causa: `Machine::is_idle()` decidia que a
máquina estava ociosa olhando timers, callbacks, threads e blits — mas **não** olhando
`pending_signals` nem o fato de o guest ter registrado interesse em entrada. O intro do Crash acaba
exatamente quando ele passa a esperar o jogador: sem timer armado e com o callback de entrada
registrado, o emulador concluía "acabou". Depois da correção o mesmo jogo passa de 27,7 s, e um
título que fica esperando botão continua rodando, como no console.

Segundo achado, ainda no Wayland: com o disco da máquina cheio, o RetroArch abriu o aviso de
gravação e o emulador travou. O core chamava `video_refresh` e `audio_sample_batch` segurando o
mutex do estado, e também chamava o callback de ambiente segurando o mutex dos ponteiros do
frontend. Agora os ponteiros são copiados e os cadeados soltos antes de qualquer chamada ao
frontend, e os buffers de quadro e áudio saem do estado antes de vídeo/áudio e voltam depois.

### Higiene dos testes

A suíte deixava lixo: os testes de SQL e de saves criavam `/tmp/zeebx-sql-*` e `/tmp/zeebx-saves-*`
e limpavam **no começo** — para o caso de a rodada anterior ter falhado — e nunca no fim. Uma sessão
com várias rodadas acumulou dez diretórios. Agora existe `src/scratch.rs::TempDir`, que apaga a
pasta quando o teste sai de escopo, passe ele ou não. Verificado: depois da suíte inteira, `/tmp`
não tem nenhum `zeebx-*`.

### Testes automatizados

```text
cargo test --lib --locked
→ 442 passaram, 0 falharam, 9 ignorados (com as features de desktop)

cargo test --lib --locked --no-default-features
→ 384 passaram, 0 falharam, 5 ignorados (só o motor, que é o que o core usa)

cargo test --lib --locked brew::vfs      → 14 passaram
cargo test --lib --locked loader::archive → 6 passaram
cargo test --lib --locked storage::      → 3 passaram
```

### Como repetir

```bash
CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=1 cargo build -p zeebx-libretro
cp target/debug/libzeebx_libretro.so  <pasta de cores>/zeebx_libretro.so
cp frontends/libretro/zeebx_libretro.info <pasta de cores>/
```

O `DEBUG=0` não é detalhe: com debuginfo a árvore passa de 5 GiB e, nesta máquina, o disco acabou
no meio da compilação mais de uma vez.

## Referências e padrões verificados

Esta seção registra fontes externas usadas para decisões de arquitetura. Não copiar código C/C++
sem revisar licença, segurança e adaptação a Rust.

### Documentação/API oficial

- [Libretro Input API](https://docs.libretro.com/development/input-api/) — semântica de RetroPad,
  teclado, mouse e pointer;
- [libretro.h canônico](https://github.com/libretro/libretro-common/blob/master/include/libretro.h)
  — ABI, `RETRO_DEVICE_SUBCLASS`, sensor, VFS, diretórios e lifecycle;
- [libretro-common archive 7z](https://github.com/libretro/libretro-common/blob/master/file/archive_file_7z.c)
  — prova de que a infraestrutura oficial trata 7z; não elimina os limites de segurança do Zeebx.

### Cores oficiais examinados

| Core | Referência | Prática aproveitada |
|---|---|---|
| Flycast | [`core/libretro/libretro.cpp`](https://github.com/libretro/flycast/blob/master/core/libretro/libretro.cpp) | subclasses de joystick, `SET_CONTROLLER_INFO`, callback de teclado, VFS solicitado cedo, refresh de dispositivos/descriptors depois de troca de porta |
| Dolphin | [`DolphinLibretro/Input.cpp`](https://github.com/libretro/dolphin/blob/master/Source/Core/DolphinLibretro/Input.cpp) | sensor por porta: habilitar, aceitar somente retorno de sucesso, transformar orientação, usar por frame e desabilitar no shutdown |
| Dolphin | [`Main.cpp`](https://github.com/libretro/dolphin/blob/master/Source/Core/DolphinLibretro/Main.cpp), [`Boot.cpp`](https://github.com/libretro/dolphin/blob/master/Source/Core/DolphinLibretro/Boot.cpp), [`VFile.cpp`](https://github.com/libretro/dolphin/blob/master/Source/Core/DolphinLibretro/Common/VFile.cpp) | `need_fullpath`+`block_extract`, layout de usuário, VFS negociado e modos de abertura/append/truncate explícitos |
| ScummVM | [`libretro-core.cpp`](https://github.com/libretro/scummvm/blob/master/backends/platform/libretro/src/libretro-core.cpp) | aceitar somente tipos/portas de input suportados, teclado por callback, VFS/authorized locations e save states declaradamente ausentes |
| DOSBox Pure | [`dosbox_pure_libretro.cpp`](https://github.com/schellingb/dosbox-pure/blob/main/dosbox_pure_libretro.cpp) | decode explícito de subclasses, teclado/mouse/pointer separados, save no diretório do frontend e serialização de tamanho fixo/validada |
| PPSSPP | [`libretro.cpp`](https://github.com/libretro/ppsspp/blob/master/libretro/libretro.cpp) | separação de system/save directory; o fallback dele para diretório do conteúdo **não** será copiado porque Zeebx não deve escrever ao lado da ROM |

Consequências incorporadas neste plano:

- VFS deixa de ser fase posterior: é requisito da arquitetura desde a primeira carga; suporte
  pleno a paths não-nativos depende também de adaptar SQLite e todas as E/S do motor;
- subtipo não é tipo de poll; Zeebx faz poll do dispositivo-base;
- sensor só vira fonte Boomerang se `set_sensor_state()` retornar sucesso;
- teclado usa callback e poll, com fila para não reentrar no guest;
- mouse/pointer só entram depois de haver aparelho BREW medido;
- 7z anunciado somente após parser seguro e testes de corpus.

## Matriz de build

Seis alvos por artefato — o core e o standalone —, três sistemas × duas arquiteturas, **cada um
no seu runner nativo**:

| | x86_64 | AArch64 |
|---|---|---|
| Linux | `ubuntu-24.04` | `ubuntu-24.04-arm` |
| Windows | `windows-latest` | `windows-11-arm` (experimental) |
| macOS | `macos-15-intel` | `macos-latest` (Apple Silicon) |

Nada aqui é cross-compilação, e é de propósito: o `unicorn` compila o QEMU em C e o `dynarmic`
compila C++20, exatamente a combinação em que cross-build exige toolchain C++ e sysroot. Existe
runner ARM64 nativo para os três sistemas, e ele sai mais barato que manter cross.

O core é publicado como `<alvo>/zeebx_libretro.{so,dll,dylib}` junto do `.info`, e o standalone
como o binário do alvo. Cada artefato passa por duas provas antes de subir:

1. **A biblioteca carrega e a ABI responde** (`ferramentas/verifica_core.py`): abre a biblioteca
   com o `dlopen`/`LoadLibrary` por trás do `ctypes` — como o frontend abre —, confere os 25
   símbolos, exige `retro_api_version` igual a 1 e exige que `retro_get_system_info` preencha
   `Zeebx`, `mod|zip` e os dois sinalizadores. Ler símbolos com `nm` não apanharia uma biblioteca
   que existe e **não carrega** por dependência que faltou no link; carregar, sim.
2. **Sem dependência de interface** (Linux): `ldd` recusa `libX11`, Wayland, EGL, ALSA, udev e GTK.
   Vídeo, áudio e entrada são do frontend.

### Windows ARM64 é experimental, e a causa é medida

O alvo existe na matriz, roda, e **não** derruba a rodada. Ele não fica verde por causa de uma
limitação de terceiros:

```text
FAILED: CMakeFiles/unicorn-common.dir/qemu/util/setjmp-wrapper-win32.asm.obj
  ml  -I...unicorn-engine-sys-2.1.5\msvc -I...
  CreateProcess failed: The system cannot find the file specified.
```

O QEMU monta esse `.asm` com o `ml` do MSVC, que **só existe para x86 e x64** — não há montador
MASM para ARM64. O `unicorn-engine` 2.1.5 é a última versão publicada, e o QEMU não tem o Windows
ARM64 como host. Quando isso mudar — versão nova do unicorn, ou o `unicorn` virar opcional nesta
plataforma, com o `dynarmic` sozinho —, basta tirar a marca de experimental do alvo.

## Estado dos itens, com a prova de cada um

| Item | Estado | Prova |
|---|---|---|
| Varredura das 62 ROMs | feito | 56 rodam (linha de base do doc: 50), 0 estados piorados — reconferido depois de todas as mudanças desta sessão, com os mesmos 6 fora |
| CI dos seis alvos do standalone | **feito** | `ci 77d5b02`: linux x86_64 e AArch64, macOS Intel e Apple Silicon, Windows x86_64 e ARM64 — **os seis verdes**, e o core também |
| `IFont` e o layout do `DrawText` | feito | métricas transcritas do `AEEFontsStandard.BID`, com teste que cobra as onze classes |
| Áudio e desempenho | feito | varredura mede pico/rms/contínuo/salto por jogo; Rolima 79% → 284%, 51 jogos mais rápidos; e **pelo caminho do core**: o Peggle entrega 229.080 amostras estéreo em 120 quadros |

### A tela que a varredura não olhava

A varredura contava **escritas** na tela, e um jogo que pinta 307.200 pixels de preto conta 307.200
escritas: registrava "roda" com a tela apagada. Agora o relatório conta **cores distintas e a
dominante**, e a linha entra na comparação com a linha de base — uma regressão que apaga a tela sem
quebrar a execução passa a aparecer no commit que a causou.

Na primeira varredura com a métrica, duas telas pretas entre os que "rodam", e dar mais tempo
virtual separou os dois casos possíveis:

| Jogo | 6 s | 20 s | Leitura |
|---|---:|---:|---|
| Zeebo F.C. Foot Camp | 1 cor | **2410 cores** | estava carregando: seis segundos é pouco |
| Zeebo F.C. Super League | 6 cores | **4133 cores** | idem |
| **Prey Evil** | 1 cor | **1 cor** | **não desenha nada, e roda a 3287%** |

Os dois primeiros são calibração: **seis segundos não bastam para quem carrega antes de desenhar**, e
o número de cores é o que denuncia isso sem ninguém olhar a tela. O terceiro é defeito: o jogo
executa, responde API e não põe um pixel na tela — e passava despercebido havia quantas sessões
ninguém sabe. **Investigado na mesma sessão, e a causa está identificada.** O Prey Evil não chama desenho
nenhum: o relatório lista onze métodos de GL e **nenhum** `DrawArrays`, `DrawElements`, `glClear`
ou `eglSwapBuffers`. Ele prepara matrizes e texturas, e para — e a tela preta é consequência:
não há o que mostrar.

O motivo está na lista de classes que ele pede e não temos, e **as seis são extensões de GL/EGL**:

| Classe | Nome | Header |
|---|---|---|
| `0x0103d8de` | `AEEIID_GLES10EXT` | `AEEGLES10Ext.h` |
| `0x0103d8eb` | `AEEIID_GLES11EXT` | `AEEGLES11Ext.h` |
| `0x0103def1` | `AEEIID_GLES11EXTPAK` | `AEEGLES11ExtPak.h` |
| `0x0103d8ef` | `AEEIID_EGLGETCOLORBUFFER` | `AEEEGLGetColorBuffer.h` |
| `0x0103d8f0` | `AEEIID_EGLGETPOWERLEVEL` | `AEEEGLGetPowerLevel.h` |
| `0x010426e3` | `AEEIID_EGLOESSWAPINTERVAL` | `AEEEGLOESSWAPInterval.h` |

O jogo particiona o caminho de desenho pelo que existe: sem as extensões, ele não desenha.

**E a correção é menor do que parece.** O motor **já implementa** essas funções —
`eglGetColorBufferQUALCOMM`, `SwapIntervalOES` e as outras da `EGL_QUALCOMM` vivem em
`machine/egl.rs` —, mas as expõe por `eglGetProcAddress`. O jogo as pede por `CreateInstance`, com
o **ClassID**, e recebe nulo. Falta registrar os seis ClassIDs como interfaces, com os slots na
ordem dos headers acima — o mesmo caminho que o `IFont` e o `IGraphics` já trilharam, e com a mesma
verificação: depois de registrar, **a contagem de cores do Prey Evil sai de uma**.

A tabela do `IGLES11Ext` já está lida, e são **15 slots**: os três de `IQueryInterface` (`AddRef`,
`Release`, `QueryInterface`) e, na ordem do `AEEGLES11Ext.h`, `CurrentPaletteMatrixOES`,
`LoadPaletteFromModelViewMatrixOES`, `MatrixIndexPointerOES`, `WeightPointerOES`, `DrawTexsOES`,
`DrawTexiOES`, `DrawTexxOES`, `DrawTexsvOES`, `DrawTexivOES`, `DrawTexxvOES`, `DrawTexfOES`,
`DrawTexfvOES`. Os `DrawTex*` são os que interessam a um jogo que monta o quadro em textura — que é
o caso do Prey Evil, com 16.746 `BindTexture` e nenhum desenho.

### Dois jogos mudos, e o que os calava

O relatório da varredura lista os sons que o decodificador recusou. Dois jogos apareciam ali, e a
medição de áudio — pico, rms, contínuo e salto entre amostras — mostrou **pico 0,000 nos dois**:
silêncio absoluto, não "sem som por enquanto".

Investigar começou por fazer a recusa **dizer o formato e os primeiros bytes**. Duas linhas
resolveram os dois casos:

```text
Turma da Mônica   recusado (formato desconhecido, 86 sons, 4f 67 67 53 ...)   ← "OggS"
Peggle            recusado (audio/mpeg, 7 sons, 49 44 33 03 00 00 00 00 ...)   ← "ID3"
```

- **Ogg/Vorbis**: o `symphonia` já sabia decodificar; faltava ligar a feature. 86 sons viraram
  música — pico 0,586 aos trinta segundos virtuais, contra 0,000 antes.
- **MP3 com etiqueta ID3 que mente**: o Peggle entrega etiqueta dizendo **zero byte** de tamanho e
  trinta mil de conteúdo. Quem pula pelo campo declarado procura o quadro no lugar errado e recusa
  o arquivo inteiro. Agora a busca é pela **sincronia do quadro** (`0xFF` e três bits altos), que é
  o que o formato garante — sete trilhas voltaram: pico 0,356 contra 0,000.

A lição vale para a próxima: uma recusa que não diz **o que** chegou custa uma investigação inteira
dentro do jogo. Dizendo o formato e os bytes, foram duas linhas de diagnóstico e vinte de correção.

No corpus inteiro, depois das duas correções: **zero sons recusados** (eram dois) e dezesseis jogos
com som medido nos seis segundos da varredura. O Turma da Mônica entra nessa conta só depois de
trinta segundos virtuais — a varredura curta o deixaria de fora, e por isso o número dele foi
medido à parte.

### Onde o tempo vai, medido

Com `ZEEBX_ROM_PERFIL=1`, a varredura grava o custo real por método de API. Nos dois jogos mais
pesados do acervo:

| Jogo | Tempo em chamadas de API | O que domina |
|---|---:|---|
| Crash Nitro Kart 3D | 462 ms | `eglSwapBuffers` 415 ms = **89,8%** |
| Zeebo Extreme Rolima | 958 ms | `memset` 520 ms, `eglSwapBuffers` 257 ms |

Os 415 ms do Crash são **3,5 ms por quadro apresentado** (118 quadros), e ali dentro está o
rasterizador software desenhando a cena — trocar o buffer é o mesmo que desenhar. Ainda assim o
jogo roda a **1039%** da velocidade do console: o rasterizador é o maior custo isolado e **não é
gargalo** para o que existe hoje. O que o render em hardware muda é a **fidelidade** (estado de GL,
texturas comprimidas, blend), não a velocidade deste acervo.

O `memset` do Rolima custava 731 ms porque o `helpers` **alocava um `Vec` do tamanho pedido em cada
chamada**. Com limpeza em blocos de um buffer de pilha: 520 ms, e o jogo de 247% para 267%.
| Render em hardware (`SET_HW_RENDER`) | **encaixado, provado até onde dá sem frontend** | rasterizador verificado contra o de software; o motor desenha no framebuffer do frontend; as features `gl`/`gpu` separadas (0 dependências de host, medido em CI nos 6 alvos); o core pede o contexto, monta o `glow::Context`, desenha no FBO e volta ao software se falhar. O desenho em si depende do RetroArch |

### O rasterizador da placa desenha o mesmo quadro, medido

Antes de ligar `SET_HW_RENDER` no core, era preciso saber se o caminho de GPU desenha **a mesma
imagem** que o de software — senão a troca seria sair de um caminho medido para um que ninguém
olhou. O teste `os_dois_rasterizadores_desenham_o_mesmo_quadro` roda o mesmo conteúdo pelo mesmo
tempo virtual nos dois e compara pixel a pixel, **sem janela** (o contexto fora de tela existe
justamente para isso: medir).

```bash
ZEEBX_TESTE_ROM="roms/jogo.zip" ZEEBX_TESTE_MS=3000   cargo test --features gpu os_dois_rasterizadores -- --nocapture
```

| Jogo | Pixels diferentes | Diferença média | Pior pixel |
|---|---:|---:|---:|
| Double Dragon | **0,00%** | 0,000 | 0 |
| Crash Nitro Kart 3D | **0,00%** | 0,000 | 0 |
| Zeebo Sports Peteca | 2,62% | 0,034 | 3 de 63 |

Dois jogos saem **idênticos**, e o terceiro difere só em arredondamento de meio nível — o que a
comparação cobra é que desenhem a mesma imagem, não que sejam bit a bit iguais. Com esta conta
feita, o que falta no item 5 é o encanamento: negociar o contexto com o frontend na
`RETRO_ENVIRONMENT_SET_HW_RENDER` e entregar o `glow::Context` que o motor já aceita.

Enquanto o encanamento não existe, dá para **ver** os dois rasterizadores lado a lado pela linha de
comando, sem interface — os mesmos dois comandos, mudando só a flag da placa:

```bash
cargo run --release -- sessao "roms/Crash.zip" --seconds=3 --dump=software.bmp
cargo run --release -- sessao "roms/Crash.zip" --seconds=3 --placa --dump=placa.bmp
```

Os dois `.bmp` são o mesmo instante virtual do mesmo jogo, um desenhado no processador e outro na
placa. É a conferência que qualquer pessoa faz sem escrever código, e foi ela que o teste acima
automatizou.

### O que já passou pelo caminho do core

O teste `a_abi_do_core_roda_uma_rom` exercita o core como o RetroArch o exercita — `retro_init`,
`retro_load_game`, quadros, `retro_unload_game` —, e conta o que chega ao frontend. Medido hoje:

| Conteúdo | Quadros | Amostras estéreo | Jogos ao lado |
|---|---:|---:|---:|
| Crash Nitro Kart 3D (`.zip`) | 360 | 267.668 | 63 |
| Double Dragon (`.zip`) | 360 | 268.729 | 63 |
| Double Dragon (`.7z`) | 300 | 224.669 | — |
| Peggle (`.zip`) | 300 | 229.080 | — |
| Z-Wheel (`.zip`) | 300 | 220.300 | 63 |

```bash
ZEEBX_CORE_ROM="roms/Crash.zip" ZEEBX_CORE_QUADROS=180 \
  cargo test -p zeebx-libretro -- --nocapture
```

Cada linha é uma coisa que deixou de ser suposição: o 3D entrega quadro e som pelo core, o `.7z`
abre pelo core, e a Z-Wheel vê os 63 jogos pelo core. O que **não** está nesta tabela é o desenho
na placa: para esse é preciso um frontend de verdade, e é o que falta no item 5.

### A lacuna de GL que o levantamento achou

O relatório da varredura tem uma seção **"GL atendido sem fazer nada"**: chamada que o rasterizador
aceita com sucesso e ignora. Nas 62 ROMs ela apontava **uma só** chamada, em seis jogos — o
`glPixelStorei`. Não é ruído: o GL alinha cada linha de textura num múltiplo do alinhamento pedido
(4 por padrão), e o que sobra é enchimento. Ignorando a chamada, uma textura cuja largura **em
bytes** não é múltipla de quatro chega com as linhas deslocadas — a imagem sai embaralhada em
diagonal, sem uma linha de aviso.

Implementado: o alinhamento vive no `Machine` (é propriedade da memória do guest, não do
rasterizador), a leitura dos texels compacta as linhas num único acesso, e `arredonda_para` tem
teste. Conferido depois: Double Dragon e Galaxy on Fire **deixam de aparecer na seção**, e continuam
rodando. É o tipo de defeito que o render em hardware resolveria de graça — e que aqui custou vinte
linhas, sem trocar de rasterizador.
| Save states | **falta** | o core **declara** que não tem (`size` 0, `serialize` falso), que é o critério de aceite |
| `.7z` | feito | `/tmp/dd.7z` → `estado: roda`; limites iguais aos do zip; e **pelo caminho do core**: 300 quadros entregues ao frontend |
| Ciclo da Z-Wheel no RetroArch | **parcial** | o motor é medido pela varredura; o laço do core que troca de sessão não tem teste automático |
| Capas e No-Intro | **falta** | depende de conta e de envio externo |

### O que a rodada de CI ensinou

Três defeitos que só apareceram quando os testes passaram a rodar até o fim, e que valem para a
próxima vez:

1. `cargo test` **nunca** tinha compilado o alvo de teste do binário: o `main.rs` importava
   `zeebx::varredura` sob `#[cfg(test)]`, e ali a biblioteca é compilada sem esse `cfg`. Rodar
   `cargo test --lib` escondia isso.
2. Dois testes de VFS comparavam a caixa exata do caminho, e o APFS do macOS **não distingue
   caixa**: o arquivo pedido já existe, e não há duas grafias para comparar.
3. `continue-on-error: ${{ matrix.experimental }}` faz o GitHub **rejeitar o arquivo de workflow
   inteiro** — o run aparece como falha **sem nenhum trabalho**. `actionlint` e um leitor de YAML
   não acusam: o diagnóstico é o run vazio.

## Publicação do core

O core é publicado junto com os instaladores, na mesma tag. O trabalho `core` do `release.yml`
compila e confere os seis alvos (a mesma prova de ABI do `libretro.yml`) e empacota **um `.zip` por
alvo**, com a biblioteca e o `.info` juntos — o RetroArch quer os dois com o mesmo nome na pasta de
cores, e seis arquivos de mesmo nome não cabem soltos numa release.

```text
zeebx_libretro-linux-x86_64.zip      zeebx_libretro-windows-x86_64.zip
zeebx_libretro-linux-aarch64.zip     zeebx_libretro-windows-aarch64.zip
zeebx_libretro-macos-x86_64.zip      zeebx_libretro-macos-arm64.zip
```

Cada sistema empacota com a ferramenta que tem: `zip` no Linux e no macOS, `Compress-Archive` no
Windows — o `zip` **não existe** no runner do Windows, e um `shell: bash` com `zip` falharia num
terço dos alvos no dia da tag.

## O que falta, com o mapa de cada item

### Item 5 — render em hardware: só o encanamento

**O que já existe:** o rasterizador da placa, verificado contra o de software (idêntico em Double
Dragon e Crash; 2,6% de arredondamento no Peteca); o contexto fora de tela para medi-lo sem janela;
`Session::start_with(..., placa, Some(Arc<glow::Context>), ...)`, que já aceita o contexto do
frontend.

**O que falta, em ordem:**

0. ~~Separar o backend de placa da criação de contexto.~~ **Feito**: o motor tem duas features —
   `gl` (o desenho, que **só** precisa do `glow`, sem nada de host) e `gpu` (abrir contexto nosso,
   que traz o `glutin` e linka EGL/GLX). Medido: `cargo tree --features gl` tem **0** glutin e o
   `glow` presente; `--features gpu` tem 3. Era isto que faltava para o core poder usar a placa: um
   core não pode linkar biblioteca de interface do host, e é isso que a checagem de `ldd` do
   `libretro.yml` cobra.
1. ~~Ligar a feature `gl`, montar o contexto e negociar o `SET_HW_RENDER`.~~ **Escrito e
   compilando**, com a válvula de segurança: se o frontend aceitar e não cumprir, ou se a sessão na
   placa falhar, o core **volta ao software** e diz por quê no log. `troca_para` também nasce na
   placa, para a Z-Wheel abrindo um jogo não devolver o desenho ao processador.
2. ~~Fazer o motor desenhar no framebuffer do frontend.~~ **Feito e verificado**: o motor tem
   `desenha_no_fbo`, o teste `o_motor_desenha_no_framebuffer_do_frontend` cria um framebuffer
   próprio, manda o motor desenhar nele e lê os pixels **dele** — nos dois jogos medidos, **0 de
   307.200 pixels** ficaram com a cor de nascença, ou seja, o desenho foi todo para o framebuffer
   de fora, e não para o interno.
3. Trocar o `retro_video_refresh` de quadro por `RETRO_HW_FRAME_BUFFER_VALID` — sem isso o
   RetroArch recebe pixels que não são os do FBO.

O lado do motor já está pronto para receber: `Session::desenha_no_fbo(Option<u32>)` é público, e
`Some(0)` significa o framebuffer padrão do frontend. O que sobra no core é ligar os fios.

**Uma armadilha de ordem, para não gastar uma sessão descobrindo:** o `libretro` só entrega um
contexto de GL **válido** depois de chamar o `context_reset` do struct que o core preencheu. Quem
chama `SET_HW_RENDER` é o core (dentro do `retro_load_game`, antes de criar a sessão), e quem avisa
"o contexto está pronto" é o frontend, **depois**. Ou seja: a sessão de placa não pode ser criada
junto com a negociação — ela precisa esperar o `context_reset`, ou o primeiro `retro_run`. As duas
saídas possíveis são criar a sessão dentro do `context_reset`, ou guardar o pedido e criá-la no
primeiro `retro_run`, quando o contexto já está corrente. E `get_current_framebuffer` é consultado
**por quadro**, no `retro_run`, porque o framebuffer pode mudar.

**Como verificar:** o teste `os_dois_rasterizadores_desenham_o_mesmo_quadro` continua valendo como
régua; o que muda é por onde o quadro sai.

### Item 6 — save states: por que ainda não

O critério de aceite do plano é "ausência de save state é declarada corretamente, **sem estado
parcial**", e é o que o core faz (`retro_serialize_size` devolve 0, `retro_serialize` é falso). Um
estado parcial seria pior que nenhum: a memória do guest e os registradores voltariam, e a mesa de
objetos do emulador não — o jogo retomaria chamando APIs com identificadores que não existem mais.

**O que um estado completo exigiria, contado:** o `Machine` tem **183 campos**. Classificados por
nome, são:

| grupo | campos | o que fazer |
|---|---:|---|
| instrumentos (`bad_pointers`, `calls`, `api_time`, `assumptions`, `debug_*`, contadores de recusa) | 13 | **nada** — não são estado |
| tabelas de objetos (`bitmaps`, `canvases`, `databases`, `decoders`, `collections`, `egl_surfaces`, …) | 18 | descrever cada objeto vivo e recriá-lo na carga |
| agendamento (`*_pendente`, `current_thread`, `delivered`) | 13 | serializar a fila |
| escalares e contadores (`clock_us`, `epoch_seconds`, séries) | 10 | um `u64` cada |
| o resto (estado de desenho, EGL, configuração em vigor, listas de diagnóstico) | ~129 | separar o que o jogo vê do que é do emulador |

**O critério que faz o escopo encolher: instrumento não é estado.** Tudo o que existe para o
relatório — ponteiros recusados, contagem de chamadas, tempo por método, hipóteses — fica de fora
do save state, senão carregar um estado carregaria também o histórico de depuração de outra sessão,
e a linha de base da varredura passaria a depender de quando o jogo foi salvo.

**E o critério que faz o escopo crescer:** as 18 tabelas de objetos precisam de uma **descrição**
que baste para recriar cada objeto — arquivo aberto e deslocamento, superfície e formato, fluxo de
mídia e posição, consulta SQL e cursores. É aí que está o trabalho, e é aí que um estado parcial
mentiria.

### Item 8 — ciclo da Z-Wheel: o que já está medido

A varredura **não** consegue exercitá-lo: doze segundos com e sem manche diferem em mil instruções
de 133 milhões, e a roda não desenha nada naquele caminho (zero quadros, zero texto). A conclusão
prática é que **só o RetroArch responde** — não vale gastar outra sessão tentando por aqui.

A **precondição**, essa sim, está medida pelo caminho do core: ao abrir a Z-Wheel, o core acha
**63 jogos** ao lado do conteúdo (o levantamento por ClassID, com deduplicação entre o `.zip` e a
cópia extraída). Isso separa dois sintomas que se parecem: **"a roda abre vazia" não é falha de
descoberta** — ela soube de todos os 63. O que falta verificar é o desenho e a escolha, e para isso
é preciso controle na mão.

### Item 9 — capas e No-Intro: depende de conta

Local está feito: 58 capas oficiais do Z-Wheel, playlist de 62 entradas (nenhuma sem arquivo),
banco No-Intro, catálogo em JSON. O que falta é publicar no repositório de thumbnails e enviar os
cinco títulos fora do No-Intro — os dois precisam de conta e de conferência humana.

**O formato estava errado para o destino, e isso foi corrigido.** Os repositórios de thumbnails do
RetroArch aceitam **só PNG**, e as capas que a Z-Wheel entrega são JPEG — os cinquenta e oito
arquivos seriam recusados no dia do envio. O catálogo ganhou `--png`, que converte na hora de
gravar (com o Pillow; sem ele, grava o original e avisa), e a geração conferida:

```bash
python3 ferramentas/catalogo.py --roms ROMS --saida saida --png --zwheel "Z-Wheel.zip"
# capas oficiais: 58 de 62  ·  png: 58  ·  jpg: 0
# Action Hero 3D ….png: PNG image data, 170 x 220, 8-bit/color RGB
```

**E os títulos fora do No-Intro saem com os hashes prontos.** O que a proposta pede é nome,
tamanho, CRC32, MD5 e SHA1 do arquivo que o DAT hasheia — e para um título que não está no DAT
esse arquivo é decisão de quem envia. A ferramenta grava `<saída>/fora-do-dat.txt` com **todos** os
arquivos de `mod/<id>/` e os três hashes de cada um, e diz no cabeçalho que a escolha é humana:
adivinhar aqui devolveria a proposta recusada, e o trabalho seria o dobro.

```text
## Bad Dudes vs. DragonNinja (Brazil) (Es,Pt)
pacote: Bad Dudes vs. DragonNinja (Brazil) (Es,Pt).zip  (1369145 bytes)
crc32 do pacote: 56020280   sha1: 2E E9 …
  mod/279888/baddudes.eng
    tamanho: 1456  crc32: AD9831B6  md5: 82A42D24…  sha1: E4DCEE01…
```

E o mesmo conteúdo sai em **formato DAT**, que é o que a proposta pede de verdade — cinco blocos
`game (`, com o nome do arquivo na convenção do banco (a pasta do módulo e o arquivo, sem barra):

```text
game (
	name "Bad Dudes vs. DragonNinja (Brazil) (Es,Pt)"
	region "Brazil"
	rom ( name "mod279888baddudes.mod" size 3034964 crc DE1F72C0 md5 3567F3EE… sha1 BB5781BE… )
	… (todos os arquivos do módulo)
)
```

Um `game` com vários `rom` é válido no formato, e é o certo aqui: **qual dos arquivos é o dump é
decisão de quem mantém o banco**. A proposta leva todos, com os hashes, em vez de apostar num — e
aposta errada, nesse caso, custa uma rodada inteira de ida e volta.

## Ordem de implementação

1. Inventariar toda E/S de core e definir `StorageFs`/`GuestFile`; decidir SQLite VFS ou staging
   transacional antes de prometer caminhos não-nativos.
2. Criar `StoragePaths`, `ContentIdentity` e `ContentLayout` sobre essa abstração.
3. Mover configuração e leitura de `.mif` para módulos neutros.
4. Refatorar archive, machine, SQL, widgets, fontes, mídia, shell e VFS para receber storage
   explícito; manter `std::fs` somente em CLI/UI/testes desktop.
5. Implementar VFS com base read-only, overlay gravável, tombstones e device compartilhado.
6. Extrair driver de frame virtual determinístico de `Session`.
7. Criar `src/lib.rs`, features e motor sem dependências desktop; converter raiz em workspace.
8. Criar `frontends/libretro/` como pacote `cdylib`, vendorizar `libretro.h`, gerar bindings e
   implementar todos os exports base, incluindo VFS v3.
9. Implementar `.mod`/ZIP, RGB565, áudio PCM16, Dragon/Z-Pad e RetroPad.
10. Adicionar `SET_CONTROLLER_INFO`, teclado USB com fila AVK e Boomerang estático.
11. Integrar sensor Libretro e calibração do Boomerang; validar fallback analógico depois.
12. Adicionar testes determinísticos, VFS, COW, ZIP malicioso e persistência cruzada.
13. Integrar e testar RetroArch x86_64.
14. Adicionar AArch64 nativo/CI.
15. Implementar 7z com corpus real e limites de segurança.
16. Só depois: Core Options, suporte SAF/Android completo, mouse guest medido, HW render e save
    states.

## Critérios de aceite do MVP

- `frontends/libretro/` é pacote independente e não traz UI desktop como dependência;
- core x86_64 carrega em RetroArch;
- `.mod` e `.zip` iniciam applet e exibem framebuffer;
- Dragon, Z-Pad, Boomerang e teclado USB aparecem ao guest conforme aparelho selecionado;
- input RetroPad chega ao Z-Pad;
- teclado Libretro entrega aperto e soltura AVK sem executar guest dentro do callback;
- Boomerang recebe amostra de acelerômetro do frontend, ou informa claramente fallback/ausência;
- áudio estéreo chega ao frontend;
- saves relativos sobrevivem a unload/reload;
- `fs:/` é comum entre títulos;
- limpar `cache/` não apaga saves nem `aparelho`;
- ROM e cache de conteúdo não são modificados por execução;
- ZIP malicioso não escreve fora de `zeebx/`;
- conteúdo, save e cache passam por `StorageFs`/VFS; Linux nativo é validado no MVP;
- caminhos não-POSIX (SAF/Android) só são anunciados após a solução SQLite/VFS e testes reais;
- `GET_SAVE_DIRECTORY` ausente/não gravável falha sem criar save temporário enganoso;
- concorrência no mesmo `aparelho/` é recusada no backend nativo quando lock existe, e documentada
  como não suportada onde VFS não oferece exclusão mútua;
- Z-Wheel pode receber ao menos `AVK_CLR`/`AVK_SELECT` por rota de teclado;
- rede do guest fica desligada sem escolha explícita;
- todos os exports base existem;
- ausência de save state é declarada corretamente, sem estado parcial;
- build e carregamento AArch64 são validados antes de release.
