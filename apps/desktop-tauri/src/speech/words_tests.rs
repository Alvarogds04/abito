use super::words::*;

fn w(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn exact_and_near_spellings_of_a_custom_word_are_corrected_keeping_case_and_punctuation() {
    let words = w(&["Kubernetes", "Abito"]);
    assert_eq!(apply_custom_words("usa kubernetis hoy", &words, 0.4), "usa Kubernetes hoy");
    assert_eq!(apply_custom_words("Kubernetis, funciona", &words, 0.4), "Kubernetes, funciona");
    assert_eq!(apply_custom_words("KUBERNETIS", &words, 0.4), "KUBERNETES");
    assert_eq!(apply_custom_words("hola qué tal", &words, 0.4), "hola qué tal", "unrelated text is untouched");
    assert_eq!(apply_custom_words("hola", &[], 0.4), "hola");
}

#[test]
fn a_split_word_is_joined_by_the_ngram_match() {
    // Handy's «Charge B» → «ChargeBee» case, here with a Spanish speaker's «ya vito».
    assert_eq!(apply_custom_words("ok ya vito que hora es", &w(&["Yavito"]), 0.3), "ok Yavito que hora es");
    assert_eq!(apply_custom_words("Charge B, che", &w(&["ChargeBee"]), 0.3), "ChargeBee, che", "Handy: the comma closes the window, the punctuation stays");
}

#[test]
fn abito_is_built_in_and_the_spellings_heard_live_become_abito() {
    for (heard, fixed) in [
        ("Habito, ¿qué hora es?", "Abito, ¿qué hora es?"),
        ("Yavito, ¿qué hora es?", "Abito, ¿qué hora es?"),
        ("Okay, Yavita que ahora es que", "Okay, Abito que ahora es que"),
        ("Ok ya Vito, que hora es.", "Ok Abito, que hora es."),
        ("ok avito", "ok Abito"),
    ] {
        assert_eq!(correct(heard, &[]), fixed, "{heard}");
    }
}

#[test]
fn common_words_never_become_abito() {
    for t in ["mi hábito de leer", "un bonito día", "abrir la puerta", "el abrigo", "abril", "tengo un hábito", "habito en Madrid", "Yes.", "So", "Años", "obito", ""] {
        assert_eq!(correct(t, &[]), t, "{t}");
    }
}

#[test]
fn the_user_words_win_and_are_sanitised() {
    let cleaned = clean_words(&w(&["  Kubernetes ", "kubernetes", "", "a", "x".repeat(60).as_str(), "Doña Ana", "with\nnewline", "ok"]));
    assert_eq!(cleaned, w(&["Kubernetes", "Doña Ana"]));
    let many: Vec<String> = (0..80).map(|i| format!("palabra{}", "x".repeat(i % 7 + 3)) + &i.to_string()).collect();
    assert!(clean_words(&many).len() <= MAX_WORDS);
    // A user word that is a common word is allowed (their choice), Abito stays guarded.
    assert_eq!(correct("hola Kubernetis", &w(&["Kubernetes"])), "hola Kubernetes");
}
