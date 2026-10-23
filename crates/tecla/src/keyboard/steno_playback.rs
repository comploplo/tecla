//! Steno translation playback engine.
//!
//! [`StenoPlayback`] interprets the opcode stream produced by the DAFSA
//! dictionary and converts it into keystrokes via the [`KeyOutput`] trait.
//! This separation allows the playback engine to be tested independently
//! of the full keyboard processor.

use crate::action::{KeyCode, ModifierKey};
use crate::devices::steno::{
    read_compact_u16_pub, CaseMode, OP_HOST_CHORD, OP_NOOP, OP_NO_SPACE,
    OP_REPEAT_LAST_TRANSLATION, OP_RESET_FORMATTING, OP_RETRO_CASE, OP_SET_CASE, OP_SET_SPACE,
    OP_SPACE, OP_STITCH, OP_TEXT, OP_UNDO_LAST_TRANSLATION,
};
use crate::pipeline::report::HidReport;
use crate::telemetry::steno::StenoTelemetry;
use core::convert::TryFrom;
use heapless::Vec;

/// Keystroke output interface consumed by the steno playback engine.
///
/// Generic over `F` to support both concrete closures and `dyn FnMut` without
/// requiring `Sized` at every call site.
pub trait KeyOutput<F: FnMut(HidReport) + ?Sized> {
    /// Register a key press and emit the resulting HID report.
    fn press_keycode(&mut self, kc: KeyCode, emit: &mut F);
    /// Register a key release and emit the resulting HID report.
    fn release_keycode(&mut self, kc: KeyCode, emit: &mut F);
    /// Activate a modifier key (report is not emitted until [`flush_keyboard`](KeyOutput::flush_keyboard)).
    fn press_modifier(&mut self, m: ModifierKey);
    /// Deactivate a modifier key.
    fn release_modifier(&mut self, m: ModifierKey);
    /// Emit the current keyboard state as an HID report.
    fn flush_keyboard(&self, emit: &mut F);
    /// Drain any pending keycodes queued by autocorrect or macro playback.
    fn drain_playback_queue(&mut self, emit: &mut F);
}

/// Steno translation opcode interpreter.
pub struct StenoPlayback {
    pub(super) needs_space: bool,
    pub(super) pending_case: Option<CaseMode>,
    pub(super) last_translation: Vec<u8, 512>,
    pub(super) visible_backspaces: usize,
    pub(super) last_needs_space: bool,
    pub(super) last_retro_case: Option<CaseMode>,
}

struct TextOpcodeState {
    needs_space: bool,
    pending_case: Option<CaseMode>,
    visible_backspaces: usize,
    saw_visible_output: bool,
}

struct TextOpcodeContext<'a, F: FnMut(HidReport) + ?Sized, O: KeyOutput<F> + ?Sized> {
    translation: &'a [u8],
    output: &'a mut O,
    emit: &'a mut F,
}

impl Default for StenoPlayback {
    fn default() -> Self {
        Self::new()
    }
}

impl StenoPlayback {
    /// Create a new playback engine with default (no-space, no-case) state.
    pub const fn new() -> Self {
        Self {
            needs_space: false,
            pending_case: None,
            last_translation: Vec::new(),
            visible_backspaces: 0,
            last_needs_space: false,
            last_retro_case: None,
        }
    }

    /// Play a steno translation opcode stream, emitting keystrokes via `output`.
    pub fn play_translation<F: FnMut(HidReport) + ?Sized>(
        &mut self,
        translation: &[u8],
        telemetry: Option<&mut StenoTelemetry>,
        output: &mut impl KeyOutput<F>,
        emit: &mut F,
    ) {
        self.play_translation_with_case(translation, None, telemetry, output, emit);
    }

    /// Play a translation with an optional case override (used by retro-case).
    pub fn play_translation_with_case<F: FnMut(HidReport) + ?Sized>(
        &mut self,
        translation: &[u8],
        retro_case: Option<CaseMode>,
        mut telemetry: Option<&mut StenoTelemetry>,
        output: &mut impl KeyOutput<F>,
        emit: &mut F,
    ) {
        let record_last = !matches!(
            translation.first().copied(),
            Some(OP_REPEAT_LAST_TRANSLATION | OP_UNDO_LAST_TRANSLATION | OP_RETRO_CASE)
        );

        let mut idx = 0;
        let starting_needs_space = self.needs_space;
        let mut text_state = TextOpcodeState {
            needs_space: starting_needs_space,
            pending_case: retro_case.or(self.pending_case),
            visible_backspaces: 0,
            saw_visible_output: false,
        };

        while idx < translation.len() {
            let Some(&opcode) = translation.get(idx) else {
                break;
            };
            match opcode {
                OP_TEXT => {
                    let mut ctx = TextOpcodeContext {
                        translation,
                        output,
                        emit,
                    };
                    let Some(next_idx) =
                        self.handle_text_opcode(idx, false, &mut text_state, &mut ctx)
                    else {
                        break;
                    };
                    idx = next_idx;
                }
                OP_STITCH => {
                    let mut ctx = TextOpcodeContext {
                        translation,
                        output,
                        emit,
                    };
                    let Some(next_idx) =
                        self.handle_text_opcode(idx, true, &mut text_state, &mut ctx)
                    else {
                        break;
                    };
                    idx = next_idx;
                }
                OP_HOST_CHORD => {
                    let Some(next_idx) =
                        self.handle_host_chord(translation, idx, output, emit, &mut text_state)
                    else {
                        break;
                    };
                    idx = next_idx;
                }
                OP_NO_SPACE => {
                    text_state.needs_space = false;
                    idx += 1;
                }
                OP_SPACE | OP_SET_SPACE => {
                    text_state.needs_space = true;
                    idx += 1;
                }
                OP_SET_CASE => {
                    let Some(next_idx) =
                        self.handle_set_case(translation, idx, &mut text_state.pending_case)
                    else {
                        break;
                    };
                    idx = next_idx;
                }
                OP_RESET_FORMATTING => {
                    self.reset_state();
                    text_state.needs_space = false;
                    text_state.pending_case = None;
                    idx += 1;
                }
                OP_REPEAT_LAST_TRANSLATION => {
                    let last = self.last_translation.clone();
                    self.play_translation_with_case(
                        last.as_slice(),
                        self.last_retro_case,
                        telemetry.as_deref_mut(),
                        output,
                        emit,
                    );
                    idx += 1;
                }
                OP_RETRO_CASE => {
                    let Some(()) = self.handle_retro_case(
                        translation,
                        idx,
                        telemetry.as_deref_mut(),
                        output,
                        emit,
                    ) else {
                        break;
                    };
                    return;
                }
                OP_UNDO_LAST_TRANSLATION => {
                    if let Some(ref mut t) = telemetry {
                        t.record_undo();
                    }
                    self.undo_last_translation(self.visible_backspaces, output, emit);
                    idx += 1;
                }
                OP_NOOP => {
                    idx += 1;
                }
                _ => break,
            }
        }

        if record_last && text_state.saw_visible_output {
            self.last_translation.clear();
            let _ = self.last_translation.extend_from_slice(translation);
            self.visible_backspaces = text_state.visible_backspaces;
            self.last_needs_space = starting_needs_space;
            self.last_retro_case = retro_case;
        }

        self.needs_space = text_state.needs_space;
        self.pending_case = text_state.pending_case;
    }

    fn handle_text_opcode<F: FnMut(HidReport) + ?Sized, O: KeyOutput<F>>(
        &mut self,
        idx: usize,
        stitch: bool,
        state: &mut TextOpcodeState,
        ctx: &mut TextOpcodeContext<'_, F, O>,
    ) -> Option<usize> {
        let operand_idx = idx.checked_add(1)?;
        let translation = ctx.translation;
        let (len, len_size) = read_compact_u16_pub(translation, operand_idx)?;
        let data_start = idx.checked_add(1)?.checked_add(len_size)?;
        let data_end = data_start.checked_add(len)?;
        let bytes = translation.get(data_start..data_end)?;
        let text = core::str::from_utf8(bytes).ok()?;
        let preserve_needs_space = state.needs_space;
        let (typed, remaining_case) = Self::play_text(
            text,
            if stitch { false } else { state.needs_space },
            state.pending_case,
            ctx.output,
            ctx.emit,
        );
        state.visible_backspaces = state.visible_backspaces.saturating_add(typed);
        state.saw_visible_output |= typed > 0;
        state.pending_case = remaining_case;
        state.needs_space = if stitch {
            preserve_needs_space
        } else {
            text.chars()
                .last()
                .is_some_and(|ch| !matches!(ch, ' ' | '\n' | '\t'))
        };
        Some(data_end)
    }

    fn handle_host_chord<F: FnMut(HidReport) + ?Sized>(
        &mut self,
        translation: &[u8],
        idx: usize,
        output: &mut impl KeyOutput<F>,
        emit: &mut F,
        state: &mut TextOpcodeState,
    ) -> Option<usize> {
        let mods = *translation.get(idx.checked_add(1)?)?;
        let uid = *translation.get(idx.checked_add(2)?)?;
        output.drain_playback_queue(emit);
        if let Some(kc) = KeyCode::from_usage_id(uid) {
            let modifier = ModifierKey::from_bits(mods);
            output.press_modifier(modifier);
            output.flush_keyboard(emit);
            output.press_keycode(kc, emit);
            output.release_keycode(kc, emit);
            output.release_modifier(modifier);
            output.flush_keyboard(emit);
            if kc != KeyCode::Space {
                state.needs_space = true;
            }
            state.saw_visible_output = true;
        }
        idx.checked_add(3)
    }

    fn handle_set_case(
        &mut self,
        translation: &[u8],
        idx: usize,
        pending_case: &mut Option<CaseMode>,
    ) -> Option<usize> {
        let mode = translation
            .get(idx.checked_add(1)?)
            .copied()
            .and_then(CaseMode::from_byte)?;
        *pending_case = Some(mode);
        idx.checked_add(2)
    }

    fn handle_retro_case<F: FnMut(HidReport) + ?Sized>(
        &mut self,
        translation: &[u8],
        idx: usize,
        telemetry: Option<&mut StenoTelemetry>,
        output: &mut impl KeyOutput<F>,
        emit: &mut F,
    ) -> Option<()> {
        let mode = translation
            .get(idx.checked_add(1)?)
            .copied()
            .and_then(CaseMode::from_byte)?;
        let last = self.last_translation.clone();
        self.undo_last_translation(self.visible_backspaces, output, emit);
        self.play_translation_with_case(last.as_slice(), Some(mode), telemetry, output, emit);
        Some(())
    }

    fn play_text<F: FnMut(HidReport) + ?Sized>(
        text: &str,
        needs_space: bool,
        case: Option<CaseMode>,
        output: &mut impl KeyOutput<F>,
        emit: &mut F,
    ) -> (usize, Option<CaseMode>) {
        let mut typed = 0usize;
        let mut inserted_space = false;
        let mut pending_case = case;

        for ch in text.chars() {
            if !inserted_space && needs_space && matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9') {
                output.press_keycode(KeyCode::Space, emit);
                output.release_keycode(KeyCode::Space, emit);
                inserted_space = true;
                typed += 1;
            }

            let mut out_ch = ch;
            if let Some(case) = pending_case {
                match case {
                    CaseMode::CapFirstWord => {
                        if ch.is_alphabetic() {
                            out_ch = ch.to_uppercase().next().unwrap_or(ch);
                            pending_case = None;
                        }
                    }
                    CaseMode::UpperFirstWord => {
                        if ch.is_whitespace() {
                            pending_case = None;
                        } else {
                            out_ch = ch.to_uppercase().next().unwrap_or(ch);
                        }
                    }
                    CaseMode::LowerFirstChar => {
                        if ch.is_alphabetic() {
                            out_ch = ch.to_lowercase().next().unwrap_or(ch);
                            pending_case = None;
                        }
                    }
                }
            }

            if let Some((modifier, key)) = char_to_key_action(out_ch) {
                output.press_modifier(modifier);
                output.flush_keyboard(emit);
                output.press_keycode(key, emit);
                output.release_keycode(key, emit);
                output.release_modifier(modifier);
                output.flush_keyboard(emit);
                typed += 1;
            }
        }

        (typed, pending_case)
    }

    fn undo_last_translation<F: FnMut(HidReport) + ?Sized>(
        &mut self,
        visible_backspaces: usize,
        output: &mut impl KeyOutput<F>,
        emit: &mut F,
    ) {
        for _ in 0..visible_backspaces {
            output.press_keycode(KeyCode::Backspace, emit);
            output.release_keycode(KeyCode::Backspace, emit);
        }
        self.needs_space = self.last_needs_space;
        self.pending_case = None;
    }

    /// Reset all formatting state (space, case, retro-case).
    pub fn reset_state(&mut self) {
        self.needs_space = false;
        self.pending_case = None;
        self.last_retro_case = None;
    }
}

/// Map a character to the (modifier, keycode) pair needed to type it on a US layout.
pub fn char_to_key_action(ch: char) -> Option<(ModifierKey, KeyCode)> {
    match ch {
        'a'..='z' => alpha_key(ch, b'a', ModifierKey::NONE),
        'A'..='Z' => alpha_key(ch, b'A', ModifierKey::LSHIFT),
        '1' => Some((ModifierKey::NONE, KeyCode::N1)),
        '2' => Some((ModifierKey::NONE, KeyCode::N2)),
        '3' => Some((ModifierKey::NONE, KeyCode::N3)),
        '4' => Some((ModifierKey::NONE, KeyCode::N4)),
        '5' => Some((ModifierKey::NONE, KeyCode::N5)),
        '6' => Some((ModifierKey::NONE, KeyCode::N6)),
        '7' => Some((ModifierKey::NONE, KeyCode::N7)),
        '8' => Some((ModifierKey::NONE, KeyCode::N8)),
        '9' => Some((ModifierKey::NONE, KeyCode::N9)),
        '0' => Some((ModifierKey::NONE, KeyCode::N0)),
        ' ' => Some((ModifierKey::NONE, KeyCode::Space)),
        '\n' | '\r' => Some((ModifierKey::NONE, KeyCode::Enter)),
        '\t' => Some((ModifierKey::NONE, KeyCode::Tab)),
        '-' => Some((ModifierKey::NONE, KeyCode::Minus)),
        '=' => Some((ModifierKey::NONE, KeyCode::Equal)),
        '[' => Some((ModifierKey::NONE, KeyCode::LeftBrace)),
        ']' => Some((ModifierKey::NONE, KeyCode::RightBrace)),
        '\\' => Some((ModifierKey::NONE, KeyCode::Backslash)),
        ';' => Some((ModifierKey::NONE, KeyCode::Semicolon)),
        '\'' => Some((ModifierKey::NONE, KeyCode::Quote)),
        '`' => Some((ModifierKey::NONE, KeyCode::Grave)),
        ',' => Some((ModifierKey::NONE, KeyCode::Comma)),
        '.' => Some((ModifierKey::NONE, KeyCode::Dot)),
        '/' => Some((ModifierKey::NONE, KeyCode::Slash)),
        '!' => Some((ModifierKey::LSHIFT, KeyCode::N1)),
        '@' => Some((ModifierKey::LSHIFT, KeyCode::N2)),
        '#' => Some((ModifierKey::LSHIFT, KeyCode::N3)),
        '$' => Some((ModifierKey::LSHIFT, KeyCode::N4)),
        '%' => Some((ModifierKey::LSHIFT, KeyCode::N5)),
        '^' => Some((ModifierKey::LSHIFT, KeyCode::N6)),
        '&' => Some((ModifierKey::LSHIFT, KeyCode::N7)),
        '*' => Some((ModifierKey::LSHIFT, KeyCode::N8)),
        '(' => Some((ModifierKey::LSHIFT, KeyCode::N9)),
        ')' => Some((ModifierKey::LSHIFT, KeyCode::N0)),
        '_' => Some((ModifierKey::LSHIFT, KeyCode::Minus)),
        '+' => Some((ModifierKey::LSHIFT, KeyCode::Equal)),
        '{' => Some((ModifierKey::LSHIFT, KeyCode::LeftBrace)),
        '}' => Some((ModifierKey::LSHIFT, KeyCode::RightBrace)),
        '|' => Some((ModifierKey::LSHIFT, KeyCode::Backslash)),
        ':' => Some((ModifierKey::LSHIFT, KeyCode::Semicolon)),
        '"' => Some((ModifierKey::LSHIFT, KeyCode::Quote)),
        '~' => Some((ModifierKey::LSHIFT, KeyCode::Grave)),
        '<' => Some((ModifierKey::LSHIFT, KeyCode::Comma)),
        '>' => Some((ModifierKey::LSHIFT, KeyCode::Dot)),
        '?' => Some((ModifierKey::LSHIFT, KeyCode::Slash)),
        _ => None,
    }
}

fn alpha_key(ch: char, base: u8, modifier: ModifierKey) -> Option<(ModifierKey, KeyCode)> {
    let code = u32::from(ch);
    let offset = code.checked_sub(u32::from(base))?;
    let usage = 0x04u8.checked_add(u8::try_from(offset).ok()?)?;
    KeyCode::from_usage_id(usage).map(|kc| (modifier, kc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{KeyCode, ModifierKey};
    use crate::devices::steno::CaseMode;
    use crate::pipeline::report::HidReport;
    use heapless::Vec as HVec;

    struct MockOutput {
        keys: HVec<(KeyCode, bool), 1024>,
        modifiers: HVec<(ModifierKey, bool), 64>,
        drains: u32,
    }

    impl MockOutput {
        fn new() -> Self {
            Self {
                keys: HVec::new(),
                modifiers: HVec::new(),
                drains: 0,
            }
        }
    }

    impl<F: FnMut(HidReport) + ?Sized> KeyOutput<F> for MockOutput {
        fn press_keycode(&mut self, kc: KeyCode, _emit: &mut F) {
            let _ = self.keys.push((kc, true));
        }

        fn release_keycode(&mut self, kc: KeyCode, _emit: &mut F) {
            let _ = self.keys.push((kc, false));
        }

        fn press_modifier(&mut self, m: ModifierKey) {
            let _ = self.modifiers.push((m, true));
        }

        fn release_modifier(&mut self, m: ModifierKey) {
            let _ = self.modifiers.push((m, false));
        }

        fn flush_keyboard(&self, _emit: &mut F) {}

        fn drain_playback_queue(&mut self, _emit: &mut F) {
            self.drains += 1;
        }
    }

    fn text_bytes(text: &str) -> HVec<u8, 64> {
        let mut v = HVec::new();
        let _ = v.push(OP_TEXT);
        push_compact_len(&mut v, text.len());
        let _ = v.extend_from_slice(text.as_bytes());
        v
    }

    fn stitch_bytes(text: &str) -> HVec<u8, 64> {
        let mut v = HVec::new();
        let _ = v.push(OP_STITCH);
        push_compact_len(&mut v, text.len());
        let _ = v.extend_from_slice(text.as_bytes());
        v
    }

    fn push_compact_len(v: &mut HVec<u8, 64>, len: usize) {
        if len < 255 {
            let _ = v.push(len as u8);
        } else {
            let _ = v.push(0xFF);
            let _ = v.push((len & 0xFF) as u8);
            let _ = v.push(((len >> 8) & 0xFF) as u8);
        }
    }

    fn typed_keys(output: &MockOutput) -> HVec<KeyCode, 128> {
        let mut out = HVec::new();
        for &(kc, pressed) in &output.keys {
            if pressed {
                let _ = out.push(kc);
            }
        }
        out
    }

    #[test]
    fn play_simple_text() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let translation = text_bytes("the");
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});

        let keys = typed_keys(&output);
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], KeyCode::T);
        assert_eq!(keys[1], KeyCode::H);
        assert_eq!(keys[2], KeyCode::E);
    }

    #[test]
    fn play_text_with_space() {
        let mut playback = StenoPlayback::new();
        playback.needs_space = true;
        let mut output = MockOutput::new();
        let translation = text_bytes("cat");
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});

        let keys = typed_keys(&output);
        assert_eq!(keys[0], KeyCode::Space);
        assert_eq!(keys[1], KeyCode::C);
    }

    #[test]
    fn normal_text_marks_next_word_for_space() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let translation = text_bytes("cat");
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});

        assert!(playback.needs_space);
    }

    #[test]
    fn text_ending_in_space_does_not_mark_next_word_for_space() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let translation = text_bytes("cat ");
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});

        assert!(!playback.needs_space);
    }

    #[test]
    fn stitch_suppresses_space() {
        let mut playback = StenoPlayback::new();
        playback.needs_space = true;
        let mut output = MockOutput::new();
        let translation = stitch_bytes("ing");
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});

        let keys = typed_keys(&output);
        assert_eq!(keys[0], KeyCode::I);
    }

    #[test]
    fn cap_first_word_capitalizes() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let mut translation: HVec<u8, 64> = HVec::new();
        let _ = translation.push(OP_SET_CASE);
        let _ = translation.push(CaseMode::CapFirstWord as u8);
        let _ = translation.extend_from_slice(text_bytes("hello").as_slice());
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});

        assert!(output
            .modifiers
            .iter()
            .any(|&(m, pressed)| pressed && m == ModifierKey::LSHIFT));
    }

    #[test]
    fn undo_emits_backspaces() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let translation = text_bytes("the");
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});
        assert_eq!(playback.visible_backspaces, 3);

        let mut output2 = MockOutput::new();
        let mut undo: HVec<u8, 4> = HVec::new();
        let _ = undo.push(OP_UNDO_LAST_TRANSLATION);
        playback.play_translation(undo.as_slice(), None, &mut output2, &mut |_| {});

        let bs_count = typed_keys(&output2)
            .iter()
            .filter(|&&kc| kc == KeyCode::Backspace)
            .count();
        assert_eq!(bs_count, 3);
    }

    #[test]
    fn reset_clears_state() {
        let mut playback = StenoPlayback::new();
        playback.needs_space = true;
        playback.pending_case = Some(CaseMode::CapFirstWord);
        playback.last_retro_case = Some(CaseMode::UpperFirstWord);
        playback.reset_state();
        assert!(!playback.needs_space);
        assert!(playback.pending_case.is_none());
        assert!(playback.last_retro_case.is_none());
    }

    #[test]
    fn char_to_key_maps_letter_case_to_shift() {
        assert_eq!(
            char_to_key_action('a'),
            Some((ModifierKey::NONE, KeyCode::A))
        );
        assert_eq!(
            char_to_key_action('Z'),
            Some((ModifierKey::LSHIFT, KeyCode::Z))
        );
    }

    #[test]
    fn no_space_opcode_suppresses() {
        let mut playback = StenoPlayback::new();
        playback.needs_space = true;
        let mut output = MockOutput::new();
        let mut translation: HVec<u8, 64> = HVec::new();
        let _ = translation.push(OP_NO_SPACE);
        let _ = translation.extend_from_slice(text_bytes("x").as_slice());
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});

        let keys = typed_keys(&output);
        assert_eq!(keys[0], KeyCode::X);
    }

    #[test]
    fn host_chord_drains_queue_first() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let mut translation: HVec<u8, 64> = HVec::new();
        let _ = translation.push(OP_HOST_CHORD);
        let _ = translation.push(0x00);
        let _ = translation.push(KeyCode::A.usage_id());
        playback.play_translation(translation.as_slice(), None, &mut output, &mut |_| {});

        assert_eq!(output.drains, 1);
    }

    // ── Formatting and history opcodes ───────────────────────────────────────

    fn op(bytes: &[u8]) -> HVec<u8, 64> {
        let mut v = HVec::new();
        let _ = v.extend_from_slice(bytes);
        v
    }

    fn concat(parts: &[&[u8]]) -> HVec<u8, 64> {
        let mut v = HVec::new();
        for p in parts {
            let _ = v.extend_from_slice(p);
        }
        v
    }

    #[test]
    fn set_case_capitalizes_the_next_word() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let t = concat(&[
            &op(&[OP_SET_CASE, CaseMode::CapFirstWord as u8]),
            text_bytes("the").as_slice(),
        ]);
        playback.play_translation(t.as_slice(), None, &mut output, &mut |_| {});

        // A capital T means a shift was held around the first keystroke.
        assert_eq!(
            typed_keys(&output).as_slice(),
            &[KeyCode::T, KeyCode::H, KeyCode::E]
        );
        assert!(
            output
                .modifiers
                .iter()
                .any(|&(m, pressed)| m == ModifierKey::LSHIFT && pressed),
            "CapFirstWord did not shift the first character"
        );
    }

    #[test]
    fn set_case_upper_shifts_the_whole_first_word() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let t = concat(&[
            &op(&[OP_SET_CASE, CaseMode::UpperFirstWord as u8]),
            text_bytes("the").as_slice(),
        ]);
        playback.play_translation(t.as_slice(), None, &mut output, &mut |_| {});

        let shifts = output
            .modifiers
            .iter()
            .filter(|&&(m, pressed)| m == ModifierKey::LSHIFT && pressed)
            .count();
        assert_eq!(shifts, 3, "UpperFirstWord should shift all three letters");
    }

    #[test]
    fn set_case_with_an_unknown_mode_byte_stops_playback() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let t = concat(&[&op(&[OP_SET_CASE, 0x7F]), text_bytes("the").as_slice()]);
        playback.play_translation(t.as_slice(), None, &mut output, &mut |_| {});
        assert!(
            typed_keys(&output).is_empty(),
            "unknown case mode should abort, not type"
        );
    }

    #[test]
    fn noop_opcode_advances_without_output() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let t = concat(&[
            &op(&[OP_NOOP]),
            text_bytes("hi").as_slice(),
            &op(&[OP_NOOP]),
        ]);
        playback.play_translation(t.as_slice(), None, &mut output, &mut |_| {});
        assert_eq!(typed_keys(&output).as_slice(), &[KeyCode::H, KeyCode::I]);
    }

    #[test]
    fn unknown_opcode_stops_playback() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let t = concat(&[
            text_bytes("hi").as_slice(),
            &op(&[0xEE]),
            text_bytes("no").as_slice(),
        ]);
        playback.play_translation(t.as_slice(), None, &mut output, &mut |_| {});
        assert_eq!(
            typed_keys(&output).as_slice(),
            &[KeyCode::H, KeyCode::I],
            "playback should stop at the unknown opcode"
        );
    }

    #[test]
    fn reset_formatting_clears_pending_space() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();

        // "hi" leaves needs_space set; RESET_FORMATTING must clear it so the
        // next word is not prefixed with a space.
        playback.play_translation(text_bytes("hi").as_slice(), None, &mut output, &mut |_| {});
        let mut out2 = MockOutput::new();
        let t = concat(&[&op(&[OP_RESET_FORMATTING]), text_bytes("yo").as_slice()]);
        playback.play_translation(t.as_slice(), None, &mut out2, &mut |_| {});

        assert_eq!(
            typed_keys(&out2).as_slice(),
            &[KeyCode::Y, KeyCode::O],
            "no leading space expected after RESET_FORMATTING"
        );
    }

    #[test]
    fn repeat_last_translation_types_the_previous_word_again() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        playback.play_translation(text_bytes("hi").as_slice(), None, &mut output, &mut |_| {});

        let mut out2 = MockOutput::new();
        playback.play_translation(
            op(&[OP_REPEAT_LAST_TRANSLATION]).as_slice(),
            None,
            &mut out2,
            &mut |_| {},
        );
        assert_eq!(
            typed_keys(&out2).as_slice(),
            &[KeyCode::Space, KeyCode::H, KeyCode::I],
            "repeat should retype the last translation"
        );
    }

    #[test]
    fn undo_last_translation_backspaces_exactly_what_was_typed() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        playback.play_translation(text_bytes("cat").as_slice(), None, &mut output, &mut |_| {});
        assert_eq!(typed_keys(&output).len(), 3);

        let mut out2 = MockOutput::new();
        playback.play_translation(
            op(&[OP_UNDO_LAST_TRANSLATION]).as_slice(),
            None,
            &mut out2,
            &mut |_| {},
        );
        let keys = typed_keys(&out2);
        assert_eq!(
            keys.len(),
            3,
            "expected one backspace per visible character"
        );
        assert!(keys.iter().all(|&k| k == KeyCode::Backspace));
    }

    #[test]
    fn undo_records_a_telemetry_undo() {
        use crate::telemetry::TelemetryExt;
        let mut playback = StenoPlayback::new();
        let mut telem = StenoTelemetry::default();
        let mut output = MockOutput::new();
        playback.play_translation(text_bytes("cat").as_slice(), None, &mut output, &mut |_| {});
        playback.play_translation(
            op(&[OP_UNDO_LAST_TRANSLATION]).as_slice(),
            Some(&mut telem),
            &mut output,
            &mut |_| {},
        );
        assert_eq!(telem.collect().undos, 1);
    }

    #[test]
    fn retro_case_rewrites_the_previous_word_capitalized() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        playback.play_translation(text_bytes("cat").as_slice(), None, &mut output, &mut |_| {});

        let mut out2 = MockOutput::new();
        playback.play_translation(
            op(&[OP_RETRO_CASE, CaseMode::CapFirstWord as u8]).as_slice(),
            None,
            &mut out2,
            &mut |_| {},
        );

        let keys = typed_keys(&out2);
        let backspaces = keys.iter().filter(|&&k| k == KeyCode::Backspace).count();
        assert_eq!(backspaces, 3, "should erase the three characters first");
        assert_eq!(
            &keys[backspaces..],
            &[KeyCode::C, KeyCode::A, KeyCode::T],
            "then retype the word"
        );
        assert!(
            out2.modifiers
                .iter()
                .any(|&(m, pressed)| m == ModifierKey::LSHIFT && pressed),
            "retyped word should be capitalized"
        );
    }

    #[test]
    fn retro_case_with_an_unknown_mode_byte_is_a_no_op() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        playback.play_translation(text_bytes("cat").as_slice(), None, &mut output, &mut |_| {});

        let mut out2 = MockOutput::new();
        playback.play_translation(
            op(&[OP_RETRO_CASE, 0x7F]).as_slice(),
            None,
            &mut out2,
            &mut |_| {},
        );
        assert!(typed_keys(&out2).is_empty());
    }

    #[test]
    fn truncated_set_case_at_end_of_stream_does_not_panic() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        playback.play_translation(
            op(&[OP_SET_CASE]).as_slice(),
            None,
            &mut output,
            &mut |_| {},
        );
        assert!(typed_keys(&output).is_empty());
    }

    #[test]
    fn reset_state_clears_case_and_space() {
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        playback.play_translation(text_bytes("hi").as_slice(), None, &mut output, &mut |_| {});
        playback.reset_state();

        let mut out2 = MockOutput::new();
        playback.play_translation(text_bytes("yo").as_slice(), None, &mut out2, &mut |_| {});
        assert_eq!(typed_keys(&out2).as_slice(), &[KeyCode::Y, KeyCode::O]);
    }

    #[test]
    fn char_to_key_action_covers_every_printable_ascii() {
        // Every printable ASCII character must be typeable, and the shifted
        // pairs must agree on the physical key (e.g. '!' is shift+'1').
        for byte in 0x20u8..0x7F {
            let ch = char::from(byte);
            assert!(
                char_to_key_action(ch).is_some(),
                "no key mapping for {ch:?} (0x{byte:02X})"
            );
        }

        let shift_pairs = [
            ('!', '1'),
            ('@', '2'),
            ('#', '3'),
            ('$', '4'),
            ('%', '5'),
            ('^', '6'),
            ('&', '7'),
            ('*', '8'),
            ('(', '9'),
            (')', '0'),
            ('_', '-'),
            ('+', '='),
            ('{', '['),
            ('}', ']'),
            ('|', '\\'),
            (':', ';'),
            ('"', '\''),
            ('~', '`'),
            ('<', ','),
            ('>', '.'),
            ('?', '/'),
        ];
        for (shifted, plain) in shift_pairs {
            let (smod, skey) = char_to_key_action(shifted).expect("shifted");
            let (pmod, pkey) = char_to_key_action(plain).expect("plain");
            assert_eq!(
                skey, pkey,
                "{shifted:?} and {plain:?} are the same physical key"
            );
            assert_eq!(smod, ModifierKey::LSHIFT, "{shifted:?} needs shift");
            assert_eq!(pmod, ModifierKey::NONE, "{plain:?} needs no modifier");
        }
    }

    #[test]
    fn char_to_key_action_handles_whitespace_and_rejects_non_ascii() {
        assert_eq!(
            char_to_key_action('\n'),
            Some((ModifierKey::NONE, KeyCode::Enter))
        );
        assert_eq!(
            char_to_key_action('\r'),
            Some((ModifierKey::NONE, KeyCode::Enter))
        );
        assert_eq!(
            char_to_key_action('\t'),
            Some((ModifierKey::NONE, KeyCode::Tab))
        );
        assert_eq!(
            char_to_key_action(' '),
            Some((ModifierKey::NONE, KeyCode::Space))
        );
        assert_eq!(char_to_key_action('\u{7F}'), None);
        assert_eq!(char_to_key_action('é'), None);
        assert_eq!(char_to_key_action('→'), None);
    }

    #[test]
    fn uppercase_letters_type_with_shift() {
        let (m, k) = char_to_key_action('Z').expect("Z");
        assert_eq!(m, ModifierKey::LSHIFT);
        assert_eq!(k, char_to_key_action('z').expect("z").1);
    }

    #[test]
    fn long_text_uses_the_two_byte_length_encoding() {
        // Lengths >= 255 switch to the 0xFF + u16 form; make sure a long
        // translation still plays back in full.
        let mut playback = StenoPlayback::new();
        let mut output = MockOutput::new();
        let text = "a".repeat(300);
        let mut v: HVec<u8, 512> = HVec::new();
        let _ = v.push(OP_TEXT);
        let _ = v.push(0xFF);
        let _ = v.push((300 & 0xFF) as u8);
        let _ = v.push(((300 >> 8) & 0xFF) as u8);
        let _ = v.extend_from_slice(text.as_bytes());

        playback.play_translation(v.as_slice(), None, &mut output, &mut |_| {});
        let pressed = output.keys.iter().filter(|&&(_, p)| p).count();
        assert_eq!(pressed, 300, "all 300 characters should be typed");
    }
}
