#!/usr/bin/env python3
"""Catálogo No-Intro dos títulos de Zeebo a partir de um acervo de `.zip`.

O que ele faz, por arquivo:

1. abre o pacote e localiza os `.mod` e `.mif` de dentro;
2. calcula tamanho, CRC32, MD5 e SHA1 do arquivo que o DAT declara como a ROM daquele título
   (o No-Intro hasheia **um arquivo dentro de `mod/<id>/`**, cujo nome no DAT é `<pasta><arquivo>`);
3. confere o dump contra o DAT — é o que separa "tenho uma cópia" de "tenho o dump verificado";
4. lê o `.mif` para tirar o ClassID do applet e o ícone do título.

O que ele escreve:

- `<saída>/Zeebo - Zeebo.lpl` — playlist do RetroArch, com o nome No-Intro em `label`;
- `<saída>/thumbnails/Mobile - Zeebo/` — as quatro pastas que o RetroArch procura, com o nome exato
  de cada jogo, prontas para receber as capas;
- `<saída>/catalogo.json` — o mesmo em dados, para a UI daqui e para conferência.

Uso:

    python3 ferramentas/catalogo.py --roms /caminho/dos/zips --saida /caminho/de/saida \\
        [--dat arquivo.dat] [--core /caminho/zeebx_libretro.so] [--icones]
"""

import argparse
import hashlib
import json
import pathlib
import re
import struct
import sys
import urllib.request
import zipfile
import zlib

DAT_URL = (
    "https://raw.githubusercontent.com/libretro/libretro-database/master/"
    "metadat/no-intro/Mobile%20-%20Zeebo.dat"
)
DAT_NOME = "Mobile - Zeebo"

ROM = re.compile(
    r'rom \( name "(?P<nome>[^"]+)" size (?P<tamanho>\d+) '
    r"crc (?P<crc>[0-9A-Fa-f]{8}) md5 (?P<md5>[0-9A-Fa-f]{32}) "
    r"sha1 (?P<sha1>[0-9A-Fa-f]{40})"
)


def le_dat(texto):
    """Devolve `{nome do jogo: (arquivo, tamanho, crc, md5, sha1)}`."""
    tabela = {}
    for bloco in texto.split("game (")[1:]:
        nome = re.search(r'name "([^"]+)"', bloco)
        rom = ROM.search(bloco)
        if nome and rom:
            tabela[nome.group(1)] = (
                rom.group("nome"),
                int(rom.group("tamanho")),
                rom.group("crc").upper(),
                rom.group("md5").upper(),
                rom.group("sha1").upper(),
            )
    return tabela


def baixa_dat(destino):
    if not destino.is_file():
        destino.write_bytes(urllib.request.urlopen(DAT_URL, timeout=60).read())
    return destino.read_text(encoding="utf-8", errors="replace")


def hasheia(zf, nome):
    """Tamanho, CRC32, MD5 e SHA1 de um arquivo dentro do pacote, em blocos."""
    crc = 0
    md5 = hashlib.md5()
    sha1 = hashlib.sha1()
    tamanho = 0
    with zf.open(nome) as fonte:
        while True:
            bloco = fonte.read(1 << 20)
            if not bloco:
                break
            tamanho += len(bloco)
            crc = zlib.crc32(bloco, crc)
            md5.update(bloco)
            sha1.update(bloco)
    return tamanho, f"{crc & 0xFFFFFFFF:08X}", md5.hexdigest().upper(), sha1.hexdigest().upper()


def acha_arquivo_do_dat(nomes, rom_do_dat):
    """O arquivo do DAT tem o caminho interno do pacote **sem as barras**.

    `mod274754sound.ggz` é `mod/274754/sound.ggz`; `modnfsresourcestracksworld_3401.viv` é
    `mod/nfs/resources/tracks/world_3401.viv`. A pasta do módulo nem sempre é numérica, então a
    comparação é feita sobre o caminho inteiro sem separador — não por prefixo.
    """
    alvo = rom_do_dat.replace("\\", "/").replace("/", "").lower()
    for nome in nomes:
        sem_barra = nome.replace("\\", "/").replace("/", "")
        if sem_barra.lower() == alvo:
            return nome
    for nome in nomes:
        sem_barra = nome.replace("\\", "/").replace("/", "")
        if sem_barra.lower().endswith(alvo):
            return nome
    return None


def icone_do_mif(dados):
    """A primeira imagem declarada no `.mif`, quando existe.

    O formato está em `src/loader/miffile.rs`: cada seção de imagem começa com o tamanho do
    próprio cabeçalho em `u16`, seguido do MIME terminado em zero, e o arquivo logo depois.
    """
    if len(dados) < 0x18:
        return None
    if struct.unpack_from("<H", dados, 0)[0] != 0x0011:
        return None
    tabela, secoes = struct.unpack_from("<II", dados, 0x10)
    if secoes == 0 or secoes > 4096:
        return None
    limites = []
    for i in range(secoes + 1):
        if tabela + i * 4 + 4 > len(dados):
            return None
        limites.append(struct.unpack_from("<I", dados, tabela + i * 4)[0])
    if limites[-1] > len(dados):
        return None
    for inicio, fim in zip(limites, limites[1:]):
        secao = dados[inicio:fim]
        if len(secao) < 4:
            continue
        cabecalho = struct.unpack_from("<H", secao, 0)[0]
        if cabecalho < 4 or cabecalho > len(secao):
            continue
        mime = secao[2:cabecalho].split(b"\0")[0]
        if mime.startswith(b"image/"):
            return mime.decode("ascii", "replace"), secao[cabecalho:]
    return None


def analisa(zip_path, tabela):
    nome_no_intro = zip_path.stem
    esperado = tabela.get(nome_no_intro)
    with zipfile.ZipFile(zip_path) as zf:
        nomes = [n for n in zf.namelist() if not n.endswith("/")]
        modulos = [n for n in nomes if n.lower().endswith(".mod")]
        manif = [n for n in nomes if n.lower().endswith(".mif")]
        ficha = {
            "zip": zip_path.name,
            "nome_no_intro": nome_no_intro,
            "no_dat": esperado is not None,
            "arquivos": len(nomes),
            "modulos": modulos,
            "manifestos": manif,
            "verificado": None,
            "crc_zip": None,
            "sha1_zip": None,
            "icone": None,
        }
        tamanho, crc, md5, sha1 = hasheia(zf, nomes and nomes[0]) if nomes else (0, "", "", "")
        ficha["crc_zip"] = f"{zlib.crc32(zip_path.read_bytes()) & 0xFFFFFFFF:08X}"
        ficha["sha1_zip"] = hashlib.sha1(zip_path.read_bytes()).hexdigest().upper()
        if esperado:
            arquivo, tam_esp, crc_esp, md5_esp, sha1_esp = esperado
            alvo = acha_arquivo_do_dat(nomes, arquivo)
            if alvo:
                tam, crc_lido, md5_lido, sha1_lido = hasheia(zf, alvo)
                ficha["verificado"] = (
                    tam == tam_esp
                    and crc_lido == crc_esp
                    and md5_lido == md5_esp
                    and sha1_lido == sha1_esp
                )
                ficha["arquivo_do_dat"] = alvo
                ficha["hash"] = {
                    "tamanho": tam,
                    "crc32": crc_lido,
                    "md5": md5_lido,
                    "sha1": sha1_lido,
                }
            else:
                ficha["verificado"] = False
                ficha["erro"] = f"o arquivo do DAT ({arquivo}) não está no pacote"
        if manif:
            dados = zf.read(manif[0])
            imagem = icone_do_mif(dados)
            if imagem:
                mime, bytes_imagem = imagem
                ficha["icone"] = {"mime": mime, "bytes": bytes_imagem, "de": manif[0]}
        del tamanho, crc, md5, sha1
        return ficha


def escreve_playlist(saida, fichas, core):
    linhas = []
    for ficha in fichas:
        entrada = {
            "path": str(ficha["caminho"]),
            "label": ficha["nome_no_intro"],
            "core_path": core or "DETECT",
            "core_name": "Zeebx",
            "crc32": ficha["crc_zip"],
            "db_name": DAT_NOME,
        }
        linhas.append(json.dumps(entrada, ensure_ascii=False))
    destino = saida / f"{DAT_NOME}.lpl"
    destino.write_text("\n".join(linhas) + "\n", encoding="utf-8")
    return destino


def escreve_capas(saida, fichas, com_icone):
    raiz = saida / "thumbnails" / DAT_NOME
    pastas = ["Named_Boxarts", "Named_Snaps", "Named_Titles", "Named_Logos"]
    escritos = 0
    for pasta in pastas:
        (raiz / pasta).mkdir(parents=True, exist_ok=True)
    if com_icone:
        # O ícone do `.mif` não é capa: serve de `Named_Titles` provisório, para a lista não ficar
        # sem imagem nenhuma enquanto a capa de verdade não é publicada.
        destino = raiz / "Named_Titles"
        for ficha in fichas:
            icone = ficha.get("icone")
            if not icone:
                continue
            extensao = {"image/png": "png", "image/jpeg": "jpg", "image/bmp": "bmp"}.get(
                icone["mime"], "bin"
            )
            (destino / f"{ficha['nome_no_intro']}.{extensao}").write_bytes(icone["bytes"])
            escritos += 1
    return raiz, escritos


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--roms", required=True, type=pathlib.Path)
    ap.add_argument("--saida", required=True, type=pathlib.Path)
    ap.add_argument("--dat", type=pathlib.Path, default=None)
    ap.add_argument("--core", default=None, help="caminho do zeebx_libretro.so para a playlist")
    ap.add_argument("--icones", action="store_true", help="grava o ícone do .mif como título")
    args = ap.parse_args()

    args.saida.mkdir(parents=True, exist_ok=True)
    caminho_dat = args.dat or args.saida / f"{DAT_NOME}.dat"
    tabela = le_dat(baixa_dat(caminho_dat))
    print(f"DAT: {len(tabela)} títulos conhecidos")

    zips = sorted(args.roms.glob("*.zip"))
    fichas = []
    for zip_path in zips:
        ficha = analisa(zip_path, tabela)
        ficha["caminho"] = zip_path
        fichas.append(ficha)

    verificados = [f for f in fichas if f["verificado"]]
    fora = [f for f in fichas if not f["no_dat"]]
    divergentes = [f for f in fichas if f["no_dat"] and f["verificado"] is False]
    print(f"pacotes: {len(fichas)}")
    print(f"verificados pelo No-Intro: {len(verificados)}")
    print(f"fora do DAT: {len(fora)}" + (": " + ", ".join(f["nome_no_intro"] for f in fora) if fora else ""))
    print(f"divergentes: {len(divergentes)}")
    for ficha in divergentes:
        print(f"  {ficha['nome_no_intro']}: {ficha.get('erro', 'hash diferente')}")

    playlist = escreve_playlist(args.saida, fichas, args.core)
    raiz, escritos = escreve_capas(args.saida, fichas, args.icones)
    print(f"playlist: {playlist}")
    print(f"capas: {raiz}" + (f" ({escritos} ícones gravados)" if args.icones else ""))

    catalogo = args.saida / "catalogo.json"
    catalogo.write_text(
        json.dumps(
            [
                {
                    chave: (str(valor) if isinstance(valor, pathlib.Path) else valor)
                    for chave, valor in ficha.items()
                    if chave != "icone"
                }
                for ficha in fichas
            ],
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )
    print(f"catálogo: {catalogo}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
