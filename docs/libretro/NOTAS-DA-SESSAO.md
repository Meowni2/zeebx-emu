# Notas da sessão — 21/09/2026

O que foi medido, o que mudou e o que ficou por fazer. Escrito para quem vai continuar: cada
achado traz o sintoma **e** a evidência, e nenhum deles é dedução.

## O "Memory is insufficient" do Double Dragon

**Causa medida:** o jogo abre `./udata/ddz.sav` com `OFM_CREATE` **sem chamar `MkDir` antes**, e o
diretório `udata/` não existia. Um refactor desta sessão (o commit `748525c`, que extraiu o bloco de
arquivos para `machine/file.rs`) perdeu, na mudança, o `create_dir_all` do diretório-pai que
existia no ramo de criação. A abertura falhava com `No such file or directory`, e o jogo — que não
distingue "não consegui abrir" de "acabou a memória" — mostrava a tela de falta de memória.

Isso explica o que a equipe viu: no `zeebx padrão` (upstream) o `create_dir_all` está lá, e por isso
o Kaio nunca teve o problema; o Tênis do João funcionava porque não cria arquivo em pasta nova no
arranque.

**Como se mediu:** o relatório da varredura passou a registrar as chamadas do `IFileMgr` com
caminho, argumentos e retorno, e o caminho do host com o erro do sistema quando a abertura falha.
A linha que resolveu:

```text
OpenFile "./udata/ddz.sav"  (0x5e078 0x4 0xf0003008) -> 0
open falhou: <cache>/…/mod/274754/udata/ddz.sav (No such file or directory)
```

Depois do conserto: `OpenFile "sound.ggz" -> 805307728`, o `.sav` é criado, e a tela desenhada passa
a ser `CARREGANDO`.

## Três armadilhas de CI, todas medidas

1. **Push cancela push.** Quatro execuções seguidas do workflow `ci` terminaram `cancelled`, porque
   cada push novo derrubava a matriz de seis alvos que estava no meio — e o CI do standalone
   **nunca** tinha rodado até o fim. Corrigido: `cancel-in-progress` só em PR.
2. **`continue-on-error: ${{ matrix.experimental }}` faz o GitHub rejeitar o arquivo inteiro.** O
   sintoma é um run **sem nenhum trabalho** (`gh api .../runs/<id>/jobs` devolve `total_count: 0`).
   Nem `actionlint` nem um leitor de YAML acusam. Regra: run vazio é o arquivo, não a máquina.
3. **Os testes nunca compilavam o alvo do binário**: o `main.rs` importava `zeebx::varredura` sob
   `#[cfg(test)]`, e nesse alvo a biblioteca é compilada **sem** o `cfg`. Rodar `cargo test --lib`
   escondia isso.

Mais dois, de plataforma:

- **macOS**: dois testes de VFS comparavam a caixa exata do caminho, e o APFS **não distingue
  caixa** — não há duas grafias para comparar. O mesmo tipo de defeito apareceu no Windows x86_64,
  num teste que procurava `/zeebx/` no texto (o Windows usa `\`).
- **Windows ARM64**: o `unicorn` (QEMU em C) **não compila** ali — o QEMU monta
  `setjmp-wrapper-win32.asm` com o `ml` do MSVC, que só existe para x86 e x64. O `unicorn-engine`
  2.1.5 é a última versão publicada. Solução: dependência condicional de alvo, e o `dynarmic` passa
  a ser o backend padrão naquele alvo, com os ganchos de depuração do unicorn implementados como
  recusa explícita.

## Dois jogos estavam mudos, e o que os calava

O relatório lista os sons recusados. Fazer a recusa **dizer o formato e os primeiros bytes** custou
uma linha e resolveu os dois casos:

```text
Turma da Mônica   recusado (formato desconhecido, 86 sons, 4f 67 67 53 …)   ← "OggS"
Peggle            recusado (audio/mpeg, 7 sons, 49 44 33 03 00 00 00 00 …)   ← "ID3"
```

| Jogo | Antes | Depois |
|---|---|---|
| Peggle | 7 recusas, pico **0,000** | 0 recusas, pico **0,356** |
| Turma da Mônica | 86 recusas, pico **0,000** | 0 recusas, pico **0,586** |

- **Ogg/Vorbis**: o `symphonia` já sabia decodificar; faltava ligar a feature.
- **MP3 com etiqueta ID3 que mente**: a etiqueta declara **zero byte** de tamanho e tem trinta mil de
  conteúdo. Quem pula pelo campo declarado procura o quadro no lugar errado e recusa o arquivo
  inteiro. Agora a busca é pela **sincronia do quadro**.

**Lição que vale para o resto do motor:** uma recusa que não diz *o que* chegou custa uma
investigação inteira dentro do jogo.

## Desempenho, medido

- **Rolima**: 6,8 milhões de chamadas de API em seis segundos, num laço de espera que o atalho de
  ocioso não reconhecia (relógio, `memset`, consulta de controle). Depois de reconhecê-lo:
  **79% → 284%**, instruções de 2,46 bilhões para 886 milhões. 51 jogos ficaram mais rápidos, e
  **nenhum estado mudou** em quatro varreduras do corpus.
- **Onde o tempo vai** (`ZEEBX_ROM_PERFIL=1`): no Crash Nitro Kart, `eglSwapBuffers` é **89,8%** do
  tempo de API — é o rasterizador software desenhando a cena. Ainda assim o jogo roda a 1039%.
- **Lacuna de GL**: a seção "GL atendido sem fazer nada" apontava **uma** chamada nas 62 ROMs, o
  `glPixelStorei` (alinhamento de linha na textura), em seis jogos. Implementado; depois, **zero**
  jogos ignoram qualquer chamada de GL.

## O que falta

- **Item 5** (render em hardware): o rasterizador da placa está **verificado contra o de software**
  (idêntico em Double Dragon e Crash; 2,6% de arredondamento no Peteca), e falta o encanamento com
  o frontend. O mapa está em `docs/libretro/LIBRETRO_PLAN.md`.
- **Item 6** (save states): o core declara que não tem, e o teste trava o critério ("sem estado
  parcial"). O estado completo precisa da descrição de cada objeto vivo.
- **Item 8** (ciclo da Z-Wheel): verificado que a varredura **não** consegue exercitá-lo — doze
  segundos com e sem manche diferem em mil instruções de 133 milhões. Só o RetroArch responde.
- **Item 9** (capas e No-Intro): local pronto; falta publicar.

## Como fechar o item 8 (o ciclo da Z-Wheel)

É a única coisa que nenhum teste daqui alcança: a roda **não desenha** no caminho da varredura
(medido: doze segundos com e sem manche diferem em mil instruções de 133 milhões), então só o
RetroArch responde. O roteiro, e o que cada resultado significa:

1. abra a **Z-Wheel** como conteúdo (o `.zip` do pacote, ou o `.mod` de dentro dele);
2. espere a roda aparecer — o core acha **63 jogos** ao lado do conteúdo (medido), então ela não
   pode abrir vazia;
3. **navegue pelo manche** (a roda é navegada pelo analógico, não pelos botões) até um jogo;
4. **confirme** com o botão de ação — o shell então pede a abertura, e o core troca de sessão
   sozinho;
5. jogue alguns segundos e **saia pelo Select** do RetroPad (ele vale como `AVK_CLR`, o "voltar" do
   console) — a volta é para a roda, como no aparelho.

O que observar, e o que cada coisa quer dizer:

| Sintoma | Leitura provável |
|---|---|
| a roda abre e navega, mas nenhum jogo abre | o pedido de abertura não chegou, ou o ClassID não está na pasta de jogos |
| o jogo abre e o Select não volta | o atalho de `AVK_CLR` ou a volta pela sessão anterior |
| a roda abre **vazia** | não é descoberta: o core vê os 63 (medido) — é desenho ou instalação |
| a roda não desenha nada | mesma família do Prey Evil: falta alguma interface gráfica |

A linha do log que interessa em cada caso:

```text
Zeebx: o shell pediu {classe}; abrindo {caminho}
Zeebx: o shell pediu {classe}, que não está na pasta de jogos
```

## Como capturar o que o core diz

O core escreve em dois lugares, e os dois servem para diagnosticar sem abrir depurador:

- **na tela**, por `RETRO_ENVIRONMENT_SET_MESSAGE`: recusa de render em hardware, falha ao abrir
  jogo, avisos de acervo. Três segundos, tempo de ler sem atrapalhar quem joga;
- **no log do RetroArch**, por `RETRO_ENVIRONMENT_GET_LOG_INTERFACE`: a mesma informação e mais
  (quantos jogos achou, se desenha na placa, que applet o shell pediu).

Para ver o log sem mexer nas preferências:

```bash
retroarch --verbose 2>&1 | grep -i zeebx
```

O que procurar, em ordem de importância para o item 5:

```text
Zeebx: o frontend aceitou render em hardware (OpenGL 3.3); o desenho passa a ser na placa
Zeebx: desenhando na placa
```

ou, se a tentativa falhar — e aí o jogo continua rodando no processador:

```text
Zeebx: o render em hardware falhou (…); seguindo no processador
```

Qualquer uma das duas linhas é resultado: a primeira diz que o encaixe fechou, a segunda diz onde
ele não fechou.

## Ferramentas que nasceram aqui

```bash
# varredura com relatório por jogo, comparando com a linha de base
ZEEBX_ROM=roms ZEEBX_ROM_MS=6000 ZEEBX_ROM_SAIDA=saida ZEEBX_ROM_BASE=docs/varredura \
  cargo test --release varredura -- --nocapture

# roteiro de controle no relógio virtual (botões e manche)
ZEEBX_ROM_TECLAS="4000:x=-128,10000:b1,11000:" …

# perfil de custo por método de API
ZEEBX_ROM_PERFIL=1 …

# os dois rasterizadores, lado a lado, sem janela
cargo run --release -- sessao "roms/Crash.zip" --seconds=3 --dump=software.bmp
cargo run --release -- sessao "roms/Crash.zip" --seconds=3 --placa --dump=placa.bmp

# o core pela própria ABI, com uma ROM de verdade
ZEEBX_CORE_ROM="roms/Z-Wheel.zip" cargo test -p zeebx-libretro -- --nocapture
```
