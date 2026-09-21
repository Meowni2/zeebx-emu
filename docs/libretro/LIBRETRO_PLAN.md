# Plano Libretro — Zeebx

> Estado: planejamento técnico. Nenhum core Libretro foi implementado ainda.
>
> Branch: `feat/libretro-core`, baseada em `upstream/development` no commit `6c74975` (`v0.2.1`).

## Objetivo

Entregar o Zeebx como um core Libretro para Linux **x86_64** e **AArch64**, inicialmente para
RetroArch. O core recebe um jogo Zeebo (`.mod`, `.zip`; `.7z` após suporte real), executa o
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
| Áudio | estéreo PCM16, 44 100 Hz, 735 frames por `retro_run` a 60 Hz |
| Entrada | até duas portas RetroPad → Z-Pad |
| Conteúdo inicial | `.mod` e `.zip` |
| `.7z` | somente após decoder embutido e testes com arquivos reais |
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
valid_extensions  = "mod|zip"             # "|7z" somente depois do suporte real
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
44 100 Hz, estéreo, 60 Hz de vídeo = 735 frames por retro_run
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

| Camada | Persistente | Compartilhada | Conteúdo |
|---|---:|---:|---|
| `cache/` | opcional | não | ROM extraída, `.mod`, `.mif`, assets |
| `saves/<content-id>/` | sim | não | arquivos relativos gravados pelo jogo |
| `aparelho/` | sim | sim | `fs:/`, Z-Wheel, dados globais |

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

O padrão deve ser cache persistente em `<save-dir>/zeebx/cache/`. Isto evita descompressão em
todo boot e funciona em ambientes sem `/tmp` estável.

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

## Desfecho de um jogo

No console, sair de um jogo devolve o controle à Z-Wheel, que é apenas outro applet instalado. No
Libretro isso tem duas formas possíveis, e elas não são equivalentes:

| Caminho | Como funciona | Estado |
|---|---|---|
| Frontend encerra o conteúdo | o core chama `RETRO_ENVIRONMENT_SHUTDOWN` (7) | **implementado** |
| Core carrega a Z-Wheel | o próprio core inicia outra `Session` com o `.mod` da Z-Wheel e segue apresentando | planejado |

O motor já sabe que o shell pediu outro applet: `Machine::pending_launch`, exposto por
`Session::take_launch_request()`, é o que a UI desktop usa para voltar à Z-Wheel. O que **não**
existe na ABI é um "carregue este outro conteúdo" do core para o frontend — nenhum comando faz o
frontend trocar de jogo a pedido do core. Logo, a volta à Z-Wheel só pode acontecer se o core a
carregar internamente.

Comportamento atual:

1. o desfecho é relatado **uma vez**, com o relógio virtual (`parou em N ms virtuais`), seguido das
   últimas linhas do log do próprio jogo e do uso de heap;
2. se o shell pediu um applet, o ClassID vai para o log, com o aviso de que a troca de conteúdo
   dentro do core ainda não existe;
3. a tela final fica à mostra **2 segundos** e então o core pede `SHUTDOWN`, e o frontend volta ao
   menu dele. Sem o pedido, o RetroArch ficaria para sempre no último quadro de um jogo terminado;
4. o pedido de descarga é feito **fora** do mutex do estado, como todas as chamadas ao frontend.

O caminho do core-carrega-a-Z-Wheel exige resolver o ClassID para um pacote instalado — o core ainda
não tem varredura de biblioteca — e decidir a política de "próximo conteúdo" com o frontend. Fica
como feature, com o motor já pronto para iniciar uma sessão nova.

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

### Testes automatizados

```text
cargo test --lib --locked
→ 439 passaram, 0 falharam, 9 ignorados

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
