//! Audio decoding, deterministic 48kHz resampling, and 128kbps Opus (Ogg) encoding.

use std::io::Cursor;

use opus_pure::{Application, MAX_PACKET_BYTES, OggOpusWriter, OpusEncoder, OpusHead};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::error::{CannerError, Result};

/// Target audio sampling rate for Opus encoding (48,000 Hz).
pub const TARGET_SAMPLE_RATE: u32 = 48_000;

/// Target audio bitrate for Opus encoding (128,000 bps).
pub const TARGET_BITRATE_BPS: i32 = 128_000;

/// Frame size in samples per channel for a standard 20ms Opus frame at 48kHz.
pub const OPUS_FRAME_SIZE: usize = 960;

/// Decodes an input audio file (WAV, Ogg, etc.), deterministically resamples it
/// to 48kHz using fixed-point arithmetic, and encodes it into 128kbps Opus in an
/// Ogg container with device-independent identity.
///
/// // spec: alhalte-canner/encode-sound
pub fn encode_sound(input: &[u8]) -> Result<Vec<u8>> {
    let mss = MediaSourceStream::new(Box::new(Cursor::new(input.to_vec())), Default::default());
    let hint = Hint::new();
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, format_opts, metadata_opts)
        .map_err(|e| CannerError::AudioDecode(e.to_string()))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or(CannerError::NoAudioTrack)?;
    let track_id = track.id;

    let audio_params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(CannerError::NoAudioTrack)?;

    let sample_rate = audio_params.sample_rate.ok_or_else(|| {
        CannerError::AudioDecode("missing sample rate in audio stream".to_string())
    })?;

    let channels_count = audio_params
        .channels
        .as_ref()
        .map(|c| c.count())
        .unwrap_or(1);

    if channels_count == 0 {
        return Err(CannerError::AudioDecode(
            "zero channels in audio stream".to_string(),
        ));
    }

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .map_err(|e| CannerError::AudioDecode(e.to_string()))?;

    let mut raw_pcm: Vec<f32> = Vec::new();

    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                decoded.copy_to_vec_interleaved(&mut raw_pcm);
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                decoder.reset();
            }
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(CannerError::AudioDecode(e.to_string())),
        }
    }

    // Determine target channel layout (1 for mono, 2 for stereo)
    let (target_channels, pcm): (usize, Vec<f32>) = match channels_count {
        1 => (1, raw_pcm),
        2 => (2, raw_pcm),
        ch => {
            // Downmix multi-channel audio (> 2 channels) to stereo
            let num_frames = raw_pcm.len() / ch;
            let mut stereo = Vec::with_capacity(num_frames * 2);
            for i in 0..num_frames {
                let frame = &raw_pcm[i * ch..(i + 1) * ch];
                let left = frame[0];
                let right = frame[1];
                let mut center_sum = 0.0f32;
                for &c in &frame[2..] {
                    center_sum += c;
                }
                let center_contrib =
                    center_sum * core::f32::consts::FRAC_1_SQRT_2 / (ch - 2) as f32;
                stereo.push((left + center_contrib).clamp(-1.0, 1.0));
                stereo.push((right + center_contrib).clamp(-1.0, 1.0));
            }
            (2, stereo)
        }
    };

    // Resample to 48kHz using deterministic fixed-point arithmetic if necessary
    let resampled_pcm = if sample_rate == TARGET_SAMPLE_RATE {
        pcm
    } else {
        resample_fixed_point(&pcm, sample_rate, TARGET_SAMPLE_RATE, target_channels)
    };

    // Encode to 128kbps Opus in an Ogg container
    encode_opus_ogg(&resampled_pcm, target_channels)
}

/// Deterministic 32.32 fixed-point linear interpolation resampler.
fn resample_fixed_point(input: &[f32], src_rate: u32, dst_rate: u32, channels: usize) -> Vec<f32> {
    let in_frames = input.len() / channels;
    if in_frames == 0 {
        return Vec::new();
    }

    let out_frames =
        ((in_frames as u64 * dst_rate as u64 + (src_rate as u64 / 2)) / src_rate as u64) as usize;
    let mut output = Vec::with_capacity(out_frames * channels);

    let step = ((src_rate as u128) << 32) / dst_rate as u128;

    for i in 0..out_frames {
        let pos_fp = i as u128 * step;
        let idx = (pos_fp >> 32) as usize;
        let frac = (pos_fp & 0xffff_ffff) as u32;

        if idx + 1 < in_frames {
            let frac_f = frac as f32 / 4294967296.0f32;
            let inv_frac = 1.0f32 - frac_f;
            for c in 0..channels {
                let s0 = input[idx * channels + c];
                let s1 = input[(idx + 1) * channels + c];
                output.push(s0 * inv_frac + s1 * frac_f);
            }
        } else if idx < in_frames {
            for c in 0..channels {
                output.push(input[idx * channels + c]);
            }
        } else {
            output.extend(std::iter::repeat_n(0.0f32, channels));
        }
    }
    output
}

/// Encodes 48kHz interleaved PCM samples into a standard 128kbps Opus Ogg stream.
fn encode_opus_ogg(pcm: &[f32], channels: usize) -> Result<Vec<u8>> {
    let mut encoder = OpusEncoder::new(TARGET_SAMPLE_RATE as i32, channels, Application::Audio)
        .map_err(|e| CannerError::AudioEncode(e.to_string()))?;
    encoder.bitrate_bps = TARGET_BITRATE_BPS;

    let head = OpusHead::for_encoder(&encoder, TARGET_SAMPLE_RATE);
    let mut writer = OggOpusWriter::new(Vec::new(), head)
        .map_err(|e| CannerError::AudioEncode(e.to_string()))?;

    let frame_samples = OPUS_FRAME_SIZE * channels;
    let mut packet = vec![0u8; MAX_PACKET_BYTES];

    if pcm.is_empty() {
        let silence = vec![0.0f32; frame_samples];
        let n = encoder
            .encode(&silence, OPUS_FRAME_SIZE, &mut packet)
            .map_err(|e| CannerError::AudioEncode(e.to_string()))?;
        writer
            .write_packet_with_duration(&packet[..n], 0)
            .map_err(|e| CannerError::AudioEncode(e.to_string()))?;
    } else {
        let mut offset = 0;
        while offset < pcm.len() {
            let remaining = pcm.len() - offset;
            if remaining >= frame_samples {
                let chunk = &pcm[offset..offset + frame_samples];
                let n = encoder
                    .encode(chunk, OPUS_FRAME_SIZE, &mut packet)
                    .map_err(|e| CannerError::AudioEncode(e.to_string()))?;
                writer
                    .write_packet(&packet[..n])
                    .map_err(|e| CannerError::AudioEncode(e.to_string()))?;
                offset += frame_samples;
            } else {
                let remaining_frames = remaining / channels;
                let mut padded = vec![0.0f32; frame_samples];
                padded[..remaining].copy_from_slice(&pcm[offset..]);
                let n = encoder
                    .encode(&padded, OPUS_FRAME_SIZE, &mut packet)
                    .map_err(|e| CannerError::AudioEncode(e.to_string()))?;
                writer
                    .write_packet_with_duration(&packet[..n], remaining_frames as u32)
                    .map_err(|e| CannerError::AudioEncode(e.to_string()))?;
                offset += remaining;
            }
        }
    }

    let ogg_bytes = writer
        .finish()
        .map_err(|e| CannerError::AudioEncode(e.to_string()))?;
    Ok(ogg_bytes)
}
