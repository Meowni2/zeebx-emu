# Proposta de inclusão no banco de DATs do libretro

Cinco títulos de Zeebo do acervo **não estão** no `Mobile - Zeebo.dat` que o RetroArch usa para
nomear as ROMs e para achar as capas. Sem entrada no DAT, esses títulos aparecem com o nome do
arquivo e ficam sem capa, mesmo com o dump em mãos.

## Os cinco

| Título | Por que está fora |
|---|---|
| Bad Dudes vs. DragonNinja | jogo oficial, lançado no console e ausente do banco |
| Caveman Ninja | idem |
| Dark Seal | idem |
| Karnov's Revenge | idem |
| Kingdom Hearts V CAST | homebrew, capítulo 1 + Agrabah |

## O que este diretório tem

- `Mobile - Zeebo (proposta).dat` — o bloco pronto para acrescentar ao DAT do banco, no **formato
  dele**: cabeçalho `clrmamepro` com os mesmos quatro campos do arquivo oficial (`name`,
  `description`, `version` entre aspas no formato `AAAA.MM.DD`, `homepage`), e um `game` por título,
  com `region "Brazil"` e os hashes de cada arquivo.
- `proposta.txt` — o mesmo, para leitura humana: pacote, tamanho, CRC32 e SHA1 do `.zip`, e todos os
  arquivos do módulo com tamanho, CRC32, MD5 e SHA1.

Os nomes dos arquivos seguem a convenção do banco: o No-Intro grava `mod274259font.bar` para o que
está em `mod/274259/font.bar`. Cada `game` traz **vários** `rom` de propósito: qual deles é o dump é
decisão de quem mantém o banco, e a proposta leva todos os hashes em vez de apostar num.

## Onde enviar

O banco é mantido em `github.com/robloach/libretro-dats`. A contribuição é uma issue ou um PR
acrescentando os blocos ao `Mobile - Zeebo.dat`.

## As capas

São 58, em PNG, no formato que o repositório de thumbnails aceita (ele **recusa** JPEG). Ficam em
`thumbnails/Mobile - Zeebo/Named_Boxarts/`, com o nome exato de cada título no DAT — o mesmo nome
que a playlist usa. O repositório é `github.com/libretro-thumbnails/Mobile_-_Zeebo`.

As capas **não** ficam versionadas aqui: são regeneráveis e pesam alguns megabytes. Para refazer,
`ferramentas/catalogo.py --roms <zips> --saida <destino> --png`.

## O que falta

Só o envio, que precisa das contas de quem mantém o fork.
