# 07 — Áudio

## O que os jogos entregam

**Efeito é WAVE; música não.** Essa é a divisão, e ela explica um sintoma que durou muito tempo:
efeito sonoro tocava em todo jogo e trilha não tocava em nenhum. Não era um defeito no
misturador — eram dois formatos inteiros faltando.

Contado nos pacotes dos sessenta e três títulos:

| Formato | Onde aparece |
|---|---|
| RIFF/WAVE | efeito sonoro, em 14 jogos — sempre tocou |
| **MP3** | a música de 9 jogos: Quake, Quake 2, Galaxy on Fire, Rally Master Pro, Powerboat Challenge, Action Hero 3D, Need For Speed, zeetris e a Z-Wheel |
| **MIDI** | a música dos ports de arcade: Double Dragon, Bad Dudes, Caveman Ninja, Dark Seal, Heavy Barrel, Karnov's Revenge, Magical Drop 3, Spin Master, Street Hoop, Super BurgerTime e Wizard Fire |
| **PCM gerado pelo jogo** | os ports de arcade da Data East de novo: o som da placa emulada sai por um `ISource`, ver abaixo |

O levantamento é de leitura dos pacotes, não de suposição: os ports de arcade trazem **um** `.wav`
cada — o efeito — e a música deles chega pelo `IMedia` como MIDI, o que o relatório dizia numa
linha fácil de passar batido, `som recusado (audio/mid)`.

O MP3 já toca (ver abaixo). O MIDI é o assunto do fim desta página.

`audio/wav.rs` lê:

- PCM de 8 bits (sem sinal, centrado em 128) e de 16 bits (com sinal);
- **IMA ADPCM** (formato 17), que é o que o Pac-Mania usa.

Formato desconhecido é recusado **pelo número**: `WavError::NotPcm(tag)`. Um som que some sem
explicação vira uma caçada; um erro que diz "formato 85" vira uma linha de pesquisa.

## O caminho do BREW

```
IMediaUtil::CreateMedia(AEEMediaData { clsData, pData, dwSize })
        │
IMedia::SetMediaParm(MM_PARM_MEDIA_DATA | VOLUME | MUTE | PLAY_REPEAT)
        │
        ├──► na volta seguinte do laço, lê os bytes e decodifica o WAVE, o MP3 ou o MIDI
        │
IMedia::Play  ──►  voz no misturador
```

Os identificadores (`MM_PARM_MEDIA_DATA = 1`, `MM_PARM_VOLUME = 4`, `MM_PARM_MUTE = 5`,
`MM_PARM_PLAY_REPEAT = 11`, `AEE_MAX_VOLUME = 100`, `MMD_FILE_NAME = 0`, `MMD_BUFFER = 1`) vêm de
`AEEIMedia.h`.

**O buffer é lido na volta seguinte do laço de eventos** (`Machine::resolve_midia`, chamado no
`deliver_signals`), e o cache (`CargaDeMidia`) é pela chave do **conteúdo**. Os dois extremos
quebram jogos diferentes:

- **Lido no `Play`**, com cache por `(endereço, tamanho)` — como foi por muito tempo —, um jogo que
  carrega vários sons pelo mesmo buffer de rascunho já o reaproveitou quando toca, e um som novo no
  mesmo endereço e com o mesmo tamanho tocava o antigo.
- **Lido na entrega** — a primeira correção, vinda da comparação com o zeebulator3 —, o Zeebo F.C.
  Super League entregava o som de navegação do menu com só o cabeçalho WAV escrito: ele entrega,
  manda tocar e copia as amostras em seguida, no mesmo tratador. Saía um chiado com o conteúdo antigo
  do buffer, onde havia até o cabeçalho de uma textura ATC.

A volta do laço fica entre os dois: o tratador do jogo já terminou de escrever e ainda não
reaproveitou o buffer. No aparelho o `Play` também só lê depois, numa tarefa separada. Um `Play`
sobre um som ainda não lido marca o objeto como tocando, avisa o início e começa a voz quando o
buffer é lido; o `GetTotalTime` força a leitura na hora, porque quem pergunta precisa da duração.

Três formas de entrega:

- `MMD_BUFFER`: memória. Um buffer que começa com a assinatura do gzip (`1f 8b`) é descomprimido
  antes — o `sound.ggz` do Double Dragon guarda assim.
- `MMD_FILE_NAME`: o nome de um arquivo do pacote, lido pelo sistema de arquivos virtual. O
  Galaxy on Fire entrega as sete músicas dele assim (`GalaxyOnFire1_Theme.mp3` e as outras), e
  antes elas eram recusadas em silêncio. Um arquivo que não existe responde `EFAILED` e entra no
  relatório.
- `MMD_ISOURCE` só é tocado como PCM cru: um `ISource` que entregue um arquivo comprimido fica
  mudo.

As classes da família `AEECLSID_MULTIMEDIA` do SDK criam todas o mesmo objeto: QCP, PMD,
MIDIOUTMSG, MIDIOUTQCP, MPEG4, MMF, PHR, AAC, IMELODY, AMR, XMF e DLS, além de MEDIA, MIDI, MP3,
ADPCM e PCM. O conteúdo passa pelos decodificadores que temos, e o que nenhum lê termina na hora.
QCELP e EVRC não são decodificados aqui nem no zeebulator3.

O `RegisterNotify` é atendido: o jogo recebe `MM_STATUS_START` quando o som começa e
`MM_STATUS_DONE` quando ele acaba, no `AEEMediaCmdNotify` de 28 bytes de `AEEIMedia.h`. Sem esse
aviso, um jogo que só toca o próximo som quando o anterior termina emudece depois do primeiro.

**O `START` e o fim natural saem na volta do laço, e não na saída do `Play`**
(`Machine::entrega_avisos_de_midia`, no `deliver_signals`). Os Zeebo Extreme dependem disso. O
gerenciador de som deles (`SoundMgr`/`SoundPlayer` da biblioteca TTD) marca o som como "pedido"
(2) **depois** do `Play`, e o `START` o passa a "tocando" (1). Com o aviso na saída da chamada, o
`START` chegava antes da marca e o som ficava em 2 para sempre. O `Stop` deles só age em 1, então
a música do menu nunca parava. A música da pista ficava na fila esperando o canal (`+0x3634` do
tocador), e com música na fila o tocador recusa todo efeito: o Bóia Cross corria só com a trilha,
sem uma chamada de som sequer. O bloco do aviso é escrito na hora de cada entrega, porque um
`START` e um `DONE` do mesmo objeto na mesma volta dividem o bloco.

**Todo aviso sai assim, inclusive o `DONE` do `Stop`, e ele não some com o `Release`.** O
Double Dragon repete a música pelo `DONE`: se o objeto ainda está marcado, agenda um `Play` para
dali a 100 ms. Ele desmarca e solta o objeto logo depois do `Stop`; com o `DONE` na saída do
`Stop`, o tratador ainda via o objeto marcado, e o `Play` agendado caía num `IMedia` já liberado —
endereço zero, ao apertar voltar. O Zeebo F.C. Super League, ao contrário, para e solta e **espera**
o `DONE` daquele som: se o aviso fosse descartado com o objeto, a abertura parava na tela de aviso.
Por isso o tratador é guardado quando o aviso nasce, e a entrega não depende de o objeto existir.

**Um som entregue por memória é relido a cada `Play`.** O `IMedia` do aparelho não copia o buffer.
O Super League tem um objeto só para os efeitos da partida, com um buffer de 500 KB: escreve o
chute ou o passo e manda tocar de novo, sem outro `SetMediaParm`. Guardado da primeira leitura,
todo efeito saía com o som de seleção do menu, o primeiro a passar por ali. A releitura segue a
regra de cima — na volta do laço — e o cache pelo conteúdo evita decodificar de novo o que não
mudou.

O `Stop` de um som que tocava também avisa, **com `MM_STATUS_DONE`**. Era o que deixava as corridas do Crash Nitro Kart mudas. Ele conta os
sons ativos e só toca a música da pista quando a conta zera; o tratador de aviso dele (`0x11a80`)
desconta no `DONE` (2) e no status 9 e **ignora o `ABORT` (3)**. Na entrada da corrida ele para as
músicas do menu com `Stop`: sem aviso nenhum, e depois com o `ABORT` que o zeebulator3 usa, elas
nunca saíam da conta, e a corrida inteira — contagem, motor e a trilha de 642 KB — ficava sem um
único `Play`. Com `DONE`, a contagem toca aos 31 s, a trilha entra em repetição e o motor troca de
som com a rotação.

**O WAVE acaba onde o `RIFF` diz.** O Zeebo F.C. Super League monta sons num buffer de rascunho de
500 KB e grava no bloco `data` o tamanho do buffer inteiro, mas no `RIFF` o tamanho do som (14 KB).
Seguir o `data` tocava onze segundos — o efeito e depois lixo de memória, alto, por cima da música.
Quando o `data` passa mais de 4 KB do fim que o `RIFF` declara, vale o `RIFF`; uma diferença de
poucos bytes é arquivo editado com o `RIFF` desatualizado, e não corta nada. Enquanto os sons eram
lidos no `Play`, esse buffer já tinha outro conteúdo e o defeito não aparecia.

Para conferir sem ouvir, o `zeebx sessao <zip> --dump-audio=A.wav` grava a mistura da sessão da
janela, no ritmo do relógio virtual.

Um `Play` sobre um som que ainda toca **não** avisa. Avisar fazia um ciclo nos jogos que tocam de
novo dentro do tratador: o novo `Play` caía sobre o som que acabara de começar, gerava outro aviso,
e o som reiniciava a cada quadro — o Zeebo F.C. Super League saía estourado e picotado.

**Um `Play` sobre a música que já toca em laço não a recomeça.** O gerenciador de som dos Zeebo
Extreme manda tocar a trilha da pista de novo toda vez que um efeito acaba: no Rolima, o efeito
de 10 KB toca, recebe `Stop`, e o `Play` cai no MP3 de 807 KB que já está em repetição infinita.
A voz recomeçava do início, e a música reiniciava a cada turbo e a cada derrapagem. Recusar com
`EBADSTATE` não serve: o jogo entende que a música parou e repete o `Play` a cada quadro. O que
ele espera é o `START`, que o passa a "tocando"; a voz segue de onde está. A regra vale só para o
laço infinito (`MM_PARM_PLAY_REPEAT` zero), porque um efeito tocado de novo por cima de si mesmo
é o que o Zeebo F.C. faz, e ele precisa recomeçar.

**O cache dos sons decodificados tem teto.** Passando de 64, saem os que nenhum `IMedia` usa. Sem
isso, cada conteúdo novo escrito no buffer de rascunho do Zeebo F.C. ficava decodificado para
sempre. Uma voz tocando não perde nada: o PCM dela está num `Arc` que o misturador também segura.

O `GetMediaParm` devolve o volume e o mudo guardados (antes, zero: um jogo que lê o volume e grava
de volta se emudecia), e um `IMedia` liberado para a voz dele no misturador — uma música em
repetição seguia tocando depois de o objeto sumir.

`IMedia::GetState` devolve o objeto a "pronto" quando o som acabou — é o que o jogo consulta para
saber que pode tocar o próximo. **Quem diz que acabou é o relógio virtual, não o misturador**: o
emulador roda mudo sem deixar de contar o tempo, e há som que toca em silêncio porque sabemos
cronometrá-lo sem saber decodificá-lo.

## O misturador

`audio/mod.rs`. Cada `Voice` tem posição, passo, volume, quanto falta e se está pausada. O passo é a
razão entre a taxa do som e a da placa: é assim que um WAVE de 8 kHz sai certo numa saída de
48 kHz. Há teste de cruzamento por zero provando que o reamostrar **preserva a altura** — errar
isso dá um som que toca, parece bem, e está no tom errado.

Vozes terminadas são recolhidas dentro do `fill`, no mesmo lugar em que são consumidas. A saída é
`cpal`.

## Como conferir sem ouvir

`--dump-audio=arquivo.wav` grava a mistura. Foi assim que a implementação foi verificada: pico de
0,700 no Quake, 0,807 no Double Dragon, 1,000 no Peteca, 0,586 no Pac-Mania, com a proporção
esperada entre blocos com som e blocos em silêncio.

Não substitui ouvir, mas separa "não sai som" de "sai som errado".

## O MP3 que não tocava, e o jogo que insistia

O Tekken 2 estava classificado como "lento demais": ele fazia **766 mil `Play` e 766 mil
`GetState` em quatro segundos virtuais**, e seis segundos de jogo não terminavam em cinco minutos
de máquina. Parecia um jogo pesado.

Não era. O rastro mostra o par se repetindo sem nada no meio:

```
IMedia::GetState → 2   (MM_STATE_READY)
IMedia::Play     → 0
IMedia::GetState → 2
IMedia::Play     → 0
```

A música é MP3, `audio/wav.rs` recusa, e o `Play` caía no caminho de "sem som legível", que respondia
ao jogo que **o som já tinha acabado**. O jogo consultava o estado, via "pronto", e mandava tocar
de novo. Para sempre.

O primeiro conserto não foi um decodificador. Foi `audio/mp3.rs`, que lê o cabeçalho do primeiro quadro
e a etiqueta `Xing`/`Info` do codificador e devolve **só a duração** — quando a etiqueta traz a
contagem de quadros o número é exato mesmo com taxa variável; sem ela, sobra a conta do tamanho
pela taxa de bits. Com a duração, o som "toca" em silêncio pelo tempo certo do relógio virtual, o
`GetState` responde "tocando" enquanto isso, e o jogo segue.

Era uma troca declarada, e saía no relatório como hipótese em uso: **o Tekken andava, mudo**. Seis
segundos virtuais saíram de mais de cinco minutos para **9,7 segundos**, e ele desenha a tela de
título. O que sobra nele agora é outro assunto: 1,08 milhão de `DrawPixel` por quadro, que é custo
de despacho de API.

## O MP3 que agora toca

A duração resolvia o travamento e não a música, e a música é de nove jogos — não de um. Então
`audio/mp3.rs` ganhou o `decode`, que devolve o mesmo `Sound` que o RIFF/WAVE produz: o misturador não
sabe de onde o som veio, e reamostragem, volume e repetição funcionam iguais.

A decodificação em si é do **symphonia**, Rust puro, só o MP3 habilitado. É a mesma decisão do
SQLite para o `ISQLMgr` e do `ab_glyph` para o `DrawText`, e vale dizer por quê: o Layer III é
Huffman, requantização, estéreo conjunto, IMDCT e banco de síntese polifásico, e escrever isso
sem uma referência para comparar a saída dá o pior resultado possível — som que toca, parece bem e
está errado.

Ordem no `machine/media.rs`: RIFF/WAVE primeiro, porque é o que quase todo som é e é o mais barato de
reconhecer; MP3 depois; e só então a recusa, que continua nomeando o formato. O caminho da duração
ficou como rede de segurança para um MP3 que o decodificador recuse.

A verificação, medida:

- O tom de teste de 440 Hz decodifica com **pico de 0,761 e altura de 440 Hz** — a altura é
  conferida por cruzamento de zero, porque tom errado passaria por qualquer outra verificação. A
  saída bate com a do decodificador de referência em sete casas decimais.
- O **Tekken 2** saiu de silêncio para **71,6% de amostras não nulas** em oito segundos, com pico
  de 0,698. A hipótese "o Tekken fica mudo" saiu do relatório dele.
- Decodificar é feito **uma vez por conteúdo**, na entrega: o resultado entra no cache de sons
  do `machine/media.rs`. Uma trilha de trinta e seis segundos a 22 kHz mono são seis megabytes de `f32`,
  e nenhum dos nossos jogos troca de música com frequência que justifique fluxo.

A lição vale além do áudio: **antes de otimizar um jogo lento, conferir se ele está trabalhando
ou insistindo.** O Heavy Weapon era o mesmo caso, com bitmaps que mediam 0×0.

## O MIDI, que não se decodifica: se sintetiza

`audio/midi.rs`. Onze jogos entregam a trilha como MIDI, e **MIDI não é som, é partitura**: não há
amostra dentro do arquivo, há "toque a nota 69 no instrumento 25 com força 100". Quem virava isso
em som era o sintetizador do firmware do console, com o banco de instrumentos dele — que está na
parte da NAND que ainda não lemos.

Então o que sai daqui é uma **aproximação declarada**, e ela aparece no relatório como hipótese em
uso. Dizer isso na cara importa mais que o de costume, porque neste caso o erro é silencioso: uma
música sintetizada errado *toca*, e soa como se fosse assim mesmo.

Por isso o módulo cobra de si só o que dá para verificar sem ouvir — **a nota certa, na hora
certa, pelo tempo certo**:

- **A partitura, como a especificação manda.** Formato 0 e 1, várias trilhas juntadas por pulso,
  status corrente, `Note On` de força zero valendo como `Note Off`, meta e SysEx pulados pelo
  tamanho declarado. O status corrente não é otimização: uma trilha de notas seguidas é quase toda
  assim, e ignorá-lo não perde um evento, perde a trilha inteira a partir do primeiro.
- **Pulso para segundo**, honrando a divisão do cabeçalho e cada `Set Tempo` do ponto dele para
  frente — recalcular a música toda com o tempo novo põe tudo o que vem depois de um
  *accelerando* no lugar errado sem nada parecer quebrado. A divisão SMPTE também conta: o byte
  alto é o número de quadros em complemento de dois, e negar os dezesseis bits juntos dá 24 onde
  deveria dar 25.
- **A afinação.** A nota 69 sai em 440 Hz, e o teste mede isso por correlação com o tom certo e
  com os dois semitons vizinhos — contar cruzamento de zero depende da forma da onda, correlação
  não. Afinação errada é o defeito que mais facilmente passa por "o sintetizador é assim".

O que é palpite, e é palpite por falta de dado: **o timbre**. A tabela mapeia família do General
MIDI para forma de onda e envoltória, tentando acertar o *comportamento* — piano decai, órgão
sustenta, metal ataca devagar —, porque é isso que faz a melodia ser reconhecível mesmo com o
instrumento errado. Percussão é ruído filtrado, com duas famílias de decaimento separando bombo
de prato.

Dois cuidados que não são detalhe:

- **Quadrada e dente são somadas por harmônicos** até abaixo de Nyquist, não geradas pela forma
  crua. Uma quadrada crua a 22 kHz rebate: os harmônicos acima da metade da taxa voltam como
  frequências que não estão na partitura, e a nota sai acompanhada de um assobio que sobe quando
  ela desce.
- **O pico é normalizado em 0,8.** Somar dezenas de vozes passa de 1,0 com facilidade, e o que
  passa corta — e corte soa exatamente como "o sintetizador é ruim", sem ser.

Medido no Double Dragon: a linha `som recusado (audio/mid)` saiu do relatório, e dez segundos de
jogo saíram de silêncio para **74,7% de amostras não nulas**, com pico de 0,807.

### O MIDI do Double Dragon, medido contra o Zeebulator

O Double Dragon foi a primeira exceção séria a "a aproximação basta". Comparado com o outro
emulador, o som dele é pobre — e a investigação mostrou que a causa é estrutural, não um ajuste.

**O pacote traz MIDI, e ele estava escondido.** Um `grep` por `MThd` no zip não acha nada: o
`sound.ggz` (1.928.097 B) é um cabeçalho de 592 B com **74 pares** `(offset, tamanho)` e 74
membros gzip contíguos. Conferidos os 74, o tamanho declarado bate em todos, e o conteúdo é
**62 `RIFF`/WAVE** (efeitos, mono 16 bits 22.050 Hz) e **12 `MThd`** (músicas). A BGM principal
tem 1.788 notas, 47,47 s, 9 canais e 404 eventos de percussão; os 12 somam 22.729 notas.

**O que a partitura pede** (e é aqui que a aproximação mais dói):

| família | fatia das notas |
|---|---:|
| pianos (0–7) | 28,7% |
| **guitarras (24–31)** | **23,2%** |
| percussão (canal 10) | 20,6% (4.681 notas) |
| metais | 9,2% |

**O Zeebulator vira isso com amostras, não com onda.** Ele carrega um soundfont General MIDI real
(GeneralUser GS, **32.319.396 B**) e toca com o **TinySoundFont** (MIT, `tsf.h`), em mono a
22.050 Hz e com **−16 dB** de folga antes do corte interno do `tsf`. Note On/Off, `Program
Change` e percussão vão para o banco: o canal 10 usa o kit do próprio soundfont (banco 128), não
uma classificação nossa.

**A comparação controlada**, mesma nota e mesma duração, passando o mesmo arquivo pelas duas
implementações (e por uma referência independente, `timidity` + `FluidR3_GM`):

| render | H2..H8 / H1 | centroide | rolloff 85% |
|---|---|---:|---:|
| Zeebulator com soundfont | 1 · .27 · .015 · .072 · .018 | 1383 Hz | 2647 Hz |
| `timidity` + FluidR3 | — | 1638 Hz | 3140 Hz |
| **Zeebx (`midi.rs`)** | 1 · **0** · .012 · **0** · .002 | **1065 Hz** | **1174 Hz** |

Ou seja: o nosso sai **mais escuro e mais pobre em harmônicos** que os dois renders de soundfont,
com afinação certa (441,0 Hz medidos contra 440,0 — erro de +0,23%, sem erro de oitava). O
problema é o **material**, não a altura.

E o defeito mais visível é este, medido programa a programa: **29 (guitarra overdrive), 30
(distorcida), 42 (violoncelo) e 48 (cordas) saem com a mesma onda** — a tabela mapeia família, e
a família `24–31` é "dente com decaimento". O 81 (lead saw) sai **quadrada** onde a partitura pede
dente de serra. O baixo (33) fica **plano em 0,65** até o fim, onde o soundfont decai para 0,27.
Nas guitarras isso é 23,2% das notas do jogo.

Medido na música inteira:

| render | pico | RMS |
|---|---:|---:|
| Zeebulator com soundfont | 25.727 (78,5% FS) | 4.877 |
| Zeebulator sem soundfont (fallback) | 32.767 (**100% FS**, corta) | 8.401 (+4,72 dB) |
| Zeebx (`midi.rs`) | 0,799988 (= 0,8 por construção) | ≈4.782 |

**Uma ressalva que evita uma conclusão errada:** no Zeebulator **upstream o soundfont só está
ligado em `tools/game_probe.cpp`**. O `frontends/standalone/main.cpp` constrói o `MediaHle` sem
ele, então cai no sintetizador tosco — que é *mais escuro* ainda (rolloff 880 Hz). Quem comparar
pelo standalone não está ouvindo soundfont nenhum; o binário com o banco é o `game_probe`.

**Licença: o código do Zeebulator não serve.** Ele é **GPLv3** e o Zeebx é **GPL-2.0-only**; as
duas não se combinam. O que serve é o que está sob licença própria: o **TinySoundFont** (MIT), o
`rustysynth` (MIT, Rust puro) e o próprio banco GeneralUser GS (licença permissiva, embora o texto
admita origem desconhecida de parte das amostras). O `oxisynth` é LGPL-2.1 e fica de fora.

**O custo, medido:** o banco tem **10,3×** o tamanho da árvore inteira do Zeebulator, pede
**+64 MiB de RSS** na carga (o `tsf` converte tudo para `float`) e levaria o `.so` do core de
**15,6 MB para ~48 MB** em seis alvos de CI. O caminho absoluto compilado do Zeebulator não serve
para o core: não é relocalizável, vaza o `$HOME` do build e, faltando o arquivo, a música degrada
em silêncio.

**Dois fatos que mudam o risco da troca:**

1. Trocar o sintetizador **não acusa regressão na linha de base** das 62 ROMs: pico e RMS ficam no
   relatório completo e **não** no resumo, por decisão registrada em `varredura.rs` — conferido,
   `0` dos 62 arquivos em `docs/varredura/` tem a linha `áudio:`.
2. Em compensação, **17 testes** dentro de `src/audio/midi.rs` travam propriedades da síntese
   atual (pico em `0,79..=0,81`, altura por correlação, brilho da percussão). Uma troca de motor
   os reescreve.

O plano, em duas etapas, com a medição no meio:

- **Etapa 1, barata:** corrigir na tabela exatamente o que foi medido — guitarras 24–31
  sustentadas (29 e 30 são 23,2% da partitura), 81 como dente de serra, baixo sem sustentação
  plana, e separar 42 de 48.
- **Etapa 2, fiel:** wavetable GM com licença compatível (`rustysynth`, MIT), com o banco como
  arquivo **opcional ao lado do core** e recuo para o sintetizador atual quando ele faltar — o
  core Libretro não pode ganhar dependência de host, e 32 MB embutidos em cada `.so` não se pagam.

O harness de comparação vive fora do repositório, em `~/zeebx-midi-harness`: `zbfont/` (C++ com o
`tsf.h` e o banco), `zxmidi/` (o nosso `midi.rs` num crate mínimo) e `abtest/` (os WAVs de cada
caso). É ele que mede cada passo em vez de opinar.

## O som que o jogo gera enquanto toca

Os ports de arcade da Data East não entregam um arquivo: eles emulam o chip de som da placa e
produzem as amostras quadro a quadro. A entrega é um `AEEMediaDataEx` com `clsData = MMD_ISOURCE`
(`0x01001012`), `bRaw` ligado, um `ISource` que o próprio jogo implementa e um `AEEMediaWaveSpec`
com o formato — 11025 Hz, mono, 16 bits com sinal, nos que foram medidos.

Enquanto só sabíamos ler memória e arquivo, a entrega era recusada e o log do jogo dizia
`Failed to SetMediaDataEx!!!!`: **nenhum** dos onze ports tinha som. Agora a entrega é
reconhecida, e o `Play` abre uma voz de fluxo no misturador.

**Quem marca o ritmo é o relógio virtual.** A cada volta do laço o emulador calcula quantos
quadros o relógio já deve, com um décimo de segundo de adiantamento para a placa não esvaziar
entre duas voltas, e chama o `ISource::Read` do jogo até juntar isso — no máximo algumas leituras
por volta, porque um `Read` que devolve menos do que foi pedido é o jogo sem amostras prontas. É a
mesma escolha do resto do som: sem placa nenhuma o emulador continua pedindo, e o jogo, que emula
o chip de som dentro do `Read`, anda do mesmo jeito.

A voz de fluxo vive em `audio/mod.rs`, ao lado das vozes de som pronto. Ela reamostra para a taxa
da placa interpolando entre dois quadros, guarda meio segundo no máximo — se o jogo entrega mais
rápido do que a placa consome, o excesso mais antigo sai em vez de o atraso crescer — e, faltando
amostra, segura o último valor em vez de estalar para o zero.

### Um `IAStream` do jogo como fonte

O Aviãozinho, um port do Quake feito por fãs, monta o `ISource` de outro jeito: escreve um
`IAStream` próprio, cujo `Read` devolve o que o mixer do Quake acabou de misturar, e pede ao
`ISourceUtil` (slot 6) que o transforme em `ISource`. Esse slot só aceitava um `IFile` aberto e
recusava o resto; o `SNDDMA_Init` desistia, e o `S_Init` lia `shm->speed` do buffer nulo e
derrubava o jogo aos 24 ms. O `Read` do `IAStream` e o do `ISource` estão no mesmo slot, com os
mesmos argumentos e o mesmo retorno, então o próprio stream é devolvido como fonte, e a voz de
fluxo chama o código do jogo como já fazia com os ports de arcade. O formato vem no
`AEEMediaWaveSpec` de sempre: 22050 Hz, estéreo, 16 bits.

## O que falta

**O banco de instrumentos do console.** Com ele, o MIDI deixa de ser aproximação e passa a soar
como soava — e o caminho para ele é o mesmo das classes que faltam: o leitor de EFS2 (ver
[14](14-z-wheel-e-o-efs2.md) e [15](15-o-que-falta-da-nand.md)).

Enquanto o banco do console não aparece, a referência de fidelidade é a comparação medida da
seção anterior: o wavetable GM com `rustysynth` (MIT) e o banco como arquivo opcional ao lado do
core. A correção da tabela de timbres vem antes, porque é barata e ataca o que mais se ouve.
