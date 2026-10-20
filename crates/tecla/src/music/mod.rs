//! Small, allocation-free musical building blocks for instrument programs.

pub mod autochord;
pub mod harmony;
pub mod realization;
pub mod theory;
pub mod tonnetz;
pub mod voicing;

pub use theory::{
    Chord, ChordKind, ChordQuality, Degree, HarmonicFunction, Key, Mode, PitchClassSet, ToneRole,
};
pub use voicing::{
    harmonize, nudge, solve, HarmonicIntent, HarmonyPolicy, Motion, Solution, Voicing,
    VoicingPreference,
};
