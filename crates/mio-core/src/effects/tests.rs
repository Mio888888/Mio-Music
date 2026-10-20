use super::*;
use rodio::buffer::SamplesBuffer;

fn source(effects: &Arc<Effects>, channels: u16) -> SpectrumSource<SamplesBuffer> {
    let samples: Vec<_> = (0..FFT_SIZE * usize::from(channels) * 8)
        .map(|index| {
            if index % usize::from(channels) == 0 {
                0.25
            } else {
                -0.75
            }
        })
        .collect();
    SpectrumSource::new(
        SamplesBuffer::new(channels, 48_000, samples),
        effects.clone(),
        Arc::new(AtomicU64::new(4)),
    )
}

#[test]
fn spectrum_channel_position_stays_bounded() {
    for channels in [1, 2, 3, 6] {
        let effects = Arc::new(Effects::new());
        let mut source = source(&effects, channels);
        while source.next().is_some() {
            assert!(source.channel < usize::from(channels));
        }
        let frame = effects.take_spectrum_frame().unwrap();
        assert!(frame.data.iter().all(|sample| *sample == 0.25));
    }
}

#[test]
fn spectrum_resumes_after_unowned_capture_is_reenabled() {
    let effects = Arc::new(Effects::new());
    let mut source = source(&effects, 2);
    source.by_ref().take(FFT_SIZE * 2).for_each(drop);
    assert!(effects.take_spectrum_frame().is_some());

    effects.set_unowned_capture(false);
    source.by_ref().take(FFT_SIZE * 2 + 1).for_each(drop);
    assert!(effects.take_spectrum_frame().is_none());
    effects.set_unowned_capture(true);
    source.by_ref().take(FFT_SIZE * 2).for_each(drop);
    let frame = effects.take_spectrum_frame().unwrap();
    assert_eq!(frame.generation(), 4);
    assert!(frame.data.iter().all(|sample| *sample == 0.25));
}

#[test]
fn spectrum_demand_resumes_with_the_current_generation() {
    let effects = Arc::new(Effects::new());
    let _owner = effects.spectrum_owner(7).unwrap();
    let mut source = source(&effects, 2);
    let demand = effects.spectrum_demand(7).unwrap();
    source.by_ref().take(FFT_SIZE * 2).for_each(drop);
    assert_eq!(effects.take_spectrum_frame().unwrap().generation(), 4);
    drop(demand);
    source.by_ref().take(FFT_SIZE * 2 + 1).for_each(drop);
    assert!(effects.take_spectrum_frame().is_none());

    source.generation.store(9, Ordering::SeqCst);
    let _demand = effects.spectrum_demand(7).unwrap();
    source.by_ref().take(FFT_SIZE * 2).for_each(drop);
    let frame = effects.take_spectrum_frame().unwrap();
    assert_eq!(frame.generation(), 9);
    assert!(frame.data.iter().all(|sample| *sample == 0.25));
}

#[test]
fn replaced_producer_cannot_publish_over_the_current_source() {
    let effects = Arc::new(Effects::new());
    let mut old = source(&effects, 2);
    let mut current = source(&effects, 2);
    old.by_ref().take(FFT_SIZE * 2).for_each(drop);
    assert!(effects.take_spectrum_frame().is_none());
    current.by_ref().take(FFT_SIZE * 2).for_each(drop);
    assert!(effects.take_spectrum_frame().is_some());
}
