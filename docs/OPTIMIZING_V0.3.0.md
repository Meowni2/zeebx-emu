# Otimização do Zeebx — linha de partida da 0.3.0

Este documento existe por um motivo simples: **os números de desempenho que o projeto publica
não descrevem mais o emulador que está na árvore.** Eles foram medidos com o Unicorn, e o
Unicorn saiu. Antes de otimizar qualquer coisa é preciso voltar a medir, e antes de medir é
preciso dizer em voz alta o que se sabe, o que se supõe e o que se vai conferir.

O alvo desta rodada não é o desktop. É o portátil: **R36S** (Cortex-A35, ArkOS) e **RG40XX-H**
(Cortex-A53, muOS), pelo core Libretro. O desktop entra como bancada de medida, porque nele dá
para instrumentar sem cartão SD no meio.

## 1. O que está desatualizado

O `ARCHITECTURE.md` traz, na seção "Limites conhecidos", uma tabela de custos que vinha do
backend antigo:

| número publicado | onde | estado |
|---|---|---|
| 52% da velocidade do console no Quake | `ARCHITECTURE.md:243` | medido com Unicorn |
| ~68% ARM + despacho, ~28% pixels, ~4% geometria | `ARCHITECTURE.md:243` | medido com Unicorn |
| 1,4 µs por chamada de API | `ARCHITECTURE.md:264` | medido com Unicorn |
| 57 ns por `read_u32` contra 0,3 ns em bloco | `ARCHITECTURE.md:252` | texto diz "FFI do unicorn" |
| 218 M instr/s no laço, ~86 M no jogo | `ARCHITECTURE.md:257` | medido com Unicorn |
| `cargo test --release instrucoes_por_segundo` | `ARCHITECTURE.md:258` | **o teste não existe mais** |

As datas fecham o caso:

```
8af069f  2026-09-11  Atualiza os números de performance medidos
a359183  2026-09-23  Remove o Unicorn e deixa o Dynarmic como backend padrão
```

A medida é doze dias mais velha que a remoção do backend que ela mediu. O documento também
descreve a árvore com `cpu/unicorn.rs` e um `CpuBackend` "sobre o unicorn"
(`ARCHITECTURE.md:59`, `:83`), arquivo que não está mais lá.

**Nada disso quer dizer que as conclusões estavam erradas.** Quer dizer que elas precisam ser
refeitas antes de servirem de base para decisão de otimização.

## 2. O que continua verdadeiro na árvore de hoje

Estes pontos foram conferidos no código, não no documento:

- **O despacho de API ainda para o JIT.** A callback chama `halt(HaltReason::UserDefined1)`
  (`src/cpu/dynarmic.rs:126`), o método é atendido no host, e o laço limpa a razão e volta a
  entrar com `jit.run()` (`src/cpu/dynarmic.rs:587`). A ida e volta por chamada continua
  existindo com o Dynarmic; o que mudou foi o custo dela, que ninguém mediu ainda.
- **A tabela de páginas já é um fastmem parcial.** `config.page_table(...)` com
  `page_table_mask(0)` (`src/cpu/dynarmic.rs:450`), e as entradas nulas caem nas callbacks:
  regiões só de leitura, páginas já executadas, páginas com vigia de escrita e a última página
  de uma região que não a completa (`src/cpu/dynarmic.rs:314`).
- **Há um número recente e útil no próprio código:** sem a tabela de páginas, o Need for Speed
  ficava em 176 milhões de instruções por segundo, 69% da velocidade do console
  (`src/cpu/dynarmic.rs:317`). Esse é um número da era da tabela, não do Unicorn.
- **O rasterizador já é paralelo.** Ele divide o quadro em faixas e usa `std::thread::scope`
  (`src/video/rasterizer.rs:2341`). Quem propuser "paralelizar o rasterizador" está propondo o
  que já existe.
- **O perfil de custo por API existe e é opt-in.** `Machine::enable_api_profile()`
  (`src/machine/diagnostico.rs:87`) e `Machine::api_profile()` (`:92`), ligados pela varredura
  com `ZEEBX_ROM_PERFIL`.
- **A varredura já mede desempenho.** `Desempenho` (`src/varredura.rs:427`) traz `instrucoes`,
  `chamadas`, `quadros`, `pixels`, `voltas`, `virtual_ms`, `laco` e `velocidade()` em por cento
  da velocidade do console.
- **O perfil de release já está apertado.** `lto = "thin"` e `codegen-units = 1`
  (`Cargo.toml:171`). Quem sugerir "ativar LTO" está sugerindo o que já está ligado; o que
  sobra é comparar `thin` com `fat`.

## 3. O que o core Libretro já oferece ao portátil

Treze opções, das quais estas mexem diretamente em custo:

| opção | efeito | vale quando |
|---|---|---|
| `zeebx_frameskip` | pula desenho, sem mexer no relógio | a rasterização domina |
| `zeebx_limite_fps` | freia apresentação em 60/30 | o aparelho corre solto e esquenta |
| `zeebx_resolucao_interna` | supersampling **para cima**, 1..4 | sobra GPU |
| `zeebx_antialias` | amostras por pixel na placa | sobra GPU |
| `zeebx_rasterizador` | força software quando o driver mente | a placa desenha errado |
| `zeebx_perfil` | Portátil: taxa, vozes e cache menores | o aparelho é fraco |
| `zeebx_soundfont_taxa`, `zeebx_midi_vozes`, `zeebx_cache_de_som_mb` | custo do MIDI | a música é pesada |

Repare no que **não** existe: uma resolução interna **abaixo** de 1. Hoje `escala` é um
multiplicador inteiro (`src/video/gpu.rs:207`, `:727`, `:2270`) e só vale no rasterizador de
placa. Para `0,5x` e `0,25x` seria preciso fração, e o ganho maior provavelmente está do lado
do rasterizador de software, onde cada pixel é CPU.

## 4. As hipóteses que vão ser testadas

Nenhuma delas está decidida. São hipóteses, e a medida decide.

1. **O gargalo do portátil é o ARM + HLE, não o preenchimento.** Se for verdade, `0,5x`,
   GLES 2 e frameskip não resolvem; o que resolve é trampolim e leitura em bloco.
2. **O custo por chamada de API caiu com o Dynarmic.** O `1,4 µs` é do Unicorn. Pode ter caído
   muito, pode ter caído pouco. Enquanto não se medir, a prioridade 1 é uma aposta.
3. **A leitura de página só-de-leitura ainda passa por callback.** Se ela pesar, estender o
   fastmem para leitura é barato e rende.
4. **A síntese do SoundFont trava a primeira música.** O custo é de carga, não de quadro, mas
   um engasgo de centenas de ms é sentido no portátil.
5. **`0,5x` no rasterizador de software vale mais que `0,5x` na placa.** O preenchimento por
   CPU escala com área; o Mali-G31 a 640x480 provavelmente não está saturado.
6. **NEON rende no host, não no guest.** O código ARM do jogo é do guest e passa pelo JIT; o
   que dá para vetorizar é o que o host faz: preencher, converter, copiar e limpar.

## 5. O que **não** vai ser feito nesta rodada

Dito agora para não virar discussão depois:

- **Não** se vai copiar o SH4ZAM. Ele é assembly SH-4 do Dreamcast, e o host aqui é ARM64 ou
  x86-64. A ideia aproveitável dele é "rotina especializada por arquitetura do host", e isso
  se escreve com NEON, não com FSCA e XMTRX.
- **Não** se vai baixar o requisito para GLES 2 por desempenho. GLES 2 é assunto de
  **compatibilidade** — Android antigo, driver limitado —, e mudaria VAO, `blit_framebuffer`,
  multisample e versão de shader (`src/video/gpu.rs:284`, `:533`, `:419`, `:1493`).
- **Não** se vai fazer backend GLES 1. Seria um segundo rasterizador de pipeline fixo.
- **Não** se vai criar thread de saída de áudio no core. Em Libretro quem puxa o áudio é o
  frontend; o core que cria thread de saída está brigando com o contrato.
- **Não** se vai ajustar mais timbre nem ganho do MIDI. Isso já foi calibrado por medição.

## 6. Como medir

### 6.1 Na bancada (desktop)

A varredura já faz o trabalho e não precisa de janela:

```sh
ZEEBX_ROM="<jogo>.zip" \
ZEEBX_ROM_PERFIL=1 \
ZEEBX_ROM_MS=15000 \
ZEEBX_ROM_SAIDA=/tmp/perf \
cargo test --release varredura -- --nocapture
```

O que sai dali: velocidade em por cento do console, instruções, chamadas, quadros, pixels e a
lista de métodos de API mais caros, com total.

Regras de medida, herdadas do erro documentado em `8af069f`:

- **medir tempo sem o perfil ligado.** O perfil de blocos custou 24% na medida antiga, e o
  preço cai quase todo na fatia do ARM;
- usar o mesmo `ZEEBX_ROM_MS` em todas as comparações;
- rodar sempre em `--release`;
- comparar proporção com perfil ligado, relógio com ele desligado.

**Cuidado com o que o perfil de API mede.** O `api_time` só soma o tempo de
`dispatch_inner` (`src/machine/mod.rs:3370`), isto é, **o corpo do método**. Ele não inclui o
`halt` do JIT nem a reentrada por `jit.run()`. O custo do trampolim, que é justamente o número
de `1,4 µs` que se quer refazer, **não sai daqui**: precisa de um micro-teste próprio — um laço
do guest chamando um método barato, comparado com o mesmo laço sem a chamada. Confundir os dois
é o erro fácil desta rodada.

### 6.2 Jogos da rodada

| jogo | por que ele |
|---|---|
| Quake | pior caso histórico, meio milhão de draw calls |
| Crash Bandicoot Nitro Kart 3D | usa `glReadPixels`, desliga frameskip |
| Need For Speed Carbon | 3D com muita geometria; deu o número do fastmem |
| Zeebo Extreme Rolimã | já mostrou `memset` como custo |
| Double Dragon | 2D, e é o que se joga nos testes de aparelho |

### 6.3 No aparelho

O que falta é telemetria por quadro no core. O que se quer contar, por segundo:

```
tempo do guest (ARM)
tempo em gles_draw
número de draws e de vértices
número de glReadPixels
tempo em memset/cópia
tempo de conversão do quadro
tempo de áudio
quadros pulados
```

Enquanto isso não existe, o aparelho só responde "quantos FPS", e isso não diz onde o tempo
foi.

## 7. A ordem de ataque

A ordem vale **depois** da medida, e muda se a medida contrariar.

| # | frente | por que agora | risco |
|---|---|---|---|
| 1 | Re-medir e corrigir `ARCHITECTURE.md` | o documento está guiando decisão com número velho | nenhum |
| 2 | Trampolim de API | é o custo fixo por chamada, e o doc já o aponta | alto: mexe no núcleo |
| 3 | Leitura em bloco e fastmem de leitura | a lição "pedir o bloco, nunca o elemento" já é do projeto | médio |
| 4 | NEON no host: preencher, converter, copiar, limpar | escala com pixel e com byte | médio |
| 5 | `0,5x`/`0,25x`, primeiro no software | corta área, e área é o custo do preenchimento por CPU | médio: viewport, tesoura, leitura |
| 6 | Síntese de SoundFont sem travar a carga | tira engasgo sentido no portátil | baixo |
| 7 | `lto = "fat"` medido contra `thin` | é barato de testar | baixo |

## 8. Critério de aceitação

Uma otimização entra quando:

1. há medida **antes e depois**, no mesmo jogo, com o mesmo `ZEEBX_ROM_MS`;
2. a varredura dos 62 jogos não regride comportamento — desempenho fica fora da comparação de
   linha de base de propósito, e é por isso que o número vive aqui e não lá;
3. o ganho aparece também no portátil, ou fica dito que é ganho só de desktop;
4. `cargo test`, `cargo clippy` e o core Libretro continuam verdes;
5. o documento de arquitetura é atualizado junto, com o número novo e a data.


## 10. Referência externa

Uma sessão de consulta com o DeepSeek sobre otimização de emuladores levantou pontos que valem
como leitura, e também mostra onde uma análise de fora erra:

https://chat.deepseek.com/share/yjigu5xc76ahsri24s

O que ela acertou: o trampolim de API é o gargalo estrutural; o rasterizador não é o gargalo; a
lição de pedir bloco em vez de elemento; estender o fastmem para leitura.

O que ela errou: propôs ativar `lto` e `codegen-units = 1`, que já estão ligados
(`Cargo.toml:171`), inferindo o arquivo pelo tamanho em bytes sem abri-lo; propôs paralelizar o
rasterizador, que já é paralelo (`src/video/rasterizer.rs:2341`); propôs thread dedicada de
áudio, que é contra o contrato do Libretro; e tratou os números do `ARCHITECTURE.md` como
atuais, quando são da era do Unicorn — que é exatamente o motivo deste documento existir.

## 11. Registro das medidas

| data | jogo | backend | velocidade | instr/s | chamadas de API | observação |
|---|---|---|---|---|---|---|
| 2026-09-11 | Quake | Unicorn | 52% | 86 M | — | `ARCHITECTURE.md:243`, cena de jogo |
| 2026-09-24 | Quake | Dynarmic | **170%** | **195 M** | 1.323.900 | 15.016 ms virtuais em 8,8 s reais, 583 quadros, rasterizador de software |

### A bancada

Número de desempenho sem a máquina que o produziu não vale nada. Todas as medidas de bancada
deste documento saem daqui, até que se diga o contrário:

| item | valor |
|---|---|
| aparelho | Lenovo IdeaPad 3 |
| processador | AMD Ryzen 7 5700U (Zen 2, 8 núcleos / 16 linhas, até 4,37 GHz) |
| gráficos | Radeon integrado (Vega), Mesa |
| memória | 9 GiB |
| sistema | Debian GNU/Linux 13 (trixie), núcleo 6.19.5-x64v3-xanmod1 |
| compilador | rustc 1.98.1 |
| perfil | `--release`, `lto = "thin"`, `codegen-units = 1` |

Isto é um portátil de escritório, não uma bancada de medição: o relógio varia com temperatura e
com o governador, e a medida do mesmo trabalho oscila entre execuções. Vale para comparar
**antes e depois no mesmo aparelho**, na mesma sessão, e não para publicar número absoluto.

E a diferença para o alvo é enorme, o que é justamente o ponto: o Ryzen 5700U tem 16 linhas de
execução e mais de 4 GHz, enquanto o R36S tem um Cortex-A35 e o RG40XX-H um Cortex-A53. Os 170%
do Quake **não** se transportam para o portátil, e qualquer conclusão sobre gargalo de aparelho
precisa ser medida no aparelho.

**A primeira medida com o Dynarmic desmente a leitura pessimista.** O Quake saiu de 52% para
170% da velocidade do console, e o núcleo de 86 M para 195 M de instruções por segundo — 2,3
vezes. O emulador que o `ARCHITECTURE.md` descreve não é o que está na árvore.

Ressalva honesta, para a medida não virar propaganda: **a cena não é a mesma.** A medida antiga
descrevia a cena de jogo com meio milhão de draw calls; esta rodou 15 segundos a partir da
abertura, com 278.297 `DrawArrays`. O que é diretamente comparável é o ritmo do núcleo
(instruções por segundo), não o relógio de parede de cenas diferentes. Para comparar relógio
será preciso um roteiro de teclas (`ZEEBX_ROM_TECLAS`) que chegue ao mesmo ponto do jogo.

O perfil de chamadas do mesmo relatório mostra onde a conversa com o guest se concentra:

| chamadas | método |
|---:|---|
| 283.982 | `IGLES11::VertexPointer` |
| 282.717 | `IGLES11::TexCoordPointer` |
| 278.297 | `IGLES11::DrawArrays` |
| 137.720 | `AEEHelpers::strcmp` |
| 61.143 | `AEEHelpers::aee_GetRand` |

As três primeiras somam 844.996 das 1.323.900 chamadas — **64% de todas as chamadas de API são
os três pontos de entrada de geometria**, e elas andam juntas: um `VertexPointer`, um
`TexCoordPointer` e um `DrawArrays` por lote. Se o custo do trampolim ainda pesar, é aqui que
ele pesa, e é aqui que agrupar paga.

## 12. Onde o tempo foi, nos cinco jogos (2026-09-24)

Rodada com `ZEEBX_ROM_PERFIL=1`, 15 s virtuais, rasterizador de software, na bancada descrita
acima. **O perfil custa caro**: o Quake sai de 8,8 s (170%) sem perfil para 12,6 s (119%) com
ele, cerca de 43%. Por isso a tabela abaixo serve para **proporção**, e o relógio absoluto vem
da rodada sem perfil.

| jogo | real | velocidade | instr/s | chamadas de API | tempo em API | apresentação |
|---|---:|---:|---:|---:|---:|---:|
| Crash Nitro Kart 3D | 1,5 s | 1009% | 207 M | 44.559 | 1.145 ms | 975 ms (85%) |
| Double Dragon | 2,6 s | 569% | 22 M | 37.808 | 2.010 ms | 1.643 ms (82%) |
| NFS Carbon | 6,3 s | 238% | 51 M | 252.324 | 4.809 ms | 4.062 ms (84%) |
| Quake | 12,6 s | 119% | 136 M | 1.323.900 | 8.109 ms | 5.139 ms (63%) |
| Zeebo Extreme Rolimã | 3,8 s | 396% | 240 M | 176.080 | 2.316 ms | 1.242 ms (54%) |

"Apresentação" é o `eglSwapBuffers`/`SwapBuffers`, e é preciso dizer o que ele **é** por dentro,
senão o número engana:

```rust
"SwapBuffers" => {
    self.sync_egl_color_from_guest()?;
    self.egl_swaps += 1;
    self.present_gl();
    self.wait_for_vsync();
    (2, gles::EGL_TRUE)
}
```
`src/machine/egl.rs:311`

O `present_gl` chama `frame_rgb565` (`src/machine/gl.rs:1051`), e o `frame_rgb565` do software
começa com `self.flush()` (`src/video/rasterizer.rs:2451`) — **é ali que a fila de triângulos
vira pixel**. Ou seja: o tempo do `SwapBuffers` é rasterização, conversão RGBA→RGB565 e cópia,
não "troca de buffer". O `wait_for_vsync` só mexe no relógio virtual e não dorme
(`src/machine/time.rs:115`).

### O que isso muda

**A frase "o rasterizador não é gargalo" do `ARCHITECTURE.md:271` não vale mais nesta
configuração.** Ela foi escrita quando o ARM custava o dobro; com o Dynarmic, a fatia do ARM
encolheu e o preenchimento passou a dominar: 54% a 85% do tempo de API em todos os cinco jogos,
e algo entre 41% e 65% do relógio de parede.

Outros alvos que o perfil mostra:

- **Geometria, no Quake:** `DrawArrays` 876 ms (10,8%), `TexCoordPointer` 431 ms (5,3%),
  `VertexPointer` 427 ms (5,3%) — 1,73 s, 21% do tempo de API. O trio não é caro só por parar o
  JIT; ele custa por dentro também.
- **`memset`, no Rolimã:** 728 ms, 31,4% do tempo de API. Confirma a suspeita antiga.
- **`strcmp`, no Quake:** 203 ms, 2,5%. Menor do que o número de chamadas sugeria — 137 mil
  chamadas, mas o `read_cbytes` já lê em blocos de 64 bytes.
- **Crash Nitro Kart:** 44 mil chamadas em 15 s e 1009% de velocidade. O jogo que motivou a
  proteção do `glReadPixels` é, na bancada, o mais folgado dos cinco.

### Consequência para o plano

A escala interna fracionária (`0,5x`, `0,25x`) **sobe** de prioridade, mas com um alvo
diferente do que se pensava: o caminho de **software**, onde o custo por pixel é CPU e o
`define_escala` hoje é ignorado de propósito (`src/video/rasterizer.rs:756`).

O trampolim **não** sobe nem desce: continua não medido, porque o `api_time` só conta o corpo
do método.
