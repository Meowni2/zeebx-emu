# 11 — Compatibility

Surveyed by running **every package** for six virtual seconds each, without a window. The procedure
is at the end of this document and can be repeated at any time. The latest survey (2026-11-25, the
"Now" column) ran the 62 packages of one ROM folder: 61 Zeebo packages and Devil May Cry: Dante x
Virgil, a BREW phone game. Kingdom Hearts V CAST, in the earlier surveys, is not in that folder.

> "Runs" means **did not crash in six seconds**. It does not mean the screen is right or that the
> game is playable. It is the floor, not the ceiling.

## Scoreboard

| State | First survey | Later | Before | Now |
|---|---:|---:|---:|---:|
| runs | 33 | 50 | 56 | **59** |
| exited on its own | — | — | 2 | 2 |
| does not create the applet | 8 | 5 | 2 | 1 |
| stops in the frame loop | 7 | 6 | 2 | **0** |
| fails on `EVT_APP_START` | 10 | **0** | 0 | 0 |
| too slow | 3 | 1 | 0 | 0 |

### The scan lies when the disk fills up

The run that produced the "Before" column was done with the disk full, and **two games showed up as
"does not load"** with `No space left on device` as the reason — the scoreboard blamed the games,
and the problem was the machine. Repeated one at a time after freeing space, **Zenonia** ran the
full six seconds.

What remained from the scare, in `src/scan.rs`:

- the `OutOfSpace` category, separate from `DoesNotLoad`: a full disk arrives as a read error,
  and it is the only error on that list that says nothing about the game (28 on Unix, 112 on
  Windows);
- a **64 MB write test** before starting, because the scan of the 62 ROMs extracts each one to the
  cache and uses close to 1 GB;
- a warning printed in the middle of the scoreboard when the disk fills, because that invalidates
  even the games that passed: the report of a half-extracted game is worthless.

### What still does not run, and why

| Game | State | Measured cause |
|---|---|---|
| Zeebo Channels (Opera Mini) | does not create the applet | `CreateInstance` refused with error 1 |
| Zeebo App | exited on its own | a system application: it exits with no timer armed. Its report also has one missing API, `IWidget` selector `0x0` on class `0x01028e51` |
| Zeebo Clube | exited on its own | a system application, like Zeebo App. It asks for `0x0100110a`, `AEECLSID_STATIC` in the SDK's `AEEClassIDs.h` — the only class the whole survey asks for and does not get |

Three rows of the previous table left it. **Bejeweled Twist** no longer stops in the frame loop: it
passes the six seconds and, run for twenty with `zeebx run`, it is on its language screen (see
[below](#bejeweled-twist-stops-and-it-is-not-for-anything-we-owe)). **Zuma's Revenge** runs since
`0x0102fd92` (`JPEGDecoderBREW`) stopped being refused and the decoder stopped trying only PNG.
**Kingdom Hearts V CAST** is not in the latest survey's folder; its last measurement is in
[03-api-dispatch.md](03-api-dispatch.md).

Twelve games changed state at once, and the cause was a single one: **the console's file system is
case-insensitive, and ours was not.** The ten arcade ports ask for `font.fnz` and ship `font.FNZ`
in the package; none of them got past `EVT_APP_START`, because the font did not open and the null
pointer came right after. Raging Thunder 2 and Reckless Racing fell into the same hole with other
files. On Windows and macOS this worked by accident.

It is worth recording how the defect was found, because the path was not obvious: the ten asked
for six ClassIDs we did not have, which looked like the cause. **It was not** — they still lacked
those classes after the fix, and ran anyway (today the six are served, see
[below](#the-six-classes-from-the-extensions-group--served)). What gave the problem away was the
game's own `DBGPRINTF`, with a `Failed to open font.fnz file!!!` on the last line of the log, and
the report's list of "files not found" right below confirming it.

## Per game

From the 2026-11-25 survey. A report's caveats are what it lists besides the state: files not
found, APIs served on assumption, classes we do not have, missing APIs and allocations the heap
refused. The scan no longer reports refused pointers or text without a font, which the previous
tables counted.

### Runs clean (27)

Passed the six seconds and the report has nothing to point out.

- Action Hero 3D - Wild Dog & IMICRO3D
- Alice no Pais das Maravilhas
- Alien Breaker Deluxe
- Alpine Racer
- Caveman Ninja
- Dark Seal
- Disney All Star Cards
- Pac-Mania
- Peggle
- Prey Evil
- Resident Evil 4 - Zeebo Edition
- Ridge Racer
- Super BurgerTime
- Tekken 2
- Tork and Kral - A Prehistorik Adventure
- Toy Raid
- Treino Cerebral
- Turma da Monica em Vamos Brincar Vol. 1
- Um Jogo de Ovos
- Wizard Fire
- Zeebo F.C. Super League
- Zeebo Sports Peteca
- Zeebo Sports Queimada
- Zeebo Sports Tenis
- Zeebo Sports Volei
- Zeeboids
- Zenonia

"Clean" is about the report, not the game: it says nothing about what is on the screen.

### Runs with caveats (32)

Passed the six seconds, but the report pointed something out. **A file not found is rarely a
defect**: most are a game trying one path and then another, or a save that does not exist yet.
Ultimate Chess 3D looks for each sound on disk first (`chessbotexplode_01_v01.pmd`, then
`pt_chessbotexplode_01_v01.pmd`) and then reads it from `main.sar`; Quake's `config.cfg` and the
Zeebo Extreme games' `udata/optionsinfo.txt` only exist after the first save.

| Game | Files not found | APIs on assumption | Other |
|---|---:|---|---|
| Armageddon Squadron | 93 |  |  |
| Bad Dudes vs. DragonNinja | 1 |  |  |
| Bejeweled Twist |  | surface without `IDIB` |  |
| Crash Bandicoot Nitro Kart 3D | 103 |  |  |
| Devil May Cry: Dante x Virgil |  | MIDI timbre | the heap refuses a 100 MB `CheckAvail`, the game's own probe |
| Double Dragon |  | MIDI timbre |  |
| FIFA 09 | 5 |  |  |
| Galaxy on Fire | 55 |  |  |
| Heavy Barrel | 1 |  |  |
| Heavy Weapon | 1 |  |  |
| Iron Sight | 5 |  |  |
| Karnov's Revenge | 6 |  |  |
| Magical Drop III | 1 |  |  |
| Need For Speed - Carbon - Domine a Cidade | 2 |  |  |
| Powerboat Challenge | 24 |  |  |
| Quake | 7 |  |  |
| Quake II | 9 |  |  |
| Raging Thunder 2 | 187 |  |  |
| Rally Master Pro | 71 |  |  |
| Reckless Racing | 291 |  |  |
| Spin Master | 1 |  |  |
| Street Hoop | 1 |  |  |
| Ultimate Chess 3D | 11 | MIDI timbre | its CMX sounds are refused after the six seconds (see [07-audio.md](07-audio.md#cmx-ultimate-chess-3ds-sound-effects)) |
| Z-Wheel | 1 | five, from the stubs it runs on |  |
| Zeebo Extreme Baja | 47 |  |  |
| Zeebo Extreme Boia Cross | 49 |  |  |
| Zeebo Extreme Corrida Aerea | 49 |  |  |
| Zeebo Extreme Jetboard | 38 |  |  |
| Zeebo Extreme Rolima | 13 |  |  |
| Zeebo F.C. Foot Camp | 1 |  |  |
| Zeebo Family Pack | 87 |  |  |
| Zuma's Revenge |  | surface without `IDIB`, MIDI timbre |  |

"MIDI timbre" is the synthesized MIDI, whose instruments are an approximation until the console's
bank is read ([07-audio.md](07-audio.md)). "Surface without `IDIB`" is a game surface drawn
through `BltIn`.

### Does not run (3)

The three rows of [What still does not run](#what-still-does-not-run-and-why): Opera Mini does not
create the applet, and Zeebo App and Zeebo Clube exit on their own.

### The `nResID` that counted for nothing

`ISHELL_LoadResObject` receives the file, the **resource number** inside it and the **class** the
game wants back. We ignored the last two: we took the path, tried to decode the whole file as a
PNG and returned null when it was not one.

Tekken 2 asks for entry **5034** of `tekken2.bar`, a 308 KB BMP. We tried to read the whole 734 KB
`.bar` as if it were the image. The game went on with a sizeless image, and a sizeless image is a
division by zero when laying out the screen — that was what filled its log with `Arithmetic
exception: Divide By Zero` in the menu and in character select.

The report said so all along, in a line that is easy to misread (historical output; that
assumption no longer exists):

```
assumptions in use:
  a resource requested by LoadResObject is not a PNG we can read
```

It was not "not a PNG". It was **not an image** — it was the whole resource file. This is the
third time in this document that the answer was in the report before anyone understood the
question.

Three things changed: `nResID` now counts, the `AEEResBlob` header is skipped (what the SDK's
`RESBLOB_DATA()` does), and the requested class now counts — Tekken asks for `AEEIID_IBITMAP` and
was receiving an `IImage`, which made it call an `IBitmap` method on an `IImage` vtable.

Three games changed state:

- **Toy Raid** stopped in the frame loop and now opens with the whole menu drawn.
- **Pac-Mania** was "runs and shows nothing"; now it shows the whole language selection screen,
  correctly. It became slow because it now had something to draw, and the API profile pointed to
  where: five of its virtual seconds went from 174 to 62 of wall clock when `IIMAGE_Draw` started
  honouring the clip, and what is left is the surface synchronization on calls that do not draw.
- **Tekken 2** stopped hitting division by zero.

### The game that kept insisting

**Tekken 2** was the last "too slow": **766 thousand `Play` and 766 thousand `GetState` in four
virtual seconds**, and six seconds of game did not finish in five minutes of machine time. It was
not workload. Its music is MP3, `IMEDIA_Play` answered "that sound has already finished", the game
queried the state, saw "ready" and told it to play again. Forever.

With the duration read from the MP3 header — without decoding it, see
[07-audio.md](07-audio.md) —, the sound "plays" in silence for the right time and the game moves
on: **9.7 seconds** for the same six virtual ones, with the title screen drawn. It still does not
get past that point, and that is input, not performance: the game registers the button signal,
drains two presses and stops responding.

### The `IDIB` nobody asked for

Two entries left this list at once, and for the same reason. A BREW software `IBitmap` **is** an
`IDIB`: the struct continues with public fields, and the game reads the size straight from them,
without `QueryInterface`. We only filled those fields when the interface was requested.

**Peggle** built every sprite as a square with side zero — 76,618 of the 77,208 triangles of a
frame discarded for zero area, a black screen with the game drawing the whole time. Its log said
`-size 0/0`, and the rasterizer's list of rejections by reason confirmed it. Today it draws
geometry, but it is still stuck on loading and the sprites come out untextured.

**Heavy Weapon** was the "too slow" one with 6 million API calls per frame. It was not workload:
it was the game retrying against bitmaps that measured 0×0. Six virtual seconds went from **more
than five minutes** (blowing the `timeout`) to **2.1 seconds**, and it draws — with the wrong
colours, which is its next step.

### The six classes from the extensions group — served

```
0x0103d8de  0x0103d8eb  0x0103d8ef  0x0103d8f0  0x0103def1  0x010426e3
```

Asked for by seventeen games in the earlier surveys, and **none of them needed them to run**:
they all received `ECLASSNOTSUPPORT` and fell back to the alternative path. Quake's log and the
arcade ports' show where they come from — `GLES_ext.c` from the BREW SDK itself, which the games
link:

```
eglGetProcAddress (NBI) - got V2 EGLSurfaceManip interface
eglGetProcAddress (NBI) - got V2 GLESImageonExt interface
```

They are the console's OpenGL ES extensions, and since `0832a559` all six are served — Prey Evil
was the game that needed them to draw:

| ID | SDK name | Interface |
|---|---|---|
| `0x0103d8de` | `AEEIID_GLES10EXT` | `IGLES10Ext` |
| `0x0103d8eb` | `AEECLSID_GLES11EXT` | `IGLES11Ext` |
| `0x0103d8ef` | `AEEIID_EGLGETCOLORBUFFER` | `IEGLGetColorBuffer` |
| `0x0103d8f0` | `AEEIID_EGLGETPOWERLEVEL` | `IEGLGetPowerLevel` |
| `0x0103def1` | `AEEIID_GLES11EXTPAK` | `IGLES11ExtPak` |
| `0x010426e3` | `AEEIID_EGLOESSWAPINTERVAL` | `IEGLOESSwapInterval` |

In the 2026-11-25 survey no report lists any of them among the classes we do not have. The
vtables follow the SDK headers, checked by zeebx-core's `tools/check_sdk_slots.py`.

## The four that do not open, one by one

Surveyed by the open scan (`the_given_rom_opens`, see [17-tests.md](17-tests.md)): **60 of the 63
ROMs create the applet**; three do not. The fourth — Kingdom Hearts — was fixed along the way,
and it is worth telling what it was.

### Kingdom Hearts: the manifest that was in the "wrong" place — fixed

`no .mif next to the module says which applet to create`. The `.mif` **was** in the package:

```
Kingdon Hearts/Kingdon Hearts.mif          <- the manifest
Kingdon Hearts/Kingdon Hearts_/swv21brew.mod   <- the module, in its own folder
```

We looked for the `.mif` in two places computed by name: next to the `.mod`, and in the console
layout's `<Title>/mif/<id>.mif`. Neither is this one. Now the search also **scans** the module's
folder and the one above it, nearest first, in a stable order.

It opens, and the next obstacle has already shown up: the game asks for `0x0100a004` and
`0x0102bbfc`, does not find them, draws `The application has failed and will close (Error 14)`
and exits on its own. The 14 is `0x14` — twenty, the `ECLASSNOTSUPPORT` we return. Neither of the
two is in the APPS partition's class table.

### Opera Mini: `CreateInstance` refuses with `EFAILED`

The applet is not born because its constructor asks for `0x0100102e` and we do not have it. Its
interface is now **read**, not guessed: 59 methods, vtable `0x10a783e4` — see
[15-whats-missing-from-the-nand.md](15-whats-missing-from-the-nand.md), including why it did not
come out before.

### Zenonia: budget exhausted inside `CreateInstance`

Same family as Need For Speed, and the same outcome: `b #0x84634`, a jump to itself right after
`dbgprintf`. It also **stops on purpose**, and the budget runs out on top of that.

The difference is the cause: here the class `0x01003109`, its text subsystem, really is missing.
Its `ISHELL_CreateInstance` returns `ECLASSNOTSUPPORT` and the game's own log names what broke:

```
:AF![B:\WIPI\WBLIB\src\WBLCore.cpp:241]:(-268435448):
:FALSE && "CWBLText::Create() failed!"(0):
```

Its vtable has been read: 40 methods, at `0x10e15c04`. And the firmware says **what it is**: slot
0 increments `[obj+4]` and slot 1 decrements and destroys at zero; slot 2 compares the first
argument against a range of four events; slot 5 returns `[obj+8] & 1`. It is the vtable of an
**`IControl`** — `AddRef`, `Release`, `HandleEvent`, `Redraw`, `SetActive`, `IsActive`,
`SetRect`, `GetRect`, `SetProperties`, `GetProperties`, `Reset` — and the arguments the probe
captured confirm it: `SetRect` with a stack pointer, `SetProperties` with `0x80010000`,
`SetActive` with 1.

With the class answered by observation and `IsActive` saying "yes"
(`--probe=0x01003109 --probe-answer=0x01003109:5=1`), Zenonia creates the applet, receives
`EVT_APP_START`, loads fonts and particles, plays sound and **runs the frame loop**.

### And then it draws — but somewhere that does not go to the screen

What came next is worth keeping as a record, because the cause is general and the solution is not
cheap.

Zenonia creates a 320×240 surface and writes the pixels **straight into memory**, without asking
for `QueryInterface` first: it reads the `pBmp` pointer from the struct's public fields, which on
the console are always valid — a software `IBitmap` *is* an `IDIB`. We only fill those fields
when someone asks for the DIB, so it read size zero and a null pointer, and fell into a path that
draws nothing: a black screen with the game running, the same symptom as Peggle.

Publishing the fields at the birth of every surface, it **draws the whole title screen** — logo,
characters, version 1.0.8. You can see it with `--dump-surfaces=DIR`, which writes one BMP per
live surface.

But publishing everything **is not viable today**: the surface synchronization copies, on every
draw call, all the ones that are published. With 49 surfaces in Pac-Mania, six virtual seconds do
not finish in ten minutes of wall clock. Measured.

What is missing, then, is not a class: it is **synchronizing only what the call touches** — or
marking the dirty surfaces with a write hook, which the core already knows how to do (it is how
`--watch` works). With that, publishing at birth becomes cheap, and Zenonia starts showing what it
already draws.

And an open question remains even after that: it draws at 320×240 and **never blits** that
surface to the screen, nor writes to the screen. What presents that frame on the console we do
not know yet.

### Need For Speed Carbon: not slowness but a deliberate stop — fixed

This is the most interesting of the four, because the label was wrong. "Instruction budget
exhausted" sounds like a heavy game; the profile says something else — **97.8% of the 500 million
instructions in a single block**, `0x001a3c80`. That address is this:

```
0x1a3c78  mov  r0, #0x32
0x1a3c7c  blx  r1              <- helper 39, dbgprintf
0x1a3c80  b    #0x1a3c80       <- and stays here forever
```

A `b` to itself. The game **decided** to stop, and the two log lines are the ones it prints
before: `source\mediacenter\ZeeboSnd.cpp:327` and `BREAKPOINT!`.

Right above is the reason. It scans up to seventy entries of a sound table calling
`strstr(entry, "snd/skid/skid.wav")`, and the trace shows all seventy returning zero — the needle
is always the same pointer, `0x1a40ec`, and the filled entries are all `snd/carbon_fe/*.wav`. Not
finding the sound, it assumes a programming error and hangs.

And the file **exists** in the package, and the table **also** has `snd/skid/skid.wav`, at index
53. It was the search that did not find it — and not because of the skid.

The game registers five in-game sounds in sequence, and **the third name on the list is the empty
string**. In C, `strstr(anything, "")` returns the text itself: every string contains the empty
string, and the search finds index 0 immediately. Ours returned "not found" — the
`needle.len().max(1)` looked for a one-byte window equal to a zero-byte needle, which never
matches. So it scanned the seventy entries, concluded the sound did not exist, and hung on
purpose.

One line of C semantics, and the game showed up in the report as "too slow". **Today it creates
the applet, receives `EVT_APP_START` and presents 181 OpenGL frames in six virtual seconds**, with
the EA intro on screen.

The method lesson is worth keeping, and it is the third time it appears on this page: `--profile`
**printed nothing** when the game died before the frame loop, which is exactly the Need For Speed
case. As long as the profile only existed in the loop, "budget exhausted" had no way of becoming
"deliberate stop at such-and-such line".

**From the intro to the race.** After that came two more stops, and today the game reaches the
race:

- **Invalid access to `0x00000006`, at `0x001eefd8`.** The mesh parts have four-byte names
  **without a terminator**, and the game converts that name with `atoi`. Our `atoi` only accepted
  the whole string as a number and returned zero for all of them; C's stops at the first
  character that is not a digit. With every part at index zero, the mesh was not found and the
  game read a field from a null pointer.
- **Jump to `0x00000000` when skipping the intro cutscene.** Skipping creates an `IMedia` of class
  `AEECLSID_MEDIAMPEG4` (`0x01005505`) and uses the object without checking the return. The class
  is now accepted as media.
- **Speed.** In the race it ran at 69% of the console on the host; Dynarmic's page table and
  reading the frame in RGB565 brought it to 108%. The numbers are in
  [19](19-arm-jit-and-renderers.md).

Still open: the intro video comes out with black, torn triangles, and a reference from the
console is missing to know what is expected there.

**From the race to the career: BREW's `snprintf` counts the terminator — fixed (issue #35).**
After the introductory races, the car selection screen opened with the name in garbage, the
speed, acceleration and handling bars empty and the car untextured, and there was no way to go
on. The career screens are Flash (EA's Apt) and ask the game for the data via `LoadVariables`;
the game answers with a `name=value&...` string, and each array with its items separated by
`0x7f`. It builds that string like this:

```
p += snprintf(p, n, "%s=", "aCarNames") - 1;   // and each item with "%d%c" or "%s%c", 0x7f
```

The `- 1` only adds up if `snprintf` counts the terminator, and BREW's does: it returns the bytes
written including the zero, and with a null buffer the required size, also including the zero.
Ours returned the C99 length, and each item landed on top of the previous `=` and separator. What
reached the screen was `aTerritoryNamesPEDREIRA...` and `aTerritoryIDs0123456789101112...`: no
array with the right name, and the numbers glued into one. That is also why the report asked for
`large_portrait_.big` and `spline_-1.big`, and the event selection asked for an empty
`Career.GetEvents?iTerritoryID=`.

The BREW semantics came from the summary of the `SNPRINTF` page in the Brew MP 1.0.2 reference,
which is no longer online; the proof is the game: with the fix, the arrays arrive whole and the
car selection, gang car, map and event screens open filled in. `vsnprintf` was left as it was: it
is from the same family in the reference, but no measured game showed what it expects.

Two defects in the same formatter came along:

- **The `printf`s counted UTF-8 bytes.** The game's text is UTF-8 read as Latin-1 (inherited from
  the PSP), and the formatter's `String` stores each byte above `0x7f` as two. "ARMAZÉM" came back
  two bytes longer than what had been written. Now they count the bytes in the game's memory.
- **`%S` did not exist.** It is the `AECHAR` string in `SPRINTF`. It came out as it went in and
  consumed no argument, and the following `%d` printed the pointer: the map said "vença 286973016
  corridas na região %S" ("win 286973016 races in region %S").

The path to finding this is also recorded. Without a save and without a driver, getting to the
screen meant winning the first race. The debug `config.ini` that EA left in the package
(`mod/nfsresources/config.ini`) accepts `TutorialCompleted=True`, which skips the prologue and
takes you to the career in thirty seconds, already with the same defect. And the game's own log —
`AptTrace`, `<< AIP >>`, `No LV handler found` — goes out through `vsprintf`, not `dbgprintf`: to
read it you have to look at what `vsprintf` writes.

### Um Jogo de Ovos: boss 1 did not start — fixed

After stage 1-5 ends, the game loads the boss's entrance cutscene (`res/cutscenes/b1i.cut`,
inside `data.pap`) and stops there. Who says why is the game itself: it writes an `app.log` in
the save directory, and there were 747 lines of `[Error] Error loading cutscene` in the same
millisecond.

The cutscene's files all exist, and its text is well-formed. The defect is a byte the game reads
outside what it allocated: it loads the cutscene with an exact `MALLOC(size)`, and the parser
(`0x5337c` in the file) looks at the byte after the end before measuring each token. With a zero
there, it rejects the token, and the loop that calls it (`0x5392c`) prints the error and moves one
character forward. Our heap delivered zero in that byte. The fix and the measurement are in
[02](02-cpu-and-memory.md#heap-and-objects).

To find the function, the module base is `-0x9c`: the strings of `game.mod` are referenced as
`file offset - 0x9c`, and there is no absolute pointer to them.

### Peggle and Treino Cerebral on the GPU: the scissor that stayed on — fixed (issue #10)

In Peggle, with 3D on the GPU, the screens did not clear: the menu and the "PLAY" stayed visible
during the stage, and the stage background never appeared. In software, everything was fine.
Peggle clips almost every draw with `glScissor` and turns the scissor off for the backgrounds.
With the context borrowed from the window, `hand_back_context` runs on every batch, records in
the state mirror that the scissor was left off and **did not turn it off** in GL. The background,
with no scissor, resent nothing, and came out clipped by the rectangle of the last sprite.
Measured without a window, with an offscreen context: forcing the scissor to be reapplied on every
batch already brought the background back.

Treino Cerebral lost pieces along the same path, in the parts that use the scissor: the doctor on
the rules screen, the "TREINO" of the logo and the row of selection balls in the "Bolas"
minigame.

### Treino Cerebral on the GPU: lines every few pixels — fixed (issue #10)

The buttons and balloons in Treino are built from narrow pieces side by side, and on the GPU each
seam came out as a light line, at 1x and at 3x. The game asks for `GL_NEAREST` for these textures,
but asks **before** sending the image, and the GPU only kept parameters of textures that already
existed on it: the texture was born in `upload_level` with `GL_LINEAR`, and bilinear filtering
pulled the atlas neighbour into the seam. Forcing `CLAMP_TO_EDGE` changed nothing; forcing
`GL_NEAREST` cleared the lines. Now the texture is born with the filter, the wrap and the crop
that the software state already kept.

### FIFA 09: the player's name vanished from the HUD — fixed (issue #12)

The HUD shows "initial. surname". The game takes the initial by converting **one** character into
a **one**-byte buffer with `wstrtoutf8`, and ours reserved room for the terminator: the initial
came out empty and the name did not appear. Only the players with a single name, like "Chris",
appeared. The function also wrote Latin-1, not UTF-8: the "ô" in "Chantôme" became a stray byte.
Now it writes UTF-8, a character only goes in if it fits whole, and the zero only goes in when
there is room left.

### Crash Nitro Kart: the scenery that appeared up close was the viewport — fixed

The report was about draw distance: a void ahead until the kart went through, and then the next
section appearing from the bottom up. It was not distance. The game draws the next section
through a portal, with the viewport on its rectangle, and we counted the `glViewport` `y` from the
top. The explanation and the fix are in [06](06-video-3d.md#the-surface). That is why there is
no draw distance option: the game already drew everything, just in the wrong place.

### Devil May Cry: Dante x Virgil: a phone game on the Zeebo — runs

Capcom's 2008 game for BREW **phones**, not for the Zeebo. The package (`dmc_mas.mod`, applet
`0x01070e36`) brings HI Corporation's IMICRO3D engine (`12875/imicro3d.mod`), the same one as
Action Hero 3D, and its art for WQVGA phones (240×400). It loaded, and four things stood between
it and a fight with sound. Each of them is a real difference between a phone and the console,
not a bug in the game.

1. **`IDISPLAY_CreateDIBitmap` did not exist**, and the game stopped at 10 virtual ms. Its own
   BMP reader (`0x40000`) asks for a `4 × 0 × 0` DIB and then **writes the `IDIB` fields by
   hand**: `pBmp` at the last row of the file's pixels, a negative `nPitch` because a BMP is
   stored bottom-up, `pRGB` at a copy of the file's `RGBQUAD`s. Every other `IDIB` here is an
   RGB565 surface of ours mirrored to the guest; this one belongs to the game, so its header is
   read again on every blit and decoded on the spot (1/2/4/8 bits with a palette, 16 bits) —
   `src/machine/guest_dib.rs`.
2. **No key moved the first menu.** Its handler (`0xae8`) answers `EVT_KEY` with a bare
   `return TRUE` and keeps its key bits only on `EVT_KEY_PRESS` and `EVT_KEY_RELEASE`. A phone
   sends all three; the console never sends the press (Quake would open its menu mid-level, see
   [09](09-input.md#real-keys)). The phone sequence goes only to the classes listed in
   `src/input/phone.rs`, which also maps the controller to the game's own default keys, from its
   HELP > KEY OPTION screen: D-pad → arrows, 1 → `OK` (evade, confirm), 2 → `8` (sword),
   3 → `9` (gun), 4 → `#` (jump), ZR → `0` (style), HOME/Start → left soft key, ZL → right soft
   key. The game lets the player remap every action to 0–9, `#`, `*` and `OK`.
3. **The `IMedia` it plays first is silence on purpose.** `dummy.mid`, 64 bytes, volume 0, one
   note held for six seconds. The MIDI synthesizer refused an all-silent score — a leftover from
   when it normalized by the peak —, the object ended at once and the game called `Play` about
   1,500 times in twenty seconds. A silent score now lasts its length (8.5 s).
4. **No sound at all, with every effect and song decoded in memory.** The sound manager only
   starts when its "ready" field is non-zero, and that field is a copy of a volume that the
   **vibration** manager (`0x350ac`) writes — 100 — only after creating `AEECLSID_SOUNDPLAYER`
   (`0x01002000`, `AEECLSID_ENH` in `AEEClassIDs.h`). We refused that class, so the game did
   not crash and played nothing. `ISoundPlayer` now exists, with its SDK slot table in zeebx-core
   (`aee_slots::SOUND_PLAYER`); `RegisterNotify`, `SetVolume`, `GetVolume`, `SetInfo` and
   `GetInfo` are served (the volume
   arrives through `AEE_SOUNDPLAYER_VOLUME_CB` with `dwParam` **pointing** at an
   `AEESoundPlayerCmdData`, which is not how `ISound` does it). Nothing plays through it yet:
   both games that create it only use it to read the volume. Ultimate Chess 3D, the other one,
   hands it a buffer with `SetInfo` first — with the class served but `SetInfo` missing, it
   stopped at `EVT_APP_START`; with both, its report over forty seconds is the same as when the
   class was refused, minus the class itself (same heap to the byte, same sounds). With it, the
   title, mission select and mission songs play (measured with `--dump-audio`: silence until
   the title song at 11 s, then a peak of 0.17).

And around it:

- **The phone screen.** Told 640×480 by `GetDeviceInfo`, the game centres a 240-pixel column and
  fills the rest with an almost-white rectangle of its own (`0x498`). Phone games now get their
  phone's screen instead — 240×400 here, the size of its `wqvga` art — in `GetDeviceInfo` and in
  `AEEAppStart`, and the game draws exactly that area from the corner (title, menus and a mission
  measured; 240×320 also lays out). The console's framebuffer stays 640×480, so save states and
  2D surfaces do not change. `Session::display_screen` is that area alone, and the Qt window and
  the `libretro` core show it, scaled with its own 3:5 aspect (the integer scale counts its
  pixels). Every other frontend still calls `Session::screen` and gets the area **centred** on a
  black 640×480 screen — never the game squeezed into a corner — until it learns to crop:
  Android, iOS, the Vita apps, Switch and the headless build.
- **Music or effects, never both — by the game's design.** OPTION > SOUND OPTION offers OFF, BGM
  and SE, and BGM is the default. The choice is the second byte of `database.sav` (read into the
  applet at `+0xe9c`), and the game's `playSound` (`0x313a4`) drops every non-looping sound under
  BGM and every looping one under SE; it has a single `IMedia` channel. Measured with the byte set
  to SE: the menu confirm (`s_01_ok.wav`, 2,934 bytes) and the gunshot (`s_04_gun2.wav`, 4,224
  bytes) load and play, IMA ADPCM decoded, peak 0.71. Nothing to fix here.
- **Vibration does not reach the controller**: the game vibrates through `ISound`, whose
  `Vibrate` only answers that it finished.
- **Ten frames per second, by the game's design.** Each frame ends with
  `ISHELL_SetTimer(100 ms − the time the frame took)`: on the title screen, 199 of the 202
  `SetTimer` calls in twenty virtual seconds ask for 100 ms, and the clock shows 201 timer
  firings. Its logic advances once per frame, so a faster pace would be a faster game, not a
  smoother one — measured on 2026-11-12 by dividing that timer by three, which made the title
  animation run about three times as fast. The emulator is not the limit: `zeebx bench` counts 38
  million ARM instructions in those twenty virtual seconds of title screen, a small fraction of
  what a 528 MHz ARM runs in that time.

## The `printf` width was an invisible defect

Worth recording because it does not show up as any crash in this table.

`cformat` read the specifier's flags and width and **discarded** them — which is fine for the
`DBGPRINTF` log, which was the original use, but the same formatter serves the guest's `snprintf`
and `vsnprintf`. Resident Evil 4 builds the stage file names with `%s_%02d.h2z`; without the
width, the `%02d` of zero came out as `0`, it looked for `3d_stg02_0.h2z` and the file is
`3d_stg02_00.h2z`. **The twelve stages were not found**, and the game ran with no textures at all
— without crashing, without errors, with nothing in the report except a list of "files not found"
that nobody had read.

It was the same list that gave away the arcade ports' `font.fnz`, and both times it had been
there from the start. The lesson: **the report's list of files not found is signal, not noise.**

## Bejeweled Twist stops, and it is not for anything we owe

> **It no longer stops.** In the 2026-11-25 survey it passes the six seconds, and run for twenty
> virtual seconds with `zeebx run` it shows its language screen (English, Español, Português),
> with no invalid access. Which change filled the three fields below was not traced. The
> diagnosis stays because it is still how this crash would be read if it came back.

Worth recording a diagnosis that did **not** end in a fix, because its value is closing the
question: the game shows up as "stops in the loop — invalid access to `0x00000024`", and the
temptation is to look for a missing API.

There is none. Its report comes out with **zero** unknown classes, **zero** files not found and
no API by assumption beyond its own surface. What it does in the six seconds is all there:
eighteen images, each through the same cycle —
`GetDeviceBitmap` → `QueryInterface(DIB)` → `MEMASTREAM` → `SetEx` → decoder → `Notify` →
`SetStream` → `SetDestination(their surface)` → `Draw` → `SetDestination(0)` → `GetInfo`. The
drawing **does reach** its surface: its `BltIn` asks for `QueryInterface(0x01001045)` on our
source bitmap and the call shows up in the trace, returning zero.

The crash is at `0x32b78`, `ldr r2, [r0, #0x14]`, with `r0 = 0x10` — because `0x3a7c8` is
`add r0, r0, #0x10; bx lr`, an accessor, and what it received was zero. The zero comes from
`[this+0x20]`. The caller at `0x32d04` only takes this path **when `[this+0x28]` is zero**, and
the watchpoint shows the constructor at `0x17ef0` zeroing `0x20`, `0x24` and `0x28` and nothing
filling them afterwards. It is the game's internal state, not an answer of ours: the three fields
stayed as they were born.

Where this continues, then, is finding who should call the setter — not implementing an API. It
is written down so the next person does not redo the digging up to here.

## How to repeat it, as a test

The same survey runs inside `cargo test`, in [`src/scan.rs`](../../src/scan.rs). The difference
that matters is not convenience: it is that a regression starts showing up **in the commit that
caused it**, and not the next time someone repeats the scan by hand.

```bash
# one game, with the whole report on screen
ZEEBX_ROM="roms/Quake.zip" cargo test --release scan::tests -- --nocapture

# the whole scan, writing one report per game and checking against the baseline
ZEEBX_ROM=roms ZEEBX_ROM_OUT=out ZEEBX_ROM_BASE=docs/scan \
  cargo test --release scan::tests -- --nocapture
```

| Variable | What it does |
|---|---|
| `ZEEBX_ROM` | a `.mod`/`.zip`, a comma-separated list, or a directory |
| `ZEEBX_ROM_MS` | how much **virtual** time to run, in ms (default 6000, the one on this page) |
| `ZEEBX_ROM_CAP` | **real**-time cap per game, in seconds (default 90) |
| `ZEEBX_ROM_OUT` | directory where to write each game's full report |
| `ZEEBX_ROM_BASE` | baseline directory: what is missing is written, what exists is checked |

> **Warm the cache before comparing.** The **first** game opened from each extraction executes
> differently from the following ones — measured: 760,921 instructions with a cold cache against
> 760,815 with a warm cache, with the heap shifted by 64 bytes. The comparison against the
> baseline flags that difference as a regression. Run the corpus once without `ZEEBX_ROM_BASE`
> (the warm-up pass) and only the second pass counts as a measurement. See
> `docs/OPTIMIZING_V0.3.0.md`, section 33.

> **`docs/scan/` no longer works as a baseline.** Every game shows up as changed against it, for
> three reasons measured on 2026-11-25: its reports are still in Portuguese (`estado: roda`), from
> before the codebase moved to English; they carry the recording machine's paths
> (`open failed: /home/<user>/.config/zeebx/...`); and the scan runs on the real saves folder, so
> a game that rotates its saves changes between two runs — Ultimate Chess 3D's `setting.sv0`,
> `sv1` and `sv2` came out different in two runs in a row against a baseline written one minute
> earlier. A fresh one would inherit the last two. Recording it again is only worth it once the
> scan writes paths relative to the profile and runs on a throwaway saves folder.

No ROM is in the tree, so without `ZEEBX_ROM` both tests warn and pass — `cargo test` does not
turn into a sixty-five-game scan without someone asking for it.

There are two tests, and the difference between them is the cost:

- **`the_given_rom_opens`** is the cheap question: it loads the `.mod`, runs `AEEMod_Load`, reads
  the `.mif` and calls `CreateInstance`. Whatever does not create an applet fails here without
  spending the six virtual seconds it would never run.
- **`the_given_rom_progresses`** delivers `EVT_APP_START`, runs the frame loop and builds the
  report: state, performance, what was missing and the game log. The start is kept separate from
  the first pass, and that is what distinguishes two lines on this page — "crashed on
  `EVT_APP_START`" and "crashed in the frame loop" point to different stretches of code.

The test fails for two reasons, and only those two:

1. **The game did not reach the end of the requested time.** It does not load, does not create
   the applet, crashes, or does not complete the virtual time within the real-time cap. A pending
   item is not a failure: a game that runs while asking for six classes we do not have keeps
   passing, because that is how it behaves.
2. **The summary changed relative to the baseline**, when there is one. That is what checks
   pending items: an API that stopped being served, a class that started being requested or a
   file that disappeared show up as `+`/`-` in the test output.

The baseline stores the state and the pending items, and **nothing about performance** — no
time, no instructions, no frames. If it did, switching machines would flag a regression in every
game at once, and a test that always complains is never read. That is also why `--release` is not
decoration: in debug the core runs an order of magnitude slower, and the real-time cap would
classify a good game as "too slow".

## How to repeat it, by hand

```bash
for z in roms/*.zip; do
  n=$(basename "$z" .zip)
  timeout 90 ./target/release/zeebx run "$z" --seconds=6 > "out/$n.txt" 2>&1
done
```

The emulator extracts the `.zip` by itself and picks the right `.mod`, so unpacking by hand is no
longer needed. Whatever blows the `timeout` is "too slow".

What to read in each output, **in this order** — the order is what separates the categories:

| Line | Means |
|---|---|
| `applet: no .mif found next to the module` | did not even get to create the applet |
| `start: EVT_APP_START → returned …` | the applet was created and received the initial event |
| `start: EVT_APP_START → invalid access …` | crashed on the initial event, before the first frame |
| `stopped on pass N at …` | crashed already inside the frame loop, on pass N |
| `files not found:` | **always read it** — a wrong name here becomes a black screen or a crash, with no error at all |
| `unknown classes: …` | ClassIDs the game asked for and we do not have |
| `calls:` | how many times each method was called — it is what points to bottlenecks |
| `game log:` | the game's own `DBGPRINTF`s, which usually name the problem |
