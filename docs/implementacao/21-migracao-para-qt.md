# 21 — A migração da interface para Qt

Plano de troca do `egui`/`eframe` por Qt 6, com o [cxx-qt](https://github.com/KDAB/cxx-qt) como
ponte entre Rust e Qt. Este documento é **plano**, não descrição do que existe: cada fase, quando
concluída, deve ser movida para o documento do assunto (o [10](10-interface.md) em especial) e
marcada aqui como feita.

## Estado

| Fase | Situação |
|---|---|
| 0 — prova de viabilidade | pontos 1 a 3 feitos no Linux (`frontends/classical-standalone/src/qt/`, feature `ui-qt`); o ponto 4 tem workflow (`qt.yml`), falta rodá-lo |
| 1 — desacoplar | o `glow` direto no núcleo já veio com a 0.3.0; o resto não começou |
| 2 a 9 | não começadas |

## Onde o egui está de verdade

O ponto de partida era a suposição de que o egui mora só em `src/ui/`. Não mora — e o que vaza é
o que mais pesa na migração:

| Acoplamento | Onde | Peso |
|---|---|---|
| ~~`eframe::glow` no núcleo~~ | resolvido na 0.3.0: o núcleo usa o `glow` direto | — |
| **O rasterizador na placa roda no contexto de GL do eframe** | `App::gl` → `Session::start_with` → `GpuState::novo` | estrutural: ver abaixo |
| Nomes de tecla | `Source::Key { name }` guarda o `egui::Key::name()` | compatibilidade do `settings.json` |
| `egui::ViewportBuilder`/`ViewportCommand` | `ui/settings.rs` | mover para o lado do egui |
| `egui::epaint::ViewportInPixels` | `ui/gpu.rs` (`Pintor`) | trocar por um tipo próprio |
| O desenho em si | `ui/app.rs` (3.400 linhas), `ui/app/vitrine.rs` | reescrita |

`input/bindings.rs` **não** usa `egui::Key`: as teclas já são guardadas por nome. Mas o nome é o
do egui, e é o que está gravado na configuração de quem já usa o
emulador. O Qt precisa de uma tabela `Qt::Key → nome do egui`, com teste, e não de um formato
novo. Os nomes são os do `egui::Key::name()`: `"Up"`, `"Enter"`, `"Space"`, `"Z"`, `"4"` — não
`"ArrowUp"`, que é o nome da variante.

O `glutin` e o `ab_glyph` **ficam**. O primeiro dá o pbuffer do `run` e do `bench`, onde toda a
medição é feita; o segundo é o `IDISPLAY_DrawText`. Nenhum dos dois é da interface.

### O contexto emprestado

`GpuState::novo` recebe o contexto da janela em vez de criar um, e o comentário ali diz por quê:
o núcleo roda na thread da interface, e um contexto próprio tornado corrente nela desliga o do
eframe — a janela congela. No Qt a mesma coisa vale, com uma saída melhor: o `QOpenGLContext`
tem `makeCurrent`/`doneCurrent` explícitos, e o Qt Quick torna o contexto dele corrente antes de
desenhar. Então o rasterizador pode ter um contexto **próprio, fora de tela e compartilhado** com
o do Qt Quick (`Qt::AA_ShareOpenGLContexts`):

- ele sobrevive à janela do jogo fechar, o que a Z-Wheel reabrindo jogos agradece;
- a textura grande do `quadro_na_placa()` é visível ao scene graph sem cópia;
- o passo fica `makeCurrent → Session::step → doneCurrent`, sem disputar contexto com ninguém.

## Por que Qt Quick e não Widgets

O cxx-qt gera `QObject`s em Rust — propriedades, sinais, slots, herança de `QAbstractListModel` e
de `QQuickPaintedItem` — para serem usados a partir de **QML**. Não há bindings de QtWidgets: cada
`QDialog` ou `QListWidget` seria C++ escrito à mão. O desenho fica em QML, o estado e a lógica em
Rust, e a cola em C++ se limita ao que o cxx-qt-lib não cobre:

1. **A preparação do GL**, antes do `QGuiApplication`: `QQuickWindow::setGraphicsApi(OpenGL)` (no
   macOS o padrão é Metal, no Windows D3D), `QSurfaceFormat` 3.3 core — o mesmo contrato que o
   eframe entrega hoje e que os shaders do `Pintor` e do `GpuState` esperam — e
   `Qt::AA_ShareOpenGLContexts`.
2. **O carregador de funções de GL** (`QOpenGLContext::getProcAddress`), para o
   `glow::Context::from_loader_function_cstr`.
3. **A tela do jogo com textura nativa**: um `QQuickItem` com `updatePaintNode`, porque não há
   binding de `QSGNode`.
4. **O provedor de capas** (`QQuickImageProvider`).

### Uma thread só, e o render loop `basic`

O emulador continua na thread da interface ([01](01-arquitetura.md#o-emulador-roda-na-linha-da-interface)).
No Qt Quick isso tem uma consequência: o render loop `threaded`, padrão no Linux com Mesa, desenha
o scene graph em outra thread — que leria a textura do rasterizador enquanto a thread principal já
escreve o quadro seguinte nela. O `basic` desenha na thread principal, como o egui fazia. Fica
fixado até haver motivo medido para trocar; se houver, a textura precisa ser copiada no
`updatePaintNode`, o único instante em que as duas threads estão paradas juntas.

O passo do jogo acompanha o quadro da janela — um `FrameAnimation` do QML chama o passo a cada
quadro, no ritmo do vsync, como o `request_repaint` do egui. Um `QTimer` de intervalo zero seria
uma CPU a 100% à toa.

### Licença

**Isto contradiz o [`AGENTS.md`](../../AGENTS.md), e a decisão é de quem mantém o projeto.** O
`AGENTS.md` diz que o `GPL-2.0-only` exclui "Qt 6, por exemplo". Os cabeçalhos dos módulos que a
interface usa dizem outra coisa — conferido no Qt 6.11 instalado, em `QtCore`, `QtGui`, `QtQuick`,
`QtQml` e `QtNetwork`:

```text
SPDX-License-Identifier: LicenseRef-Qt-Commercial OR LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only
```

Com a opção `GPL-2.0-only`, o binário continua inteiro em GPLv2. O que a regra acerta é que **nem
todo módulo Qt oferece essa opção** — há módulos só GPLv3 —, então cada um que entrar precisa ter o
cabeçalho conferido. O cxx-qt é MIT/Apache. Até o `AGENTS.md` ser revisto, a interface Qt não deve
ir para uma release.

## As fases

### 0 — Prova de viabilidade

Antes de migrar tela nenhuma, provar os quatro pontos que podem inviabilizar o plano:

1. uma janela QML com texto vindo de uma propriedade Rust;
2. o quadro RGB565 de uma `Session` desenhado por um `QQuickPaintedItem` com
   `QImage::Format_RGB16` — que é exatamente RGB565, sem conversão na CPU;
3. um `glow::Context` tirado de um contexto do Qt, com o `GpuState` rodando nele;
4. o build nos três sistemas.

Fica atrás da feature `ui-qt` e do subcomando `zeebx qt <jogo> [--placa]`, sem tocar no caminho
do egui:

```bash
cargo run --release -p zeebx-classical-standalone --features ui-qt -- \
    qt "Crash Bandicoot Nitro Kart 3D (Brazil) (Es,Pt).zip" --placa
```

O código mora em `frontends/classical-standalone/` (`src/qt/`, `qml/`), e não no núcleo: o núcleo
é também o do core Libretro e do Android, e janela é coisa de frontend. A feature `ui-qt` é do
standalone e fica fora de todo job do `ci.yml` e do `release.yml`, que compilam por `-p` sem
`--all-features`.

O que a prova mostrou, no Linux com Wayland e NVIDIA (driver 615, Qt 6.11, cxx-qt 0.10):

- **Os pontos 1 a 3 funcionam.** O Pac-Mania aparece pelo `Format_RGB16`, e o Crash Nitro Kart
  roda com o `GpuState` num contexto fora de tela do Qt, compartilhado, passando pelo `glow` —
  o mesmo `GpuState`, sem mudança nenhuma no núcleo.
- **O formato precisa dizer `setRenderableType(OpenGL)`.** Sem isso, o pedido de 3.3 core volta
  `EGL_BAD_MATCH` no Wayland e o Qt Quick aborta ao abrir a janela; no X11 passava. O `qFatal`
  vai para o journal, não para o terminal — `QT_FORCE_STDERR_LOGGING=1` o traz de volta.
- **O construtor de um item precisa de `impl cxx_qt::Initialize`.** Sem ele o cxx-qt gera um
  construtor que repassa o pai `QObject*` à base, e o `QQuickPaintedItem` quer um `QQuickItem*`.
- **A cola em C++ foi pequena:** a preparação do GL e o contexto fora de tela, umas 80 linhas em
  `src/qt/cpp/`. As funções de GL passam ao Rust como `rust::Str`, não `const char*`.
- **O contexto fora de tela precisa ser destruído à mão, e na ordem certa.** Deixado para os
  destrutores estáticos do C++, ele morria depois do `QGuiApplication`, e **fechar a janela
  terminava em SIGSEGV** dentro do `~QOffscreenSurface` — nenhum teste pegava, porque todos
  terminavam pelo `timeout`. A ordem agora é: a engine (a tela solta a sessão com o contexto
  vivo), o contexto (`gl_destroi`), o `QGuiApplication`. Medido fechando a janela pelo QML: saída
  0 nos dois rasterizadores.

Falta: rodar o `qt.yml` — Linux, Windows x64 e macOS nas duas arquiteturas, todos com o Qt 6.8
LTS do `install-qt-action` (ponto 4) —, e a entrada, que existe — teclado pelo nome do
egui e controles pelo `gilrs`, com o mapeamento do `settings.json` — mas não foi exercitada à
mão. O quadro grande do `quadro_na_placa()` ainda passa pela leitura em RGB565: embrulhar a
textura sem cópia é da fase 3.

### 1 — Desacoplar o núcleo do eframe

- ~~`glow` como dependência direta~~ — feito na 0.3.0.
- `ui/gpu.rs` recebe um `Viewport` próprio em vez do `ViewportInPixels`.
- `ui/settings.rs` perde os dois métodos que falam de `ViewportBuilder`.
- `input/teclas.rs` com a lista canônica de nomes, e o teste que confere a tradução nos dois
  backends.
- O `ui::App` se parte em `ui/nucleo.rs` (estado e lógica: `play`, `rescan`, `pads_now` recebendo
  o conjunto de teclas apertadas, `avks_ativos`, `placement`, saves, Discord, atualização,
  calibração) e `ui/egui/` (só desenho). Os testes de `ui/app.rs` vão com o núcleo.

Sem mudança de comportamento: o egui continua sendo a interface.

### 2 — Estrutura Qt ao lado do egui

Features `ui-egui` (padrão) e `ui-qt`; o `build.rs` só chama o `CxxQtBuilder` com `ui-qt`,
mantendo a `libatomic` e o `winresource`. Um `Rc<RefCell<Nucleo>>` na thread principal, e
`QObject`s finos (`Biblioteca`, `Jogo`, `Configuracoes`, `Saves`) sobre ele.

### 3 — A janela do jogo

O contexto próprio compartilhado, o passo no ritmo da janela, os dois caminhos de desenho (a
textura do `quadro_na_placa()` embrulhada num `QSGTexture`, ou o RGB565 subido só quando a série
muda), teclado por `Keys.onPressed`/`onReleased` com o conjunto limpo ao perder o foco, tela cheia
por `Window.visibility`, e as sobreposições (aviso de calibração, "parou", linha do tempo) como
itens QML.

Critério de saída: Quake e Crash Nitro Kart nos dois rasterizadores (a placa em escala 3, 16:9 e
MSAA 4x), a Z-Wheel lançando um jogo e voltando, e uma porta com Boomerang.

### 4 — Biblioteca

Varredura, `acervo`, `casa_com_a_busca` não mudam. Um `QAbstractListModel` em Rust; grade em
`GridView` com `smooth: false` para ícone menor que o cartão (a ampliação sem interpolação do
[10](10-interface.md#a-imagem-de-cada-cartão)); slider em `PathView`, com a mola e a volta da
lista continuando em Rust. O controle é lido por um `QTimer` enquanto a biblioteca tem o foco —
o controle não gera evento no Qt, como não gerava no egui.

### 5 — Configurações

Uma `Window` com as abas em `ScrollView`. O `PadArt` já é independente de toolkit: cada silhueta
vira uma `Image`, e o clique continua testado pela máscara em Rust. Captura de tecla pela mesma
tabela `Qt::Key → nome`. O `rfd` fica na primeira passada — ele não depende de toolkit.

### 6 — Janelas auxiliares

Saves, log, aviso de abertura e atualização. A thread da atualização devolve o resultado por
`qt_thread().queue(...)`, sem polling.

### 7 — Conferência

`discord.rs`, `i18n.rs`, `saves.rs`, `acervo.rs` e `library.rs` não mudam. Os textos continuam
vindo do `Catalog` — **não** do `qsTr` —, para os JSON soltos e o teste de chaves seguirem
valendo.

### 8 — Corte do egui

`ui-qt` como padrão por uma versão, depois saem o `eframe`, o `ui/egui/` e, se trocado pelo
`QtQuick.Dialogs`, o `rfd`. O `--window` (`minifb`) é decisão pendente do `TODO.md`,
independente desta. O [10](10-interface.md), o diagrama de camadas do
[`ARCHITECTURE.md`](../../ARCHITECTURE.md) e os comentários de `video/contexto.rs` e
`video/gpu.rs` que falam do eframe são reescritos.

### 9 — Build e empacotamento

O CI instala o Qt nos três sistemas. O `cargo packager` não implanta Qt: cada sistema ganha o
passo dele (`linuxdeploy-plugin-qt`, `windeployqt`, `macdeployqt`), e o `.deb` passa a depender
de `libqt6gui6`, `libqt6qml6`, `libqt6quick6` e dos módulos QML usados.

## Riscos, do maior para o menor

1. O contexto compartilhado entre o `GpuState` e o scene graph, sobretudo no macOS, onde o OpenGL
   está obsoleto mas ainda atrás do RHI. É a razão de a fase 0 existir.
2. O Qt no CI e nos pacotes dos três sistemas.
3. A fidelidade do slider cilíndrico.
4. O `settings.json` antigo: os nomes de tecla do egui precisam continuar valendo.
