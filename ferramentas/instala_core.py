#!/usr/bin/env python3
"""Instala o core Libretro no RetroArch: o `.so` **e** o `.info`, sempre juntos.

Os dois andam em par: o RetroArch lê o campo `database` do `.info` para associar o banco No-Intro
ao core, e um `.info` velho ao lado de um core novo faz o scan pela interface marcar `??` em tudo.
Foi exatamente o que aconteceu aqui uma vez, por instalar os dois em momentos diferentes.

    python3 ferramentas/instala_core.py                 # copia para o RetroArch do usuário
    python3 ferramentas/instala_core.py --destino DIR    # outra pasta de cores
    python3 ferramentas/instala_core.py --release         # usa target/release
"""

import argparse
import pathlib
import shutil
import sys

RAIZ = pathlib.Path(__file__).resolve().parent.parent
ORIGEM_INFO = RAIZ / "frontends" / "libretro" / "zeebx_libretro.info"
NOMES = ("libzeebx_libretro.so", "zeebx_libretro.so")


def acha_perfil():
    """A pasta de configuração do RetroArch, no lugar em que ele a procura."""
    import os

    if xdg := os.environ.get("XDG_CONFIG_HOME"):
        return pathlib.Path(xdg) / "retroarch"
    return pathlib.Path.home() / ".config" / "retroarch"


def acha_so(release):
    perfis = ["release", "debug"] if release else ["debug", "release"]
    for perfil in perfis:
        for nome in NOMES:
            caminho = RAIZ / "target" / perfil / nome
            if caminho.is_file():
                return caminho
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--destino", type=pathlib.Path, default=None)
    ap.add_argument("--release", action="store_true", help="prefere target/release")
    args = ap.parse_args()

    origem_so = acha_so(args.release)
    if origem_so is None:
        print(
            "não achou o core compilado.\n"
            "  CARGO_PROFILE_DEV_DEBUG=0 cargo build -p zeebx-libretro",
            file=sys.stderr,
        )
        return 1
    if not ORIGEM_INFO.is_file():
        print(f"não achou {ORIGEM_INFO}", file=sys.stderr)
        return 1

    destino = args.destino or (acha_perfil() / "cores")
    destino.mkdir(parents=True, exist_ok=True)
    alvo_so = destino / "zeebx_libretro.so"
    alvo_info = destino / "zeebx_libretro.info"

    # Avisa quando o `.info` que está lá é diferente do que vai entrar: é o caso que quebrou o
    # scan uma vez, e um aviso no console custa menos que descobrir pelo sintoma.
    if alvo_info.is_file() and alvo_info.read_bytes() != ORIGEM_INFO.read_bytes():
        print(f"aviso: o .info em {alvo_info} era diferente e será substituído")

    shutil.copy(origem_so, alvo_so)
    shutil.copy(ORIGEM_INFO, alvo_info)
    print(f"core:  {alvo_so}  ({origem_so.stat().st_size} bytes, {origem_so.parent.name})")
    print(f"info:  {alvo_info}")
    print("\nabra o RetroArch e escolha o core Zeebx; o banco No-Intro sai do campo `database` daqui")
    return 0


if __name__ == "__main__":
    sys.exit(main())
