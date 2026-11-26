//! `IFont`: the system bitmap fonts.
//!
//! BREW does not draw text only with the console's font: it has ready-made **font classes**, and
//! the application asks `ISHELL_CreateInstance` for one of them. There are 29 classes in four
//! families:
//!
//! | Family | Classes |
//! |---|---|
//! | `AEECLSID_FONTSYS*` | `NORMAL`, `LARGE`, `BOLD`, `ITALIC`, `BOLDITALIC`, `LARGEITALIC` |
//! | `AEECLSID_FONT_STANDARD*` | `11`, `11B`, `15`, `15B`, `18`, `18B`, `23`, `23B`, `26`, `26B`, `36` |
//! | `AEECLSID_FONT_BASIC*` | `6`, `9`, `10`, `11`, `11B`, `12`, `12B`, `14`, `15` |
//! | standalone | `AEECLSID_FONT`, `FONT_FIXED4X6`, `BITFONTFOUNDRY` |
//!
//! **It is not `ITypeface`.** `ITypeface` creates fonts from a TTF; `IFont` already **is** the
//! drawable font. The two interfaces have different methods, and treating them as the same thing
//! is what made Double Dragon, Resident Evil 4 and the Data East ports fall into the
//! "Memory is insufficient. Please delete some files." screen — their generic failure message when
//! the font's `CreateInstance` comes back null.
//!
//! The metrics table comes from the reference code that already implements this interface
//! (`zeebo-emulator/research/sources/zeemu/brew/BrewFont.cpp`). Nothing else does: the order of
//! the six methods, their arguments and the `AEEFontInfo` they fill are `INHERIT_IFont` and its
//! comments in `inc/AEEIFont.h` (see [`crate::brew::aee_slots::FONT`]). The reference had its own
//! `AEEFontInfo` layout and guessed `DrawText`'s arguments; both disagreed with the header.

use super::*;

/// What a system font says about itself, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metrics {
    pub ascent: u16,
    pub descent: u16,
    pub leading: u16,
    pub max_char_width: u16,
    pub height: u16,
    pub bold: bool,
    pub italic: bool,
}

/// The classes this interface answers for.
///
/// Without this list, `CreateInstance` for these classes returns "not supported" — and the game,
/// which does not tell "does not exist" from "out of memory", shows the out-of-memory screen.
pub const FONT_CLASSES: [u32; 29] = [
    // FONTSYS
    0x0101_2786,
    0x0101_2787,
    0x0101_2788,
    0x0101_402c,
    0x0101_402d,
    0x0101_402e,
    // FONT
    0x0100_1022,
    // FONT_STANDARD
    0x0102_f679,
    0x0102_f67a,
    0x0103_0852,
    0x0103_0853,
    0x0102_f67b,
    0x0102_f67c,
    0x0102_f67d,
    0x0102_f67e,
    0x0102_f67f,
    0x0102_f680,
    0x0102_f681,
    // FIXED4X6
    0x0100_a001,
    // BASIC
    0x0100_a002,
    0x0100_a003,
    0x0100_a004,
    0x0100_a005,
    0x0100_a006,
    0x0100_a007,
    0x0100_a008,
    0x0100_a009,
    0x0100_a00a,
    // BITFONTFOUNDRY
    0x0100_a100,
];

/// The metrics of each class, as the reference has them.
///
/// The numbers are declared in the `.bid` for some (`FONT_STANDARD11` has ascent 13 and descent 3)
/// and measured in the reference for the rest. The two sources agree where they overlap.
pub fn class_metrics(class_id: u32) -> Metrics {
    let (ascent, descent, leading, max_char_width, height, bold) = match class_id {
        0x0100_a001 => (4, 2, 0, 4, 6, false),
        0x0100_a002 => (6, 2, 1, 5, 9, false),
        0x0100_a003 => (9, 3, 1, 6, 13, false),
        0x0100_a004 => (10, 3, 1, 7, 14, false),
        0x0100_a005 | 0x0101_2786 | 0x0101_402c | 0x0100_1022 => (13, 3, 2, 8, 18, false),
        0x0100_a006 | 0x0101_2788 | 0x0101_402d => (13, 3, 2, 8, 18, true),
        0x0100_a007 => (14, 4, 2, 9, 20, false),
        0x0100_a008 => (12, 4, 2, 8, 18, false),
        0x0100_a009 => (12, 4, 2, 8, 18, true),
        0x0100_a00a => (15, 4, 2, 10, 21, false),
        0x0101_2787 | 0x0101_402e => (21, 5, 2, 10, 28, false),
        // The `FONT_STANDARD` family, one line per class, with `ascent` and `descent` **copied
        // from `AEEFontsStandard.BID`** and the bold that the `.bid`'s own text declares
        // ("Bolded"). The table got three wrong: `STANDARD15B` answered 17/4 — the 18B numbers —,
        // and the bold of 23B and 26B lived in a patch outside the table, where the next edit
        // wouldn't see it. A game that reserves height from the `GetInfo` answer draws line over
        // line when it comes back wrong, and the 2 px difference between 15/3 and 17/4 is the
        // height of a whole line on a 320-pixel screen.
        0x0102_f679 => (13, 3, 2, 8, 18, false), // STANDARD11
        0x0102_f67a => (13, 3, 2, 8, 18, true),  // STANDARD11B
        0x0103_0852 => (15, 3, 2, 8, 20, false), // STANDARD15
        0x0103_0853 => (15, 3, 2, 8, 20, true),  // STANDARD15B
        0x0102_f67b => (17, 4, 2, 9, 23, false), // STANDARD18
        0x0102_f67c => (17, 4, 2, 9, 23, true),  // STANDARD18B
        0x0102_f67d => (21, 5, 2, 10, 28, false), // STANDARD23
        0x0102_f67e => (21, 5, 2, 10, 28, true), // STANDARD23B
        0x0102_f67f => (23, 6, 2, 10, 31, false), // STANDARD26
        0x0102_f680 => (23, 6, 2, 10, 31, true), // STANDARD26B
        // The largest in the system, used for big titles.
        0x0102_f681 => (38, 10, 2, 12, 50, false), // STANDARD36
        _ => (12, 4, 2, 8, 16, false),
    };
    let is_bold = bold;
    Metrics {
        ascent,
        descent,
        leading,
        max_char_width,
        height,
        bold: is_bold,
        italic: matches!(class_id, 0x0101_402c | 0x0101_402d | 0x0101_402e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::DefaultBackend;
    use crate::loader::modfile::ModImage;

    /// **The `FONT_STANDARD` metrics are transcribed from the `.bid`, not made up.**
    ///
    /// `AEEFontsStandard.BID` declares ascent and descent for the eleven classes, and says which
    /// are bold. This test is a literal copy of that file: if someone "fixes" the table from
    /// memory, it disagrees — which is the point, because memory already got three lines wrong
    /// once.
    #[test]
    fn the_standard_metrics_match_the_bid() {
        let bid = [
            (0x0102_f679u32, 13u16, 3u16, false), // STANDARD11
            (0x0102_f67a, 13, 3, true),           // STANDARD11B
            (0x0103_0852, 15, 3, false),          // STANDARD15
            (0x0103_0853, 15, 3, true),           // STANDARD15B
            (0x0102_f67b, 17, 4, false),          // STANDARD18
            (0x0102_f67c, 17, 4, true),           // STANDARD18B
            (0x0102_f67d, 21, 5, false),          // STANDARD23
            (0x0102_f67e, 21, 5, true),           // STANDARD23B
            (0x0102_f67f, 23, 6, false),          // STANDARD26
            (0x0102_f680, 23, 6, true),           // STANDARD26B
            (0x0102_f681, 38, 10, false),         // STANDARD36
        ];
        for (class_id, ascent, descent, is_bold) in bid {
            let measured = class_metrics(class_id);
            assert_eq!(
                (measured.ascent, measured.descent),
                (ascent, descent),
                "ascent/descent of class {class_id:#010x}"
            );
            assert_eq!(measured.bold, is_bold, "bold of class {class_id:#010x}");
            assert!(
                measured.height >= measured.ascent + measured.descent,
                "height smaller than ascent plus descent in {class_id:#010x}"
            );
        }
    }

    /// A bare machine with one font of `class_id`, and the stack ready for the arguments past r3.
    fn machine_with_font(class_id: u32) -> (Machine<DefaultBackend>, u32) {
        // `bx lr`, five times: a `.mod` shorter than its header does not parse.
        let code = 0xe12f_ff1eu32.to_le_bytes().repeat(5);
        let module = loader::load(&ModImage::parse(code).unwrap()).unwrap();
        let mut machine = Machine::new(DefaultBackend::new().unwrap(), module, ".");
        machine.cpu.reset(&machine.module.mem).unwrap();
        machine.cpu.write_reg(Reg::Sp, STACK);
        let out = loader::HEAP_BASE + 0x100;
        assert_eq!(machine.create_font(class_id, out).unwrap(), SUCCESS);
        let font = machine.cpu.read_u32(out).unwrap();
        (machine, font)
    }

    const STACK: u32 = loader::STACK_BASE + 0x1000;
    const STANDARD15: u32 = 0x0103_0852;

    /// Calls an `IFont` method with `regs` in r0–r3 and `stack` past them, as the guest would.
    fn call(machine: &mut Machine<DefaultBackend>, name: &str, regs: [u32; 4], stack: &[u32]) -> u32 {
        let slot = (0..16)
            .find(|&slot| Interface::Font.method(slot) == Some(name))
            .unwrap_or_else(|| panic!("IFont has no {name}"));
        for (index, value) in regs.iter().enumerate() {
            machine.cpu.write_reg([Reg::R0, Reg::R1, Reg::R2, Reg::R3][index], *value);
        }
        for (index, value) in stack.iter().enumerate() {
            machine.cpu.write_u32(STACK + index as u32 * 4, *value).unwrap();
        }
        machine
            .dispatch(aee::encode(Interface::Font, slot))
            .unwrap()
            .expect("IFont serves this method")
    }

    /// Writes `text` as AECHARs at `addr`, with no terminator of its own.
    fn write_aechars(machine: &mut Machine<DefaultBackend>, addr: u32, text: &str) {
        let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        machine.cpu.write_mem(addr, &bytes).unwrap();
    }

    fn read_i16(machine: &Machine<DefaultBackend>, addr: u32) -> i16 {
        let mut pair = [0u8; 2];
        machine.cpu.read_mem(addr, &mut pair).unwrap();
        i16::from_le_bytes(pair)
    }

    /// **`GetInfo` fills the SDK's `AEEFontInfo`: ascent and descent, from offset zero.**
    ///
    /// It wrote five fields from offset 4, and with `nSize = sizeof(AEEFontInfo)` — 4, the call
    /// in the SDK's own example — it wrote nothing, leaving whatever was on the game's stack.
    #[test]
    fn get_info_fills_the_sdk_struct_and_only_up_to_its_size() {
        let (mut machine, font) = machine_with_font(STANDARD15);
        let info = loader::HEAP_BASE + 0x200;
        machine.cpu.write_mem(info, &[0xaa; 8]).unwrap();

        assert_eq!(call(&mut machine, "GetInfo", [font, info, 4, 0], &[]), SUCCESS);
        assert_eq!((read_i16(&machine, info), read_i16(&machine, info + 2)), (15, 3));
        assert_eq!(read_i16(&machine, info + 4), -0x5556, "nothing past the struct");

        machine.cpu.write_mem(info, &[0xaa; 8]).unwrap();
        assert_eq!(call(&mut machine, "GetInfo", [font, info, 2, 0], &[]), SUCCESS);
        assert_eq!(read_i16(&machine, info), 15);
        assert_eq!(read_i16(&machine, info + 2), -0x5556, "only what fits in nSize");

        // A size this font does not know is a version it does not support.
        machine.cpu.write_mem(info, &[0xaa; 8]).unwrap();
        assert_eq!(call(&mut machine, "GetInfo", [font, info, 8, 0], &[]), EUNSUPPORTED);
        assert_eq!(read_i16(&machine, info), -0x5556);
    }

    /// `MeasureText` counts exactly `nChars` — a zero in the middle is a character — and says how
    /// many of them fit in `nMaxWidth`.
    #[test]
    fn measure_text_counts_n_chars_and_what_fits_in_the_width() {
        let (mut machine, font) = machine_with_font(STANDARD15); // 8 pixels per character
        let text = loader::HEAP_BASE + 0x200;
        write_aechars(&mut machine, text, "AB\0CD");
        let (fits, pixels) = (loader::HEAP_BASE + 0x300, loader::HEAP_BASE + 0x304);

        let all = i32::MAX as u32; // IFONT_MAXWIDTH
        assert_eq!(call(&mut machine, "MeasureText", [font, text, 5, all], &[fits, pixels]), SUCCESS);
        assert_eq!(machine.cpu.read_u32(fits).unwrap(), 5);
        assert_eq!(machine.cpu.read_u32(pixels).unwrap(), 40);

        call(&mut machine, "MeasureText", [font, text, 5, 20], &[fits, pixels]);
        assert_eq!(machine.cpu.read_u32(fits).unwrap(), 2, "two 8-pixel characters fit in 20");
        assert_eq!(machine.cpu.read_u32(pixels).unwrap(), 16);
    }

    /// A 64×32 surface of the game's, painted `0x1234`.
    fn surface(machine: &mut Machine<DefaultBackend>) -> u32 {
        let bitmap = machine.new_object(Interface::Bitmap).unwrap();
        let mut frame = Framebuffer::new(64, 32);
        frame.fill_rect_native(Rect { x: 0, y: 0, width: 64, height: 32 }, 0x1234);
        machine.bitmaps.insert(bitmap, frame);
        bitmap
    }

    /// An `AEERect` as the guest stores it: four `int16`.
    fn rect_bytes(x: i16, y: i16, width: i16, height: i16) -> Vec<u8> {
        [x, y, width, height].iter().flat_map(|value| value.to_le_bytes()).collect()
    }

    fn pixel(machine: &Machine<DefaultBackend>, bitmap: u32, x: i32, y: i32) -> u16 {
        machine.bitmaps[&bitmap].get_pixel(x, y)
    }

    /// **`DrawText` reads its arguments in the header's order and draws on `pDst`.**
    ///
    /// The cell is three 8-pixel characters by ascent plus descent (15 + 3), at `(4, 2)`, and its
    /// background is `bg`. Spaces, so that no glyph covers what the test reads.
    #[test]
    fn draw_text_fills_the_cell_on_the_destination_with_the_background() {
        let (mut machine, font) = machine_with_font(STANDARD15);
        let destination = surface(&mut machine);
        let (text, clip) = (loader::HEAP_BASE + 0x200, loader::HEAP_BASE + 0x300);
        write_aechars(&mut machine, text, "   ");
        machine.cpu.write_mem(clip, &rect_bytes(0, 0, 64, 32)).unwrap();

        let stack = [text, 3, 0xffff, 0x001f, clip, 0];
        assert_eq!(call(&mut machine, "DrawText", [font, destination, 4, 2], &stack), SUCCESS);
        assert_eq!(pixel(&machine, destination, 4, 2), 0x001f);
        assert_eq!(pixel(&machine, destination, 27, 19), 0x001f, "the cell's last pixel");
        assert_eq!(pixel(&machine, destination, 28, 2), 0x1234, "past the three characters");
        assert_eq!(pixel(&machine, destination, 4, 20), 0x1234, "below ascent plus descent");
        assert_eq!(machine.display_target, 0, "the display's own destination is left alone");
    }

    #[test]
    fn draw_text_is_cut_by_the_clip_and_a_negative_clip_draws_nothing() {
        let (mut machine, font) = machine_with_font(STANDARD15);
        let destination = surface(&mut machine);
        let (text, clip) = (loader::HEAP_BASE + 0x200, loader::HEAP_BASE + 0x300);
        write_aechars(&mut machine, text, "   ");

        machine.cpu.write_mem(clip, &rect_bytes(0, 0, 10, 10)).unwrap();
        call(&mut machine, "DrawText", [font, destination, 4, 2], &[text, 3, 0, 0x001f, clip, 0]);
        assert_eq!(pixel(&machine, destination, 9, 9), 0x001f);
        assert_eq!(pixel(&machine, destination, 10, 5), 0x1234, "right of the clip");
        assert_eq!(pixel(&machine, destination, 5, 10), 0x1234, "below the clip");

        let other = surface(&mut machine);
        machine.cpu.write_mem(clip, &rect_bytes(0, 0, -1, 32)).unwrap();
        let stack = [text, 3, 0, 0x001f, clip, 0];
        assert_eq!(call(&mut machine, "DrawText", [font, other, 4, 2], &stack), SUCCESS);
        assert_eq!(pixel(&machine, other, 4, 2), 0x1234);
    }

    #[test]
    fn draw_text_transparent_keeps_the_background_and_records_exactly_n_chars() {
        let (mut machine, font) = machine_with_font(STANDARD15);
        let destination = surface(&mut machine);
        let text = loader::HEAP_BASE + 0x200;
        write_aechars(&mut machine, text, "Hi there");

        let stack = [text, 2, 0xffff, 0x001f, 0, IFONT_DRAW_TRANSPARENT];
        assert_eq!(call(&mut machine, "DrawText", [font, destination, 30, 4], &stack), SUCCESS);
        // The descent band of the cell (y 19 to 21): "Hi" has no descender, so only a background
        // fill would paint it.
        assert_eq!(pixel(&machine, destination, 31, 21), 0x1234, "no background fill");
        let last = machine.drawn_text().last().cloned();
        assert_eq!(last, Some((machine.clock_ms(), 30, 4, "Hi".to_string())));
    }

    /// `NativeColor` is our surfaces' RGB565: on anything else there is no format to draw in.
    #[test]
    fn draw_text_on_an_unknown_bitmap_is_unsupported() {
        let (mut machine, font) = machine_with_font(STANDARD15);
        let text = loader::HEAP_BASE + 0x200;
        write_aechars(&mut machine, text, "Hi");
        let stack = [text, 2, 0, 0, 0, 0];
        assert_eq!(call(&mut machine, "DrawText", [font, 0x1234_5678, 0, 0], &stack), EUNSUPPORTED);
    }

    /// `STANDARD15B` answers like the 15, not like the 18: that was the defect that was here.
    #[test]
    fn standard15b_does_not_inherit_the_18b_numbers() {
        let fifteen_b = class_metrics(0x0103_0853);
        let fifteen = class_metrics(0x0103_0852);
        assert_eq!((fifteen_b.ascent, fifteen_b.descent), (fifteen.ascent, fifteen.descent));
        assert_ne!((fifteen_b.ascent, fifteen_b.descent), (17, 4));
    }
}

/// `sizeof(AEEFontInfo)` in `inc/AEEIFont.h`: `nAscent` and `nDescent`, two `int16`.
const FONT_INFO_SIZE: i32 = 4;

/// `IFONT_DRAW_TRANSPARENT`, from `inc/AEEIFont.h`: draw only the glyphs, not the cell background.
const IFONT_DRAW_TRANSPARENT: u32 = 0x0000_8000;

impl<C: CpuBackend> Machine<C> {
    /// Creates the object of a system font, recording which class it is.
    pub(super) fn create_font(&mut self, class_id: u32, out: u32) -> Result<u32, CpuError> {
        let object = self.new_object(Interface::Font)?;
        if object == 0 {
            return Ok(ENOMEMORY);
        }
        self.fonts.insert(object, class_metrics(class_id));
        if out != 0 {
            self.cpu.write_u32(out, object)?;
        }
        Ok(SUCCESS)
    }

    /// Serves `IFont`.
    pub(super) fn font_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::Font.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        let metrics = self.fonts.get(&this).copied().unwrap_or_default();
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                let remaining = self.objects.release(this);
                if remaining == 0 {
                    self.fonts.remove(&this);
                }
                remaining
            }
            // int QueryInterface(IFont *, AEECLSID, void **ppv)
            "QueryInterface" => {
                let out = self.cpu.read_reg(Reg::R2);
                if out != 0 {
                    self.cpu.write_u32(out, this)?;
                }
                SUCCESS
            }
            // int GetInfo(IFont *, AEEFontInfo *pinfo, int nSize)
            //
            // `AEEFontInfo` is two `int16`, `nAscent` and `nDescent`, at the start of the struct
            // (`inc/AEEIFont.h`). Only what fits in `nSize` is written, and a larger `nSize` is a
            // version this font does not know: `EUNSUPPORTED`, as the SDK says. It used to write
            // five fields from offset 4, a layout from the reference code that the header does not
            // have — with the SDK's own `sizeof(AEEFontInfo)`, which is 4, it wrote nothing at all.
            "GetInfo" => {
                let info = self.cpu.read_reg(Reg::R1);
                let size = self.cpu.read_reg(Reg::R2) as i32;
                if size > FONT_INFO_SIZE {
                    EUNSUPPORTED
                } else {
                    if info != 0 {
                        for (i, value) in [metrics.ascent, metrics.descent].iter().enumerate() {
                            let offset = i as i32 * 2;
                            if offset + 2 > size {
                                break;
                            }
                            self.cpu.write_mem(info + offset as u32, &value.to_le_bytes())?;
                        }
                    }
                    SUCCESS
                }
            }
            // int MeasureText(IFont *, const AECHAR *pcText, int nChars, int nMaxWidth,
            //                 int *pnFits, int *pnPixels)
            //
            // From `inc/AEEIFont.h`: `nChars` is exact — "zero-termination is not recognized" —,
            // and `pnFits` is how many of them fit in `nMaxWidth` pixels, `IFONT_MAXWIDTH` meaning
            // all. Every character is `max_char_width` wide here, so the text itself is not read.
            "MeasureText" => {
                let requested = (self.cpu.read_reg(Reg::R2) as i32).clamp(0, MAX_STRING as i32);
                let max_width = (self.cpu.read_reg(Reg::R3) as i32).max(0) as u32;
                let out_fits = self.stack_arg(0)?;
                let out_pixels = self.stack_arg(1)?;
                let width = u32::from(metrics.max_char_width);
                let fits = match width {
                    0 => requested as u32,
                    width => (requested as u32).min(max_width / width),
                };
                if out_fits != 0 {
                    self.cpu.write_u32(out_fits, fits)?;
                }
                if out_pixels != 0 {
                    self.cpu.write_u32(out_pixels, fits * width)?;
                }
                SUCCESS
            }
            // int DrawText(IFont *, IBitmap *pDst, int x, int y, const AECHAR *pcText,
            //              int nChars, NativeColor fg, NativeColor bg, const AEERect *prcClip,
            //              uint32 dwFlags)
            //
            // The layout is `INHERIT_IFont` in `inc/AEEIFont.h`. It used to be read as
            // `(pcText, nChars, x, y)`, a guess, because no game calls it — in the 2026-11-25
            // survey none does. `(x, y)` is the top-left of the first character cell; the cell is
            // `max_char_width` per character by ascent plus descent, and its background is filled
            // unless `IFONT_DRAW_TRANSPARENT` is set. Everything is cut by `prcClip`, and a
            // negative width or height there draws nothing.
            "DrawText" => self.font_draw_text(metrics)?,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
    /// `IFont::DrawText`, with the arguments in the SDK's order. See the comment in
    /// [`Machine::font_call`].
    fn font_draw_text(&mut self, metrics: Metrics) -> Result<u32, CpuError> {
        let destination = self.cpu.read_reg(Reg::R1);
        let x = self.cpu.read_reg(Reg::R2) as i32;
        let y = self.cpu.read_reg(Reg::R3) as i32;
        let text = self.stack_arg(0)?;
        let requested = (self.stack_arg(1)? as i32).clamp(0, MAX_STRING as i32) as usize;
        let foreground = self.stack_arg(2)? as u16;
        let background = self.stack_arg(3)? as u16;
        let clip = self.read_rect(self.stack_arg(4)?)?;
        let flags = self.stack_arg(5)?;
        // `NativeColor` is the surface's own RGB565 here, so a destination that is not one of our
        // surfaces is one whose format we do not know.
        if !self.bitmaps.contains_key(&destination) {
            return Ok(EUNSUPPORTED);
        }
        if clip.is_some_and(|clip| clip.width < 0 || clip.height < 0) {
            return Ok(SUCCESS);
        }
        // Exactly `nChars`: the SDK does not stop at a terminator.
        let mut bytes = vec![0u8; requested * 2];
        if text != 0 {
            self.cpu.read_mem(text, &mut bytes)?;
        }
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        if flags & IFONT_DRAW_TRANSPARENT == 0 {
            let cell = Rect {
                x: x as i16,
                y: y as i16,
                width: (requested as u32 * u32::from(metrics.max_char_width)).min(i16::MAX as u32)
                    as i16,
                height: (metrics.ascent + metrics.descent) as i16,
            };
            if let Some(area) = clip_rect(clip, cell)
                && let Some(surface) = self.bitmaps.get_mut(&destination)
            {
                surface.fill_rect_native(area, background);
            }
        }
        self.record_drawn_text(&text, x, y);
        // The glyphs go through the same writer as `IDISPLAY_DrawText`, pointed at `pDst` and at
        // `prcClip` for this call only.
        let target = std::mem::replace(&mut self.display_target, destination);
        let previous_clip = std::mem::replace(&mut self.clip, clip);
        let written = self.write(&text, x, y, Rgb::from_rgb565(foreground));
        self.display_target = target;
        self.clip = previous_clip;
        written?;
        Ok(SUCCESS)
    }
}
