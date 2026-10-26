use super::*;

// --- derive: the §10.3 matrix ------------------------------------------------

#[test]
fn connected_with_nothing_pending_is_idle() {
    assert_eq!(derive("connected", false, false, false), TrayState::Idle);
}

#[test]
fn working_is_a_turn_in_flight_over_a_good_connection() {
    assert_eq!(derive("working", false, false, false), TrayState::Working);
    // the transport failing still wins: the app cannot see anything reliably
    assert_eq!(derive("working", false, false, true), TrayState::Disconnected);
}

#[test]
fn every_not_connected_value_is_disconnected() {
    for connection in [
        "", "no_daemon", "not_connected", "outdated", "unavailable", "error",
        "connecting", "Connected", "connected ", "working-x",
    ] {
        assert_eq!(
            derive(connection, false, false, false),
            TrayState::Disconnected,
            "{connection:?} is not a live connection: fail closed"
        );
    }
    // the local transport failing is honest disconnection even when connected
    assert_eq!(derive("connected", false, false, true), TrayState::Disconnected);
}

#[test]
fn a_pending_card_or_question_wins_over_everything() {
    for connection in ["connected", "working", "no_daemon", "error", ""] {
        assert_eq!(
            derive(connection, true, false, false),
            TrayState::NeedsAttention,
            "pending card over {connection:?}"
        );
        assert_eq!(
            derive(connection, false, true, false),
            TrayState::NeedsAttention,
            "pending question over {connection:?}"
        );
        // even with the transport gone: the human must decide
        assert_eq!(
            derive(connection, true, false, true),
            TrayState::NeedsAttention,
            "attention over {connection:?} with transport down"
        );
    }
}

#[test]
fn the_state_names_are_the_contracted_vocabulary() {
    assert_eq!(TrayState::Idle.as_str(), "idle");
    assert_eq!(TrayState::Working.as_str(), "working");
    assert_eq!(TrayState::NeedsAttention.as_str(), "needs-attention");
    assert_eq!(TrayState::Disconnected.as_str(), "disconnected");
}

#[test]
fn index_round_trip_and_unknown_fails_closed() {
    for state in [
        TrayState::Idle,
        TrayState::Working,
        TrayState::NeedsAttention,
        TrayState::Disconnected,
    ] {
        assert_eq!(TrayState::from_index(state.to_index()), state);
    }
    for unknown in [4u8, 99, 255] {
        assert_eq!(TrayState::from_index(unknown), TrayState::Disconnected, "{unknown}");
    }
}

// --- glyphs: four shapes, not four colors ------------------------------------

fn alpha_at(g: &Glyph, x: u32, y: u32) -> u8 {
    assert_eq!(g.width, 32);
    assert_eq!(g.height, 32);
    assert_eq!(g.rgba.len(), (32 * 32 * 4) as usize, "32×32 RGBA");
    g.rgba[((y * g.width + x) * 4 + 3) as usize]
}

#[test]
fn every_glyph_is_32x32_rgba_and_deterministic() {
    for state in [
        TrayState::Idle,
        TrayState::Working,
        TrayState::NeedsAttention,
        TrayState::Disconnected,
    ] {
        let a = glyph_rgba(state);
        let b = glyph_rgba(state);
        assert_eq!(a, b, "{:?} must draw deterministically", state);
        assert_eq!(a.width, 32);
        assert_eq!(a.height, 32);
        assert_eq!(a.rgba.len(), 32 * 32 * 4);
    }
}

#[test]
fn the_four_glyphs_are_pairwise_distinct() {
    let states = [
        TrayState::Idle,
        TrayState::Working,
        TrayState::NeedsAttention,
        TrayState::Disconnected,
    ];
    for (i, a) in states.iter().enumerate() {
        for b in &states[i + 1..] {
            assert_ne!(
                glyph_rgba(*a),
                glyph_rgba(*b),
                "{a:?} and {b:?} must differ by shape, not only by color"
            );
        }
    }
}

#[test]
fn idle_ring_is_continuous_and_working_ring_has_gaps() {
    let idle = glyph_rgba(TrayState::Idle);
    let working = glyph_rgba(TrayState::Working);
    // ring pixel straight above the center (16,16): r=11 -> y=16-11=5
    assert!(alpha_at(&idle, 16, 5) > 128, "idle ring top is drawn: {}", alpha_at(&idle, 16, 5));
    assert!(alpha_at(&idle, 5, 16) > 128, "idle ring left is drawn");
    // the dashed working ring MUST have transparent gaps somewhere ON the ring
    let mut gaps = 0;
    let mut drawn = 0;
    for x in 5..=27 {
        // follow the top half of the ring: y = 16 - sqrt(121 - (x-16)^2)
        let dx = x as f64 - 16.0;
        let y = (16.0 - (121.0_f64 - dx * dx).max(0.0).sqrt()).round() as i64;
        if (0..32).contains(&y) {
            match alpha_at(&working, x, y as u32) {
                0 => gaps += 1,
                a if a > 128 => drawn += 1,
                _ => {}
            }
        }
    }
    assert!(drawn > 0, "working ring is drawn somewhere");
    assert!(gaps > 0, "working ring is DASHED: gaps on the ring, got none");
    // and the idle ring stays continuous
    let mut idle_gaps = 0;
    for x in 5..=27 {
        let dx = x as f64 - 16.0;
        let y = (16.0 - (121.0_f64 - dx * dx).max(0.0).sqrt()).round() as i64;
        if (0..32).contains(&y) && alpha_at(&idle, x, y as u32) == 0 {
            idle_gaps += 1;
        }
    }
    assert_eq!(idle_gaps, 0, "idle ring must be continuous");
}

#[test]
fn attention_has_a_badge_dot_off_the_ring_center() {
    let attention = glyph_rgba(TrayState::NeedsAttention);
    // badge sits top-right, off the ring stroke: around (23, 9)
    let badge = (10..14)
        .map(|y| (19..24).map(move |x| (x, y)))
        .flatten()
        .any(|(x, y)| alpha_at(&attention, x, y) > 128);
    assert!(badge, "needs-attention glyph carries an opaque badge dot top-right");
}

#[test]
fn disconnected_is_dimmed_and_slashed() {
    let idle = glyph_rgba(TrayState::Idle);
    let gone = glyph_rgba(TrayState::Disconnected);
    // dimmed: strictly less total alpha than the clean idle glyph
    let sum = |g: &Glyph| g.rgba.iter().skip(3).step_by(4).map(|&a| a as u64).sum::<u64>();
    assert!(sum(&gone) < sum(&idle), "disconnected must be dimmer than idle");
    // slashed: opaque pixels along the anti-diagonal inside the ring,
    // where idle is transparent
    let slash = (10i32..=22).any(|i| {
        let x = 16 + (i - 16).unsigned_abs() / 2 + 4;
        let y = 32 - i - 4;
        alpha_at(&gone, x.min(31) as u32, y.min(31) as u32) > 128
            && alpha_at(&idle, x.min(31) as u32, y.min(31) as u32) == 0
    });
    assert!(slash, "disconnected carries a diagonal slash idle lacks");
}
