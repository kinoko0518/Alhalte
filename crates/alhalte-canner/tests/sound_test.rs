//! Tests for deterministic audio encoding in `alhalte-canner`.

use alhalte_canner::encode_sound;

/// Creates a standard 16-bit PCM RIFF WAV in memory for testing.
fn create_test_wav(sample_rate: u32, channels: u16, num_frames: usize) -> Vec<u8> {
    let total_samples = num_frames * (channels as usize);
    let data_len = (total_samples * 2) as u32;
    let file_len = 36 + data_len;

    let mut buf = Vec::with_capacity((44 + data_len) as usize);
    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_len.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt subchunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // subchunk size = 16 for PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // audio format = 1 (PCM)
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * (channels as u32) * 2;
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = channels * 2;
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample = 16

    // data subchunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());

    // Generate audio samples (a simple 440 Hz tone)
    for frame in 0..num_frames {
        let t = frame as f32 / sample_rate as f32;
        // 440 Hz sine wave
        let val = (t * 440.0 * 2.0 * std::f32::consts::PI).sin();
        let sample = (val * 16000.0) as i16;
        for _ in 0..channels {
            buf.extend_from_slice(&sample.to_le_bytes());
        }
    }

    buf
}

// spec: alhalte-canner/encode-sound
#[test]
fn test_encode_sound_48k_native() {
    // 0.1s of 48kHz mono audio = 4800 frames
    let wav_bytes = create_test_wav(48_000, 1, 4800);
    let ogg_bytes = encode_sound(&wav_bytes).expect("encode_sound should succeed");

    // Must be valid Ogg container starting with "OggS"
    assert!(ogg_bytes.len() > 28);
    assert_eq!(&ogg_bytes[0..4], b"OggS");

    // Must contain OpusHead header in first page
    assert!(
        ogg_bytes.windows(8).any(|w| w == b"OpusHead"),
        "Ogg stream must contain OpusHead"
    );
}

// spec: alhalte-canner/encode-sound
#[test]
fn test_encode_sound_resampling_44_1k_to_48k() {
    // 0.1s of 44.1kHz stereo audio = 4410 frames
    let wav_bytes = create_test_wav(44_100, 2, 4410);
    let ogg_bytes = encode_sound(&wav_bytes).expect("resampling and encoding should succeed");

    assert!(ogg_bytes.len() > 28);
    assert_eq!(&ogg_bytes[0..4], b"OggS");
    assert!(ogg_bytes.windows(8).any(|w| w == b"OpusHead"));
}

// spec: alhalte-canner/encode-sound
#[test]
fn test_encode_sound_deterministic_identity() {
    let wav_bytes = create_test_wav(44_100, 1, 3000);

    let enc1 = encode_sound(&wav_bytes).expect("first encode");
    let enc2 = encode_sound(&wav_bytes).expect("second encode");

    // Device-independent identical bit-for-bit output guaranteed
    assert_eq!(enc1, enc2);
}

// spec: alhalte-canner/encode-sound
#[test]
fn test_encode_sound_invalid_input() {
    let invalid = b"this is completely invalid audio data";
    assert!(encode_sound(invalid).is_err());
}
