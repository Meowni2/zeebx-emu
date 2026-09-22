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
