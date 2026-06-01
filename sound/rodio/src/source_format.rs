//! Boilerplate for rodio `Source` impls. Every audio source in this crate
//! is an infinite stereo stream at `SAMPLE_RATE` (Doom's fixed audio rate);
//! this macro factors out the four trait methods that say so.

/// Implement `rodio::Source` for an infinite stereo `SAMPLE_RATE` stream.
///
/// The implementer must already provide `Iterator<Item = f32>` returning
/// interleaved left/right samples.
#[macro_export]
macro_rules! impl_stereo_source {
    ($ty:ty) => {
        impl ::rodio::Source for $ty {
            fn current_span_len(&self) -> Option<usize> {
                None
            }

            fn channels(&self) -> ::rodio::ChannelCount {
                ::rodio::ChannelCount::new(2).expect("stereo channel count")
            }

            fn sample_rate(&self) -> ::rodio::SampleRate {
                ::rodio::SampleRate::new(::sound_common::SAMPLE_RATE).expect("non-zero sample rate")
            }

            fn total_duration(&self) -> Option<::std::time::Duration> {
                None
            }
        }
    };
}
