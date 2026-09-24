#!/bin/sh
# Mede o Zeebx no aparelho e junta tudo num pacote de log.
#
# Uso:   sh testa_zeebx.sh [segundos_por_rodada] [pasta_das_roms]
#        sh testa_zeebx.sh 20 /roms/zeebo
#
# Escreve tudo ao lado do script, numa pasta `logs-zeebx-<data>`. Cada rodada roda o core por um
# numero fixo de quadros e mede o tempo de parede: quadros por segundo reais do aparelho. O log de
# cada rodada fica junto, com as linhas do nucleo.

set -u

SEG=${1:-20}
ROMDIR=${2:-}
AQUI=$(cd "$(dirname "$0")" && pwd)
SAIDA="$AQUI/logs-zeebx-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$SAIDA"
RESUMO="$SAIDA/resumo.txt"
: > "$RESUMO"

fala() { printf '%s\n' "$*" | tee -a "$RESUMO"; }

# ------------------------------------------------------------------ achar o retroarch e o core

RA=$(command -v retroarch || true)
[ -n "$RA" ] || RA=$(ls /usr/local/bin/retroarch /usr/bin/retroarch 2>/dev/null | head -1)
[ -n "$RA" ] || { fala "RETROARCH: nao achei o executavel"; exit 1; }

CFG=""
for c in "$HOME/.config/retroarch" "$HOME/RetroArch" "/home/ark/.config/retroarch"; do
    [ -d "$c" ] && CFG="$c" && break
done

CORE=""
for c in "$AQUI/zeebx_libretro.so" "$CFG/cores/zeebx_libretro.so" \
         "/opt/muos/share/core/zeebx_libretro.so" /usr/lib/libretro/zeebx_libretro.so; do
    [ -f "$c" ] && CORE="$c" && break
done
[ -n "$CORE" ] || { fala "CORE: nao achei o zeebx_libretro.so"; exit 1; }

# ------------------------------------------------------------------ achar as roms

if [ -z "$ROMDIR" ]; then
    for d in /roms/zeebo /roms/ROMS/Zeebo /mnt/mmc/ROMS/Zeebo "$HOME/roms/zeebo"; do
        [ -d "$d" ] && ROMDIR="$d" && break
    done
fi
[ -n "$ROMDIR" ] || { fala "ROMS: nao achei a pasta; passe como segundo argumento"; exit 1; }

# ------------------------------------------------------------------ o tempo, com ou sem nanos

if [ "$(date +%N 2>/dev/null | wc -c)" -ge 10 ]; then
    agora_ms() { date +%s%N | cut -c1-13; }
    UNIDADE=ms
else
    agora_ms() { echo $(( $(date +%s) * 1000 )); }
    UNIDADE=s
fi

# ------------------------------------------------------------------ as opcoes do core, por rodada

OPC="$CFG/config/Zeebx/Zeebx.opt"
OPC_ANTES="$SAIDA/Zeebx.opt.antes"
salva_opcoes() {
    if [ -f "$OPC" ]; then cp "$OPC" "$OPC_ANTES"; else : > "$OPC_ANTES"; fi
    mkdir -p "$(dirname "$OPC")"
}
devolve_opcoes() {
    if [ -s "$OPC_ANTES" ]; then cp "$OPC_ANTES" "$OPC"; else rm -f "$OPC"; fi
}
escreve() { printf '%s = "%s"\n' "$1" "$2" >> "$OPC"; }

opcoes() {
    : > "$OPC"
    escreve zeebx_perfil padrao
    escreve zeebx_rasterizador auto
    escreve zeebx_resolucao_interna 1
    escreve zeebx_descarte_de_tiles desligado
    escreve zeebx_frameskip desligado
    escreve zeebx_limite_fps desligado
    escreve zeebx_log aviso
    case "$1" in
        portatil)  escreve zeebx_perfil portatil ;;
        tiles)     escreve zeebx_descarte_de_tiles ligado ;;
        software)  escreve zeebx_rasterizador software ;;
        depuracao) escreve zeebx_log depuracao ;;
    esac
}

# ------------------------------------------------------------------ uma rodada

roda() {
    rotulo=$1; jogo=$2; config=$3
    rom=$(ls "$ROMDIR/$jogo"*.zip 2>/dev/null | head -1)
    if [ -n "$rom" ]; then
        opcoes "$config"
        quadros=$((SEG * 60))
        t0=$(agora_ms)
        "$RA" -L "$CORE" "$rom" --max-frames="$quadros" --verbose > "$SAIDA/$rotulo.log" 2>&1
        t1=$(agora_ms)
        gasto=$((t1 - t0))
        [ "$UNIDADE" = ms ] && seg=$(awk "BEGIN{printf \"%.1f\", $gasto/1000}") || seg=$gasto
        fps=$(awk "BEGIN{printf \"%.1f\", $quadros/($gasto/1000)}")
        fala "$rotulo [$config] ${seg}s para $quadros quadros -> $fps q/s"
    else
        fala "$rotulo [$config] PULADO: nao achei $jogo"
    fi
}

# ------------------------------------------------------------------ o cabecalho

fala "Zeebx: teste no aparelho $(date)"
fala "retroarch: $RA"
fala "config:    ${CFG:-(nao achei)}"
fala "core:      $CORE"
fala "roms:      $ROMDIR"
fala "duracao:   $SEG s por rodada"
fala "----------------------------------------"
{
    echo "== aparelho =="
    uname -a
    cat /etc/os-release 2>/dev/null | head -4
    grep -m2 -iE 'hardware|model name|revision' /proc/cpuinfo
    echo "nucleos: $(grep -c ^processor /proc/cpuinfo)"
    free -m 2>/dev/null | head -2
    echo "glibc: $(getconf GNU_LIBC_VERSION 2>/dev/null || ldd --version 2>/dev/null | head -1)"
    echo "df:"; df -h "$SAIDA" 2>/dev/null | tail -2
    echo "== retroarch =="; "$RA" --version 2>&1 | head -3
    echo "== core =="; ls -la "$CORE"; sha256sum "$CORE" 2>/dev/null
    echo "== opcoes antes =="; cat "$OPC_ANTES" 2>/dev/null || cat "$OPC" 2>/dev/null
    echo "== gpu =="; cat /sys/class/drm/card*/device/uevent 2>/dev/null | head -6
    dmesg 2>/dev/null | grep -iE 'mali|panfrost|drm' | tail -6
} > "$SAIDA/00-aparelho.txt" 2>&1

salva_opcoes

# ------------------------------------------------------------------ as rodadas

fala "--- 3D na placa: o descarte de tiles e a escala fracionaria ---"
roda base-crash    "Crash Bandicoot Nitro Kart 3D" base
roda base-rally    "Rally Master Pro"              base
roda portatil-rally "Rally Master Pro"             portatil
roda tiles-rally   "Rally Master Pro"              tiles
roda tiles-crash   "Crash Bandicoot Nitro Kart 3D" tiles

fala "--- 2D e pesados: o caminho de processador ---"
roda base-quake    "Quake"                         base
roda base-dd       "Double Dragon"                 base
roda base-mdrop    "Magical Drop III"              base
roda base-zeeboids "Zeeboids"                      base
roda base-peteca   "Zeebo Sports Peteca"           base

fala "--- Zenonia: o jogo que so desenha 2D, na placa e no processador ---"
roda base-zenonia  "Zenonia"                       base
roda sw-zenonia    "Zenonia"                       software

fala "--- o log do nucleo, em depuracao ---"
roda diag-rally    "Rally Master Pro"              depuracao

# ------------------------------------------------------------------ devolve e fecha

devolve_opcoes

fala "----------------------------------------"
fala "pronto. Mande esta pasta de volta: $SAIDA"
fala "o resumo esta em resumo.txt, e o log de cada rodada no arquivo com o nome dela."
