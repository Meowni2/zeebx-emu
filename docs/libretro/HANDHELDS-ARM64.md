# Core Libretro em handhelds Linux AArch64

O core `zeebx_libretro.so` é construído para **Linux AArch64**. O mesmo arquivo atende os
handhelds de 64 bits desta família:

- R36S/R35S/RGB20S com ArkOS ou AeolusUX/ArkOS-R3XS;
- R36S com dArkOS ou dArkOSen-R36S;
- RG40XX-H com MustardOS (muOS) Pixie `2502.0` ou mais novo.

Referências dos sistemas usados para definir o alvo:

- <https://github.com/christianhaitian/arkos>
- <https://github.com/AeolusUX/ArkOS-R3XS>
- <https://github.com/christianhaitian/dArkOS>
- <https://github.com/djparentx/dArkOSen-R36S>
- <https://muos.dev/release/archive/2502_0>

## O que é compatível

O pacote `zeebx_libretro_linux-aarch64.zip` contém dois arquivos que devem permanecer juntos:

```text
zeebx_libretro.so
zeebx_libretro.info
```

O core não depende de X11, Wayland, EGL, ALSA, udev ou GTK. Vídeo, áudio e controles chegam pela
ABI Libretro. Se o RetroArch não oferecer contexto OpenGL ES 3, o core aceita a recusa e usa o
rasterizador software; ele não cai por falta de uma GPU compatível.

Em Linux AArch64 o pedido de renderização em hardware é `RETRO_HW_CONTEXT_OPENGLES_VERSION`,
com versão 3.2. O rasterizador seleciona `#version 300 es`, pois usa somente o subconjunto GLES 3
necessário ao desenho. Se o driver aberto expuser apenas GLES 3.1, o frontend recusa o pedido e o
core usa software. Em desktop continua pedindo OpenGL Core 3.3, pois o contexto é diferente. Os
dois caminhos usam o mesmo FBO do frontend.

## Instalação

1. Baixe o artefato **Linux AArch64** da release/Actions.
2. Extraia os dois arquivos.
3. Copie-os para o diretório de cores que o RetroArch mostra em `Settings > Directory > Cores`.
   Em imagens que mantêm uma pasta de cores no cartão, use essa pasta; não renomeie o `.so` nem
   separe o `.info` dele.
4. Abra o RetroArch uma vez, carregue `zeebx_libretro` e só então faça o scan da pasta de ROMs.
5. Use `.zip`, `.7z` ou `.mod`. O arquivo `.info` declara `mod|zip|7z` e `block_extract=true`.

O cartão pode ter bibliotecas ARM de 32 bits para PortMaster. Isso não muda este pacote: o core
precisa de RetroArch AArch64 e não carrega em um RetroArch `armhf`/32-bit.


## muOS (RG40XX-H): o que o muOS lê, e de onde

Medido na imagem muOS do cartão do aparelho (kernel de 64 bits, RetroArch AArch64). O muOS **não**
carrega core de uma pasta de cores do RetroArch: o lançador chama

```sh
nice --20 retroarch -v -f $RA_ARGS -L "$MUOS_SHARE_DIR/core/$CORE" "$FILE"
```

ou seja, `/opt/muos/share/core/<core>`. Quem escolhe o core é o arquivo de associação
(`info/assign/<Sistema>/<core>.ini`), e o `.info` do core fica em
`/opt/muos/share/emulator/retroarch/info/`.

O sistema de arquivos de conteúdo é um **unionfs** (`/opt/muos/script/mount/union.sh`):
`/mnt/union/ROMS` junta, nesta ordem, `USB/ROMS`, `SDCARD/ROMS` e `ROMS` do cartão do sistema. Com
dois cartões, o de ROMs é o `SDCARD`, e as ROMs vão em **`/ROMS/Zeebo`** nele — não em
`ROMS/ROMS` no cartão do sistema.

O que o muOS precisa, e onde:

| Peça | Caminho | O que é |
|---|---|---|
| Core | `/opt/muos/share/core/zeebx_libretro.so` | o `.so` AArch64 |
| Info | `/opt/muos/share/emulator/retroarch/info/zeebx_libretro.info` | metadados que o RetroArch lê |
| Sistema | `/opt/muos/share/info/assign/Zeebo/{global.ini,zeebx.ini}` | nome do sistema, core padrão e o comando de lançamento |
| Associação | `/opt/muos/share/info/assign/assign.json` | `friendly` → pasta do sistema |
| Nome da pasta | `MUOS/info/name/folder.json` | nome da pasta → nome exibido |
| Capas | `MUOS/info/catalogue/Zeebo/{box,grid}/<jogo>.png` | arte, com o nome do `.zip` sem extensão |
| Jogos | `ROMS/Zeebo/*.zip` no cartão de ROMs | aparecem em `/mnt/union/ROMS/Zeebo` |

Duas armadilhas medidas, as duas por permissão:

1. `assign.json` é `root:root 0644`, mas o **diretório** é do usuário. Como remover um arquivo
   depende da permissão do diretório, dá para apagá-lo e reescrevê-lo com a chave nova sem `sudo`.
   Sem a chave `zeebo`, o sistema não aparece — o `assign.sh` só roda no cartão (tarefa *Refresh
   Automatic Core Assign*) e no reset, não a cada boot.
2. `emulator/retroarch/{database,playlists,thumbnails}` são `root:root 0750`: não dá para escrever
   neles pelo cartão montado. Não são necessários para jogar pelo muOS; só servem ao menu do
   próprio RetroArch.

**Compatibilidade de biblioteca, medida antes de copiar:** o core exige no máximo `GLIBC_2.34` e
`GLIBCXX_3.4.31`; o aparelho traz glibc até `GLIBC_2.38` e `libstdc++.so.6.0.32` com `GLIBCXX`
até `3.4.32`. Nada faltando — e é essa conferência que evita o sintoma clássico de "o core não
aparece" por `dlopen` recusado.

### Instalar

Com a partição ROOTFS do cartão do sistema montada, é **um comando**:

```bash
python3 ferramentas/instala_core.py --muos /media/$USER/ROOTFS --banco GeneralUser-GS.sf2
```

Ele faz o backup do core anterior com data no nome antes de sobrescrever, copia o `.so` e o
`.info`, cria as associações do sistema, acrescenta a chave nos dois JSON e confere o `sha256` no
fim. Sem um cartão montado o comando **recusa e diz qual partição montar** — foi medido, e é
melhor que escrever no lugar errado.

O que ele faz por baixo, para quem quiser conferir ou fizer à mão:

1. Copie o `.so` para `/opt/muos/share/core/` e o `.info` para
   `/opt/muos/share/emulator/retroarch/info/`.
2. Crie `/opt/muos/share/info/assign/Zeebo/` com `global.ini` (`name`, `default=zeebx`,
   `catalogue`, `lookup=0`, `governor=performance` e um `[friendly] zeebo`) e `zeebx.ini`
   (`name=Zeebx`, `core=zeebx_libretro.so`, `exec=/opt/muos/script/launch/lr-general.sh`).
3. Acrescente `"zeebo": "Zeebo"` em `info/assign/assign.json` e em `MUOS/info/name/folder.json`.
4. Coloque os jogos em `ROMS/Zeebo/` **no cartão de ROMs**.
5. Opcional: capas em `MUOS/info/catalogue/Zeebo/box/` e `grid/`, com o nome do arquivo igual ao
   do `.zip` sem extensão.

A **Z-Wheel precisa dos jogos na mesma pasta**: ela enumera os applets instalados ao lado do
conteúdo, então abrir a roda de dentro de `ROMS/Zeebo` mostra os 63 títulos.


## Banco de amostras do MIDI (opcional)

Onze jogos tocam a trilha como MIDI, e a partitura não tem som dentro — quem vira som é o
sintetizador. Sem banco, o Zeebx usa a tabela de timbres dele; com banco, toca as amostras de
verdade. Medido na música do Double Dragon: com o banco o centroide fica a 2,5% do de referência
(o Zeebulator com o GeneralUser GS), contra 38% da tabela.

Instalar é copiar **um arquivo**:

```text
<raiz do aparelho ou perfil>/soundfonts/*.sf2
```

No muOS a raiz do aparelho é o diretório de sistema do core; no desktop, `~/.config/zeebx/aparelho`.
Para experimentar sem instalar nada, `ZEEBX_SOUNDFONT=/caminho/para/Banco.sf2`.

Sem o arquivo o core funciona igual — muda o som, não a carga. Medido: o `.so` do core fica em
15,2 MB com o sintetizador de banco compilado dentro, e o `ldd` continua só com libstdc++, libgcc,
libm e libc.

## Verificação sem adivinhar

O workflow `libretro` confere no próprio artefato:

- `file`/`readelf -h`: ELF64 `AArch64`;
- `ldd`: nenhuma dependência de janela, áudio ou entrada do host;
- `readelf -d`: nenhum `RPATH`/`RUNPATH` apontando para o runner;
- `verifica_core.py`: `dlopen`, 25 símbolos da ABI e `retro_api_version == 1`.

Se o RetroArch não listar o core, confira primeiro se o RetroArch é 64-bit e se `zeebx_libretro.info`
está na mesma pasta. Se o vídeo não abrir em hardware, selecione o driver de vídeo do sistema ou
deixe o core cair para software; isso é uma degradação suportada, não uma falha de carga.

## Evidência dos chips e firmwares

- A página oficial da Rockchip para o RK3326 lista Cortex-A35, Mali-G31MP2 e **OpenGL ES 3.2**:
  <https://www.rock-chips.com/a/cn/product/RK33xilie/2018/0514/901.html>
- O brief oficial do H700 lista Cortex-A53 64-bit, G31 e **OpenGL ES 3.2/Vulkan 1.1**:
  <https://www.allwinnertech.com/uploads/pdf/2021070513595227.pdf>
- A página oficial da Arm confirma que Mali-G31 suporta OpenGL ES 3.2:
  <https://www.arm.com/products/silicon-ip-multimedia/gpu/mali-g31>
- O ArkOS versionado mantém o pacote arm64 `libmali-rk-bifrost-g31-rxp0-wayland-gbm`; o binário
  contém a string `OpenGL ES 3.2` e o controle do pacote identifica PX30/RK3326:
  <https://github.com/christianhaitian/arkos/tree/main/12152020>
- O dArkOSen-R36S inclui um ICD Vulkan que aponta para `libmali.so`:
  <https://github.com/djparentx/dArkOSen-R36S/blob/main/usr/share/vulkan/icd.d/mali_icd.json>
- O repositório interno do muOS identifica o dispositivo `rg40xx-h` e configura o RetroArch com
  `video_driver = "gl"`:
  <https://github.com/MustardOS/internal/blob/main/device/rg40xx-h/control/retroarch.resolution.cfg>
- A documentação do Mesa registra G31 como GLES 3.1 no caminho Panfrost atual:
  <https://docs.mesa3d.org/drivers/panfrost.html>

Portanto **3.2 é a capacidade anunciada do hardware e do libMali**, mas não é seguro afirmar que
cada imagem com Panfrost expõe 3.2. O core solicita 3.2 para aproveitar o caminho de hardware e
continua funcional em software quando o driver só oferece 3.1.


## Procedimento atual de teste — R36S com ArkOS/AeolusUX/dArkOSen

O R36S original tem dois RetroArchs:

```text
64-bit: /opt/retroarch/bin/retroarch
32-bit: /opt/retroarch/bin/retroarch32
```

Use **somente o RetroArch 64-bit** para o Zeebx. O core é `aarch64` e não carrega no RetroArch
`armhf`. O `retroarch32` fica para cores antigos do PortMaster e outros emuladores 32-bit.

### Caminhos

Com a partição `EASYROMS` montada pelo sistema em `/roms`:

```text
Core:       /home/ark/.config/retroarch/cores/zeebx_libretro.so
Info:       /home/ark/.config/retroarch/cores/zeebx_libretro.info
ROMs:       /roms/zeebo/*.zip
BIOS/cache: /roms/bios/zeebx/
SoundFont:  /roms/bios/zeebx/aparelho/soundfonts/GeneralUser-GS.sf2
```

Na montagem do cartão no Linux, `EASYROMS` é a raiz que aparece como `/roms` no aparelho. Não
crie `EASYROMS/roms/zeebo`; o caminho correto é diretamente `EASYROMS/zeebo/`.

### Instalação manual

1. Monte as partições `root`, `EASYROMS` e `BOOT`.
2. Copie o par `.so` + `.info` para `home/ark/.config/retroarch/cores/`.
3. Coloque os jogos diretamente em `EASYROMS/zeebo/`.
4. Coloque o banco `.sf2` em `EASYROMS/bios/zeebx/aparelho/soundfonts/`.
5. Se o sistema não listar o Zeebo, verifique `etc/emulationstation/es_systems.cfg`.

A entrada do sistema deve apontar para `/roms/zeebo/` e para o core 64-bit:

```xml
<name>zeebo</name>
<path>/roms/zeebo/</path>
<extension>.mod .MOD .zip .ZIP .7z .7Z</extension>
<command>sudo perfmax %GOVERNOR% %ROM%; nice -n -19 /usr/local/bin/retroarch -L /home/ark/.config/retroarch/cores/zeebx_libretro.so %ROM%; sudo perfnorm</command>
```

O EmulationStation também pode usar o override do usuário:

```text
/home/ark/.emulationstation/es_systems.cfg
```

Se esse arquivo existir, ele pode ter precedência sobre `/etc/emulationstation/es_systems.cfg`.
Depois de qualquer edição, valide XML e reinicie o EmulationStation. Não deixe `&` literal dentro
de texto XML: use `&amp;` (`2&gt;&amp;1` para o redirecionamento de shell).

### Opções recomendadas no R36S

No RetroArch 64-bit, em **Quick Menu → Core Options**:

```text
zeebx_perfil = "portatil"
zeebx_limite_fps = "60"
zeebx_frameskip = "automatico"
```

O perfil Portátil força tabela de timbres, 22.050 Hz, 48 vozes, cache de 8 MiB e sem
supersampling. Volume e névoa continuam independentes. Se o jogo ficar instável ou perder
imagem, use `zeebx_frameskip = "desligado"`; jogos que usam `glReadPixels` desligam frameskip
sozinhos depois da primeira leitura.

### Logs e diagnóstico

Ative temporariamente no arquivo:

```text
/home/ark/.config/retroarch/retroarch.cfg
```

```ini
log_verbosity = "true"
log_to_file = "true"
log_to_file_timestamp = "true"
```

Os logs ficam em:

```text
/home/ark/.config/retroarch/logs/
```

Mensagens importantes:

```text
Zeebx: sintetizador MIDI selecionado: Auto
Zeebx: SoundFont ... carregado
Zeebx: sem banco de amostras do MIDI
Zeebx: desenhando na placa
Zeebx: frameskip automático pediu o aviso de buffer de áudio ao frontend
```

Se aparecer `sem banco de amostras`, o caminho do `.sf2` está errado ou o banco não foi copiado.
Se o core não carregar, verifique `file zeebx_libretro.so`: precisa dizer `ELF 64-bit ... ARM
aarch64`. Um core Linux AArch64 comum pode exigir `GLIBC_2.34`; ArkOS antigo com glibc 2.30
precisa do artefato compatível construído com `cargo zigbuild` e o shim `r36s_compat.c`.

## Procedimento atual de teste — RG40XX-H com muOS Loose Goose

Use o **RetroArch AArch64**. Não use `retroarch32`.

### Caminhos muOS

No cartão do sistema (`ROOTFS`):

```text
Core:      /opt/muos/share/core/zeebx_libretro.so
Info:      /opt/muos/share/emulator/retroarch/info/zeebx_libretro.info
Config:    /opt/muos/share/info/config/Zeebx/zeebo.cfg
SoundFont: /opt/muos/share/emulator/retroarch/system/zeebx/aparelho/soundfonts/GeneralUser-GS.sf2
```

No cartão de ROMs:

```text
ROMs: /ROMS/Zeebo/*.zip
```

O arquivo de associação esperado é:

```text
/opt/muos/share/info/assign/Zeebo/zeebx.ini
```

```ini
[zeebx]
name=Zeebx
core=zeebx_libretro.so

[launch]
prep=
exec=/opt/muos/script/launch/lr-general.sh
done=
```

Não coloque ROMs em `ROMS/ROMS/Zeebo`. Em configurações com dois cartões, o diretório correto é
`ROMS/Zeebo` no cartão que o muOS expõe como a partição de ROMs.

### Opções recomendadas no RG40XX-H

Para comparar qualidade e desempenho:

```text
zeebx_perfil = "padrao"
zeebx_midi_backend = "auto"
zeebx_soundfont_taxa = "44100"
zeebx_limite_fps = "60"
zeebx_frameskip = "automatico"
```

Para priorizar fluidez no H700:

```text
zeebx_perfil = "portatil"
zeebx_limite_fps = "60"
zeebx_frameskip = "automatico"
```

Depois de alterar uma opção, observe o texto do rótulo: volume, névoa, frameskip e melhorias
valem na hora; MIDI e rasterizador exigem recarregar o conteúdo; taxa/vozes/cache valem a partir
da próxima música ou descarte.

### Logs muOS/RetroArch

No muOS, ative temporariamente `log_verbosity`, `log_to_file` e `log_to_file_timestamp` no
`retroarch.cfg` do RetroArch 64-bit. Os logs normalmente ficam em:

```text
/home/ark/.config/retroarch/logs/
```

O `SYSTEM_DIRECTORY` que o core deve relatar é normalmente `/roms/bios`; portanto a mensagem
esperada para o SoundFont é:

```text
/roms/bios/zeebx/aparelho/soundfonts/GeneralUser-GS.sf2
```

### Rollback

Sempre faça backup antes de substituir o core:

```text
zeebx_libretro.so.before-AAAAmmdd-HHMMSS
```

Se o novo core não carregar, restaure o `.so` anterior e mantenha o `.info` correspondente. O
problema mais comum em imagens antigas é `GLIBC_2.34 not found`; nesse caso não adianta trocar
configuração do RetroArch — é necessário um core compilado com glibc mínima compatível.
