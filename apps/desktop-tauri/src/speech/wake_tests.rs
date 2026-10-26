use super::wake::*;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("abito-wake-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ── The pinned VAD model ──────────────────────────────────────────────────────

#[test]
fn the_vad_download_is_the_official_pinned_file() {
    assert!(VAD
        .url
        .starts_with("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/"));
    assert_eq!(VAD.size, 643_854);
    assert_eq!(
        VAD.sha256,
        "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6"
    );
    assert_eq!(VAD.sha256.len(), 64);
}

#[test]
fn a_vad_file_with_the_wrong_hash_is_refused_and_nothing_is_left() {
    let d = scratch("badvad");
    let part = d.join("silero_vad.onnx.part");
    std::fs::write(&part, b"not the model").unwrap();
    assert!(install_vad(&part, &d).is_err());
    assert!(!d.join(VAD.id).join("silero_vad.onnx").exists());
}

// ── Sensitivity ───────────────────────────────────────────────────────────────

#[test]
fn sensitivity_maps_to_a_vad_threshold_and_a_match_tolerance() {
    let lo = params(0);
    let mid = params(50);
    let hi = params(100);
    assert!(hi.vad_threshold < mid.vad_threshold && mid.vad_threshold < lo.vad_threshold);
    assert!(lo.vad_threshold <= 0.7 && hi.vad_threshold >= 0.2);
    assert_eq!((lo.tolerance, mid.tolerance, hi.tolerance), (0, 1, 2));
    assert_eq!(params(250), params(100), "clamped");
}

// ── Matching «OK Abito» in a transcript ───────────────────────────────────────

fn m(text: &str, tol: usize) -> Option<String> {
    match_wake(text, tol)
}

#[test]
fn the_phrase_is_found_with_accents_case_and_punctuation_ignored() {
    for t in [
        "OK Abito",
        "ok abito",
        "Ok, Abito.",
        "¡OK Abito!",
        "okay abito",
        "okey abito",
        "oca abito",
        "okey, Ábito",
        "hey abito",
    ] {
        assert_eq!(m(t, 1), Some(String::new()), "{t}");
    }
}

#[test]
fn spoken_variants_of_the_name_match() {
    for t in [
        "ok habito",
        "ok hábito",
        "ok a vito",
        "ok a bito",
        "ok avito",
        "ok abbito",
        "ok abeto",
        "okey a vito",
    ] {
        assert!(m(t, 1).is_some(), "{t}");
    }
}

#[test]
fn spellings_parakeet_actually_gave_for_the_phrase_match_after_an_ok_word() {
    for t in [
        "ok cavito",
        "okey vito",
        "oca vito",
        "ok kavito",
        "ok cabito",
    ] {
        assert!(m(t, 1).is_some(), "{t}");
    }
    assert_eq!(m("okey vito con música", 1), Some("con música".into()));
    // …but «vito» alone, or a plain name in ordinary speech, never wakes.
    for t in ["vito", "cavito", "hola vito", "conocí a vito ayer"] {
        assert_eq!(m(t, 2), None, "{t}");
    }
}

#[test]
fn glued_transcriptions_match_too() {
    assert_eq!(m("okabito", 1), Some(String::new()));
    assert_eq!(m("okey abito pon música", 1), Some("pon música".into()));
    assert_eq!(m("okayabito pausa", 1), Some("pausa".into()));
}

#[test]
fn the_command_in_the_same_segment_keeps_its_spelling() {
    assert_eq!(m("OK Abito, añade GitHub", 1), Some("añade GitHub".into()));
    assert_eq!(m("ok abito: ¿qué suena?", 1), Some("¿qué suena?".into()));
    assert_eq!(
        m("Euh ok abito pon Daft Punk", 1),
        Some("pon Daft Punk".into())
    );
    // The LAST occurrence wins.
    assert_eq!(m("ok abito ok abito pausa", 1), Some("pausa".into()));
}

#[test]
fn ok_then_a_short_word_then_the_name_wakes() {
    // Heard live on 2026-10-22: «Ok ya Vito, que hora es.»
    assert_eq!(
        m("Ok ya Vito, que hora es.", 1),
        Some("que hora es.".into())
    );
    // …and the same phrase glued, said first: «Yavito, ¿qué hora es?»
    assert_eq!(m("Yavito, ¿qué hora es?", 1), Some("¿qué hora es?".into()));
    assert_eq!(m("okey a habito pon música", 1), Some("pon música".into()));
    // Only a SHORT word in between, and only before the name.
    for t in [
        "ok ya",
        "ok ya voy",
        "ok vale vito",
        "ok ya bonito",
        "ya vito",
    ] {
        assert_eq!(m(t, 2), None, "{t}");
    }
}

#[test]
fn the_name_said_first_and_followed_by_a_comma_wakes_when_parakeet_drops_the_ok() {
    // Heard live over RDP on 2026-10-22: the «OK» came out as a separate «Yes.» segment
    // or was lost, and the phrase arrived as «Habito, ¿qué hora es?».
    assert_eq!(m("Habito, ¿qué hora es?", 1), Some("¿qué hora es?".into()));
    assert_eq!(m("Habito, que hora es.", 1), Some("que hora es.".into()));
    assert_eq!(m("Abito, pon música", 0), Some("pon música".into()));
    assert_eq!(m("Ábito, ¿qué suena?", 1), Some("¿qué suena?".into()));
    // Only a known spelling, only as the FIRST word, only with the comma, only with a command.
    for t in [
        "Hábito",
        "Habito,",
        "Habito que hora es",
        "Abido, pon música",
        "mi hábito, de leer",
        "Hola, habito, qué hora es",
        "El hábito, hace al monje",
    ] {
        assert_eq!(m(t, 2), None, "{t}");
    }
}

#[test]
fn ordinary_speech_does_not_wake() {
    for t in [
        "",
        "hola qué tal",
        "abito",
        "mi hábito de leer",
        "ok",
        "ok vale",
        "el hábito hace al monje",
        "tengo un abrigo",
        "okupa en abril",
    ] {
        assert_eq!(m(t, 1), None, "{t}");
    }
}

#[test]
fn tolerance_controls_the_fuzzy_names_and_zero_keeps_only_the_known_spellings() {
    assert_eq!(m("ok abidu", 0), None);
    assert!(m("ok abido", 1).is_some());
    assert_eq!(m("ok abxdo", 1), None);
    assert!(m("ok abxdo", 2).is_some());
    assert!(m("ok abito", 0).is_some());
    assert!(
        m("ok habito", 0).is_some(),
        "the known spellings never need tolerance"
    );
    // A high tolerance still never turns a common word into the name.
    for t in ["ok hola", "ok bonito", "ok aquí", "ok abrir"] {
        assert_eq!(m(t, 2), None, "{t}");
    }
}

// ── The flow: segments in, events out ─────────────────────────────────────────

const SEC: usize = 16_000;

fn flow() -> WakeFlow {
    WakeFlow::new()
}

#[test]
fn a_transcript_without_the_phrase_changes_nothing() {
    let mut f = flow();
    assert!(f.on_transcript("hola qué tal", 1).is_empty());
    assert!(!f.armed());
}

#[test]
fn wake_and_command_in_one_segment_give_both_events() {
    let mut f = flow();
    let ev = f.on_transcript("ok abito pon música", 1);
    assert_eq!(
        ev,
        vec![FlowEvent::Wake, FlowEvent::Command("pon música".into())]
    );
    assert!(!f.armed());
}

#[test]
fn a_bare_wake_arms_the_next_segment_as_the_command() {
    let mut f = flow();
    assert_eq!(f.on_transcript("ok abito", 1), vec![FlowEvent::Wake]);
    assert!(f.armed());
    assert_eq!(
        f.on_transcript("añade GitHub", 1),
        vec![FlowEvent::Command("añade GitHub".into())]
    );
    assert!(!f.armed());
    // Back to idle: ordinary speech is ignored again.
    assert!(f.on_transcript("añade GitHub", 1).is_empty());
}

#[test]
fn the_armed_command_may_repeat_the_phrase_and_it_is_stripped() {
    let mut f = flow();
    f.on_transcript("ok abito", 1);
    assert_eq!(
        f.on_transcript("ok abito pausa", 1),
        vec![FlowEvent::Command("pausa".into())]
    );
}

#[test]
fn nothing_said_after_the_wake_times_out() {
    let mut f = flow();
    f.on_transcript("ok abito", 1);
    assert!(f.advance(3 * SEC).is_empty());
    assert_eq!(f.advance(4 * SEC), vec![FlowEvent::Timeout]);
    assert!(!f.armed());
    assert!(f.advance(30 * SEC).is_empty(), "one timeout only");
}

#[test]
fn a_segment_in_flight_holds_the_timeout_until_its_transcript_arrives() {
    let mut f = flow();
    f.on_transcript("ok abito", 1);
    f.segment_sent();
    assert!(f.advance(20 * SEC).is_empty());
    assert_eq!(
        f.on_transcript("pausa", 1),
        vec![FlowEvent::Command("pausa".into())]
    );
}

#[test]
fn a_follow_up_arm_listens_without_a_wake() {
    let mut f = flow();
    f.arm();
    assert_eq!(
        f.on_transcript("quita Vercel", 1),
        vec![FlowEvent::Command("quita Vercel".into())]
    );
}

#[test]
fn an_empty_transcript_in_command_mode_keeps_waiting() {
    let mut f = flow();
    f.arm();
    assert!(f.on_transcript("   ", 1).is_empty());
    assert!(f.armed());
}

// ── Segmenting: the busy worker drops segments ────────────────────────────────

#[test]
fn a_busy_recognizer_drops_the_segment_instead_of_queueing_it() {
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(1);
    assert!(offer(&tx, vec![0.0; 10]));
    assert!(
        !offer(&tx, vec![0.0; 10]),
        "second one dropped while the first waits"
    );
    drop(rx.recv().unwrap());
    assert!(offer(&tx, vec![0.0; 10]));
}

#[test]
fn segments_longer_than_the_cap_are_cut_and_tiny_ones_ignored() {
    assert!(MAX_SEGMENT_SECONDS <= 6.0);
    assert!(usable(&vec![0.0; SEC]));
    assert!(!usable(&vec![0.0; SEC / 10]));
    let long = vec![0.0; 12 * SEC];
    assert_eq!(
        clip(&long).len(),
        (MAX_SEGMENT_SECONDS * SEC as f32) as usize
    );
}

// ── Real VAD on synthetic audio (needs the model; run with the env var) ──────

#[test]
#[ignore]
fn live_vad_probe() {
    let Ok(model) = std::env::var("NB488_VAD") else {
        return;
    };
    let mut vad = SileroGate::create(std::path::Path::new(&model), params(50)).expect("vad");
    // 5 s of low noise: no segment, and cheap.
    let noise: Vec<f32> = (0..5 * SEC)
        .map(|i| ((i * 7919) % 200) as f32 / 200_000.0)
        .collect();
    let t = std::time::Instant::now();
    let mut seg = 0;
    for c in noise.chunks(1600) {
        seg += vad.feed(c).len();
    }
    println!("noise: {seg} segments, {:?} for 5 s of audio", t.elapsed());
    assert_eq!(seg, 0);
}

/// Manual accuracy/CPU probe: NB488_VAD=<silero_vad.onnx> NB488_WAVS=<dir of 16 kHz mono .f32>
/// cargo test --offline --lib live_wake_probe -- --ignored --nocapture
#[test]
#[ignore]
fn live_wake_probe() {
    let (Ok(model), Ok(dir)) = (std::env::var("NB488_VAD"), std::env::var("NB488_WAVS")) else {
        return;
    };
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    names.sort();
    for p in names {
        let bytes = std::fs::read(&p).unwrap();
        let mut s: Vec<f32> = vec![0.0; SEC / 2];
        s.extend(
            bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])),
        );
        s.extend(vec![0.0; SEC]);
        let mut gate = SileroGate::create(std::path::Path::new(&model), params(50)).expect("vad");
        let mut segs = vec![];
        let t = std::time::Instant::now();
        for c in s.chunks(1600) {
            segs.extend(gate.feed(c));
        }
        let vad_ms = t.elapsed().as_millis();
        let mut line = format!(
            "{} vad={}ms segs={}",
            p.file_name().unwrap().to_string_lossy(),
            vad_ms,
            segs.len()
        );
        for seg in &segs {
            let t = std::time::Instant::now();
            let text = crate::speech::transcribe(clip(seg)).unwrap_or_default();
            let ms = t.elapsed().as_millis();
            line += &format!(
                " | {:.1}s -> {:?} in {}ms wake={:?}",
                seg.len() as f32 / 16000.0,
                text,
                ms,
                match_wake(&text, 1)
            );
        }
        println!("{line}");
    }
}
