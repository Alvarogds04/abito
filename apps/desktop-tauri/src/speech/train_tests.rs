use super::train::*;
use super::wake::{match_wake_with, WakeFlow, FlowEvent};

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("abito-train-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ── Learning variants from transcripts ────────────────────────────────────────

#[test]
fn a_spelling_heard_in_several_recordings_is_learned_with_its_lead_word() {
    let l = learn(&v(&["Yes, Gabíto.", "yes gabito", "Yes gabito!", "Habito.", "yes gabito"]));
    assert_eq!(l.names, v(&["gabito"]));
    assert_eq!(l.leads, v(&["yes"]));
    assert!(l.phrases.contains(&"yes gabito".to_string()));
}

#[test]
fn a_spelling_heard_only_once_is_not_learned() {
    let l = learn(&v(&["ok abito", "ok abito", "ok abito", "Obito", "ok abito"]));
    assert!(!l.names.contains(&"obito".to_string()));
}

#[test]
fn duplicates_are_stored_once_and_known_spellings_are_not_re_learned() {
    let l = learn(&v(&["ok habito", "ok habito", "ok habito", "ok habito", "ok habito"]));
    assert!(l.names.is_empty(), "habito is already a known spelling: {:?}", l.names);
    assert!(l.leads.is_empty(), "ok is already a known lead");
    assert_eq!(l.phrases, v(&["ok habito"]));
}

#[test]
fn common_words_and_noise_are_never_learned() {
    let l = learn(&v(&["hola", "hola", "ok bonito", "ok bonito", "abrir", "abrir", "pon musica de fondo ahora", "pon musica de fondo ahora", "abril", "abril"]));
    assert!(l.names.is_empty() && l.leads.is_empty() && l.phrases.is_empty(), "{l:?}");
    // A common word as the lead is not learned either, even next to a good name.
    let l = learn(&v(&["mi gabito", "mi gabito", "mi gabito"]));
    assert_eq!(l.names, v(&["gabito"]));
    assert!(l.leads.is_empty(), "{:?}", l.leads);
}

#[test]
fn the_lists_are_capped() {
    let many: Vec<String> = ["ggabito", "cgabito", "kabito", "tabito", "labito", "mabito", "nabito", "pabito", "rabito", "sabito"].iter().flat_map(|w| vec![w.to_string(), w.to_string()]).collect();
    let l = learn(&many);
    assert!(l.names.len() <= MAX_NAMES && l.phrases.len() <= MAX_NAMES, "{l:?}");
}

// ── Matching with what was learned ────────────────────────────────────────────

fn learned() -> Learned {
    learn(&v(&["Yes, Gabíto.", "yes gabito", "yes gabito", "Ggabito", "ggabito"]))
}

#[test]
fn a_learned_name_wakes_after_ok_and_after_its_learned_lead() {
    let l = learned();
    assert_eq!(match_wake_with("ok ggabito pon música", 0, &l), Some("pon música".into()));
    assert_eq!(match_wake_with("yes gabito, qué hora es", 0, &l), Some("qué hora es".into()));
    // …and as the comma-first vocative with a command.
    assert_eq!(match_wake_with("Ggabito, qué hora es", 0, &l), Some("qué hora es".into()));
}

#[test]
fn the_whole_trained_phrase_alone_is_a_bare_wake() {
    let l = learned();
    assert_eq!(match_wake_with("Yes. Avito.", 0, &l), Some(String::new()));
    assert_eq!(match_wake_with("ggabito", 0, &l), Some(String::new()));
}

#[test]
fn learning_never_loosens_ordinary_speech() {
    let l = learned();
    for t in ["", "hola qué tal", "yes", "gabito", "yes vale", "mi ggabito de leer", "el ggabito hace al monje", "hola, ggabito, qué hora es", "yes pon música", "ok hola"] {
        let wake = match_wake_with(t, 0, &l);
        // «gabito» alone / «ggabito» alone only count as the complete trained phrases above.
        if t == "gabito" { assert_eq!(wake, None, "{t}"); continue; }
        assert_eq!(wake, None, "{t}");
    }
    // Without training the very same sentences behave as before.
    assert_eq!(match_wake_with("ggabito, qué hora es", 0, &Learned::default()), None);
    assert_eq!(match_wake_with("yes gabito", 0, &Learned::default()), None);
}

#[test]
fn the_flow_uses_what_was_learned_and_forgets_it_on_reset() {
    let mut f = WakeFlow::new();
    f.set_learned(learned());
    assert_eq!(f.on_transcript("Ggabito, qué hora es", 0), vec![FlowEvent::Wake, FlowEvent::Command("qué hora es".into())]);
    f.set_learned(Learned::default());
    assert!(f.on_transcript("Ggabito, qué hora es", 0).is_empty());
}

// ── The five recordings ───────────────────────────────────────────────────────

struct Fake(std::cell::RefCell<Vec<Result<Option<String>, String>>>);
impl Recorder for Fake {
    fn hear(&self) -> Result<Option<String>, String> {
        self.0.borrow_mut().remove(0)
    }
}
fn fake(items: &[Option<&str>]) -> Fake {
    Fake(std::cell::RefCell::new(items.iter().map(|s| Ok(s.map(String::from))).collect()))
}

#[test]
fn five_heard_recordings_finish_training_and_the_heard_text_is_reported() {
    let rec = fake(&[Some("Yes. Avito."), Some("yes gabito"), Some("yes gabito"), Some("Yes gabito!"), Some("yes gabito")]);
    let mut t = Trainer::default();
    for n in 1..=TARGET {
        let step = t.record(&rec).unwrap();
        assert_eq!(step, Step::Heard { text: step_text(&step), count: n });
    }
    assert_eq!(t.recorded(), TARGET);
    let l = t.finish().unwrap();
    assert_eq!(l.names, v(&["gabito"]));
    assert!(!t.active(), "the session is over");
}

fn step_text(s: &Step) -> String {
    match s { Step::Heard { text, .. } => text.clone(), Step::Silence => String::new() }
}

#[test]
fn silence_is_not_counted_and_an_empty_transcript_neither() {
    let rec = fake(&[None, Some("   "), Some("ok abito")]);
    let mut t = Trainer::default();
    assert_eq!(t.record(&rec).unwrap(), Step::Silence);
    assert_eq!(t.record(&rec).unwrap(), Step::Silence);
    assert_eq!(t.recorded(), 0);
    t.record(&rec).unwrap();
    assert_eq!(t.recorded(), 1);
}

#[test]
fn undo_redoes_the_last_recording_and_cancel_forgets_everything() {
    let rec = fake(&[Some("ok abito"), Some("ruido")]);
    let mut t = Trainer::default();
    t.record(&rec).unwrap();
    t.record(&rec).unwrap();
    t.undo();
    assert_eq!(t.recorded(), 1);
    t.cancel();
    assert_eq!(t.recorded(), 0);
    assert!(!t.active());
}

#[test]
fn finishing_early_or_after_a_recorder_error_is_refused_and_keeps_nothing_new() {
    let mut t = Trainer::default();
    assert!(t.finish().is_err());
    let rec = Fake(std::cell::RefCell::new(vec![Err("no microphone".into())]));
    assert_eq!(t.record(&rec).unwrap_err(), "no microphone");
    assert_eq!(t.recorded(), 0);
    let rec = fake(&[Some("ok abito"); 6]);
    for _ in 0..TARGET { t.record(&rec).unwrap(); }
    assert!(t.record(&rec).is_err(), "no sixth recording");
}

// ── Stored next to the prefs; reset forgets ───────────────────────────────────

#[test]
fn the_training_is_saved_as_text_only_loaded_back_and_reset() {
    let d = scratch("store");
    assert_eq!(load_in(&d), Learned::default());
    let l = learned();
    save_in(&d, &l).unwrap();
    assert_eq!(load_in(&d), l);
    let on_disk = std::fs::read_to_string(d.join(FILE)).unwrap();
    assert!(on_disk.contains("gabito"));
    assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1, "one small JSON, no audio, no temp file");
    reset_in(&d).unwrap();
    assert_eq!(load_in(&d), Learned::default());
    reset_in(&d).unwrap();
}

#[test]
fn a_corrupt_or_hostile_file_loads_as_nothing_and_cannot_loosen_the_rules() {
    let d = scratch("corrupt");
    std::fs::write(d.join(FILE), "{ not json").unwrap();
    assert_eq!(load_in(&d), Learned::default());
    // A hand-edited file with common words is sanitised on load.
    std::fs::write(d.join(FILE), r#"{"names":["hola","gabito"],"leads":["el","yes"],"phrases":["hola","yes gabito"]}"#).unwrap();
    let l = load_in(&d);
    assert_eq!(l.names, v(&["gabito"]));
    assert_eq!(l.leads, v(&["yes"]));
    assert_eq!(l.phrases, v(&["yes gabito"]));
}

// ── Live transcripts of a Spanish speaker over RDP (2026-10-22) ──────────────

#[test]
fn endings_in_a_and_split_forms_heard_live_are_learned() {
    let l = learn(&v(&["Okay, Yavita que ahora es que", "Yavita.", "Yavita,", "Ok ya Vito", "Ok ya vito.", "Yes."]));
    assert!(l.names.contains(&"yavita".to_string()), "{l:?}");
    // «ya vito» joins into «yavito», already a known spelling, so nothing new is stored for it.
    assert!(!l.names.contains(&"yavito".to_string()), "{l:?}");
    assert!(!l.names.contains(&"vito".to_string()), "a bare «vito» is never a name: {l:?}");
    // Noise and the English auto-detection are not learned.
    let noise = learn(&v(&["Yes.", "Yes.", "So", "So", "Años", "Años", "", "", "So it doesn't seem like this time.", "So it doesn't seem like this time."]));
    assert!(noise.is_empty(), "{noise:?}");
}

#[test]
fn a_trained_person_wakes_with_their_own_spellings_including_split_forms() {
    let l = learn(&v(&["Yavita.", "Yavita.", "ok ya vito", "ok ya vito", "Habito."]));
    assert_eq!(match_wake_with("Yavita, ¿qué hora es?", 0, &l), Some("¿qué hora es?".into()));
    assert_eq!(match_wake_with("ok ya vato, que hora es", 0, &l), None, "only what was heard");
    assert_eq!(match_wake_with("ok ya vito, que hora es", 0, &l), Some("que hora es".into()));
    assert_eq!(match_wake_with("Años", 0, &l), None);
}
