use super::*;

fn context(bars: usize, mood: &str) -> Context {
    Context {
        key: Key::parse("C major").unwrap(),
        signature: TimeSignature::default(),
        bars,
        mood: Mood::named(mood).unwrap(),
        ending: Ending::Settled,
    }
}

fn melody(pitches: &[u8]) -> Vec<Note> {
    pitches
        .iter()
        .enumerate()
        .map(|(i, p)| Note::new(*p, Ticks::QUARTER * i as i64, Ticks::QUARTER))
        .collect()
}

fn coloured(suggestion: &Suggestion, key: Key) -> bool {
    suggestion
        .chart
        .bars
        .iter()
        .flatten()
        .any(|n| !matches!(n.chord_in(key).quality, Quality::Major | Quality::Minor))
}

#[test]
fn an_unambiguous_triad_stays_plain_even_in_a_tense_mood() {
    for mood in Mood::NAMES {
        let result = suggest(&melody(&[60, 64, 67, 60]), context(1, mood)).unwrap();
        assert!(result[0].plain, "{mood}: {:?}", result[0]);
        assert_eq!(result[0].chart.to_text(), "| IM |", "{mood}");
        assert!(!coloured(&result[0], context(1, mood).key));
    }
}

#[test]
fn a_sustained_seventh_can_earn_colour_but_a_short_passing_note_cannot() {
    let sustained = suggest(&melody(&[60, 64, 67, 71]), context(1, "dreamy")).unwrap();
    assert!(
        sustained[0].chart.bars[0]
            .iter()
            .any(|n| n.chord_in(context(1, "dreamy").key).quality == Quality::Major7),
        "{sustained:?}"
    );
    assert!(sustained.iter().any(|s| s.plain));
    let mut passing = melody(&[60, 64, 67, 60]);
    passing.push(Note::new(71, Ticks::QUARTER * 2 + Ticks(480), Ticks(80)));
    let result = suggest(&passing, context(1, "tense")).unwrap();
    assert!(result[0].plain, "{result:?}");
    assert!(
        result.iter().all(|s| !coloured(s, context(1, "tense").key)),
        "{result:?}"
    );
}

#[test]
fn suspension_follows_the_melodic_fourth_then_resolves_inside_the_bar() {
    let notes = vec![
        Note::new(60, Ticks(0), Ticks(480)),
        Note::new(65, Ticks(480), Ticks(1440)),
        Note::new(67, Ticks(1920), Ticks(480)),
        Note::new(64, Ticks(2400), Ticks(1440)),
    ];
    let result = suggest(&notes, context(1, "dreamy")).unwrap();
    assert_eq!(result[0].chart.to_text(), "| Isus4 IM |", "{result:?}");
    assert!(
        result[0].evidence[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("resolves"))
    );
    assert!(result[0].evidence[0].melody_fit > 0.99);
}

#[test]
fn diminished_harmony_needs_melodic_support_and_a_resolving_successor() {
    let notes = melody(&[71, 74, 77, 71, 72, 76, 79, 72]);
    let result = suggest(&notes, context(2, "tense")).unwrap();
    assert!(
        result.iter().any(|s| s.chart.bars[0]
            .iter()
            .any(|n| diminished(n.chord_in(context(2, "tense").key).quality))),
        "{result:?}"
    );
    for suggestion in result {
        let events = suggestion
            .chart
            .resolve(context(2, "tense").key, Ticks::QUARTER * 4);
        for pair in events.windows(2) {
            assert!(transition(pair[0].chord, pair[1].chord).is_some());
        }
        assert!(!unresolved(events.last().unwrap().chord));
    }
    let final_only = suggest(&notes[..4], context(1, "tense")).unwrap();
    assert!(final_only.iter().all(|s| {
        !s.chart.bars[0]
            .iter()
            .any(|n| diminished(n.chord_in(context(1, "tense").key).quality))
    }));
}

#[test]
fn mood_changes_the_tradeoff_not_a_required_count_of_complex_chords() {
    let notes = melody(&[60, 64, 67, 71]);
    let calm = suggest(&notes, context(1, "calm")).unwrap();
    let tense = suggest(&notes, context(1, "tense")).unwrap();
    let margin =
        |result: &[Suggestion]| result[0].score - result.iter().find(|s| s.plain).unwrap().score;
    assert!(margin(&tense) > margin(&calm));
    assert_eq!(calm[0].chart, tense[0].chart);
}

#[test]
fn seventh_quality_follows_the_declared_mode_and_diminished_sevenths_resolve() {
    for (key, pitches, quality) in [
        ("C mixolydian", [60, 64, 67, 70], Quality::Dominant7),
        ("A minor", [69, 72, 76, 79], Quality::Minor7),
        ("C lydian", [67, 71, 74, 78], Quality::Major7),
    ] {
        let mut ctx = context(1, "dreamy");
        ctx.key = Key::parse(key).unwrap();
        ctx.ending = Ending::Open;
        let result = suggest(&melody(&pitches), ctx).unwrap();
        assert_eq!(
            result[0].chart.bars[0][0].chord_in(ctx.key).quality,
            quality,
            "{key}: {result:?}"
        );
    }
    let mut ctx = context(2, "tense");
    ctx.key = Key::parse("C harmonic minor").unwrap();
    let result = suggest(&melody(&[71, 74, 77, 80, 72, 75, 79, 72]), ctx).unwrap();
    assert!(
        result
            .iter()
            .any(|s| s.chart.bars[0][0].chord_in(ctx.key).quality == Quality::Diminished7),
        "{result:?}"
    );
    for item in result {
        let parsed = Chart::parse(&item.chart.to_text())
            .unwrap()
            .spelled_in(ctx.key);
        assert_eq!(
            parsed.resolve(ctx.key, Ticks::QUARTER * 4),
            item.chart.resolve(ctx.key, Ticks::QUARTER * 4)
        );
    }
}

#[test]
fn suggestions_are_deterministic_distinct_transposable_and_roundtrip_in_minor_and_modes() {
    for scale in [
        "major",
        "minor",
        "harmonic minor",
        "dorian",
        "lydian",
        "mixolydian",
        "phrygian",
    ] {
        let mut ctx = context(2, "dreamy");
        ctx.key = Key::parse(&format!("C {scale}")).unwrap();
        let notes: Vec<_> = (0..8)
            .map(|i| {
                Note::new(
                    ctx.key.midi([0, 2, 4, 6][i % 4], 4) as u8,
                    Ticks::QUARTER * i as i64,
                    Ticks::QUARTER,
                )
            })
            .collect();
        let result = suggest(&notes, ctx).unwrap();
        let repeated = suggest(&notes, ctx).unwrap();
        assert_eq!(
            result.iter().map(|s| s.chart.to_text()).collect::<Vec<_>>(),
            repeated
                .iter()
                .map(|s| s.chart.to_text())
                .collect::<Vec<_>>()
        );
        let texts: std::collections::HashSet<_> =
            result.iter().map(|s| s.chart.to_text()).collect();
        assert_eq!(texts.len(), result.len());
        assert!(result.iter().any(|s| s.plain));
        for item in result {
            let parsed = Chart::parse(&item.chart.to_text())
                .unwrap()
                .spelled_in(ctx.key);
            assert_eq!(
                parsed.resolve(ctx.key, Ticks::QUARTER * 4),
                item.chart.resolve(ctx.key, Ticks::QUARTER * 4)
            );
        }
        let mut shifted = ctx;
        shifted.key = ctx.key.transposed(2);
        let mut transposed = notes.clone();
        for n in &mut transposed {
            n.pitch += 2;
        }
        let result = suggest(&transposed, shifted).unwrap();
        assert_eq!(
            result.iter().map(|s| s.chart.to_text()).collect::<Vec<_>>(),
            repeated
                .iter()
                .map(|s| s.chart.to_text())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn rests_hold_harmony_and_invalid_or_empty_inputs_do_not_invent_evidence() {
    let result = suggest(&melody(&[60, 64, 67, 60]), context(2, "tense")).unwrap();
    for s in result {
        assert_eq!(s.chart.bars[0], s.chart.bars[1]);
    }
    let extended = suggest(&melody(&[60, 64, 67, 71]), context(2, "dreamy")).unwrap();
    assert_eq!(extended[0].chart.to_text(), "| Imaj7 | Imaj7 |");
    assert_eq!(
        extended[0].evidence[1].reasons,
        ["Hold the previous harmony through the rest"]
    );
    assert!(suggest(&[], context(2, "neutral")).is_err());
    assert!(suggest(&melody(&[60]), context(17, "neutral")).is_err());
    let mut bad = context(1, "neutral");
    bad.mood.tension = f32::NAN;
    assert!(suggest(&melody(&[60]), bad).is_err());
}
