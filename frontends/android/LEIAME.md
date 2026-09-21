# O Zeebx no Android

O núcleo é o mesmo. O que muda é quem o gira.

A emulação — o BREW, a CPU, o rasterizador, o carregador — virou a biblioteca `zeebx`, e três
coisas a consomem: o binário de desktop (`src/main.rs`, com a janela do egui e a linha de
comando), os testes, e o `.so` deste diretório. Nenhuma linha de emulação sabe em qual dos três
está rodando.

Este diretório é `frontends/android/` porque ele é **um** frontend, não *o* frontend do Android:
quando houver outro alvo — um console, um navegador —, ele entra ao lado, e o núcleo continua
onde está.

## O corte

O que depende de uma janela de desktop está atrás de `cfg(not(target_os = "android"))`:

| Fica em toda plataforma | Só no desktop |
|---|---|
| `audio`, `brew`, `cpu`, `input`, `loader`, `machine`, `video`, `session` | `ui::app` (a janela do egui), `ui::window`, `ui::atualizacao`, `ui::discord` |
| `ui::settings`, `ui::library`, `ui::acervo`, `ui::saves`, `ui::i18n`, `ui::gpu` | `glutin`, `gilrs`, `minifb`, `rfd`, `ureq`, `discord-rich-presence` |

A segunda linha da primeira coluna é o ponto: a biblioteca de jogos, os ajustes e os saves são
lidos **pelo núcleo** — o `machine`, o `loader` e a `session` falam com eles —, então não são
interface, apesar de morarem em `src/ui/`.

## O frontend

Três arquivos, e nenhuma linha de Java ou Kotlin: a `NativeActivity` do sistema carrega a
`libzeebx_android.so` e chama o `android_main` dela.

| Arquivo | O que é |
|---|---|
| `src/lib.rs` | o laço de eventos, as três telas (biblioteca, seletor de pasta, jogo) e o JNI da permissão |
| `src/tela.rs` | a `ANativeWindow` virando superfície EGL, e o `egui_glow` desenhando nela |
| `src/entrada.rs` | o evento do Android virando botão do Zeebo e ponteiro do egui |

**Não há eframe aqui, e é de propósito.** Pelo winit — que é o que o eframe gira — o controle se
perde duas vezes: os códigos de tecla de um `Gamepad` viram `Key::Unidentified`, que o egui
descarta, e os eixos do manche são `MotionEvent`, que o winit só lê quando são toque. Interceptar
em Java também não resolve: a `NativeActivity` entrega a entrada pela fila nativa e **não passa
pela hierarquia de views**, então um `dispatchKeyEvent` nosso nunca seria chamado — foi medido.
Girando a `android-activity` direto, chega o que o Android mandou: `Keycode::ButtonA`, `Axis::X`,
os gatilhos e o "voltar".

O mapa de botões é o mesmo que o desktop dá a um controle moderno: sul no `b1`, leste no `b2`,
oeste no `b3`, norte no `b4`, os gatilhos superiores no `zl`/`zr`, o Start no `back` — o controle
do Zeebo não tem Start, e quem ocupa o lugar dele é o HOME.

### A pasta de ROMs

Ela é escolhida no aplicativo e guardada nos mesmos ajustes do desktop. O padrão é
`/sdcard/zeebo/roms`, e lê-lo exige o "acesso a todos os arquivos", que o próprio aplicativo pede.
Quem prefere não conceder nada usa o diretório privado, que sempre funciona:

```
adb push jogo.mod /sdcard/Android/data/io.github.zeebxteam.zeebx/files/roms/
```

O seletor de pastas é nosso, sobre o `std::fs`, e não o do sistema: o `ACTION_OPEN_DOCUMENT_TREE`
devolve um `content://`, e o carregador do núcleo abre caminho de arquivo.

## Compilar

```
./frontends/android/compilar.sh          # só o .so
./frontends/android/compilar.sh --apk    # o .so e a APK
```

O script espera a toolchain no `$HOME`, sem `sudo`:

| O quê | Onde | De onde |
|---|---|---|
| rustup + alvo `aarch64-linux-android` | `~/.cargo` | `https://sh.rustup.rs` |
| JDK 17 | `~/Android/jdk` | Adoptium |
| SDK (platform 35, build-tools 35) e NDK r27.3 | `~/Android/sdk` | `sdkmanager` |
| Gradle 8.11 | `~/Android/gradle` | `services.gradle.org` |
| `cargo-ndk` | `~/.cargo/bin` | `cargo install cargo-ndk` |

Os caminhos são ajustáveis por ambiente: `ANDROID_SDK_HOME`, `ANDROID_NDK_VERSAO`, `JAVA_HOME`,
`GRADLE`, `ZEEBX_ANDROID_ABI`.

## Duas pedras no caminho, e por que elas existem

**O CMake não achava o NDK.** O `unicorn` e o `dynarmic` compilam C e C++ por CMake, e o
`cmake-rs` monta a linha de comando sozinho: ele põe `CMAKE_SYSTEM_NAME=Android` e
`--target=aarch64-linux-android35` nas flags, mas não diz a ABI ao toolchain do NDK. Sem ela o
NDK assume `armeabi-v7a` e acrescenta `-march=armv7-a`, que o clang recusa junto de um alvo
aarch64 — e o build morre no teste do compilador, antes de qualquer código nosso. O
`ndk-toolchain.cmake` daqui fixa a ABI e a API antes de ler o toolchain do NDK.

**`-lpthread` não existe.** No bionic a pthread e a rt vivem dentro da libc; não há
`libpthread.so` nem `librt.so`. O `unicorn-engine-sys` pede as duas por nome, sem olhar o
sistema. O `compilar.sh` gera arquivos vazios com esses nomes em `target/android-libs-vazias` e
os põe no caminho do ligador: ele acha o nome, não acha símbolo nenhum, e segue.

## O que ainda não está aqui

- **A placa.** A sessão abre com `placa: false`, no rasterizador de software. O contexto de GL
  agora existe — é o do `tela.rs` —, então ligá-la é passá-lo à sessão; o passo seguinte, depois
  de medir.
- **Som.** O `cpal` tem backend de AAudio e já compila, mas ninguém o liga ainda.
- **Controle remapeável.** O mapa de botões é fixo no `entrada.rs`. O `input::bindings`, que é
  quem o desktop usa para deixar o usuário remapear, ainda não está ligado aqui.
- **Sensores e Wii Remote.** Continuam atrás de `cfg(target_os = "linux")`.
