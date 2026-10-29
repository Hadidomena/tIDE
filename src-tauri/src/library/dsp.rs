use std::path::Path;

use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::conv::IntoSample;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::Sample;

use serde::{Deserialize, Serialize};

use hound::{SampleFormat, WavSpec, WavWriter};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    Reverse,
    Trim { start_ms: u32, end_ms: u32 },
    Normalize { mode: NormalizeMode },
    Gain { db: f32 },
    Fade { in_ms: u32, out_ms: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NormalizeMode {
    Peak,
    Rms,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Decoded {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (self.frames() as f64 * 1000.0 / self.sample_rate as f64) as u64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderOut {
    pub path: String,
    pub hash: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub file_size: u64,
}

const RMS_TARGET: f32 = 0.891_25;

pub fn decode(path: &Path) -> Result<Decoded> {
    let file = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    decode_stream(Box::new(file), hint)
}

fn decode_stream(reader: Box<dyn MediaSource>, hint: Hint) -> Result<Decoded> {
    let mss = MediaSourceStream::new(reader, Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("unsupported or undetectable audio format: {e}"))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| "no audio track found".to_string())?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("no decoder for this codec: {e}"))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_rate = 0u32;
    let mut channels = 0u16;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(format!("read error: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(AudioBufferRef::F32(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::F64(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::S8(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::S16(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::S24(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::S32(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::U8(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::U16(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::U24(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Ok(AudioBufferRef::U32(buf)) => {
                append_interleaved(buf.as_ref(), &mut samples, &mut sample_rate, &mut channels)
            }
            Err(SymphoniaError::DecodeError(_)) => continue, // lost/corrupt frame — skip
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(format!("decode error: {e}")),
        }
    }

    if samples.is_empty() {
        return Err("no audio frames decoded".to_string());
    }
    Ok(Decoded {
        samples,
        sample_rate,
        channels,
    })
}

fn append_interleaved<S>(
    buf: &AudioBuffer<S>,
    out: &mut Vec<f32>,
    sample_rate: &mut u32,
    channels: &mut u16,
) where
    S: Sample + IntoSample<f32>,
{
    let spec = buf.spec();
    *sample_rate = spec.rate;
    *channels = spec.channels.count() as u16;
    let frames = buf.frames();
    let n_channels = spec.channels.count();
    out.reserve(frames * n_channels);
    for frame in 0..frames {
        for ch in 0..n_channels {
            let value: f32 = buf.chan(ch)[frame].into_sample();
            out.push(value);
        }
    }
}

pub fn apply(decoded: &mut Decoded, op: Op) -> Result<()> {
    match op {
        Op::Reverse => reverse(decoded),
        Op::Trim { start_ms, end_ms } => trim(decoded, start_ms, end_ms)?,
        Op::Normalize { mode } => normalize(decoded, mode),
        Op::Gain { db } => gain(decoded, db),
        Op::Fade { in_ms, out_ms } => fade(decoded, in_ms, out_ms),
    }
    Ok(())
}

fn reverse(decoded: &mut Decoded) {
    let ch = decoded.channels.max(1) as usize;
    let frames = decoded.samples.len() / ch;
    let mut out = Vec::with_capacity(decoded.samples.len());
    for frame in (0..frames).rev() {
        let start = frame * ch;
        out.extend_from_slice(&decoded.samples[start..start + ch]);
    }
    decoded.samples = out;
}

fn trim(decoded: &mut Decoded, start_ms: u32, end_ms: u32) -> Result<()> {
    let ch = decoded.channels.max(1) as usize;
    let rate = decoded.sample_rate.max(1) as u64;
    let total_frames = decoded.samples.len() / ch;
    let start = ((start_ms as u64 * rate) / 1000) as usize;
    let end = ((end_ms as u64 * rate) / 1000) as usize;
    if end <= start || start >= total_frames {
        return Err("trim window is empty or beyond the sample".to_string());
    }
    let end = end.min(total_frames);
    let start_idx = start * ch;
    let end_idx = end * ch;
    decoded.samples = decoded.samples[start_idx..end_idx].to_vec();
    Ok(())
}

fn normalize(decoded: &mut Decoded, mode: NormalizeMode) {
    if decoded.samples.is_empty() {
        return;
    }
    let scale = match mode {
        NormalizeMode::Peak => {
            let peak = decoded
                .samples
                .iter()
                .fold(0f32, |acc, &s| acc.max(s.abs()));
            if peak <= f32::EPSILON {
                return; // silence — no-op
            }
            1.0 / peak
        }
        NormalizeMode::Rms => {
            let rms = (decoded.samples.iter().map(|s| s * s).sum::<f32>()
                / decoded.samples.len() as f32)
                .sqrt();
            if rms <= f32::EPSILON {
                return;
            }
            RMS_TARGET / rms
        }
    };
    for s in &mut decoded.samples {
        *s *= scale;
    }
}

fn gain(decoded: &mut Decoded, db: f32) {
    let k = 10f32.powf(db / 20.0);
    for s in &mut decoded.samples {
        *s *= k;
    }
}

fn fade(decoded: &mut Decoded, in_ms: u32, out_ms: u32) {
    let ch = decoded.channels.max(1) as usize;
    let rate = decoded.sample_rate.max(1) as u64;
    let frames = decoded.samples.len() / ch;
    let in_frames = (((in_ms as u64 * rate) / 1000) as usize).min(frames);
    let out_frames = (((out_ms as u64 * rate) / 1000) as usize).min(frames);

    for frame in 0..frames {
        let mut g = 1.0f32;
        if frame < in_frames {
            g *= frame as f32 / in_frames as f32;
        }
        if frame >= frames.saturating_sub(out_frames) {
            g *= (frames - 1 - frame) as f32 / out_frames as f32;
        }
        for c in 0..ch {
            decoded.samples[frame * ch + c] *= g;
        }
    }
}

pub fn encode_wav(path: &Path, decoded: &Decoded, bits_per_sample: u16) -> Result<u64> {
    let spec = WavSpec {
        channels: decoded.channels.max(1),
        sample_rate: decoded.sample_rate,
        bits_per_sample,
        sample_format: SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(path, spec).map_err(|e| format!("create {}: {e}", path.display()))?;
    match bits_per_sample {
        16 => {
            for &s in &decoded.samples {
                writer
                    .write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                    .map_err(|e| e.to_string())?;
            }
        }
        24 => {
            for &s in &decoded.samples {
                writer
                    .write_sample((s.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
                    .map_err(|e| e.to_string())?;
            }
        }
        _ => {
            return Err(format!(
                "unsupported bits_per_sample {bits_per_sample} (use 16 or 24)"
            ));
        }
    }
    writer
        .finalize()
        .map_err(|e| format!("finalize {}: {e}", path.display()))?;
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(size)
}

pub fn hash(decoded: &Decoded) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&decoded.sample_rate.to_le_bytes());
    hasher.update(&decoded.channels.to_le_bytes());
    for s in &decoded.samples {
        hasher.update(&s.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

pub fn render(source: &Path, out: &Path, op: Op, bits_per_sample: u16) -> Result<RenderOut> {
    let mut decoded = decode(source)?;
    apply(&mut decoded, op)?;
    let file_size = encode_wav(out, &decoded, bits_per_sample)?;
    Ok(RenderOut {
        path: out.display().to_string(),
        hash: hash(&decoded),
        duration_ms: decoded.duration_ms(),
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        file_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mono(samples: Vec<f32>, rate: u32) -> Decoded {
        Decoded {
            samples,
            sample_rate: rate,
            channels: 1,
        }
    }

    #[test]
    fn reverse_flips_frame_order() {
        let mut d = mono(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0], 44100);
        apply(&mut d, Op::Reverse).unwrap();
        assert_eq!(d.samples, vec![5.0, 4.0, 3.0, 2.0, 1.0, 0.0]);
    }

    #[test]
    fn reverse_keeps_stereo_frames_together() {
        let mut d = Decoded {
            samples: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            sample_rate: 44100,
            channels: 2,
        };
        apply(&mut d, Op::Reverse).unwrap();
        assert_eq!(d.samples, vec![4.0, 5.0, 2.0, 3.0, 0.0, 1.0]);
    }

    #[test]
    fn trim_slices_frames_by_milliseconds() {
        let mut d = mono((0..10).map(|i| i as f32).collect(), 1000);
        apply(
            &mut d,
            Op::Trim {
                start_ms: 2,
                end_ms: 5,
            },
        )
        .unwrap();
        assert_eq!(d.samples, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn trim_rejects_empty_or_past_end_window() {
        let mut d = mono(vec![0.0; 10], 1000);
        assert!(
            apply(
                &mut d,
                Op::Trim {
                    start_ms: 5,
                    end_ms: 5
                }
            )
            .is_err()
        );
        assert!(
            apply(
                &mut d,
                Op::Trim {
                    start_ms: 10,
                    end_ms: 12
                }
            )
            .is_err()
        );
        assert!(
            apply(
                &mut d,
                Op::Trim {
                    start_ms: 9,
                    end_ms: 10
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn normalize_peak_reaches_unity() {
        let mut d = mono(vec![0.5, -0.25, 0.125], 44100);
        apply(
            &mut d,
            Op::Normalize {
                mode: NormalizeMode::Peak,
            },
        )
        .unwrap();
        assert_eq!(d.samples, vec![1.0, -0.5, 0.25]);
    }

    #[test]
    fn normalize_rms_hits_target() {
        let mut d = mono(vec![0.5, -0.5, 0.5, -0.5], 44100);
        apply(
            &mut d,
            Op::Normalize {
                mode: NormalizeMode::Rms,
            },
        )
        .unwrap();
        let rms = (d.samples.iter().map(|s| s * s).sum::<f32>() / 4.0).sqrt();
        assert!((rms - RMS_TARGET).abs() < 1e-4, "rms {rms}");
    }

    #[test]
    fn normalize_silence_is_a_noop() {
        let mut d = mono(vec![0.0, 0.0, 0.0], 44100);
        apply(
            &mut d,
            Op::Normalize {
                mode: NormalizeMode::Peak,
            },
        )
        .unwrap();
        assert_eq!(d.samples, vec![0.0, 0.0, 0.0]);
        apply(
            &mut d,
            Op::Normalize {
                mode: NormalizeMode::Rms,
            },
        )
        .unwrap();
        assert_eq!(d.samples, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn gain_applies_db() {
        let mut d = mono(vec![0.1, -0.1], 44100);
        apply(&mut d, Op::Gain { db: 6.0 }).unwrap();
        let k = 10f32.powf(0.3);
        assert!((d.samples[0] - 0.1 * k).abs() < 1e-6);
        assert!((d.samples[1] + 0.1 * k).abs() < 1e-6);
    }

    #[test]
    fn fade_ramps_edges_and_keeps_middle() {
        let mut d = mono(vec![1.0; 10], 1000);
        apply(
            &mut d,
            Op::Fade {
                in_ms: 2,
                out_ms: 2,
            },
        )
        .unwrap();
        let expected = [0.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.0];
        for (got, want) in d.samples.iter().zip(expected) {
            assert!((got - want).abs() < 1e-6, "{got} != {want}");
        }
    }

    #[test]
    fn hash_is_content_deterministic() {
        let a = mono(vec![0.0, 0.5, -0.5], 44100);
        let b = mono(vec![0.0, 0.5, -0.5], 44100);
        let c = mono(vec![0.0, 0.5, -0.6], 44100);
        assert_eq!(hash(&a), hash(&b));
        assert_ne!(hash(&a), hash(&c));
    }

    #[test]
    fn wav_round_trip_preserves_rate_and_channels() {
        let original = Decoded {
            samples: vec![0.0, -0.5, 0.5, 1.0, 0.25, -1.0],
            sample_rate: 48000,
            channels: 2,
        };

        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let spec = WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            };
            let mut w = WavWriter::new(&mut cursor, spec).unwrap();
            for &s in &original.samples {
                w.write_sample((s * i16::MAX as f32) as i16).unwrap();
            }
            w.finalize().unwrap();
        }

        let mut hint = Hint::new();
        hint.with_extension("wav");
        let decoded = decode_stream(Box::new(std::io::Cursor::new(cursor.into_inner())), hint)
            .expect("decode in-memory wav");
        assert_eq!(decoded.sample_rate, 48000);
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.frames(), 3);
        for (got, want) in decoded.samples.iter().zip(original.samples.iter()) {
            assert!((got - want).abs() < 2.0 / 32768.0, "{got} != {want}");
        }
    }

    #[test]
    fn render_writes_wav_and_returns_meta() {
        let out = std::env::temp_dir().join(format!("tide-dsp-render-{}.wav", std::process::id()));
        let source =
            std::env::temp_dir().join(format!("tide-dsp-source-{}.wav", std::process::id()));
        let original = mono((0..441).map(|i| i as f32 / 441.0).collect(), 44100);
        encode_wav(&source, &original, 16).unwrap();

        let meta = render(&source, &out, Op::Reverse, 16).unwrap();
        assert_eq!(meta.sample_rate, 44100);
        assert_eq!(meta.channels, 1);
        assert!(meta.file_size > 0);
        assert_eq!(meta.duration_ms, 10); // 441 frames @ 44.1 kHz
        assert!(!meta.hash.is_empty());
        assert_eq!(meta.path, out.display().to_string());

        let source_decoded = decode(&source).unwrap();
        let decoded = decode(&out).unwrap();
        assert!((decoded.samples[0] - source_decoded.samples[440]).abs() < 2.0 / 32768.0);

        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&source);
    }
}
