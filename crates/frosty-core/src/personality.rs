//! Personality engine: events in, animation and microcopy out
//! (docs/PROJECT_SPEC.md section 7.6, docs/DESIGN_SPEC.md section 8).
//!
//! Rule-based, no AI. Time is passed in so cooldowns are testable.

use std::collections::HashMap;

use serde::Deserialize;

use crate::{
    config::{ReactionLevel, Tone},
    events::{Event, pattern_matches},
};

pub const REACTION_COOLDOWN_MS: u64 = 2_000;
pub const CELEBRATE_COOLDOWN_MS: u64 = 10_000;

/// Marker for "use the event's own name as the microcopy key".
const EVENT_KEY: &str = "{event}";

#[derive(Debug, Clone, PartialEq)]
pub struct ReactionRule {
    /// Event pattern, see `events::pattern_matches`.
    pub on: String,
    pub animation: String,
    pub intensity: f32,
    pub microcopy: Option<String>,
    /// Whether the speech chip still shows in `subtle` mode (and, for chip
    /// rules, in `off` mode as a static chip).
    pub chip_in_subtle: bool,
}

impl ReactionRule {
    fn core(
        on: &str,
        animation: &str,
        intensity: f32,
        microcopy: Option<&str>,
        chip: bool,
    ) -> Self {
        Self {
            on: on.into(),
            animation: animation.into(),
            intensity,
            microcopy: microcopy.map(str::to_string),
            chip_in_subtle: chip,
        }
    }
}

/// Manifest intensity: a number from 0 to 1, or a named level.
pub fn named_intensity(name: &str) -> Option<f32> {
    match name {
        "small" => Some(0.3),
        "medium" => Some(0.5),
        "large" => Some(0.8),
        _ => None,
    }
}

/// The core defaults from docs/DESIGN_SPEC.md 8.1.
pub fn core_rules() -> Vec<ReactionRule> {
    use ReactionRule as R;
    vec![
        R::core(
            "core.first_summon_today",
            "wave",
            0.4,
            Some("summon.first_today"),
            false,
        ),
        R::core(
            "*.streak_milestone",
            "celebrate",
            0.8,
            Some("streak.milestone"),
            true,
        ),
        R::core("*.created", "happy_small", 0.3, Some(EVENT_KEY), false),
        R::core("*.completed", "celebrate", 0.3, Some(EVENT_KEY), false),
        R::core(
            "core.setting_saved",
            "happy_small",
            0.2,
            Some("setting.saved"),
            false,
        ),
        R::core(
            "core.empty_state",
            "curious_tilt",
            0.3,
            Some("empty.generic"),
            false,
        ),
        R::core("*.due", "nudge", 0.6, Some("reminder.due"), true),
        R::core("core.dnd_enabled", "sleepy", 0.5, Some("dnd.on"), false),
        R::core("core.error", "oops", 0.5, Some("error.generic"), true),
        R::core(
            "core.onboarding_finished",
            "celebrate",
            0.5,
            Some("onboarding.done"),
            true,
        ),
        R::core("core.quit", "bye", 0.4, Some("quit"), false),
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reaction {
    /// `None` means a static chip with no animation (reaction level `off`).
    pub animation: Option<String>,
    pub intensity: f32,
    pub microcopy_key: Option<String>,
    pub text: Option<String>,
    pub show_chip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    NoReaction,
    UserOverride,
    ReactionsOff,
    Cooldown,
    CelebrateCooldown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Play(Reaction),
    Dropped(DropReason),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum VariantDef {
    Plain(String),
    Weighted { text: String, weight: u32 },
}

/// Microcopy keyed by tone then key, each key holding weighted variants.
#[derive(Debug, Clone, Default)]
pub struct Microcopy {
    tones: HashMap<String, HashMap<String, Vec<(String, u32)>>>,
}

impl Microcopy {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let raw: HashMap<String, HashMap<String, Vec<VariantDef>>> = serde_json::from_str(json)?;
        let tones = raw
            .into_iter()
            .map(|(tone, keys)| {
                let keys = keys
                    .into_iter()
                    .map(|(key, variants)| {
                        let variants = variants
                            .into_iter()
                            .map(|v| match v {
                                VariantDef::Plain(text) => (text, 1),
                                VariantDef::Weighted { text, weight } => (text, weight.max(1)),
                            })
                            .collect();
                        (key, variants)
                    })
                    .collect();
                (tone, keys)
            })
            .collect();
        Ok(Self { tones })
    }

    /// The pack shipped in the app.
    pub fn core_en() -> Self {
        Self::from_json(include_str!("../microcopy/en.json")).expect("core microcopy must be valid")
    }

    /// Keys in `other` replace the same keys here. Used for user overrides.
    pub fn merge(&mut self, other: Microcopy) {
        for (tone, keys) in other.tones {
            self.tones.entry(tone).or_default().extend(keys);
        }
    }

    fn variants(&self, tone: Tone, key: &str) -> Option<&Vec<(String, u32)>> {
        let tone_name = match tone {
            Tone::Casual => "casual",
            Tone::Neutral => "neutral",
        };
        // Fall back to neutral so a pack with only one tone still works.
        self.tones
            .get(tone_name)
            .and_then(|k| k.get(key))
            .or_else(|| self.tones.get("neutral").and_then(|k| k.get(key)))
    }
}

fn substitute(template: &str, payload: &serde_json::Value) -> String {
    let mut out = template.to_string();
    if let Some(map) = payload.as_object() {
        for (key, value) in map {
            let text = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => continue,
            };
            out = out.replace(&format!("{{{key}}}"), &text);
        }
    }
    out
}

pub struct PersonalityEngine {
    tone: Tone,
    level: ReactionLevel,
    never_react: Vec<String>,
    manifest_rules: Vec<ReactionRule>,
    core_rules: Vec<ReactionRule>,
    microcopy: Microcopy,
    last_reaction_ms: Option<u64>,
    last_celebrate_ms: Option<u64>,
    last_variant: HashMap<String, usize>,
    rng: u64,
}

impl PersonalityEngine {
    pub fn new(tone: Tone, level: ReactionLevel, microcopy: Microcopy) -> Self {
        Self {
            tone,
            level,
            never_react: Vec::new(),
            manifest_rules: Vec::new(),
            core_rules: core_rules(),
            microcopy,
            last_reaction_ms: None,
            last_celebrate_ms: None,
            last_variant: HashMap::new(),
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn set_tone(&mut self, tone: Tone) {
        self.tone = tone;
    }

    pub fn set_level(&mut self, level: ReactionLevel) {
        self.level = level;
    }

    pub fn set_never_react(&mut self, patterns: Vec<String>) {
        self.never_react = patterns;
    }

    pub fn set_manifest_rules(&mut self, rules: Vec<ReactionRule>) {
        self.manifest_rules = rules;
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.rng = seed.max(1);
    }

    fn next_random(&mut self) -> u64 {
        // xorshift64: plenty for choosing between a few phrases.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn pick_text(&mut self, key: &str, payload: &serde_json::Value) -> Option<String> {
        let variants = self.microcopy.variants(self.tone, key)?.clone();
        if variants.is_empty() {
            return None;
        }
        let total: u64 = variants.iter().map(|(_, w)| u64::from(*w)).sum();
        let mut roll = self.next_random() % total;
        let mut index = 0;
        for (i, (_, weight)) in variants.iter().enumerate() {
            let weight = u64::from(*weight);
            if roll < weight {
                index = i;
                break;
            }
            roll -= weight;
        }
        // Never repeat the previous variant for a key when there is a choice.
        if variants.len() > 1 && self.last_variant.get(key) == Some(&index) {
            index = (index + 1) % variants.len();
        }
        self.last_variant.insert(key.to_string(), index);
        Some(substitute(&variants[index].0, payload))
    }

    /// Resolution order (PROJECT_SPEC.md 11.2): user override, capability
    /// manifest, core default, none. Then reaction level, cooldowns, and tone.
    pub fn handle(&mut self, event: &Event, now_ms: u64) -> Decision {
        if self
            .never_react
            .iter()
            .any(|p| pattern_matches(p, &event.name))
        {
            return Decision::Dropped(DropReason::UserOverride);
        }

        let rule = self
            .manifest_rules
            .iter()
            .chain(self.core_rules.iter())
            .find(|r| pattern_matches(&r.on, &event.name))
            .cloned();
        let Some(rule) = rule else {
            return Decision::Dropped(DropReason::NoReaction);
        };

        if self.level == ReactionLevel::Off && !rule.chip_in_subtle {
            return Decision::Dropped(DropReason::ReactionsOff);
        }

        if let Some(last) = self.last_reaction_ms
            && now_ms.saturating_sub(last) < REACTION_COOLDOWN_MS
        {
            return Decision::Dropped(DropReason::Cooldown);
        }
        let celebrating = rule.animation == "celebrate" && self.level != ReactionLevel::Off;
        if celebrating
            && let Some(last) = self.last_celebrate_ms
            && now_ms.saturating_sub(last) < CELEBRATE_COOLDOWN_MS
        {
            return Decision::Dropped(DropReason::CelebrateCooldown);
        }

        let mut intensity = rule.intensity;
        if let Some(significance) = event.significance {
            intensity = intensity.max(significance);
        }
        let (animation, show_chip) = match self.level {
            ReactionLevel::Full => (Some(rule.animation.clone()), rule.microcopy.is_some()),
            ReactionLevel::Subtle => {
                intensity *= 0.5;
                (Some(rule.animation.clone()), rule.chip_in_subtle)
            }
            ReactionLevel::Off => (None, true),
        };

        let key = rule.microcopy.as_deref().map(|k| {
            if k == EVENT_KEY {
                event.name.clone()
            } else {
                k.to_string()
            }
        });
        let text = match (&key, show_chip) {
            (Some(key), true) => self.pick_text(key, &event.payload),
            _ => None,
        };

        self.last_reaction_ms = Some(now_ms);
        if celebrating {
            self.last_celebrate_ms = Some(now_ms);
        }

        Decision::Play(Reaction {
            animation,
            intensity: intensity.clamp(0.0, 1.0),
            microcopy_key: key,
            show_chip: show_chip && text.is_some(),
            text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn engine(tone: Tone, level: ReactionLevel) -> PersonalityEngine {
        PersonalityEngine::new(tone, level, Microcopy::core_en())
    }

    fn event(name: &str) -> Event {
        Event::new(name, json!({}))
    }

    fn play(d: Decision) -> Reaction {
        match d {
            Decision::Play(r) => r,
            Decision::Dropped(why) => panic!("expected a reaction, got {why:?}"),
        }
    }

    #[test]
    fn completed_celebrates_with_text() {
        let mut e = engine(Tone::Neutral, ReactionLevel::Full);
        let r = play(e.handle(&event("tasks.completed"), 0));
        assert_eq!(r.animation.as_deref(), Some("celebrate"));
        assert!((r.intensity - 0.3).abs() < 1e-6);
        assert!(r.show_chip);
        assert!(["Task completed.", "Marked as done."].contains(&r.text.unwrap().as_str()));
    }

    #[test]
    fn unknown_event_has_no_reaction() {
        let mut e = engine(Tone::Casual, ReactionLevel::Full);
        assert_eq!(
            e.handle(&event("tasks.moved"), 0),
            Decision::Dropped(DropReason::NoReaction)
        );
    }

    #[test]
    fn user_override_beats_everything() {
        let mut e = engine(Tone::Casual, ReactionLevel::Full);
        e.set_never_react(vec!["tasks.*".into()]);
        assert_eq!(
            e.handle(&event("tasks.completed"), 0),
            Decision::Dropped(DropReason::UserOverride)
        );
    }

    #[test]
    fn manifest_rule_beats_core_default() {
        let mut e = engine(Tone::Casual, ReactionLevel::Full);
        e.set_manifest_rules(vec![ReactionRule {
            on: "guitar.completed".into(),
            animation: "happy_small".into(),
            intensity: 0.9,
            microcopy: None,
            chip_in_subtle: false,
        }]);
        let r = play(e.handle(&event("guitar.completed"), 0));
        assert_eq!(r.animation.as_deref(), Some("happy_small"));
        assert!(!r.show_chip);
    }

    #[test]
    fn significance_raises_intensity() {
        let mut e = engine(Tone::Casual, ReactionLevel::Full);
        let r = play(e.handle(&event("tasks.completed").with_significance(0.9), 0));
        assert!((r.intensity - 0.9).abs() < 1e-6);
    }

    #[test]
    fn reaction_cooldown_drops_excess_and_recovers() {
        let mut e = engine(Tone::Casual, ReactionLevel::Full);
        play(e.handle(&event("tasks.created"), 1_000));
        assert_eq!(
            e.handle(&event("tasks.created"), 1_500),
            Decision::Dropped(DropReason::Cooldown)
        );
        play(e.handle(&event("tasks.created"), 3_100));
    }

    #[test]
    fn celebrate_has_its_own_longer_cooldown() {
        let mut e = engine(Tone::Casual, ReactionLevel::Full);
        play(e.handle(&event("tasks.completed"), 0));
        assert_eq!(
            e.handle(&event("tasks.completed"), 5_000),
            Decision::Dropped(DropReason::CelebrateCooldown)
        );
        // A non-celebrate reaction is fine once the short cooldown has passed.
        play(e.handle(&event("tasks.created"), 5_000));
        play(e.handle(&event("tasks.completed"), 11_000));
    }

    #[test]
    fn subtle_halves_intensity_and_hides_most_chips() {
        let mut e = engine(Tone::Casual, ReactionLevel::Subtle);
        let r = play(e.handle(&event("tasks.completed"), 0));
        assert!((r.intensity - 0.15).abs() < 1e-6);
        assert!(!r.show_chip);

        let r = play(e.handle(
            &Event::new("tasks.due", json!({ "title": "stretch" })),
            5_000,
        ));
        assert!(r.show_chip);
        assert_eq!(r.animation.as_deref(), Some("nudge"));
        assert!(r.text.unwrap().contains("stretch"));
    }

    #[test]
    fn off_only_lets_reminders_and_errors_through_as_static_chips() {
        let mut e = engine(Tone::Neutral, ReactionLevel::Off);
        assert_eq!(
            e.handle(&event("tasks.completed"), 0),
            Decision::Dropped(DropReason::ReactionsOff)
        );
        let r = play(e.handle(&event("core.error"), 0));
        assert_eq!(r.animation, None);
        assert!(r.show_chip);
        assert!(r.text.is_some_and(|t| !t.is_empty()));
    }

    #[test]
    fn placeholders_are_filled_from_payload() {
        let mut e = engine(Tone::Neutral, ReactionLevel::Full);
        let r = play(e.handle(
            &Event::new("tasks.streak_milestone", json!({ "count": 7 })),
            0,
        ));
        assert!(r.text.unwrap().contains('7'));
    }

    #[test]
    fn variants_do_not_repeat_back_to_back() {
        let mut e = engine(Tone::Casual, ReactionLevel::Full);
        let mut last = String::new();
        for i in 0..20 {
            let r = play(e.handle(&event("tasks.created"), i * 3_000));
            let text = r.text.unwrap();
            assert_ne!(text, last);
            last = text;
        }
    }

    #[test]
    fn user_microcopy_overrides_core_keys() {
        let mut pack = Microcopy::core_en();
        pack.merge(Microcopy::from_json(r#"{"casual":{"tasks.created":["yes boss."]}}"#).unwrap());
        let mut e = PersonalityEngine::new(Tone::Casual, ReactionLevel::Full, pack);
        let r = play(e.handle(&event("tasks.created"), 0));
        assert_eq!(r.text.as_deref(), Some("yes boss."));
    }

    #[test]
    fn named_intensities() {
        assert_eq!(named_intensity("small"), Some(0.3));
        assert_eq!(named_intensity("huge"), None);
    }
}
