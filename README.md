# Zeebx 

Emulador do Zeebo, o console que a TecToy lançou em 2009 no Brasil e no México.

O Zeebo era digital-only: os jogos vinham da loja da TecToy, que saiu do ar. Sobraram cerca de
60 títulos que só rodam em quem ainda tem o aparelho ou por meio de modding no console. 
Este projeto existe para ajudar a preservar essas pérolas que fizeram parte da nossa história.

Em desenvolvimento. Hoje 50 dos 62 títulos de teste passam do carregamento e desenham.

## Links úteis

[Servidor Discord: https://discord.gg/D96HjsKTPa](https://discord.gg/D96HjsKTPa)

[GitHub: https://github.com/ZeebxTeam](https://github.com/ZeebxTeam)

## Como funciona

Emular Zeebo não é emular um console: é reimplementar o Qualcomm BREW 4.0.2. O jogo é um binário
ARM que nunca toca hardware — ele chama interfaces do sistema por tabelas de ponteiros. Então o
caminho é executar o código ARM num núcleo emulado e atender cada chamada de API no host.

As vtables que entregamos ao jogo apontam para endereços que **não existem** no mapa de memória.
Quando o jogo chama um método, o núcleo aborta a busca de instrução e o endereço nos diz qual
interface e qual método foram pedidos. Não há stub, nem código de cola.

O desenho completo está em [ARCHITECTURE.md](ARCHITECTURE.md).

## Compatibilidade

Poucas ROMs ainda rodam sem problemas, diversos jogos podem apresentar travamentos antes da inicialização ou durante a execução.

O estado de cada título, com os endereços de cada parada, está em
[docs/implementacao/11-compatibilidade.md](docs/implementacao/11-compatibilidade.md).

## Compilando

Rust 1.88 ou mais novo.

```bash
cargo build --release
```

### O que mais precisa estar instalado

O `cargo` sozinho não basta: duas dependências compilam **código nativo na hora**, e é aí que o
build falha em máquina limpa.

| Dependência | O que ela constrói | O que ela exige |
|---|---|---|
| `unicorn-engine` | o QEMU inteiro, em C | compilador C, `make`, `pkg-config`, Python 3, `glib-2.0` e **`libclang`** (o `bindgen` carrega a biblioteca para gerar os vínculos) |
| `dynarmic` | um JIT ARM em C++20, com CMake | `cmake`, `ninja` e compilador **C++20** |

Debian, Ubuntu e derivados:

```bash
sudo apt install build-essential cmake ninja-build pkg-config python3 \
    libclang-dev libglib2.0-dev \
    libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev
```

Arch:

```bash
sudo pacman -S --needed base-devel cmake ninja pkgconf python clang glib2 \
    alsa-lib systemd-libs wayland libxkbcommon
```

Fedora:

```bash
sudo dnf install gcc-c++ cmake ninja-build pkgconf-pkg-config python3 clang-devel \
    glib2-devel alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel
```

As cinco últimas linhas de cada lista são da **interface**: áudio (ALSA), controles (udev), janela
(Wayland, X11 e xkbcommon). Quem só quer o **core Libretro** não precisa delas — o motor compila sem
nenhuma dessas bibliotecas:

```bash
cargo build --release -p zeebx-libretro
```

Para conferir a máquina antes de tentar (e saber **o que** falta, em vez de ler "failed to run
custom build command" sem causa):

```bash
python3 ferramentas/prepara_build.py
```

### Quando o build falha

`failed to run custom build command for dynarmic` ou `for unicorn-engine-sys` é só o topo da
mensagem: a causa está no bloco `--- stderr` logo acima dela. As quatro que aparecem:

| Sintoma | Causa |
|---|---|
| `Unable to find libclang` | falta a biblioteca do `libclang` (pacote acima) |
| `ninja: command not found` / erro de gerador do CMake | falta `ninja` — o `dynarmic` pede `Ninja` explicitamente |
| `GLIB_2.0 not found` ou erro de `pkg-config` | falta `glib2` de desenvolvimento |
| `No space left on device`, no meio de centenas de alvos do CMake | o `dynarmic` sozinho passa de 5 GiB com debuginfo. Use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=1` e libere espaço |

### Instaladores e releases

Os instaladores saem do [cargo-packager](https://github.com/crabnebula-dev/cargo-packager), com a
configuração em `[package.metadata.packager]` no `Cargo.toml`:

```bash
cargo install cargo-packager --locked
cargo packager --release --formats deb,appimage   # Linux
cargo packager --release --formats nsis           # Windows
cargo packager --release --formats dmg            # macOS
```

Os arquivos ficam em `target/pacotes/`. No Arch, o AppImage precisa de `NO_STRIP=1`: o `strip` do
linuxdeploy não reconhece as bibliotecas do sistema.

Uma tag de versão (`v0.1.0` ou `0.1.0`) enviada ao GitHub dispara o
[`release.yml`](.github/workflows/release.yml), que gera os quatro pacotes — Linux, Windows, macOS
Apple Silicon e macOS Intel — e monta a release como rascunho, com o título igual à tag. O
emulador procura versões novas nessas releases ao abrir.

## Usando

Sem argumentos, abre a interface. Pela linha de comando:

```bash
cargo run --release -- run "roms/Quake.zip" --window
```

Pacotes `.zip` e `.7z` são extraídos para um cache e o `.mod` certo é escolhido sozinho — o formato
é reconhecido pela assinatura do arquivo, então um `.7z` renomeado para `.zip` também abre. O que
não for extraído com segurança é recusado antes de escrever qualquer coisa: caminho com `..`,
link simbólico, entradas demais ou tamanho além do teto. `--seconds=N` define quantos segundos de
tempo virtual emular quando não há janela; com janela, roda até você fechar.

Os controles no teclado:

| Tecla | Controle do Zeebo |
|---|---|
| Setas | direcional |
| Z, X ou Espaço, C, V | botões 1, 2, 3 e 4 |
| Q, W | ZL e ZR |
| F, G | analógico esquerdo, direito |
| H, Backspace, Enter | HOME |

O `run` informa onde o jogo parou, o que ele pediu e não temos, e o log que os próprios
desenvolvedores deixaram no binário — por `DBGPRINTF` e por semihosting do ARM. Esse relatório é
o backlog do projeto. As opções de depuração estão em [ARCHITECTURE.md](ARCHITECTURE.md).

## Integração contínua

Cada push nas ramas de desenvolvimento compila e testa o emulador em cinco plataformas — Linux
x86_64 e AArch64, Windows x86_64 e macOS Intel e Apple Silicon. Em pull request roda só o Linux
x86_64, que é onde o retorno precisa ser rápido.

As dependências nativas do build estão na seção acima, e `python3 ferramentas/prepara_build.py`
confere quais faltam na sua máquina.

## Plataformas

Linux, Windows e macOS. Mobile está fora do escopo por enquanto.

## Jogos

O repositório não distribui jogos. Coloque os seus em `roms/`, que é ignorada pelo git ou em qualquer outra pasta, e defina nas configurações do programa.

## Documentação

- [ARCHITECTURE.md](ARCHITECTURE.md) — como o emulador é feito
- [docs/](docs/README.md) — a pesquisa: o console, a plataforma BREW, os formatos de arquivo e o
  estado da arte da emulação de Zeebo
- [docs/implementacao/](docs/implementacao/README.md) — cada subsistema, com as decisões e o
  porquê de cada uma
- [TODO.md](TODO.md) — o diário de bordo: o que está pronto, o que falta e o que já custou caro

## Licença

GPL-2.0, o texto completo em [LICENSE](LICENSE).


## Menções

- **tripleoxygen** — engenharia reversa de hardware e firmware do Zeebo, e o material público
  que torna este projeto possível :)